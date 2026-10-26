//! Register dispatch, interned byte strings, and native Lua tables.

use crate::{
    Error, ErrorKind, FunctionId, Program, StringId, TableId, ThreadId, UserdataId, Value,
    bytecode::{Constant, OPEN, OPEN_ARGS, Op, ProgramData, Upvalue},
    locale::parse_number,
    table::Table,
};
use std::{collections::HashMap, sync::Arc};
mod console;
mod debug;
mod errors;
mod gc;
mod hooks;
mod io;
mod limits;
mod loads;
mod math;
mod meta;
mod native;
mod os;
mod package;
mod standalone;
mod strings;
mod tables;
mod threads;
mod traces;
mod userdata;
use crate::arena::{Arena, index as slot_index};
pub use gc::{HeapStats, Root};
use native::format_number;

#[derive(Debug, Default)]
struct Strings {
    bytes: Arena<Box<[u8]>>,
    buckets: Vec<u32>,
}

impl Strings {
    fn hash(bytes: &[u8]) -> usize {
        // FNV-1a with a final avalanche; this table is internal and has no stored pointers.
        let mut h = 0xcbf29ce484222325u64;
        for &b in bytes {
            h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        h ^= h >> 32;
        h = h.wrapping_mul(0xd6e8feb86659fd93);
        (h ^ (h >> 32)) as usize
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
    fn add(&mut self, bytes: Box<[u8]>) -> StringId {
        if (self.bytes.live_len() + 1) * 4 >= self.buckets.len() * 3 {
            let mut buckets = vec![0; (self.buckets.len() * 2).max(16)];
            for (id, text) in self.bytes.entries() {
                let mut slot = Self::hash(text) & (buckets.len() - 1);
                while buckets[slot] != 0 {
                    slot = (slot + 1) & (buckets.len() - 1);
                }
                buckets[slot] = id as u32 + 1;
            }
            self.buckets = buckets;
        }
        let mut slot = Self::hash(&bytes) & (self.buckets.len() - 1);
        while self.buckets[slot] != 0 {
            slot = (slot + 1) & (self.buckets.len() - 1);
        }
        self.bytes.note_alloc(bytes.len());
        let id = self.bytes.insert(bytes);
        self.buckets[slot] = slot_index(id) as u32 + 1;
        StringId(id)
    }
    fn intern(&mut self, bytes: &[u8]) -> StringId {
        if let Some(id) = self.find(bytes) {
            self.bytes.preserve_new_reference(id.0);
            id
        } else {
            self.add(bytes.into())
        }
    }
    fn substring(&mut self, id: StringId, range: std::ops::Range<usize>) -> StringId {
        let bytes = &self.bytes[slot_index(id.0)][range];
        if let Some(id) = self.find(bytes) {
            self.bytes.preserve_new_reference(id.0);
            return id;
        }
        let owned = bytes.into();
        self.add(owned)
    }
}

#[derive(Debug)]
struct LoadedProgram {
    data: Arc<ProgramData>,
    owner: std::sync::Weak<()>,
    constants: Box<[Box<[Value]>]>,
    entry: FunctionId,
}

#[derive(Debug)]
struct Closure {
    program: usize,
    prototype: usize,
    upvalues: Box<[u32]>,
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
const _: () = assert!(std::mem::size_of::<Frame>() == 64);

/// A Lua state with persistent globals and reusable contiguous register storage.
///
/// Object values must only be used with the VM that produced them. Pin values
/// with [`Self::pin`] when the host needs them across executions or collections.
#[derive(Debug)]
pub struct Vm {
    registers: Vec<Value>,
    programs: Arena<LoadedProgram>,
    functions: Arena<Closure>,
    upvalues: Arena<Cell>,
    open_upvalues: Vec<u32>,
    frames: Vec<Frame>,
    call_slots: u16,
    strings: Strings,
    tables: Arena<Table>,
    globals: TableId,
    registry: TableId,
    top: usize,
    budget: Option<u64>,
    type_metatables: [Option<TableId>; 9],
    meta_names: [StringId; 19],
    output: Option<Vec<u8>>,
    active_native: Option<usize>,
    patterns: HashMap<(StringId, bool), crate::pattern::Pattern>,
    pattern_work: crate::pattern::Work,
    pattern_locale: u64,
    native_closures: Arena<threads::NativeClosure>,
    native_envs: HashMap<u64, TableId>,
    substitutions: Vec<strings::Substitution>,
    sorts: Vec<tables::Sort>,
    buffers: Vec<Vec<u8>>,
    threads: Arena<threads::Thread>,
    main: ThreadId,
    initial_globals: TableId,
    thread: ThreadId,
    transition: Option<threads::Transition>,
    gc: gc::Collector,
    roots: Arena<Value>,
    identity: u64,
    loads: Vec<loads::Loading>,
    tracebacks: Vec<traces::Traceback>,
    hooks: hooks::Hooks,
    userdata: Arena<userdata::Userdata>,
    io: io::Io,
    proxy_metatables: std::collections::HashSet<TableId>,
    userdata_order: u64,
    standalone: u64,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    /// Create a state with the currently implemented Lua 5.1 libraries.
    pub fn new() -> Self {
        let mut tables = Arena::default();
        let globals = TableId(tables.insert(Table::default()));
        let registry = TableId(tables.insert(Table::default()));
        let mut threads = Arena::default();
        let main = ThreadId(threads.insert(threads::Thread::main(globals)));
        let mut vm = Self {
            registers: Vec::new(),
            programs: Arena::default(),
            functions: Arena::default(),
            upvalues: Arena::default(),
            open_upvalues: Vec::new(),
            frames: Vec::new(),
            call_slots: 8,
            strings: Strings::default(),
            tables,
            globals,
            registry,
            top: 0,
            budget: None,
            type_metatables: [None; 9],
            meta_names: [StringId(0); 19],
            output: None,
            active_native: None,
            patterns: HashMap::new(),
            pattern_work: crate::pattern::Work::default(),
            pattern_locale: crate::locale::revision(),
            native_closures: Arena::default(),
            native_envs: HashMap::new(),
            substitutions: Vec::new(),
            sorts: Vec::new(),
            buffers: Vec::new(),
            threads,
            thread: main,
            main,
            initial_globals: globals,
            transition: None,
            gc: gc::Collector::default(),
            roots: Arena::default(),
            identity: gc::new_vm_identity(),
            loads: Vec::new(),
            tracebacks: Vec::new(),
            hooks: hooks::Hooks::default(),
            userdata: Arena::default(),
            io: io::Io::default(),
            proxy_metatables: std::collections::HashSet::new(),
            userdata_order: 0,
            standalone: 0,
        };
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
            vm.set_global(name, Value::Native(id as u64))
                .expect("base library names are valid");
        }
        vm.meta_names = META_NAMES.map(|name| vm.strings.intern(name));
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
            vm.set_global(name, Value::Native(id))
                .expect("base library names are valid");
        }
        let debug = vm.new_table();
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
            let key = vm.intern(name);
            vm.tables[slot_index(debug.0)]
                .set(key, Value::Native(id))
                .expect("debug library names are valid");
        }
        vm.set_global(b"debug", Value::Table(debug))
            .expect("debug name is valid");
        vm.set_global(b"_G", Value::Table(globals))
            .expect("global table name is valid");
        let version = vm.intern(b"Lua 5.1");
        vm.set_global(b"_VERSION", version)
            .expect("version name is valid");
        vm.open_math();
        vm.open_strings();
        vm.open_tables();
        vm.open_threads();
        vm.open_io();
        vm.open_os();
        let loaded = vm.new_table();
        let key = vm.intern(b"_LOADED");
        vm.raw_set(registry, key, Value::Table(loaded)).unwrap();
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
            let value = vm.global(name);
            let name = vm.intern(name);
            vm.raw_set(loaded, name, value).unwrap();
        }
        vm.open_package(loaded);
        vm.reset_gc_debt();
        vm
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
        Value::String(self.strings.intern(bytes))
    }

    fn take_buffer(&mut self, capacity: usize) -> Vec<u8> {
        let mut bytes = self.buffers.pop().unwrap_or_default();
        bytes.clear();
        bytes.reserve(capacity);
        bytes
    }
    fn finish_buffer(&mut self, bytes: Vec<u8>) -> Value {
        let value = self.intern(&bytes);
        self.buffers.push(bytes);
        value
    }

    /// Borrow bytes for a string identity produced by this VM.
    pub fn string_bytes(&self, id: StringId) -> Option<&[u8]> {
        self.strings.bytes.get(id.0).map(AsRef::as_ref)
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
            self.registers.truncate(1);
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
        self.validate_value(value)?;
        let k = self.intern(name);
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

    /// Set a table entry without invoking metamethods.
    ///
    /// # Errors
    /// Returns an error for foreign/collected handles, nil keys or NaN keys.
    pub fn raw_table_set(&mut self, table: TableId, key: Value, value: Value) -> Result<(), Error> {
        self.validate_value(Value::Table(table))?;
        self.validate_value(key)?;
        self.validate_value(value)?;
        self.raw_set(table, key, value)
    }

    fn raw_set(&mut self, id: TableId, key: Value, value: Value) -> Result<(), Error> {
        let table = &mut self.tables[slot_index(id.0)];
        let before = table.heap_bytes();
        if let Err(error) = table.set(key, value) {
            return Err(self.operand_error(error));
        }
        let growth = table.heap_bytes().saturating_sub(before);
        if growth != 0 {
            self.tables.note_alloc(growth);
            self.gc.pending = true;
        }
        self.gc_table_write(id, key, value);
        Ok(())
    }
    fn resize_registers(&mut self, len: usize) {
        let before = self.registers.capacity();
        self.registers.resize(len, Value::Nil);
        self.gc_note_control((self.registers.capacity() - before) * std::mem::size_of::<Value>());
    }
    fn push_record(&mut self, frame: Frame) {
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
        let id = TableId(self.tables.insert(Table::default()));
        self.gc_new_value(Value::Table(id));
        id
    }
    fn new_record_table(&mut self, capacity: usize) -> TableId {
        let table = Table::with_record_capacity(capacity);
        self.tables.note_alloc(table.heap_bytes());
        self.gc.pending = true;
        let id = TableId(self.tables.insert(table));
        self.gc_new_value(Value::Table(id));
        id
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

    fn load(&mut self, program: &Program) -> FunctionId {
        let cached = self
            .programs
            .entries()
            .find(|(_, p)| Arc::ptr_eq(&p.data, &program.data))
            .map(|(index, p)| (index, p.entry));
        if let Some((index, old_entry)) = cached {
            if self.functions.contains(old_entry.0) {
                self.gc_new_value(Value::Function(old_entry));
                return old_entry;
            }
            let upvalues = self.entry_upvalues(program);
            let entry = FunctionId(self.functions.insert(Closure {
                program: index,
                prototype: program.data.entry as usize,
                upvalues,
                env: self.globals,
            }));
            self.programs[index].entry = entry;
            self.gc_new_value(Value::Function(entry));
            return entry;
        }
        let mut constants = Vec::with_capacity(program.data.prototypes.len());
        for prototype in &program.data.prototypes {
            let mut values = Vec::with_capacity(prototype.constants.len());
            for k in &prototype.constants {
                values.push(match k {
                    Constant::Nil => Value::Nil,
                    Constant::Boolean(v) => Value::Boolean(*v),
                    Constant::Number(v) => Value::Number(*v),
                    Constant::String(s) => self.intern(s),
                });
            }
            constants.push(values.into_boxed_slice());
        }
        let index = slot_index(self.programs.insert(LoadedProgram {
            data: Arc::clone(&program.data),
            owner: Arc::downgrade(&program.lifetime),
            constants: constants.into_boxed_slice(),
            entry: FunctionId(0),
        }));
        let bytes = gc::program_bytes(&self.programs[index]);
        self.programs.note_alloc(bytes);
        let upvalues = self.entry_upvalues(program);
        let entry = FunctionId(self.functions.insert(Closure {
            program: index,
            prototype: program.data.entry as usize,
            upvalues,
            env: self.globals,
        }));
        self.programs[index].entry = entry;
        self.gc_new_value(Value::Function(entry));
        entry
    }

    fn entry_upvalues(&mut self, program: &Program) -> Box<[u32]> {
        let count = program.data.prototypes[program.data.entry as usize]
            .upvalues
            .len();
        let mut upvalues = Vec::with_capacity(count);
        for _ in 0..count {
            let id = slot_index(self.upvalues.insert(Cell::Closed(Value::Nil))) as u32;
            self.gc_new_cell(id as usize);
            upvalues.push(id);
        }
        self.functions.note_alloc(count * 4);
        upvalues.into_boxed_slice()
    }

    fn capture_register(&mut self, register: usize) -> u32 {
        if let Some(&id) = self
            .open_upvalues
            .iter()
            .find(|&&id| matches!(self.upvalues[id as usize], Cell::Open(_,r) if r == register))
        {
            return id;
        }
        let id = slot_index(self.upvalues.insert(Cell::Open(self.thread, register))) as u32;
        self.open_upvalues.push(id);
        self.gc_new_cell(id as usize);
        id
    }

    fn close_upvalues(&mut self, base: usize) {
        let mut i = 0;
        while i < self.open_upvalues.len() {
            let id = self.open_upvalues[i] as usize;
            if let Cell::Open(_, r) = self.upvalues[id]
                && r >= base
            {
                self.upvalues[id] = Cell::Closed(self.registers[r]);
                self.gc_cell_barrier(id, self.registers[r]);
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
        let (registers, params, compat_arg) = (
            usize::from(proto.registers),
            usize::from(proto.params),
            proto.compat_arg,
        );
        let base = self.registers.len();
        self.resize_registers(base + registers);
        self.registers
            .copy_within(call + 1..call + 1 + count.min(params), base);
        let vararg_count = count.saturating_sub(params);
        let vararg_start = call + 1 + count.min(params);
        if compat_arg {
            let t = self.new_table();
            for i in 0..vararg_count {
                self.raw_set(
                    t,
                    Value::Number((i + 1) as f64),
                    self.registers[vararg_start + i],
                )?;
            }
            let n = self.intern(b"n");
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
        });
        self.top = base;
        Ok(())
    }

    fn tail_frame(&mut self, function: FunctionId, call: usize, count: usize) -> Result<(), Error> {
        let frame = *self.frames.last().expect("tail call has an active frame");
        self.close_upvalues(frame.base);
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
        self.registers.truncate(end);
        if compat_arg {
            let t = self.new_table();
            for i in 0..extras {
                self.raw_set(
                    t,
                    Value::Number((i + 1) as f64),
                    self.registers[frame.stack_start + i],
                )?;
            }
            let n = self.intern(b"n");
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
            ..frame
        };
        self.top = base;
        Ok(())
    }

    fn return_frame(&mut self, frame: Frame, source: usize, count: usize) -> Option<usize> {
        if self.hooks.mask & hooks::RETURN != 0
            && self.hooks.allowed()
            && self.defer_hook_return(frame, source, count)
        {
            return None;
        }
        self.finish_frame(frame, source, count)
    }
    fn finish_frame(&mut self, frame: Frame, source: usize, count: usize) -> Option<usize> {
        self.close_upvalues(frame.base);
        if self.frames.len() == 1 {
            self.registers.copy_within(source..source + count, 0);
            self.registers.truncate(count);
            self.top = count;
            self.frames.clear();
            return Some(count);
        }
        let wanted = if frame.wanted == FRAME_OPEN {
            count
        } else {
            frame.wanted as usize
        };
        self.resize_registers(self.registers.len().max(frame.result + wanted));
        let copied = count.min(wanted);
        self.registers
            .copy_within(source..source + copied, frame.result);
        self.registers[frame.result + copied..frame.result + wanted].fill(Value::Nil);
        self.registers
            .truncate(frame.stack_start.max(frame.result + wanted));
        self.top = frame.result + wanted;
        self.frames.pop();
        None
    }

    /// Load a verified program as a function without executing it.
    ///
    /// The function is retained in the result registers until the next VM
    /// mutation. Pin it when retaining it across other executions or collections.
    /// A live Program also retains its entry binding; dropping the Program lets
    /// Lua ownership alone determine the function/environment's lifetime.
    pub fn load_program(&mut self, program: &Program) -> FunctionId {
        let function = self.load(program);
        self.registers.clear();
        self.registers.push(Value::Function(function));
        self.top = 1;
        function
    }

    /// Execute verified bytecode, retaining globals and reusing register storage.
    ///
    /// # Errors
    /// Returns Lua runtime errors or an instruction-limit error. Returned values
    /// borrow the register storage until the next mutation of the VM.
    pub fn execute(&mut self, program: &Program) -> Result<&[Value], Error> {
        if self.standalone != 0 {
            let function = self.load(program);
            return self.call_root(Value::Function(function), &[]);
        }
        let function = self.load(program);
        let closure = &self.functions[slot_index(function.0)];
        let entry = &self.programs[closure.program].data.prototypes[closure.prototype];
        let compat_arg = entry.compat_arg;
        let params = usize::from(entry.params);
        let registers = usize::from(entry.registers);
        self.registers.clear();
        self.resize_registers(registers);
        if compat_arg {
            let arg = self.new_table();
            let n = self.intern(b"n");
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
        let function = self.load(program);
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
        if self.standalone != 0 {
            return self.standalone_call(function, arguments, standalone::EntryMode::Message);
        }
        if arguments.len() > u32::MAX as usize {
            return Err(Error::new(ErrorKind::Limit, 0, "too many arguments"));
        }
        self.registers.clear();
        self.registers.push(function);
        self.registers.extend_from_slice(arguments);
        self.frames.clear();
        self.top = 0;
        let result = match self.start_lua_call(0, arguments.len(), OPEN, false) {
            Ok(Some(count)) => {
                self.registers.truncate(count);
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
            Err(error) => {
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
        let mut budget = self.budget;
        let result = loop {
            let result = self.run(&mut budget, &mut error_line);
            match result {
                Err(error) if error.kind() == ErrorKind::Runtime => {
                    match self.catch_error(&error, &mut budget) {
                        Ok(true) => continue,
                        Ok(false) if self.thread != self.main => {
                            self.fail_thread(&error, &mut error_line);
                            continue;
                        }
                        Ok(false) => break Err(error),
                        Err(next) => break Err(next),
                    }
                }
                Ok(count) if self.thread != self.main => {
                    self.finish_thread(0, count, false, false, &mut error_line);
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
    fn charge(budget: &mut Option<u64>) -> Result<(), Error> {
        if let Some(n) = budget {
            if *n == 0 {
                return Err(Error::new(
                    ErrorKind::Limit,
                    0,
                    "instruction limit exceeded",
                ));
            }
            *n -= 1;
        }
        Ok(())
    }

    fn run(&mut self, budget: &mut Option<u64>, error_line: &mut u32) -> Result<usize, Error> {
        'frames: loop {
            self.maybe_gc();
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
                    Self::charge(budget)?;
                    let previous = self.active_native.replace(frame_index);
                    let result = self.step_native(id, frame).map_err(|error| {
                        if error.kind() == ErrorKind::Runtime && error.value().is_none() {
                            let value = self.intern(error.message_bytes());
                            let value = self.located_error(value, 1);
                            error.with_value(value)
                        } else {
                            error
                        }
                    });
                    self.active_native = previous;
                    if let Some(count) = result? {
                        return Ok(count);
                    }
                    self.apply_transition(error_line)?;
                    continue;
                }
                FrameKind::Lua(id) => id,
            };
            let closure = &self.functions[slot_index(function.0)];
            let (program_index, proto_index, env) =
                (closure.program, closure.prototype, closure.env);
            // Hold immutable code independently of VM-vector growth during native calls.
            // Shared ownership is acquired once on frame entry, never per instruction/value.
            let data = Arc::clone(&self.programs[program_index].data);
            let prototype = &data.prototypes[proto_index];
            let mut pc = frame.pc();
            let base = frame.base;
            loop {
                if self.hooks.mask & (hooks::LINE | hooks::COUNT) != 0
                    && self.hook_instruction(frame_index, pc, prototype.debug.hook_lines[pc])?
                {
                    continue 'frames;
                }
                Self::charge(budget)?;
                let i = prototype.code[pc];
                *error_line = prototype.lines[pc];
                pc += 1;
                let a = base + usize::from(i.a);
                let b = base + usize::from(i.b);
                let c = i.c as usize;
                self.frames[frame_index].pc = pc as u32;
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
                    Op::Move | Op::Bool => self.registers[a] = self.registers[b],
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
                        let id = self.new_table();
                        self.registers[a] = Value::Table(id);
                        self.maybe_gc();
                    }
                    Op::SetList => {
                        let count = if i.b == OPEN_ARGS {
                            self.top - a - 1
                        } else {
                            usize::from(i.b)
                        };
                        let table = self.registers[a];
                        for offset in 0..count {
                            let value = self.registers[a + 1 + offset];
                            self.table(table)?;
                            let Value::Table(id) = table else {
                                unreachable!("validated table")
                            };
                            self.raw_set(id, Value::Number((c + offset) as f64), value)?;
                        }
                        if self.gc.pending {
                            self.maybe_gc();
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
                        let mut upvalues = Vec::with_capacity(len);
                        for index in 0..len {
                            let capture = if i.b == 0 {
                                self.programs[program_index].data.prototypes[target].upvalues[index]
                            } else {
                                self.programs[program_index].data.prototypes[proto_index].bindings
                                    [c]
                                    .upvalues[index]
                            };
                            upvalues.push(match capture {
                                Upvalue::Local(r) => self.capture_register(base + usize::from(r)),
                                Upvalue::Outer(u) => {
                                    self.functions[slot_index(function.0)].upvalues[usize::from(u)]
                                }
                            });
                        }
                        self.functions.note_alloc(upvalues.len() * 4);
                        let id = FunctionId(self.functions.insert(Closure {
                            program: program_index,
                            prototype: target,
                            upvalues: upvalues.into_boxed_slice(),
                            env,
                        }));
                        self.registers[a] = Value::Function(id);
                        self.gc_new_value(Value::Function(id));
                        self.maybe_gc();
                    }
                    Op::GetUpvalue => {
                        let id = self.functions[slot_index(function.0)].upvalues[c] as usize;
                        self.registers[a] = self.cell_value(id);
                    }
                    Op::SetUpvalue => {
                        let id = self.functions[slot_index(function.0)].upvalues[c] as usize;
                        self.set_cell(id, self.registers[a]);
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
                        let key = self.registers[base + c];
                        if let Value::Table(id) = target
                            && self.tables[slot_index(id.0)].metatable.is_none()
                        {
                            self.registers[a] = self.tables[slot_index(id.0)].get(key);
                        } else {
                            self.get_table(target, key, a)?;
                        }
                    }
                    Op::SetTable => {
                        let target = self.registers[a];
                        let key = self.registers[b];
                        let value = self.registers[base + c];
                        if let Value::Table(id) = target
                            && self.tables[slot_index(id.0)].metatable.is_none()
                        {
                            self.raw_set(id, key, value)?;
                        } else {
                            self.set_table(target, key, value)?;
                        }
                        if self.gc.pending && self.frames.len() == frame_index + 1 {
                            self.maybe_gc();
                        }
                    }
                    Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                        let (left, right) = (self.registers[b], self.registers[base + c]);
                        if let (Value::Number(x), Value::Number(y)) = (left, right) {
                            self.registers[a] = Value::Number(Self::number_op(i.op, x, y));
                        } else {
                            self.arithmetic(i.op, left, right, a)?;
                        }
                    }
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                        let (left, right) = (self.registers[b], self.registers[base + c]);
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
                            && next.op == Op::Bool
                        {
                            self.registers[base + usize::from(next.a)] = self.registers[a];
                            pc += 1;
                        }
                    }
                    Op::Concat => {
                        self.concatenate(self.registers[b], self.registers[base + c], a)?;
                        if self.frames.len() == frame_index + 1 {
                            self.maybe_gc();
                        }
                    }
                    Op::ConcatRange => {
                        self.concat_range(a, b, base + c)?;
                        if self.frames.len() == frame_index + 1 {
                            self.maybe_gc();
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
                            if let Some(count) = self.return_frame(frame, a, actual) {
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
                        if let Some(count) = self.return_frame(frame, a, count) {
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

    fn concat_bytes(&self, value: Value) -> Result<Vec<u8>, Error> {
        match value {
            Value::String(id) => Ok(self.string_bytes(id).unwrap_or_default().to_vec()),
            Value::Number(n) => Ok(format_number(n)),
            _ => Err(Error::new(
                ErrorKind::Runtime,
                0,
                format!("attempt to concatenate a {} value", value.type_name()),
            )),
        }
    }
}
