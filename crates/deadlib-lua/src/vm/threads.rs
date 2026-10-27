//! Shared-heap Lua threads with vector context switches and an explicit scheduler.
use super::*;

pub(super) const CO_CREATE: u64 = 80;
pub(super) const CO_RESUME: u64 = 81;
pub(super) const CO_RUNNING: u64 = 82;
pub(super) const CO_STATUS: u64 = 83;
pub(super) const CO_WRAP: u64 = 84;
pub(super) const CO_YIELD: u64 = 85;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Status {
    Running,
    Suspended,
    Normal,
    Dead,
}
/// Lua 5.1 execution status retained by a thread, separate from coroutine.status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NativeThreadStatus {
    /// Fresh, running, normal or successfully completed execution.
    Ok = 0,
    /// Execution is suspended at a yield.
    Yielded = 1,
    /// The coroutine failed with an unprotected runtime error.
    RuntimeError = 2,
}
impl Status {
    fn name(self) -> &'static [u8] {
        match self {
            Self::Running => b"running",
            Self::Suspended => b"suspended",
            Self::Normal => b"normal",
            Self::Dead => b"dead",
        }
    }
}

#[derive(Debug)]
pub(super) enum NativeKind {
    Builtin(u64),
    Match,
    Wrap,
    Lines,
    Host(host::Function),
    StandaloneMain,
    StandaloneTraceback,
}
#[derive(Debug)]
pub(super) struct NativeClosure {
    pub env: TableId,
    pub kind: NativeKind,
    pub captures: Box<[Value]>,
}

#[derive(Debug)]
pub(super) enum Transition {
    Resume(ThreadId),
    Yield { source: usize, count: usize },
}

#[derive(Debug)]
pub(super) struct Thread {
    pub(super) registers: Vec<Value>,
    pub(super) frames: Vec<Frame>,
    pub(super) call_slots: u16,
    pub(super) open_upvalues: Vec<u32>,
    pub(super) globals: TableId,
    pub(super) top: usize,
    pub(super) active_native: Option<usize>,
    pub(super) substitutions: Vec<strings::Substitution>,
    pub(super) sorts: Vec<tables::Sort>,
    pub(super) loads: Vec<loads::Loading>,
    pub(super) tracebacks: Vec<traces::Traceback>,
    pub(super) hooks: hooks::Hooks,
    pub(super) entry: Option<(usize, usize)>,
    pub(super) api_stack: std::ops::Range<usize>,
    pub(super) status: Status,
    pub(super) resume_status: NativeThreadStatus,
    pub(super) resumer: Option<ThreadId>,
    pub(super) depth: u16,
    pub(super) line: u32,
    pub(super) host_states: Vec<host::ThreadData>,
}
impl Thread {
    pub fn main(globals: TableId) -> Self {
        Self {
            registers: Vec::new(),
            frames: Vec::new(),
            call_slots: 8,
            open_upvalues: Vec::new(),
            globals,
            top: 0,
            active_native: None,
            substitutions: Vec::new(),
            sorts: Vec::new(),
            loads: Vec::new(),
            tracebacks: Vec::new(),
            hooks: hooks::Hooks::default(),
            entry: None,
            api_stack: 0..0,
            status: Status::Running,
            resume_status: NativeThreadStatus::Ok,
            resumer: None,
            depth: 0,
            line: 0,
            host_states: Vec::new(),
        }
    }
    fn new(entry: FunctionId, globals: TableId) -> Self {
        Self {
            registers: vec![Value::Function(entry)],
            top: 1,
            api_stack: 0..1,
            globals,
            status: Status::Suspended,
            ..Self::main(globals)
        }
    }
}

impl Vm {
    pub(super) fn allocate_thread(&mut self) -> ThreadId {
        let id = ThreadId(self.threads.insert(Thread {
            status: Status::Suspended,
            hooks: self.hooks.inherited(),
            ..Thread::main(self.globals)
        }));
        self.gc_new_value(Value::Thread(id));
        id
    }
    pub(super) fn coroutine_status(&self, id: ThreadId) -> Status {
        if id == self.thread {
            return Status::Running;
        }
        let state = &self.threads[slot_index(id.0)];
        if state.resume_status == NativeThreadStatus::Ok
            && state.frames.is_empty()
            && state.status != Status::Normal
        {
            if state.api_stack.is_empty() {
                Status::Dead
            } else {
                Status::Suspended
            }
        } else {
            state.status
        }
    }
    pub(super) fn open_threads(&mut self) {
        let table = self.new_table();
        for (i, name) in [
            b"create".as_slice(),
            b"resume",
            b"running",
            b"status",
            b"wrap",
            b"yield",
        ]
        .iter()
        .enumerate()
        {
            let name = self.intern(name);
            self.tables[slot_index(table.0)]
                .set(name, Value::Native(CO_CREATE + i as u64))
                .expect("coroutine names are valid");
        }
        self.set_global(b"coroutine", Value::Table(table))
            .expect("coroutine name is valid");
    }
    pub(super) fn new_native(
        &mut self,
        kind: NativeKind,
        captures: &[Value],
    ) -> Result<Value, Error> {
        let env = self
            .frames
            .last()
            .map_or(self.globals, |frame| match frame.function {
                FrameKind::Lua(id) => self.functions[slot_index(id.0)].env,
                FrameKind::Native(id) => self.native_environment(id),
            });
        self.native_closures
            .note_alloc(std::mem::size_of_val(captures));
        let id = self.native_closures.insert(NativeClosure {
            env,
            kind,
            captures: captures.into(),
        });
        self.gc_new_value(Value::Native(id));
        Ok(Value::Native(id))
    }
    pub(super) fn scheduled_native(&self, id: u64) -> u8 {
        if id < strings::ITERATOR_FIRST {
            return 0;
        }
        match self.native_closures.get(id).map(|c| &c.kind) {
            Some(NativeKind::Wrap) => 1,
            Some(NativeKind::StandaloneMain) => 2,
            Some(NativeKind::StandaloneTraceback) => 3,
            _ => 0,
        }
    }
    pub(super) fn native_environment(&self, id: u64) -> TableId {
        if id >= strings::ITERATOR_FIRST {
            self.native_closures[slot_index(id)].env
        } else {
            self.native_envs
                .get(&id)
                .copied()
                .unwrap_or(self.initial_globals)
        }
    }
    pub(super) fn set_native_environment(&mut self, id: u64, env: TableId) {
        if id >= strings::ITERATOR_FIRST {
            self.native_closures[slot_index(id)].env = env;
        } else {
            self.native_envs.insert(id, env);
        }
        self.gc_barrier(Value::Native(id), Value::Table(env));
    }
    pub(super) fn thread_environment(&self, id: ThreadId) -> TableId {
        if id == self.thread {
            self.globals
        } else {
            self.threads[slot_index(id.0)].globals
        }
    }
    pub(super) fn set_thread_environment(&mut self, id: ThreadId, env: TableId) {
        if id == self.thread {
            self.globals = env;
        } else {
            self.threads[slot_index(id.0)].globals = env;
        }
        self.gc_barrier(Value::Thread(id), Value::Table(env));
    }
    pub(super) fn cell_value(&self, id: usize) -> Value {
        match self.upvalues[id] {
            Cell::Closed(value) => value,
            Cell::Open(thread, register) => {
                if thread == self.thread {
                    self.registers[register]
                } else {
                    self.threads[slot_index(thread.0)].registers[register]
                }
            }
        }
    }
    pub(super) fn set_cell(&mut self, id: usize, value: Value) {
        match self.upvalues[id] {
            Cell::Closed(_) => self.upvalues[id] = Cell::Closed(value),
            Cell::Open(thread, register) => {
                if thread == self.thread {
                    self.registers[register] = value;
                } else {
                    self.threads[slot_index(thread.0)].registers[register] = value;
                }
            }
        }
        self.gc_cell_barrier(id, value);
    }
    // Active vectors stay directly in Vm for ordinary dispatch. Switching swaps
    // ownership with the inactive thread slots; it never copies stack contents.
    fn exchange_thread(&mut self, id: ThreadId, line: &mut u32) {
        let state = &mut self.threads[slot_index(id.0)];
        std::mem::swap(&mut self.registers, &mut state.registers);
        std::mem::swap(&mut self.frames, &mut state.frames);
        std::mem::swap(&mut self.call_slots, &mut state.call_slots);
        std::mem::swap(&mut self.open_upvalues, &mut state.open_upvalues);
        std::mem::swap(&mut self.globals, &mut state.globals);
        std::mem::swap(&mut self.top, &mut state.top);
        std::mem::swap(&mut self.active_native, &mut state.active_native);
        std::mem::swap(&mut self.substitutions, &mut state.substitutions);
        std::mem::swap(&mut self.sorts, &mut state.sorts);
        std::mem::swap(&mut self.loads, &mut state.loads);
        std::mem::swap(&mut self.tracebacks, &mut state.tracebacks);
        std::mem::swap(&mut self.hooks, &mut state.hooks);
        std::mem::swap(line, &mut state.line);
    }
    fn switch_thread(&mut self, id: ThreadId, line: &mut u32) {
        self.exchange_thread(self.thread, line);
        self.exchange_thread(id, line);
        self.thread = id;
    }
    pub(super) fn apply_transition(&mut self, line: &mut u32) -> Result<Option<usize>, Error> {
        match self.transition.take() {
            None => {}
            Some(Transition::Resume(id)) => {
                self.switch_thread(id, line);
                if let Some((call, count)) = self.threads[slot_index(id.0)].entry.take()
                    && let Some(actual) = self.start_lua_call(call, count, OPEN, false)?
                {
                    self.top = call + actual;
                    return Ok(Some(self.top));
                }
            }
            Some(Transition::Yield { source, count }) => {
                self.finish_thread(source, count, false, true, line)
            }
        }
        Ok(None)
    }
    pub(super) fn finish_thread(
        &mut self,
        source: usize,
        count: usize,
        failed: bool,
        suspended: bool,
        line: &mut u32,
    ) {
        let id = self.thread;
        let parent = self.threads[slot_index(id.0)]
            .resumer
            .take()
            .expect("resumed coroutine has a parent");
        let caller = &mut self.threads[slot_index(parent.0)];
        let frame = caller
            .frames
            .last_mut()
            .expect("resumer has an awaiting frame");
        let prefix = usize::from(matches!(frame.function, FrameKind::Native(CO_RESUME)));
        let base = frame.base;
        let arguments = frame.argc();
        caller.registers.resize(
            caller
                .registers
                .len()
                .max(base + arguments + prefix + count),
            Value::Nil,
        );
        if prefix != 0 {
            caller.registers[base + arguments] = Value::Boolean(!failed);
        }
        caller.registers[base + arguments + prefix..base + arguments + prefix + count]
            .copy_from_slice(&self.registers[source..source + count]);
        caller.top = base + arguments + prefix + count;
        frame.vararg_start = base;
        frame.vararg_count = (arguments + prefix + count) as u32;
        // Low two bits select the continuation; higher bits retain the C
        // argument prefix below its results, including return-hook aliases.
        frame.pc = ((arguments as u32) << 2) | if failed { 3 } else { 2 };
        caller.status = Status::Running;
        self.threads[slot_index(id.0)].status = if suspended {
            Status::Suspended
        } else {
            Status::Dead
        };
        self.threads[slot_index(id.0)].resume_status = if failed {
            NativeThreadStatus::RuntimeError
        } else if suspended {
            NativeThreadStatus::Yielded
        } else {
            NativeThreadStatus::Ok
        };
        if suspended {
            self.threads[slot_index(id.0)].api_stack = source..source;
            self.top = source;
        } else if !failed {
            self.threads[slot_index(id.0)].api_stack = 0..0;
            self.top = 0;
        }
        if !suspended && !failed {
            self.registers.clear();
        }
        self.switch_thread(parent, line);
    }
    pub(super) fn fail_thread(&mut self, error: &Error, line: &mut u32) {
        self.discard_finalizers(0);
        let value = error.value().unwrap_or_else(|| {
            let value = self.intern(error.message().as_bytes());
            self.located_error(value, 1)
        });
        if let Some(frame) = self.frames.last().copied() {
            let (base, values) = match frame.function {
                FrameKind::Native(id) => {
                    let base = frame.vararg_start;
                    let count = if id == 1 {
                        frame.argc().min(1)
                    } else {
                        frame.argc()
                    };
                    let mut values = self.registers[base..base + count].to_vec();
                    if self.scheduled_native(id) != 1
                        && (id != 1 || values.last().copied() != Some(value))
                    {
                        values.push(value);
                    }
                    (base, values)
                }
                FrameKind::Lua(id) => {
                    let f = &self.functions[slot_index(id.0)];
                    let count =
                        self.programs[f.program].data.prototypes[f.prototype].debug_registers();
                    let mut values = self.registers[frame.base..frame.base + count].to_vec();
                    values.push(self.intern(error.message_bytes()));
                    values.push(value);
                    (frame.base, values)
                }
            };
            self.resize_registers(self.registers.len().max(base + values.len()));
            self.registers[base..base + values.len()].copy_from_slice(&values);
            self.top = base + values.len();
            self.threads[slot_index(self.thread.0)].api_stack = base..self.top;
            if matches!(frame.function, FrameKind::Native(_)) {
                let f = self.frames.last_mut().unwrap();
                f.base = base;
                f.stack_start = base;
                f.vararg_count = values.len() as u32;
            }
        }
        let source = self.registers.len();
        self.registers.push(value);
        // Keep faulting frames and open cells for debug inspection and escaped
        // closures. A dead thread cannot execute again or overwrite those cells.
        self.finish_thread(source, 1, true, false, line);
    }
    pub(super) fn abort_threads(&mut self, line: &mut u32) {
        self.abort_threads_to(self.main, line);
    }
    pub(super) fn abort_threads_to(&mut self, target: ThreadId, line: &mut u32) {
        self.transition = None;
        while self.thread != target {
            let id = self.thread;
            let parent = self.threads[slot_index(id.0)]
                .resumer
                .take()
                .expect("active thread has a resumer");
            self.close_upvalues(0);
            self.discard_finalizers(0);
            self.frames.clear();
            self.restore_calls();
            self.trim_substitutions();
            self.sorts.clear();
            self.trim_loads();
            self.trim_tracebacks();
            self.trim_hooks();
            self.registers.clear();
            self.top = 0;
            self.threads[slot_index(id.0)].api_stack = 0..0;
            self.threads[slot_index(id.0)].status = Status::Dead;
            self.threads[slot_index(id.0)].resume_status = NativeThreadStatus::RuntimeError;
            self.switch_thread(parent, line);
        }
        self.threads[slot_index(target.0)].status = Status::Running;
    }
    pub(super) fn coroutine_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        let result = match id {
            CO_CREATE | CO_WRAP => {
                let Value::Function(entry) = self.argument(base, count, 0) else {
                    return Err(self.argument_error(0, b"Lua function expected"));
                };
                let thread = ThreadId(self.threads.insert(Thread::new(entry, self.globals)));
                self.threads.note_alloc(size_of::<Value>());
                self.threads[slot_index(thread.0)].hooks = self.hooks.inherited();
                self.gc_new_value(Value::Thread(thread));
                if id == CO_WRAP {
                    self.new_native(NativeKind::Wrap, &[Value::Thread(thread)])?
                } else {
                    Value::Thread(thread)
                }
            }
            CO_RUNNING => {
                if self.thread == self.main {
                    Value::Nil
                } else {
                    Value::Thread(self.thread)
                }
            }
            CO_STATUS => {
                let Value::Thread(thread) = self.argument(base, count, 0) else {
                    return Err(self.argument_error(0, b"coroutine expected"));
                };
                let status = self.coroutine_status(thread);
                self.intern(status.name())
            }
            _ => unreachable!("coroutine continuations are scheduled"),
        };
        self.registers[base] = result;
        Ok(1)
    }
    pub(super) fn native_depth(&self, end: usize) -> usize {
        let iterators = self.frames[..end]
            .windows(2)
            .filter(|pair| {
                if pair[1].barriers <= pair[0].barriers {
                    return false;
                }
                let FrameKind::Lua(id) = pair[0].function else {
                    return false;
                };
                let closure = &self.functions[slot_index(id.0)];
                let prototype = &self.programs[closure.program].data.prototypes[closure.prototype];
                pair[0]
                    .pc
                    .checked_sub(1)
                    .and_then(|pc| prototype.code.get(pc as usize))
                    .is_some_and(|i| i.op == Op::CallIter)
            })
            .count();
        let boundaries = self.frames[..end]
            .iter()
            .filter(|f| {
                // The standalone outer main is the implicit root C call,
                // already accounted for by the depth-limit convention.
                matches!(
                    f.function,
                    FrameKind::Native(
                        PCALL
                            | debug::CONSOLE
                            | XPCALL
                            | TOSTRING
                            | PRINT
                            | DO_FILE
                            | package::REQUIRE
                            | package::MODULE
                            | strings::STR_GSUB
                            | tables::TAB_FOREACH
                            | tables::TAB_FOREACHI
                            | tables::TAB_SORT
                            | CONT_RAW
                            | CONT_BOOL
                            | CONT_NOT
                            | CONT_IGNORE
                            | CONT_HANDLER
                            | CONT_GC
                            | CONT_HOOK
                            | CONT_STANDALONE
                    )
                ) || (matches!(f.function, FrameKind::Native(id) if self.scheduled_native(id) == 3))
                    || (matches!(f.function, FrameKind::Native(id) if self.host_callback(id).is_some()))
                    || (loads::protected(f.function)
                        && !matches!(f.function, FrameKind::Native(CONT_HOST_LOAD))
                        && matches!(f.pc, 1 | 3))
            })
            .count();
        usize::from(self.threads[slot_index(self.thread.0)].depth)
            + boundaries
            + iterators
            + self.parser_nesting(end)
    }
    pub(super) fn coroutine_step(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        if id == CO_YIELD {
            if frame.pc() == 0 {
                if self.thread == self.main || frame.barriers > 1 {
                    let message = "attempt to yield across metamethod/C-call boundary";
                    let value = self.intern(message.as_bytes());
                    return Err(Error::raised(value, message));
                }
                self.frames[owner].pc = 1;
                self.transition = Some(Transition::Yield {
                    source: frame.vararg_start,
                    count: frame.argc(),
                });
                return Ok(None);
            }
            return Ok(self.return_frame(frame, base, self.top - base));
        }
        let wrapped = id != CO_RESUME;
        let phase = frame.pc & 3;
        if phase >= 2 {
            let source = base + (frame.pc >> 2) as usize;
            if wrapped && phase == 3 {
                let value = self.located_error(self.registers[source], 1);
                self.registers[source] = value;
                let message = match value {
                    Value::String(s) => {
                        String::from_utf8_lossy(&self.strings.bytes[slot_index(s.0)]).into_owned()
                    }
                    _ => format!("error object is a {} value", value.type_name()),
                };
                return Err(Error::raised(value, message));
            }
            return Ok(self.return_frame(frame, source, self.top - source));
        }
        let thread = if wrapped {
            let Value::Thread(thread) = self.native_closures[slot_index(id)].captures[0] else {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "invalid coroutine wrapper capture",
                ));
            };
            thread
        } else {
            let Value::Thread(thread) = self.argument(frame.vararg_start - 1, frame.argc(), 0)
            else {
                return Err(self.argument_error(0, b"coroutine expected"));
            };
            thread
        };
        let status = self.coroutine_status(thread);
        // A resumed thread inherits the caller's C-call count. Suspended native
        // callbacks and tag-method continuations each add one boundary; ordinary
        // Lua calls do not. The host's initial call accounts for the remaining one.
        let depth = self.native_depth(owner);
        if status != Status::Suspended || depth >= 199 {
            let message = if status != Status::Suspended {
                format!(
                    "cannot resume {} coroutine",
                    String::from_utf8_lossy(status.name())
                )
            } else {
                "C stack overflow".into()
            };
            if status == Status::Suspended
                && depth >= 199
                && self.threads[slot_index(thread.0)].frames.is_empty()
            {
                // lua_resume's initial-stack overflow error replaces the entry
                // function; auxresume removes that error, leaving a dead thread.
                let state = &mut self.threads[slot_index(thread.0)];
                state.entry = None;
                state.registers.clear();
                state.top = 0;
                state.api_stack = 0..0;
                state.status = Status::Dead;
            }
            let value = self.intern(message.as_bytes());
            let arguments =
                self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].to_vec();
            let start = base + arguments.len();
            self.resize_registers(self.registers.len().max(start + 2));
            self.registers[base..start].copy_from_slice(&arguments);
            if wrapped {
                self.registers[start] = value;
            } else {
                self.registers[start] = Value::Boolean(false);
                self.registers[start + 1] = value;
            }
            self.top = start + if wrapped { 1 } else { 2 };
            self.frames[owner].vararg_start = base;
            self.frames[owner].vararg_count = (self.top - base) as u32;
            self.frames[owner].pc = ((arguments.len() as u32) << 2) | 3;
            return Ok(None);
        }
        let offset = usize::from(!wrapped);
        let count = frame.argc() - offset;
        let args =
            &self.registers[frame.vararg_start + offset..frame.vararg_start + offset + count];
        let state = &mut self.threads[slot_index(thread.0)];
        let capacity = state.registers.capacity();
        if state.frames.is_empty() {
            let call = state.api_stack.end - 1;
            state.registers.truncate(state.api_stack.end);
            state.registers.extend_from_slice(args);
            state.top = state.registers.len();
            state.entry = Some((call, count));
        } else {
            let call = state
                .frames
                .last()
                .expect("suspended coroutine has a yield frame")
                .base;
            state
                .registers
                .resize(state.registers.len().max(call + count), Value::Nil);
            state.registers[call..call + count].copy_from_slice(args);
            state.top = call + count;
        }
        state.resumer = Some(self.thread);
        state.status = Status::Running;
        state.resume_status = NativeThreadStatus::Ok;
        state.depth = (depth + 1) as u16;
        let growth = (state.registers.capacity() - capacity) * std::mem::size_of::<Value>();
        self.threads.note_alloc(growth);
        // These are the caller's owned call temporaries. auxresume has moved
        // them away; the child now owns the values and the remaining C window
        // below retains only the explicit thread argument.
        self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].fill(Value::Nil);
        self.threads[slot_index(self.thread.0)].status = Status::Normal;
        // auxresume has moved its arguments off the C window. Keep the thread
        // argument in its owned native result area while the child executes.
        if !wrapped {
            self.registers[frame.base] = Value::Thread(thread);
        }
        self.frames[owner].vararg_start = frame.base;
        self.frames[owner].vararg_count = usize::from(!wrapped) as u32;
        self.frames[owner].pc = 1;
        self.transition = Some(Transition::Resume(thread));
        Ok(None)
    }
}
