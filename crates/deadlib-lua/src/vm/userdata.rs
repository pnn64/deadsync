//! Full userdata storage and the Lua 5.1 proxy constructor.
use super::*;
use std::mem::align_of;

// C userdata must support the target's pointer/double alignment. Select an
// aligned subslice of an ordinary byte allocation, without casting Rust slices.
const ALIGNMENT: usize = if align_of::<usize>() > align_of::<f64>() {
    align_of::<usize>()
} else {
    align_of::<f64>()
};

#[derive(Debug)]
pub(super) struct Bytes(Box<[u8]>);
impl Bytes {
    fn new(size: usize) -> Result<Self, Error> {
        let failed = || Error::new(ErrorKind::Runtime, 0, "not enough memory");
        // Keep even a zero-sized payload address inside its own allocation,
        // so it cannot coincide with the next allocation's first address.
        let total = size.checked_add(ALIGNMENT).ok_or_else(failed)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(total).map_err(|_| failed())?;
        bytes.resize(total, 0);
        Ok(Self(bytes.into_boxed_slice()))
    }
    fn offset(&self) -> usize {
        (self.0.as_ptr() as usize).wrapping_neg() & (ALIGNMENT - 1)
    }
    fn payload_len(&self) -> usize {
        self.0.len() - ALIGNMENT
    }
    pub(super) fn len(&self) -> usize {
        self.payload_len()
    }
    pub(super) fn storage_bytes(&self) -> usize {
        self.0.len()
    }
}
impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        let start = self.offset();
        &self.0[start..start + self.payload_len()]
    }
}
impl AsMut<[u8]> for Bytes {
    fn as_mut(&mut self) -> &mut [u8] {
        let start = self.offset();
        let len = self.payload_len();
        &mut self.0[start..start + len]
    }
}

#[derive(Debug)]
pub(super) struct Userdata {
    pub library: Option<Arc<dyn package::NativeLibrary>>,
    pub file: Option<std::num::NonZeroU64>,
    pub env: TableId,
    pub metatable: Option<TableId>,
    pub bytes: Bytes,
    pub finalized: bool,
    pub order: u64,
}

impl Vm {
    pub(super) fn separate_userdata(&mut self, all: bool) {
        let mut candidates = std::mem::take(&mut self.gc.candidates);
        candidates.clear();
        for i in 0..self.userdata.len() {
            let Some(key) = self.userdata.key(i) else {
                continue;
            };
            if !self.userdata[i].finalized && (all || !self.userdata.marked(key)) {
                let finalizer = self.metafield(Value::Userdata(UserdataId(key)), Meta::Gc);
                self.userdata[i].finalized = true;
                if finalizer != Value::Nil {
                    candidates.push((self.userdata[i].order, UserdataId(key)));
                }
            }
        }
        candidates.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
        self.gc
            .finalizers
            .extend(candidates.drain(..).map(|(_, id)| id));
        self.gc.candidates = candidates;
    }

    pub(super) fn push_finalizers(&mut self, host: bool) {
        let base = self.registers.len();
        let top = self.top;
        self.resize_registers(base + 3);
        self.push_record(Frame {
            function: FrameKind::Native(CONT_GC),
            pc: 0,
            base,
            stack_start: base,
            result: top,
            wanted: if host { FRAME_OPEN } else { 0 },
            barriers: self.frames.last().map_or(1, |f| f.barriers + 1),
            vararg_start: 0,
            vararg_count: 0,
            tail_calls: 0,
            c_level_bias: 0,
        });
        self.gc.running += 1;
        self.hooks.finalizers += 1;
    }

    pub(super) fn discard_finalizers(&mut self, first: usize) {
        let count = self.frames[first..]
            .iter()
            .filter(|f| matches!(f.function, FrameKind::Native(CONT_GC)))
            .count();
        self.gc.running -= count;
        self.hooks.finalizers -= count as u16;
    }

    pub(super) fn finalizer_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        if frame.pc != 0 {
            // Native Lua restores controls only after a successful tag method.
            // Error unwinding skips this continuation and retains its changes.
            self.leave_gc_finalizer(owner, frame);
        }
        // A tag method can start another cycle with an explicit step. Finish
        // its sweep before dispatching the newly separated tag methods.
        self.prepare_gc_finalizers();
        while let Some(id) = self.gc.finalizers.pop_front() {
            let value = Value::Userdata(id);
            let finalizer = self.metafield(value, Meta::Gc);
            if finalizer == Value::Nil {
                continue;
            }
            self.registers[base] = finalizer;
            self.registers[base + 1] = value;
            self.enter_gc_finalizer(owner);
            self.gc.tag_calls = self.gc.tag_calls.wrapping_add(1);
            if let Some(actual) = self.start_call(base, 1, 0, false)? {
                self.finish_call(base, actual, 0);
            }
            return Ok(None);
        }
        self.finish_gc_cycle();
        self.frames.pop();
        self.registers.truncate(frame.stack_start);
        self.top = frame.result;
        self.gc.running -= 1;
        self.hooks.finalizers -= 1;
        Ok((frame.wanted == FRAME_OPEN).then_some(0))
    }

    pub(super) fn collect_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let index = self.frames.len() - 1;
        if frame.pc == 0 {
            self.resize_registers(self.registers.len().max(frame.base + frame.argc() + 1));
            self.registers.copy_within(
                frame.vararg_start..frame.vararg_start + frame.argc(),
                frame.base + 1,
            );
            let dispatch = self.gc_native(COLLECT_GARBAGE, frame.base, frame.argc())?;
            self.frames[index].pc = 1;
            if dispatch && !self.gc.finalizers.is_empty() {
                self.push_finalizers(false);
            }
            return Ok(None);
        }
        // The C builtin pushes its result after its arguments before the call
        // moves it to the destination. A later Lua hook can see that spare
        // slot, including the boolean returned by collectgarbage('step').
        let pushed = frame.vararg_start + frame.argc();
        if pushed < frame.stack_start
            && self
                .frames
                .get(index.wrapping_sub(1))
                .is_some_and(|caller| matches!(caller.function, FrameKind::Lua(_)))
        {
            self.registers[pushed] = self.registers[frame.base];
        }
        Ok(self.return_frame(frame, frame.base, 1))
    }

    fn run_host_finalizers(&mut self) -> Result<(), Error> {
        self.reset_budget();
        let len = self.registers.len();
        let top = self.top;
        if self.standalone != 0 && self.frames.is_empty() {
            self.standalone_main(self.standalone, 0);
        }
        self.push_finalizers(true);
        let result = self.drive();
        self.close_upvalues(len);
        self.discard_finalizers(0);
        self.frames.clear();
        self.restore_calls();
        self.trim_substitutions();
        self.sorts.clear();
        self.trim_loads();
        self.trim_tracebacks();
        self.trim_hooks();
        self.registers.truncate(len);
        self.top = top;
        result.map(|_| ())
    }

    /// Complete a tracing cycle and run pending Lua userdata finalizers.
    /// Host values that must survive need a root. Full collection is synchronous;
    /// automatic and explicit-step sweeping can run in slices.
    ///
    /// # Errors
    /// Returns a finalizer's Lua error or an instruction-limit error. Remaining
    /// finalizers stay pending for a later collection or VM destruction.
    pub fn collect_garbage(&mut self) -> Result<HeapStats, Error> {
        self.collect_cycle();
        self.finish_gc_cycle();
        if !self.gc.finalizers.is_empty() {
            self.run_host_finalizers()?;
        }
        Ok(self.heap_stats())
    }

    fn finalize_on_close(&mut self) {
        self.gc.closing = true;
        self.budget = self.budget.map(|n| n.max(1));
        self.separate_userdata(true);
        while !self.gc.finalizers.is_empty() {
            let _ = self.run_host_finalizers();
        }
    }

    pub(super) fn allocate_userdata(&mut self, size: usize, env: TableId) -> UserdataId {
        self.try_allocate_userdata(size, env)
            .expect("allocate userdata")
    }

    pub(super) fn try_allocate_userdata(
        &mut self,
        size: usize,
        env: TableId,
    ) -> Result<UserdataId, Error> {
        let bytes = Bytes::new(size)?;
        self.userdata_order = self
            .userdata_order
            .checked_add(1)
            .expect("userdata creation order exhausted");
        self.userdata.note_alloc(bytes.storage_bytes());
        let id = UserdataId(self.userdata.insert(Userdata {
            library: None,
            file: None,
            env,
            metatable: None,
            bytes,
            finalized: false,
            order: self.userdata_order,
        }));
        self.gc_new_value(Value::Userdata(id));
        Ok(id)
    }

    /// Allocate zero-initialized userdata in this thread's environment.
    /// Pin the returned value if the host retains it across executions/collections.
    pub fn new_userdata(&mut self, size: usize) -> Value {
        Value::Userdata(self.allocate_userdata(size, self.globals))
    }

    /// Borrow the byte storage of live userdata, or `None` for a collected handle.
    pub fn userdata_bytes(&self, id: UserdataId) -> Option<&[u8]> {
        self.userdata.get(id.0).map(|u| u.bytes.as_ref())
    }

    /// Mutably borrow live userdata storage without changing its size or identity.
    pub fn userdata_bytes_mut(&mut self, id: UserdataId) -> Option<&mut [u8]> {
        self.userdata.get_mut(id.0).map(|u| u.bytes.as_mut())
    }

    pub(super) fn proxy_native(
        &mut self,
        closure: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        let arg = self.argument(base, count, 0);
        let env = self.native_environment(closure);
        let id = self.allocate_userdata(0, env);
        let metatable = if !arg.is_truthy() {
            None
        } else if arg == Value::Boolean(true) {
            let table = self.new_table();
            let valid = self.native_closures[slot_index(closure)].captures[0];
            let Value::Table(valid) = valid else {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "invalid newproxy capture",
                ));
            };
            self.raw_set(valid, Value::Table(table), Value::Boolean(true))?;
            Some(table)
        } else {
            Some(
                self.metatable(arg)
                    .filter(
                        |m| match self.native_closures[slot_index(closure)].captures[0] {
                            Value::Table(valid) => self.tables[slot_index(valid.0)]
                                .get(Value::Table(*m))
                                .is_truthy(),
                            _ => false,
                        },
                    )
                    .ok_or_else(|| self.argument_error(0, b"boolean or proxy expected"))?,
            )
        };
        self.set_metatable(Value::Userdata(id), metatable);
        self.registers[base] = Value::Userdata(id);
        Ok(1)
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        if !self.gc.closing {
            self.finalize_on_close();
        }
        let _ = io::stdout_flush();
    }
}
