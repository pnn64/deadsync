//! Native execution of a verified, finite subset of Lua 5.1 callbacks.
//!
//! Loading uses Lua to produce bytecode. Compilation binds object fields and
//! actions to a closed native schema, then discards all Lua handles. Programs
//! process new inputs and keep mutable scalar state between calls. Unsupported
//! operations fail compilation before any callback body is executed.
//!
//! Bindings describe semantics established by the host: object identities,
//! raw fields without metamethods, action identity/arity/results, and explicit
//! shared cells. The compiler cannot infer these promises from field names or
//! equal captured values. Hosts must validate them against their Lua objects.
//! These contracts must hold for every call, including action side effects.
//! Mutable captures must be private to one machine or explicitly shared group;
//! live Lua must not observe or change their former storage. A group binds
//! the same host-established cell indices across its programs. Actions cannot change
//! inputs or native cells indirectly, and must have the declared static return.
//! Mutable cells keep one fixed primitive type. Byte cells store only compiled
//! constants or values copied from other byte cells. String/number coercion,
//! dynamic lookup, object equality, loops and general Lua calls are unsupported.
//! An input frame is immutable for a complete call. Action arguments borrow
//! its bytes or the program's owned constants without allocating.

mod compile;
mod decode;
mod strings;
use strings::Strings;

use crate::Function;
use std::{
    borrow::Cow,
    fmt,
    mem::{size_of, size_of_val},
    ops::{BitOr, BitOrAssign},
    sync::Arc,
};

/// A set of accepted native value types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Types(u8);
impl Types {
    pub const NIL: Self = Self(1);
    pub const BOOLEAN: Self = Self(2);
    pub const NUMBER: Self = Self(4);
    pub const BYTES: Self = Self(8);
    pub const OBJECT: Self = Self(16);
    pub const ACTION: Self = Self(32);
    pub const ANY: Self = Self(63);
    const PRIMITIVE: Self = Self(15);
    fn accepts(self, actual: Self) -> bool {
        actual.0 != 0 && actual.0 & !self.0 == 0
    }
}
impl BitOr for Types {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl BitOrAssign for Types {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Owned literal used while binding a program.
#[derive(Clone, Debug)]
pub enum Literal {
    Nil,
    Boolean(bool),
    Number(f64),
    Bytes(Box<[u8]>),
}

/// Initial value and fixed type of one explicit mutable cell.
#[derive(Clone, Copy, Debug)]
pub enum Cell {
    Boolean(bool),
    Number(f64),
    /// Initial bytes at this index in `Bindings::strings`. Runtime writes must
    /// come from compiled constants or other byte cells, never borrowed inputs.
    Bytes(usize),
}

/// Source of a parameter, capture or bound field.
#[derive(Clone, Debug)]
pub enum Binding {
    Literal(Literal),
    Object(u32),
    Cell(usize),
    Input { slot: usize, types: Types },
}

/// A host action with checked argument types and a statically known return.
/// Its effects must preserve the remaining bindings; native cells change only
/// through compiled writes. Action failures and dynamic results are unsupported.
#[derive(Clone, Debug)]
pub struct Action {
    pub tag: u32,
    /// Fixed signatures may borrow static storage instead of allocating per field.
    pub parameters: Cow<'static, [Parameter]>,
    /// None means no return values. Only literal and object bindings are accepted.
    pub result: Option<Binding>,
}

/// A host action's parameter contract. Object constraints preserve receiver
/// identity when lowering methods whose implementation depends on `self`.
#[derive(Clone, Copy, Debug)]
pub enum Parameter {
    Type(Types),
    Object(u32),
    /// The identity of the object defining this action's field. This constraint
    /// checks the supplied argument, independently of the action's return object.
    Receiver,
}

/// A field may be a value, a mutable cell, or a bound host action.
#[derive(Clone, Debug)]
pub enum FieldValue {
    Value(Binding),
    Action(Action),
}

/// A field of an opaque, host-validated object.
#[derive(Clone, Debug)]
pub struct Field {
    pub object: u32,
    /// Static schema keys can be borrowed; captured or generated keys are owned.
    pub name: Cow<'static, [u8]>,
    pub value: FieldValue,
}

/// Complete native schema for one callback. Repeated cell indices or object
/// identities express aliasing; distinct indices are never merged by value.
#[derive(Clone, Debug, Default)]
pub struct Bindings {
    pub arguments: Vec<Binding>,
    pub upvalues: Vec<Binding>,
    pub cells: Vec<Cell>,
    /// Owned initial byte values referenced by `Cell::Bytes`.
    pub strings: Vec<Box<[u8]>>,
    pub fields: Vec<Field>,
    pub environment: Option<u32>,
}

/// Rejection at compilation; no authored callback body has run.
#[derive(Debug)]
pub enum CompileError {
    InvalidChunk,
    InvalidBindings(&'static str),
    Unsupported { pc: usize, reason: &'static str },
    Dump(String),
}
impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native Lua callback compilation: {self:?}")
    }
}
impl std::error::Error for CompileError {}

/// A malformed input frame. Compiled code and mutable state remain memory safe;
/// actions/state changes preceding the failing read are not rolled back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunError {
    MissingInput(usize),
    InputType(usize),
}
impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native Lua input: {self:?}")
    }
}
impl std::error::Error for RunError {}

/// Native inputs, results and action arguments. Opaque IDs refer to the host's
/// validated bindings, not Lua pointers. Byte slices may contain arbitrary data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Datum<'a> {
    Nil,
    Boolean(bool),
    Number(f64),
    Bytes(&'a [u8]),
    Object(u32),
    Action(u32),
}
impl Datum<'_> {
    pub fn is_truthy(self) -> bool {
        !matches!(self, Self::Nil | Self::Boolean(false))
    }
    fn types(self) -> Types {
        match self {
            Self::Nil => Types::NIL,
            Self::Boolean(_) => Types::BOOLEAN,
            Self::Number(_) => Types::NUMBER,
            Self::Bytes(_) => Types::BYTES,
            Self::Object(_) => Types::OBJECT,
            Self::Action(_) => Types::ACTION,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Nil,
    Boolean(bool),
    Number(u64),
    String(usize),
    InputString(usize),
    Object(u32),
    Action(u32),
}
impl Value {
    fn types(self) -> Types {
        match self {
            Self::Nil => Types::NIL,
            Self::Boolean(_) => Types::BOOLEAN,
            Self::Number(_) => Types::NUMBER,
            Self::String(_) | Self::InputString(_) => Types::BYTES,
            Self::Object(_) => Types::OBJECT,
            Self::Action(_) => Types::ACTION,
        }
    }
    fn truthy(self) -> bool {
        !matches!(self, Self::Nil | Self::Boolean(false))
    }
    fn number(self) -> f64 {
        let Self::Number(bits) = self else {
            unreachable!("verified numeric operand")
        };
        f64::from_bits(bits)
    }
}

#[derive(Clone, Copy, Debug)]
enum Source {
    Value(Value),
    Cell(usize),
    Input(usize, Types),
}
#[derive(Clone, Copy, Debug)]
enum Operand {
    Register(usize),
    Value(Value),
}
impl Operand {
    fn get(self, registers: &[Value]) -> Value {
        match self {
            Self::Register(r) => registers[r],
            Self::Value(v) => v,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Unreachable,
    Load {
        dst: usize,
        source: Source,
        skip: bool,
    },
    Move {
        dst: usize,
        source: usize,
    },
    Nil {
        first: usize,
        last: usize,
    },
    Write {
        cell: usize,
        source: Operand,
    },
    Method {
        dst: usize,
        receiver: usize,
        action: u32,
    },
    Not {
        dst: usize,
        source: usize,
    },
    Neg {
        dst: usize,
        source: usize,
    },
    Arithmetic {
        dst: usize,
        left: Operand,
        right: Operand,
        operator: u8,
    },
    Compare {
        left: Operand,
        right: Operand,
        operator: u8,
        expected: bool,
    },
    Test {
        source: usize,
        expected: bool,
        copy: Option<usize>,
    },
    Jump(usize),
    Call {
        tag: u32,
        first: usize,
        count: usize,
        dst: Option<usize>,
        result: Value,
        tail: bool,
    },
    Return(Option<usize>),
}

struct Code {
    ops: Box<[Op]>,
    strings: Strings,
    cells: Box<[Value]>,
    parameters: Box<[Source]>,
    registers: usize,
}

/// Immutable compiled callback. It contains Rust-owned code/data and no Lua
/// handle, registry reference, callback or state pointer.
#[derive(Clone)]
pub struct Program(Arc<Code>);
impl fmt::Debug for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Program")
            .field("instructions", &self.0.ops.len())
            .field("cells", &self.0.cells.len())
            .finish()
    }
}

/// Compile a Lua closure using a validated native schema.
pub fn compile(function: &Function, bindings: &Bindings) -> Result<Program, CompileError> {
    compile_bytecode(&dump(function)?, bindings)
}

/// Copy a Lua closure's bytecode without executing it. The returned bytes retain
/// no Lua handles and may be reused with different schemas via [`compile_bytecode`]
/// or [`Compiler::compile_bytecode`]. Native closures are rejected. Compilation
/// separately checks the Lua 5.1 header, instructions and binding contracts.
pub fn dump(function: &Function) -> Result<Vec<u8>, CompileError> {
    if function.environment().is_none() {
        return Err(CompileError::Unsupported {
            pc: 0,
            reason: "native closure",
        });
    }
    function
        .try_dump(false)
        .map_err(|error| CompileError::Dump(error.to_string()))
}

/// Safely decode and compile a Lua 5.1 dump. These bytes are never loaded into
/// Lua. Headers and reachable operands/control flow are validated. Unsupported
/// opcodes and noncanonical constants are rejected.
pub fn compile_bytecode(bytes: &[u8], bindings: &Bindings) -> Result<Program, CompileError> {
    compile::lower(decode::decode(bytes)?, bindings)
}

/// A reusable, borrowed field schema for compiling related callbacks.
///
/// Object fields are validated once. Larger schemas use a sorted index instead
/// of repeating linear field searches. Cells and environment come from this
/// schema; each call supplies its own arguments and captures. Programs own all
/// resulting code/data and do not borrow this compiler or retain Lua handles.
pub struct Compiler<'a> {
    bindings: &'a Bindings,
    fields: compile::Fields,
}

impl<'a> Compiler<'a> {
    /// Reject duplicate object fields without executing authored code.
    pub fn new(bindings: &'a Bindings) -> Result<Self, CompileError> {
        Ok(Self {
            bindings,
            fields: compile::Fields::new(&bindings.fields)?,
        })
    }

    /// Compile using these argument/capture bindings in upvalue index order.
    pub fn compile(
        &self,
        function: &Function,
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError> {
        self.compile_bytecode(&dump(function)?, arguments, upvalues)
    }

    /// Compile a checked Lua 5.1 dump against the same immutable field schema.
    pub fn compile_bytecode(
        &self,
        bytes: &[u8],
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError> {
        compile::lower_with(
            decode::decode(bytes)?,
            self.bindings,
            arguments,
            upvalues,
            &self.fields,
        )
    }
}

impl Program {
    /// Create independent mutable state and reusable register storage.
    pub fn machine(&self) -> Machine {
        Machine {
            program: self.clone(),
            cells: self.0.cells.to_vec().into_boxed_slice(),
            registers: vec![Value::Nil; self.0.registers].into_boxed_slice(),
        }
    }

    /// Owned code/data payload, excluding allocator bookkeeping and Arc counters.
    pub fn retained_bytes(&self) -> usize {
        self.code_bytes() + self.0.strings.retained_bytes()
    }

    fn code_bytes(&self) -> usize {
        size_of::<Code>()
            + size_of_val(self.0.ops.as_ref())
            + size_of_val(self.0.cells.as_ref())
            + size_of_val(self.0.parameters.as_ref())
    }
}

/// A borrowed action argument view; it creates no intermediate collection.
pub struct Arguments<'a> {
    values: &'a [Value],
    code: &'a Code,
    inputs: &'a [Datum<'a>],
}
impl<'a> Arguments<'a> {
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<Datum<'a>> {
        self.values.get(index).map(|v| datum(*v, self.code, self.inputs))
    }
}

/// A callback's explicit state and scratch space. Calls allocate no engine
/// storage; host actions may allocate according to the host's own policy.
pub struct Machine {
    program: Program,
    cells: Box<[Value]>,
    registers: Box<[Value]>,
}
impl Machine {
    /// Owned payload including code/data once; excludes allocator bookkeeping.
    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.program.retained_bytes()
            + size_of_val(self.cells.as_ref())
            + size_of_val(self.registers.as_ref())
    }

    /// Observe a scalar cell without exposing mutable or Lua-backed storage.
    pub fn cell(&self, index: usize) -> Option<Datum<'_>> {
        self.cells.get(index).map(|v| datum(*v, &self.program.0, &[]))
    }

    /// Execute against actual immutable inputs and emit host actions in authored
    /// order. The result is the first returned value (nil for no results).
    pub fn run<'a>(
        &'a mut self,
        inputs: &'a [Datum<'a>],
        emit: impl FnMut(u32, Arguments<'_>),
    ) -> Result<Datum<'a>, RunError> {
        execute(
            &self.program.0,
            &mut self.cells,
            &mut self.registers,
            inputs,
            emit,
        )
    }
}

/// Programs with one explicit cell store and one reusable scratch buffer.
///
/// The host declares that identical cell indices refer to the same logical
/// storage across programs. Matching initial values do not prove aliasing.
/// Construction checks complete initial layouts, including numeric bits; the
/// host must establish identity and exclude observers outside this owner.
/// Programs execute in the order selected by the host. No Lua handles remain.
pub struct Group {
    programs: Box<[Program]>,
    cells: Box<[Value]>,
    registers: Box<[Value]>,
}

impl Group {
    /// Consume programs using the same host-established cell layout.
    pub fn new(mut programs: Vec<Program>) -> Result<Self, CompileError> {
        if let Some(first) = programs.first() {
            if programs.iter().any(|p| !strings::same_cells(&first.0, &p.0)) {
                return Err(CompileError::InvalidBindings("shared cell layout"));
            }
            if first.0.cells.iter().any(|v| matches!(v, Value::String(_))) {
                programs = strings::share(programs);
            }
        }
        let initial = programs.first().map_or(&[][..], |p| p.0.cells.as_ref());
        let cells = initial.to_vec().into_boxed_slice();
        let registers =
            vec![Value::Nil; programs.iter().map(|p| p.0.registers).max().unwrap_or(0)].into_boxed_slice();
        Ok(Self {
            programs: programs.into_boxed_slice(),
            cells,
            registers,
        })
    }

    pub fn len(&self) -> usize {
        self.programs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.programs.is_empty()
    }

    /// Current scalar state; values and aliases are never merged by equality.
    pub fn cell(&self, index: usize) -> Option<Datum<'_>> {
        self.cells.get(index).map(|value| match *value {
            Value::Boolean(value) => Datum::Boolean(value),
            Value::Number(bits) => Datum::Number(f64::from_bits(bits)),
            Value::String(index) => Datum::Bytes(&self.programs[0].0.strings[index]),
            _ => unreachable!("verified scalar cell"),
        })
    }

    /// Execute one program. None means an invalid program index and has no
    /// effects. Input errors retain preceding writes/actions, as in Machine.
    pub fn run<'a>(
        &'a mut self,
        index: usize,
        inputs: &'a [Datum<'a>],
        emit: impl FnMut(u32, Arguments<'_>),
    ) -> Option<Result<Datum<'a>, RunError>> {
        let program = self.programs.get(index)?;
        Some(execute(
            &program.0,
            &mut self.cells,
            &mut self.registers[..program.0.registers],
            inputs,
            emit,
        ))
    }

    /// Owned payload, counting shared program allocations once; allocator and
    /// Arc bookkeeping are excluded. This diagnostic does not allocate.
    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + size_of_val(self.programs.as_ref())
            + size_of_val(self.cells.as_ref())
            + size_of_val(self.registers.as_ref())
            + self
                .programs
                .iter()
                .enumerate()
                .filter(|(index, program)| {
                    !self.programs[..*index]
                        .iter()
                        .any(|other| Arc::ptr_eq(&other.0, &program.0))
                })
                .map(|(_, program)| program.code_bytes())
                .sum::<usize>()
            + self
                .programs
                .iter()
                .enumerate()
                .filter(|(index, program)| {
                    !self.programs[..*index].iter().any(|other| {
                        Arc::ptr_eq(&other.0, &program.0) || other.0.strings.shared_with(&program.0.strings)
                    })
                })
                .map(|(_, program)| program.0.strings.retained_bytes())
                .sum::<usize>()
    }
}

fn execute<'a>(
    code: &'a Code,
    cells: &mut [Value],
    registers: &mut [Value],
    inputs: &'a [Datum<'a>],
    mut emit: impl FnMut(u32, Arguments<'_>),
) -> Result<Datum<'a>, RunError> {
    registers.fill(Value::Nil);
    for (dst, source) in code.parameters.iter().enumerate() {
        registers[dst] = read(*source, cells, inputs)?;
    }
    let mut pc = 0;
    loop {
        let op = code.ops[pc];
        pc += 1;
        match op {
            Op::Unreachable => unreachable!("verified control flow"),
            Op::Load { dst, source, skip } => {
                registers[dst] = read(source, cells, inputs)?;
                pc += usize::from(skip);
            }
            Op::Move { dst, source } => registers[dst] = registers[source],
            Op::Nil { first, last } => registers[first..=last].fill(Value::Nil),
            Op::Write { cell, source } => cells[cell] = source.get(registers),
            Op::Method {
                dst,
                receiver,
                action,
            } => {
                let value = registers[receiver];
                registers[dst + 1] = value;
                registers[dst] = Value::Action(action);
            }
            Op::Not { dst, source } => registers[dst] = Value::Boolean(!registers[source].truthy()),
            Op::Neg { dst, source } => {
                registers[dst] = Value::Number((-registers[source].number()).to_bits())
            }
            Op::Arithmetic {
                dst,
                left,
                right,
                operator,
            } => {
                let (a, b) = (left.get(registers).number(), right.get(registers).number());
                registers[dst] = Value::Number(
                    match operator {
                        12 => a + b,
                        13 => a - b,
                        14 => a * b,
                        15 => a / b,
                        _ => unreachable!(),
                    }
                    .to_bits(),
                );
            }
            Op::Compare {
                left,
                right,
                operator,
                expected,
            } => {
                let (a, b) = (left.get(registers), right.get(registers));
                let equal = match operator {
                    23 => equal(a, b, code, inputs),
                    24 => a.number() < b.number(),
                    25 => a.number() <= b.number(),
                    _ => unreachable!(),
                };
                if equal != expected {
                    pc += 1;
                }
            }
            Op::Test {
                source,
                expected,
                copy,
            } => {
                if registers[source].truthy() == expected {
                    if let Some(dst) = copy {
                        registers[dst] = registers[source];
                    }
                } else {
                    pc += 1;
                }
            }
            Op::Jump(target) => pc = target,
            Op::Call {
                tag,
                first,
                count,
                dst,
                result,
                tail,
            } => {
                emit(
                    tag,
                    Arguments {
                        values: &registers[first..first + count],
                        code,
                        inputs,
                    },
                );
                if tail {
                    return Ok(datum(result, code, inputs));
                }
                if let Some(dst) = dst {
                    registers[dst] = result;
                }
            }
            Op::Return(source) => {
                return Ok(datum(source.map_or(Value::Nil, |i| registers[i]), code, inputs));
            }
        }
    }
}

fn read(source: Source, cells: &[Value], inputs: &[Datum<'_>]) -> Result<Value, RunError> {
    Ok(match source {
        Source::Value(v) => v,
        Source::Cell(i) => cells[i],
        Source::Input(i, expected) => {
            let v = *inputs.get(i).ok_or(RunError::MissingInput(i))?;
            if !expected.accepts(v.types()) {
                return Err(RunError::InputType(i));
            }
            match v {
                Datum::Nil => Value::Nil,
                Datum::Boolean(v) => Value::Boolean(v),
                Datum::Number(v) => Value::Number(v.to_bits()),
                Datum::Bytes(_) => Value::InputString(i),
                Datum::Object(_) | Datum::Action(_) => unreachable!("primitive input schema"),
            }
        }
    })
}

fn datum<'a>(v: Value, code: &'a Code, inputs: &'a [Datum<'a>]) -> Datum<'a> {
    match v {
        Value::Nil => Datum::Nil,
        Value::Boolean(v) => Datum::Boolean(v),
        Value::Number(bits) => Datum::Number(f64::from_bits(bits)),
        Value::String(i) => Datum::Bytes(&code.strings[i]),
        Value::InputString(i) => inputs[i],
        Value::Object(i) => Datum::Object(i),
        Value::Action(i) => Datum::Action(i),
    }
}

fn equal(a: Value, b: Value, code: &Code, inputs: &[Datum<'_>]) -> bool {
    match (datum(a, code, inputs), datum(b, code, inputs)) {
        (Datum::Nil, Datum::Nil) => true,
        (Datum::Boolean(a), Datum::Boolean(b)) => a == b,
        (Datum::Number(a), Datum::Number(b)) => a == b,
        (Datum::Bytes(a), Datum::Bytes(b)) => a == b,
        _ => false,
    }
}
