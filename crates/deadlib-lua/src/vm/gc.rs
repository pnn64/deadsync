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
}
fn edge(value: Value) -> Option<Trace> {
    match value {
        Value::Nil | Value::Boolean(_) | Value::Number(_) => None,
        _ => Some(Trace::Value(value)),
    }
}
#[derive(Debug)]
pub(super) struct Collector {
    stopped: bool,
    pause: i32,
    stepmul: i32,
    last_live: usize,
    threshold: usize,
    credit: usize,
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
            stopped: false,
            pause: 200,
            stepmul: 200,
            last_live: 0,
            threshold: 64 * 1024,
            credit: 0,
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

impl Strings {
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
fn sweep<T>(arena: &mut Arena<T>) -> u64 {
    let mut count = 0;
    for i in (0..arena.len()).rev() {
        if let Some(key) = arena.key(i)
            && !arena.marked(key)
        {
            arena.remove(key);
            count += 1;
        }
    }
    count
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
        if !matches!(frame.function, FrameKind::Native(threads::CO_YIELD)) || frame.pc() == 0 {
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
                matches!(id,0..=29|32..=59|64..=77|80..=85|96..=104)
                    && (self.gc.collections == 0 || self.marked_value(value))
            }
            _ => true,
        };
        if valid {
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
        let mut control = self.registers.capacity() * std::mem::size_of::<Value>()
            + self.frames.capacity() * std::mem::size_of::<Frame>()
            + self.open_upvalues.capacity() * 4
            + self.substitutions.capacity() * std::mem::size_of::<strings::Substitution>()
            + jobs_bytes(&self.substitutions)
            + self.sorts.capacity() * std::mem::size_of::<tables::Sort>();
        for (_, t) in self.threads.entries() {
            control += t.registers.capacity() * std::mem::size_of::<Value>()
                + t.frames.capacity() * std::mem::size_of::<Frame>()
                + t.open_upvalues.capacity() * 4
                + t.substitutions.capacity() * std::mem::size_of::<strings::Substitution>()
                + jobs_bytes(&t.substitutions)
                + t.sorts.capacity() * std::mem::size_of::<tables::Sort>();
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
        let objects =
            objects + self.userdata.live_len() * std::mem::size_of::<userdata::Userdata>();
        let storage = self.strings.bytes.storage_bytes()
            + self.tables.storage_bytes()
            + self.functions.storage_bytes()
            + self.upvalues.storage_bytes()
            + self.threads.storage_bytes()
            + self.native_closures.storage_bytes()
            + self.programs.storage_bytes()
            + self.roots.storage_bytes();
        let storage = storage + self.userdata.storage_bytes();
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
        self.gc.control_allocated = 0;
        self.gc.pending = false;
        self.gc.last_live = self.heap_stats().live_bytes;
        self.gc.threshold = self
            .gc
            .last_live
            .saturating_mul(self.gc.pause.max(1) as usize)
            .saturating_div(100)
            .max(64 * 1024);
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
            .saturating_add(self.gc.control_allocated)
    }
    pub(super) fn maybe_gc(&mut self) {
        self.gc.pending = false;
        if !self.gc.stopped
            && self.gc.running == 0
            && !self.gc.closing
            && self.gc.last_live.saturating_add(self.gc_debt()) >= self.gc.threshold
        {
            self.collect_cycle();
            if !self.gc.finalizers.is_empty() {
                self.push_finalizers(false);
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
    fn trace_thread(&self, id: ThreadId, work: &mut Vec<Trace>) {
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
                _ => {}
            }
        }
    }
    pub(super) fn collect_cycle(&mut self) -> HeapStats {
        self.heap_stats();
        self.strings.bytes.clear_marks();
        self.tables.clear_marks();
        self.functions.clear_marks();
        self.upvalues.clear_marks();
        self.threads.clear_marks();
        self.native_closures.clear_marks();
        self.programs.clear_marks();
        self.userdata.clear_marks();
        self.gc.natives.fill(0);
        let mut work = std::mem::take(&mut self.gc.work);
        work.clear();
        work.push(Trace::Value(Value::Thread(self.main)));
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
        // A live host Program retains its mutable per-VM entry binding. Internal
        // code ownership by escaped Lua closures does not pin unused entries.
        work.extend(
            self.programs
                .iter()
                .filter(|p| p.owner.strong_count() != 0)
                .map(|p| Trace::Value(Value::Function(p.entry))),
        );
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
        let reclaimed = sweep(&mut self.tables)
            + sweep(&mut self.functions)
            + sweep(&mut self.upvalues)
            + sweep(&mut self.threads)
            + sweep(&mut self.native_closures)
            + sweep(&mut self.programs)
            + sweep(&mut self.strings.bytes);
        let reclaimed = reclaimed + sweep(&mut self.userdata);
        self.proxy_metatables
            .retain(|id| self.tables.contains(id.0));
        self.strings.rebuild();
        self.patterns
            .retain(|(id, _), _| self.strings.bytes.contains(id.0));
        self.native_envs.retain(|id, _| {
            *id < 256 && self.gc.natives[*id as usize / 64] & (1u64 << (*id % 64)) != 0
        });
        self.gc.work = work;
        self.gc.collections += 1;
        self.gc.reclaimed += reclaimed;
        self.gc.credit = 0;
        self.reset_gc_debt();
        self.heap_stats()
    }
    pub(super) fn gc_native(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        if id == GC_INFO {
            self.registers[base] = Value::Number((self.heap_stats().live_bytes / 1024) as f64);
            return Ok(1);
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
            .ok_or_else(|| {
                Error::new(ErrorKind::Runtime, 0, "invalid option to 'collectgarbage'")
            })?;
        let extra = self.integer_argument(base, count, 1, Some(0))? as i32;
        let option = choices[choice];
        let value = match option {
            b"stop" => {
                self.gc.stopped = true;
                Value::Number(0.0)
            }
            b"restart" => {
                self.gc.stopped = false;
                Value::Number(0.0)
            }
            b"collect" => {
                self.collect_cycle();
                Value::Number(0.0)
            }
            b"count" => Value::Number(self.heap_stats().live_bytes as f64 / 1024.0),
            b"setpause" => {
                let old = self.gc.pause;
                self.gc.pause = extra;
                self.gc.threshold = self.gc.last_live.saturating_mul(extra.max(1) as usize) / 100;
                Value::Number(f64::from(old))
            }
            b"setstepmul" => {
                let old = self.gc.stepmul;
                self.gc.stepmul = extra;
                Value::Number(f64::from(old))
            }
            b"step" => {
                let bytes = (extra.max(1) as usize)
                    .saturating_mul(1024)
                    .saturating_mul(self.gc.stepmul.max(1) as usize)
                    / 100;
                self.gc.credit = self.gc.credit.saturating_add(bytes.max(1));
                let complete = self.gc.credit >= self.heap_stats().live_bytes;
                if complete {
                    self.collect_cycle();
                }
                Value::Boolean(complete)
            }
            _ => unreachable!("validated collector option"),
        };
        self.registers[base] = value;
        Ok(1)
    }
}
