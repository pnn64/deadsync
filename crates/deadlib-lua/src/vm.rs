//! Register dispatch, interned byte strings, and native Lua tables.

use crate::{
    Error, ErrorKind, FunctionId, Program, StringId, TableId, Value,
    bytecode::{Constant, OPEN, OPEN_ARGS, Op, ProgramData, Upvalue},
    lexer::parse_number,
    table::Table,
};
use std::{collections::HashMap, sync::Arc};
mod math;
mod meta;
mod native;
mod strings;
mod tables;
use native::format_number;

#[derive(Debug, Default)]
struct Strings {
    bytes: Vec<Box<[u8]>>,
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
                return Some(StringId(id));
            }
            slot = (slot + 1) & (self.buckets.len() - 1);
        }
        None
    }
    fn add(&mut self, bytes: Box<[u8]>) -> StringId {
        if (self.bytes.len() + 1) * 4 >= self.buckets.len() * 3 {
            let mut buckets = vec![0; (self.buckets.len() * 2).max(16)];
            for (id, text) in self.bytes.iter().enumerate() {
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
        let id =
            u32::try_from(self.bytes.len()).expect("Lua string arena exhausted addressable memory");
        self.bytes.push(bytes);
        self.buckets[slot] = id + 1;
        StringId(id)
    }
    fn intern(&mut self, bytes: &[u8]) -> StringId {
        self.find(bytes).unwrap_or_else(|| self.add(bytes.into()))
    }
    fn substring(&mut self, id: StringId, range: std::ops::Range<usize>) -> StringId {
        let bytes = &self.bytes[id.0 as usize][range];
        if let Some(id) = self.find(bytes) {
            return id;
        }
        let owned = bytes.into();
        self.add(owned)
    }
}

#[derive(Debug)]
struct LoadedProgram {
    data: Arc<ProgramData>,
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
    Open(usize),
    Closed(Value),
}

// Numeric native IDs are shared by globals and the explicit continuation frames.
const NEXT: u32 = 9;
const PAIRS: u32 = 10;
const IPAIRS: u32 = 11;
const IPAIRS_STEP: u32 = 12;
const GET_META: u32 = 13;
const SET_META: u32 = 14;
const PCALL: u32 = 15;
const XPCALL: u32 = 16;
const TOSTRING: u32 = 17;
const PRINT: u32 = 18;
const DEBUG_GET_META: u32 = 19;
const DEBUG_SET_META: u32 = 20;
const PAIRS_NEXT: u32 = 21;
const GET_ENV: u32 = 22;
const SET_ENV: u32 = 23;
const LOAD_STRING: u32 = 24;
const CONT_RAW: u32 = 128;
const CONT_BOOL: u32 = 129;
const CONT_NOT: u32 = 130;
const CONT_IGNORE: u32 = 131;
const CONT_HANDLER: u32 = 132;

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
}
const META_NAMES: [&[u8]; 17] = [
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
];

#[derive(Clone, Copy, Debug)]
enum FrameKind {
    Lua(FunctionId),
    Native(u32),
}

#[derive(Clone, Copy, Debug)]
struct Frame {
    function: FrameKind,
    pc: usize,
    base: usize,
    stack_start: usize,
    result: usize,
    wanted: u32,
    vararg_start: usize,
    vararg_count: usize,
    tail_calls: u32,
}

/// A Lua state with persistent globals and reusable contiguous register storage.
///
/// Object values must only be used with the VM that produced them. The current
/// collector is not implemented; heap objects remain alive until the VM is dropped.
#[derive(Debug)]
pub struct Vm {
    registers: Vec<Value>,
    programs: Vec<LoadedProgram>,
    functions: Vec<Closure>,
    upvalues: Vec<Cell>,
    open_upvalues: Vec<u32>,
    frames: Vec<Frame>,
    strings: Strings,
    tables: Vec<Table>,
    globals: TableId,
    top: usize,
    budget: Option<u64>,
    type_metatables: [Option<TableId>; 6],
    meta_names: [StringId; 17],
    output: Option<Vec<u8>>,
    active_native: Option<usize>,
    patterns: HashMap<(StringId, bool), crate::pattern::Pattern>,
    pattern_work: crate::pattern::Work,
    match_iterators: Vec<strings::MatchIterator>,
    substitutions: Vec<strings::Substitution>,
    sorts: Vec<tables::Sort>,
    buffers: Vec<Vec<u8>>,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    /// Create a state with the currently implemented base and math functions.
    pub fn new() -> Self {
        let mut vm = Self {
            registers: Vec::new(),
            programs: Vec::new(),
            functions: Vec::new(),
            upvalues: Vec::new(),
            open_upvalues: Vec::new(),
            frames: Vec::new(),
            strings: Strings::default(),
            tables: vec![Table::default()],
            globals: TableId(0),
            top: 0,
            budget: None,
            type_metatables: [None; 6],
            meta_names: [StringId(0); 17],
            output: None,
            active_native: None,
            patterns: HashMap::new(),
            pattern_work: crate::pattern::Work::default(),
            match_iterators: Vec::new(),
            substitutions: Vec::new(),
            sorts: Vec::new(),
            buffers: Vec::new(),
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
            vm.set_global(name, Value::Native(id as u32))
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
        ] {
            vm.set_global(name, Value::Native(id))
                .expect("base library names are valid");
        }
        let debug = vm.new_table();
        for (id, name) in [
            (DEBUG_GET_META, b"getmetatable".as_slice()),
            (DEBUG_SET_META, b"setmetatable"),
        ] {
            let key = vm.intern(name);
            vm.tables[debug.0 as usize]
                .set(key, Value::Native(id))
                .expect("debug library names are valid");
        }
        vm.set_global(b"debug", Value::Table(debug))
            .expect("debug name is valid");
        vm.set_global(b"_G", Value::Table(TableId(0)))
            .expect("global table name is valid");
        let version = vm.intern(b"Lua 5.1");
        vm.set_global(b"_VERSION", version)
            .expect("version name is valid");
        vm.open_math();
        vm.open_strings();
        vm.open_tables();
        vm
    }

    /// Set an optional instruction limit for each call to [`Self::execute`].
    pub fn set_instruction_limit(&mut self, limit: Option<u64>) {
        self.budget = limit;
    }

    /// Capture subsequent `print` output in a buffer instead of writing to stdout.
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
        self.strings.bytes.get(id.0 as usize).map(AsRef::as_ref)
    }

    /// Read a global variable; absent variables have value `nil`.
    pub fn global(&mut self, name: &[u8]) -> Value {
        let k = self.intern(name);
        self.tables[self.globals.0 as usize].get(k)
    }

    /// Set a global variable.
    ///
    /// # Errors
    /// Returns an error for an invalid table key.
    pub fn set_global(&mut self, name: &[u8], value: Value) -> Result<(), Error> {
        let k = self.intern(name);
        self.tables[self.globals.0 as usize].set(k, value)
    }

    fn new_table(&mut self) -> TableId {
        let id = TableId(
            u32::try_from(self.tables.len()).expect("Lua table arena exhausted addressable memory"),
        );
        self.tables.push(Table::default());
        id
    }
    fn number(&self, value: Value) -> Option<f64> {
        match value {
            Value::Number(n) => Some(n),
            Value::String(id) => parse_number(self.string_bytes(id)?),
            _ => None,
        }
    }
    fn numeric(&self, value: Value) -> Result<f64, Error> {
        self.number(value).ok_or_else(|| {
            Error::new(
                ErrorKind::Runtime,
                0,
                format!(
                    "attempt to perform arithmetic on a {} value",
                    value.type_name()
                ),
            )
        })
    }
    fn table(&self, value: Value) -> Result<&Table, Error> {
        if let Value::Table(id) = value
            && let Some(t) = self.tables.get(id.0 as usize)
        {
            return Ok(t);
        }
        Err(Error::new(
            ErrorKind::Runtime,
            0,
            format!("attempt to index a {} value", value.type_name()),
        ))
    }
    fn table_mut(&mut self, value: Value) -> Result<&mut Table, Error> {
        if let Value::Table(id) = value
            && let Some(t) = self.tables.get_mut(id.0 as usize)
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
        if let Some(p) = self
            .programs
            .iter()
            .find(|p| Arc::ptr_eq(&p.data, &program.data))
        {
            return p.entry;
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
        let entry = FunctionId(self.functions.len() as u32);
        self.functions.push(Closure {
            program: self.programs.len(),
            prototype: program.data.entry as usize,
            upvalues: Box::default(),
            env: self.globals,
        });
        self.programs.push(LoadedProgram {
            data: Arc::clone(&program.data),
            constants: constants.into_boxed_slice(),
            entry,
        });
        entry
    }

    fn capture_register(&mut self, register: usize) -> u32 {
        if let Some(&id) = self
            .open_upvalues
            .iter()
            .find(|&&id| matches!(self.upvalues[id as usize], Cell::Open(r) if r == register))
        {
            return id;
        }
        let id = self.upvalues.len() as u32;
        self.upvalues.push(Cell::Open(register));
        self.open_upvalues.push(id);
        id
    }

    fn close_upvalues(&mut self, base: usize) {
        let mut i = 0;
        while i < self.open_upvalues.len() {
            let id = self.open_upvalues[i] as usize;
            if let Cell::Open(r) = self.upvalues[id]
                && r >= base
            {
                self.upvalues[id] = Cell::Closed(self.registers[r]);
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
        if self.frames.len() >= 10000 {
            return Err(Error::new(
                ErrorKind::Limit,
                0,
                "Lua call stack limit exceeded",
            ));
        }
        let closure = &self.functions[function.0 as usize];
        let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
        let (registers, params, compat_arg) = (
            usize::from(proto.registers),
            usize::from(proto.params),
            proto.compat_arg,
        );
        let base = self.registers.len();
        self.registers.resize(base + registers, Value::Nil);
        self.registers
            .copy_within(call + 1..call + 1 + count.min(params), base);
        let vararg_count = count.saturating_sub(params);
        let vararg_start = call + 1 + count.min(params);
        if compat_arg {
            let t = self.new_table();
            for i in 0..vararg_count {
                self.tables[t.0 as usize].set(
                    Value::Number((i + 1) as f64),
                    self.registers[vararg_start + i],
                )?;
            }
            let n = self.intern(b"n");
            self.tables[t.0 as usize].set(n, Value::Number(vararg_count as f64))?;
            self.registers[base + params] = Value::Table(t);
        }
        self.frames.push(Frame {
            function: FrameKind::Lua(function),
            pc: 0,
            base,
            stack_start: base,
            result: call,
            wanted,
            vararg_start,
            vararg_count,
            tail_calls: 0,
        });
        self.top = base;
        Ok(())
    }

    fn tail_frame(&mut self, function: FunctionId, call: usize, count: usize) -> Result<(), Error> {
        let frame = *self.frames.last().expect("tail call has an active frame");
        self.close_upvalues(frame.base);
        let closure = &self.functions[function.0 as usize];
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
        self.registers.resize(scratch + count, Value::Nil);
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
                self.tables[t.0 as usize].set(
                    Value::Number((i + 1) as f64),
                    self.registers[frame.stack_start + i],
                )?;
            }
            let n = self.intern(b"n");
            self.tables[t.0 as usize].set(n, Value::Number(extras as f64))?;
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
            vararg_count: extras,
            tail_calls: frame.tail_calls.saturating_add(1),
            ..frame
        };
        self.top = base;
        Ok(())
    }

    fn return_frame(&mut self, frame: Frame, source: usize, count: usize) -> Option<usize> {
        self.close_upvalues(frame.base);
        if self.frames.len() == 1 {
            self.registers.copy_within(source..source + count, 0);
            self.frames.clear();
            return Some(count);
        }
        let wanted = if frame.wanted == OPEN {
            count
        } else {
            frame.wanted as usize
        };
        self.registers
            .resize(self.registers.len().max(frame.result + wanted), Value::Nil);
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

    /// Execute verified bytecode, retaining globals and reusing register storage.
    ///
    /// # Errors
    /// Returns Lua runtime errors or an instruction-limit error. Returned values
    /// borrow the register storage until the next mutation of the VM.
    pub fn execute(&mut self, program: &Program) -> Result<&[Value], Error> {
        let function = self.load(program);
        let closure = &self.functions[function.0 as usize];
        let entry = &self.programs[closure.program].data.prototypes[closure.prototype];
        self.registers.clear();
        self.registers
            .resize(usize::from(entry.registers), Value::Nil);
        self.frames.clear();
        self.frames.push(Frame {
            function: FrameKind::Lua(function),
            pc: 0,
            base: 0,
            stack_start: 0,
            result: 0,
            wanted: OPEN,
            vararg_start: 0,
            vararg_count: 0,
            tail_calls: 0,
        });
        self.top = 0;
        let mut error_line = 0;
        let mut budget = self.budget;
        let result = loop {
            let result = self.run(&mut budget, &mut error_line);
            match result {
                Err(error) if error.kind() == ErrorKind::Runtime => {
                    match self.catch_error(&error, error_line) {
                        Ok(true) => continue,
                        Ok(false) => break Err(error),
                        Err(next) => break Err(next),
                    }
                }
                result => break result,
            }
        };
        self.close_upvalues(0);
        self.frames.clear();
        self.trim_substitutions();
        self.sorts.clear();
        match result {
            Ok(count) => Ok(&self.registers[..count]),
            Err(error) => Err(error.at_line(error_line)),
        }
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
            let frame_index = self.frames.len() - 1;
            let frame = self.frames[frame_index];
            let function = match frame.function {
                FrameKind::Native(id) => {
                    Self::charge(budget)?;
                    let previous = self.active_native.replace(frame_index);
                    let result = self.step_native(id, frame).map_err(|error| {
                        if error.kind() == ErrorKind::Runtime && error.value().is_none() {
                            let value = self.intern(error.message().as_bytes());
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
                    continue;
                }
                FrameKind::Lua(id) => id,
            };
            let closure = &self.functions[function.0 as usize];
            let (program_index, proto_index, env) =
                (closure.program, closure.prototype, closure.env);
            // Hold immutable code independently of VM-vector growth during native calls.
            // Shared ownership is acquired once on frame entry, never per instruction/value.
            let data = Arc::clone(&self.programs[program_index].data);
            let prototype = &data.prototypes[proto_index];
            let mut pc = frame.pc;
            let base = frame.base;
            loop {
                Self::charge(budget)?;
                let i = prototype.code[pc];
                *error_line = prototype.lines[pc];
                pc += 1;
                let a = base + usize::from(i.a);
                let b = base + usize::from(i.b);
                let c = i.c as usize;
                self.frames[frame_index].pc = pc;
                match i.op {
                    Op::Load => {
                        self.registers[a] = self.programs[program_index].constants[proto_index][c]
                    }
                    Op::Move if i.c == OPEN => {
                        let count = self.top.checked_sub(b).ok_or_else(|| {
                            Error::new(ErrorKind::Runtime, 0, "invalid open result range")
                        })?;
                        self.registers
                            .resize(self.registers.len().max(a + count), Value::Nil);
                        self.registers.copy_within(b..b + count, a);
                        self.top = a + count;
                    }
                    Op::Move => self.registers[a] = self.registers[b],
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
                            self.table_mut(table)?
                                .set(Value::Number((c + offset) as f64), value)?;
                        }
                    }
                    Op::Closure => {
                        let len = self.programs[program_index].data.prototypes[c]
                            .upvalues
                            .len();
                        let mut upvalues = Vec::with_capacity(len);
                        for index in 0..len {
                            let capture =
                                self.programs[program_index].data.prototypes[c].upvalues[index];
                            upvalues.push(match capture {
                                Upvalue::Local(r) => self.capture_register(base + usize::from(r)),
                                Upvalue::Outer(u) => {
                                    self.functions[function.0 as usize].upvalues[usize::from(u)]
                                }
                            });
                        }
                        let id = FunctionId(self.functions.len() as u32);
                        self.functions.push(Closure {
                            program: program_index,
                            prototype: c,
                            upvalues: upvalues.into_boxed_slice(),
                            env,
                        });
                        self.registers[a] = Value::Function(id);
                    }
                    Op::GetUpvalue => {
                        let id = self.functions[function.0 as usize].upvalues[c] as usize;
                        self.registers[a] = match self.upvalues[id] {
                            Cell::Open(r) => self.registers[r],
                            Cell::Closed(v) => v,
                        };
                    }
                    Op::SetUpvalue => {
                        let id = self.functions[function.0 as usize].upvalues[c] as usize;
                        match &mut self.upvalues[id] {
                            Cell::Open(r) => self.registers[*r] = self.registers[a],
                            Cell::Closed(v) => *v = self.registers[a],
                        }
                    }
                    Op::Close => self.close_upvalues(a),
                    Op::Vararg => {
                        let wanted = if i.c == OPEN { frame.vararg_count } else { c };
                        let copied = wanted.min(frame.vararg_count);
                        self.registers
                            .resize(self.registers.len().max(a + wanted), Value::Nil);
                        self.registers
                            .copy_within(frame.vararg_start..frame.vararg_start + copied, a);
                        self.registers[a + copied..a + wanted].fill(Value::Nil);
                        self.top = a + wanted;
                    }
                    Op::GetTable => {
                        let target = self.registers[b];
                        let key = self.registers[base + c];
                        if let Value::Table(id) = target
                            && self.tables[id.0 as usize].metatable.is_none()
                        {
                            self.registers[a] = self.tables[id.0 as usize].get(key);
                        } else {
                            self.get_table(target, key, a)?;
                        }
                    }
                    Op::SetTable => {
                        let target = self.registers[a];
                        let key = self.registers[b];
                        let value = self.registers[base + c];
                        if let Value::Table(id) = target
                            && self.tables[id.0 as usize].metatable.is_none()
                        {
                            self.tables[id.0 as usize].set(key, value)?;
                        } else {
                            self.set_table(target, key, value)?;
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
                    }
                    Op::Concat => {
                        self.concatenate(self.registers[b], self.registers[base + c], a)?
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
                    Op::Call | Op::TailCall => {
                        let count = if i.b == OPEN_ARGS {
                            self.top.checked_sub(a + 1).ok_or_else(|| {
                                Error::new(ErrorKind::Runtime, 0, "invalid open argument range")
                            })?
                        } else {
                            usize::from(i.b)
                        };
                        self.frames[frame_index].pc = pc;
                        let Some(actual) = self.start_call(
                            a,
                            count,
                            if i.op == Op::TailCall { OPEN } else { i.c },
                            i.op == Op::TailCall,
                        )?
                        else {
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
                self.frames[frame_index].pc = pc;
                if self.frames.len() != frame_index + 1 {
                    continue 'frames;
                }
            }
        }
    }

    fn concat_bytes(&self, value: Value) -> Result<Vec<u8>, Error> {
        match value {
            Value::String(id) => Ok(self.string_bytes(id).unwrap_or_default().to_vec()),
            Value::Number(n) => Ok(format_number(n).into_bytes()),
            _ => Err(Error::new(
                ErrorKind::Runtime,
                0,
                format!("attempt to concatenate a {} value", value.type_name()),
            )),
        }
    }
}
