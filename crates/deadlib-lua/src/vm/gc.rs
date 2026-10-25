//! Iterative tracing, weak tables, host roots and reusable object generations.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static VM_ID: AtomicU64 = AtomicU64::new(1);
pub(super) fn new_vm_identity() -> u64 {
    VM_ID.fetch_add(1, Ordering::Relaxed)
}

/// A pinned Lua value. Release it with [`Vm::unpin`] when the host is finished.
/// Root identities are checked against their originating VM and generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Root {
    vm: u64,
    key: u64,
}

/// VM heap estimates. Shared compiled code is counted once per loaded program.
/// Reserved bytes include reusable slots/capacities; live bytes exclude free slots.
/// Allocator headers and exact hash bucket layouts are excluded. Peaks are
/// observations at statistics/collection points, not allocator high-water marks.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapStats {
    /// Estimated storage of live objects, payloads and retained working buffers.
    pub live_bytes: usize,
    /// Estimated allocated capacities, including reusable vacant arena slots.
    pub reserved_bytes: usize,
    /// Largest reserved estimate observed by this VM.
    pub peak_reserved_bytes: usize,
    /// Live interned strings.
    pub strings: usize,
    /// Live tables.
    pub tables: usize,
    /// Live Lua closures.
    pub functions: usize,
    /// Live threads, including the main thread.
    pub threads: usize,
    /// Live native closures, excluding numeric builtin dispatch IDs.
    pub native_closures: usize,
    /// Live loaded program bindings.
    pub programs: usize,
    /// Live shared upvalue cells.
    pub upvalues: usize,
    /// Live full userdata objects.
    pub userdata: usize,
    /// Live file records, including closed userdata and active source readers.
    pub files: usize,
    /// Completed tracing cycles.
    pub collections: u64,
    /// Total arena objects reclaimed by completed cycles.
    pub reclaimed_objects: u64,
}

#[derive(Debug)]
pub(super) enum Trace {
    Value(Value),
    Program(usize),
    Cell(usize),
    File(u64),
}
fn edge(value: Value) -> Option<Trace> {
    match value {
        Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_) => None,
        _ => Some(Trace::Value(value)),
    }
}
#[derive(Debug)]
pub(super) struct Collector {
    phase: Phase,
    sweep_arena: u8,
    sweep_cursor: usize,
    stopped: bool,
    pause: i32,
    stepmul: i32,
    last_live: usize,
    threshold: usize,
    pub(super) collections: u64,
    reclaimed: u64,
    peak: usize,
    work: Vec<Trace>,
    erase: Vec<(usize, Value)>,
    natives: [u64; 4],
    control_allocated: usize,
    pub(super) pending: bool,
    pub(super) finalizers: std::collections::VecDeque<UserdataId>,
    pub(super) candidates: Vec<(u64, UserdataId)>,
    pub(super) running: usize,
    pub(super) closing: bool,
}
impl Default for Collector {
    fn default() -> Self {
        Self {
            phase: Phase::Pause,
            sweep_arena: 0,
            sweep_cursor: 0,
            stopped: false,
            pause: 200,
            stepmul: 200,
            last_live: 0,
            threshold: 64 * 1024,
            collections: 0,
            reclaimed: 0,
            peak: 0,
            work: Vec::new(),
            erase: Vec::new(),
            natives: [0; 4],
            control_allocated: 0,
            pending: false,
            finalizers: std::collections::VecDeque::new(),
            candidates: Vec::new(),
            running: 0,
            closing: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Pause,
    Sweep,
    Finalize,
}

impl Strings {
    fn remove(&mut self, key: u64) {
        let index = slot_index(key);
        let mask = self.buckets.len() - 1;
        let mut hole = Self::hash(&self.bytes[index]) & mask;
        while self.buckets[hole] != index as u32 + 1 {
            debug_assert_ne!(self.buckets[hole], 0);
            hole = (hole + 1) & mask;
        }
        let mut cursor = (hole + 1) & mask;
        while self.buckets[cursor] != 0 {
            // Backshift the cluster before freeing/reusing its arena slot.
            let entry = self.buckets[cursor];
            let home = Self::hash(&self.bytes[entry as usize - 1]) & mask;
            if (cursor.wrapping_sub(home) & mask) >= (hole.wrapping_sub(home) & mask) {
                self.buckets[hole] = entry;
                hole = cursor;
            }
            cursor = (cursor + 1) & mask;
        }
        self.buckets[hole] = 0;
        self.bytes.remove(key);
    }
    fn sweep_some(&mut self, cursor: &mut usize, budget: usize) -> u64 {
        let end = cursor.saturating_sub(budget);
        let mut reclaimed = 0;
        for index in (end..*cursor).rev() {
            if let Some(key) = self.bytes.key(index)
                && !self.bytes.marked(key)
            {
                self.remove(key);
                reclaimed += 1;
            }
        }
        *cursor = end;
        reclaimed
    }
    fn rebuild(&mut self) {
        let size = (self.bytes.live_len().saturating_mul(2))
            .max(16)
            .next_power_of_two();
        self.buckets.resize(size, 0);
        self.buckets.fill(0);
        for (i, bytes) in self.bytes.entries() {
            let mut slot = Self::hash(bytes) & (size - 1);
            while self.buckets[slot] != 0 {
                slot = (slot + 1) & (size - 1);
            }
            self.buckets[slot] = i as u32 + 1;
        }
    }
}
fn jobs_bytes(substitutions: &[strings::Substitution]) -> usize {
    substitutions
        .iter()
        .map(strings::Substitution::heap_bytes)
        .sum()
}
pub(super) fn program_bytes(p: &LoadedProgram) -> usize {
    let mut bytes = p.constants.len() * std::mem::size_of::<Box<[Value]>>()
        + p.constants
            .iter()
            .map(|k| k.len() * std::mem::size_of::<Value>())
            .sum::<usize>();
    bytes += p.data.name.len()
        + p.data.prototypes.len() * std::mem::size_of::<crate::bytecode::Prototype>();
    for f in &p.data.prototypes {
        bytes += f.code.len() * 8
            + f.debug.heap_bytes()
            + f.lines.len() * 4
            + f.gc_live.len() * 4
            + f.gc_maps.len() * 32
            + f.upvalues.len() * std::mem::size_of::<Upvalue>()
            + f.constants.len() * std::mem::size_of::<Constant>()
            + f.constants
                .iter()
                .map(|v| {
                    if let Constant::String(s) = v {
                        s.len()
                    } else {
                        0
                    }
                })
                .sum::<usize>();
    }
    bytes
}
fn frames_roots(
    vm: &Vm,
    registers: &[Value],
    frames: &[Frame],
    top: usize,
    cutoff: usize,
    work: &mut Vec<Trace>,
) {
    if frames.is_empty() {
        work.extend(registers.iter().take(top).copied().filter_map(edge));
        return;
    }
    for (i, frame) in frames.iter().enumerate() {
        match frame.function {
            FrameKind::Lua(function) => {
                work.push(Trace::Value(Value::Function(function)));
                let closure = &vm.functions[slot_index(function.0)];
                let proto = &vm.programs[closure.program].data.prototypes[closure.prototype];
                if frames
                    .get(i + 1)
                    .is_some_and(|f| matches!(f.function, FrameKind::Native(CONT_HOOK)))
                {
                    let end = (frame.base + usize::from(proto.registers))
                        .min(registers.len())
                        .min(cutoff);
                    work.extend(
                        registers[frame.base.min(end)..end]
                            .iter()
                            .copied()
                            .filter_map(edge),
                    );
                }
                let pc = frame.pc().saturating_sub(1).min(proto.code.len() - 1);
                let maps = proto.gc_live[pc];
                // Finalization interrupts an already completed opcode. Its
                // destination/results are live, unlike an outstanding Lua call.
                let interrupt = frames
                    .get(i + 1)
                    .filter(|f| matches!(f.function, FrameKind::Native(CONT_GC)));
                let completed = frame.pc() != 0 && (i + 1 == frames.len() || interrupt.is_some());
                let map = if completed {
                    (maps >> 16) as usize
                } else {
                    maps as u16 as usize
                };
                for (word, &bits) in proto.gc_maps[map].iter().enumerate() {
                    let mut bits = bits;
                    while bits != 0 {
                        let r = word * 64 + bits.trailing_zeros() as usize;
                        let pos = frame.base + r;
                        if pos < registers.len().min(cutoff) {
                            work.extend(edge(registers[pos]));
                        }
                        bits &= bits - 1;
                    }
                }
                let instruction = proto.code[pc];
                if completed
                    && ((matches!(instruction.op, Op::Call | Op::Vararg | Op::Move)
                        && instruction.c == OPEN)
                        || instruction.op == Op::TailCall)
                {
                    let start = frame.base + usize::from(instruction.a);
                    let frame_top = interrupt.map_or(top, |f| f.result);
                    let end = frame_top.min(cutoff).min(registers.len());
                    work.extend(
                        registers[start.min(end)..end]
                            .iter()
                            .copied()
                            .filter_map(edge),
                    );
                }
            }
            FrameKind::Native(id) => {
                if id == CONT_GC && frame.wanted == FRAME_OPEN {
                    work.extend(
                        registers
                            .iter()
                            .take(frame.result)
                            .copied()
                            .filter_map(edge),
                    );
                }
                if !(CONT_RAW..strings::ITERATOR_FIRST).contains(&id) {
                    work.push(Trace::Value(Value::Native(id)));
                }
                let end = frames
                    .get(i + 1)
                    .map_or(registers.len(), |f| f.stack_start)
                    .min(cutoff);
                work.extend(
                    registers[frame.base.min(end)..end]
                        .iter()
                        .copied()
                        .filter_map(edge),
                );
            }
        }
        if !matches!(frame.function, FrameKind::Native(CONT_HOOK | CONT_TRACE))
            && (!matches!(frame.function, FrameKind::Native(threads::CO_YIELD)) || frame.pc() == 0)
        {
            let start = frame.vararg_start;
            let end = (start + frame.argc()).min(registers.len()).min(cutoff);
            work.extend(
                registers[start.min(end)..end]
                    .iter()
                    .copied()
                    .filter_map(edge),
            );
        }
    }
}

impl Vm {
    pub(super) fn gc_note_control(&mut self, bytes: usize) {
        self.gc.control_allocated = self.gc.control_allocated.saturating_add(bytes);
    }
    pub(super) fn validate_value(&self, value: Value) -> Result<(), Error> {
        let valid = match value {
            Value::String(id) => self.strings.bytes.contains(id.0),
            Value::Table(id) => self.tables.contains(id.0),
            Value::Function(id) => self.functions.contains(id.0),
            Value::Thread(id) => self.threads.contains(id.0),
            Value::Userdata(id) => self.userdata.contains(id.0),
            Value::Native(id) if id >= strings::ITERATOR_FIRST => self.native_closures.contains(id),
            Value::Native(id) => {
                (matches!(id,0..=59|64..=77|80..=85|96..=104|debug::GET_INFO..=debug::GET_REGISTRY|DO_FILE|io::FIRST..=io::LAST|os::FIRST..=os::LAST)
                    || package::contains(id))
                    && (self.gc.collections == 0 || self.marked_value(value))
            }
            _ => true,
        };
        let live_during_sweep = self.gc.phase != Phase::Sweep
            || match value {
                Value::String(id) => self.strings.bytes.marked(id.0),
                _ => self.marked_value(value),
            };
        if valid && live_during_sweep {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::Runtime,
                0,
                "invalid or collected Lua object handle",
            ))
        }
    }
    /// Keep a value alive across future executions and garbage collections.
    /// Values must originate in this VM. Root handles additionally check VM ownership.
    ///
    /// # Errors
    /// Returns an error if the object handle is invalid or has been collected.
    pub fn pin(&mut self, value: Value) -> Result<Root, Error> {
        self.validate_value(value)?;
        Ok(Root {
            vm: self.identity,
            key: self.roots.insert(value),
        })
    }
    /// Read a pinned value, rejecting released roots or roots from another VM.
    ///
    /// # Errors
    /// Returns an error if the root belongs to another VM or has been released.
    pub fn rooted(&self, root: Root) -> Result<Value, Error> {
        if root.vm == self.identity
            && let Some(value) = self.roots.get(root.key)
        {
            return Ok(*value);
        }
        Err(Error::new(ErrorKind::Runtime, 0, "invalid Lua root"))
    }
    /// Release a root. Its value can be collected at the next collection.
    ///
    /// # Errors
    /// Returns an error if the root belongs to another VM or has been released.
    pub fn unpin(&mut self, root: Root) -> Result<Value, Error> {
        self.rooted(root)?;
        Ok(self.roots.remove(root.key).expect("validated root"))
    }
    /// Inspect live object counts and estimated VM-owned heap storage.
    pub fn heap_stats(&mut self) -> HeapStats {
        let mut payload = self.strings.bytes.iter().map(|s| s.len()).sum::<usize>()
            + self.strings.buckets.capacity() * 4;
        payload += self.tables.iter().map(Table::heap_bytes).sum::<usize>();
        payload += self
            .functions
            .iter()
            .map(|f| f.upvalues.len() * 4)
            .sum::<usize>();
        payload += self.programs.iter().map(program_bytes).sum::<usize>();
        payload += self.userdata.iter().map(|u| u.bytes.len()).sum::<usize>();
        payload += self.io.heap_bytes();
        let mut control = self.registers.capacity() * std::mem::size_of::<Value>()
            + self.frames.capacity() * std::mem::size_of::<Frame>()
            + self.open_upvalues.capacity() * 4
            + self.substitutions.capacity() * std::mem::size_of::<strings::Substitution>()
            + jobs_bytes(&self.substitutions)
            + self.sorts.capacity() * std::mem::size_of::<tables::Sort>();
        control += loads::storage_bytes(&self.loads);
        control += self.tracebacks.capacity() * std::mem::size_of::<traces::Traceback>();
        for (_, t) in self.threads.entries() {
            control += t.registers.capacity() * std::mem::size_of::<Value>()
                + t.frames.capacity() * std::mem::size_of::<Frame>()
                + t.open_upvalues.capacity() * 4
                + t.substitutions.capacity() * std::mem::size_of::<strings::Substitution>()
                + jobs_bytes(&t.substitutions)
                + t.sorts.capacity() * std::mem::size_of::<tables::Sort>();
            control += loads::storage_bytes(&t.loads);
            control += t.tracebacks.capacity() * std::mem::size_of::<traces::Traceback>();
        }
        control += self.buffers.capacity() * std::mem::size_of::<Vec<u8>>()
            + self.buffers.iter().map(Vec::capacity).sum::<usize>()
            + self.output.as_ref().map_or(0, Vec::capacity)
            + self.pattern_work.heap_bytes()
            + self.gc.work.capacity() * std::mem::size_of::<Trace>()
            + self.gc.erase.capacity() * std::mem::size_of::<(usize, Value)>()
            + self.patterns.values().map(crate::pattern::Pattern::heap_bytes).sum::<usize>()
            // HashMap bucket counts/allocator headers are implementation details;
            // capacity-based estimates include payloads and one control byte.
            + self.patterns.capacity() * (std::mem::size_of::<((StringId, bool), crate::pattern::Pattern)>() + 1)
            + self.native_envs.capacity() * (std::mem::size_of::<(u64, TableId)>() + 1);
        control += self.proxy_metatables.capacity() * (std::mem::size_of::<TableId>() + 1);
        control += self.hook_functions.capacity() * (std::mem::size_of::<(ThreadId, Value)>() + 1);
        control += self.gc.finalizers.capacity() * std::mem::size_of::<UserdataId>()
            + self.gc.candidates.capacity() * std::mem::size_of::<(u64, UserdataId)>();
        let objects = self.strings.bytes.live_len() * std::mem::size_of::<Box<[u8]>>()
            + self.tables.live_len() * std::mem::size_of::<Table>()
            + self.functions.live_len() * std::mem::size_of::<Closure>()
            + self.upvalues.live_len() * std::mem::size_of::<Cell>()
            + self.threads.live_len() * std::mem::size_of::<threads::Thread>()
            + self.native_closures.live_len() * std::mem::size_of::<threads::NativeClosure>()
            + self.programs.live_len() * std::mem::size_of::<LoadedProgram>()
            + self.roots.live_len() * std::mem::size_of::<Value>();
        let objects = objects
            + self.userdata.live_len() * std::mem::size_of::<userdata::Userdata>()
            + self.io.files.live_len() * std::mem::size_of::<io::Stream>();
        let storage = self.strings.bytes.storage_bytes()
            + self.tables.storage_bytes()
            + self.functions.storage_bytes()
            + self.upvalues.storage_bytes()
            + self.threads.storage_bytes()
            + self.native_closures.storage_bytes()
            + self.programs.storage_bytes()
            + self.roots.storage_bytes();
        let storage = storage + self.userdata.storage_bytes() + self.io.files.storage_bytes();
        let reserved = payload + control + storage;
        self.gc.peak = self.gc.peak.max(reserved);
        HeapStats {
            live_bytes: payload + control + objects,
            reserved_bytes: reserved,
            peak_reserved_bytes: self.gc.peak,
            strings: self.strings.bytes.live_len(),
            tables: self.tables.live_len(),
            functions: self.functions.live_len(),
            threads: self.threads.live_len(),
            native_closures: self.native_closures.live_len(),
            programs: self.programs.live_len(),
            upvalues: self.upvalues.live_len(),
            userdata: self.userdata.live_len(),
            files: self.io.files.live_len(),
            collections: self.gc.collections,
            reclaimed_objects: self.gc.reclaimed,
        }
    }
    pub(super) fn reset_gc_debt(&mut self) {
        self.strings.bytes.allocated = 0;
        self.tables.allocated = 0;
        self.functions.allocated = 0;
        self.upvalues.allocated = 0;
        self.threads.allocated = 0;
        self.native_closures.allocated = 0;
        self.programs.allocated = 0;
        self.roots.allocated = 0;
        self.userdata.allocated = 0;
        self.io.files.allocated = 0;
        self.gc.control_allocated = 0;
        self.gc.pending = false;
        self.gc.last_live = self.heap_stats().live_bytes;
        self.set_gc_threshold();
    }
    fn set_gc_threshold(&mut self) {
        self.gc.threshold = self
            .gc
            .last_live
            .saturating_mul(self.gc.pause.max(1) as usize)
            .saturating_div(100)
            .max(64 * 1024);
    }
    pub(super) fn finish_gc_cycle(&mut self) {
        self.gc.stopped = false;
        self.set_gc_threshold();
        if self.gc.finalizers.is_empty() {
            self.gc.phase = Phase::Pause;
            self.protect_new_allocations(false);
        }
    }
    pub(super) fn enter_gc_finalizer(&mut self, owner: usize) {
        // CONT_GC has no arguments. Its unused argument start saves the native
        // threshold; pc records the callback phase and previous stop state.
        debug_assert_eq!(self.frames[owner].argc(), 0);
        self.frames[owner].vararg_start = self.gc.threshold;
        self.frames[owner].pc = 1 | (u32::from(self.gc.stopped) << 1);
        self.gc.threshold = self
            .gc
            .last_live
            .saturating_add(self.gc_debt())
            .saturating_mul(2);
        self.gc.stopped = false;
    }
    pub(super) fn leave_gc_finalizer(&mut self, owner: usize, frame: Frame) {
        self.gc.threshold = frame.vararg_start;
        self.gc.stopped = frame.pc & 2 != 0;
        self.frames[owner].pc = 0;
        self.frames[owner].vararg_start = 0;
    }
    fn gc_debt(&self) -> usize {
        self.strings
            .bytes
            .allocated
            .saturating_add(self.tables.allocated)
            .saturating_add(self.functions.allocated)
            .saturating_add(self.upvalues.allocated)
            .saturating_add(self.threads.allocated)
            .saturating_add(self.native_closures.allocated)
            .saturating_add(self.programs.allocated)
            .saturating_add(self.userdata.allocated)
            .saturating_add(self.io.files.allocated)
            .saturating_add(self.gc.control_allocated)
    }
    pub(super) fn maybe_gc(&mut self) {
        self.gc.pending = false;
        if !self.gc.stopped
            && self.gc.running == 0
            && !self.gc.closing
            && self.gc.last_live.saturating_add(self.gc_debt()) >= self.gc.threshold
        {
            if self.advance_gc_some(self.gc_step_budget(0)) {
                if self.gc.finalizers.is_empty() {
                    self.finish_gc_cycle();
                } else {
                    self.push_finalizers(false);
                }
            } else {
                self.defer_gc_until_allocation();
            }
        }
    }
    fn table_mode(&self, table: &Table) -> (bool, bool) {
        let Some(mt) = table.metatable else {
            return (false, false);
        };
        let Value::String(mode) =
            self.tables[slot_index(mt.0)].get(Value::String(self.meta_names[Meta::Mode as usize]))
        else {
            return (false, false);
        };
        let mode = &self.strings.bytes[slot_index(mode.0)];
        let mode = &mode[..mode.iter().position(|&b| b == 0).unwrap_or(mode.len())];
        (mode.contains(&b'k'), mode.contains(&b'v'))
    }
    fn marked_value(&self, value: Value) -> bool {
        match value {
            Value::String(_) => true,
            Value::Table(id) => self.tables.marked(id.0),
            Value::Function(id) => self.functions.marked(id.0),
            Value::Thread(id) => self.threads.marked(id.0),
            Value::Userdata(id) => self.userdata.marked(id.0),
            Value::Native(id) if id >= strings::ITERATOR_FIRST => self.native_closures.marked(id),
            Value::Native(id) => {
                id < 256 && self.gc.natives[id as usize / 64] & (1u64 << (id % 64)) != 0
            }
            _ => true,
        }
    }
    fn trace_thread(&mut self, id: ThreadId, work: &mut Vec<Trace>) {
        let t = &self.threads[slot_index(id.0)];
        if let Some(entry) = t.entry {
            work.push(Trace::Value(Value::Function(entry)));
        }
        if let Some(parent) = t.resumer {
            work.push(Trace::Value(Value::Thread(parent)));
        }
        let (registers, frames, open, env, top, subs, sorts) = if id == self.thread {
            (
                &self.registers,
                &self.frames,
                &self.open_upvalues,
                self.globals,
                self.top,
                &self.substitutions,
                &self.sorts,
            )
        } else {
            (
                &t.registers,
                &t.frames,
                &t.open_upvalues,
                t.globals,
                t.top,
                &t.substitutions,
                &t.sorts,
            )
        };
        work.push(Trace::Value(Value::Table(env)));
        work.extend(open.iter().map(|&i| Trace::Cell(i as usize)));
        let cutoff = if t.status == threads::Status::Suspended {
            frames
                .last()
                .filter(|f| matches!(f.function, FrameKind::Native(threads::CO_YIELD)))
                .map_or(registers.len(), |f| f.vararg_start)
        } else {
            registers.len()
        };
        frames_roots(self, registers, frames, top, cutoff, work);
        for job in subs {
            work.extend(job.roots().into_iter().filter_map(edge));
        }
        for job in sorts {
            work.extend(job.roots().into_iter().filter_map(edge));
        }
        let loads = if id == self.thread {
            &self.loads
        } else {
            &t.loads
        };
        work.extend(
            loads
                .iter()
                .filter_map(|j| j.file.map(|f| Trace::File(f.key))),
        );
        self.shrink_thread_calls(id);
    }
    fn marked_weak_value(&self, value: Value, key: bool) -> bool {
        self.marked_value(value)
            && (key
                || !matches!(value, Value::Userdata(id) if self.userdata[slot_index(id.0)].finalized))
    }
    fn trace_graph(&mut self, work: &mut Vec<Trace>) {
        while let Some(item) = work.pop() {
            match item {
                Trace::Value(Value::String(id)) => {
                    self.strings.bytes.mark(id.0);
                }
                Trace::Value(Value::Table(id)) => {
                    if self.tables.mark(id.0) {
                        let table = &self.tables[slot_index(id.0)];
                        if let Some(mt) = table.metatable {
                            work.push(Trace::Value(Value::Table(mt)));
                        }
                        let (weak_keys, weak_values) = self.table_mode(table);
                        for (k, v) in table.pairs() {
                            if !weak_keys || matches!(k, Value::String(_)) {
                                work.extend(edge(k));
                            }
                            if !weak_values || matches!(v, Value::String(_)) {
                                work.extend(edge(v));
                            }
                        }
                    }
                }
                Trace::Value(Value::Function(id)) => {
                    if self.functions.mark(id.0) {
                        let f = &self.functions[slot_index(id.0)];
                        work.push(Trace::Value(Value::Table(f.env)));
                        work.push(Trace::Program(f.program));
                        work.extend(f.upvalues.iter().map(|&i| Trace::Cell(i as usize)));
                    }
                }
                Trace::Value(Value::Userdata(id)) => {
                    if self.userdata.mark(id.0) {
                        let u = &self.userdata[slot_index(id.0)];
                        if let Some(file) = u.file {
                            self.io.files.mark(file.get());
                        }
                        work.push(Trace::Value(Value::Table(u.env)));
                        if let Some(mt) = u.metatable {
                            work.push(Trace::Value(Value::Table(mt)));
                        }
                    }
                }
                Trace::Value(Value::Thread(id)) => {
                    if self.threads.mark(id.0) {
                        self.trace_thread(id, work);
                    }
                }
                Trace::Value(Value::Native(id)) => {
                    if id >= strings::ITERATOR_FIRST {
                        if self.native_closures.mark(id) {
                            let f = &self.native_closures[slot_index(id)];
                            work.push(Trace::Value(Value::Table(f.env)));
                            match &f.kind {
                                threads::NativeKind::Match(m) => {
                                    work.extend(m.roots().into_iter().filter_map(edge))
                                }
                                threads::NativeKind::Wrap(t) => {
                                    work.push(Trace::Value(Value::Thread(*t)))
                                }
                                threads::NativeKind::Lines { file, .. } => {
                                    work.push(Trace::Value(Value::Userdata(*file)))
                                }
                            }
                        }
                    } else if id < 256 {
                        let word = id as usize / 64;
                        let bit = 1u64 << (id % 64);
                        if self.gc.natives[word] & bit == 0 {
                            self.gc.natives[word] |= bit;
                            work.push(Trace::Value(Value::Table(self.native_environment(id))));
                        }
                    }
                }
                Trace::Program(i) => {
                    if let Some(key) = self.programs.key(i)
                        && self.programs.mark(key)
                    {
                        for k in &self.programs[i].constants {
                            work.extend(k.iter().copied().filter_map(edge));
                        }
                    }
                }
                Trace::Cell(i) => {
                    if let Some(key) = self.upvalues.key(i)
                        && self.upvalues.mark(key)
                    {
                        work.extend(edge(self.cell_value(i)));
                    }
                }
                Trace::File(key) => {
                    self.io.files.mark(key);
                }
                _ => {}
            }
        }
    }
    fn begin_gc_cycle(&mut self) {
        self.heap_stats();
        self.protect_new_allocations(false);
        self.strings.bytes.clear_marks();
        self.tables.clear_marks();
        self.functions.clear_marks();
        self.upvalues.clear_marks();
        self.threads.clear_marks();
        self.native_closures.clear_marks();
        self.programs.clear_marks();
        self.userdata.clear_marks();
        self.io.files.clear_marks();
        if let Some(key) = self.io.stdin {
            self.io.files.mark(key.get());
        }
        self.gc.natives.fill(0);
        let mut work = std::mem::take(&mut self.gc.work);
        work.clear();
        work.push(Trace::Value(Value::Thread(self.main)));
        work.push(Trace::Value(Value::Table(self.registry)));
        if let Some(tag) = self.io.file_tag {
            work.push(Trace::Value(Value::String(tag)));
        }
        work.push(Trace::Value(Value::Thread(self.thread)));
        work.extend(
            self.meta_names
                .iter()
                .copied()
                .map(|s| Trace::Value(Value::String(s))),
        );
        work.extend(
            self.type_metatables
                .iter()
                .flatten()
                .copied()
                .map(|t| Trace::Value(Value::Table(t))),
        );
        work.extend(self.roots.iter().copied().filter_map(edge));
        work.extend(self.hook_functions.values().copied().filter_map(edge));
        // A live host Program retains its mutable per-VM entry binding. Internal
        // code ownership by escaped Lua closures does not pin unused entries.
        work.extend(
            self.programs
                .iter()
                .filter(|p| p.owner.strong_count() != 0)
                .map(|p| Trace::Value(Value::Function(p.entry))),
        );
        self.trace_graph(&mut work);
        // Lua 5.1 revisits initially reachable threads in its gray-again pass.
        // Threads reached only through finalizer preservation are visited once.
        for index in 0..self.threads.len() {
            if let Some(key) = self.threads.key(index)
                && self.threads.marked(key)
            {
                self.shrink_thread_calls(ThreadId(key));
            }
        }
        // Pending callbacks preserve their graphs only after discovering newly
        // finalizable userdata, including graphs attached after a callback error.
        self.separate_userdata(false);
        work.extend(
            self.gc
                .finalizers
                .iter()
                .copied()
                .map(|id| Trace::Value(Value::Userdata(id))),
        );
        self.trace_graph(&mut work);
        let mut erase = std::mem::take(&mut self.gc.erase);
        erase.clear();
        for (i, t) in self.tables.entries() {
            if self.tables.key(i).is_some_and(|k| self.tables.marked(k)) {
                let (wk, wv) = self.table_mode(t);
                if wk || wv {
                    for (k, v) in t.pairs() {
                        if (wk && !self.marked_weak_value(k, true))
                            || (wv && !self.marked_weak_value(v, false))
                        {
                            erase.push((i, k));
                        }
                    }
                }
            }
        }
        for (i, key) in erase.drain(..) {
            self.tables[i].set(key, Value::Nil).expect("existing key");
        }
        self.gc.erase = erase;
        for (i, t) in self.threads.entries() {
            if self.threads.key(i).is_some_and(|k| !self.threads.marked(k)) {
                for &cell in &t.open_upvalues {
                    if let Cell::Open(_, r) = self.upvalues[cell as usize] {
                        self.upvalues[cell as usize] = Cell::Closed(t.registers[r]);
                    }
                }
            }
        }
        self.gc.work = work;
        self.protect_new_allocations(true);
        self.gc.phase = Phase::Sweep;
        self.gc.sweep_arena = 0;
        self.gc.sweep_cursor = self.tables.len();
    }
    fn protect_new_allocations(&mut self, enabled: bool) {
        self.tables.protect_new(enabled);
        self.functions.protect_new(enabled);
        self.upvalues.protect_new(enabled);
        self.threads.protect_new(enabled);
        self.native_closures.protect_new(enabled);
        self.programs.protect_new(enabled);
        self.strings.bytes.protect_new(enabled);
        self.userdata.protect_new(enabled);
        self.io.files.protect_new(enabled);
    }
    fn sweep_arena_len(&self, arena: u8) -> usize {
        match arena {
            0 => self.tables.len(),
            1 => self.functions.len(),
            2 => self.upvalues.len(),
            3 => self.threads.len(),
            4 => self.native_closures.len(),
            5 => self.programs.len(),
            6 => self.strings.bytes.len(),
            7 => self.userdata.len(),
            8 => self.io.files.len(),
            _ => 0,
        }
    }
    fn sweep_gc_some(&mut self, mut budget: usize) {
        while budget != 0 && self.gc.sweep_arena < 9 {
            let mut cursor = self.gc.sweep_cursor;
            let before = cursor;
            let count = match self.gc.sweep_arena {
                0 => self.tables.sweep_some(&mut cursor, budget),
                1 => self.functions.sweep_some(&mut cursor, budget),
                2 => self.upvalues.sweep_some(&mut cursor, budget),
                3 => self.threads.sweep_some(&mut cursor, budget),
                4 => self.native_closures.sweep_some(&mut cursor, budget),
                5 => self.programs.sweep_some(&mut cursor, budget),
                6 => self.strings.sweep_some(&mut cursor, budget),
                7 => self.userdata.sweep_some(&mut cursor, budget),
                8 => self.io.files.sweep_some(&mut cursor, budget),
                _ => unreachable!(),
            };
            self.gc.reclaimed += count;
            budget -= before - cursor;
            if cursor == 0 {
                self.gc.sweep_arena += 1;
                cursor = self.sweep_arena_len(self.gc.sweep_arena);
            }
            self.gc.sweep_cursor = cursor;
        }
        if self.gc.sweep_arena != 9 {
            return;
        }
        self.proxy_metatables
            .retain(|id| self.tables.contains(id.0));
        self.strings.rebuild();
        self.patterns
            .retain(|(id, _), _| self.strings.bytes.contains(id.0));
        self.native_envs.retain(|id, _| {
            *id < 256 && self.gc.natives[*id as usize / 64] & (1u64 << (*id % 64)) != 0
        });
        self.gc.phase = Phase::Finalize;
        self.gc.collections += 1;
        self.reset_gc_debt();
        self.heap_stats();
    }
    fn advance_gc_some(&mut self, budget: usize) -> bool {
        if self.gc.phase == Phase::Pause {
            self.begin_gc_cycle();
        }
        if self.gc.phase == Phase::Sweep {
            self.sweep_gc_some(budget);
        }
        self.gc.phase == Phase::Finalize
    }
    fn gc_step_budget(&self, extra: i32) -> usize {
        if self.gc.stepmul == 0 {
            return usize::MAX;
        }
        (extra as usize)
            .wrapping_shl(10)
            .max(1024)
            .saturating_mul(self.gc.stepmul.max(1) as usize)
            .saturating_div(100)
            .saturating_div(16)
            .max(1)
    }
    fn defer_gc_until_allocation(&mut self) {
        self.gc.threshold = self
            .gc
            .last_live
            .saturating_add(self.gc_debt())
            .saturating_add(1);
    }
    pub(super) fn finish_pending_sweep(&mut self) {
        if self.gc.phase == Phase::Sweep {
            self.sweep_gc_some(usize::MAX);
        }
    }
    pub(super) fn collect_cycle(&mut self) -> HeapStats {
        self.finish_pending_sweep();
        self.begin_gc_cycle();
        self.sweep_gc_some(usize::MAX);
        self.heap_stats()
    }
    pub(super) fn gc_native(&mut self, id: u64, base: usize, count: usize) -> Result<bool, Error> {
        if id == GC_INFO {
            self.registers[base] = Value::Number((self.heap_stats().live_bytes / 1024) as f64);
            return Ok(false);
        }
        let option = if self.argument(base, count, 0) == Value::Nil {
            None
        } else {
            Some(self.string_argument(base, count, 0)?)
        };
        let option = option.map_or(b"collect".as_slice(), |id| {
            self.strings.bytes[slot_index(id.0)].as_ref()
        });
        let option = &option[..option.iter().position(|&b| b == 0).unwrap_or(option.len())];
        let choices: [&[u8]; 7] = [
            b"stop",
            b"restart",
            b"collect",
            b"count",
            b"step",
            b"setpause",
            b"setstepmul",
        ];
        let choice = choices
            .iter()
            .position(|&name| name == option)
            .ok_or_else(|| self.argument_option_error(0, option))?;
        let extra = self.integer_argument(base, count, 1, Some(0))? as i32;
        let option = choices[choice];
        let mut dispatch = false;
        let value = match option {
            b"stop" => {
                self.gc.stopped = true;
                Value::Number(0.0)
            }
            b"restart" => {
                self.gc.stopped = false;
                // Frame-boundary checks must wait for the next allocation.
                self.defer_gc_until_allocation();
                Value::Number(0.0)
            }
            b"collect" => {
                self.collect_cycle();
                self.finish_gc_cycle();
                dispatch = true;
                Value::Number(0.0)
            }
            b"count" => Value::Number(self.heap_stats().live_bytes as f64 / 1024.0),
            b"setpause" => {
                let old = self.gc.pause;
                self.gc.pause = extra;
                Value::Number(f64::from(old))
            }
            b"setstepmul" => {
                let old = self.gc.stepmul;
                self.gc.stepmul = extra;
                Value::Number(f64::from(old))
            }
            b"step" => {
                self.gc.stopped = false;
                let complete = self.advance_gc_some(self.gc_step_budget(extra));
                if complete {
                    self.finish_gc_cycle();
                    dispatch = true;
                } else {
                    self.defer_gc_until_allocation();
                }
                Value::Boolean(complete)
            }
            _ => unreachable!("validated collector option"),
        };
        self.registers[base] = value;
        Ok(dispatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_sweeping_repairs_wrapped_clusters_and_preserves_reinterned_values() {
        let mut pool = Strings::default();
        let texts: Vec<_> = (0..)
            .map(|n| format!("collision-{n}").into_bytes())
            .filter(|s| Strings::hash(s) & 15 == 14)
            .take(6)
            .collect();
        let ids: Vec<_> = texts.iter().map(|s| pool.intern(s)).collect();
        pool.bytes.mark(ids[0].0);
        pool.bytes.mark(ids[3].0);
        pool.bytes.protect_new(true);
        let mut cursor = pool.bytes.len();
        assert_eq!(pool.sweep_some(&mut cursor, 2), 2);
        assert_eq!(pool.intern(&texts[2]), ids[2]);
        let fresh = pool.intern(b"new-after-sweep");
        while cursor != 0 {
            pool.sweep_some(&mut cursor, 2);
        }
        for index in [0, 2, 3] {
            assert_eq!(pool.find(&texts[index]), Some(ids[index]));
        }
        for index in [1, 4, 5] {
            assert_eq!(pool.find(&texts[index]), None);
            assert!(!pool.bytes.contains(ids[index].0));
        }
        assert_eq!(pool.find(b"new-after-sweep"), Some(fresh));
        pool.bytes.protect_new(false);
        pool.bytes.clear_marks();
        let mut cursor = pool.bytes.len();
        pool.sweep_some(&mut cursor, usize::MAX);
        assert_eq!(pool.find(b"new-after-sweep"), None);
        assert_eq!(pool.bytes.live_len(), 0);
    }

    #[test]
    fn sweep_slices_keep_new_mutable_graphs_and_reject_logically_dead_handles() {
        let mut vm = Vm::new();
        vm.execute(
            &crate::compile(b"collectgarbage('stop');holder={};for i=1,1000 do local t={}end")
                .unwrap(),
        )
        .unwrap();
        let stale = Value::Table(vm.new_table());
        let old_string = vm.intern(b"revived-during-sweep");
        vm.begin_gc_cycle();
        assert_eq!(vm.gc.phase, Phase::Sweep);
        assert!(vm.pin(stale).is_err());
        assert!(vm.pin(old_string).is_err());
        assert_eq!(vm.intern(b"revived-during-sweep"), old_string);
        let string_root = vm.pin(old_string).unwrap();
        let mutate = crate::compile(
            br#"
          fresh=fresh or {};local obj={text='new'..#fresh};fresh[#fresh+1]=obj
          holder.value=obj;local state=obj;getter=function()return state end
          co=coroutine.create(function()coroutine.yield(state);return state end)
          local ok,v=coroutine.resume(co);assert(ok and v==obj)
          guard=newproxy(true);getmetatable(guard).__gc=function()finalized=(finalized or 0)+1 end
          obj.guard=guard
        "#,
        )
        .unwrap();
        let mut appends = 0;
        for step in 0..10000 {
            if step % 31 == 0 {
                vm.execute(&mutate).unwrap();
                appends += 1;
            }
            let before = vm.gc.reclaimed;
            vm.sweep_gc_some(1);
            assert!(vm.gc.reclaimed - before <= 1);
            if vm.gc.phase == Phase::Finalize {
                break;
            }
        }
        assert_eq!(vm.gc.phase, Phase::Finalize);
        vm.finish_gc_cycle();
        assert_eq!(vm.rooted(string_root).unwrap(), old_string);
        let check=crate::compile(b"local ok,v=coroutine.resume(co);return ok and v==holder.value and getter()==v,#fresh,finalized or 0").unwrap();
        assert_eq!(
            vm.execute(&check).unwrap(),
            &[
                Value::Boolean(true),
                Value::Number(f64::from(appends)),
                Value::Number(0.0)
            ]
        );
        vm.collect_garbage().unwrap();
        vm.execute(&crate::compile(b"fresh=nil;holder=nil;getter=nil;co=nil;guard=nil").unwrap())
            .unwrap();
        vm.collect_garbage().unwrap();
        assert_eq!(vm.global(b"finalized"), Value::Number(f64::from(appends)));
        vm.unpin(string_root).unwrap();
    }

    #[test]
    fn all_registered_builtins_can_be_pinned_across_sweeping() {
        let mut vm = Vm::new();
        let code=crate::compile(b"local out={};for _,lib in ipairs{_G,math,string,table,coroutine,debug,io,os,package}do for _,v in pairs(lib)do if type(v)=='function'then out[#out+1]=v end end end;return unpack(out)").unwrap();
        let values = vm.execute(&code).unwrap().to_vec();
        let roots: Vec<_> = values
            .iter()
            .map(|&value| vm.pin(value).unwrap_or_else(|e| panic!("{value:?}: {e}")))
            .collect();
        vm.begin_gc_cycle();
        for (&value, &root) in values.iter().zip(&roots) {
            assert_eq!(vm.rooted(root).unwrap(), value);
        }
        vm.sweep_gc_some(usize::MAX);
        vm.finish_gc_cycle();
        for (&value, &root) in values.iter().zip(&roots) {
            assert_eq!(vm.rooted(root).unwrap(), value);
            vm.unpin(root).unwrap();
        }
    }
}
