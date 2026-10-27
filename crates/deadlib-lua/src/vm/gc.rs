//! Iterative tracing, weak tables, host roots and reusable object generations.
use super::*;
use crate::table::TableTrace;
use std::sync::atomic::{AtomicU64, Ordering};

static VM_ID: AtomicU64 = AtomicU64::new(1);

/// A synchronous collector operation available within a native callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GcOperation {
    /// Stop automatic collection until an explicit restart, step or collection.
    Stop,
    /// Restart automatic collection at the next allocation check.
    Restart,
    /// Complete a cycle and run its pending finalizers.
    Collect,
    /// Read estimated live storage in whole kibibytes.
    CountKilobytes,
    /// Read the remainder of estimated live storage modulo 1024 bytes.
    CountRemainder,
    /// Advance collection using Lua 5.1's pointer-width byte request.
    Step(i32),
    /// Replace the collector pause percentage, returning its previous value.
    SetPause(i32),
    /// Replace the step multiplier, returning its previous value.
    SetStepMultiplier(i32),
}
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
    /// Live private native-parser prototypes.
    pub parser_prototypes: usize,
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
    MarkedValue(Value),
    ThreadAgain(ThreadId),
    TableAgain(TableId),
    TableSlice(TableId, usize),
    CellAgain(usize),
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
    natives_previous: [u64; 4],
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
            natives_previous: [0; 4],
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
    Propagate,
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
    let native_roots = std::mem::size_of_val(p.parser_prototypes.as_ref());
    let mut bytes = p.constants.len() * std::mem::size_of::<Box<[Value]>>()
        + p.constants
            .iter()
            .map(|k| k.len() * std::mem::size_of::<Value>())
            .sum::<usize>();
    bytes += std::mem::size_of_val(p.upvalue_names.as_ref())
        + p.upvalue_names
            .iter()
            .map(|names| std::mem::size_of_val(names.as_ref()))
            .sum::<usize>();
    bytes += p.data.name.len()
        + std::mem::size_of_val(p.data.sources.as_ref())
        + p.data.sources.iter().map(|s| s.len()).sum::<usize>()
        + p.data.prototypes.len() * std::mem::size_of::<crate::bytecode::Prototype>();
    for f in &p.data.prototypes {
        bytes += f.code.len() * 8
            + f.debug.heap_bytes()
            + f.lines.len() * 4
            + f.gc_live.len() * 4
            + f.gc_maps.len() * 32
            + f.upvalues.len() * std::mem::size_of::<Upvalue>()
            + std::mem::size_of_val(f.bindings.as_ref())
            + f.bindings
                .iter()
                .map(|b| std::mem::size_of_val(b.upvalues.as_ref()))
                .sum::<usize>()
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
    bytes + native_roots
}
fn register_roots(
    registers: &[Value],
    range: std::ops::Range<usize>,
    live: &mut [u64],
    work: &mut Vec<Trace>,
) {
    for position in range {
        live[position / 64] |= 1u64 << (position % 64);
        work.extend(edge(registers[position]));
    }
}
fn frames_roots(
    vm: &Vm,
    registers: &[Value],
    frames: &[Frame],
    top: usize,
    cutoff: usize,
    live: &mut [u64],
    work: &mut Vec<Trace>,
) {
    if frames.is_empty() {
        register_roots(
            registers,
            0..top.min(registers.len()).min(cutoff),
            live,
            work,
        );
        return;
    }
    if let Some(frame) = frames.first() {
        register_roots(
            registers,
            0..frame.result.min(registers.len()).min(cutoff),
            live,
            work,
        );
    }
    for (i, frame) in frames.iter().enumerate() {
        match frame.function {
            FrameKind::Lua(function) => {
                work.push(Trace::Value(Value::Function(function)));
                let closure = &vm.functions[slot_index(function.0)];
                let proto = &vm.programs[closure.program].data.prototypes[closure.prototype];
                let return_top = frames
                    .get(i + 1)
                    .filter(|f| matches!(f.function, FrameKind::Native(CONT_RETURN)))
                    .map_or(cutoff, |f| (f.vararg_start + f.argc()).min(cutoff));
                let visible_top =
                    hooks::stack_top(proto, *frame, frames.get(i + 1)).or_else(|| {
                        (i + 1 == frames.len()).then(|| {
                            let previous =
                                frame.pc().checked_sub(1).and_then(|pc| proto.code.get(pc));
                            if previous.is_some_and(|op| {
                                matches!(op.op, Op::Call | Op::Vararg | Op::Move) && op.c == OPEN
                            }) {
                                top
                            } else {
                                frame.base + proto.debug_registers()
                            }
                        })
                    });
                if let Some(visible_top) = visible_top {
                    let end = visible_top.min(registers.len()).min(return_top);
                    register_roots(registers, frame.base.min(end)..end, live, work);
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
                        if pos < registers.len().min(return_top) {
                            live[pos / 64] |= 1u64 << (pos % 64);
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
                    register_roots(registers, start.min(end)..end, live, work);
                }
            }
            FrameKind::Native(id) => {
                if id == CONT_GC && frame.wanted == FRAME_OPEN {
                    register_roots(
                        registers,
                        0..frame.result.min(registers.len()).min(cutoff),
                        live,
                        work,
                    );
                }
                if !(CONT_RAW..strings::ITERATOR_FIRST).contains(&id) {
                    work.push(Trace::Value(Value::Native(id)));
                }
                let end = frames
                    .get(i + 1)
                    .map_or(registers.len(), |f| f.stack_start)
                    .min(cutoff);
                register_roots(registers, frame.base.min(end)..end, live, work);
            }
        }
        if !matches!(frame.function, FrameKind::Native(CONT_HOOK | CONT_TRACE))
            && (!matches!(frame.function, FrameKind::Native(threads::CO_YIELD)) || frame.pc() == 0)
        {
            let start = frame.vararg_start;
            let end = (start + frame.argc()).min(registers.len()).min(cutoff);
            register_roots(registers, start.min(end)..end, live, work);
        }
    }
}
fn clear_unused_registers(registers: &mut [Value], frames: &[Frame], live: &[u64]) {
    for (index, frame) in frames.iter().enumerate() {
        if !matches!(frame.function, FrameKind::Lua(_)) {
            continue;
        }
        let end = frames
            .get(index + 1)
            .map_or(registers.len(), |next| next.stack_start)
            .min(registers.len());
        for position in frame.base.min(end)..end {
            if live[position / 64] & (1u64 << (position % 64)) == 0 {
                registers[position] = Value::Nil;
            }
        }
    }
}

impl Vm {
    pub(super) fn gc_note_control(&mut self, bytes: usize) {
        self.gc.control_allocated = self.gc.control_allocated.saturating_add(bytes);
    }
    pub(super) fn validate_value(&self, value: Value) -> Result<(), Error> {
        if self.is_live_value(value) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::Runtime,
                0,
                "invalid or collected Lua object handle",
            ))
        }
    }
    pub(super) fn is_live_value(&self, value: Value) -> bool {
        let valid = match value {
            Value::String(id) => self.strings.bytes.contains(id.0),
            Value::Table(id) => self.tables.contains(id.0),
            Value::Function(id) => self.functions.contains(id.0),
            Value::Thread(id) => self.threads.contains(id.0),
            Value::Userdata(id) => self.userdata.contains(id.0),
            Value::ParserPrototype(id) => self.parser_prototypes.contains(id.0),
            Value::Native(id) if id >= strings::ITERATOR_FIRST => self.native_closures.contains(id),
            Value::Native(id) => {
                !matches!(id, PAIRS | IPAIRS | NEW_PROXY)
                    && (matches!(id,0..=59|64..=77|80..=85|96..=104|debug::GET_INFO..=debug::GET_REGISTRY|DO_FILE|debug::CONSOLE|io::FIRST..=io::LAST|os::FIRST..=os::LAST)
                        || package::contains(id))
                    && (self.gc.collections == 0
                        || self.marked_value(value)
                        || (self.gc.phase == Phase::Propagate
                            && self.gc.natives_previous[id as usize / 64] & (1u64 << (id % 64))
                                != 0))
            }
            _ => true,
        };
        let live_during_sweep = self.gc.phase != Phase::Sweep
            || match value {
                Value::String(id) => self.strings.bytes.marked(id.0),
                _ => self.marked_value(value),
            };
        valid && live_during_sweep
    }
    /// Keep a value alive across future executions and garbage collections.
    /// Values must originate in this VM. Root handles additionally check VM ownership.
    ///
    /// # Errors
    /// Returns an error if the object handle is invalid or has been collected.
    pub fn pin(&mut self, value: Value) -> Result<Root, Error> {
        self.validate_value(value)?;
        self.gc_new_value(value);
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
        let mut payload = self
            .strings
            .bytes
            .iter()
            .map(|s| s.c_bytes().len())
            .sum::<usize>()
            + self.strings.buckets.capacity() * 4;
        payload += self.tables.iter().map(Table::heap_bytes).sum::<usize>();
        payload += self
            .functions
            .iter()
            .map(|f| f.upvalues.len() * 4)
            .sum::<usize>();
        payload += self.programs.iter().map(program_bytes).sum::<usize>();
        payload += self
            .parser_prototypes
            .iter()
            .map(loads::ParserPrototype::heap_bytes)
            .sum::<usize>();
        payload += self
            .userdata
            .iter()
            .map(|u| u.bytes.storage_bytes())
            .sum::<usize>();
        payload += self
            .native_closures
            .iter()
            .map(|f| {
                std::mem::size_of_val(f.captures.as_ref())
                    + match &f.kind {
                        threads::NativeKind::Host(f) => f.heap_bytes(),
                        _ => 0,
                    }
            })
            .sum::<usize>();
        payload += self.io.heap_bytes();
        let mut control = self.registers.capacity() * std::mem::size_of::<Value>()
            + self.frames.capacity() * std::mem::size_of::<Frame>()
            + self.open_upvalues.capacity() * 4
            + self.substitutions.capacity() * std::mem::size_of::<strings::Substitution>()
            + jobs_bytes(&self.substitutions)
            + self.sorts.capacity() * std::mem::size_of::<tables::Sort>();
        control += loads::storage_bytes(&self.loads);
        control += self.tracebacks.capacity() * std::mem::size_of::<traces::Traceback>();
        control += self.object_pointers.capacity()
            * (std::mem::size_of::<((u8, u64), Box<usize>)>() + 1)
            + self.object_pointers.len() * std::mem::size_of::<usize>();
        for (_, t) in self.threads.entries() {
            control += t.registers.capacity() * std::mem::size_of::<Value>()
                + t.frames.capacity() * std::mem::size_of::<Frame>()
                + t.open_upvalues.capacity() * 4
                + t.substitutions.capacity() * std::mem::size_of::<strings::Substitution>()
                + jobs_bytes(&t.substitutions)
                + t.sorts.capacity() * std::mem::size_of::<tables::Sort>();
            control += loads::storage_bytes(&t.loads);
            control += t.tracebacks.capacity() * std::mem::size_of::<traces::Traceback>();
            control += t.host_states.capacity() * std::mem::size_of::<host::ThreadData>()
                + t.host_states
                    .iter()
                    .map(host::ThreadData::heap_bytes)
                    .sum::<usize>();
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
        control += self.gc.finalizers.capacity() * std::mem::size_of::<UserdataId>()
            + self.gc.candidates.capacity() * std::mem::size_of::<(u64, UserdataId)>();
        let objects = self.strings.bytes.live_len() * std::mem::size_of::<ByteString>()
            + self.tables.live_len() * std::mem::size_of::<Table>()
            + self.functions.live_len() * std::mem::size_of::<Closure>()
            + self.upvalues.live_len() * std::mem::size_of::<Cell>()
            + self.threads.live_len() * std::mem::size_of::<threads::Thread>()
            + self.native_closures.live_len() * std::mem::size_of::<threads::NativeClosure>()
            + self.programs.live_len() * std::mem::size_of::<LoadedProgram>()
            + self.roots.live_len() * std::mem::size_of::<Value>();
        let objects = objects
            + self.parser_prototypes.live_len() * std::mem::size_of::<loads::ParserPrototype>();
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
        let storage = storage + self.parser_prototypes.storage_bytes();
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
            parser_prototypes: self.parser_prototypes.live_len(),
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
        self.parser_prototypes.allocated = 0;
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
            .saturating_add(self.parser_prototypes.allocated)
            .saturating_add(self.io.files.allocated)
            .saturating_add(self.gc.control_allocated)
    }
    fn mark_barrier_value(&mut self, value: Value, work: &mut Vec<Trace>) {
        let fresh = match value {
            Value::String(id) => {
                self.strings.bytes.mark(id.0);
                false
            }
            Value::Table(id) => {
                let fresh = self.tables.mark(id.0);
                if fresh {
                    self.tables[slot_index(id.0)].trace = TableTrace::Restart;
                }
                fresh
            }
            Value::Function(id) => self.functions.mark(id.0),
            Value::Thread(id) => self.threads.mark(id.0),
            Value::Userdata(id) => self.userdata.mark(id.0),
            Value::ParserPrototype(id) => self.parser_prototypes.mark(id.0),
            Value::Native(id) if id >= strings::ITERATOR_FIRST => self.native_closures.mark(id),
            Value::Native(id) if id < 256 => {
                let word = id as usize / 64;
                let bit = 1u64 << (id % 64);
                let fresh = self.gc.natives[word] & bit == 0;
                self.gc.natives[word] |= bit;
                fresh
            }
            _ => false,
        };
        if fresh {
            work.push(Trace::MarkedValue(value));
        }
    }
    pub(super) fn gc_new_value(&mut self, value: Value) {
        if self.gc.phase == Phase::Propagate {
            let mut work = std::mem::take(&mut self.gc.work);
            self.mark_barrier_value(value, &mut work);
            self.gc.work = work;
        }
    }
    pub(super) fn gc_barrier(&mut self, owner: Value, value: Value) {
        if self.gc.phase == Phase::Propagate && self.marked_value(owner) {
            self.gc_new_value(value);
        }
    }
    pub(super) fn gc_cell_barrier(&mut self, cell: usize, value: Value) {
        if self.gc.phase == Phase::Propagate
            && self
                .upvalues
                .key(cell)
                .is_some_and(|key| self.upvalues.marked(key))
        {
            self.gc_new_value(value);
        }
    }
    pub(super) fn gc_new_cell(&mut self, cell: usize) {
        if self.gc.phase == Phase::Propagate
            && let Some(key) = self.upvalues.key(cell)
            && self.upvalues.mark(key)
        {
            self.gc.work.push(Trace::CellAgain(cell));
        }
    }
    pub(super) fn gc_table_write(&mut self, id: TableId, key: Value, value: Value) {
        if self.gc.phase == Phase::Propagate && self.tables.marked(id.0) {
            let (weak_keys, weak_values) = self.table_mode(&self.tables[slot_index(id.0)]);
            let mut work = std::mem::take(&mut self.gc.work);
            if !weak_keys || matches!(key, Value::String(_)) {
                self.mark_barrier_value(key, &mut work);
            }
            if !weak_values || matches!(value, Value::String(_)) {
                self.mark_barrier_value(value, &mut work);
            }
            self.gc.work = work;
        }
    }
    pub(super) fn gc_metatable_write(&mut self, owner: Value, table: Option<TableId>) {
        if self.gc.phase != Phase::Propagate {
            return;
        }
        if let Value::Table(id) = owner {
            if self.tables.marked(id.0) {
                let table = &mut self.tables[slot_index(id.0)];
                if table.trace == TableTrace::Idle {
                    self.gc.work.push(Trace::TableSlice(id, 0));
                }
                table.trace = TableTrace::Restart;
            }
        } else if let Some(table) = table {
            if matches!(owner, Value::Userdata(_)) {
                self.gc_barrier(owner, Value::Table(table));
            } else {
                self.gc_new_value(Value::Table(table));
            }
        }
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
        let Some(mt) = table.metatable() else {
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
            Value::ParserPrototype(id) => self.parser_prototypes.marked(id.0),
            Value::Native(id) if id >= strings::ITERATOR_FIRST => self.native_closures.marked(id),
            Value::Native(id) => {
                id < 256 && self.gc.natives[id as usize / 64] & (1u64 << (id % 64)) != 0
            }
            _ => true,
        }
    }
    fn trace_thread(&mut self, id: ThreadId, work: &mut Vec<Trace>) {
        let t = &self.threads[slot_index(id.0)];
        for state in &t.host_states {
            work.extend(state.roots().iter().copied().filter_map(edge));
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
        let cutoff = if id != self.thread
            && matches!(t.status, threads::Status::Suspended | threads::Status::Dead)
        {
            t.api_stack.end.min(registers.len())
        } else if t.status == threads::Status::Suspended {
            frames
                .last()
                .filter(|f| matches!(f.function, FrameKind::Native(threads::CO_YIELD)))
                .map_or(registers.len(), |f| f.vararg_start)
        } else {
            registers.len()
        };
        let mut live = vec![0; registers.len().div_ceil(64)];
        frames_roots(self, registers, frames, top, cutoff, &mut live, work);
        if id != self.thread && t.status != threads::Status::Normal {
            let end = t.api_stack.end.min(registers.len());
            register_roots(registers, t.api_stack.start.min(end)..end, &mut live, work);
        }
        for &cell in open {
            if let Cell::Open(owner, position) = self.upvalues[cell as usize]
                && owner == id
                && position < registers.len()
            {
                live[position / 64] |= 1u64 << (position % 64);
            }
        }
        for job in subs {
            work.extend(job.roots().filter_map(edge));
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
        work.extend(
            loads
                .iter()
                .flat_map(loads::Loading::roots)
                .filter_map(edge),
        );
        // Lua 5.1 clears unused stack slots while traversing a thread. Clear
        // scalar values too: debug.getlocal can observe them after collection.
        // Native scratch windows and aliased argument/upvalue slots stay live.
        if id == self.thread {
            clear_unused_registers(&mut self.registers, &self.frames, &live);
        } else {
            let thread = &mut self.threads[slot_index(id.0)];
            clear_unused_registers(&mut thread.registers, &thread.frames, &live);
        }
        self.shrink_thread_calls(id);
    }
    fn marked_weak_value(&self, value: Value, key: bool) -> bool {
        self.marked_value(value)
            && (key
                || !matches!(value, Value::Userdata(id) if self.userdata[slot_index(id.0)].finalized))
    }
    fn trace_some(&mut self, work: &mut Vec<Trace>, budget: usize) -> usize {
        let mut visited = 0;
        while visited < budget {
            let Some(item) = work.pop() else {
                break;
            };
            visited += 1;
            let (item, forced) = match item {
                Trace::MarkedValue(value) => (Trace::Value(value), true),
                item => (item, false),
            };
            match item {
                Trace::Value(Value::String(id)) => {
                    self.strings.bytes.mark(id.0);
                }
                Trace::Value(Value::Table(id)) => {
                    if self.tables.mark(id.0) || forced {
                        self.trace_table(id, work);
                    }
                }
                Trace::Value(Value::Function(id)) => {
                    if self.functions.mark(id.0) || forced {
                        let f = &self.functions[slot_index(id.0)];
                        if let Some(&prototype) =
                            self.programs[f.program].parser_prototypes.get(f.prototype)
                        {
                            work.push(Trace::Value(Value::ParserPrototype(prototype)));
                        }
                        work.push(Trace::Value(Value::Table(f.env)));
                        work.push(Trace::Program(f.program));
                        work.extend(f.upvalues.iter().map(|&i| Trace::Cell(i as usize)));
                    }
                }
                Trace::Value(Value::Userdata(id)) => {
                    if self.userdata.mark(id.0) || forced {
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
                Trace::Value(Value::ParserPrototype(id)) => {
                    if self.parser_prototypes.mark(id.0) || forced {
                        let p = &self.parser_prototypes[slot_index(id.0)];
                        work.extend(p.roots().filter_map(edge));
                    }
                }
                Trace::Value(Value::Thread(id)) => {
                    if self.threads.mark(id.0) || forced {
                        self.trace_thread(id, work);
                    }
                }
                Trace::Value(Value::Native(id)) => {
                    if id >= strings::ITERATOR_FIRST {
                        if self.native_closures.mark(id) || forced {
                            let f = &self.native_closures[slot_index(id)];
                            work.push(Trace::Value(Value::Table(f.env)));
                            work.extend(f.captures.iter().copied().filter_map(edge));
                        }
                    } else if id < 256 {
                        let word = id as usize / 64;
                        let bit = 1u64 << (id % 64);
                        if self.gc.natives[word] & bit == 0 || forced {
                            self.gc.natives[word] |= bit;
                            work.push(Trace::Value(Value::Table(self.native_environment(id))));
                        }
                    }
                }
                Trace::ThreadAgain(id) => self.trace_thread(id, work),
                Trace::TableAgain(id) => self.trace_table(id, work),
                Trace::TableSlice(id, cursor) => self.trace_table_slice(id, cursor, work),
                Trace::CellAgain(i) => work.extend(edge(self.cell_value(i))),
                Trace::Program(i) => {
                    if let Some(key) = self.programs.key(i)
                        && self.programs.mark(key)
                    {
                        for k in &self.programs[i].constants {
                            work.extend(k.iter().copied().filter_map(edge));
                        }
                        for names in &self.programs[i].upvalue_names {
                            work.extend(
                                names.iter().map(|&name| Trace::Value(Value::String(name))),
                            );
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
        visited
    }
    fn trace_graph(&mut self, work: &mut Vec<Trace>) {
        self.trace_some(work, usize::MAX);
    }
    fn trace_table(&mut self, id: TableId, work: &mut Vec<Trace>) {
        // Fresh marks also discard scan state left by an interrupted cycle.
        self.tables[slot_index(id.0)].trace = TableTrace::Restart;
        self.trace_table_slice(id, 0, work);
    }
    fn trace_table_slice(&mut self, id: TableId, cursor: usize, work: &mut Vec<Trace>) {
        let table = &self.tables[slot_index(id.0)];
        // Read __mode once per scan. The atomic metatable refresh observes mode
        // mutations before weak cleanup, just as for an unsliced table scan.
        let (cursor, weak_keys, weak_values) = match table.trace {
            TableTrace::Pending(wk, wv) => (cursor, wk, wv),
            TableTrace::Idle | TableTrace::Restart => {
                let (wk, wv) = self.table_mode(table);
                (0, wk, wv)
            }
        };
        self.tables[slot_index(id.0)].trace = TableTrace::Pending(weak_keys, weak_values);
        let table = &self.tables[slot_index(id.0)];
        let (next, pairs) = table.trace_slice(cursor, 64);
        // Resume after children: a wide table needs only one pending slice and
        // the current slice's edges rather than a frontier for its entire width.
        if let Some(next) = next {
            work.push(Trace::TableSlice(id, next));
        }
        if cursor == 0
            && let Some(mt) = table.metatable()
        {
            work.push(Trace::Value(Value::Table(mt)));
        }
        for (key, value) in pairs {
            if !weak_keys || matches!(key, Value::String(_)) {
                work.extend(edge(key));
            }
            if !weak_values || matches!(value, Value::String(_)) {
                work.extend(edge(value));
            }
        }
        if next.is_none() {
            self.tables[slot_index(id.0)].trace = TableTrace::Idle;
        }
    }
    fn begin_gc_cycle(&mut self) {
        self.heap_stats();
        self.gc.phase = Phase::Propagate;
        self.protect_new_allocations(false);
        self.strings.bytes.clear_marks();
        self.tables.clear_marks();
        self.functions.clear_marks();
        self.upvalues.clear_marks();
        self.threads.clear_marks();
        self.native_closures.clear_marks();
        self.programs.clear_marks();
        self.parser_prototypes.clear_marks();
        self.userdata.clear_marks();
        self.io.files.clear_marks();
        if let Some(key) = self.io.stdin {
            self.io.files.mark(key.get());
        }
        self.strings.bytes.protect_new(true);
        self.io.files.protect_new(true);
        self.gc.natives_previous = self.gc.natives;
        self.gc.natives.fill(0);
        let mut work = std::mem::take(&mut self.gc.work);
        work.clear();
        self.seed_gc_roots(&mut work);
        self.gc.work = work;
    }
    fn seed_gc_roots(&self, work: &mut Vec<Trace>) {
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
        if self.standalone != 0 {
            work.push(Trace::Value(Value::Native(self.standalone)));
        }
        // A live host Program retains its mutable per-VM entry binding. Internal
        // code ownership by escaped Lua closures does not pin unused entries.
        work.extend(
            self.programs
                .iter()
                .filter(|p| p.owner.strong_count() != 0)
                .map(|p| Trace::Value(Value::Function(p.entry))),
        );
    }
    fn finish_gc_marking(&mut self) {
        let mut work = std::mem::take(&mut self.gc.work);
        self.trace_graph(&mut work);
        // Snapshot the already reached containers before refreshing roots;
        // newly reached threads/cells still receive their ordinary first scan.
        for index in 0..self.threads.len() {
            if let Some(key) = self.threads.key(index)
                && self.threads.marked(key)
            {
                work.push(Trace::ThreadAgain(ThreadId(key)));
            }
        }
        for (index, table) in self.tables.entries() {
            if table.metatable().is_some()
                && self
                    .tables
                    .key(index)
                    .is_some_and(|key| self.tables.marked(key))
            {
                work.push(Trace::TableAgain(TableId(self.tables.key(index).unwrap())));
            }
        }
        for (index, cell) in self.upvalues.entries() {
            if matches!(cell, Cell::Open(..))
                && self
                    .upvalues
                    .key(index)
                    .is_some_and(|key| self.upvalues.marked(key))
            {
                work.push(Trace::CellAgain(index));
            }
        }
        self.seed_gc_roots(&mut work);
        self.trace_graph(&mut work);
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
        self.parser_prototypes.protect_new(enabled);
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
            9 => self.parser_prototypes.len(),
            _ => 0,
        }
    }
    fn sweep_gc_some(&mut self, mut budget: usize) {
        while budget != 0 && self.gc.sweep_arena < 10 {
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
                9 => self.parser_prototypes.sweep_some(&mut cursor, budget),
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
        if self.gc.sweep_arena != 10 {
            return;
        }
        self.strings.rebuild();
        self.prune_object_pointers();
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
    fn advance_gc_some(&mut self, mut budget: usize) -> bool {
        if self.gc.phase == Phase::Pause {
            self.begin_gc_cycle();
        }
        if self.gc.phase == Phase::Propagate {
            let mut work = std::mem::take(&mut self.gc.work);
            budget -= self.trace_some(&mut work, budget);
            self.gc.work = work;
            if self.gc.work.is_empty() {
                self.finish_gc_marking();
            }
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
    pub(super) fn prepare_gc_finalizers(&mut self) {
        if self.gc.phase == Phase::Propagate {
            self.finish_gc_marking();
        }
        self.finish_pending_sweep();
    }
    pub(super) fn collect_cycle(&mut self) -> HeapStats {
        self.finish_pending_sweep();
        self.begin_gc_cycle();
        self.finish_gc_marking();
        self.sweep_gc_some(usize::MAX);
        self.heap_stats()
    }
    pub(super) fn gc_control(&mut self, operation: GcOperation) -> (i32, bool) {
        let mut dispatch = false;
        let result = match operation {
            GcOperation::Stop => {
                self.gc.stopped = true;
                0
            }
            GcOperation::Restart => {
                self.gc.stopped = false;
                self.defer_gc_until_allocation();
                0
            }
            GcOperation::Collect => {
                self.collect_cycle();
                self.finish_gc_cycle();
                dispatch = true;
                0
            }
            GcOperation::CountKilobytes => (self.heap_stats().live_bytes >> 10) as i32,
            GcOperation::CountRemainder => (self.heap_stats().live_bytes & 1023) as i32,
            GcOperation::SetPause(value) => std::mem::replace(&mut self.gc.pause, value),
            GcOperation::SetStepMultiplier(value) => std::mem::replace(&mut self.gc.stepmul, value),
            GcOperation::Step(extra) => {
                self.gc.stopped = false;
                let complete = self.advance_gc_some(self.gc_step_budget(extra));
                if complete {
                    self.finish_gc_cycle();
                    dispatch = true;
                } else {
                    self.defer_gc_until_allocation();
                }
                i32::from(complete)
            }
        };
        (result, dispatch)
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
        if choice == 3 {
            self.registers[base] = Value::Number(self.heap_stats().live_bytes as f64 / 1024.0);
            return Ok(false);
        }
        let operation = match choice {
            0 => GcOperation::Stop,
            1 => GcOperation::Restart,
            2 => GcOperation::Collect,
            4 => GcOperation::Step(extra),
            5 => GcOperation::SetPause(extra),
            6 => GcOperation::SetStepMultiplier(extra),
            _ => unreachable!("validated collector option"),
        };
        let (result, dispatch) = self.gc_control(operation);
        let value = if choice == 4 {
            Value::Boolean(result != 0)
        } else {
            Value::Number(f64::from(result))
        };
        self.registers[base] = value;
        Ok(dispatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_capture_writes_barrier_already_propagated_builtin_and_host_closures() {
        fn empty(_: &mut HostCall<'_>) -> Result<(), Error> {
            Ok(())
        }
        for source in [
            b"return pairs".as_slice(),
            b"return ipairs",
            b"return newproxy",
            b"return string.gmatch('abc','.')",
            b"return coroutine.wrap(function()end)",
            b"return io.tmpfile():lines()",
            b"return host",
        ] {
            let mut vm = Vm::new();
            let host = vm.new_host_function(empty, &[Value::Nil]).unwrap();
            vm.set_global(b"host", host).unwrap();
            let function = vm.execute(&crate::compile(source).unwrap()).unwrap()[0];
            let root = vm.pin(function).unwrap();
            let replacement = Value::Table(vm.new_table());
            vm.begin_gc_cycle();
            let mut work = vec![Trace::Value(function)];
            assert_eq!(vm.trace_some(&mut work, 1), 1);
            assert!(vm.marked_value(function) && !vm.marked_value(replacement));
            vm.gc.work.extend(work);
            let Value::Native(id) = function else {
                panic!("native closure")
            };
            vm.set_native_capture(id, 0, replacement).unwrap();
            assert!(
                vm.marked_value(replacement),
                "new capture must mark an old white graph"
            );
            finish_marked_cycle(&mut vm);
            assert!(vm.validate_value(replacement).is_ok());
            vm.unpin(root).unwrap();
        }
    }

    #[test]
    fn retained_programs_trace_interned_upvalue_names_after_the_compiler_is_dropped() {
        let mut vm = Vm::new();
        let function = vm.execute(&crate::compile(b"local unique_upvalue_name_123456789=17;return function()return unique_upvalue_name_123456789 end").unwrap()).unwrap()[0];
        let root = vm.pin(function).unwrap();
        let Value::Function(id) = function else {
            panic!("Lua closure")
        };
        let program = vm.functions[slot_index(id.0)].program;
        let names: Vec<_> = vm.programs[program]
            .upvalue_names
            .iter()
            .flatten()
            .copied()
            .collect();
        assert_eq!(names.len(), 1);
        vm.begin_gc_cycle();
        let mut work = vec![Trace::Value(function)];
        while !work.is_empty() {
            vm.trace_some(&mut work, 1);
        }
        assert!(names.iter().all(|name| vm.strings.bytes.marked(name.0)));
        finish_marked_cycle(&mut vm);
        assert!(
            names
                .iter()
                .all(|name| vm.validate_value(Value::String(*name)).is_ok())
        );
        vm.unpin(root).unwrap();
    }

    #[test]
    fn newly_initialized_host_thread_roots_barrier_an_already_marked_thread() {
        struct State(Value);
        impl HostThreadState for State {
            fn roots(&self) -> &[Value] {
                std::slice::from_ref(&self.0)
            }
        }
        let mut vm = Vm::new();
        let table = vm.new_table();
        vm.begin_gc_cycle();
        while !vm.threads.marked(vm.thread.0) {
            let mut work = std::mem::take(&mut vm.gc.work);
            assert_eq!(vm.trace_some(&mut work, 1), 1);
            vm.gc.work = work;
        }
        assert!(!vm.tables.marked(table.0));
        vm.initialize_thread_state(State(Value::Table(table)))
            .unwrap();
        assert!(
            vm.tables.marked(table.0),
            "new host-state edges need an immediate barrier"
        );
        finish_marked_cycle(&mut vm);
        assert!(vm.validate_value(Value::Table(table)).is_ok());
    }

    fn finish_marked_cycle(vm: &mut Vm) {
        vm.finish_gc_marking();
        vm.sweep_gc_some(usize::MAX);
        vm.finish_gc_cycle();
    }

    fn value_table(value: Value) -> TableId {
        let Value::Table(id) = value else {
            panic!("table expected")
        };
        id
    }

    fn trace_until(vm: &mut Vm, mut ready: impl FnMut(&Vm) -> bool) {
        for _ in 0..100000 {
            if ready(vm) {
                return;
            }
            let mut work = std::mem::take(&mut vm.gc.work);
            assert_eq!(vm.trace_some(&mut work, 1), 1);
            vm.gc.work = work;
            assert_eq!(vm.gc.phase, Phase::Propagate);
        }
        panic!("mark frontier did not reach the requested objects");
    }

    #[test]
    fn wide_table_frontiers_stay_bounded_and_preserve_every_child() {
        for shape in 0..3 {
            let mut vm = Vm::new();
            let table = vm.new_table();
            let root = vm.pin(Value::Table(table)).unwrap();
            let mut children = Vec::new();
            for i in 1..=4096 {
                let value = Value::Table(vm.new_table());
                let key = if shape == 1 || (shape == 2 && i % 2 == 0) {
                    let key = Value::Table(vm.new_table());
                    children.push(key);
                    key
                } else {
                    Value::Number(f64::from(if shape == 2 { (i + 1) / 2 } else { i }))
                };
                vm.raw_set(table, key, value).unwrap();
                children.push(value);
            }
            vm.begin_gc_cycle();
            let mut work = vec![Trace::Value(Value::Table(table))];
            let mut peak = 0;
            while !work.is_empty() {
                assert_eq!(vm.trace_some(&mut work, 1), 1);
                peak = peak.max(work.len());
            }
            assert!(peak <= 129, "wide table frontier grew to {peak}");
            assert!(children.iter().all(|&value| vm.marked_value(value)));
            finish_marked_cycle(&mut vm);
            assert!(
                children
                    .iter()
                    .all(|&value| vm.validate_value(value).is_ok())
            );
            vm.unpin(root).unwrap();
        }
    }

    #[test]
    fn sparse_table_slices_count_holes_and_tombstones() {
        for hash in [false, true] {
            let mut vm = Vm::new();
            let table = vm.new_table();
            let root = vm.pin(Value::Table(table)).unwrap();
            let child = Value::Table(vm.new_table());
            for i in 1..=16384 {
                let key = Value::Number(if hash { -f64::from(i) } else { f64::from(i) });
                vm.raw_set(table, key, child).unwrap();
            }
            for i in 1..16384 {
                let key = Value::Number(if hash { -f64::from(i) } else { f64::from(i) });
                vm.raw_set(table, key, Value::Nil).unwrap();
            }
            vm.begin_gc_cycle();
            let mut work = vec![Trace::Value(Value::Table(table))];
            for _ in 0..200 {
                assert_eq!(vm.trace_some(&mut work, 1), 1);
                assert_eq!(work.len(), 1);
                assert!(!vm.marked_value(child));
            }
            vm.trace_graph(&mut work);
            assert!(vm.marked_value(child));
            finish_marked_cycle(&mut vm);
            assert!(vm.validate_value(child).is_ok());
            vm.unpin(root).unwrap();
        }
    }

    #[test]
    fn hash_compaction_restarts_an_active_scan_before_shifted_entries() {
        let mut vm = Vm::new();
        let table = vm.new_record_table(128);
        let root = vm.pin(Value::Table(table)).unwrap();
        let mut children = Vec::new();
        for i in 1..=191 {
            let child = Value::Table(vm.new_table());
            vm.raw_set(table, Value::Number(-f64::from(i)), child)
                .unwrap();
            children.push(child);
        }
        for i in 1..=128 {
            vm.raw_set(table, Value::Number(-f64::from(i)), Value::Nil)
                .unwrap();
        }
        vm.begin_gc_cycle();
        let mut work = vec![Trace::Value(Value::Table(table))];
        assert_eq!(vm.trace_some(&mut work, 1), 1);
        assert!(matches!(work.as_slice(), [Trace::TableSlice(_, 129)]));
        vm.gc.work.extend(work);
        vm.raw_set(table, Value::Number(-1000.0), Value::Boolean(true))
            .unwrap();
        assert_eq!(vm.tables[slot_index(table.0)].trace, TableTrace::Restart);
        finish_marked_cycle(&mut vm);
        assert!(
            children[..128]
                .iter()
                .all(|&v| vm.validate_value(v).is_err())
        );
        assert!(
            children[128..]
                .iter()
                .all(|&v| vm.validate_value(v).is_ok())
        );
        vm.unpin(root).unwrap();
    }

    #[test]
    fn hash_to_array_moves_restart_an_active_hash_scan() {
        let mut vm = Vm::new();
        let table = vm.new_table();
        let root = vm.pin(Value::Table(table)).unwrap();
        let mut children = Vec::new();
        for i in 2..=256 {
            let child = Value::Table(vm.new_table());
            vm.raw_set(table, Value::Number(f64::from(i)), child)
                .unwrap();
            children.push(child);
        }
        vm.begin_gc_cycle();
        let mut work = vec![Trace::Value(Value::Table(table))];
        assert_eq!(vm.trace_some(&mut work, 2), 2);
        assert!(children[64..].iter().all(|&v| !vm.marked_value(v)));
        vm.gc.work.extend(work);
        vm.raw_set(table, Value::Number(1.0), Value::Boolean(true))
            .unwrap();
        assert_eq!(vm.tables[slot_index(table.0)].trace, TableTrace::Restart);
        finish_marked_cycle(&mut vm);
        assert!(children.iter().all(|&v| vm.validate_value(v).is_ok()));
        vm.unpin(root).unwrap();
    }

    #[test]
    fn metatable_writes_keep_one_pending_table_scan() {
        let mut vm = Vm::new();
        vm.begin_gc_cycle();
        let table = vm.new_table();
        let root = vm.pin(Value::Table(table)).unwrap();
        // A freshly allocated table is marked but its MarkedValue is still queued.
        let before = vm.gc.work.len();
        vm.set_metatable(Value::Table(table), Some(vm.registry));
        vm.set_metatable(Value::Table(table), None);
        assert_eq!(vm.gc.work.len(), before);
        finish_marked_cycle(&mut vm);
        vm.begin_gc_cycle();
        trace_until(&mut vm, |vm| {
            vm.tables.marked(table.0) && vm.tables[slot_index(table.0)].trace == TableTrace::Idle
        });
        let before = vm.gc.work.len();
        vm.set_metatable(Value::Table(table), Some(vm.registry));
        vm.set_metatable(Value::Table(table), None);
        assert_eq!(vm.gc.work.len(), before + 1);
        finish_marked_cycle(&mut vm);
        vm.unpin(root).unwrap();
    }

    #[test]
    fn partial_weak_scans_refresh_changed_modes_and_removed_metatables() {
        for remove in [false, true] {
            let mut vm = Vm::new();
            let table = vm.new_table();
            let mt = vm.new_table();
            let mode = vm.intern(b"__mode");
            let weak = vm.intern(b"v");
            let strong = vm.intern(b"");
            vm.raw_set(mt, mode, weak).unwrap();
            vm.set_metatable(Value::Table(table), Some(mt));
            let root = vm.pin(Value::Table(table)).unwrap();
            let mut children = Vec::new();
            for i in 1..=1024 {
                let child = Value::Table(vm.new_table());
                vm.raw_set(table, Value::Number(f64::from(i)), child)
                    .unwrap();
                children.push(child);
            }
            vm.begin_gc_cycle();
            let mut work = vec![Trace::Value(Value::Table(table))];
            assert_eq!(vm.trace_some(&mut work, 1), 1);
            assert!(children.iter().all(|&v| !vm.marked_value(v)));
            vm.gc.work.extend(work);
            if remove {
                vm.set_metatable(Value::Table(table), None);
            } else {
                vm.raw_set(mt, mode, strong).unwrap();
            }
            finish_marked_cycle(&mut vm);
            assert!(children.iter().all(|&v| vm.validate_value(v).is_ok()));
            vm.unpin(root).unwrap();
        }
    }

    #[test]
    fn restarting_a_cycle_discards_stale_table_cursors() {
        let mut vm = Vm::new();
        let table = vm.new_table();
        let root = vm.pin(Value::Table(table)).unwrap();
        let mut children = Vec::new();
        for i in 1..=1000 {
            let child = Value::Table(vm.new_table());
            vm.raw_set(table, Value::Number(f64::from(i)), child)
                .unwrap();
            children.push(child);
        }
        vm.begin_gc_cycle();
        trace_until(&mut vm, |vm| vm.tables.marked(table.0));
        assert_eq!(
            vm.tables[slot_index(table.0)].trace,
            TableTrace::Pending(false, false)
        );
        vm.collect_garbage().unwrap();
        assert_eq!(vm.tables[slot_index(table.0)].trace, TableTrace::Idle);
        assert!(children.iter().all(|&v| vm.validate_value(v).is_ok()));
        vm.unpin(root).unwrap();
    }

    #[test]
    fn marking_budget_and_table_barriers_preserve_white_keys_and_values() {
        let mut vm = Vm::new();
        vm.execute(&crate::compile(b"collectgarbage('stop');holder={};weak=setmetatable({{value=77},{value=88},{value=99}},{__mode='v'});anchor={};for i=1,1000 do anchor[i]={i}end").unwrap()).unwrap();
        let holder = value_table(vm.global(b"holder"));
        let weak = value_table(vm.global(b"weak"));
        let values: Vec<_> = (1..=3)
            .map(|i| vm.tables[slot_index(weak.0)].get(Value::Number(f64::from(i))))
            .collect();
        vm.begin_gc_cycle();
        trace_until(&mut vm, |vm| {
            vm.tables.marked(holder.0) && vm.tables.marked(weak.0)
        });
        assert!(values.iter().all(|&v| !vm.marked_value(v)));
        let key = vm.intern(b"saved");
        vm.raw_set(holder, key, values[0]).unwrap();
        vm.raw_set(holder, values[1], Value::Number(12.0)).unwrap();
        vm.raw_set(weak, Value::Number(4.0), values[2]).unwrap();
        assert!(vm.marked_value(values[0]) && vm.marked_value(values[1]));
        assert!(!vm.marked_value(values[2]));
        finish_marked_cycle(&mut vm);
        assert_eq!(vm.tables[slot_index(holder.0)].get(key), values[0]);
        assert_eq!(
            vm.tables[slot_index(holder.0)].get(values[1]),
            Value::Number(12.0)
        );
        assert!(vm.validate_value(values[0]).is_ok() && vm.validate_value(values[1]).is_ok());
        assert!(vm.validate_value(values[2]).is_err());
    }

    #[test]
    fn metatable_changes_and_weak_mode_upgrades_revisit_strong_edges() {
        let mut vm = Vm::new();
        vm.execute(&crate::compile(b"collectgarbage('stop');mt={__mode='v'};a=setmetatable({{value=17}},mt);b=setmetatable({{value=19}},{__mode='v'})").unwrap()).unwrap();
        let a = value_table(vm.global(b"a"));
        let b = value_table(vm.global(b"b"));
        let mt = value_table(vm.global(b"mt"));
        let va = vm.tables[slot_index(a.0)].get(Value::Number(1.0));
        let vb = vm.tables[slot_index(b.0)].get(Value::Number(1.0));
        vm.begin_gc_cycle();
        trace_until(&mut vm, |vm| {
            vm.tables.marked(a.0) && vm.tables.marked(b.0) && vm.tables.marked(mt.0)
        });
        assert!(!vm.marked_value(va) && !vm.marked_value(vb));
        let mode = vm.intern(b"__mode");
        let strong = vm.intern(b"");
        vm.raw_set(mt, mode, strong).unwrap();
        vm.set_metatable(Value::Table(b), None);
        finish_marked_cycle(&mut vm);
        assert!(vm.validate_value(va).is_ok() && vm.validate_value(vb).is_ok());
        assert_eq!(vm.tables[slot_index(a.0)].get(Value::Number(1.0)), va);
        assert_eq!(vm.tables[slot_index(b.0)].get(Value::Number(1.0)), vb);
    }

    #[test]
    fn environment_and_userdata_barriers_retain_old_white_graphs() {
        let mut vm = Vm::new();
        vm.execute(&crate::compile(b"collectgarbage('stop');f=function()return field end;u=newproxy();n=string.gmatch('x','.');weak=setmetatable({{field=17},{mark=18},{mark=19},{__index={flag=99}}},{__mode='v'})").unwrap()).unwrap();
        let f = vm.global(b"f");
        let u = vm.global(b"u");
        let n = vm.global(b"n");
        let weak = value_table(vm.global(b"weak"));
        let values: Vec<_> = (1..=4)
            .map(|i| vm.tables[slot_index(weak.0)].get(Value::Number(f64::from(i))))
            .collect();
        vm.begin_gc_cycle();
        trace_until(&mut vm, |vm| {
            vm.marked_value(f) && vm.marked_value(u) && vm.marked_value(n)
        });
        assert!(values.iter().all(|&v| !vm.marked_value(v)));
        vm.execute(&crate::compile(b"debug.setfenv(f,weak[1]);debug.setfenv(u,weak[2]);debug.setfenv(n,weak[3]);debug.setmetatable(u,weak[4])").unwrap()).unwrap();
        assert!(values.iter().all(|&v| vm.marked_value(v)));
        finish_marked_cycle(&mut vm);
        assert_eq!(
            vm.execute(
                &crate::compile(b"return f(),debug.getfenv(u).mark,debug.getfenv(n).mark,u.flag")
                    .unwrap()
            )
            .unwrap(),
            &[
                Value::Number(17.0),
                Value::Number(18.0),
                Value::Number(19.0),
                Value::Number(99.0)
            ]
        );
    }

    #[test]
    fn atomic_open_cell_refresh_closes_an_unreachable_suspended_thread() {
        let mut vm = Vm::new();
        vm.execute(&crate::compile(b"collectgarbage('stop');weak=setmetatable({{value=8}},{__mode='v'});local co=coroutine.create(function()local y={value=2};get=function()return y end;coroutine.yield()end);coroutine.resume(co)").unwrap()).unwrap();
        let Value::Function(get) = vm.global(b"get") else {
            panic!("closure expected")
        };
        let cell = vm.functions[slot_index(get.0)].upvalues[0] as usize;
        let Cell::Open(thread, register) = vm.upvalues[cell] else {
            panic!("open cell expected")
        };
        let weak = value_table(vm.global(b"weak"));
        let value = vm.tables[slot_index(weak.0)].get(Value::Number(1.0));
        vm.begin_gc_cycle();
        trace_until(&mut vm, |vm| {
            vm.upvalues
                .key(cell)
                .is_some_and(|key| vm.upvalues.marked(key))
        });
        assert!(!vm.threads.marked(thread.0) && !vm.marked_value(value));
        vm.threads[slot_index(thread.0)].registers[register] = value;
        finish_marked_cycle(&mut vm);
        assert!(!vm.threads.contains(thread.0));
        assert!(matches!(vm.upvalues[cell],Cell::Closed(v)if v==value));
        assert_eq!(
            vm.execute(&crate::compile(b"return get().value").unwrap())
                .unwrap(),
            &[Value::Number(8.0)]
        );
    }

    #[test]
    fn partial_marking_keeps_valid_native_handles_and_rejects_retired_ones() {
        let mut vm = Vm::new();
        let retired = vm.global(b"assert");
        vm.execute(&crate::compile(b"debug.setfenv(assert,{});assert=nil").unwrap())
            .unwrap();
        vm.collect_garbage().unwrap();
        let live = vm
            .execute(&crate::compile(b"return os.date").unwrap())
            .unwrap()[0];
        let metatable = vm.new_table();
        vm.begin_gc_cycle();
        assert!(!vm.marked_value(live));
        vm.set_metatable(live, Some(metatable));
        assert!(vm.tables.marked(metatable.0));
        assert!(vm.pin(retired).is_err());
        let root = vm.pin(live).unwrap();
        finish_marked_cycle(&mut vm);
        assert_eq!(vm.rooted(root).unwrap(), live);
        assert!(vm.tables.contains(metatable.0));
        vm.unpin(root).unwrap();
    }

    #[test]
    fn closed_cell_writes_and_closing_coroutines_mark_new_references() {
        for coroutine in [false, true] {
            let mut vm = Vm::new();
            let setup = if coroutine {
                "collectgarbage('stop');weak=setmetatable({{value=7}},{__mode='v'});co=coroutine.create(function()local x={value=1};get=function()return x end;coroutine.yield();x=weak[1]end);coroutine.resume(co)"
            } else {
                "collectgarbage('stop');weak=setmetatable({{value=7}},{__mode='v'});local x={value=1};get=function()return x end;put=function(v)x=v end"
            };
            vm.execute(&crate::compile(setup.as_bytes()).unwrap())
                .unwrap();
            let Value::Function(get) = vm.global(b"get") else {
                panic!("closure expected")
            };
            let cell = vm.functions[slot_index(get.0)].upvalues[0] as usize;
            let weak = value_table(vm.global(b"weak"));
            let child = vm.tables[slot_index(weak.0)].get(Value::Number(1.0));
            vm.begin_gc_cycle();
            trace_until(&mut vm, |vm| {
                vm.upvalues
                    .key(cell)
                    .is_some_and(|key| vm.upvalues.marked(key))
            });
            assert!(!vm.marked_value(child));
            let body = if coroutine {
                b"coroutine.resume(co);weak[1]=nil".as_slice()
            } else {
                b"put(weak[1]);weak[1]=nil"
            };
            vm.execute(&crate::compile(body).unwrap()).unwrap();
            assert!(matches!(vm.upvalues[cell],Cell::Closed(v)if v==child));
            assert!(vm.marked_value(child));
            finish_marked_cycle(&mut vm);
            assert_eq!(
                vm.execute(&crate::compile(b"return get().value").unwrap())
                    .unwrap(),
                &[Value::Number(7.0)]
            );
        }
    }

    #[test]
    fn full_collection_restarts_a_partial_mark_and_reclaims_floating_garbage() {
        let mut vm = Vm::new();
        vm.execute(
            &crate::compile(
                b"collectgarbage('stop');holder={};weak=setmetatable({{}},{__mode='v'})",
            )
            .unwrap(),
        )
        .unwrap();
        let holder = value_table(vm.global(b"holder"));
        let weak = value_table(vm.global(b"weak"));
        let child = vm.tables[slot_index(weak.0)].get(Value::Number(1.0));
        vm.begin_gc_cycle();
        trace_until(&mut vm, |vm| vm.tables.marked(holder.0));
        let key = vm.intern(b"temporary");
        vm.raw_set(holder, key, child).unwrap();
        vm.raw_set(holder, key, Value::Nil).unwrap();
        assert!(vm.marked_value(child));
        vm.collect_garbage().unwrap();
        assert!(vm.validate_value(child).is_err());
        assert_eq!(
            vm.tables[slot_index(weak.0)].get(Value::Number(1.0)),
            Value::Nil
        );
    }

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
        vm.finish_gc_marking();
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
        vm.finish_gc_marking();
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
