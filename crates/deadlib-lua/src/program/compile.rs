use super::decode::Prototype;
use super::*;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    Unknown,
    Constant(Value),
    Scalar(Types),
    Object(u32),
    Action(usize),
}
impl Symbol {
    fn types(self) -> Option<Types> {
        match self {
            Self::Unknown => None,
            Self::Constant(v) => Some(v.types()),
            Self::Scalar(t) => Some(t),
            Self::Object(_) => Some(Types::OBJECT),
            Self::Action(_) => Some(Types::ACTION),
        }
    }
    fn merge(self, other: Self) -> Self {
        if self == other {
            return self;
        }
        match (self.types(), other.types()) {
            (Some(a), Some(b)) if Types::PRIMITIVE.accepts(a | b) => Self::Scalar(a | b),
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy)]
enum Resolved {
    Value(Source),
    Action(usize),
}
struct Call<'a> {
    tag: u32,
    parameters: &'a [Parameter],
    result: Value,
}
struct Builder<'a> {
    bindings: &'a Bindings,
    index: &'a Fields,
    strings: Vec<Box<[u8]>>,
    cells: Vec<Value>,
    fields: Vec<Option<Resolved>>,
    actions: Vec<Call<'a>>,
}

impl Builder<'_> {
    fn literal(&mut self, literal: Literal) -> Value {
        match literal {
            Literal::Nil => Value::Nil,
            Literal::Boolean(v) => Value::Boolean(v),
            Literal::Number(v) => Value::Number(v.to_bits()),
            Literal::Bytes(v) => {
                let id = self.strings.len();
                self.strings.push(v);
                Value::String(id)
            }
        }
    }
    fn binding(&mut self, binding: &Binding) -> Result<Source, CompileError> {
        Ok(match binding {
            Binding::Literal(v) => Source::Value(self.literal(v.clone())),
            Binding::Object(id) => Source::Value(Value::Object(*id)),
            Binding::Cell(id) => {
                if *id >= self.cells.len() {
                    return Err(CompileError::InvalidBindings("cell index"));
                }
                Source::Cell(*id)
            }
            Binding::Input { slot, types } => {
                if !Types::PRIMITIVE.accepts(*types) {
                    return Err(CompileError::InvalidBindings("input type"));
                }
                Source::Input(*slot, *types)
            }
        })
    }
    fn symbol(&self, source: Source) -> Symbol {
        match source {
            Source::Value(Value::Object(id)) => Symbol::Object(id),
            Source::Value(v) => Symbol::Constant(v),
            Source::Cell(i) => Symbol::Scalar(self.cells[i].types()),
            Source::Input(_, types) => Symbol::Scalar(types),
        }
    }
    fn field(&mut self, object: Symbol, key: Symbol, pc: usize) -> Result<Resolved, CompileError> {
        let Symbol::Object(object) = object else {
            return Err(unsupported(pc, "dynamic object lookup"));
        };
        let Symbol::Constant(Value::String(key)) = key else {
            return Err(unsupported(pc, "dynamic field key"));
        };
        let index = self
            .index
            .find(&self.bindings.fields, object, &self.strings[key])
            .ok_or_else(|| unsupported(pc, "unbound object field"))?;
        if let Some(field) = self.fields[index] {
            return Ok(field);
        }
        let bindings = self.bindings;
        let field = match &bindings.fields[index].value {
            FieldValue::Value(value) => Resolved::Value(self.binding(value)?),
            FieldValue::Action(action) => {
                if action.parameters.len() > 249
                    || action
                        .parameters
                        .iter()
                        .any(|t| matches!(t, Parameter::Type(Types(0))))
                {
                    return Err(CompileError::InvalidBindings("action parameters"));
                }
                let result = match &action.result {
                    None => Value::Nil,
                    Some(binding) => match self.binding(binding)? {
                        Source::Value(v) => v,
                        _ => return Err(CompileError::InvalidBindings("action result must be constant")),
                    },
                };
                let id = self.actions.len();
                if id > u32::MAX as usize {
                    return Err(CompileError::InvalidBindings("action count"));
                }
                self.actions.push(Call {
                    tag: action.tag,
                    parameters: &action.parameters,
                    result,
                });
                Resolved::Action(id)
            }
        };
        self.fields[index] = Some(field);
        Ok(field)
    }
    fn field_symbol(&self, field: Resolved) -> Symbol {
        match field {
            Resolved::Value(s) => self.symbol(s),
            Resolved::Action(id) => Symbol::Action(id),
        }
    }
    fn field_source(&self, field: Resolved) -> Source {
        match field {
            Resolved::Value(s) => s,
            Resolved::Action(id) => Source::Value(Value::Action(id as u32)),
        }
    }
    fn write(&self, target: Source, value: Symbol, pc: usize) -> Result<usize, CompileError> {
        let Source::Cell(index) = target else {
            return Err(unsupported(pc, "write to unbound cell"));
        };
        if !value
            .types()
            .is_some_and(|t| self.cells[index].types().accepts(t))
        {
            return Err(unsupported(pc, "cell type change"));
        }
        Ok(index)
    }
}

fn unsupported(pc: usize, reason: &'static str) -> CompileError {
    CompileError::Unsupported { pc, reason }
}
fn reg(symbols: &[Symbol], index: usize, pc: usize) -> Result<Symbol, CompileError> {
    let value = *symbols.get(index).ok_or(CompileError::InvalidChunk)?;
    if value == Symbol::Unknown {
        return Err(unsupported(pc, "unproven register value"));
    }
    Ok(value)
}
fn set(symbols: &mut [Symbol], index: usize, value: Symbol) -> Result<(), CompileError> {
    *symbols.get_mut(index).ok_or(CompileError::InvalidChunk)? = value;
    Ok(())
}
fn rk(
    symbols: &[Symbol],
    constants: &[Value],
    index: usize,
    pc: usize,
) -> Result<(Operand, Symbol), CompileError> {
    if index & 256 != 0 {
        let value = *constants.get(index & 255).ok_or(CompileError::InvalidChunk)?;
        Ok((Operand::Value(value), Symbol::Constant(value)))
    } else {
        Ok((Operand::Register(index), reg(symbols, index, pc)?))
    }
}
fn require(symbol: Symbol, types: Types, pc: usize) -> Result<(), CompileError> {
    if symbol.types().is_some_and(|actual| types.accepts(actual)) {
        Ok(())
    } else {
        Err(unsupported(pc, "operand coercion or metamethod"))
    }
}
fn edge(
    states: &mut [Option<Vec<Symbol>>],
    from: usize,
    target: usize,
    symbols: Cow<'_, [Symbol]>,
) -> Result<(), CompileError> {
    if target <= from {
        return Err(unsupported(from, "backward control flow"));
    }
    let state = states.get_mut(target).ok_or(CompileError::InvalidChunk)?;
    match state {
        None => *state = Some(symbols.into_owned()),
        Some(state) => {
            for (old, new) in state.iter_mut().zip(symbols.iter()) {
                *old = old.merge(*new);
            }
        }
    }
    Ok(())
}

pub(super) fn lower(prototype: Prototype<'_>, bindings: &Bindings) -> Result<Program, CompileError> {
    if prototype.upvalues != bindings.upvalues.len() {
        return Err(CompileError::InvalidBindings("upvalue count"));
    }
    let fields = Fields::new(&bindings.fields)?;
    lower_with(
        prototype,
        bindings,
        &bindings.arguments,
        &bindings.upvalues,
        &fields,
    )
}

pub(super) struct Fields(Vec<usize>);
impl Fields {
    pub(super) fn new(fields: &[Field]) -> Result<Self, CompileError> {
        // Small schemas benefit from contiguous scans without another allocation.
        let mut order = Vec::new();
        if fields.len() < 32 {
            for (index, field) in fields.iter().enumerate() {
                if fields[..index]
                    .iter()
                    .any(|f| f.object == field.object && f.name == field.name)
                {
                    return Err(CompileError::InvalidBindings("duplicate object field"));
                }
            }
        } else {
            order.extend(0..fields.len());
            order.sort_unstable_by(|&a, &b| {
                (fields[a].object, &fields[a].name).cmp(&(fields[b].object, &fields[b].name))
            });
            if order.windows(2).any(|pair| {
                fields[pair[0]].object == fields[pair[1]].object
                    && fields[pair[0]].name == fields[pair[1]].name
            }) {
                return Err(CompileError::InvalidBindings("duplicate object field"));
            }
        }
        Ok(Self(order))
    }

    fn find(&self, fields: &[Field], object: u32, name: &[u8]) -> Option<usize> {
        if self.0.is_empty() {
            fields
                .iter()
                .position(|field| field.object == object && field.name.as_ref() == name)
        } else {
            self.0
                .binary_search_by(|&index| {
                    (fields[index].object, fields[index].name.as_ref()).cmp(&(object, name))
                })
                .ok()
                .map(|index| self.0[index])
        }
    }
}

pub(super) fn lower_with(
    prototype: Prototype<'_>,
    bindings: &Bindings,
    arguments: &[Binding],
    captures: &[Binding],
    fields: &Fields,
) -> Result<Program, CompileError> {
    if prototype.upvalues != captures.len() {
        return Err(CompileError::InvalidBindings("upvalue count"));
    }
    let cells = bindings
        .cells
        .iter()
        .map(|c| match c {
            Cell::Boolean(v) => Value::Boolean(*v),
            Cell::Number(v) => Value::Number(v.to_bits()),
        })
        .collect();
    let mut builder = Builder {
        bindings,
        index: fields,
        strings: Vec::new(),
        cells,
        fields: vec![None; bindings.fields.len()],
        actions: Vec::new(),
    };
    let constants: Vec<_> = prototype
        .constants
        .into_iter()
        .map(|v| builder.literal(v))
        .collect();
    let upvalues = captures
        .iter()
        .map(|v| builder.binding(v))
        .collect::<Result<Vec<_>, _>>()?;
    let mut parameters = Vec::with_capacity(prototype.parameters);
    // Lua 5.1 clears non-parameter registers on entry and can omit LOADNIL.
    let mut first = vec![Symbol::Constant(Value::Nil); prototype.registers];
    for (index, symbol) in first.iter_mut().take(prototype.parameters).enumerate() {
        let source = match arguments.get(index) {
            Some(v) => builder.binding(v)?,
            None => Source::Value(Value::Nil),
        };
        *symbol = builder.symbol(source);
        parameters.push(source);
    }
    let mut states = vec![None; prototype.code.len()];
    states[0] = Some(first);
    let mut ops = vec![Op::Unreachable; prototype.code.len()];
    for (pc, word) in prototype.code.into_iter().enumerate() {
        let opcode = (word & 63) as u8;
        if !matches!(opcode,0..=9|11..=15|18|19|22..=30) {
            return Err(unsupported(pc, "opcode"));
        }
        let Some(mut symbols) = states[pc].take() else {
            continue;
        };
        let a = ((word >> 6) & 255) as usize;
        let c = ((word >> 14) & 511) as usize;
        let b = ((word >> 23) & 511) as usize;
        let bx = ((word >> 14) & 262143) as usize;
        let mut next = Some(pc + 1);
        ops[pc] = match opcode {
            0 => {
                let v = reg(&symbols, b, pc)?;
                set(&mut symbols, a, v)?;
                Op::Move { dst: a, source: b }
            }
            1 => {
                let v = *constants.get(bx).ok_or(CompileError::InvalidChunk)?;
                set(&mut symbols, a, Symbol::Constant(v))?;
                Op::Load {
                    dst: a,
                    source: Source::Value(v),
                    skip: false,
                }
            }
            2 => {
                if b > 1 || c > 1 {
                    return Err(CompileError::InvalidChunk);
                }
                let v = Value::Boolean(b != 0);
                set(&mut symbols, a, Symbol::Constant(v))?;
                if c != 0 {
                    next = Some(pc + 2);
                }
                Op::Load {
                    dst: a,
                    source: Source::Value(v),
                    skip: c != 0,
                }
            }
            3 => {
                if b < a || b >= symbols.len() {
                    return Err(CompileError::InvalidChunk);
                }
                symbols[a..=b].fill(Symbol::Constant(Value::Nil));
                Op::Nil { first: a, last: b }
            }
            4 => {
                let source = *upvalues.get(b).ok_or(CompileError::InvalidChunk)?;
                set(&mut symbols, a, builder.symbol(source))?;
                Op::Load {
                    dst: a,
                    source,
                    skip: false,
                }
            }
            5 | 6 => {
                let (object, key) = if opcode == 5 {
                    let object = bindings
                        .environment
                        .ok_or_else(|| unsupported(pc, "unbound environment"))?;
                    (
                        Symbol::Object(object),
                        Symbol::Constant(*constants.get(bx).ok_or(CompileError::InvalidChunk)?),
                    )
                } else {
                    (reg(&symbols, b, pc)?, rk(&symbols, &constants, c, pc)?.1)
                };
                let field = builder.field(object, key, pc)?;
                set(&mut symbols, a, builder.field_symbol(field))?;
                Op::Load {
                    dst: a,
                    source: builder.field_source(field),
                    skip: false,
                }
            }
            7 | 8 | 9 => {
                let (target, source, symbol) = if opcode == 8 {
                    (
                        *upvalues.get(b).ok_or(CompileError::InvalidChunk)?,
                        Operand::Register(a),
                        reg(&symbols, a, pc)?,
                    )
                } else {
                    let (object, key, source, symbol) = if opcode == 7 {
                        let object = bindings
                            .environment
                            .ok_or_else(|| unsupported(pc, "unbound environment"))?;
                        (
                            Symbol::Object(object),
                            Symbol::Constant(*constants.get(bx).ok_or(CompileError::InvalidChunk)?),
                            Operand::Register(a),
                            reg(&symbols, a, pc)?,
                        )
                    } else {
                        let (source, symbol) = rk(&symbols, &constants, c, pc)?;
                        (
                            reg(&symbols, a, pc)?,
                            rk(&symbols, &constants, b, pc)?.1,
                            source,
                            symbol,
                        )
                    };
                    let field = builder.field(object, key, pc)?;
                    (builder.field_source(field), source, symbol)
                };
                let cell = builder.write(target, symbol, pc)?;
                Op::Write { cell, source }
            }
            11 => {
                let receiver = reg(&symbols, b, pc)?;
                let field = builder.field(receiver, rk(&symbols, &constants, c, pc)?.1, pc)?;
                let Resolved::Action(action) = field else {
                    return Err(unsupported(pc, "unbound method"));
                };
                set(&mut symbols, a + 1, receiver)?;
                set(&mut symbols, a, Symbol::Action(action))?;
                Op::Method {
                    dst: a,
                    receiver: b,
                    action: action as u32,
                }
            }
            12..=15 => {
                let (left, lt) = rk(&symbols, &constants, b, pc)?;
                let (right, rt) = rk(&symbols, &constants, c, pc)?;
                require(lt, Types::NUMBER, pc)?;
                require(rt, Types::NUMBER, pc)?;
                set(&mut symbols, a, Symbol::Scalar(Types::NUMBER))?;
                Op::Arithmetic {
                    dst: a,
                    left,
                    right,
                    operator: opcode,
                }
            }
            18 => {
                require(reg(&symbols, b, pc)?, Types::NUMBER, pc)?;
                set(&mut symbols, a, Symbol::Scalar(Types::NUMBER))?;
                Op::Neg { dst: a, source: b }
            }
            19 => {
                reg(&symbols, b, pc)?;
                set(&mut symbols, a, Symbol::Scalar(Types::BOOLEAN))?;
                Op::Not { dst: a, source: b }
            }
            22 => {
                let target = (pc as isize + 1 + bx as isize - 131071)
                    .try_into()
                    .map_err(|_| CompileError::InvalidChunk)?;
                next = Some(target);
                Op::Jump(target)
            }
            23..=25 => {
                if a > 1 {
                    return Err(CompileError::InvalidChunk);
                }
                let (left, lt) = rk(&symbols, &constants, b, pc)?;
                let (right, rt) = rk(&symbols, &constants, c, pc)?;
                let types = if opcode == 23 {
                    Types::PRIMITIVE
                } else {
                    Types::NUMBER
                };
                require(lt, types, pc)?;
                require(rt, types, pc)?;
                edge(&mut states, pc, pc + 2, Cow::Borrowed(&symbols))?;
                Op::Compare {
                    left,
                    right,
                    operator: opcode,
                    expected: a != 0,
                }
            }
            26 | 27 => {
                if c > 1 {
                    return Err(CompileError::InvalidChunk);
                }
                let source = if opcode == 26 { a } else { b };
                let value = reg(&symbols, source, pc)?;
                edge(&mut states, pc, pc + 2, Cow::Borrowed(&symbols))?;
                if opcode == 27 {
                    set(&mut symbols, a, value)?;
                }
                Op::Test {
                    source,
                    expected: c != 0,
                    copy: (opcode == 27).then_some(a),
                }
            }
            28 | 29 => {
                if b == 0 || (opcode == 28 && !matches!(c, 1 | 2)) {
                    return Err(unsupported(pc, "variable call arity"));
                }
                let Symbol::Action(id) = reg(&symbols, a, pc)? else {
                    return Err(unsupported(pc, "unbound call target"));
                };
                let call = &builder.actions[id];
                if b - 1 != call.parameters.len() {
                    return Err(unsupported(pc, "action arity"));
                }
                for (offset, parameter) in call.parameters.iter().enumerate() {
                    let symbol = reg(&symbols, a + 1 + offset, pc)?;
                    match *parameter {
                        Parameter::Type(types) => require(symbol, types, pc)?,
                        Parameter::Object(id) if symbol == Symbol::Object(id) => {}
                        Parameter::Object(_) => return Err(unsupported(pc, "unproven action receiver")),
                    }
                }
                let dst = (opcode == 28 && c == 2).then_some(a);
                if let Some(dst) = dst {
                    set(
                        &mut symbols,
                        dst,
                        match call.result {
                            Value::Object(i) => Symbol::Object(i),
                            v => Symbol::Constant(v),
                        },
                    )?;
                }
                if opcode == 29 {
                    next = None;
                }
                Op::Call {
                    tag: call.tag,
                    first: a + 1,
                    count: b - 1,
                    dst,
                    result: call.result,
                    tail: opcode == 29,
                }
            }
            30 => {
                if b == 0 {
                    return Err(unsupported(pc, "variable return arity"));
                }
                if b > 1 {
                    for index in a..a + b - 1 {
                        reg(&symbols, index, pc)?;
                    }
                }
                next = None;
                Op::Return((b > 1).then_some(a))
            }
            _ => unreachable!(),
        };
        if let Some(target) = next {
            edge(&mut states, pc, target, Cow::Owned(symbols))?;
        }
    }
    Ok(Program(Arc::new(Code {
        ops: ops.into_boxed_slice(),
        strings: builder.strings.into_boxed_slice(),
        cells: builder.cells.into_boxed_slice(),
        parameters: parameters.into_boxed_slice(),
        registers: prototype.registers,
    })))
}
