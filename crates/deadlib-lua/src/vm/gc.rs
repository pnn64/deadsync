//! Iterative tracing, weak tables, host roots and reusable object generations.
use super::*;
use crate::table::TableTrace;
use std::sync::atomic::{AtomicU64, Ordering};
mod finalizers;
mod parked;
mod storage;
mod work;
use work::Work;
#[cfg(test)]
mod allocation_tests;

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

fn pause_threshold(estimate: usize, pause: i32) -> usize {
    // setthreshold multiplies lu_mem by the signed setting after dividing the
    // estimate into whole hundreds. The usual unsigned conversion can wrap.
    (estimate / 100).wrapping_mul(pause as usize)
}

fn step_limit(multiplier: i32) -> isize {
    // GCSTEPSIZE is an unsigned int. Its product wraps at 32 bits before Lua
    // assigns it to l_mem, whose signed width follows the target pointer size.
    ((1024u32 / 100).wrapping_mul(multiplier as u32) as usize) as isize
}

#[cfg(test)]
mod integer_policy_tests {
    use super::*;

    #[test]
    fn pause_thresholds_divide_first_and_wrap_at_the_pointer_width() {
        assert_eq!(pause_threshold(99, i32::MAX), 0);
        assert_eq!(pause_threshold(199, 1), 1);
        assert_eq!(pause_threshold(10_099, 200), 20_000);
        assert_eq!(pause_threshold(10_099, 0), 0);
        assert_eq!(pause_threshold(10_099, -1), usize::MAX - 99);
        assert_eq!(pause_threshold(10_099, -100), usize::MAX - 9_999);
        #[cfg(target_pointer_width = "32")]
        assert_eq!(pause_threshold(400, 1 << 30), 0);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(pause_threshold(400, 1 << 30), 4_294_967_296);
    }

    #[test]
    fn step_limits_wrap_the_unsigned_product_before_the_signed_assignment() {
        for (multiplier, wide, narrow) in [
            (0, 0i64, 0i64),
            (200, 2_000, 2_000),
            (-1, 4_294_967_286, -10),
            (-429_496_729, 6, 6),
            (429_496_729, 4_294_967_290, -6),
            (i32::MIN, 0, 0),
            (i32::MAX, 4_294_967_286, -10),
            (-2_000_000_000, 1_474_836_480, 1_474_836_480),
            (2_000_000_000, 2_820_130_816, -1_474_836_480),
        ] {
            let expected = if cfg!(target_pointer_width = "64") {
                wide
            } else {
                narrow
            };
            assert_eq!(step_limit(multiplier) as i64, expected);
        }
        let mut vm = Vm::new();
        vm.gc.stepmul = i32::MIN;
        assert_eq!(vm.gc_step_budget(0), usize::MAX);
        vm.gc.stepmul = -2_000_000_000;
        assert_eq!(vm.gc_step_budget(0), 92_177_280);
        #[cfg(target_pointer_width = "32")]
        {
            vm.gc.stepmul = -1;
            assert_eq!(vm.gc_step_budget(0), 1);
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq)]
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
const TABLE_TRACE_SLOTS: usize = 64;

// A full collection usually has only the active stack to refresh. Keep a
// short list during marking and use the arena scan for larger refresh sets.
struct ThreadRefresh {
    ids: [ThreadId; 8],
    count: usize,
}

impl ThreadRefresh {
    fn new() -> Self {
        Self {
            ids: [ThreadId(0); 8],
            count: 0,
        }
    }

    fn record(&mut self, id: ThreadId) {
        if self.count <= self.ids.len() && !self.ids[..self.count].contains(&id) {
            if let Some(slot) = self.ids.get_mut(self.count) {
                *slot = id;
            }
            self.count += 1;
        }
    }

    fn ordered(&mut self) -> Option<&[ThreadId]> {
        let ids = self.ids.get_mut(..self.count)?;
        ids.sort_unstable_by_key(|id| slot_index(id.0));
        Some(ids)
    }
}

fn edge(value: Value) -> Option<Trace> {
    match value {
        Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_) => None,
        _ => Some(Trace::Value(value)),
    }
}
fn table_edge(value: Value, weak: bool) -> Option<Trace> {
    if weak && !matches!(value, Value::String(_)) {
        return None;
    }
    edge(value)
}
#[derive(Default)]
struct TableModeCache {
    entries: [(u64, (bool, bool)); 4],
}
impl TableModeCache {
    fn get(&mut self, vm: &Vm, metatable: TableId) -> (bool, bool) {
        let cached = &mut self.entries[slot_index(metatable.0) & 3];
        if cached.0 != metatable.0 {
            *cached = (metatable.0, vm.metatable_mode(metatable));
        }
        cached.1
    }
}
#[derive(Debug)]
pub(super) struct Collector {
    phase: Phase,
    sweep_arena: u8,
    // Arena slot counts are bounded by u32::MAX.
    sweep_cursor: u32,
    stopped: bool,
    pause: i32,
    stepmul: i32,
    // One past the highest slot that can retain a metatable.
    pub(super) metatable_table_end: u32,
    last_live: usize,
    threshold: usize,
    pub(super) collections: u64,
    pub(super) tag_calls: u64,
    reclaimed: u64,
    peak: usize,
    work: Work,
    live: Buffer<u64>,
    closed: Work,
    natives: [u64; 4],
    natives_previous: [u64; 4],
    control_allocated: usize,
    pub(super) pending: bool,
    pub(super) finalizers: finalizers::Finalizers,
    pub(super) candidates: Buffer<(u64, UserdataId)>,
    pub(super) running: usize,
    pub(super) closing: bool,
}
impl Default for Collector {
    fn default() -> Self {
        Self::new_in(Memory::default())
    }
}
impl Collector {
    pub(super) fn new_in(memory: Memory) -> Self {
        Self {
            phase: Phase::Pause,
            sweep_arena: 0,
            sweep_cursor: 0,
            stopped: false,
            pause: 200,
            stepmul: 200,
            metatable_table_end: 0,
            last_live: 0,
            threshold: 64 * 1024,
            collections: 0,
            tag_calls: 0,
            reclaimed: 0,
            peak: 0,
            work: Work::new_in(memory.clone()),
            live: Buffer::new_in(memory.clone()),
            closed: Work::new_in(memory.clone()),
            natives: [0; 4],
            natives_previous: [0; 4],
            control_allocated: 0,
            pending: false,
            finalizers: finalizers::Finalizers::new_in(memory.clone()),
            candidates: Buffer::new_in(memory),
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
    fn sweep_some(&mut self, cursor: &mut usize, budget: usize, finishes_sweep: bool) -> u64 {
        let live = self.bytes.live_len();
        if live >= 1024
            && *cursor == self.bytes.len()
            && budget >= *cursor
            && self.bytes.at_most_marked(live / 4)
        {
            // A single batch can rebuild from survivors without hashing each
            // dead string and its neighbors. Compact now only when sweep will
            // finish before the mutator can insert any more strings.
            let reclaimed = self.bytes.sweep_some(cursor, budget);
            if finishes_sweep {
                self.buckets.truncate(self.rebuilt_bucket_count());
            }
            self.reindex();
            return reclaimed;
        }
        let index_mask = self.bucket_index_mask();
        let buckets = &mut self.buckets;
        let payload_bytes = &mut self.payload_bytes;
        self.bytes.sweep_with(cursor, budget, |bytes, index| {
            let mask = buckets.len() - 1;
            let string = &bytes[index];
            let hash = string.hash;
            *payload_bytes -= string.storage_bytes();
            let removed = (index as u32 + 1) | (hash as u32 & !index_mask);
            let mut hole = hash & mask;
            while buckets[hole] != removed {
                debug_assert_ne!(buckets[hole], 0);
                hole = (hole + 1) & mask;
            }
            let mut cursor = (hole + 1) & mask;
            while buckets[cursor] != 0 {
                // Backshift the cluster before freeing/reusing its arena slot.
                let entry = buckets[cursor];
                let home = bytes[(entry & index_mask) as usize - 1].hash & mask;
                if (cursor.wrapping_sub(home) & mask) >= (hole.wrapping_sub(home) & mask) {
                    buckets[hole] = entry;
                    hole = cursor;
                }
                cursor = (cursor + 1) & mask;
            }
            buckets[hole] = 0;
        })
    }
    fn rebuild(&mut self) {
        self.sweep_packed();
        if self.buckets.is_empty() {
            return;
        }
        let size = self.rebuilt_bucket_count();
        if size < self.buckets.len() {
            self.buckets.truncate(size);
            self.reindex();
        }
    }
    fn rebuilt_bucket_count(&self) -> usize {
        // Reclamation can compact the logical index but must never grow it.
        (self.bytes.live_len().saturating_mul(2))
            .max(16)
            .next_power_of_two()
            .min(self.buckets.len())
    }
    fn reindex(&mut self) {
        self.buckets.fill(0);
        let index_mask = self.bucket_index_mask();
        self.payload_bytes = 0;
        for (i, bytes) in self.bytes.entries() {
            Self::place_bucket(&mut self.buckets, i, bytes.hash, index_mask);
            self.payload_bytes += bytes.storage_bytes();
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
    let native_roots = p.parser_prototypes.capacity() * size_of::<ParserPrototypeId>()
        + p.debug_strings.capacity() * size_of::<StringId>();
    let mut bytes = p.constants.capacity() * size_of::<Buffer<Value>>()
        + p.constants
            .iter()
            .map(|k| k.capacity() * size_of::<Value>())
            .sum::<usize>();
    bytes += p.upvalue_names.capacity() * size_of::<Buffer<StringId>>()
        + p.upvalue_names
            .iter()
            .map(|names| names.capacity() * size_of::<StringId>())
            .sum::<usize>();
    bytes += crate::bytecode::ProgramData::heap_bytes(&p.data);
    bytes + native_roots
}
fn register_roots(
    registers: &[Value],
    range: std::ops::Range<usize>,
    live: &mut [u64],
    work: &mut Work,
) {
    for position in range {
        let bit = 1u64 << (position % 64);
        if live[position / 64] & bit == 0 {
            live[position / 64] |= bit;
            work.extend(edge(registers[position]));
        }
    }
}
#[inline]
fn register_roots_span(
    registers: &[Value],
    range: std::ops::Range<usize>,
    live: &mut [u64],
    work: &mut Work,
) {
    if range.len() < 16 {
        register_roots(registers, range, live, work);
    } else {
        register_roots_wide(registers, range, live, work);
    }
}
#[inline(never)]
fn register_roots_wide(
    registers: &[Value],
    range: std::ops::Range<usize>,
    live: &mut [u64],
    work: &mut Work,
) {
    let mut start = range.start;
    while start < range.end {
        let offset = start % 64;
        let count = (64 - offset).min(range.end - start);
        let mask = u64::MAX >> (64 - count);
        let marks = &mut live[start / 64];
        let mut fresh = (!*marks >> offset) & mask;
        *marks |= mask << offset;
        if fresh == mask {
            work.extend(
                registers[start..start + count]
                    .iter()
                    .copied()
                    .filter_map(edge),
            );
        } else {
            while fresh != 0 {
                work.extend(edge(registers[start + fresh.trailing_zeros() as usize]));
                fresh &= fresh - 1;
            }
        }
        start += count;
    }
}
fn frames_roots(
    functions: &Arena<Closure>,
    programs: &Arena<LoadedProgram>,
    registers: &[Value],
    frames: &[Frame],
    top: usize,
    cutoff: usize,
    live: &mut [u64],
    work: &mut Work,
) {
    for (i, frame) in frames.iter().enumerate() {
        match frame.function {
            FrameKind::Lua(function) => {
                work.push(Trace::Value(Value::Function(function)));
                let closure = &functions[slot_index(function.0)];
                let proto = &programs[closure.program].data.prototypes[closure.prototype];
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
                    register_roots_span(registers, frame.base.min(end)..end, live, work);
                }
                let pc = frame.pc().saturating_sub(1).min(proto.code.len() - 1);
                let maps = proto.metadata[pc].gc_live;
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
                let end = registers.len().min(return_top);
                let shift = frame.base % 64;
                for (word, &bits) in proto.gc_maps[map].iter().enumerate() {
                    let start = frame.base + word * 64;
                    if start >= end {
                        break;
                    }
                    if bits == 0 {
                        continue;
                    }
                    if bits.is_power_of_two() {
                        let position = start + bits.trailing_zeros() as usize;
                        if position < end {
                            register_roots(registers, position..position + 1, live, work);
                        }
                        continue;
                    }
                    let width = (end - start).min(64);
                    let marks = &mut live[start / 64..];
                    let bits = bits & (u64::MAX >> (64 - width));
                    // A frame-relative word can span two stack bitmap words.
                    let low = bits << shift;
                    let mut fresh = (low & !marks[0]) >> shift;
                    marks[0] |= low;
                    if width > 64 - shift {
                        let high = bits >> (64 - shift);
                        fresh |= (high & !marks[1]) << (64 - shift);
                        marks[1] |= high;
                    }
                    while fresh != 0 {
                        work.extend(edge(registers[start + fresh.trailing_zeros() as usize]));
                        fresh &= fresh - 1;
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
            register_roots_span(registers, start.min(end)..end, live, work);
        }
    }
}
fn clear_popped_hook_slots(hooks: &mut hooks::Hooks, live: &[u64]) {
    for (i, value) in hooks.popped.iter_mut().enumerate() {
        let position = hooks.popped_start + i;
        if live.get(position / 64).copied().unwrap_or(0) & (1u64 << (position % 64)) == 0 {
            *value = Value::Nil;
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
        let mut start = frame.base.min(end);
        while start < end {
            let offset = start % 64;
            let count = (64 - offset).min(end - start);
            let mask = u64::MAX >> (64 - count);
            let mut dead = (!live[start / 64] >> offset) & mask;
            if dead == mask {
                registers[start..start + count].fill(Value::Nil);
            } else {
                while dead != 0 {
                    registers[start + dead.trailing_zeros() as usize] = Value::Nil;
                    dead &= dead - 1;
                }
            }
            start += count;
        }
    }
}

impl Vm {
    pub(super) fn gc_note_control(&mut self, bytes: usize) {
        self.gc.control_allocated = self.gc.control_allocated.saturating_add(bytes);
    }
    #[inline]
    pub(super) fn validate_value(&self, value: Value) -> Result<(), Error> {
        let live = match value {
            Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_) => true,
            // Marks already attest that the same generation occupies a slot.
            Value::Table(id) if self.gc.phase == Phase::Sweep => self.tables.marked(id.0),
            Value::Table(id) => self.tables.contains(id.0),
            _ => self.is_live_value(value),
        };
        if live {
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
                    && (matches!(id,0..=59|64..=78|80..=85|96..=104|debug::GET_INFO..=debug::GET_REGISTRY|DO_FILE|debug::CONSOLE|io::FIRST..=io::LAST|os::FIRST..=os::LAST)
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
    /// Returns an error if the object handle is invalid, has been collected, or
    /// root/collector storage cannot grow.
    pub fn pin(&mut self, value: Value) -> Result<Root, Error> {
        self.validate_value(value)?;
        self.reserve_gc_value(value)?;
        let key = self
            .roots
            .try_insert(value)
            .map_err(|_| self.memory_failure())?;
        self.gc_new_value(value);
        Ok(Root {
            vm: self.identity,
            key,
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
        self.heap_stats_with_thread_payload(None)
    }

    // Thread records retain parked stacks and host state. The active stack
    // buffers are accounted for separately in the VM.
    fn thread_payload_bytes(thread: &threads::Thread) -> usize {
        thread.registers.capacity() * size_of::<Value>()
            + thread.frames.capacity() * size_of::<Frame>()
            + thread.open_upvalues.capacity() * size_of::<u32>()
            + thread.substitutions.capacity() * size_of::<strings::Substitution>()
            + jobs_bytes(&thread.substitutions)
            + thread.sorts.capacity() * size_of::<tables::Sort>()
            + loads::storage_bytes(&thread.loads)
            + thread.tracebacks.capacity() * size_of::<traces::Traceback>()
            + thread.hooks.hidden.capacity() * size_of::<Value>()
            + thread.hooks.popped.capacity() * size_of::<Value>()
            + thread.host_states.capacity() * size_of::<host::ThreadData>()
            + thread
                .host_states
                .iter()
                .map(host::ThreadData::heap_bytes)
                .sum::<usize>()
    }

    fn heap_stats_with_thread_payload(&mut self, thread_payload: Option<usize>) -> HeapStats {
        let mut payload = self.strings.payload_bytes
            + self.strings.buckets.capacity() * 4
            + self.strings.packed_bytes
            + self.strings.packed.capacity() * size_of::<Shared<Buffer<u8>>>();
        payload += self.object_payload_bytes;
        payload += self.io.heap_bytes();
        let mut control = self.registers.capacity() * std::mem::size_of::<Value>()
            + self.frames.capacity() * std::mem::size_of::<Frame>()
            + self.open_upvalues.capacity() * 4
            + self.substitutions.capacity() * std::mem::size_of::<strings::Substitution>()
            + jobs_bytes(&self.substitutions)
            + self.sorts.capacity() * std::mem::size_of::<tables::Sort>();
        control += loads::storage_bytes(&self.loads);
        control += self.tracebacks.capacity() * std::mem::size_of::<traces::Traceback>();
        control += self.native_bodies.capacity() * std::mem::size_of::<host::Body>();
        control += self.api_calls.capacity() * std::mem::size_of::<host::ApiCall>();
        control += self.hooks.hidden.capacity() * std::mem::size_of::<Value>();
        control += self.hooks.popped.capacity() * std::mem::size_of::<Value>();
        control += self.builtin_envs.capacity() * size_of::<Option<TableId>>();
        control += thread_payload
            .unwrap_or_else(|| self.threads.iter().map(Self::thread_payload_bytes).sum());
        control += self.buffers.capacity() * std::mem::size_of::<Buffer<u8>>()
            + self.buffers.iter().map(Buffer::capacity).sum::<usize>()
            + self.output.as_ref().map_or(0, Vec::capacity)
            + self.pattern_work.heap_bytes()
            + self.array_reduction.storage_bytes()
            + self.gc.work.capacity() * std::mem::size_of::<Trace>()
            + self.gc.live.capacity() * std::mem::size_of::<u64>()
            + self.gc.closed.capacity() * std::mem::size_of::<Trace>()
            + self
                .patterns
                .values()
                .map(crate::pattern::Pattern::heap_bytes)
                .sum::<usize>()
            + self.patterns.storage_bytes();
        control += self.gc.finalizers.capacity() * std::mem::size_of::<UserdataId>()
            + self.gc.candidates.capacity() * std::mem::size_of::<(u64, UserdataId)>()
            + self.static_strings.heap_bytes();
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
        self.reset_gc_debt_with_thread_payload(None);
    }

    fn reset_gc_debt_with_thread_payload(&mut self, thread_payload: Option<usize>) {
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
        self.gc.last_live = self
            .heap_stats_with_thread_payload(thread_payload)
            .live_bytes;
        self.set_gc_threshold();
    }
    fn set_gc_threshold(&mut self) {
        self.gc.threshold = pause_threshold(self.gc.last_live, self.gc.pause);
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
    #[inline]
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
    fn mark_barrier_value(&mut self, value: Value) -> bool {
        match value {
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
        }
    }
    pub(super) fn reserve_gc_new_value(&mut self) -> Result<(), Error> {
        self.reserve_gc_new_values(1)
    }
    pub(super) fn reserve_gc_new_values(&mut self, count: usize) -> Result<(), Error> {
        if self.gc.phase == Phase::Propagate {
            self.reserve_gc_work(count)?;
        }
        Ok(())
    }
    pub(super) fn reserve_gc_value(&mut self, value: Value) -> Result<(), Error> {
        if !self.marked_value(value) {
            self.reserve_gc_new_value()?;
        }
        Ok(())
    }
    pub(super) fn reserve_gc_barrier(&mut self, owner: Value, value: Value) -> Result<(), Error> {
        if self.marked_value(owner) {
            self.reserve_gc_value(value)?;
        }
        Ok(())
    }
    #[inline]
    pub(super) fn gc_mark_new_dense_table(&mut self, id: TableId) {
        if self.gc.phase == Phase::Propagate {
            // All references installed during construction are also marked.
            // Leave the table idle so later metatable writes can queue a scan.
            self.tables.mark(id.0);
        }
    }
    pub(super) fn gc_new_value(&mut self, value: Value) {
        if self.gc.phase == Phase::Propagate && self.mark_barrier_value(value) {
            self.gc.work.push(Trace::MarkedValue(value));
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
    pub(super) fn reserve_gc_cell(&mut self, cell: usize, value: Value) -> Result<(), Error> {
        if self.gc.phase == Phase::Propagate
            && self
                .upvalues
                .key(cell)
                .is_some_and(|key| self.upvalues.marked(key))
        {
            self.reserve_gc_value(value)?;
        }
        Ok(())
    }
    pub(super) fn gc_close_cell(&mut self, cell: usize, value: Value) {
        if self.gc.phase == Phase::Propagate
            && self
                .upvalues
                .key(cell)
                .is_some_and(|key| self.upvalues.marked(key))
            && self.mark_barrier_value(value)
        {
            // Closing also runs during error unwinding. Each open cell reserved
            // its slot when captured, before publication. Mark immediately and
            // retain the forced scan without requiring another allocation.
            self.gc.closed.push(Trace::MarkedValue(value));
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
    pub(super) fn reserve_gc_table_write(
        &mut self,
        id: TableId,
        key: Value,
        value: Value,
    ) -> Result<(), Error> {
        if self.gc.phase == Phase::Propagate && self.tables.marked(id.0) {
            let (weak_keys, weak_values) = self.table_mode(&self.tables[slot_index(id.0)]);
            let key_needed =
                (!weak_keys || matches!(key, Value::String(_))) && !self.marked_value(key);
            let value_needed = (!weak_values || matches!(value, Value::String(_)))
                && !self.marked_value(value)
                && (!key_needed || key != value);
            self.reserve_gc_work(usize::from(key_needed) + usize::from(value_needed))?;
        }
        Ok(())
    }
    pub(super) fn gc_table_write(&mut self, id: TableId, key: Value, value: Value) {
        if self.gc.phase == Phase::Propagate && self.tables.marked(id.0) {
            let (weak_keys, weak_values) = self.table_mode(&self.tables[slot_index(id.0)]);
            if (!weak_keys || matches!(key, Value::String(_))) && self.mark_barrier_value(key) {
                self.gc.work.push(Trace::MarkedValue(key));
            }
            if (!weak_values || matches!(value, Value::String(_))) && self.mark_barrier_value(value)
            {
                self.gc.work.push(Trace::MarkedValue(value));
            }
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
    pub(super) fn reserve_gc_metatable(
        &mut self,
        owner: Value,
        table: Option<TableId>,
    ) -> Result<(), Error> {
        if self.gc.phase != Phase::Propagate || matches!(owner, Value::ParserPrototype(_)) {
            return Ok(());
        }
        if let Value::Table(id) = owner {
            if self.tables.marked(id.0) && self.tables[slot_index(id.0)].trace == TableTrace::Idle {
                self.reserve_gc_new_value()?;
            }
        } else if let Some(table) = table {
            if matches!(owner, Value::Userdata(_)) {
                self.reserve_gc_barrier(owner, Value::Table(table))?;
            } else {
                self.reserve_gc_value(Value::Table(table))?;
            }
        }
        Ok(())
    }
    // Mutations that add no collector edges can skip this checkpoint in any
    // phase when the next poll would have no work to perform.
    pub(super) fn gc_checkpoint_idle(&self) -> bool {
        self.gc.running == 0
            && !self.gc.closing
            && (self.gc.stopped
                || self.gc.last_live.saturating_add(self.gc_debt()) < self.gc.threshold)
    }
    // Leave the store at the next GC threshold to ordinary dispatch.
    pub(super) fn dense_array_gc_growth(&self) -> Option<usize> {
        if self.gc.running != 0 || self.gc.closing {
            return None;
        }
        if self.gc.stopped {
            return Some(usize::MAX);
        }
        self.gc
            .threshold
            .checked_sub(self.gc.last_live.saturating_add(self.gc_debt()))?
            .checked_sub(1)
    }
    // Scalar callbacks and batched continuations can skip only a checkpoint
    // with no collector work. Keep pending-state behavior identical to maybe_gc.
    pub(super) fn sort_gc_ready(&mut self) -> bool {
        if self.can_skip_gc_checkpoint() {
            self.gc.pending = false;
            true
        } else {
            false
        }
    }

    pub(super) fn can_reuse_loaded_program(&self) -> bool {
        self.gc.phase == Phase::Pause && self.gc.running == 0 && !self.gc.closing
    }

    pub(super) fn can_skip_gc_checkpoint(&self) -> bool {
        self.gc.phase == Phase::Pause && self.gc_checkpoint_idle()
    }

    // Runs at every frame entry: the test stays inline and the work does not.
    #[inline]
    pub(super) fn maybe_gc(&mut self) -> Result<(), Error> {
        self.gc.pending = false;
        if !self.gc.stopped
            && self.gc.running == 0
            && !self.gc.closing
            && self.gc.last_live.saturating_add(self.gc_debt()) >= self.gc.threshold
        {
            return self.step_gc();
        }
        Ok(())
    }
    #[cold]
    #[inline(never)]
    fn step_gc(&mut self) -> Result<(), Error> {
        if self.advance_gc_some(self.gc_step_budget(0))? {
            if self.gc.finalizers.is_empty() {
                self.finish_gc_cycle();
            } else {
                if let Err(error) = self.push_finalizers(false) {
                    self.defer_gc_until_allocation();
                    return Err(error);
                }
            }
        } else {
            self.defer_gc_until_allocation();
        }
        Ok(())
    }
    fn table_mode(&self, table: &Table) -> (bool, bool) {
        let Some(mt) = table.metatable() else {
            return (false, false);
        };
        self.metatable_mode(mt)
    }
    fn metatable_mode(&self, mt: TableId) -> (bool, bool) {
        let Value::String(mode) =
            self.tables[slot_index(mt.0)].get_string(self.meta_names[Meta::Mode as usize])
        else {
            return (false, false);
        };
        let mode = io::c_bytes(&self.strings.bytes[slot_index(mode.0)]);
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
    fn trace_thread(&mut self, id: ThreadId, work: &mut Work) {
        let t = &self.threads[slot_index(id.0)];
        let hooks = if id == self.thread {
            &self.hooks
        } else {
            &t.hooks
        };
        if let hooks::Callback::Host(function) = hooks.callback {
            work.push(Trace::Value(Value::Native(function)));
        }
        if let Some(function) = hooks.host_running {
            work.push(Trace::Value(Value::Native(function)));
        }
        work.extend(
            hooks
                .hidden
                .iter()
                .copied()
                .filter(|&v| self.is_live_value(v))
                .filter_map(edge),
        );
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
        let prefix = frames
            .first()
            .map_or(top, |frame| frame.result)
            .min(registers.len())
            .min(cutoff);
        let api = if id != self.thread && t.status != threads::Status::Normal {
            let end = t.api_stack.end.min(registers.len());
            t.api_stack.start.max(prefix).min(end)..end
        } else {
            0..0
        };
        let use_bitmap = !frames.is_empty() || !open.is_empty() || !hooks.popped.is_empty();
        let live = &mut self.gc.live;
        if use_bitmap {
            assert!(registers.len().div_ceil(64) <= live.capacity());
            live.resize(registers.len().div_ceil(64), 0);
            live.fill(0);
            register_roots(registers, 0..prefix, live, work);
            if !frames.is_empty() {
                frames_roots(
                    &self.functions,
                    &self.programs,
                    registers,
                    frames,
                    top,
                    cutoff,
                    live,
                    work,
                );
            }
            // A Lua activation resumed from an instruction hook retains its
            // additional resume arguments until a stack-normalizing instruction.
            let hook_top = hooks.resumed_top.or_else(|| {
                hooks
                    .yield_owner
                    .filter(|_| t.resumed_values.is_some())
                    .map(|owner| (owner, top))
            });
            if let Some((owner, end)) = hook_top
                && let Some(frame) = frames.get(owner)
            {
                let end = end.min(registers.len()).min(cutoff);
                register_roots(registers, frame.base.min(end)..end, live, work);
            }
            register_roots(registers, api, live, work);
            for &cell in open {
                if let Cell::Open(owner, position) = self.upvalues[cell as usize]
                    && owner == id
                    && position < registers.len().min(cutoff)
                {
                    live[position / 64] |= 1u64 << (position % 64);
                }
            }
        } else {
            // These two ranges do not overlap and there are no slots to clear.
            work.extend(registers[..prefix].iter().copied().filter_map(edge));
            work.extend(registers[api].iter().copied().filter_map(edge));
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
        // Native scratch windows and aliased argument/upvalue slots below the
        // physical top stay live. An API returned from a yield can lower that
        // top even past an open local; Lua then clears that slot as well.
        if use_bitmap {
            if id == self.thread {
                clear_unused_registers(&mut self.registers, &self.frames, live);
                clear_popped_hook_slots(&mut self.hooks, live);
            } else {
                let thread = &mut self.threads[slot_index(id.0)];
                clear_unused_registers(&mut thread.registers, &thread.frames, live);
                clear_popped_hook_slots(&mut thread.hooks, live);
            }
        }
        self.shrink_thread_calls(id);
    }
    fn marked_weak_value(&self, value: Value, key: bool) -> bool {
        self.marked_value(value)
            && (key
                || !matches!(value, Value::Userdata(id) if self.userdata[slot_index(id.0)].finalized))
    }
    #[inline]
    fn quiet_parked_thread(thread: &threads::Thread) -> bool {
        (thread.status == threads::Status::Suspended
            || (thread.status == threads::Status::Dead
                && thread.registers.is_empty()
                && thread.frames.is_empty()))
            && thread.open_upvalues.is_empty()
            && thread.call_slots <= 16
            && !matches!(thread.hooks.callback, hooks::Callback::Host(_))
            && thread.hooks.host_running.is_none()
            && thread.hooks.hidden.is_empty()
            && thread.hooks.popped.is_empty()
            && thread.host_states.is_empty()
            && thread.resumer.is_none()
            && thread.substitutions.is_empty()
            && thread.sorts.is_empty()
            && thread.loads.is_empty()
    }

    #[inline]
    fn dormant_thread_roots(thread: &threads::Thread) -> Option<(TableId, FunctionId)> {
        let [Value::Function(entry)] = &*thread.registers else {
            return None;
        };
        if thread.top != 1 || thread.api_stack != (0..1) || !thread.frames.is_empty() {
            return None;
        }
        Some((thread.globals, *entry))
    }

    fn trace_some(&mut self, work: &mut Work, budget: usize) -> Result<usize, Error> {
        self.trace_some_with_refresh(work, budget, None)
    }

    fn trace_some_with_refresh(
        &mut self,
        work: &mut Work,
        budget: usize,
        mut refresh: Option<&mut ThreadRefresh>,
    ) -> Result<usize, Error> {
        let mut visited = 0;
        // No mutator runs within this trace batch.
        let mut modes = TableModeCache::default();
        let mut layouts = parked::Layouts::default();
        while visited < budget {
            let Some(item) = work.last().copied() else {
                break;
            };
            let handled = match item {
                Trace::Value(Value::String(id)) => {
                    self.strings.bytes.mark(id.0);
                    true
                }
                Trace::Value(value) => self.marked_value(value),
                Trace::Program(slot) => self.programs.slot_marked(slot),
                Trace::Cell(slot) => self.upvalues.slot_marked(slot),
                _ => false,
            };
            if handled {
                work.pop();
                visited += 1;
                continue;
            }
            if let Trace::Value(Value::Thread(id))
            | Trace::MarkedValue(Value::Thread(id))
            | Trace::ThreadAgain(id) = item
                && id != self.thread
                && let Some(thread) = self.threads.get(id.0)
                && Self::quiet_parked_thread(thread)
            {
                if thread.status == threads::Status::Dead {
                    // A completed stack with no attached roots only retains
                    // its environment. Popping the thread frees that slot.
                    let env = thread.globals;
                    work.pop();
                    self.threads.mark(id.0);
                    if budget != usize::MAX || !self.tables.marked(env.0) {
                        work.push(Trace::Value(Value::Table(env)));
                    }
                    visited += 1;
                    continue;
                }
                if let Some((env, entry)) = Self::dormant_thread_roots(thread) {
                    // There are no frames or extra roots to size. In an
                    // unbounded pass, skip edges that were already marked.
                    // Bounded steps still charge the original queue visits.
                    let env_needed = budget != usize::MAX || !self.tables.marked(env.0);
                    let entry_needed = budget != usize::MAX || !self.functions.marked(entry.0);
                    if env_needed && entry_needed {
                        self.reserve_trace_work(work, 1)?;
                    }
                    work.pop();
                    self.threads.mark(id.0);
                    if env_needed {
                        work.push(Trace::Value(Value::Table(env)));
                    }
                    if entry_needed {
                        work.push(Trace::Value(Value::Function(entry)));
                    }
                    visited += 1;
                    continue;
                }
                if budget == usize::MAX
                    && let Some(stack) = parked::Stack::capture(thread)
                {
                    self.trace_parked_stack(id, stack, &mut layouts, work)?;
                    visited += 1;
                    continue;
                }
            }
            if let Trace::Value(Value::Table(id)) | Trace::MarkedValue(Value::Table(id)) = item
                && self
                    .tables
                    .get(id.0)
                    .is_some_and(Table::is_edge_free_array)
            {
                // This still consumes one trace visit. Without any edges,
                // metadata or hash nodes, the complete table fits in this step.
                self.tables.mark(id.0);
                self.tables[slot_index(id.0)].trace = TableTrace::Idle;
                work.pop();
                visited += 1;
                continue;
            }
            self.reserve_trace_item(item, work)?;
            work.pop();
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
                        self.trace_table(id, work, &mut modes);
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
                        if let Some(refresh) = refresh.as_deref_mut()
                            && (id == self.thread
                                || !Self::quiet_parked_thread(&self.threads[slot_index(id.0)]))
                        {
                            refresh.record(id);
                        }
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
                Trace::TableAgain(id) => self.trace_table(id, work, &mut modes),
                Trace::TableSlice(id, cursor) => {
                    self.trace_table_slice(id, cursor, work, &mut modes)
                }
                Trace::CellAgain(i) => work.extend(edge(self.cell_value(i))),
                Trace::Program(i) => {
                    if self.programs.mark_slot(i) {
                        for k in &self.programs[i].constants {
                            work.extend(k.iter().copied().filter_map(edge));
                        }
                        for names in &self.programs[i].upvalue_names {
                            work.extend(
                                names.iter().map(|&name| Trace::Value(Value::String(name))),
                            );
                        }
                        work.extend(
                            self.programs[i]
                                .debug_strings
                                .iter()
                                .map(|&id| Trace::Value(Value::String(id))),
                        );
                    }
                }
                Trace::Cell(i) => {
                    if self.upvalues.mark_slot(i) {
                        work.extend(edge(self.cell_value(i)));
                    }
                }
                Trace::File(key) => {
                    self.io.files.mark(key);
                }
                _ => {}
            }
        }
        Ok(visited)
    }
    fn trace_graph(&mut self, work: &mut Work) -> Result<(), Error> {
        self.trace_some(work, usize::MAX).map(|_| ())
    }
    fn trace_table(&mut self, id: TableId, work: &mut Work, modes: &mut TableModeCache) {
        // Fresh marks also discard scan state left by an interrupted cycle.
        self.tables[slot_index(id.0)].trace = TableTrace::Restart;
        self.trace_table_slice(id, 0, work, modes);
    }
    fn trace_table_slice(
        &mut self,
        id: TableId,
        cursor: usize,
        work: &mut Work,
        modes: &mut TableModeCache,
    ) {
        let table = &self.tables[slot_index(id.0)];
        // Read __mode once per scan. The atomic metatable refresh observes mode
        // mutations before weak cleanup, just as for an unsliced table scan.
        let (cursor, weak_keys, weak_values) = match table.trace {
            TableTrace::Pending(wk, wv) => (cursor, wk, wv),
            TableTrace::Idle | TableTrace::Restart => {
                let (wk, wv) = table
                    .metatable()
                    .map_or((false, false), |mt| modes.get(self, mt));
                (0, wk, wv)
            }
        };
        self.tables[slot_index(id.0)].trace = TableTrace::Pending(weak_keys, weak_values);
        if !weak_keys || !weak_values {
            self.tables[slot_index(id.0)]
                .retire_native_keys_in_trace_slice(cursor, TABLE_TRACE_SLOTS);
        }
        let table = &self.tables[slot_index(id.0)];
        let (next, array, pairs) = table.trace_slice(cursor, TABLE_TRACE_SLOTS);
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
        if cursor == 0
            && let Some(state) = table.host_state()
        {
            work.push(Trace::Value(Value::Table(state)));
        }
        if weak_values {
            work.extend(array.iter().copied().filter_map(|value| match value {
                Value::String(_) => Some(Trace::Value(value)),
                _ => None,
            }));
        } else {
            work.extend(array.iter().copied().filter_map(edge));
        }
        for (_, key, value) in pairs {
            work.extend(table_edge(key, weak_keys));
            work.extend(table_edge(value, weak_values));
        }
        if next.is_none() {
            self.tables[slot_index(id.0)].trace = TableTrace::Idle;
        }
    }
    fn begin_gc_cycle(&mut self) -> Result<(), Error> {
        self.begin_gc_cycle_with_thread_payload(false).map(|_| ())
    }

    fn begin_gc_cycle_with_thread_payload(
        &mut self,
        measure_for_sweep: bool,
    ) -> Result<Option<usize>, Error> {
        let roots = self.seed_gc_root_count(self.gc.work.capacity());
        if let Err(error) = self.reserve_gc_work(roots.saturating_sub(self.gc.work.len())) {
            self.defer_gc_until_allocation();
            return Err(error);
        }
        let thread_payload = if measure_for_sweep {
            let mut has_host_state = false;
            let bytes = self
                .threads
                .iter()
                .map(|thread| {
                    has_host_state |= !thread.host_states.is_empty();
                    Self::thread_payload_bytes(thread)
                })
                .sum();
            self.heap_stats_with_thread_payload(Some(bytes));
            // Host resources can report a different footprint at the next
            // observation. Always measure them again after sweeping.
            (!has_host_state).then_some(bytes)
        } else {
            self.heap_stats();
            None
        };
        self.gc.phase = Phase::Propagate;
        self.gc.closed.clear();
        self.protect_new_allocations(false);
        self.strings.bytes.clear_marks();
        // Strings have no references to trace; marking them here keeps them.
        for id in self.static_strings.strings() {
            self.strings.bytes.mark(id.0);
        }
        self.tables.clear_marks();
        self.functions.clear_marks();
        self.upvalues.clear_marks();
        self.threads.clear_marks();
        self.native_closures.clear_marks();
        self.programs.clear_marks();
        self.parser_prototypes.clear_marks();
        self.userdata.clear_marks();
        self.io.files.clear_marks();
        for key in [self.io.stdin, self.io.stdout, self.io.stderr]
            .into_iter()
            .flatten()
        {
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
        Ok(thread_payload)
    }
    fn seed_gc_roots(&self, work: &mut Work) {
        work.push(Trace::Value(Value::Thread(self.main)));
        work.push(Trace::Value(Value::Table(self.registry)));
        work.push(Trace::Value(Value::String(self.memory_error)));
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
                .filter(|p| p.owner.load(std::sync::atomic::Ordering::Acquire) != 0)
                .map(|p| Trace::Value(Value::Function(p.entry))),
        );
    }
    fn finish_gc_marking(&mut self, fresh_cycle: bool) -> Result<(), Error> {
        let mut work = std::mem::take(&mut self.gc.work);
        let result = self.finish_gc_marking_work(&mut work, fresh_cycle);
        self.gc.work = work;
        if result.is_err() {
            self.defer_gc_until_allocation();
        }
        result
    }
    fn finish_gc_marking_work(&mut self, work: &mut Work, fresh_cycle: bool) -> Result<(), Error> {
        self.transfer_closed_work(work)?;
        let mut refresh_threads = ThreadRefresh::new();
        if fresh_cycle {
            self.trace_some_with_refresh(work, usize::MAX, Some(&mut refresh_threads))?;
        } else {
            self.trace_graph(work)?;
        }
        // Snapshot the already reached containers before refreshing roots;
        // newly reached threads/cells still receive their ordinary first scan.
        let small_refresh = fresh_cycle.then(|| refresh_threads.ordered()).flatten();
        let threads = small_refresh.map_or_else(|| self.threads.marked_count(), <[_]>::len);
        let metatable_words = (self.gc.metatable_table_end as usize).div_ceil(64);
        let tables = self
            .tables
            .marked_slots_in_words(0..metatable_words)
            .filter(|&index| self.tables[index].metatable().is_some())
            .count();
        let open_words = if self.upvalues.live_len() == 0 {
            0..0
        } else if self.threads.live_len() == 1 {
            // The active stack owns all open cells. Slot order still comes
            // from the bitmap, since closing a cell can swap list entries.
            if let Some((&first, rest)) = self.open_upvalues.split_first() {
                let (lowest, highest) = rest.iter().fold((first, first), |(lo, hi), &index| {
                    (lo.min(index), hi.max(index))
                });
                lowest as usize / 64..highest as usize / 64 + 1
            } else {
                0..0
            }
        } else {
            0..self.upvalues.len().div_ceil(64)
        };
        let cells = self
            .upvalues
            .marked_slots_in_words(open_words.clone())
            .filter(|&index| matches!(self.upvalues[index], Cell::Open(..)))
            .count();
        let refresh = threads.saturating_add(tables).saturating_add(cells);
        let available = (work.capacity() - work.len()).saturating_sub(refresh);
        let roots = self.seed_gc_root_count(available);
        self.reserve_trace_work(work, refresh.saturating_add(roots))?;
        if let Some(ids) = small_refresh {
            work.extend(ids.iter().copied().map(Trace::ThreadAgain));
        } else {
            for key in self.threads.marked_keys() {
                let id = ThreadId(key);
                // During synchronous collection the first pass handled quiet
                // parked threads, and their roots stay fixed.
                if !fresh_cycle
                    || id == self.thread
                    || !Self::quiet_parked_thread(&self.threads[slot_index(key)])
                {
                    work.push(Trace::ThreadAgain(id));
                }
            }
        }
        for index in self.tables.marked_slots_in_words(0..metatable_words) {
            if self.tables[index].metatable().is_some() {
                work.push(Trace::TableAgain(TableId(self.tables.key(index).unwrap())));
            }
        }
        for index in self.upvalues.marked_slots_in_words(open_words) {
            if matches!(self.upvalues[index], Cell::Open(..)) {
                work.push(Trace::CellAgain(index));
            }
        }
        self.seed_gc_roots(work);
        self.trace_graph(work)?;
        // Pending callbacks preserve their graphs only after discovering newly
        // finalizable userdata, including graphs attached after a callback error.
        self.separate_userdata(false);
        self.reserve_trace_work(work, self.gc.finalizers.iter().len())?;
        work.extend(
            self.gc
                .finalizers
                .iter()
                .copied()
                .map(|id| Trace::Value(Value::Userdata(id))),
        );
        self.trace_graph(work)?;
        let metatable_end = self.clear_weak_tables();
        if self.threads.has_unmarked() {
            for thread in self.threads.unmarked_values() {
                for &cell in &thread.open_upvalues {
                    if let Cell::Open(_, r) = self.upvalues[cell as usize] {
                        self.upvalues[cell as usize] = Cell::Closed(thread.registers[r]);
                    }
                }
            }
        }
        // Existing unmarked tables cannot survive sweep. Attaching a
        // metatable to a new table extends the bound at the point of the write.
        self.gc.metatable_table_end = metatable_end;
        self.protect_new_allocations(true);
        self.gc.phase = Phase::Sweep;
        self.gc.sweep_arena = 0;
        self.gc.sweep_cursor = self.tables.len() as u32;
        Ok(())
    }
    fn clear_weak_tables(&mut self) -> u32 {
        let mut metatable_end = 0;
        // A string __mode survives weak cleanup. Clearing a non-string mode
        // leaves the same strong-table flags.
        let mut modes = TableModeCache::default();
        for word in 0..(self.gc.metatable_table_end as usize).div_ceil(64) {
            let mut marked = self.tables.marked_word(word);
            while marked != 0 {
                let index = word * 64 + marked.trailing_zeros() as usize;
                marked &= marked - 1;
                let Some(mt) = self.tables[index].metatable() else {
                    continue;
                };
                metatable_end = index as u32 + 1;
                let (weak_keys, weak_values) = modes.get(self, mt);
                if !weak_keys && !weak_values {
                    continue;
                }
                // Weak keys cannot remove array entries with integer keys.
                let mut cursor = if weak_values { 0 } else { 1 };
                loop {
                    let mut dead_array = 0u64;
                    let mut dead_hash = 0u64;
                    let (next, array, pairs) =
                        self.tables[index].trace_slice(cursor, TABLE_TRACE_SLOTS);
                    // Implicit array keys are numbers and cannot be collected.
                    if weak_values {
                        for (offset, &value) in array.iter().enumerate() {
                            if !self.marked_weak_value(value, false) {
                                dead_array |= 1u64 << offset;
                            }
                        }
                    }
                    for (offset, key, value) in pairs {
                        if (weak_keys && !self.marked_weak_value(key, true))
                            || (weak_values && !self.marked_weak_value(value, false))
                        {
                            dead_hash |= 1u64 << offset;
                        }
                    }
                    // Deleting an existing key neither allocates nor compacts its
                    // physical slot. The next cursor therefore remains valid across
                    // array holes, hash tombstones and the array/hash boundary.
                    let table = &mut self.tables[index];
                    table.clear_array_slots(cursor >> 1, dead_array);
                    table.clear_weak_hash_slots(cursor, dead_hash);
                    let Some(next) = next else { break };
                    cursor = next;
                }
            }
        }
        metatable_end
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
    fn sweep_arena_len(&self, arena: u8) -> u32 {
        (match arena {
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
        }) as u32
    }
    fn sweep_gc_some(&mut self, budget: usize) {
        self.sweep_gc_some_with_thread_payload(budget, None);
    }

    fn sweep_gc_some_with_thread_payload(
        &mut self,
        mut budget: usize,
        mut thread_payload: Option<&mut usize>,
    ) {
        while budget != 0 && self.gc.sweep_arena < 10 {
            let mut cursor = self.gc.sweep_cursor as usize;
            let before = cursor;
            let count = match self.gc.sweep_arena {
                0 => {
                    let bytes = &mut self.object_payload_bytes;
                    self.tables
                        .sweep_with(&mut cursor, budget, |tables, index| {
                            *bytes -= tables[index].heap_bytes();
                        })
                }
                1 => {
                    let bytes = &mut self.object_payload_bytes;
                    self.functions
                        .sweep_with(&mut cursor, budget, |functions, index| {
                            *bytes -= functions[index].upvalues.len() * size_of::<u32>();
                        })
                }
                2 => self.upvalues.sweep_some(&mut cursor, budget),
                3 => {
                    if let Some(bytes) = thread_payload.as_deref_mut() {
                        self.threads
                            .sweep_with(&mut cursor, budget, |threads, index| {
                                *bytes -= Self::thread_payload_bytes(&threads[index]);
                            })
                    } else {
                        self.threads.sweep_some(&mut cursor, budget)
                    }
                }
                4 => {
                    let bytes = &mut self.object_payload_bytes;
                    self.native_closures
                        .sweep_with(&mut cursor, budget, |closures, index| {
                            *bytes -= closures[index].heap_bytes();
                        })
                }
                5 => {
                    let bytes = &mut self.object_payload_bytes;
                    self.programs
                        .sweep_with(&mut cursor, budget, |programs, index| {
                            *bytes -= program_bytes(&programs[index]);
                        })
                }
                6 => {
                    let later = self
                        .userdata
                        .len()
                        .saturating_add(self.io.files.len())
                        .saturating_add(self.parser_prototypes.len());
                    // Leave budget for advancing past any trailing empty arena.
                    let finishes_sweep = budget.saturating_sub(cursor) > later;
                    self.strings.sweep_some(&mut cursor, budget, finishes_sweep)
                }
                7 => {
                    let bytes = &mut self.object_payload_bytes;
                    self.userdata
                        .sweep_with(&mut cursor, budget, |userdata, index| {
                            *bytes -= userdata[index].bytes.storage_bytes();
                        })
                }
                8 => self.io.files.sweep_some(&mut cursor, budget),
                9 => {
                    let bytes = &mut self.object_payload_bytes;
                    self.parser_prototypes
                        .sweep_with(&mut cursor, budget, |prototypes, index| {
                            *bytes -= prototypes[index].heap_bytes();
                        })
                }
                _ => unreachable!(),
            };
            self.gc.reclaimed += count;
            budget -= before - cursor;
            self.gc.sweep_cursor = if cursor == 0 {
                self.gc.sweep_arena += 1;
                self.sweep_arena_len(self.gc.sweep_arena)
            } else {
                cursor as u32
            };
        }
        if self.gc.sweep_arena != 10 {
            return;
        }
        self.strings.rebuild();
        self.patterns
            .retain(|(id, _), _| self.strings.bytes.contains(id.0));
        for (envs, marked) in self.builtin_envs.chunks_mut(64).zip(self.gc.natives) {
            if marked == 0 {
                envs.fill(None);
                continue;
            }
            let mut clear = !marked & (u64::MAX >> (64 - envs.len()));
            while clear != 0 {
                envs[clear.trailing_zeros() as usize] = None;
                clear &= clear - 1;
            }
        }
        self.gc.phase = Phase::Finalize;
        self.gc.collections += 1;
        // Resetting the debt measures the heap, which also records its peak.
        self.reset_gc_debt_with_thread_payload(thread_payload.copied());
    }
    fn advance_gc_some(&mut self, mut budget: usize) -> Result<bool, Error> {
        if self.gc.phase == Phase::Pause {
            self.begin_gc_cycle()?;
        }
        if self.gc.phase == Phase::Propagate {
            let mut work = std::mem::take(&mut self.gc.work);
            let result = self
                .transfer_closed_work(&mut work)
                .and_then(|()| self.trace_some(&mut work, budget));
            self.gc.work = work;
            if result.is_err() {
                self.defer_gc_until_allocation();
            }
            budget -= result?;
            if self.gc.work.is_empty() {
                self.finish_gc_marking(false)?;
            }
        }
        if self.gc.phase == Phase::Sweep {
            self.sweep_gc_some(budget);
        }
        Ok(self.gc.phase == Phase::Finalize)
    }
    fn gc_step_budget(&self, extra: i32) -> usize {
        let limit = step_limit(self.gc.stepmul);
        if limit == 0 {
            return usize::MAX;
        }
        if limit < 0 {
            // luaC_step still performs its first incremental operation when
            // the signed work limit is negative.
            return 1;
        }
        // Convert the C limit into tracing units before narrowing. Saturating
        // the intermediate product at usize would cut a large i686 budget off
        // before division, even when the final tracing count fits.
        let requested = (extra as usize).wrapping_shl(10).max(1024) as u128;
        let budget = requested * limit as u128 / 1024 / 16;
        budget.clamp(1, usize::MAX as u128) as usize
    }
    pub(super) fn defer_gc_until_allocation(&mut self) {
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
    pub(super) fn prepare_gc_finalizers(&mut self) -> Result<(), Error> {
        if self.gc.phase == Phase::Propagate {
            self.finish_gc_marking(false)?;
        }
        self.finish_pending_sweep();
        Ok(())
    }
    pub(super) fn collect_cycle(&mut self) -> Result<(), Error> {
        self.finish_pending_sweep();
        // No mutator runs between the observations. Marking may clear stack
        // slots in place, and sweeping accounts for every freed thread.
        let mut thread_payload = self.begin_gc_cycle_with_thread_payload(true)?;
        self.finish_gc_marking(true)?;
        self.sweep_gc_some_with_thread_payload(usize::MAX, thread_payload.as_mut());
        Ok(())
    }
    pub(super) fn gc_control(&mut self, operation: GcOperation) -> Result<(i32, bool), Error> {
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
                self.collect_cycle()?;
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
                let complete = self.advance_gc_some(self.gc_step_budget(extra))?;
                if complete {
                    self.finish_gc_cycle();
                    dispatch = true;
                } else {
                    self.defer_gc_until_allocation();
                }
                i32::from(complete)
            }
        };
        Ok((result, dispatch))
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
        let (result, dispatch) = self.gc_control(operation)?;
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
    fn thread_refresh_orders_slots_and_falls_back_when_full() {
        let mut refresh = ThreadRefresh::new();
        let first = ThreadId((2 << 32) | 1);
        let second = ThreadId((1 << 32) | 2);
        refresh.record(second);
        refresh.record(first);
        refresh.record(second);
        assert_eq!(refresh.ordered(), Some(&[first, second][..]));
        for slot in 3..=9 {
            refresh.record(ThreadId((1 << 32) | slot));
        }
        assert!(refresh.ordered().is_none());
    }

    #[test]
    fn completed_coroutine_traces_its_private_environment() {
        let program = crate::compile(
            br#"
            local thread = coroutine.create(function()
                coroutine.yield(19)
                return 42
            end)
            assert(coroutine.resume(thread))
            assert(coroutine.resume(thread))
            return thread
            "#,
        )
        .unwrap();
        for incremental in [false, true] {
            let mut vm = Vm::new();
            let [Value::Thread(id)] = *vm.execute(&program).unwrap() else {
                panic!("expected a completed coroutine");
            };
            assert!(Vm::quiet_parked_thread(&vm.threads[slot_index(id.0)]));
            let root = vm.pin(Value::Thread(id)).unwrap();
            let environment = vm.new_table();
            vm.set_thread_environment(id, environment).unwrap();
            let marker = vm.intern(b"only reachable from the thread environment");
            let key = Value::Number(1.0);
            vm.raw_set(environment, key, marker).unwrap();

            if incremental {
                while vm.gc_control(GcOperation::Step(0)).unwrap().0 == 0 {}
            } else {
                vm.collect_garbage().unwrap();
            }
            assert_eq!(vm.thread_environment(id), environment);
            assert_eq!(vm.tables[slot_index(environment.0)].get(key), marker);
            assert!(vm.is_live_value(marker));
            vm.unpin(root).unwrap();
        }
    }

    #[test]
    fn full_collection_accounts_for_freed_coroutine_stacks() {
        let mut vm = Vm::new();
        vm.execute(
            &crate::compile(
                br#"
                collectgarbage('stop')
                retained = {}
                local function entry() coroutine.yield(19) end
                for i = 1, 24 do
                    local thread = coroutine.create(entry)
                    if i % 2 == 0 then assert(coroutine.resume(thread)) end
                    if i <= 2 then retained[i] = thread end
                end
                "#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(vm.heap_stats().threads, 25);
        vm.collect_cycle().unwrap();
        let after = vm.heap_stats();
        assert_eq!(after.threads, 3);
        assert_eq!(vm.gc.last_live, after.live_bytes);
    }
    #[test]
    fn weak_cleanup_never_allocates_and_preserves_live_entries_across_storage_slices() {
        let mut vm = Vm::new_bare();
        let table = vm.new_table();
        let meta = vm.new_table();
        let name = vm.intern(b"__mode");
        let mode = vm.intern(b"kv");
        vm.raw_set(meta, name, mode).unwrap();
        vm.set_metatable(Value::Table(table), Some(meta)).unwrap();
        vm.pin(Value::Table(table)).unwrap();
        let mut retained = Vec::new();
        for n in 1..=259 {
            let value = Value::Table(vm.new_table());
            let key = Value::Number(f64::from(n));
            vm.raw_set(table, key, value).unwrap();
            if n % 11 == 0 {
                vm.pin(value).unwrap();
                retained.push((key, value));
            } else if n % 13 == 0 {
                vm.raw_set(table, key, Value::Nil).unwrap();
            }
        }
        for n in 1..=193 {
            let key = Value::Table(vm.new_table());
            let value = Value::Table(vm.new_table());
            vm.raw_set(table, key, value).unwrap();
            if n % 11 == 0 {
                vm.pin(key).unwrap();
                vm.pin(value).unwrap();
                retained.push((key, value));
            } else if n % 13 == 0 {
                vm.raw_set(table, key, Value::Nil).unwrap();
            }
        }
        let string = vm.intern(b"strings remain strong");
        vm.raw_set(table, string, string).unwrap();
        retained.push((string, string));
        vm.begin_gc_cycle().unwrap();
        let mut work = std::mem::take(&mut vm.gc.work);
        vm.trace_graph(&mut work).unwrap();
        vm.gc.work = work;
        let (_, stats) = crate::allocation_probe::probe(1, || vm.clear_weak_tables());
        assert_eq!(stats.attempts, 0);
        let table = &vm.tables[slot_index(table.0)];
        assert_eq!(table.pairs().count(), retained.len());
        for (key, value) in retained {
            assert_eq!(table.get(key), value);
        }
    }

    #[test]
    fn fresh_userdata_can_be_finalized_in_the_current_marking_cycle() {
        fn finalizer(_: &mut HostCall<'_>) -> Result<(), Error> {
            Ok(())
        }
        let mut vm = Vm::new_bare();
        let callback = vm.new_host_function(finalizer, &[]).unwrap();
        let mt = vm.new_table();
        let gc = vm.intern(b"__gc");
        vm.raw_set(mt, gc, callback).unwrap();
        vm.begin_gc_cycle().unwrap();
        let dead = vm.try_new_userdata(0).unwrap();
        let live = vm.try_new_userdata(0).unwrap();
        vm.set_metatable(dead, Some(mt)).unwrap();
        vm.set_metatable(live, Some(mt)).unwrap();
        assert!(!vm.marked_value(dead));
        assert!(!vm.marked_value(live));
        let root = vm.pin(live).unwrap();
        vm.finish_gc_marking(false).unwrap();
        let Value::Userdata(dead) = dead else {
            unreachable!()
        };
        let Value::Userdata(live) = live else {
            unreachable!()
        };
        assert!(vm.gc.finalizers.contains(&dead));
        assert!(!vm.gc.finalizers.contains(&live));
        assert_eq!(vm.rooted(root).unwrap(), Value::Userdata(live));
    }
    #[test]
    fn mutation_reserves_both_strong_edges_before_publishing_either() {
        let mut vm = Vm::new_bare();
        let table = vm.try_table_with_capacity(0, 2).unwrap();
        let key = Value::Table(vm.new_table());
        let value = Value::Table(vm.new_table());
        vm.gc.phase = Phase::Propagate;
        vm.gc.work = Work::with_capacity(1);
        vm.tables.mark(table.0);
        vm.tables[slot_index(table.0)].trace = TableTrace::Pending(false, false);
        let (result, stats) = crate::allocation_probe::probe(1, || vm.raw_set(table, key, value));
        assert_eq!(stats.rejected, 1);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
        assert_eq!(vm.tables[slot_index(table.0)].get(key), Value::Nil);
        assert_eq!(
            vm.tables[slot_index(table.0)].trace,
            TableTrace::Pending(false, false)
        );
        assert!(vm.gc.work.is_empty());
        assert!(!vm.marked_value(key));
        assert!(!vm.marked_value(value));
        vm.raw_set(table, key, value).unwrap();
        assert_eq!(vm.gc.work.len(), 2);
        assert!(vm.marked_value(key));
        assert!(vm.marked_value(value));
        vm.gc.phase = Phase::Pause;
        eprintln!("TABLE_MUTATION_GC_FAILURE_POINTS 1");
    }
    #[test]
    fn mutation_weak_and_duplicate_edges_do_not_request_unneeded_queue_storage() {
        for (mode, same, expected) in [
            (b"k".as_slice(), false, 1),
            (b"v", false, 1),
            (b"kv", false, 0),
            (b"", true, 1),
        ] {
            let mut vm = Vm::new_bare();
            let table = vm.try_table_with_capacity(0, 2).unwrap();
            let mt = vm.new_table();
            let name = vm.intern(b"__mode");
            let mode_value = vm.intern(mode);
            vm.raw_set(mt, name, mode_value).unwrap();
            vm.set_metatable(Value::Table(table), Some(mt)).unwrap();
            let key = Value::Table(vm.new_table());
            let value = if same {
                key
            } else {
                Value::Table(vm.new_table())
            };
            vm.gc.phase = Phase::Propagate;
            vm.gc.work = Work::with_capacity(expected);
            vm.tables.mark(table.0);
            let (result, stats) =
                crate::allocation_probe::probe(1, || vm.raw_set(table, key, value));
            result.unwrap();
            assert_eq!(stats.attempts, 0);
            assert_eq!(vm.gc.work.len(), expected);
            assert_eq!(vm.tables[slot_index(table.0)].get(key), value);
            vm.gc.phase = Phase::Pause;
        }
    }
    #[test]
    fn mutation_failed_hash_reservation_accounts_for_the_buffer_that_already_grew() {
        let mut vm = Vm::new_bare();
        let table = vm.try_table_with_capacity(0, 5).unwrap();
        for n in 1..=5 {
            vm.raw_set(
                table,
                Value::Number(-f64::from(n)),
                Value::Number(f64::from(n)),
            )
            .unwrap();
        }
        let before = vm.tables[slot_index(table.0)].heap_bytes();
        let debt = vm.tables.allocated;
        let (result, stats) = crate::allocation_probe::probe(2, || {
            vm.raw_set(table, Value::Number(-6.), Value::Number(42.))
        });
        assert_eq!(stats.rejected, 1);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
        let retained = vm.tables[slot_index(table.0)].heap_bytes() - before;
        assert!(retained > 0);
        assert_eq!(vm.tables.allocated - debt, retained);
        assert_eq!(
            vm.tables[slot_index(table.0)].get(Value::Number(-6.)),
            Value::Nil
        );
        vm.raw_set(table, Value::Number(-6.), Value::Number(42.))
            .unwrap();
        eprintln!("TABLE_MUTATION_GC_FAILURE_POINTS 1");
    }

    #[test]
    fn allocation_failure_during_marking_keeps_the_work_queue_and_object_identity_unpublished() {
        let mut attempts = 0;
        // Two finalizer buffers + payload + the arena's four parallel buffers.
        // A new white userdata
        // needs no mark work until it is retained through a host root/barrier.
        for nth in 0..=7 {
            let mut vm = Vm::new_bare();
            vm.gc.phase = Phase::Propagate;
            vm.gc.work = Work::default();
            let (result, stats) = crate::allocation_probe::probe(nth, || vm.try_new_userdata(16));
            if nth == 0 {
                attempts = stats.attempts;
                assert_eq!(attempts, 7);
                let value = result.unwrap();
                assert!(vm.gc.work.is_empty());
                let (pinned, stats) = crate::allocation_probe::probe(1, || vm.pin(value));
                assert_eq!(stats.rejected, 1);
                assert_eq!(pinned.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(vm.roots.live_len(), 0);
                assert!(vm.gc.work.is_empty());
                vm.pin(value).unwrap();
                assert_eq!(vm.gc.work.len(), 1);
            } else {
                assert!(nth <= attempts);
                assert_eq!(stats.rejected, 1);
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(vm.userdata.live_len(), 0);
                assert_eq!(vm.userdata_order, 0);
                assert!(vm.gc.work.is_empty());
                let Value::Userdata(id) = vm.try_new_userdata(16).unwrap() else {
                    unreachable!()
                };
                assert!(!vm.userdata.marked(id.0));
                assert!(vm.gc.work.is_empty());
                assert_eq!(vm.userdata_order, 1);
            }
            vm.gc.phase = Phase::Pause;
        }
        eprintln!("ALLOCATION_GC_FAILURE_POINTS 8");
    }

    #[test]
    fn binding_failure_during_marking_does_not_publish_objects_or_mark_work() {
        let program = crate::compile(b"return 42").unwrap();
        let run = |nth| {
            let mut vm = Vm::new_bare();
            vm.gc.phase = Phase::Propagate;
            vm.gc.work = Work::default();
            let (result, stats) =
                crate::allocation_probe::probe(nth, || vm.try_load_program(&program));
            if nth == 0 {
                result.unwrap();
                assert_eq!(vm.gc.work.len(), 1);
            } else {
                assert_eq!(stats.rejected, 1);
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(vm.programs.live_len(), 0);
                assert_eq!(vm.functions.live_len(), 0);
                assert_eq!(vm.upvalues.live_len(), 0);
                assert!(vm.gc.work.is_empty());
                vm.try_load_program(&program).unwrap();
                assert_eq!(vm.gc.work.len(), 1);
            }
            vm.gc.phase = Phase::Pause;
            stats.attempts
        };
        let total = run(0);
        assert!(total >= 12);
        for nth in 1..=total {
            run(nth);
        }
        let mut vm = Vm::new_bare();
        let function = vm.try_load_program(&program).unwrap();
        vm.gc.phase = Phase::Propagate;
        vm.gc.work = Work::default();
        let (result, stats) = crate::allocation_probe::probe(1, || vm.try_load_program(&program));
        assert_eq!(stats.rejected, 1);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
        assert!(!vm.functions.marked(function.0));
        assert!(vm.gc.work.is_empty());
        assert_eq!(vm.try_load_program(&program).unwrap(), function);
        assert!(vm.functions.marked(function.0));
        vm.gc.phase = Phase::Pause;
        eprintln!("BINDING_MARK_REJECTION_POINTS {}", total + 1);
    }

    #[test]
    fn host_binding_and_thread_state_failures_during_marking_publish_no_roots() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct State {
            roots: [Value; 2],
            drops: Arc<AtomicUsize>,
        }
        impl host::HostThreadState for State {
            fn roots(&self) -> &[Value] {
                &self.roots
            }
        }
        impl Drop for State {
            fn drop(&mut self) {
                self.drops.fetch_add(1, Ordering::Relaxed);
            }
        }
        fn empty(_: &mut HostCall<'_>) -> Result<(), Error> {
            Ok(())
        }
        let run = |nth, thread_state| {
            let mut vm = Vm::new_bare();
            let roots = [Value::Table(vm.new_table()), Value::Table(vm.new_table())];
            let drops = Arc::new(AtomicUsize::new(0));
            let state = State {
                roots,
                drops: drops.clone(),
            };
            vm.gc.phase = Phase::Propagate;
            vm.gc.work = Work::default();
            let (result, stats) = crate::allocation_probe::probe(nth, || {
                if thread_state {
                    vm.initialize_thread_state(state)
                } else {
                    vm.new_host_function_with_data(empty, &roots, state)
                        .map(|_| ())
                }
            });
            if nth == 0 {
                result.unwrap();
                assert!(!vm.gc.work.is_empty());
                assert_eq!(drops.load(Ordering::Relaxed), 0);
            } else {
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(stats.rejected, 1);
                assert_eq!(drops.load(Ordering::Relaxed), 1);
                assert!(vm.gc.work.is_empty());
                assert!(roots.iter().all(|&v| !vm.marked_value(v)));
                assert!(vm.threads[slot_index(vm.main.0)].host_states.is_empty());
                assert_eq!(vm.native_closures.live_len(), 0);
            }
            vm.gc.phase = Phase::Pause;
            drop(vm);
            assert_eq!(drops.load(Ordering::Relaxed), 1);
            stats.attempts
        };
        let mut total = 0;
        for thread_state in [false, true] {
            let points = run(0, thread_state);
            assert!(points >= 3);
            for nth in 1..=points {
                run(nth, thread_state);
            }
            total += points;
        }
        eprintln!("BINDING_MARK_REJECTION_POINTS {total}");
    }

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
            vm.begin_gc_cycle().unwrap();
            let mut work = Work::from([Trace::Value(function)]);
            assert_eq!(vm.trace_some(&mut work, 1).unwrap(), 1);
            assert!(vm.marked_value(function) && !vm.marked_value(replacement));
            vm.gc.work.try_reserve(work.len()).unwrap();
            vm.gc.work.extend(work.iter().copied());
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
        vm.begin_gc_cycle().unwrap();
        let mut work = Work::from([Trace::Value(function)]);
        while !work.is_empty() {
            vm.trace_some(&mut work, 1).unwrap();
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
        vm.begin_gc_cycle().unwrap();
        while !vm.threads.marked(vm.thread.0) {
            let mut work = std::mem::take(&mut vm.gc.work);
            assert_eq!(vm.trace_some(&mut work, 1).unwrap(), 1);
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
        vm.finish_gc_marking(false).unwrap();
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
            assert_eq!(vm.trace_some(&mut work, 1).unwrap(), 1);
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
            vm.begin_gc_cycle().unwrap();
            let mut work = Work::from([Trace::Value(Value::Table(table))]);
            let mut peak = 0;
            while !work.is_empty() {
                assert_eq!(vm.trace_some(&mut work, 1).unwrap(), 1);
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
            vm.begin_gc_cycle().unwrap();
            let mut work = Work::from([Trace::Value(Value::Table(table))]);
            for _ in 0..200 {
                assert_eq!(vm.trace_some(&mut work, 1).unwrap(), 1);
                assert_eq!(work.len(), 1);
                assert!(!vm.marked_value(child));
            }
            vm.trace_graph(&mut work).unwrap();
            assert!(vm.marked_value(child));
            finish_marked_cycle(&mut vm);
            assert!(vm.validate_value(child).is_ok());
            vm.unpin(root).unwrap();
        }
    }

    #[test]
    fn hash_compaction_restarts_an_active_scan_before_shifted_entries() {
        let mut vm = Vm::new();
        let table = vm.new_record_table(128).unwrap();
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
        vm.begin_gc_cycle().unwrap();
        let mut work = Work::from([Trace::Value(Value::Table(table))]);
        assert_eq!(vm.trace_some(&mut work, 1).unwrap(), 1);
        assert!(matches!(&work[..], [Trace::TableSlice(_, 129)]));
        vm.gc.work.try_reserve(work.len()).unwrap();
        vm.gc.work.extend(work.iter().copied());
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
        vm.begin_gc_cycle().unwrap();
        let mut work = Work::from([Trace::Value(Value::Table(table))]);
        assert_eq!(vm.trace_some(&mut work, 2).unwrap(), 2);
        assert!(children[64..].iter().all(|&v| !vm.marked_value(v)));
        vm.gc.work.try_reserve(work.len()).unwrap();
        vm.gc.work.extend(work.iter().copied());
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
        vm.begin_gc_cycle().unwrap();
        let table = vm.new_table();
        let root = vm.pin(Value::Table(table)).unwrap();
        // A freshly allocated table is marked but its MarkedValue is still queued.
        let before = vm.gc.work.len();
        vm.set_metatable(Value::Table(table), Some(vm.registry))
            .unwrap();
        vm.set_metatable(Value::Table(table), None).unwrap();
        assert_eq!(vm.gc.work.len(), before);
        finish_marked_cycle(&mut vm);
        vm.begin_gc_cycle().unwrap();
        trace_until(&mut vm, |vm| {
            vm.tables.marked(table.0) && vm.tables[slot_index(table.0)].trace == TableTrace::Idle
        });
        let before = vm.gc.work.len();
        vm.set_metatable(Value::Table(table), Some(vm.registry))
            .unwrap();
        vm.set_metatable(Value::Table(table), None).unwrap();
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
            vm.set_metatable(Value::Table(table), Some(mt)).unwrap();
            let root = vm.pin(Value::Table(table)).unwrap();
            let mut children = Vec::new();
            for i in 1..=1024 {
                let child = Value::Table(vm.new_table());
                vm.raw_set(table, Value::Number(f64::from(i)), child)
                    .unwrap();
                children.push(child);
            }
            vm.begin_gc_cycle().unwrap();
            let mut work = Work::from([Trace::Value(Value::Table(table))]);
            assert_eq!(vm.trace_some(&mut work, 1).unwrap(), 1);
            assert!(children.iter().all(|&v| !vm.marked_value(v)));
            vm.gc.work.try_reserve(work.len()).unwrap();
            vm.gc.work.extend(work.iter().copied());
            if remove {
                vm.set_metatable(Value::Table(table), None).unwrap();
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
        vm.begin_gc_cycle().unwrap();
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
        vm.begin_gc_cycle().unwrap();
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
        vm.begin_gc_cycle().unwrap();
        trace_until(&mut vm, |vm| {
            vm.tables.marked(a.0) && vm.tables.marked(b.0) && vm.tables.marked(mt.0)
        });
        assert!(!vm.marked_value(va) && !vm.marked_value(vb));
        let mode = vm.intern(b"__mode");
        let strong = vm.intern(b"");
        vm.raw_set(mt, mode, strong).unwrap();
        vm.set_metatable(Value::Table(b), None).unwrap();
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
        vm.begin_gc_cycle().unwrap();
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
        vm.begin_gc_cycle().unwrap();
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
        vm.begin_gc_cycle().unwrap();
        assert!(!vm.marked_value(live));
        vm.set_metatable(live, Some(metatable)).unwrap();
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
            vm.begin_gc_cycle().unwrap();
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
        vm.begin_gc_cycle().unwrap();
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
        assert_eq!(pool.sweep_some(&mut cursor, 2, false), 2);
        assert_eq!(pool.intern(&texts[2]), ids[2]);
        let fresh = pool.intern(b"new-after-sweep");
        while cursor != 0 {
            pool.sweep_some(&mut cursor, 2, false);
        }
        for index in [0, 2, 3] {
            assert_eq!(pool.find(&texts[index]), Some(ids[index]));
        }
        for index in [1, 4, 5] {
            assert_eq!(pool.find(&texts[index]), None);
            assert!(!pool.bytes.contains(ids[index].0));
        }
        assert_eq!(pool.find(b"new-after-sweep"), Some(fresh));
        assert_eq!(
            pool.payload_bytes,
            pool.bytes.iter().map(ByteString::storage_bytes).sum()
        );
        pool.bytes.protect_new(false);
        pool.bytes.clear_marks();
        let mut cursor = pool.bytes.len();
        pool.sweep_some(&mut cursor, usize::MAX, false);
        assert_eq!(pool.find(b"new-after-sweep"), None);
        assert_eq!(pool.bytes.live_len(), 0);
    }

    #[test]
    fn packed_substrings_retain_their_terminated_storage_until_the_last_sweep() {
        let mut pool = Strings::default();
        let mut source = [0u8; 32 * 24];
        for (i, chunk) in source.chunks_exact_mut(24).enumerate() {
            chunk.fill(i as u8 + 1);
            chunk[11] = 0;
        }
        let parent = pool.intern(&source);
        let block = pool.pack_substrings(parent, 0, 24, 24, 32, usize::MAX).unwrap();
        let ids: Vec<_> = (0..32)
            .map(|i| pool.intern_packed(&block, 25 * i, 24).unwrap())
            .collect();
        drop(block);
        let kept = ids[17];
        let expected = &source[17 * 24..18 * 24];
        let address = pool.bytes[slot_index(kept.0)].c_bytes().as_ptr();
        assert_eq!(pool.intern(expected), kept);
        pool.bytes.mark(kept.0);
        let mut cursor = pool.bytes.len();
        assert_eq!(pool.sweep_some(&mut cursor, usize::MAX, true), 32);
        pool.rebuild();
        assert!(pool.packed_bytes > 0);
        let bytes = pool.bytes[slot_index(kept.0)].c_bytes();
        assert_eq!(bytes.as_ptr(), address);
        assert_eq!(bytes.split_last(), Some((&0, expected)));

        pool.bytes.clear_marks();
        let mut cursor = pool.bytes.len();
        assert_eq!(pool.sweep_some(&mut cursor, usize::MAX, true), 1);
        pool.rebuild();
        assert_eq!(pool.packed_bytes, 0);
        assert!(pool.packed.is_empty());
    }

    #[test]
    fn string_index_widens_in_place_and_keeps_high_slots_after_rebuild() {
        let mut pool = Strings::default();
        let first = pool.intern(b"first");
        for n in 1..Strings::BUCKET_INDEX_MASK {
            pool.intern(&n.to_le_bytes());
        }
        let capacity = pool.buckets.len();
        let last = pool.intern(b"last");
        assert_eq!(pool.buckets.len(), capacity);
        assert_eq!(slot_index(last.0), Strings::BUCKET_INDEX_MASK as usize);
        assert_eq!(pool.find(b"first"), Some(first));
        assert_eq!(pool.find(b"last"), Some(last));

        pool.bytes.mark(first.0);
        pool.bytes.mark(last.0);
        let mut cursor = pool.bytes.len();
        pool.sweep_some(&mut cursor, usize::MAX, false);
        assert_eq!(pool.buckets.len(), capacity);
        assert_eq!(pool.find(b"first"), Some(first));
        assert_eq!(pool.find(b"last"), Some(last));
        assert_eq!(pool.find(&1u32.to_le_bytes()), None);
        pool.rebuild();
        assert!(pool.buckets.len() < capacity);
        assert_eq!(
            pool.payload_bytes,
            pool.bytes.iter().map(ByteString::storage_bytes).sum()
        );
        assert_eq!(pool.intern(b"first"), first);
        assert_eq!(pool.intern(b"last"), last);
        assert_eq!(pool.find(&1u32.to_le_bytes()), None);
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
        vm.begin_gc_cycle().unwrap();
        vm.finish_gc_marking(false).unwrap();
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
    fn private_host_state_write_traces_white_graph_from_a_black_owner() {
        let mut vm = Vm::new();
        let owner = vm.new_table();
        let root = vm.pin(Value::Table(owner)).unwrap();
        let state = vm.new_table();
        let leaf = vm.new_table();
        vm.raw_table_set(state, Value::Number(1.), Value::Table(leaf))
            .unwrap();
        vm.begin_gc_cycle().unwrap();
        trace_until(&mut vm, |vm| {
            vm.tables.marked(owner.0) && vm.tables[slot_index(owner.0)].trace == TableTrace::Idle
        });
        assert!(!vm.marked_value(Value::Table(state)));
        vm.set_table_host_state(owner, Some(state)).unwrap();
        finish_marked_cycle(&mut vm);
        assert!(vm.validate_value(Value::Table(state)).is_ok());
        assert!(vm.validate_value(Value::Table(leaf)).is_ok());
        vm.set_table_host_state(owner, None).unwrap();
        vm.collect_garbage().unwrap();
        assert!(vm.validate_value(Value::Table(state)).is_err());
        assert!(vm.validate_value(Value::Table(leaf)).is_err());
        vm.unpin(root).unwrap();
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
        vm.begin_gc_cycle().unwrap();
        vm.finish_gc_marking(false).unwrap();
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
