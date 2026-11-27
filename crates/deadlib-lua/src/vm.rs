//! Register dispatch, interned byte strings, and native Lua tables.

use crate::{
    Error, ErrorKind, FunctionId, ParserPrototypeId, Program, StringId, TableId, ThreadId,
    UserdataId, Value,
    bytecode::{COMPARE_BRANCH, Constant, OPEN, OPEN_ARGS, Op, ProgramData, Upvalue},
    locale::parse_number,
    table::Table,
};
#[cfg(test)]
mod binding_tests;
mod console;
mod debug;
mod errors;
mod forwarding;
mod fresh_triplet;
mod gc;
mod hooks;
mod host;
mod identity;
pub(crate) mod io;
mod limits;
mod loads;
mod math;
mod memory_limit;
mod meta;
mod native;
mod native_fast;
mod numeric_leaf;
mod os;
mod package;
mod radix_selector;
mod reductions;
mod return_argument;
mod simple;
mod standalone;
mod strings;
mod table_literals;
mod table_search;
mod tables;
mod threads;
mod traces;
mod userdata;
use crate::arena::{Arena, index as slot_index};
use crate::memory::{Buffer, Memory, Shared};
pub use gc::{GcOperation, HeapStats, Root};
pub use host::{
    HostCall, HostFunction, HostThreadState, IndexResolver, IndexView, NativeBuiltinBody,
    NativeCallStatus, NativeDebugFrame, NativeDebugInfo, NativeDebugString, NativeHook,
    NativeHookEvent, NativeHookInfo, NativeLoadStatus, NativeStack, NativeThreadStack,
    NumberFields, TripletFieldResolver, TripletResolver,
};
pub use memory_limit::MemoryUsage;
use native::format_number;
pub use threads::NativeThreadStatus;

// Lua 5.1's lua_Integer is ptrdiff_t. The supported SSE2/MSVC profiles
// truncate valid numbers and return the native minimum for invalid ranges.
fn lua_integer(number: f64) -> i64 {
    let minimum = isize::MIN as f64;
    if (minimum..-minimum).contains(&number) {
        number as isize as i64
    } else {
        isize::MIN as i64
    }
}

/// Stable Lua string storage, including the C API's trailing zero byte.
#[derive(Debug)]
struct ByteString {
    storage: StringStorage,
    hash: usize,
}

#[derive(Debug)]
enum StringStorage {
    Owned(Buffer<u8>),
    Packed(Shared<[u8]>),
}

impl ByteString {
    fn try_new(bytes: &[u8], hash: usize, memory: Memory) -> Result<Self, Error> {
        let size = bytes.len().checked_add(1).ok_or_else(Error::memory)?;
        let mut storage = Buffer::new_in(memory);
        storage
            .try_reserve_exact(size)
            .map_err(|_| Error::memory())?;
        storage.extend_from_slice(bytes);
        storage.push(0);
        // Keep the vector: shrinking an overallocated buffer to a boxed slice
        // must not introduce an infallible reallocation on this failure path.
        Ok(Self {
            storage: StringStorage::Owned(storage),
            hash,
        })
    }
    fn c_bytes(&self) -> &[u8] {
        match &self.storage {
            StringStorage::Owned(storage) => storage,
            StringStorage::Packed(storage) => storage,
        }
    }
    fn storage_bytes(&self) -> usize {
        match &self.storage {
            StringStorage::Owned(storage) => storage.capacity(),
            StringStorage::Packed(_) => 0,
        }
    }
}
impl std::ops::Deref for ByteString {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        let bytes = self.c_bytes();
        &bytes[..bytes.len() - 1]
    }
}
impl AsRef<[u8]> for ByteString {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

#[derive(Debug, Default)]
struct Strings {
    bytes: Arena<ByteString>,
    reclaimed_epoch: u64,
    buckets: Buffer<u32>,
    payload_bytes: usize,
    // The owner list charges shared payloads once and releases empty blocks
    // during sweep. Each interned string retains its own terminated view.
    packed: Buffer<Shared<Buffer<u8>>>,
    packed_bytes: usize,
}

impl Strings {
    // Arena slots are one-based in buckets. Use the remaining bits for a hash
    // tag until the arena's high-water mark requires the entire word.
    const BUCKET_INDEX_MASK: u32 = (1 << if cfg!(test) { 10 } else { 24 }) - 1;

    fn new_in(memory: Memory) -> Self {
        Self {
            bytes: Arena::new_in(memory.clone()),
            reclaimed_epoch: 0,
            buckets: Buffer::new_in(memory.clone()),
            payload_bytes: 0,
            packed: Buffer::new_in(memory),
            packed_bytes: 0,
        }
    }
    fn hash(bytes: &[u8]) -> usize {
        crate::hash::hash_bytes(bytes) as usize
    }
    fn find(&self, bytes: &[u8]) -> Option<StringId> {
        self.probe(bytes, Self::hash(bytes)).ok()
    }
    // Return the vacant bucket as well as hits so insertion can reuse the probe.
    fn probe(&self, bytes: &[u8], hash: usize) -> Result<StringId, usize> {
        if self.buckets.is_empty() {
            return Err(0);
        }
        let mask = self.buckets.len() - 1;
        let index_mask = self.bucket_index_mask();
        let tag = hash as u32 & !index_mask;
        let mut slot = hash & mask;
        loop {
            let entry = self.buckets[slot];
            if entry == 0 {
                return Err(slot);
            }
            if entry & !index_mask == tag {
                let index = (entry & index_mask) as usize - 1;
                let candidate = &self.bytes[index];
                if candidate.hash == hash && candidate.as_ref() == bytes {
                    return Ok(StringId(
                        self.bytes.key(index).expect("live interned string"),
                    ));
                }
            }
            slot = (slot + 1) & mask;
        }
    }
    fn bucket_index_mask(&self) -> u32 {
        if self.bytes.len() <= Self::BUCKET_INDEX_MASK as usize {
            Self::BUCKET_INDEX_MASK
        } else {
            u32::MAX
        }
    }
    fn place_bucket(buckets: &mut [u32], index: usize, hash: usize, index_mask: u32) {
        let mask = buckets.len() - 1;
        let mut slot = hash & mask;
        while buckets[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        buckets[slot] = (index as u32 + 1) | (hash as u32 & !index_mask);
    }
    fn try_add(&mut self, bytes: ByteString, vacant: usize) -> Result<StringId, Error> {
        let hash = bytes.hash;
        let old_index_mask = self.bucket_index_mask();
        let mut replacement = None;
        if (self.bytes.live_len() + 1) * 4 >= self.buckets.len() * 3 {
            let size = self
                .buckets
                .len()
                .checked_mul(2)
                .ok_or_else(Error::memory)?
                .max(16);
            let mut buckets = Buffer::new_in(self.bytes.memory().clone());
            buckets
                .try_reserve_exact(size)
                .map_err(|_| Error::memory())?;
            buckets.resize(size, 0);
            for (id, text) in self.bytes.entries() {
                Self::place_bucket(&mut buckets, id, text.hash, old_index_mask);
            }
            replacement = Some(buckets);
        }
        let storage = bytes.storage_bytes();
        // Publish neither the hash index nor the string until every allocation
        // has succeeded. Arena accounting includes partially reserved buffers.
        let id = self.bytes.try_insert(bytes)?;
        let resized = replacement.is_some();
        if let Some(buckets) = replacement {
            let growth =
                buckets.capacity().saturating_sub(self.buckets.capacity()) * size_of::<u32>();
            self.bytes.note_alloc(growth);
            self.buckets = buckets;
        }
        let index_mask = self.bucket_index_mask();
        if index_mask != old_index_mask {
            // Widen the index in place even if only the arena needed to grow.
            for entry in &mut *self.buckets {
                *entry &= old_index_mask;
            }
        }
        self.bytes.note_alloc(storage);
        self.payload_bytes += storage;
        if resized {
            Self::place_bucket(&mut self.buckets, slot_index(id), hash, index_mask);
        } else {
            self.buckets[vacant] = (slot_index(id) as u32 + 1) | (hash as u32 & !index_mask);
        }
        Ok(StringId(id))
    }
    #[cfg(test)]
    fn intern(&mut self, bytes: &[u8]) -> StringId {
        self.try_intern(bytes).expect("allocate Lua string")
    }
    fn try_intern(&mut self, bytes: &[u8]) -> Result<StringId, Error> {
        let hash = Self::hash(bytes);
        match self.probe(bytes, hash) {
            Ok(id) => {
                self.bytes.preserve_new_reference(id.0);
                Ok(id)
            }
            Err(vacant) => {
                let owned = ByteString::try_new(bytes, hash, self.bytes.memory().clone())?;
                self.try_add(owned, vacant)
            }
        }
    }
    fn try_intern_owned(&mut self, mut bytes: Buffer<u8>) -> Result<StringId, Error> {
        let hash = Self::hash(&bytes);
        match self.probe(&bytes, hash) {
            Ok(id) => {
                self.bytes.preserve_new_reference(id.0);
                Ok(id)
            }
            Err(vacant) => {
                debug_assert!(bytes.capacity() > bytes.len());
                bytes.push(0);
                self.try_add(
                    ByteString {
                        storage: StringStorage::Owned(bytes),
                        hash,
                    },
                    vacant,
                )
            }
        }
    }
    fn substring(
        &mut self,
        id: StringId,
        range: std::ops::Range<usize>,
    ) -> Result<StringId, Error> {
        let slot = slot_index(id.0);
        let source = &self.bytes[slot];
        let bytes = &source[range];
        if bytes.len() == source.len() {
            let key = self.bytes.key(slot).expect("live Lua string slot");
            self.bytes.preserve_new_reference(key);
            return Ok(StringId(key));
        }
        let hash = Self::hash(bytes);
        match self.probe(bytes, hash) {
            Ok(id) => {
                self.bytes.preserve_new_reference(id.0);
                Ok(id)
            }
            Err(vacant) => {
                let owned = ByteString::try_new(bytes, hash, self.bytes.memory().clone())?;
                self.try_add(owned, vacant)
            }
        }
    }

    fn pack_substrings(
        &mut self,
        source: StringId,
        first: i64,
        step: i64,
        width: usize,
        count: usize,
        growth: usize,
    ) -> Option<Shared<Buffer<u8>>> {
        let bytes = &self.bytes[slot_index(source.0)];
        let size = (width + 1) * count;
        // Prefer ordinary lookups when this span is already interned or is
        // near a collection boundary. Account for the exact charge below.
        if size + size_of::<ByteString>() * count + 256 > growth
            || self
                .find(&bytes[first as usize..first as usize + width])
                .is_some()
        {
            return None;
        }
        let before = self.packed.capacity();
        self.packed.try_reserve(1).ok()?;
        self.bytes
            .note_alloc((self.packed.capacity() - before) * size_of::<Shared<Buffer<u8>>>());
        let mut data = Buffer::new_in(self.bytes.memory().clone());
        data.try_reserve_exact(size).ok()?;
        let bytes = &self.bytes[slot_index(source.0)];
        let mut at = first;
        for _ in 0..count {
            data.extend_from_slice(&bytes[at as usize..at as usize + width]);
            data.push(0);
            at += step;
        }
        let block = Shared::try_new_in(data, self.bytes.memory().clone()).ok()?;
        let bytes = block.capacity() + block.heap_bytes();
        self.packed_bytes += bytes;
        self.bytes.note_alloc(bytes);
        self.packed.push(block.clone());
        Some(block)
    }

    fn intern_packed(
        &mut self,
        block: &Shared<Buffer<u8>>,
        start: usize,
        width: usize,
    ) -> Result<StringId, Error> {
        let end = start + width;
        let bytes = &block[start..end];
        let hash = Self::hash(bytes);
        match self.probe(bytes, hash) {
            Ok(id) => {
                self.bytes.preserve_new_reference(id.0);
                Ok(id)
            }
            Err(vacant) => self.try_add(
                ByteString {
                    storage: StringStorage::Packed(
                        block.clone().project(|data| &data[start..=end]),
                    ),
                    hash,
                },
                vacant,
            ),
        }
    }

    fn sweep_packed(&mut self) {
        let retained = &mut self.packed_bytes;
        self.packed.retain(|block| {
            if block.is_sole_owner() {
                *retained -= block.capacity() + block.heap_bytes();
                false
            } else {
                true
            }
        });
    }
}

/// Strings interned from `&'static` slices, found again by the slice's address.
///
/// A `&'static` slice names memory that never changes, so one address and
/// length always hold the same bytes and identify the string without reading
/// them. The collector marks every entry at the start of a cycle.
#[derive(Debug)]
struct StaticStrings {
    // Open addressing over (address, length, string). No slice starts at
    // address zero, which marks a vacant slot.
    slots: Buffer<(usize, usize, StringId)>,
    len: usize,
}
impl StaticStrings {
    fn new_in(memory: Memory) -> Self {
        Self {
            slots: Buffer::new_in(memory),
            len: 0,
        }
    }
    #[inline]
    fn start(&self, address: usize, len: usize) -> usize {
        // Literals lie close together; multiply so that the slot comes from
        // high bits which every address bit reaches.
        let mixed =
            (address ^ len.rotate_left(20)).wrapping_mul(0x9e37_79b9_7f4a_7c15_u64 as usize);
        mixed >> (usize::BITS - self.slots.len().trailing_zeros())
    }
    #[inline]
    fn find(&self, bytes: &'static [u8]) -> Option<StringId> {
        if self.slots.is_empty() {
            return None;
        }
        let (address, len) = (bytes.as_ptr() as usize, bytes.len());
        let mut slot = self.start(address, len);
        loop {
            let (other, other_len, id) = self.slots[slot];
            if other == address && other_len == len {
                return Some(id);
            }
            if other == 0 {
                return None;
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }
    // Make room for one more entry, so that `insert` cannot fail.
    fn try_reserve(&mut self) -> Result<(), Error> {
        if (self.len + 1) * 2 <= self.slots.len() {
            return Ok(());
        }
        let size = self
            .slots
            .len()
            .checked_mul(2)
            .ok_or_else(Error::memory)?
            .max(64);
        let mut slots = Buffer::new_in(self.slots.memory().clone());
        slots.try_reserve_exact(size)?;
        slots.resize(size, (0, 0, StringId(0)));
        let old = std::mem::replace(&mut self.slots, slots);
        for &(address, len, id) in old.iter().filter(|slot| slot.0 != 0) {
            self.place(address, len, id);
        }
        Ok(())
    }
    fn place(&mut self, address: usize, len: usize, id: StringId) {
        let mut slot = self.start(address, len);
        while self.slots[slot].0 != 0 {
            slot = (slot + 1) & (self.slots.len() - 1);
        }
        self.slots[slot] = (address, len, id);
    }
    fn insert(&mut self, bytes: &'static [u8], id: StringId) {
        self.place(bytes.as_ptr() as usize, bytes.len(), id);
        self.len += 1;
    }
    fn strings(&self) -> impl Iterator<Item = StringId> + '_ {
        self.slots
            .iter()
            .filter(|slot| slot.0 != 0)
            .map(|slot| slot.2)
    }
    fn heap_bytes(&self) -> usize {
        self.slots.capacity() * size_of::<(usize, usize, StringId)>()
    }
}

#[derive(Debug)]
struct LoadedProgram {
    data: crate::memory::Shared<ProgramData>,
    _budget: crate::memory::Reservation,
    constants: Buffer<Buffer<Value>>,
    upvalue_names: Buffer<Buffer<StringId>>,
    debug_strings: Buffer<StringId>,
    // Non-owning prototype map. Each live closure traces only its own subtree.
    parser_prototypes: Buffer<ParserPrototypeId>,
    entry: FunctionId,
}

impl LoadedProgram {
    #[inline]
    fn bound_constants(&self, prototype: usize) -> &[Value] {
        let owner = self.data.prototypes[prototype]
            .shared_owner()
            .map_or(prototype, |owner| owner as usize);
        &self.constants[owner]
    }
}

#[derive(Debug)]
struct Closure {
    program: usize,
    prototype: usize,
    upvalues: Buffer<u32>,
    env: TableId,
}

#[derive(Debug)]
enum Cell {
    Open(ThreadId, usize),
    Closed(Value),
}

// Numeric native IDs are shared by globals and the explicit continuation frames.
const NEXT: u64 = 9;
const PAIRS: u64 = 10;
const IPAIRS: u64 = 11;
const IPAIRS_STEP: u64 = 12;
const GET_META: u64 = 13;
const SET_META: u64 = 14;
const PCALL: u64 = 15;
const XPCALL: u64 = 16;
const TOSTRING: u64 = 17;
const PRINT: u64 = 18;
const DEBUG_GET_META: u64 = 19;
const DEBUG_SET_META: u64 = 20;
const PAIRS_NEXT: u64 = 21;
const GET_ENV: u64 = 22;
const SET_ENV: u64 = 23;
const LOAD_STRING: u64 = 24;
const DEBUG_GET_ENV: u64 = 25;
const DEBUG_SET_ENV: u64 = 26;
const COLLECT_GARBAGE: u64 = 27;
const GC_INFO: u64 = 28;
const NEW_PROXY: u64 = 29;
const LOAD_READER: u64 = 30;
const LOAD_FILE: u64 = 31;
const DO_FILE: u64 = 121;
const CONT_RAW: u64 = 256;
const CONT_BOOL: u64 = 257;
const CONT_NOT: u64 = 258;
const CONT_IGNORE: u64 = 259;
const CONT_HANDLER: u64 = 260;
const CONT_GC: u64 = 261;
const CONT_HOOK: u64 = 262;
const CONT_TRACE: u64 = 263;
const CONT_RETURN: u64 = 264;
const CONT_FILE: u64 = 265;
const CONT_CONCAT: u64 = 266;
const CONT_STANDALONE: u64 = 267;
const CONT_HOST_CALL: u64 = 268;
const CONT_HOST_META: u64 = 269;
const CONT_HOST_PCALL: u64 = 270;
const CONT_HOST_CONCAT: u64 = 271;
const CONT_HOST_LOAD: u64 = 272;
const CONT_NATIVE_HOOK: u64 = 273;
const CONT_NATIVE_WINDOW: u64 = 274;

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum Meta {
    Index,
    NewIndex,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Unm,
    Len,
    Concat,
    Eq,
    Lt,
    Le,
    Call,
    ToString,
    Protected,
    Mode,
    Gc,
}
const META_NAMES: [&[u8]; 19] = [
    b"__index",
    b"__newindex",
    b"__add",
    b"__sub",
    b"__mul",
    b"__div",
    b"__mod",
    b"__pow",
    b"__unm",
    b"__len",
    b"__concat",
    b"__eq",
    b"__lt",
    b"__le",
    b"__call",
    b"__tostring",
    b"__metatable",
    b"__mode",
    b"__gc",
];

#[derive(Clone, Copy, Debug)]
enum FrameKind {
    Lua(FunctionId),
    Native(u64),
}

#[derive(Clone, Copy, Debug)]
struct Frame {
    function: FrameKind,
    pc: u32,
    base: usize,
    stack_start: usize,
    result: usize,
    wanted: u16,
    // C-boundary depth at entry, including this frame when it is native.
    barriers: u16,
    vararg_start: usize,
    vararg_count: u32,
    tail_calls: u32,
    c_level_bias: u16,
}
impl Frame {
    fn is_call(self) -> bool {
        !matches!(self.function, FrameKind::Native(id) if (CONT_RAW..strings::ITERATOR_FIRST).contains(&id))
    }
    fn pc(self) -> usize {
        self.pc as usize
    }
    fn argc(self) -> usize {
        self.vararg_count as usize
    }
}

const FRAME_OPEN: u16 = u16::MAX;
fn frame_wanted(wanted: u32) -> u16 {
    if wanted == OPEN {
        FRAME_OPEN
    } else {
        u16::try_from(wanted).expect("verified result count fits a frame")
    }
}
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Frame>() == 72);

/// A Lua state with persistent globals and reusable contiguous register storage.
///
/// Object values must only be used with the VM that produced them. Pin values
/// with [`Self::pin`] when the host needs them across executions or collections.
#[derive(Debug)]
pub struct Vm {
    lua51_table_order: bool,
    registers: Buffer<Value>,
    programs: Arena<LoadedProgram>,
    functions: Arena<Closure>,
    // Heap payload of tables, userdata, closures, and program objects.
    object_payload_bytes: usize,
    upvalues: Arena<Cell>,
    open_upvalues: Buffer<u32>,
    frames: Buffer<Frame>,
    call_slots: u16,
    strings: Strings,
    tables: Arena<Table>,
    globals: TableId,
    registry: TableId,
    top: usize,
    budget: Option<u64>,
    remaining: u64,
    type_metatables: [Option<TableId>; 9],
    meta_names: [StringId; 19],
    memory_error: StringId,
    output: Option<Vec<u8>>,
    active_native: Option<usize>,
    // A synchronous protected host invocation can also drive child threads.
    native_protected: bool,
    api_calls: Buffer<host::ApiCall>,
    native_bodies: Buffer<host::Body>,
    patterns: crate::pattern::Cache,
    pattern_work: crate::pattern::Work,
    // Parser and code generator storage reused by unbudgeted source loads.
    compile_spare: loads::Spare,
    array_reduction: reductions::while_numeric::ArrayReduction,
    pattern_locale: u64,
    native_closures: Arena<threads::NativeClosure>,
    builtin_envs: Buffer<Option<TableId>>,
    substitutions: Buffer<strings::Substitution>,
    sorts: Buffer<tables::Sort>,
    buffers: Buffer<Buffer<u8>>,
    plain_format: Option<(StringId, Option<strings::PlainFormat>)>,
    threads: Arena<threads::Thread>,
    main: ThreadId,
    initial_globals: TableId,
    thread: ThreadId,
    transition: Option<threads::Transition>,
    gc: gc::Collector,
    roots: Arena<Value>,
    identity: u64,
    parser_prototypes: Arena<loads::ParserPrototype>,
    loads: Buffer<loads::Loading>,
    tracebacks: Buffer<traces::Traceback>,
    hooks: hooks::Hooks,
    userdata: Arena<userdata::Userdata>,
    initializing: bool,
    io: io::Io,
    userdata_order: u64,
    standalone: u64,
    // Completed scalar returns, consumed immediately by native dispatch.
    sort_line: Option<u32>,
    // Embedding-owned state borrowed by host callbacks; never traced.
    host_state: Option<Box<dyn std::any::Any + Send + Sync>>,
    static_strings: StaticStrings,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    /// Create a state with the currently implemented Lua 5.1 libraries.
    ///
    /// # Panics
    /// Panics if initialization fails. Use [`Self::try_new`] for recovery.
    pub fn new() -> Self {
        Self::try_new().expect("allocate Lua state")
    }

    /// Create a Lua state without opening its standard libraries.
    /// Its globals and registry start empty, and strings have no metatable.
    /// The compiler and core language operations remain available to the host.
    ///
    /// # Panics
    /// Panics if allocation fails. Use [`Self::try_new_bare`] for recovery.
    pub fn new_bare() -> Self {
        Self::try_new_bare().expect("allocate Lua state")
    }

    /// Create a state and open its standard libraries with fallible allocation.
    ///
    /// # Errors
    /// Returns a memory error if initialization cannot allocate. The error has
    /// no Lua value, because the unfinished state has been released. Platform
    /// errors reading package paths also release the unfinished state.
    /// This does not make subsequent compilation or execution wholly fallible.
    /// Recovery is supported on the documented Windows and Linux targets.
    pub fn try_new() -> Result<Self, Error> {
        let mut vm = Self::try_new_bare()?;
        vm.initializing = true;
        vm.open_libraries().map_err(Error::without_vm_values)?;
        vm.initializing = false;
        Ok(vm)
    }

    /// Open a state whose table traversal follows Lua 5.1's node placement.
    ///
    /// Useful for existing scripts that build ordered side effects with `pairs`.
    /// String/number/boolean keys follow the reference hash and resize rules;
    /// object-key ordering uses VM identities instead of native pointer addresses.
    /// Ordinary states retain the compact insertion-ordered representation.
    pub fn try_new_with_lua51_table_order() -> Result<Self, Error> {
        let mut vm = Self::try_new_bare()?;
        vm.lua51_table_order = true;
        for id in [vm.globals, vm.registry] {
            vm.tables[slot_index(id.0)] =
                Table::with_native51_capacity(0, 0, vm.tables.memory().clone())?;
        }
        vm.initializing = true;
        vm.open_libraries().map_err(Error::without_vm_values)?;
        vm.initializing = false;
        Ok(vm)
    }

    /// Create a state without standard libraries, returning allocation errors.
    ///
    /// # Errors
    /// Returns a memory error with no Lua value if state allocation fails.
    pub fn try_new_bare() -> Result<Self, Error> {
        Self::try_bare_in(Memory::default()).map_err(Error::without_vm_values)
    }

    pub(crate) fn try_bare_in(memory: Memory) -> Result<Self, Error> {
        let mut tables = Arena::identified_in(memory.clone());
        let globals = TableId(tables.try_insert(Table::new_in(memory.clone()))?);
        let registry = TableId(tables.try_insert(Table::new_in(memory.clone()))?);
        let mut threads = Arena::identified_in(memory.clone());
        let main = ThreadId(threads.try_insert(threads::Thread::main(globals, memory.clone()))?);
        // Builtin IDs fit the collector's four-word bitmap. This fixed buffer
        // also supplies their stable per-VM opaque pointer identities.
        let mut builtin_envs = Buffer::new_in(memory.clone());
        builtin_envs.try_reserve_exact(CONT_RAW as usize)?;
        builtin_envs.resize(CONT_RAW as usize, None);
        let mut vm = Self {
            lua51_table_order: false,
            registers: Buffer::new_in(memory.clone()),
            programs: Arena::new_in(memory.clone()),
            functions: Arena::identified_in(memory.clone()),
            object_payload_bytes: 0,
            upvalues: Arena::new_in(memory.clone()),
            open_upvalues: Buffer::new_in(memory.clone()),
            frames: Buffer::new_in(memory.clone()),
            call_slots: 8,
            strings: Strings::new_in(memory.clone()),
            tables,
            globals,
            registry,
            top: 0,
            budget: None,
            remaining: 0,
            type_metatables: [None; 9],
            meta_names: [StringId(0); 19],
            memory_error: StringId(0),
            output: None,
            active_native: None,
            native_protected: false,
            api_calls: Buffer::new_in(memory.clone()),
            native_bodies: Buffer::new_in(memory.clone()),
            patterns: crate::pattern::Cache::new_in(memory.clone()),
            pattern_work: crate::pattern::Work::new_in(memory.clone()),
            compile_spare: loads::Spare::default(),
            array_reduction: reductions::while_numeric::ArrayReduction::new_in(memory.clone()),
            pattern_locale: crate::locale::revision(),
            native_closures: Arena::identified_in(memory.clone()),
            builtin_envs,
            substitutions: Buffer::new_in(memory.clone()),
            sorts: Buffer::new_in(memory.clone()),
            buffers: Buffer::new_in(memory.clone()),
            plain_format: None,
            threads,
            thread: main,
            main,
            initial_globals: globals,
            transition: None,
            gc: gc::Collector::new_in(memory.clone()),
            roots: Arena::new_in(memory.clone()),
            identity: gc::new_vm_identity(),
            parser_prototypes: Arena::new_in(memory.clone()),
            loads: Buffer::new_in(memory.clone()),
            tracebacks: Buffer::new_in(memory.clone()),
            hooks: hooks::Hooks::new_in(memory.clone()),
            userdata: Arena::new_in(memory.clone()),
            initializing: true,
            io: io::Io {
                files: Arena::new_in(memory.clone()),
                ..io::Io::default()
            },
            userdata_order: 0,
            standalone: 0,
            sort_line: None,
            host_state: None,
            static_strings: StaticStrings::new_in(memory),
        };
        vm.memory_error = vm.strings.try_intern(b"not enough memory")?;
        for (slot, name) in vm.meta_names.iter_mut().zip(META_NAMES) {
            *slot = vm.strings.try_intern(name)?;
        }
        // An unprotected allocation error must be publishable without allocating
        // again, even when the state's main stack has never held a value.
        vm.registers.try_reserve_exact(1)?;
        vm.reset_gc_debt();
        vm.initializing = false;
        Ok(vm)
    }

    fn open_libraries(&mut self) -> Result<(), Error> {
        let globals = self.globals;
        let registry = self.registry;
        for (id, name) in [
            b"assert".as_slice(),
            b"error",
            b"type",
            b"tonumber",
            b"rawequal",
            b"rawget",
            b"rawset",
            b"select",
            b"unpack",
        ]
        .iter()
        .enumerate()
        {
            self.set_global(name, Value::Native(id as u64))?;
        }
        for (id, name) in [
            (NEXT, b"next".as_slice()),
            (PAIRS, b"pairs"),
            (IPAIRS, b"ipairs"),
            (GET_META, b"getmetatable"),
            (SET_META, b"setmetatable"),
            (PCALL, b"pcall"),
            (XPCALL, b"xpcall"),
            (TOSTRING, b"tostring"),
            (PRINT, b"print"),
            (GET_ENV, b"getfenv"),
            (SET_ENV, b"setfenv"),
            (LOAD_STRING, b"loadstring"),
            (COLLECT_GARBAGE, b"collectgarbage"),
            (GC_INFO, b"gcinfo"),
            (NEW_PROXY, b"newproxy"),
            (LOAD_READER, b"load"),
            (LOAD_FILE, b"loadfile"),
            (DO_FILE, b"dofile"),
        ] {
            let function = match id {
                PAIRS | IPAIRS => self.new_native(
                    threads::NativeKind::Builtin(id),
                    &[Value::Native(if id == PAIRS {
                        PAIRS_NEXT
                    } else {
                        IPAIRS_STEP
                    })],
                )?,
                NEW_PROXY => {
                    let valid = self.try_new_table()?;
                    self.set_metatable(Value::Table(valid), Some(valid))?;
                    let mode = self.try_intern(b"__mode")?;
                    let weak = self.try_intern(b"kv")?;
                    self.raw_set(valid, mode, weak)?;
                    self.new_native(threads::NativeKind::Builtin(id), &[Value::Table(valid)])?
                }
                _ => Value::Native(id),
            };
            self.set_global(name, function)?;
        }
        let debug = self.try_new_table()?;
        for (id, name) in [
            (DEBUG_GET_META, b"getmetatable".as_slice()),
            (DEBUG_SET_META, b"setmetatable"),
            (DEBUG_GET_ENV, b"getfenv"),
            (DEBUG_SET_ENV, b"setfenv"),
            (debug::GET_INFO, b"getinfo"),
            (debug::GET_UPVALUE, b"getupvalue"),
            (debug::SET_UPVALUE, b"setupvalue"),
            (debug::GET_LOCAL, b"getlocal"),
            (debug::SET_LOCAL, b"setlocal"),
            (debug::TRACEBACK, b"traceback"),
            (hooks::GET_HOOK, b"gethook"),
            (hooks::SET_HOOK, b"sethook"),
            (debug::GET_REGISTRY, b"getregistry"),
            (debug::CONSOLE, b"debug"),
        ] {
            let key = self.try_intern(name)?;
            self.raw_set(debug, key, Value::Native(id))?;
        }
        self.set_global(b"debug", Value::Table(debug))?;
        self.set_global(b"_G", Value::Table(globals))?;
        let version = self.try_intern(b"Lua 5.1")?;
        self.set_global(b"_VERSION", version)?;
        self.open_math()?;
        self.open_strings()?;
        self.open_tables()?;
        self.open_threads()?;
        self.open_io()?;
        self.open_os()?;
        let loaded = self.try_new_table()?;
        let key = self.try_intern(b"_LOADED")?;
        self.raw_set(registry, key, Value::Table(loaded))?;
        for name in [
            b"_G".as_slice(),
            b"math",
            b"string",
            b"table",
            b"debug",
            b"coroutine",
            b"io",
            b"os",
        ] {
            let name = self.try_intern(name)?;
            let value = self.tables[slot_index(self.globals.0)].get(name);
            self.raw_set(loaded, name, value)?;
        }
        self.open_package(loaded)?;
        self.reset_gc_debt();
        Ok(())
    }

    /// Set an optional instruction limit for each call to [`Self::execute`].
    pub fn set_instruction_limit(&mut self, limit: Option<u64>) {
        self.budget = limit;
    }

    /// Capture logical bytes from `print` and I/O writes to stdout.
    pub fn set_output_capture(&mut self, enabled: bool) {
        self.output = enabled.then(Vec::new);
    }

    /// Borrow captured output, or `None` when stdout is used directly.
    pub fn output(&self) -> Option<&[u8]> {
        self.output.as_deref()
    }

    /// Intern an arbitrary byte string in this VM.
    pub fn intern(&mut self, bytes: &[u8]) -> Value {
        self.try_intern(bytes).expect("allocate Lua string")
    }

    /// Intern bytes without aborting when string storage cannot be allocated.
    /// Existing strings require no allocation. Store/root the returned value
    /// before any collection, as with [`Self::intern`].
    ///
    /// # Errors
    /// Returns the fixed Lua memory error without publishing a partial string.
    pub fn try_intern(&mut self, bytes: &[u8]) -> Result<Value, Error> {
        self.strings
            .try_intern(bytes)
            .map(Value::String)
            .map_err(|_| self.memory_failure())
    }

    /// Intern a byte string that lives for the whole program, such as a literal.
    ///
    /// The first call with a slice interns it and keeps the string alive for
    /// as long as this VM exists. Every later call with the same slice finds
    /// the string by the slice's address and length, without hashing or
    /// comparing its bytes, so a field name spelled as a literal costs a few
    /// instructions to resolve. The returned value needs no rooting.
    ///
    /// Equal bytes at different addresses yield the same string; each address
    /// is remembered separately. Keep this for a bounded set of names: every
    /// string interned here stays allocated until the VM is dropped.
    ///
    /// ```
    /// use deadlib_lua::{Value, Vm};
    /// let mut vm = Vm::new();
    /// let name = vm.intern_static(b"position")?;
    /// assert_eq!(vm.intern_static(b"position")?, name);
    /// assert_eq!(vm.intern(b"position"), name);
    /// # Ok::<(), deadlib_lua::Error>(())
    /// ```
    ///
    /// # Errors
    /// Returns the fixed Lua memory error if the string or its index entry
    /// cannot be allocated.
    #[inline]
    pub fn intern_static(&mut self, bytes: &'static [u8]) -> Result<Value, Error> {
        match self.static_strings.find(bytes) {
            Some(id) => Ok(Value::String(id)),
            None => self.remember_static(bytes),
        }
    }
    #[cold]
    fn remember_static(&mut self, bytes: &'static [u8]) -> Result<Value, Error> {
        if self.static_strings.try_reserve().is_err() {
            return Err(self.memory_failure());
        }
        // Interning marks the string for a collection in progress; every
        // later cycle marks it from the index.
        let id = match self.strings.try_intern(bytes) {
            Ok(id) => id,
            Err(_) => return Err(self.memory_failure()),
        };
        self.static_strings.insert(bytes, id);
        Ok(Value::String(id))
    }

    fn string_slice(
        &mut self,
        id: StringId,
        range: std::ops::Range<usize>,
    ) -> Result<Value, Error> {
        self.strings
            .substring(id, range)
            .map(Value::String)
            .map_err(|_| self.memory_failure())
    }
    fn intern_number(&mut self, number: f64) -> Result<Value, Error> {
        let text = native::number_text(number);
        let mut bytes = self.try_take_buffer(text.as_bytes().len())?;
        bytes.extend_from_slice(text.as_bytes());
        crate::locale::try_localize_owned(&mut bytes)?;
        self.try_finish_buffer(bytes)
    }
    fn try_take_buffer(&mut self, capacity: usize) -> Result<Buffer<u8>, Error> {
        self.buffers
            .try_reserve(1)
            .map_err(|_| self.memory_failure())?;
        let mut bytes = self
            .buffers
            .pop()
            .unwrap_or_else(|| Buffer::new_in(self.registers.memory().clone()));
        bytes.clear();
        bytes
            .try_reserve(capacity)
            .map_err(|_| self.memory_failure())?;
        Ok(bytes)
    }
    fn try_finish_buffer(&mut self, bytes: Buffer<u8>) -> Result<Value, Error> {
        let value = self.try_intern(&bytes);
        // Recycling is optional; never turn a successful operation or memory
        // error into an infallible allocation just to retain scratch storage.
        if self.buffers.len() < self.buffers.capacity() {
            self.buffers.push(bytes);
        }
        value
    }

    fn recycle_buffer(&mut self, bytes: Buffer<u8>) {
        if self.buffers.len() < self.buffers.capacity() {
            self.buffers.push(bytes);
        }
    }

    /// Borrow bytes for a string identity produced by this VM.
    pub fn string_bytes(&self, id: StringId) -> Option<&[u8]> {
        self.strings.bytes.get(id.0).map(AsRef::as_ref)
    }

    /// Convert strings and numbers to Lua string identities without invoking
    /// `__tostring`. Store or pin a newly interned result before executing Lua.
    pub fn coerce_string(&mut self, value: Value) -> Result<Option<StringId>, Error> {
        self.validate_value(value)?;
        match value {
            Value::String(id) => Ok(Some(id)),
            Value::Number(number) => {
                let Value::String(id) = self.intern_number(number)? else {
                    unreachable!("intern returns a string")
                };
                Ok(Some(id))
            }
            _ => Ok(None),
        }
    }

    /// Read a global variable; absent variables have value `nil`.
    pub fn global(&mut self, name: &[u8]) -> Value {
        let k = self.intern(name);
        self.tables[slot_index(self.globals.0)].get(k)
    }

    /// Read a global variable, honoring the global table's `__index` metamethod.
    ///
    /// The result is retained until the next VM mutation; pin it for longer use.
    /// Unlike [`Self::global`], this can execute Lua. In a standalone context,
    /// lookup runs in the outer main frame without a message handler.
    ///
    /// # Errors
    /// Returns lookup errors or instruction-limit errors.
    pub fn get_global(&mut self, name: &[u8]) -> Result<Value, Error> {
        self.reset_budget();
        let key = self.intern(name);
        let target = Value::Table(self.globals);
        let value = self.tables[slot_index(self.globals.0)].get(key);
        if value != Value::Nil || self.metafield(target, Meta::Index) == Value::Nil {
            self.registers.clear();
            self.registers.push(value);
            self.top = 1;
            return Ok(value);
        }
        // Keep previous results rooted while a lookup callback runs. The
        // console looks up print with its arguments still on the Lua stack.
        let contextual = self.standalone != 0;
        if contextual {
            self.standalone_main(self.standalone, 0);
        }
        let result_slot = self.registers.len();
        self.resize_registers(result_slot + 1);
        let result = self.get_table(target, key, result_slot).and_then(|()| {
            let slot = if self.frames.is_empty() {
                result_slot
            } else {
                self.drive()?;
                if contextual { result_slot } else { 0 }
            };
            self.registers[0] = self.registers[slot];
            self.truncate_registers(1);
            self.top = 1;
            Ok(1)
        });
        if result.is_err() {
            self.standalone = 0;
        }
        self.finish_execution(result).map(|values| values[0])
    }

    /// Set a global variable.
    ///
    /// # Errors
    /// Honors the global table's `__newindex` metamethod.
    /// Returns an error for invalid/collected handles, Lua errors or quotas.
    pub fn set_global(&mut self, name: &[u8], value: Value) -> Result<(), Error> {
        self.reset_budget();
        self.validate_value(value)?;
        let k = self.try_intern(name)?;
        let target = Value::Table(self.globals);
        if self.metafield(target, Meta::NewIndex) == Value::Nil {
            return self.raw_set(self.globals, k, value);
        }
        if self.standalone != 0 {
            self.standalone_setter();
        }
        let result = self.set_table(target, k, value).and_then(|()| {
            if self.frames.is_empty() {
                Ok(0)
            } else {
                self.drive()
            }
        });
        if result.is_err() {
            self.standalone = 0;
        }
        self.finish_execution(result).map(|_| ())
    }

    /// Read a table entry without invoking metamethods.
    /// All object handles must originate in this VM and remain live.
    pub fn raw_table_get(&self, table: TableId, key: Value) -> Result<Value, Error> {
        self.validate_value(Value::Table(table))?;
        self.validate_value(key)?;
        Ok(self.tables[slot_index(table.0)].get(key))
    }

    /// [`HostCall::raw_fields_version`](crate::HostCall::raw_fields_version)
    /// outside a host call.
    ///
    /// # Errors
    /// Returns an error for an invalid or collected table handle.
    #[inline]
    pub fn raw_fields_version(&self, table: TableId) -> Result<u32, Error> {
        self.validate_value(Value::Table(table))?;
        Ok(self.tables[slot_index(table.0)].fields_version())
    }

    /// Return Lua's raw sequence boundary for a live table.
    pub fn raw_table_len(&self, table: TableId) -> Result<usize, Error> {
        self.validate_value(Value::Table(table))?;
        Ok(self.tables[slot_index(table.0)].len())
    }

    /// Set a table entry without invoking metamethods.
    /// All object handles must originate in this VM.
    ///
    /// # Errors
    /// Returns an error for invalid/collected handles, nil keys, NaN keys or
    /// allocation failure. A failed reservation leaves the table values intact.
    pub fn raw_table_set(&mut self, table: TableId, key: Value, value: Value) -> Result<(), Error> {
        self.validate_value(Value::Table(table))?;
        self.validate_value(key)?;
        self.validate_value(value)?;
        self.raw_set(table, key, value)
    }

    /// Read the private embedding-state table attached to a live Lua table.
    /// This slot is separate from Lua entries and metatables.
    pub fn table_host_state(&self, table: TableId) -> Result<Option<TableId>, Error> {
        self.validate_value(Value::Table(table))?;
        Ok(self.tables[slot_index(table.0)].host_state())
    }

    /// Attach private embedding state to a Lua table, or detach it with `None`.
    /// The owner strongly traces this state even when its Lua entries are weak.
    /// Unreachable cycles through the state are collectible. Lua field writes,
    /// iteration and metatable replacement cannot read or detach this slot.
    ///
    /// Both handles must be live and belong to this VM. A failed GC reservation
    /// leaves the previous attachment intact. The slot does not root its owner.
    pub fn set_table_host_state(
        &mut self,
        table: TableId,
        state: Option<TableId>,
    ) -> Result<(), Error> {
        let owner = Value::Table(table);
        self.validate_value(owner)?;
        let value = state.map_or(Value::Nil, Value::Table);
        self.validate_value(value)?;
        self.reserve_gc_barrier(owner, value)?;
        self.tables[slot_index(table.0)].set_host_state(state);
        self.gc_barrier(owner, value);
        Ok(())
    }

    fn raw_set(&mut self, id: TableId, key: Value, value: Value) -> Result<(), Error> {
        // Scalar array writes and overwrites of live string-keyed entries can
        // complete without allocating or adding a collector edge. The caller
        // has already resolved metamethods.
        let table = &mut self.tables[slot_index(id.0)];
        let replaced = match key {
            Value::String(name) => table.set_existing_string_scalar(name, value),
            _ => table.set_array_scalar_in_place(key, value),
        };
        if replaced {
            return Ok(());
        }
        Table::validate_set_key(key).map_err(|error| self.operand_error(error))?;
        self.raw_set_valid_key(id, key, value)
    }

    fn raw_set_valid_key(&mut self, id: TableId, key: Value, value: Value) -> Result<(), Error> {
        self.reserve_gc_table_write(id, key, value)?;
        let table = &mut self.tables[slot_index(id.0)];
        let before = table.heap_bytes();
        let result = table.set_with_native_hash(key, value, |id| {
            Table::native_string_hash(&self.strings.bytes[slot_index(id.0)])
        });
        let after = table.heap_bytes();
        self.note_table_resize(before, after);
        if let Err(error) = result {
            return Err(if error.kind() == ErrorKind::Memory {
                self.memory_failure()
            } else {
                self.operand_error(error)
            });
        }
        self.gc_table_write(id, key, value);
        Ok(())
    }
    #[inline]
    fn note_table_resize(&mut self, before: usize, after: usize) {
        if before != after {
            self.object_payload_bytes = self.object_payload_bytes - before + after;
            let growth = after.saturating_sub(before);
            self.tables.note_alloc(growth);
            self.gc.pending |= growth != 0;
        }
    }
    fn memory_failure(&self) -> Error {
        Error::memory().with_value(Value::String(self.memory_error))
    }
    // Calls and returns reserve on every frame; usually the capacity is there.
    #[inline]
    fn reserve_registers(&mut self, len: usize) -> Result<(), Error> {
        if len <= self.registers.capacity() {
            return Ok(());
        }
        self.grow_registers(len)
    }
    #[cold]
    #[inline(never)]
    fn grow_registers(&mut self, len: usize) -> Result<(), Error> {
        let before = self.registers.capacity();
        self.registers
            .try_reserve(len.saturating_sub(self.registers.len()))
            .map_err(|_| self.memory_failure())?;
        self.gc_note_control((self.registers.capacity() - before) * std::mem::size_of::<Value>());
        Ok(())
    }
    #[inline]
    fn resize_registers(&mut self, len: usize) {
        let current = self.registers.len();
        if len <= current {
            if len < current {
                self.truncate_registers(len);
            }
            return;
        }
        let before = self.registers.capacity();
        self.registers.resize(len, Value::Nil);
        if self.registers.capacity() != before {
            self.gc_note_control(
                (self.registers.capacity() - before) * std::mem::size_of::<Value>(),
            );
        }
    }
    fn truncate_registers(&mut self, len: usize) {
        self.hooks.preserve_popped(&self.registers, len);
        self.registers.truncate(len);
    }
    #[inline]
    fn push_record(&mut self, mut frame: Frame) {
        frame.c_level_bias = self.threads[slot_index(self.thread.0)].native_bias;
        let before = self.frames.capacity();
        self.frames.push(frame);
        if self.hooks.mask & hooks::CALL != 0 && self.hooks.allowed() {
            self.hook_call_record(frame);
        }
        self.gc_note_control((self.frames.capacity() - before) * std::mem::size_of::<Frame>());
    }

    /// Create an empty table owned by this VM.
    ///
    /// Store or pin the returned identity before executing Lua or collecting.
    pub fn new_table(&mut self) -> TableId {
        self.try_new_table().expect("allocate Lua table")
    }
    /// Try to create an empty table. Store or pin it before executing Lua or
    /// collecting.
    ///
    /// # Errors
    /// Returns a memory error if arena or collector queue allocation fails.
    pub fn try_new_table(&mut self) -> Result<TableId, Error> {
        self.try_table_with_capacity(0, 0)
    }
    fn try_table_with_capacity(&mut self, array: usize, records: usize) -> Result<TableId, Error> {
        self.try_allocate_table(array, records, true)
    }

    // The host builds these with only scalars or other fresh dense arrays,
    // without collection checkpoints or Lua reentry during construction.
    fn try_allocate_dense_table(&mut self, array: usize) -> Result<TableId, Error> {
        self.try_allocate_table(array, 0, false)
    }

    fn try_allocate_number_array(&mut self, numbers: &[f64]) -> Result<TableId, Error> {
        let memory = self.tables.memory();
        if self.lua51_table_order
            && memory.allocator().is_none()
            && let Some(table) = Table::with_shared_number_array(numbers, memory)
        {
            return self.try_insert_table(table, false);
        }
        let table = self.try_allocate_dense_table(numbers.len())?;
        self.tables[slot_index(table.0)].initialize_number_array(numbers);
        Ok(table)
    }

    #[inline]
    fn try_allocate_table(
        &mut self,
        array: usize,
        records: usize,
        needs_trace: bool,
    ) -> Result<TableId, Error> {
        let create = if self.lua51_table_order {
            Table::with_native51_capacity
        } else {
            Table::with_capacity_in
        };
        let table = create(array, records, self.tables.memory().clone()).map_err(|error| {
            if error.kind() == ErrorKind::Memory {
                self.memory_failure()
            } else {
                error
            }
        })?;
        self.try_insert_table(table, needs_trace)
    }
    fn try_insert_table(&mut self, table: Table, needs_trace: bool) -> Result<TableId, Error> {
        if needs_trace {
            self.reserve_gc_new_value()?;
        }
        let bytes = table.heap_bytes();
        let id = TableId(
            self.tables
                .try_insert(table)
                .map_err(|_| self.memory_failure())?,
        );
        self.object_payload_bytes += bytes;
        self.tables.note_alloc(bytes);
        self.gc.pending |= bytes != 0;
        if needs_trace {
            self.gc_new_value(Value::Table(id));
        } else {
            self.gc_mark_new_dense_table(id);
        }
        Ok(id)
    }
    fn new_record_table(&mut self, capacity: usize) -> Result<TableId, Error> {
        self.try_table_with_capacity(0, capacity)
    }
    fn number(&self, value: Value) -> Option<f64> {
        match value {
            Value::Number(n) => Some(n),
            Value::String(id) => parse_number(self.string_bytes(id)?),
            _ => None,
        }
    }
    fn table(&self, value: Value) -> Result<&Table, Error> {
        if let Value::Table(id) = value
            && let Some(t) = self.tables.get(id.0)
        {
            return Ok(t);
        }
        Err(Error::new(
            ErrorKind::Runtime,
            0,
            format!("attempt to index a {} value", value.type_name()),
        ))
    }

    fn load(&mut self, program: &Program) -> Result<FunctionId, Error> {
        self.bind_program(program)
            .map_err(|_| self.memory_failure())
    }

    // A parser returns a new ProgramData owner for every freshly compiled
    // chunk. Only public Program handles can refer to an already-bound owner.
    fn load_fresh(&mut self, program: &Program) -> Result<FunctionId, Error> {
        self.bind_new_program(program)
            .map_err(|_| self.memory_failure())
    }

    fn bind_program(&mut self, program: &Program) -> Result<FunctionId, Error> {
        let cached = self
            .programs
            .entries()
            .find(|(_, p)| crate::memory::Shared::ptr_eq(&p.data, &program.data))
            .map(|(index, p)| (index, p.entry));
        if let Some((index, old_entry)) = cached {
            if self.functions.contains(old_entry.0) {
                self.reserve_gc_value(Value::Function(old_entry))?;
                self.gc_new_value(Value::Function(old_entry));
                return Ok(old_entry);
            }
            self.functions.try_reserve(1)?;
            let upvalues = self.entry_upvalues(program)?;
            let entry = self.insert_closure(Closure {
                program: index,
                prototype: program.data.entry as usize,
                upvalues,
                env: self.globals,
            });
            self.programs[index].entry = entry;
            self.gc_new_value(Value::Function(entry));
            return Ok(entry);
        }
        self.bind_new_program(program)
    }

    fn bind_new_program(&mut self, program: &Program) -> Result<FunctionId, Error> {
        let memory = self.programs.memory().clone();
        let budget = memory.reserve_external(|| program.heap_bytes())?;
        let (mut constants, mut upvalue_names, mut reused) =
            if let Some(buffers) = self.compile_spare.bindings.take(&program.data, &memory) {
                (
                    buffers.constants,
                    buffers.upvalue_names,
                    Some(buffers.values),
                )
            } else {
                (
                    Buffer::new_in(memory.clone()),
                    Buffer::new_in(memory.clone()),
                    None,
                )
            };
        constants.try_reserve_exact(program.data.prototypes.len())?;
        upvalue_names.try_reserve_exact(program.data.prototypes.len())?;
        for (index, prototype) in program.data.prototypes.iter().enumerate() {
            let mut values = reused
                .take()
                .unwrap_or_else(|| Buffer::new_in(memory.clone()));
            if !prototype
                .shared_owner()
                .is_some_and(|owner| owner as usize != index)
            {
                values.try_reserve_exact(prototype.constants().len())?;
                for k in prototype.constants() {
                    values.push(match k {
                        Constant::Nil => Value::Nil,
                        Constant::Boolean(v) => Value::Boolean(*v),
                        Constant::Number(v) => Value::Number(*v),
                        Constant::String(_) | Constant::PooledString { .. } => self
                            .bind_constant_string(
                                k.string(&prototype.constant_strings)
                                    .expect("verified constant string"),
                            )?,
                    });
                }
            }
            constants.push(values);
            let defined_names = prototype.debug.upvalue_names();
            let mut names = Buffer::new_in(memory.clone());
            names.try_reserve_exact(defined_names.len())?;
            for name in defined_names {
                let Value::String(id) = self.try_intern(prototype.debug.name(name))? else {
                    unreachable!()
                };
                names.push(id);
            }
            upvalue_names.push(names);
        }
        let loaded = LoadedProgram {
            data: program.data.clone(),
            _budget: budget,
            constants,
            upvalue_names,
            debug_strings: Buffer::new_in(memory.clone()),
            parser_prototypes: Buffer::new_in(memory),
            entry: FunctionId(0),
        };
        // All fallible work precedes publishing a program, closure or cell.
        // No collection or callback occurs between these reserved insertions.
        self.programs.try_reserve(1)?;
        self.functions.try_reserve(1)?;
        let upvalues = self.entry_upvalues(program)?;
        let bytes = gc::program_bytes(&loaded);
        let index = slot_index(self.programs.insert(loaded));
        self.programs.note_alloc(bytes);
        self.object_payload_bytes += bytes;
        let entry = self.insert_closure(Closure {
            program: index,
            prototype: program.data.entry as usize,
            upvalues,
            env: self.globals,
        });
        self.programs[index].entry = entry;
        self.gc_new_value(Value::Function(entry));
        Ok(entry)
    }

    fn insert_closure(&mut self, closure: Closure) -> FunctionId {
        let bytes = closure.upvalues.len() * size_of::<u32>();
        self.functions
            .note_alloc(closure.upvalues.capacity() * size_of::<u32>());
        let id = FunctionId(self.functions.insert(closure));
        self.object_payload_bytes += bytes;
        id
    }

    fn entry_upvalues(&mut self, program: &Program) -> Result<Buffer<u32>, Error> {
        let count = program.data.prototypes[program.data.entry as usize]
            .upvalues
            .len();
        let mut upvalues = Buffer::new_in(self.functions.memory().clone());
        upvalues.try_reserve_exact(count)?;
        self.upvalues.try_reserve(count)?;
        self.reserve_gc_new_values(count + 1)?;
        for _ in 0..count {
            let id = slot_index(self.upvalues.insert(Cell::Closed(Value::Nil))) as u32;
            self.gc_new_cell(id as usize);
            upvalues.push(id);
        }
        Ok(upvalues)
    }

    fn capture_register(&mut self, register: usize) -> Result<u32, Error> {
        if let Some(&id) = self
            .open_upvalues
            .iter()
            .find(|&&id| matches!(self.upvalues[id as usize], Cell::Open(_,r) if r == register))
        {
            return Ok(id);
        }
        let before = self.open_upvalues.capacity();
        self.reserve_gc_close_slot()?;
        self.open_upvalues
            .try_reserve(1)
            .map_err(|_| self.memory_failure())?;
        self.gc_note_control((self.open_upvalues.capacity() - before) * size_of::<u32>());
        self.reserve_gc_new_value()?;
        let id = slot_index(
            self.upvalues
                .try_insert(Cell::Open(self.thread, register))
                .map_err(|_| self.memory_failure())?,
        ) as u32;
        self.open_upvalues.push(id);
        self.gc_new_cell(id as usize);
        Ok(id)
    }

    fn close_upvalues(&mut self, base: usize) {
        let mut i = 0;
        while i < self.open_upvalues.len() {
            let id = self.open_upvalues[i] as usize;
            if let Cell::Open(_, r) = self.upvalues[id]
                && r >= base
            {
                self.upvalues[id] = Cell::Closed(self.registers[r]);
                self.gc_close_cell(id, self.registers[r]);
                self.open_upvalues.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    fn push_frame(
        &mut self,
        function: FunctionId,
        call: usize,
        count: usize,
        wanted: u32,
    ) -> Result<(), Error> {
        self.check_calls()?;
        let closure = &self.functions[slot_index(function.0)];
        let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
        let (registers, params, compat_arg, vararg) = (
            usize::from(proto.registers),
            usize::from(proto.params),
            proto.compat_arg,
            proto.vararg,
        );
        let base = self.registers.len();
        self.reserve_registers(base + registers)?;
        let fixed = call + 1..call + 1 + count.min(params);
        // Small fixed prefixes can be copied directly from the caller while
        // initializing the callee's window. Only the tail still needs nils.
        match fixed.len() {
            0 => self.registers.resize(base + registers, Value::Nil),
            1 => {
                let argument = self.registers[fixed.start];
                self.registers.push(argument);
                self.registers.resize(base + registers, Value::Nil);
            }
            2 => {
                let arguments = [self.registers[fixed.start], self.registers[fixed.start + 1]];
                self.registers.extend_from_slice(&arguments);
                self.registers.resize(base + registers, Value::Nil);
            }
            32.. => {
                self.registers.extend_from_within(fixed.clone());
                self.registers.resize(base + registers, Value::Nil);
            }
            _ => {
                self.registers.resize(base + registers, Value::Nil);
                self.registers.copy_within(fixed.clone(), base);
            }
        }
        // lua_pcall keeps a stack-slot reference, including when that slot is
        // passed as a parameter. Follow moved fixed arguments; vararg entry
        // clears their original slots as Lua 5.1 adjust_varargs does.
        let vararg_start = fixed.end;
        if self.native_protected {
            if vararg {
                self.discard_native_error_function_slots(fixed);
            } else {
                self.relocate_native_error_functions(fixed, base);
                self.discard_native_error_function_slots(vararg_start..call + 1 + count);
            }
        }
        if self
            .frames
            .last()
            .is_some_and(|frame| matches!(frame.function, FrameKind::Native(CONT_HOST_META)))
        {
            // Native API operands remain in the visible C window. Their
            // private metamethod argument copies stop owning roots after
            // entering Lua; vararg extras remain in the callee's vararg area.
            let end = if vararg {
                vararg_start
            } else {
                call + 1 + count
            };
            self.registers[call + 1..end].fill(Value::Nil);
        }
        let vararg_count = count.saturating_sub(params);
        if compat_arg {
            let t = self.try_new_table()?;
            for i in 0..vararg_count {
                self.raw_set(
                    t,
                    Value::Number((i + 1) as f64),
                    self.registers[vararg_start + i],
                )?;
            }
            let n = self.try_intern(b"n")?;
            self.raw_set(t, n, Value::Number(vararg_count as f64))?;
            self.registers[base + params] = Value::Table(t);
        }
        self.push_record(Frame {
            function: FrameKind::Lua(function),
            pc: 0,
            base,
            stack_start: base,
            result: call,
            wanted: frame_wanted(wanted),
            barriers: self.frames.last().map_or(0, |f| f.barriers),
            vararg_start,
            vararg_count: vararg_count as u32,
            tail_calls: 0,
            c_level_bias: 0,
        });
        self.top = base;
        Ok(())
    }

    fn tail_frame(&mut self, function: FunctionId, call: usize, count: usize) -> Result<(), Error> {
        let frame = *self.frames.last().expect("tail call has an active frame");
        let closure = &self.functions[slot_index(function.0)];
        let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
        let (registers, params, compat_arg) = (
            usize::from(proto.registers),
            usize::from(proto.params),
            proto.compat_arg,
        );
        let extras = count.saturating_sub(params);
        // Keep varargs below the register window; open calls can grow above it.
        let base = frame.stack_start + extras;
        let end = base + registers;
        let scratch = self.registers.len().max(end);
        // Keep the same register capacity and collector accounting as frame setup.
        self.reserve_registers(scratch + count)?;
        self.close_upvalues(frame.base);
        let arguments = call + 1..call + 1 + count;
        if self.hooks.popped.is_empty() {
            self.registers.resize(scratch, Value::Nil);
        } else {
            // A suspended host hook can observe the old scratch slots when popped.
            self.registers.resize(scratch + count, Value::Nil);
            self.registers.copy_within(arguments.clone(), scratch);
        }
        let argument_end = frame.stack_start + count;
        self.registers.copy_within(arguments, frame.stack_start);
        if extras != 0 {
            self.registers[frame.stack_start..argument_end].rotate_right(extras);
        }
        self.registers[argument_end..end].fill(Value::Nil);
        self.truncate_registers(end);
        if compat_arg {
            let t = self.try_new_table()?;
            for i in 0..extras {
                self.raw_set(
                    t,
                    Value::Number((i + 1) as f64),
                    self.registers[frame.stack_start + i],
                )?;
            }
            let n = self.try_intern(b"n")?;
            self.raw_set(t, n, Value::Number(extras as f64))?;
            self.registers[base + params] = Value::Table(t);
        }
        *self
            .frames
            .last_mut()
            .expect("tail call has an active frame") = Frame {
            function: FrameKind::Lua(function),
            pc: 0,
            base,
            vararg_start: frame.stack_start,
            vararg_count: extras as u32,
            tail_calls: frame.tail_calls.saturating_add(1),
            c_level_bias: frame.c_level_bias,
            ..frame
        };
        self.top = base;
        Ok(())
    }

    fn return_frame(
        &mut self,
        frame: Frame,
        source: usize,
        count: usize,
    ) -> Result<Option<usize>, Error> {
        if self.complete_native_body(frame, source, count)? {
            return Ok(Some(count));
        }
        if self.hooks.mask & hooks::RETURN != 0
            && self.hooks.allowed()
            && self.defer_hook_return(frame, source, count)?
        {
            return Ok(None);
        }
        self.finish_frame(frame, source, count)
    }
    fn finish_frame(
        &mut self,
        frame: Frame,
        source: usize,
        count: usize,
    ) -> Result<Option<usize>, Error> {
        let wanted = if frame.wanted == FRAME_OPEN {
            count
        } else {
            frame.wanted as usize
        };
        let top = frame.result + wanted;
        let end = frame.stack_start.max(top);
        if self.frames.len() > 1 {
            self.reserve_registers(end)?;
        }
        if frame.is_call() {
            self.relocate_native_error_functions(
                frame.base..self.registers.len(),
                frame.result + 1,
            );
        }
        self.close_upvalues(frame.base);
        if self.frames.len() == 1 {
            let destination = if self.thread == self.main {
                0
            } else {
                frame.result
            };
            let total = destination + count;
            self.registers
                .copy_within(source..source + count, destination);
            self.truncate_registers(total);
            self.top = total;
            self.frames.clear();
            return Ok(Some(total));
        }
        // An API call on a suspended state can truncate the spare storage of
        // its pending Lua caller. A resumed native yield must restore that
        // caller's register window, as luaD_poscall restores ci->top.
        if self.registers.len() < end {
            self.registers.resize(end, Value::Nil);
        }
        let copied = count.min(wanted);
        match copied {
            0 => {}
            1 => self.registers[frame.result] = self.registers[source],
            2 => {
                let values = [self.registers[source], self.registers[source + 1]];
                self.registers[frame.result..frame.result + 2].copy_from_slice(&values);
            }
            _ => self
                .registers
                .copy_within(source..source + copied, frame.result),
        }
        if copied < wanted {
            self.registers[frame.result + copied..top].fill(Value::Nil);
        }
        self.registers.truncate(end);
        self.top = top;
        self.frames.pop();
        Ok(None)
    }

    /// Load a verified program as a function without executing it.
    ///
    /// The function is retained in the result registers until the next VM
    /// mutation. Pin it when retaining it across other executions or collections.
    /// A live Program also retains its entry binding; dropping the Program lets
    /// Lua ownership alone determine the function/environment's lifetime.
    /// Use [`Self::try_load_program`] to recover from allocation failure.
    pub fn load_program(&mut self, program: &Program) -> FunctionId {
        self.try_load_program(program).expect("load Lua program")
    }

    /// Bind a verified program without executing it, retaining its entry in the
    /// result registers with the same ownership rules as [`Self::load_program`].
    /// Immutable compiled code stays shared; runtime bindings use this VM's heap.
    ///
    /// # Errors
    /// Returns the fixed memory error if binding or result storage cannot grow.
    /// Failure preserves the previous result registers and publishes no new
    /// program binding, closure or upvalue cell.
    pub fn try_load_program(&mut self, program: &Program) -> Result<FunctionId, Error> {
        self.reserve_registers(1)?;
        let function = self.load(program)?;
        self.registers.clear();
        self.registers.push(Value::Function(function));
        self.top = 1;
        Ok(function)
    }

    /// Execute verified bytecode, retaining globals and reusing register storage.
    ///
    /// # Errors
    /// Returns Lua runtime errors or an instruction-limit error. Returned values
    /// borrow the register storage until the next mutation of the VM.
    pub fn execute(&mut self, program: &Program) -> Result<&[Value], Error> {
        self.reset_budget();
        if self.standalone != 0 {
            let function = self.load(program)?;
            return self.call_root(Value::Function(function), &[]);
        }
        let function = self.load(program)?;
        let closure = &self.functions[slot_index(function.0)];
        let entry = &self.programs[closure.program].data.prototypes[closure.prototype];
        let compat_arg = entry.compat_arg;
        let params = usize::from(entry.params);
        let registers = usize::from(entry.registers);
        self.reserve_registers(registers)?;
        self.reserve_frames(1)?;
        self.registers.clear();
        self.resize_registers(registers);
        if compat_arg {
            let arg = self.try_new_table()?;
            let n = self.try_intern(b"n")?;
            self.raw_set(arg, n, Value::Number(0.0))?;
            self.registers[params] = Value::Table(arg);
        }
        self.frames.clear();
        self.push_record(Frame {
            function: FrameKind::Lua(function),
            pc: 0,
            base: 0,
            stack_start: 0,
            result: 0,
            wanted: FRAME_OPEN,
            barriers: 0,
            vararg_start: 0,
            vararg_count: 0,
            tail_calls: 0,
            c_level_bias: 0,
        });
        self.top = 0;
        let result = self.drive();
        self.finish_execution(result)
    }

    /// Execute a verified chunk with positional arguments, retaining globals.
    ///
    /// Arguments must belong to this VM and remain live until this call.
    /// Fixed parameters, varargs and Lua 5.1's compatibility `arg` use the same
    /// entry path as ordinary Lua calls. Returned values borrow VM storage.
    ///
    /// # Errors
    /// Returns an invalid-handle, Lua runtime or instruction-limit error.
    pub fn execute_with_args(
        &mut self,
        program: &Program,
        arguments: &[Value],
    ) -> Result<&[Value], Error> {
        if arguments.is_empty() {
            return self.execute(program);
        }
        for &value in arguments {
            self.validate_value(value)?;
        }
        let function = self.load(program)?;
        self.call_root(Value::Function(function), arguments)
    }

    /// Call a live Lua function, builtin or callable object with positional values.
    ///
    /// This starts a top-level call and reuses this VM's globals and storage.
    /// Object arguments must belong to this VM; pin values retained across other
    /// executions or collections. Returned values borrow the register storage.
    ///
    /// # Errors
    /// Returns an invalid-handle, Lua runtime or instruction-limit error.
    pub fn call(&mut self, function: Value, arguments: &[Value]) -> Result<&[Value], Error> {
        self.validate_value(function)?;
        for &value in arguments {
            self.validate_value(value)?;
        }
        self.call_root(function, arguments)
    }

    fn call_root(&mut self, function: Value, arguments: &[Value]) -> Result<&[Value], Error> {
        self.reset_budget();
        if self.standalone != 0 {
            return self.standalone_call(function, arguments, standalone::EntryMode::Message);
        }
        if arguments.len() > u32::MAX as usize {
            return Err(Error::new(ErrorKind::Limit, 0, "too many arguments"));
        }
        self.reserve_registers(arguments.len() + 1)?;
        self.registers.clear();
        self.registers.push(function);
        self.registers.extend_from_slice(arguments);
        self.frames.clear();
        self.top = 0;
        let result = match self.start_lua_call(0, arguments.len(), OPEN, false) {
            Ok(Some(count)) => {
                self.truncate_registers(count);
                self.top = count;
                Ok(count)
            }
            Ok(None) => self.drive(),
            Err(error) => Err(error),
        };
        self.finish_execution(result)
    }

    fn call_unprotected(
        &mut self,
        function: Value,
        arguments: &[Value],
    ) -> Result<&[Value], Error> {
        if self.standalone != 0 {
            self.standalone_call(function, arguments, standalone::EntryMode::Unprotected)
        } else {
            self.call_root(function, arguments)
        }
    }

    fn finish_execution(&mut self, result: Result<usize, Error>) -> Result<&[Value], Error> {
        if result.is_err()
            && let Some(frame) = self.frames.first()
        {
            self.set_thread_native_bias(self.thread, frame.c_level_bias);
        }
        self.close_upvalues(0);
        self.discard_finalizers(0);
        self.frames.clear();
        self.restore_calls();
        self.trim_substitutions();
        self.sorts.clear();
        self.trim_loads();
        self.trim_tracebacks();
        self.trim_hooks();
        match result {
            Ok(count) => Ok(&self.registers[..count]),
            Err(mut error) => {
                error.target_thread = None;
                self.registers.clear();
                if let Some(value) = error.value() {
                    self.registers.push(value);
                }
                self.top = self.registers.len();
                Err(error)
            }
        }
    }

    // Shared entry driver for source execution and host-requested finalization.
    fn drive(&mut self) -> Result<usize, Error> {
        let mut error_line = 0;
        let mut pending = None;
        let result = loop {
            let result = pending
                .take()
                .map_or_else(|| self.run(&mut error_line), Err);
            match result {
                Err(error) if error.catchable() => {
                    if let Some(target) = error.target_thread {
                        self.leave_threads_to(target, &mut error_line)?;
                    }
                    match self.catch_error(&error) {
                        Ok(true) => continue,
                        Ok(false) if self.thread != self.main => {
                            if let Err(error) = self.fail_thread(&error, &mut error_line) {
                                pending = Some(error);
                            }
                            continue;
                        }
                        Ok(false) => break Err(error),
                        Err(next) if next.target_thread.is_some() => {
                            pending = Some(next);
                            continue;
                        }
                        Err(next) => break Err(next),
                    }
                }
                Ok(count) if self.thread != self.main => {
                    if let Err(error) = self.finish_thread(0, count, false, false, &mut error_line)
                    {
                        pending = Some(error);
                    }
                    continue;
                }
                result => break result,
            }
        };
        let result = result.map_err(|error| error.at_line(error_line));
        self.abort_threads(&mut error_line);
        result
    }

    #[inline]
    fn reset_budget(&mut self) {
        self.remaining = self.budget.unwrap_or(0);
    }

    #[inline]
    fn charge(&mut self) -> Result<(), Error> {
        if self.budget.is_some() {
            if self.remaining == 0 {
                return Err(Error::new(
                    ErrorKind::Limit,
                    0,
                    "instruction limit exceeded",
                ));
            }
            self.remaining -= 1;
        }
        Ok(())
    }

    fn run(&mut self, error_line: &mut u32) -> Result<usize, Error> {
        // The executing program's code, retained across frame transitions:
        // calls and returns within one program keep it without touching its
        // atomic reference count.
        let mut held: Option<crate::memory::Shared<ProgramData>> = None;
        let mut numeric_call_cache = numeric_leaf::Cache::default();
        let mut return_argument_cache = return_argument::Cache::default();
        let mut radix_cache = radix_selector::Cache::default();
        let mut forwarding_cache = forwarding::Cache::default();
        let mut table_literal_cache = table_literals::Cache::default();
        let mut triplet_sites = fresh_triplet::Sites::default();
        let mut field_hints = [[0; 2]; simple::FIELD_HINTS];
        let mut reduction_retry: Option<reductions::Retry> = None;
        'frames: loop {
            self.maybe_gc()?;
            let frame_index = self.frames.len() - 1;
            if self.hooks.enter.is_some() && self.hook_entry(frame_index)? {
                continue;
            }
            if self.hooks.tail.is_some() {
                self.merge_hook_tail(frame_index)?;
            }
            let frame_index = self.frames.len() - 1;
            let frame = self.frames[frame_index];
            let function = match frame.function {
                FrameKind::Native(id) => {
                    self.charge()?;
                    let id = self.body_code(self.thread, frame_index).unwrap_or(id);
                    let previous = self.active_native.replace(frame_index);
                    let result = if matches!(id, threads::CO_RESUME | threads::CO_YIELD) {
                        self.coroutine_step(id, frame)
                    } else {
                        self.step_native(id, frame)
                    }
                    .map_err(|error| self.attach_error_value(error, 1));
                    self.active_native = previous;
                    if let Some(line) = self.sort_line.take() {
                        *error_line = line;
                    }
                    if let Some(count) = result? {
                        return Ok(count);
                    }
                    if self.suspend_native_body(frame_index) {
                        return Ok(0);
                    }
                    if let Some(count) = self.apply_transition(error_line)? {
                        return Ok(count);
                    }
                    continue;
                }
                FrameKind::Lua(id) => id,
            };
            let closure = &self.functions[slot_index(function.0)];
            let (program_index, proto_index, env) =
                (closure.program, closure.prototype, closure.env);
            // Hold immutable code independently of VM-vector growth during native calls.
            // Shared ownership changes only when a frame runs another program.
            let current = &self.programs[program_index].data;
            if !held
                .as_ref()
                .is_some_and(|data| crate::memory::Shared::ptr_eq(data, current))
            {
                held = Some(current.clone());
            }
            let data = held.as_ref().expect("executing program");
            let prototype = &data.prototypes[proto_index];
            let mut pc = frame.pc();
            let base = frame.base;
            if pc == 0
                && prototype.code.len() == 20
                && prototype.params == 1
                && !prototype.vararg
                && let Some((return_pc, value)) = self.try_table_search(function, prototype, base)
            {
                self.frames[frame_index].pc = (return_pc + 1) as u32;
                *error_line = prototype.instruction_line(return_pc);
                self.registers[base] = value;
                if let Some(count) = self.return_frame(frame, base, 1)? {
                    return Ok(count);
                }
                continue 'frames;
            }
            // Simple instructions and successful numeric leaf calls cannot
            // allocate or change frame observers. Any other dispatch ends
            // this interval, and a new frame always starts with a GC poll.
            let mut numeric_checkpoint_ready = false;
            let mut resume_hints = threads::inline_resume::ResumeHints::default();
            let mut ignored_numeric_while = None;
            let mut deferred_literal: Option<fresh_triplet::Pending> = None;
            loop {
                let simple_start = pc;
                // With no hook or instruction limit counting instructions,
                // runs of simple ones execute without frame bookkeeping.
                if self.hooks.mask & (hooks::LINE | hooks::COUNT) == 0 && self.budget.is_none() {
                    let inline_calls = numeric_checkpoint_ready
                        && self.frames.len() < self.frames.capacity()
                        && self.frames.len() + 1 < usize::from(self.call_slots);
                    let can_change_field_liveness = self.gc_can_change_field_liveness();
                    let mut cx = simple::Context {
                        registers: &mut self.registers,
                        constants: self.programs[program_index].bound_constants(proto_index),
                        field_hints: &mut field_hints,
                        tables: &mut self.tables,
                        strings: &self.strings.bytes,
                        upvalue_ids: &self.functions[slot_index(function.0)].upvalues,
                        upvalues: &self.upvalues,
                        threads: &mut self.threads,
                        thread: self.thread,
                        code: &prototype.code,
                        stop_before_for_loop: reduction_retry
                            .as_ref()
                            .filter(|retry| retry.frame == frame_index)
                            .map(|retry| retry.pc),
                        ignored_numeric_while,
                        env,
                        base,
                        gc_pending: self.gc.pending,
                        can_change_field_liveness,
                        closed: self.open_upvalues.is_empty(),
                        index_name: self.meta_names[Meta::Index as usize],
                        // A hook would observe the handler call a resolver skips.
                        resolvers: (self.hooks.mask == 0).then_some(simple::Resolvers {
                            natives: &self.native_closures,
                            strings: &self.strings.bytes,
                        }),
                        string_metatable: self.type_metatables[meta::STRING_METATABLE_SLOT],
                    };
                    if inline_calls
                        && let Some((next, resumed)) =
                            resume_hints.run_consumer_cycle(&mut cx, pc, &mut self.top)
                    {
                        pc = next;
                        if resumed {
                            self.gc.pending = false;
                        }
                    }
                    loop {
                        while let Some(next) = simple::step(&mut cx, pc) {
                            pc = next;
                        }
                        if !inline_calls {
                            break;
                        }
                        let call = cx.code[pc];
                        if call.op != Op::Call {
                            break;
                        }
                        let slot = base + usize::from(call.a);
                        let callee = cx.registers[slot];
                        match (callee, call.b) {
                            (Value::Native(threads::CO_RESUME), 1)
                                if call.c == 2 && resume_hints.resume(&mut cx, slot) =>
                            {
                                self.gc.pending = false;
                            }
                            (Value::Native(threads::CO_RESUME), 1)
                                if call.c != OPEN
                                    && resume_hints.resume_projected(
                                        &mut cx,
                                        &self.functions,
                                        &self.programs,
                                        slot,
                                        call.c as usize,
                                    ) =>
                            {
                                self.gc.pending = false;
                            }
                            (Value::Native(NEXT | PAIRS_NEXT), 2)
                                if call.c == 2
                                    && native_fast::next_pair(cx.tables, cx.registers, slot) => {}
                            (Value::Native(0), 1)
                                if call.c <= 1 && cx.registers[slot + 1].is_truthy() =>
                            {
                                cx.registers[slot] = cx.registers[slot + 1];
                            }
                            (Value::Native(id @ (math::MATH_MIN | math::MATH_MAX)), 2)
                                if call.c <= 1 =>
                            {
                                let Some(value) = math::numeric_extremum(
                                    cx.registers[slot + 1],
                                    cx.registers[slot + 2],
                                    id == math::MATH_MIN,
                                ) else {
                                    break;
                                };
                                cx.registers[slot] = value;
                            }
                            (Value::Native(id @ 32..=35), 1)
                            | (Value::Native(id @ math::MATH_FMOD), 2)
                                if call.c <= 1 =>
                            {
                                let Some(value) = math::numeric_primitive(
                                    id,
                                    &cx.registers[slot + 1..slot + 1 + usize::from(call.b)],
                                ) else {
                                    break;
                                };
                                cx.registers[slot] = value;
                            }
                            _ => break,
                        }
                        self.top = slot + call.c as usize;
                        pc += 1;
                    }
                    // The parked stack has no open upvalues. Each call above
                    // publishes its own result, and no instruction in this
                    // loop can observe the child's numeric register copies.
                    if inline_calls {
                        resume_hints.write_back(cx.threads);
                    } else {
                        // A general instruction ended the previous checkpoint.
                        resume_hints.clear();
                    }
                }
                let i = prototype.code[pc];
                let a = base + usize::from(i.a);
                if self.hooks.mask & (hooks::LINE | hooks::COUNT) != 0
                    && self.hook_instruction(frame_index, pc, prototype.hook_line(pc))?
                {
                    continue 'frames;
                }
                self.charge()?;
                let checkpoint_ready = std::mem::take(&mut numeric_checkpoint_ready);
                *error_line = prototype.instruction_line(pc);
                pc += 1;
                let b = base + usize::from(i.b);
                let c = i.c as usize;
                self.frames[frame_index].pc = pc as u32;
                if matches!(i.op, Op::Call | Op::CallIter | Op::TailCall | Op::Return)
                    || i.op == Op::Vararg && i.c == OPEN
                    || i.op == Op::SetList && i.b == OPEN_ARGS
                {
                    if self
                        .hooks
                        .resumed_top
                        .is_some_and(|(owner, _)| owner == frame_index)
                    {
                        self.hooks.resumed_top = None;
                    }
                    if self.hooks.yield_owner == Some(frame_index) {
                        self.hooks.yield_owner = None;
                    }
                }
                if let Some(literal) = deferred_literal.take() {
                    let table_slot = base + usize::from(literal.register);
                    if literal.begin == simple_start
                        && literal.end == pc - 1
                        && self.write_inlined_triplet(table_slot)?
                    {
                        // Both the scalar-only SETLIST and a result-discarding
                        // CALL were completed without running arbitrary Lua.
                        pc += 1;
                        self.frames[frame_index].pc = pc as u32;
                        self.top = table_slot - 3;
                        // The earlier budget check covered the sole inline
                        // table we just allocated. Stay in this Lua frame.
                        continue;
                    }
                    // No collector, hook or nested Lua has run since NEWTABLE.
                    // Restore its ordinary table before the first slow instruction.
                    self.registers[table_slot] = Value::Table(self.try_table_with_capacity(3, 0)?);
                }
                match i.op {
                    Op::Load => {
                        // Initial nil slots were cleared on frame entry. A call
                        // hook can already have replaced them before execution.
                        if i.b == 0 {
                            self.registers[a] =
                                self.programs[program_index].bound_constants(proto_index)[c];
                        }
                    }
                    Op::Move if i.c == OPEN => {
                        let count = self.top.checked_sub(b).ok_or_else(|| {
                            Error::new(ErrorKind::Runtime, 0, "invalid open result range")
                        })?;
                        self.resize_registers(self.registers.len().max(a + count));
                        self.registers.copy_within(b..b + count, a);
                        self.top = a + count;
                    }
                    Op::Move => self.registers[a] = self.registers[b],
                    Op::Bool => {
                        self.registers[a] = if c == 0 {
                            self.registers[b]
                        } else {
                            Value::Boolean(!self.registers[b].is_truthy())
                        }
                    }
                    Op::GetGlobal => self.get_table(
                        Value::Table(env),
                        self.programs[program_index].bound_constants(proto_index)[c],
                        a,
                    )?,
                    Op::SetGlobal => {
                        let key = self.programs[program_index].bound_constants(proto_index)[c];
                        let value = self.registers[a];
                        if !matches!(key, Value::String(name)
                            if self.tables[slot_index(env.0)].set_existing_string_scalar(name, value))
                        {
                            self.set_table(Value::Table(env), key, value)?;
                        }
                    }
                    Op::NewTable => {
                        if i.b == 3
                            && i.c == 0
                            && i.a >= 3
                            && self.lua51_table_order
                            && self.has_triplet_resolver(self.registers[a - 3])
                            && let Some(end) = triplet_sites.fill_end(function, prototype, pc)
                            && self.open_upvalues.is_empty()
                            && self.sort_fast_context()
                            && self.tables.has_free_slots(2)
                            && self.tables.memory().allocator().is_none()
                            && self.can_defer_inline_triplet()
                        {
                            self.gc.pending = false;
                            deferred_literal = Some(fresh_triplet::Pending {
                                begin: pc,
                                end,
                                register: i.a,
                            });
                            self.registers[a] = Value::Nil;
                            continue;
                        }
                        let reduced = if i.b == 0 && i.c == 0 {
                            self.elide_fresh_dense_fill(
                                base,
                                pc,
                                prototype,
                                program_index,
                                proto_index,
                            )
                        } else if i.c == 0 && i.b >= table_literals::MIN_ARRAY_HINT {
                            self.copy_table_literal(
                                &mut table_literal_cache,
                                function,
                                prototype,
                                base,
                                pc,
                            )?
                        } else {
                            None
                        };
                        if let Some(end) = reduced {
                            pc = end;
                            self.frames[frame_index].pc = pc as u32;
                        } else {
                            let id = if self.lua51_table_order {
                                let array = table_literals::allocation_hint(u32::from(i.b))?;
                                let records = table_literals::allocation_hint(i.c)?;
                                self.try_table_with_capacity(array, records)?
                            } else {
                                self.try_new_table()?
                            };
                            self.registers[a] = Value::Table(id);
                        }
                        self.maybe_gc()?;
                    }
                    Op::SetList => {
                        let count = if i.b == OPEN_ARGS {
                            self.top - a - 1
                        } else {
                            usize::from(i.b)
                        };
                        let table = self.registers[a];
                        self.table(table)?;
                        let Value::Table(id) = table else {
                            unreachable!("validated table")
                        };
                        // Lua 5.1 SETLIST reserves the complete list range
                        // before storing fields, including holes and trailing nil.
                        let end = c
                            .checked_add(count)
                            .and_then(|end| end.checked_sub(1))
                            .ok_or_else(|| self.memory_failure())?;
                        let target = &mut self.tables[slot_index(id.0)];
                        if c == 0
                            || end > u32::MAX as usize
                            || !target.set_scalar_list_in_place(
                                c - 1,
                                &self.registers[a + 1..a + 1 + count],
                            )
                        {
                            let before = target.heap_bytes();
                            let result = target.grow_list_array(end);
                            let after = target.heap_bytes();
                            self.note_table_resize(before, after);
                            result.map_err(|_| self.memory_failure())?;
                            if end <= u32::MAX as usize {
                                for offset in 0..count {
                                    let value = self.registers[a + 1 + offset];
                                    let index = c + offset - 1;
                                    if matches!(
                                        value,
                                        Value::Nil | Value::Boolean(_) | Value::Number(_)
                                    ) {
                                        self.tables[slot_index(id.0)].set_grown_list(index, value);
                                    } else {
                                        let key = Value::Number((index + 1) as f64);
                                        self.reserve_gc_table_write(id, key, value)?;
                                        self.tables[slot_index(id.0)].set_grown_list(index, value);
                                        self.gc_table_write(id, key, value);
                                    }
                                }
                            } else {
                                for offset in 0..count {
                                    let value = self.registers[a + 1 + offset];
                                    self.raw_set(id, Value::Number((c + offset) as f64), value)?;
                                }
                            }
                        }
                        if self.gc.pending {
                            self.maybe_gc()?;
                        }
                    }
                    Op::Closure => {
                        let target = if i.b == 0 {
                            c
                        } else {
                            self.programs[program_index].data.prototypes[proto_index].bindings[c]
                                .prototype as usize
                        };
                        let len = self.programs[program_index].data.prototypes[target]
                            .upvalues
                            .len();
                        let mut upvalues = Buffer::new_in(self.functions.memory().clone());
                        upvalues
                            .try_reserve_exact(len)
                            .map_err(|_| self.memory_failure())?;
                        self.functions
                            .try_reserve(1)
                            .map_err(|_| self.memory_failure())?;
                        for index in 0..len {
                            let capture = if i.b == 0 {
                                self.programs[program_index].data.prototypes[target].upvalues[index]
                            } else {
                                self.programs[program_index].data.prototypes[proto_index].bindings
                                    [c]
                                    .upvalues[index]
                            };
                            upvalues.push(match capture {
                                Upvalue::Local(r) => {
                                    self.capture_register(base + usize::from(r))?
                                }
                                Upvalue::Outer(u) => {
                                    self.functions[slot_index(function.0)].upvalues[usize::from(u)]
                                }
                            });
                        }
                        self.reserve_gc_new_value()?;
                        let id = self.insert_closure(Closure {
                            program: program_index,
                            prototype: target,
                            upvalues,
                            env,
                        });
                        self.registers[a] = Value::Function(id);
                        self.gc_new_value(Value::Function(id));
                        self.maybe_gc()?;
                    }
                    Op::GetUpvalue => {
                        let id = self.functions[slot_index(function.0)].upvalues[c] as usize;
                        self.registers[a] = self.cell_value(id);
                    }
                    Op::SetUpvalue => {
                        let id = self.functions[slot_index(function.0)].upvalues[c] as usize;
                        self.set_cell(id, self.registers[a])?;
                    }
                    Op::Close => self.close_upvalues(a),
                    Op::Vararg => {
                        let wanted = if i.c == OPEN { frame.argc() } else { c };
                        let copied = wanted.min(frame.argc());
                        self.resize_registers(self.registers.len().max(a + wanted));
                        self.registers
                            .copy_within(frame.vararg_start..frame.vararg_start + copied, a);
                        self.registers[a + copied..a + wanted].fill(Value::Nil);
                        self.top = a + wanted;
                    }
                    Op::GetTable => {
                        let target = self.registers[b];
                        let key = self.operand_value(base, program_index, proto_index, i.c);
                        if let Value::Table(id) = target
                            && self.tables[slot_index(id.0)].metatable().is_none()
                        {
                            self.registers[a] = self.tables[slot_index(id.0)].get(key);
                        } else {
                            self.get_table(target, key, a)?;
                        }
                    }
                    Op::SetTable => {
                        let target = self.registers[a];
                        let key =
                            self.operand_value(base, program_index, proto_index, u32::from(i.b));
                        let value = self.operand_value(base, program_index, proto_index, i.c);
                        if let Value::Table(id) = target
                            && self.tables[slot_index(id.0)].metatable().is_none()
                        {
                            self.raw_set(id, key, value)?;
                        } else {
                            self.set_table(target, key, value)?;
                        }
                        if self.gc.pending && self.frames.len() == frame_index + 1 {
                            self.maybe_gc()?;
                        }
                    }
                    Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                        let left =
                            self.operand_value(base, program_index, proto_index, u32::from(i.b));
                        let right = self.operand_value(base, program_index, proto_index, i.c);
                        if let (Value::Number(x), Value::Number(y)) = (left, right) {
                            self.registers[a] = Value::Number(Self::number_op(i.op, x, y));
                        } else {
                            self.arithmetic(i.op, left, right, a)?;
                        }
                    }
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                        let left =
                            self.operand_value(base, program_index, proto_index, u32::from(i.b));
                        let right = self.operand_value(base, program_index, proto_index, i.c);
                        if let (Value::Number(x), Value::Number(y)) = (left, right) {
                            self.registers[a] = Value::Boolean(match i.op {
                                Op::Eq => x == y,
                                Op::Ne => x != y,
                                Op::Lt => x < y,
                                Op::Le => x <= y,
                                Op::Gt => x > y,
                                Op::Ge => x >= y,
                                _ => unreachable!("numeric comparison opcode"),
                            });
                        } else {
                            self.compare(i.op, left, right, a)?;
                        }
                        if self.frames.len() == frame_index + 1
                            && self.hooks.mask & (hooks::LINE | hooks::COUNT) == 0
                            && let Some(next) = prototype.code.get(pc)
                        {
                            match next.op {
                                Op::Bool => {
                                    self.registers[base + usize::from(next.a)] = if next.c == 0 {
                                        self.registers[a]
                                    } else {
                                        Value::Boolean(!self.registers[a].is_truthy())
                                    };
                                    pc += 1;
                                }
                                Op::JumpFalse | Op::JumpTrue if next.b == COMPARE_BRANCH => {
                                    if self.registers[a].is_truthy() == (next.op == Op::JumpTrue) {
                                        pc = next.c as usize;
                                    } else {
                                        pc += 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Op::Concat => {
                        self.concatenate(self.registers[b], self.registers[base + c], a)?;
                        if self.frames.len() == frame_index + 1 {
                            self.maybe_gc()?;
                        }
                    }
                    Op::ConcatRange => {
                        self.concat_range(a, b, base + c)?;
                        if self.frames.len() == frame_index + 1 {
                            self.maybe_gc()?;
                        }
                    }
                    Op::Neg => {
                        if let Value::Number(x) = self.registers[b] {
                            self.registers[a] = Value::Number(-x);
                        } else {
                            self.arithmetic(Op::Neg, self.registers[b], self.registers[b], a)?;
                        }
                    }
                    Op::Not => self.registers[a] = Value::Boolean(!self.registers[b].is_truthy()),
                    Op::Len => self.length(self.registers[b], a)?,
                    Op::Jump => {
                        if i.b == crate::bytecode::NUMERIC_WHILE
                            && ignored_numeric_while != Some(pc - 1)
                            && !self.run_numeric_while(
                                &prototype.code,
                                c,
                                pc - 1,
                                base,
                                program_index,
                                proto_index,
                            )
                        {
                            ignored_numeric_while = Some(pc - 1);
                        }
                        pc = c;
                    }
                    Op::JumpFalse => {
                        if !self.registers[a].is_truthy() {
                            pc = c;
                        }
                    }
                    Op::JumpTrue => {
                        if self.registers[a].is_truthy() {
                            pc = c;
                        }
                    }
                    Op::ForPrep => {
                        let start = self.number(self.registers[a]).ok_or_else(|| {
                            Error::new(
                                ErrorKind::Runtime,
                                0,
                                "'for' initial value must be a number",
                            )
                        })?;
                        let limit = self.number(self.registers[a + 1]).ok_or_else(|| {
                            Error::new(ErrorKind::Runtime, 0, "'for' limit must be a number")
                        })?;
                        let step = self.number(self.registers[a + 2]).ok_or_else(|| {
                            Error::new(ErrorKind::Runtime, 0, "'for' step must be a number")
                        })?;
                        self.registers[a] = Value::Number(start - step);
                        self.registers[a + 1] = Value::Number(limit);
                        self.registers[a + 2] = Value::Number(step);
                        match self.reduce_numeric_loop(
                            base,
                            usize::from(i.a),
                            c,
                            prototype,
                            program_index,
                            proto_index,
                        ) {
                            Some(reductions::Reduction::Applied(skipped))
                                if self.budget.is_some() =>
                            {
                                self.remaining -= skipped;
                            }
                            Some(reductions::Reduction::Retry) => {
                                // The first call can still need a stack growth or GC checkpoint.
                                // Retry at the backedge after ordinary dispatch has run it.
                                reduction_retry = Some(reductions::Retry {
                                    frame: frame_index,
                                    pc: c,
                                    attempts: 8,
                                });
                            }
                            _ => {}
                        }
                        pc = c;
                    }
                    Op::ForLoop => {
                        if let Some(retry) = &mut reduction_retry
                            && retry.frame == frame_index
                            && retry.pc + 1 == pc
                        {
                            retry.attempts -= 1;
                            if retry.attempts == 0
                                || self.budget.is_some()
                                || !matches!(
                                    self.reduce_numeric_loop(
                                        base,
                                        usize::from(i.a),
                                        pc - 1,
                                        prototype,
                                        program_index,
                                        proto_index,
                                    ),
                                    Some(reductions::Reduction::Retry)
                                )
                            {
                                reduction_retry = None;
                            }
                        }
                        let (Value::Number(index), Value::Number(limit), Value::Number(step)) = (
                            self.registers[a],
                            self.registers[a + 1],
                            self.registers[a + 2],
                        ) else {
                            return Err(Error::new(
                                ErrorKind::Runtime,
                                0,
                                "invalid numeric for state",
                            ));
                        };
                        let next = index + step;
                        if if step > 0.0 {
                            next <= limit
                        } else {
                            next >= limit
                        } {
                            self.registers[a] = Value::Number(next);
                            self.registers[a + 3] = Value::Number(next);
                            pc = c;
                        }
                    }
                    Op::Call | Op::TailCall | Op::CallIter => {
                        if i.op == Op::Call
                            && i.b == 2
                            && i.c == 1
                            && self.registers[a] == Value::Native(debug::GET_INFO)
                            && let Some(get) = prototype.code.get(pc)
                            && get.op == Op::GetTable
                            && get.a == i.a
                            && usize::from(get.b) == usize::from(i.a)
                            && get.c >= crate::bytecode::RK
                            && self.try_debug_name_field(
                                a,
                                frame_index,
                                self.programs[program_index].bound_constants(proto_index)
                                    [(get.c - crate::bytecode::RK) as usize],
                            )
                        {
                            *error_line = prototype.instruction_line(pc);
                            pc += 1;
                            self.frames[frame_index].pc = pc as u32;
                            continue;
                        }
                        if i.op == Op::Call
                            && (i.c <= 1 || i.c == OPEN)
                            && let Value::Function(callee) = self.registers[a]
                            && let Some(projection) = return_argument_cache.get(self, callee)
                            && self.try_return_argument(
                                projection,
                                a,
                                i.b,
                                i.c,
                                checkpoint_ready,
                                error_line,
                            )
                        {
                            numeric_checkpoint_ready = self.open_upvalues.is_empty();
                            continue;
                        }
                        if i.op == Op::Call && i.b != OPEN_ARGS {
                            if i.b >= 2
                                && i.c <= 1
                                && let Value::Function(callee) = self.registers[a]
                                && let Some(next) = self.try_radix_selection(
                                    &mut radix_cache,
                                    callee,
                                    radix_selector::Window {
                                        code: &prototype.code,
                                        hints: &mut field_hints,
                                        caller: function,
                                        env,
                                        base,
                                        next: pc,
                                    },
                                    error_line,
                                )
                            {
                                pc = next;
                                numeric_checkpoint_ready = self.open_upvalues.is_empty();
                                continue;
                            }
                            let completed = match self.registers[a] {
                                Value::Native(LOAD_STRING)
                                    if i.b == 1 && matches!(i.c, 1 | OPEN) =>
                                {
                                    let completed = self.try_inline_source_load(a, i.c)?;
                                    if completed && !self.sort_gc_ready() {
                                        continue 'frames;
                                    }
                                    completed
                                }
                                Value::Native(NEXT | PAIRS_NEXT) if i.b == 2 && i.c == 2 => {
                                    self.try_inline_next_pair(a, checkpoint_ready)
                                }
                                Value::Native(0) if i.b == 1 && i.c <= 1 => {
                                    self.try_inline_assert(a, i.c == 1, checkpoint_ready)
                                }
                                Value::Native(id @ (32..=35 | math::MATH_FMOD)) if i.c <= 1 => self
                                    .try_inline_numeric_primitive(
                                        id,
                                        a,
                                        usize::from(i.b),
                                        c,
                                        checkpoint_ready,
                                    ),
                                Value::Native(id @ (math::MATH_MIN | math::MATH_MAX))
                                    if i.b == 2 && i.c <= 1 =>
                                {
                                    self.try_inline_extremum(
                                        a,
                                        i.c == 1,
                                        id == math::MATH_MIN,
                                        checkpoint_ready,
                                    )
                                }
                                Value::Native(threads::CO_RESUME) if i.b == 1 && i.c != OPEN => {
                                    self.try_inline_coroutine_resume(
                                        a,
                                        c,
                                        checkpoint_ready,
                                        &mut resume_hints,
                                    )
                                }
                                Value::Native(threads::CO_RESUME)
                                    if i.b == 2
                                        && i.c == 2
                                        && prototype
                                            .code
                                            .get(pc + 3)
                                            .is_some_and(|next| next.op == Op::GetTable) =>
                                {
                                    if let Some(next) =
                                        self.try_run_coroutine_feedback_cycle(pc - 1)
                                    {
                                        pc = next;
                                        true
                                    } else {
                                        false
                                    }
                                }
                                Value::Native(strings::STR_SUB) if i.b == 3 && i.c == 1 => {
                                    if !self.loads.is_empty() {
                                        self.try_inline_substring(a, checkpoint_ready)
                                    } else {
                                        let completed = self.try_inline_allocating_substring(a)?;
                                        if completed && self.frames.len() != frame_index + 1 {
                                            continue 'frames;
                                        }
                                        completed
                                    }
                                }
                                Value::Function(_) if i.c <= 2 => self.try_numeric_leaf_call(
                                    &mut numeric_call_cache,
                                    checkpoint_ready,
                                    a,
                                    usize::from(i.b),
                                    c,
                                    error_line,
                                ),
                                _ => false,
                            };
                            if completed {
                                numeric_checkpoint_ready = true;
                                continue;
                            }
                            if let Value::Function(function) = self.registers[a]
                                && i.c != OPEN
                                && !forwarding_cache.rejects(function, usize::from(i.b))
                                && self.try_forwarded_call(
                                    &mut forwarding_cache,
                                    function,
                                    a,
                                    usize::from(i.b),
                                    c,
                                    error_line,
                                )
                            {
                                continue;
                            }
                        }
                        let count = if i.b == OPEN_ARGS {
                            self.top.checked_sub(a + 1).ok_or_else(|| {
                                Error::new(ErrorKind::Runtime, 0, "invalid open argument range")
                            })?
                        } else {
                            usize::from(i.b)
                        };
                        if i.op == Op::CallIter
                            && (usize::from(frame.barriers)
                                + usize::from(self.threads[slot_index(self.thread.0)].depth)
                                >= 197
                                || self.threads[slot_index(self.thread.0)].native_bias != 0
                                || !self.loads.is_empty())
                        {
                            self.check_iterator_call()?;
                        }
                        let actual = self.start_lua_call(
                            a,
                            count,
                            if i.op == Op::TailCall { OPEN } else { i.c },
                            i.op == Op::TailCall,
                        )?;
                        let Some(actual) = actual else {
                            if i.op == Op::CallIter {
                                self.frames.last_mut().expect("scheduled iterator").barriers += 1;
                            }
                            continue 'frames;
                        };
                        if i.op == Op::TailCall {
                            if let Some(count) = self.return_frame(frame, a, actual)? {
                                return Ok(count);
                            }
                            continue 'frames;
                        }
                        self.finish_call(a, actual, i.c);
                        continue 'frames;
                    }
                    Op::Return => {
                        let count = if i.c == OPEN {
                            self.top.checked_sub(a).ok_or_else(|| {
                                Error::new(ErrorKind::Runtime, 0, "invalid open return range")
                            })?
                        } else {
                            c
                        };
                        if let Some(count) = self.return_frame(frame, a, count)? {
                            return Ok(count);
                        }
                        continue 'frames;
                    }
                }
                self.frames[frame_index].pc = pc as u32;
                if self.frames.len() != frame_index + 1 {
                    continue 'frames;
                }
            }
        }
    }

    #[inline]
    fn operand_value(&self, base: usize, program: usize, prototype: usize, operand: u32) -> Value {
        if operand < crate::bytecode::RK {
            self.registers[base + operand as usize]
        } else {
            self.programs[program].bound_constants(prototype)
                [(operand - crate::bytecode::RK) as usize]
        }
    }

    fn concat_bytes(&self, value: Value) -> Result<Vec<u8>, Error> {
        match value {
            Value::String(id) => {
                let source = self.string_bytes(id).unwrap_or_default();
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(source.len())
                    .map_err(|_| self.memory_failure())?;
                bytes.extend_from_slice(source);
                Ok(bytes)
            }
            Value::Number(n) => native::try_format_number(n).map_err(|_| self.memory_failure()),
            _ => Err(Error::new(
                ErrorKind::Runtime,
                0,
                format!("attempt to concatenate a {} value", value.type_name()),
            )),
        }
    }
}
