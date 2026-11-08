use super::decode::Prototype;
use super::*;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    Unknown,
    Constant(Value),
    Scalar(Types),
    StableBytes,
    Object(u32),
    Action(usize),
    BooleanTable,
    BooleanTableArray(usize),
    NumberRange(i32, i32),
}
impl Symbol {
    fn types(self) -> Option<Types> {
        match self {
            Self::Unknown => None,
            Self::Constant(v) => Some(v.types()),
            Self::Scalar(t) => Some(t),
            Self::StableBytes => Some(Types::BYTES),
            Self::Object(_) | Self::BooleanTable | Self::BooleanTableArray(_) => Some(Types::OBJECT),
            Self::NumberRange(_, _) => Some(Types::NUMBER),
            Self::Action(_) => Some(Types::ACTION),
        }
    }
    fn merge(self, other: Self) -> Self {
        if self == other {
            return self;
        }
        if let (Some((a,b)), Some((c,d))) = (self.integer_range(), other.integer_range()) {
            return Self::NumberRange(a.min(c), b.max(d));
        }
        if self.stable_bytes() && other.stable_bytes() {
            return Self::StableBytes;
        }
        match (self.types(), other.types()) {
            (Some(a), Some(b)) if Types::PRIMITIVE.accepts(a | b) => Self::Scalar(a | b),
            _ => Self::Unknown,
        }
    }
    fn stable_bytes(self) -> bool {
        matches!(self, Self::StableBytes | Self::Constant(Value::String(_)))
    }

    fn integer_range(self) -> Option<(i32, i32)> {
        match self {
            Self::NumberRange(a,b) => Some((a,b)),
            Self::Constant(Value::Number(bits)) => {
                let value = f64::from_bits(bits);
                (value.is_finite() && value.fract() == 0.0 && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX))
                    .then_some((value as i32, value as i32))
            }
            _ => None,
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
    receiver: u32,
    parameters: &'a [Parameter],
    result: Value,
}
struct Builder<'a> {
    bindings: &'a Bindings,
    index: &'a Fields,
    tables: Option<&'a Arc<tables::Layout>>,
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
            Binding::Object(id) => Source::Value(
                self.tables.and_then(|tables| {
                    tables.index(*id).map(Value::BooleanTable)
                        .or_else(|| tables.array_index(*id).map(Value::BooleanTableArray))
                }).unwrap_or(Value::Object(*id))
            ),
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
            Source::Value(Value::BooleanTable(_)) => Symbol::BooleanTable,
            Source::Value(Value::BooleanTableArray(index)) => Symbol::BooleanTableArray(index),
            Source::Value(Value::Object(id)) => Symbol::Object(id),
            Source::Value(v) => Symbol::Constant(v),
            Source::Cell(i) if matches!(self.cells[i], Value::String(_)) => Symbol::StableBytes,
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
                    receiver: object,
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
        if matches!(self.cells[index], Value::String(_)) && !value.stable_bytes() {
            return Err(unsupported(pc, "transient bytes in cell"));
        }
        Ok(index)
    }
}

fn unsupported(pc: usize, reason: &'static str) -> CompileError {
    CompileError::Unsupported { pc, reason }
}
fn reg(clear: &mut bool, symbols: &[Symbol], index: usize, pc: usize) -> Result<Symbol, CompileError> {
    let value = *symbols.get(index).ok_or(CompileError::InvalidChunk)?;
    if value == Symbol::Unknown {
        return Err(unsupported(pc, "unproven register value"));
    }
    // Every path that can read an implicit entry nil keeps NIL in its type
    // union. Conservatively retain clearing for all possibly-nil reads,
    // including values explicitly assigned nil or loaded from nullable inputs.
    *clear |= value.types().is_some_and(|types| types.0 & Types::NIL.0 != 0);
    Ok(value)
}
fn set(symbols: &mut [Symbol], index: usize, value: Symbol) -> Result<(), CompileError> {
    *symbols.get_mut(index).ok_or(CompileError::InvalidChunk)? = value;
    Ok(())
}
fn rk(
    clear: &mut bool,
    symbols: &[Symbol],
    constants: &[Value],
    index: usize,
    pc: usize,
) -> Result<(Operand, Symbol), CompileError> {
    if index & 256 != 0 {
        let value = *constants.get(index & 255).ok_or(CompileError::InvalidChunk)?;
        Ok((Operand::Value(value), Symbol::Constant(value)))
    } else {
        Ok((Operand::Register(index), reg(clear, symbols, index, pc)?))
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
    let tables = tables::Layout::new(bindings)?;
    lower_with(
        prototype,
        bindings,
        &bindings.arguments,
        &bindings.upvalues,
        &fields,
        tables.as_ref(),
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
    tables: Option<&Arc<tables::Layout>>,
) -> Result<Program, CompileError> {
    if prototype.upvalues != captures.len() {
        return Err(CompileError::InvalidBindings("upvalue count"));
    }
    let mut builder = Builder {
        bindings,
        index: fields,
        tables,
        strings: Vec::new(),
        cells: Vec::with_capacity(bindings.cells.len()),
        fields: vec![None; bindings.fields.len()],
        actions: Vec::new(),
    };
    let constants: Vec<_> = prototype
        .constants
        .into_iter()
        .map(|v| builder.literal(v))
        .collect();
    let constant_strings = builder.strings.len();
    let mut previous_bytes = None;
    for cell in &bindings.cells {
        let value = match cell {
            Cell::Boolean(v) => Value::Boolean(*v),
            Cell::Number(v) => Value::Number(v.to_bits()),
            Cell::Bytes(index) => {
                if let Some((previous, value)) = previous_bytes
                    && previous == *index
                {
                    value
                } else {
                    let bytes = bindings
                        .strings
                        .get(*index)
                        .ok_or(CompileError::InvalidBindings("cell string index"))?;
                    // Immutable literal storage can also seed a mutable cell;
                    // the cell retains its own declared storage index.
                    let value = builder.strings[..constant_strings]
                        .iter()
                        .position(|literal| literal == bytes)
                        .map(Value::String)
                        .unwrap_or_else(|| builder.literal(Literal::Bytes(bytes.clone())));
                    previous_bytes = Some((*index, value));
                    value
                }
            }
        };
        builder.cells.push(value);
    }
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
    let mut clear_registers = false;
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
                let v = reg(&mut clear_registers, &symbols, b, pc)?;
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
                    (reg(&mut clear_registers, &symbols, b, pc)?, rk(&mut clear_registers, &symbols, &constants, c, pc)?.1)
                };
                if object == Symbol::BooleanTable {
                    require(key, Types::BYTES, pc)?;
                    let key = rk(&mut clear_registers, &symbols, &constants, c, pc)?.0;
                    set(&mut symbols, a, Symbol::Scalar(Types::NIL | Types::BOOLEAN))?;
                    Op::ReadBooleanTable { dst: a, table: b, key }
                } else if let Symbol::BooleanTableArray(array) = object {
                    let len = builder.tables.expect("array schema").array(array).len();
                    let Some((min,max)) = key.integer_range() else { return Err(unsupported(pc, "unproven array index")); };
                    if min < 1 || max as usize > len { return Err(unsupported(pc, "array index range")); }
                    let key = rk(&mut clear_registers, &symbols, &constants, c, pc)?.0;
                    set(&mut symbols, a, Symbol::BooleanTable)?;
                    Op::ReadBooleanTableArray { dst: a, array, key }
                } else {
                    let field = builder.field(object, key, pc)?;
                    set(&mut symbols, a, builder.field_symbol(field))?;
                    Op::Load {
                        dst: a,
                        source: builder.field_source(field),
                        skip: false,
                    }
                }
            }
            7 | 8 | 9 => {
                if opcode == 9 && symbols.get(a) == Some(&Symbol::BooleanTable) {
                    let (key, key_type) = rk(&mut clear_registers, &symbols, &constants, b, pc)?;
                    let (source, value_type) = rk(&mut clear_registers, &symbols, &constants, c, pc)?;
                    require(key_type, Types::BYTES, pc)?;
                    require(value_type, Types::NIL | Types::BOOLEAN, pc)?;
                    Op::WriteBooleanTable { table: a, key, source }
                } else {
                let (target, source, symbol) = if opcode == 8 {
                    (
                        *upvalues.get(b).ok_or(CompileError::InvalidChunk)?,
                        Operand::Register(a),
                        reg(&mut clear_registers, &symbols, a, pc)?,
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
                            reg(&mut clear_registers, &symbols, a, pc)?,
                        )
                    } else {
                        let (source, symbol) = rk(&mut clear_registers, &symbols, &constants, c, pc)?;
                        (
                            reg(&mut clear_registers, &symbols, a, pc)?,
                            rk(&mut clear_registers, &symbols, &constants, b, pc)?.1,
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
            }
            11 => {
                let receiver = reg(&mut clear_registers, &symbols, b, pc)?;
                let field = builder.field(receiver, rk(&mut clear_registers, &symbols, &constants, c, pc)?.1, pc)?;
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
                let (left, lt) = rk(&mut clear_registers, &symbols, &constants, b, pc)?;
                let (right, rt) = rk(&mut clear_registers, &symbols, &constants, c, pc)?;
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
                require(reg(&mut clear_registers, &symbols, b, pc)?, Types::NUMBER, pc)?;
                set(&mut symbols, a, Symbol::Scalar(Types::NUMBER))?;
                Op::Neg { dst: a, source: b }
            }
            19 => {
                reg(&mut clear_registers, &symbols, b, pc)?;
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
                let (left, lt) = rk(&mut clear_registers, &symbols, &constants, b, pc)?;
                let (right, rt) = rk(&mut clear_registers, &symbols, &constants, c, pc)?;
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
                let value = reg(&mut clear_registers, &symbols, source, pc)?;
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
                let Symbol::Action(id) = reg(&mut clear_registers, &symbols, a, pc)? else {
                    return Err(unsupported(pc, "unbound call target"));
                };
                let call = &builder.actions[id];
                if b - 1 != call.parameters.len() {
                    return Err(unsupported(pc, "action arity"));
                }
                for (offset, parameter) in call.parameters.iter().enumerate() {
                    let symbol = reg(&mut clear_registers, &symbols, a + 1 + offset, pc)?;
                    match *parameter {
                        Parameter::Type(types) => require(symbol, types, pc)?,
                        Parameter::Object(id) if symbol == Symbol::Object(id) => {}
                        Parameter::Receiver if symbol == Symbol::Object(call.receiver) => {}
                        Parameter::Object(_) | Parameter::Receiver => {
                            return Err(unsupported(pc, "unproven action receiver"));
                        }
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
                        reg(&mut clear_registers, &symbols, index, pc)?;
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
        strings: Strings::Unique(builder.strings.into_boxed_slice()),
        tables: tables.cloned(),
        cells: builder.cells.into_boxed_slice(),
        parameters: parameters.into_boxed_slice(),
        registers: prototype.registers,
        clear_registers,
    })))
}
