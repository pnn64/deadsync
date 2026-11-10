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
mod os;
mod package;
mod reductions;
mod standalone;
mod strings;
mod tables;
mod threads;
mod traces;
mod userdata;
use crate::arena::{Arena, index as slot_index};
use crate::memory::{Buffer, Memory};
pub use gc::{GcOperation, HeapStats, Root};
pub use host::{
    HostCall, HostFunction, HostThreadState, NativeBuiltinBody, NativeCallStatus, NativeDebugFrame,
    NativeDebugInfo, NativeDebugString, NativeHook, NativeHookEvent, NativeHookInfo,
    NativeLoadStatus, NativeStack, NativeThreadStack,
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
struct ByteString(Buffer<u8>);
impl ByteString {
    fn try_new(bytes: &[u8], memory: Memory) -> Result<Self, Error> {
        let size = bytes.len().checked_add(1).ok_or_else(Error::memory)?;
        let mut storage = Buffer::new_in(memory);
        storage
            .try_reserve_exact(size)
            .map_err(|_| Error::memory())?;
        storage.extend_from_slice(bytes);
        storage.push(0);
        // Keep the vector: shrinking an overallocated buffer to a boxed slice
        // must not introduce an infallible reallocation on this failure path.
        Ok(Self(storage))
    }
    fn c_bytes(&self) -> &[u8] {
        &self.0
    }
    fn storage_bytes(&self) -> usize {
        self.0.capacity()
    }
}
impl std::ops::Deref for ByteString {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0[..self.0.len() - 1]
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
    buckets: Buffer<u32>,
}

impl Strings {
    fn new_in(memory: Memory) -> Self {
        Self {
            bytes: Arena::new_in(memory.clone()),
            buckets: Buffer::new_in(memory),
        }
    }
    fn hash(bytes: &[u8]) -> usize {
        const A: u64 = 0x9e3779b185ebca87;
        const B: u64 = 0xc2b2ae3d27d4eb4f;
        fn word(bytes: &[u8]) -> u64 {
            u64::from_le_bytes(bytes[..8].try_into().unwrap())
        }
        fn tail(bytes: &[u8]) -> u64 {
            let n = bytes.len();
            if n >= 4 {
                u64::from(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
                    | (u64::from(u32::from_le_bytes(bytes[n - 4..].try_into().unwrap())) << 32)
            } else if n != 0 {
                u64::from(bytes[0])
                    | (u64::from(bytes[n / 2]) << 8)
                    | (u64::from(bytes[n - 1]) << 16)
            } else {
                0
            }
        }
        let mut h = (bytes.len() as u64).wrapping_mul(A) ^ B;
        if bytes.len() <= 16 {
            h ^= if bytes.len() >= 8 {
                word(bytes).wrapping_mul(A)
                    ^ word(&bytes[bytes.len() - 8..])
                        .rotate_left(27)
                        .wrapping_mul(B)
            } else {
                tail(bytes).wrapping_mul(B)
            };
        } else {
            // Four independent lanes consume every byte; equality still checks
            // the complete slice. Loads copy bytes safely, including unaligned input.
            let mut lanes = [A, B, !A, !B];
            let (blocks, rest) = bytes.as_chunks::<32>();
            for block in blocks {
                for (lane, bytes) in lanes.iter_mut().zip(block.as_chunks::<8>().0) {
                    *lane = (*lane ^ u64::from_le_bytes(*bytes))
                        .rotate_left(29)
                        .wrapping_mul(A);
                }
            }
            h ^= lanes[0].wrapping_add(lanes[1].rotate_left(13))
                ^ lanes[2]
                    .rotate_left(31)
                    .wrapping_add(lanes[3].rotate_left(47));
            let (words, rest) = rest.as_chunks::<8>();
            for bytes in words {
                h = (h ^ u64::from_le_bytes(*bytes))
                    .rotate_left(27)
                    .wrapping_mul(B);
            }
            h ^= tail(rest).wrapping_mul(A);
        }
        h ^= h >> 29;
        h = h.wrapping_mul(0x165667b19e3779f9);
        (h ^ (h >> 31)) as usize
    }
    fn find(&self, bytes: &[u8]) -> Option<StringId> {
        if self.buckets.is_empty() {
            return None;
        }
        let mut slot = Self::hash(bytes) & (self.buckets.len() - 1);
        while self.buckets[slot] != 0 {
            let id = self.buckets[slot] - 1;
            if self.bytes[id as usize].as_ref() == bytes {
                return self.bytes.key(id as usize).map(StringId);
            }
            slot = (slot + 1) & (self.buckets.len() - 1);
        }
        None
    }
    fn try_add(&mut self, bytes: ByteString) -> Result<StringId, Error> {
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
                let mut slot = Self::hash(text) & (buckets.len() - 1);
                while buckets[slot] != 0 {
                    slot = (slot + 1) & (buckets.len() - 1);
                }
                buckets[slot] = id as u32 + 1;
            }
            replacement = Some(buckets);
        }
        let hash = Self::hash(&bytes);
        let storage = bytes.storage_bytes();
        // Publish neither the hash index nor the string until every allocation
        // has succeeded. Arena accounting includes partially reserved buffers.
        let id = self.bytes.try_insert(bytes)?;
        if let Some(buckets) = replacement {
            let growth =
                buckets.capacity().saturating_sub(self.buckets.capacity()) * size_of::<u32>();
            self.bytes.note_alloc(growth);
            self.buckets = buckets;
        }
        let mut slot = hash & (self.buckets.len() - 1);
        while self.buckets[slot] != 0 {
            slot = (slot + 1) & (self.buckets.len() - 1);
        }
        self.bytes.note_alloc(storage);
        self.buckets[slot] = slot_index(id) as u32 + 1;
        Ok(StringId(id))
    }
    #[cfg(test)]
    fn intern(&mut self, bytes: &[u8]) -> StringId {
        self.try_intern(bytes).expect("allocate Lua string")
    }
    fn try_intern(&mut self, bytes: &[u8]) -> Result<StringId, Error> {
        if let Some(id) = self.find(bytes) {
            self.bytes.preserve_new_reference(id.0);
            Ok(id)
        } else {
            self.try_add(ByteString::try_new(bytes, self.bytes.memory().clone())?)
        }
    }
    fn substring(
        &mut self,
        id: StringId,
        range: std::ops::Range<usize>,
    ) -> Result<StringId, Error> {
        let bytes = &self.bytes[slot_index(id.0)][range];
        if let Some(id) = self.find(bytes) {
            self.bytes.preserve_new_reference(id.0);
            return Ok(id);
        }
        let owned = ByteString::try_new(bytes, self.bytes.memory().clone())?;
        self.try_add(owned)
    }
}

#[derive(Debug)]
struct LoadedProgram {
    data: crate::memory::Shared<ProgramData>,
    owner: crate::memory::Shared<std::sync::atomic::AtomicUsize>,
    _budget: crate::memory::Reservation,
    constants: Buffer<Buffer<Value>>,
    upvalue_names: Buffer<Buffer<StringId>>,
    debug_strings: Buffer<StringId>,
    // Non-owning prototype map. Each live closure traces only its own subtree.
    parser_prototypes: Buffer<ParserPrototypeId>,
    entry: FunctionId,
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
    registers: Buffer<Value>,
    programs: Arena<LoadedProgram>,
    functions: Arena<Closure>,
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
    pattern_locale: u64,
    native_closures: Arena<threads::NativeClosure>,
    builtin_envs: Buffer<Option<TableId>>,
    substitutions: Buffer<strings::Substitution>,
    sorts: Buffer<tables::Sort>,
    buffers: Buffer<Buffer<u8>>,
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
            registers: Buffer::new_in(memory.clone()),
            programs: Arena::new_in(memory.clone()),
            functions: Arena::identified_in(memory.clone()),
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
            pattern_locale: crate::locale::revision(),
            native_closures: Arena::identified_in(memory.clone()),
            builtin_envs,
            substitutions: Buffer::new_in(memory.clone()),
            sorts: Buffer::new_in(memory.clone()),
            buffers: Buffer::new_in(memory.clone()),
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
                files: Arena::new_in(memory),
                ..io::Io::default()
            },
            userdata_order: 0,
            standalone: 0,
            sort_line: None,
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
            self.tables[slot_index(debug.0)].set(key, Value::Native(id))?;
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
        let bytes = native::try_format_number(number).map_err(|_| self.memory_failure())?;
        self.try_intern(&bytes)
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
                let Value::String(id) = self.try_intern(&format_number(number))? else {
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
        Table::validate_set_key(key).map_err(|error| self.operand_error(error))?;
        self.reserve_gc_table_write(id, key, value)?;
        let table = &mut self.tables[slot_index(id.0)];
        let before = table.heap_bytes();
        let result = table.set(key, value);
        let growth = table.heap_bytes().saturating_sub(before);
        if growth != 0 {
            self.tables.note_alloc(growth);
            self.gc.pending = true;
        }
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
    fn memory_failure(&self) -> Error {
        Error::memory().with_value(Value::String(self.memory_error))
    }
    fn reserve_registers(&mut self, len: usize) -> Result<(), Error> {
        let before = self.registers.capacity();
        self.registers
            .try_reserve(len.saturating_sub(self.registers.len()))
            .map_err(|_| self.memory_failure())?;
        self.gc_note_control((self.registers.capacity() - before) * std::mem::size_of::<Value>());
        Ok(())
    }
    fn resize_registers(&mut self, len: usize) {
        self.hooks.preserve_popped(&self.registers, len);
        let before = self.registers.capacity();
        self.registers.resize(len, Value::Nil);
        self.gc_note_control((self.registers.capacity() - before) * std::mem::size_of::<Value>());
    }
    fn truncate_registers(&mut self, len: usize) {
        self.hooks.preserve_popped(&self.registers, len);
        self.registers.truncate(len);
    }
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
        let table = Table::with_capacity_in(array, records, self.tables.memory().clone()).map_err(
            |error| {
                if error.kind() == ErrorKind::Memory {
                    self.memory_failure()
                } else {
                    error
                }
            },
        )?;
        self.reserve_gc_new_value()?;
        let bytes = table.heap_bytes();
        let id = TableId(
            self.tables
                .try_insert(table)
                .map_err(|_| self.memory_failure())?,
        );
        self.tables.note_alloc(bytes);
        self.gc.pending |= bytes != 0;
        self.gc_new_value(Value::Table(id));
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
            self.functions
                .note_alloc(upvalues.capacity() * size_of::<u32>());
            let entry = FunctionId(self.functions.insert(Closure {
                program: index,
                prototype: program.data.entry as usize,
                upvalues,
                env: self.globals,
            }));
            self.programs[index].entry = entry;
            self.gc_new_value(Value::Function(entry));
            return Ok(entry);
        }
        let memory = self.programs.memory().clone();
        let budget = memory.reserve_external(|| program.heap_bytes())?;
        let mut constants = Buffer::new_in(memory.clone());
        let mut upvalue_names = Buffer::new_in(memory.clone());
        constants.try_reserve_exact(program.data.prototypes.len())?;
        upvalue_names.try_reserve_exact(program.data.prototypes.len())?;
        for prototype in &program.data.prototypes {
            let mut values = Buffer::new_in(memory.clone());
            values.try_reserve_exact(prototype.constants.len())?;
            for k in &prototype.constants {
                values.push(match k {
                    Constant::Nil => Value::Nil,
                    Constant::Boolean(v) => Value::Boolean(*v),
                    Constant::Number(v) => Value::Number(*v),
                    Constant::String(s) => self.try_intern(s)?,
                });
            }
            constants.push(values);
            let count = prototype
                .debug
                .native
                .as_ref()
                .map_or(prototype.debug.upvalues.len(), |native| native.wire.names);
            let mut names = Buffer::new_in(memory.clone());
            names.try_reserve_exact(count)?;
            for name in &prototype.debug.upvalues[..count] {
                let Value::String(id) = self.try_intern(name)? else {
                    unreachable!()
                };
                names.push(id);
            }
            upvalue_names.push(names);
        }
        let loaded = LoadedProgram {
            data: program.data.clone(),
            owner: program.lifetime.clone(),
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
        self.functions
            .note_alloc(upvalues.capacity() * size_of::<u32>());
        let entry = FunctionId(self.functions.insert(Closure {
            program: index,
            prototype: program.data.entry as usize,
            upvalues,
            env: self.globals,
        }));
        self.programs[index].entry = entry;
        self.gc_new_value(Value::Function(entry));
        Ok(entry)
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
        self.resize_registers(base + registers);
        self.registers
            .copy_within(call + 1..call + 1 + count.min(params), base);
        // lua_pcall keeps a stack-slot reference, including when that slot is
        // passed as a parameter. Follow moved fixed arguments; vararg entry
        // clears their original slots as Lua 5.1 adjust_varargs does.
        let fixed = call + 1..call + 1 + count.min(params);
        if vararg {
            self.discard_native_error_function_slots(fixed);
        } else {
            self.relocate_native_error_functions(fixed, base);
            self.discard_native_error_function_slots(
                call + 1 + count.min(params)..call + 1 + count,
            );
        }
        if self
            .frames
            .last()
            .is_some_and(|frame| matches!(frame.function, FrameKind::Native(CONT_HOST_META)))
        {
            // Native API operands remain in the visible C window. Their
            // private metamethod argument copies stop owning roots after
            // entering Lua; vararg extras remain in the callee's vararg area.
            let copied = if vararg { count.min(params) } else { count };
            self.registers[call + 1..call + 1 + copied].fill(Value::Nil);
        }
        let vararg_count = count.saturating_sub(params);
        let vararg_start = call + 1 + count.min(params);
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
        self.reserve_registers(scratch + count)?;
        self.close_upvalues(frame.base);
        self.resize_registers(scratch + count);
        self.registers
            .copy_within(call + 1..call + 1 + count, scratch);
        self.registers[frame.stack_start..end].fill(Value::Nil);
        self.registers
            .copy_within(scratch..scratch + count.min(params), base);
        self.registers.copy_within(
            scratch + count.min(params)..scratch + count,
            frame.stack_start,
        );
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
        if self.frames.len() > 1 {
            self.reserve_registers(frame.stack_start.max(frame.result + wanted))?;
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
        self.resize_registers(
            self.registers
                .len()
                .max(frame.stack_start)
                .max(frame.result + wanted),
        );
        let copied = count.min(wanted);
        self.registers
            .copy_within(source..source + copied, frame.result);
        self.registers[frame.result + copied..frame.result + wanted].fill(Value::Nil);
        self.registers
            .truncate(frame.stack_start.max(frame.result + wanted));
        self.top = frame.result + wanted;
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
                    let result = self
                        .step_native(id, frame)
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
            // Shared ownership is acquired once on frame entry, never per instruction/value.
            let data = self.programs[program_index].data.clone();
            let prototype = &data.prototypes[proto_index];
            let mut pc = frame.pc();
            let base = frame.base;
            loop {
                if self.hooks.mask & (hooks::LINE | hooks::COUNT) != 0
                    && self.hook_instruction(frame_index, pc, prototype.debug.hook_lines[pc])?
                {
                    continue 'frames;
                }
                self.charge()?;
                let i = prototype.code[pc];
                *error_line = prototype.lines[pc];
                pc += 1;
                let a = base + usize::from(i.a);
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
                match i.op {
                    Op::Load => {
                        // Initial nil slots were cleared on frame entry. A call
                        // hook can already have replaced them before execution.
                        if i.b == 0 {
                            self.registers[a] =
                                self.programs[program_index].constants[proto_index][c];
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
                        self.programs[program_index].constants[proto_index][c],
                        a,
                    )?,
                    Op::SetGlobal => self.set_table(
                        Value::Table(env),
                        self.programs[program_index].constants[proto_index][c],
                        self.registers[a],
                    )?,
                    Op::NewTable => {
                        let id = self.try_new_table()?;
                        self.registers[a] = Value::Table(id);
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
                        let before = target.heap_bytes();
                        let result = target.grow_list_array(end);
                        let growth = target.heap_bytes().saturating_sub(before);
                        if growth != 0 {
                            self.tables.note_alloc(growth);
                            self.gc.pending = true;
                        }
                        result.map_err(|_| self.memory_failure())?;
                        for offset in 0..count {
                            let value = self.registers[a + 1 + offset];
                            self.raw_set(id, Value::Number((c + offset) as f64), value)?;
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
                        self.functions
                            .note_alloc(upvalues.capacity() * size_of::<u32>());
                        let id = FunctionId(self.functions.insert(Closure {
                            program: program_index,
                            prototype: target,
                            upvalues,
                            env,
                        }));
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
                    Op::Jump => pc = c,
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
                        if self.hooks.mask & (hooks::LINE | hooks::COUNT) == 0
                            // A skipped Close must not capture an intermediate
                            // register value in an existing open upvalue.
                            && !self.open_upvalues.iter().any(|&id| {
                                matches!(self.upvalues[id as usize], Cell::Open(_, r) if r >= base)
                            })
                            && let Some(skipped) = reductions::sum_loop(
                                &mut self.registers,
                                base,
                                usize::from(i.a),
                                c,
                                &prototype.code,
                                self.budget.map(|_| self.remaining),
                            )
                            && self.budget.is_some()
                        {
                            self.remaining -= skipped;
                        }
                        pc = c;
                    }
                    Op::ForLoop => {
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
                        let count = if i.b == OPEN_ARGS {
                            self.top.checked_sub(a + 1).ok_or_else(|| {
                                Error::new(ErrorKind::Runtime, 0, "invalid open argument range")
                            })?
                        } else {
                            usize::from(i.b)
                        };
                        self.frames[frame_index].pc = pc as u32;
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
            self.programs[program].constants[prototype][(operand - crate::bytecode::RK) as usize]
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
