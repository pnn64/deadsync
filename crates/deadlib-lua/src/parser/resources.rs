//! Lua 5.1 logical stack accounting, independent of physical bytecode registers.
//!
//! Descriptors retain only register effects, constant identity and pending jump
//! lists. Checks run at parser continuations, before the next reader request
//! whenever the reference would already reject the source. No code is emitted.
use super::{
    Ast, ConstantOperand, Expr, ExprId, NativeRootEvent, RootText,
    bindings::{Bindings, Reference},
    hash,
};
use crate::lexer::{Kind, Lexeme};
use crate::{
    Error, ErrorKind,
    memory::fallible::{self, VecExt},
};

type Check<T = ()> = Result<T, Error>;
fn too_complex() -> Error {
    Error::new_static(ErrorKind::Limit, 0, "function or expression too complex")
}
const RK: u16 = 256;
#[derive(Clone, Copy, Debug, PartialEq)]
enum Form {
    Void,
    Nil,
    False,
    True,
    Number,
    Constant,
    Local,
    Upvalue,
    Global,
    Index,
    Reloc,
    Not,
    Concat,
    Temp,
    Call,
    Vararg,
    Jump,
}
#[derive(Clone, Copy, Debug)]
struct Value {
    number: f64,
    info: u32,
    key: u16,
    form: Form,
    jumps: u8,
}
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Value>() == 16);
impl Value {
    fn new(form: Form, info: u32) -> Self {
        Self {
            number: 0.0,
            info,
            key: 0,
            form,
            jumps: 0,
        }
    }
    fn number(number: f64) -> Self {
        Self {
            number,
            ..Self::new(Form::Number, 0)
        }
    }
    fn numeral(self) -> bool {
        self.form == Form::Number && self.jumps == 0
    }
    fn multi(self) -> bool {
        matches!(self.form, Form::Call | Form::Vararg)
    }
}
#[derive(Clone, Copy, Debug)]
struct Key {
    hash: u64,
    data: u64,
    // 0 number, 1 nil, 2 false, 3 true, 4 AST string, 5 interned name.
    kind: u8,
}
#[derive(Debug, Default)]
struct Pool {
    entries: Vec<Key>,
    buckets: Vec<u32>,
}
pub(super) struct Context<'a> {
    pub ast: &'a Ast,
    pub input: &'a [u8],
    pub bindings: &'a Bindings,
}

impl Key {
    fn native_event(self, cx: &Context<'_>, negative_zero: bool) -> Result<NativeRootEvent, Error> {
        use crate::bytecode::Constant;
        let constant = match self.kind {
            0 => Constant::Number(if negative_zero && self.data == 0 {
                -0.0
            } else {
                f64::from_bits(self.data)
            }),
            1 => Constant::Nil,
            2 | 3 => Constant::Boolean(self.kind == 3),
            4 => {
                let text = RootText::from_lexeme(self.string(cx.ast))?;
                return Ok(NativeRootEvent::ConstantText(text));
            }
            5 => {
                let text = cx.bindings.root_text(cx.input, self.data as u32)?;
                return Ok(NativeRootEvent::ConstantText(text));
            }
            _ => unreachable!(),
        };
        Ok(NativeRootEvent::Constant(constant))
    }
    fn string(self, ast: &Ast) -> &Lexeme {
        let Expr::String(bytes) = &ast.exprs[self.data as usize].0 else {
            unreachable!()
        };
        bytes
    }
    fn scalar(kind: u8, data: u64) -> Self {
        Self {
            hash: hash(&data.to_le_bytes()) ^ u64::from(kind),
            data,
            kind,
        }
    }
    fn number(n: f64) -> Self {
        Self::scalar(0, if n == 0.0 { 0 } else { n.to_bits() })
    }
    fn bytes<'a>(self, cx: &Context<'a>) -> &'a [u8] {
        if self.kind == 5 {
            cx.bindings.bytes(cx.input, self.data as u32)
        } else {
            self.string(cx.ast).bytes(cx.input)
        }
    }
}
impl Pool {
    fn clear(&mut self) {
        self.entries.clear();
        self.buckets.clear();
    }

    fn insert(&mut self, key: Key, cx: &Context<'_>) -> Result<u32, Error> {
        let mut slot = 0;
        if !self.buckets.is_empty() {
            let mask = self.buckets.len() - 1;
            slot = key.hash as usize & mask;
            while self.buckets[slot] != 0 {
                let index = self.buckets[slot] - 1;
                let old = self.entries[index as usize];
                if old.hash == key.hash
                    && (if old.kind >= 4 && key.kind >= 4 && (old.kind != 5 || key.kind != 5) {
                        old.bytes(cx) == key.bytes(cx)
                    } else {
                        old.kind == key.kind && old.data == key.data
                    })
                {
                    return Ok(index);
                }
                slot = (slot + 1) & mask;
            }
        }
        if (self.entries.len() + 1) * 2 >= self.buckets.len() {
            let size = (self.buckets.len() * 2).max(16);
            self.buckets.clear();
            self.buckets.try_resize(size, 0)?;
            for (index, entry) in self.entries.iter().enumerate() {
                let mut slot = entry.hash as usize & (self.buckets.len() - 1);
                while self.buckets[slot] != 0 {
                    slot = (slot + 1) & (self.buckets.len() - 1);
                }
                self.buckets[slot] = index as u32 + 1;
            }
            slot = key.hash as usize & (size - 1);
            while self.buckets[slot] != 0 {
                slot = (slot + 1) & (size - 1);
            }
        }
        let index = self.entries.len() as u32;
        self.entries.try_push(key)?;
        self.buckets[slot] = index + 1;
        Ok(index)
    }
    fn heap_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<Key>() + self.buckets.capacity() * size_of::<u32>()
    }
}
#[derive(Debug, Default)]
struct Function {
    next: u16,
    max: u16,
    locals: u16,
    pool: Pool,
    zero_sign: Option<bool>,
}
#[derive(Debug)]
struct Table {
    reg: u16,
    saved: u16,
    last: Option<ExprId>,
    pending: u8,
    prepared: bool,
}
#[derive(Debug, Default)]
pub(super) struct Resources {
    function: Function,
    parents: Vec<Function>,
    child_pools: Vec<Pool>,
    values: Vec<Value>,
    tables: Vec<Table>,
    constants: Vec<(ExprId, ConstantOperand)>,
}
impl Resources {
    /// Return to the default state, keeping storage.
    pub(super) fn clear(&mut self) {
        // A recursive parse can stop inside a function before the streaming
        // parser restarts. Recover the pools from those unfinished scopes too.
        while !self.parents.is_empty() {
            self.leave();
        }
        self.function.pool.clear();
        self.function.next = 0;
        self.function.max = 0;
        self.function.locals = 0;
        self.function.zero_sign = None;
        self.values.clear();
        self.tables.clear();
        self.constants.clear();
    }

    pub(super) fn reuse_operands(&mut self, ast: &mut Ast) {
        // A successful parse moved the operand buffer into the output tree.
        if ast.constant_operands.capacity() > self.constants.capacity() {
            std::mem::swap(&mut ast.constant_operands, &mut self.constants);
        }
    }
    /// Reserve per-expression accounting for an estimated expression count.
    pub(super) fn reserve_values(&mut self, expressions: usize) -> Result<(), Error> {
        crate::memory::fallible::reserve(&mut self.values, expressions)
    }
    pub(super) fn constant_count(&self) -> usize {
        self.function.pool.entries.len()
    }
    pub(super) fn append_native_constants(
        &self,
        start: usize,
        cx: &Context<'_>,
        events: &mut Vec<NativeRootEvent>,
    ) -> Result<(), Error> {
        let entries = &self.function.pool.entries[start..];
        if entries.is_empty() {
            return Ok(());
        }
        fallible::reserve(events, entries.len())?;
        let before = events.len();
        let negative_zero = self.function.zero_sign == Some(true);
        for key in entries {
            match key.native_event(cx, negative_zero) {
                Ok(event) => events.push(event),
                Err(error) => {
                    events.truncate(before);
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    pub(super) fn heap_bytes(&self) -> usize {
        self.parents.capacity() * size_of::<Function>()
            + self.values.capacity() * size_of::<Value>()
            + self.tables.capacity() * size_of::<Table>()
            + self.constants.capacity() * size_of::<(ExprId, ConstantOperand)>()
            + self.function.pool.heap_bytes()
            + self.child_pools.capacity() * size_of::<Pool>()
            + self.child_pools.iter().map(Pool::heap_bytes).sum::<usize>()
            + self
                .parents
                .iter()
                .map(|f| f.pool.heap_bytes())
                .sum::<usize>()
    }
    pub(super) fn check(&mut self, count: usize) -> Check {
        if usize::from(self.function.next) + count >= 250 {
            Err(too_complex())
        } else {
            self.function.max = self.function.max.max(self.function.next + count as u16);
            Ok(())
        }
    }
    pub(super) fn default_step(&mut self, cx: &Context<'_>) -> Check {
        self.function.pool.insert(Key::number(1.0), cx)?;
        self.reserve(1)?;
        Ok(())
    }
    pub(super) fn reserve(&mut self, count: usize) -> Check<u16> {
        self.check(count)?;
        let start = self.function.next;
        self.function.next += count as u16;
        Ok(start)
    }
    pub(super) fn locals(&mut self, count: u16) {
        self.function.locals = count;
        self.function.next = count;
        self.function.max = self.function.max.max(count);
    }
    pub(super) fn reset(&mut self) {
        self.function.next = self.function.locals;
    }
    pub(super) fn enter(&mut self, count: u16) -> Result<(), Error> {
        let depth = self.parents.len();
        if depth == self.child_pools.len() {
            self.child_pools.try_push(Pool::default())?;
        }
        self.parents.try_push(std::mem::take(&mut self.function))?;
        self.function.pool = std::mem::take(&mut self.child_pools[depth]);
        self.locals(count);
        Ok(())
    }
    pub(super) fn leave(&mut self) -> (Option<bool>, u16) {
        let mut child = std::mem::replace(
            &mut self.function,
            self.parents.pop().expect("resource parent"),
        );
        child.pool.clear();
        self.child_pools[self.parents.len()] = child.pool;
        (child.zero_sign, child.max.max(2))
    }
    pub(super) fn finish(&mut self, ast: &mut Ast) {
        // An operand normally has a direct index. Only oversized operand
        // lists and conflicting entries require the ordered lookup path.
        let indexed = self.constants.len() < usize::from(u16::MAX)
            && self
                .constants
                .iter()
                .enumerate()
                .all(|(index, &(id, operand))| {
                    let slot = &mut ast.exprs[id].1.operand;
                    if *slot == 0 {
                        *slot = (index + 1) as u16;
                        true
                    } else {
                        Self::same_operand(self.constants[usize::from(*slot - 1)].1, operand)
                    }
                });
        if !indexed {
            self.constants.sort_unstable_by_key(|&(id, _)| id);
            let mut offset = 0;
            for group in self.constants.chunk_by(|left, right| left.0 == right.0) {
                let id = group[0].0;
                let index = if group.len() == 1 {
                    offset
                } else {
                    self.constants
                        .binary_search_by_key(&id, |&(id, _)| id)
                        .expect("known operand")
                };
                ast.exprs[id].1.operand = u16::try_from(index + 1).unwrap_or(u16::MAX);
                offset += group.len();
            }
        }
        ast.constant_operands = std::mem::take(&mut self.constants);
        ast.root_zero_sign = self.function.zero_sign;
        ast.root_stack = self.function.max.max(2);
    }
    fn same_operand(left: ConstantOperand, right: ConstantOperand) -> bool {
        match (left, right) {
            (ConstantOperand::Nil, ConstantOperand::Nil) => true,
            (ConstantOperand::Boolean(left), ConstantOperand::Boolean(right)) => left == right,
            (ConstantOperand::String(left), ConstantOperand::String(right)) => left == right,
            (ConstantOperand::Number(left, a), ConstantOperand::Number(right, b)) => {
                left.to_bits() == right.to_bits() && a == b
            }
            _ => false,
        }
    }
    fn number(&mut self, number: f64, cx: &Context<'_>) -> Result<u32, Error> {
        if number == 0.0 && self.function.zero_sign.is_none() {
            self.function.zero_sign = Some(number.is_sign_negative());
        }
        self.function.pool.insert(Key::number(number), cx)
    }
    fn save_operand(&mut self, id: ExprId, operand: u16) -> Result<(), Error> {
        if operand < RK {
            return Ok(());
        }
        let key = self.function.pool.entries[usize::from(operand - RK)];
        let constant = match key.kind {
            0 => {
                let number = f64::from_bits(key.data);
                ConstantOperand::Number(
                    if number == 0.0 && self.function.zero_sign == Some(true) {
                        -0.0
                    } else {
                        number
                    },
                    Some((operand - RK) as u8),
                )
            }
            1 => ConstantOperand::Nil,
            2 | 3 => ConstantOperand::Boolean(key.kind == 3),
            4 => ConstantOperand::String(key.data as ExprId),
            _ => unreachable!("a constant string operand has an AST literal"),
        };
        self.constants.try_push((id, constant))?;
        Ok(())
    }
    fn free(&mut self, reg: u16) {
        if reg < RK && reg >= self.function.locals {
            debug_assert_eq!(reg + 1, self.function.next, "logical stack free");
            self.function.next -= 1;
        }
    }
    fn free_value(&mut self, value: Value) {
        if value.form == Form::Temp {
            self.free(value.info as u16);
        }
    }
    fn discharge(&mut self, value: &mut Value) {
        value.form = match value.form {
            Form::Local | Form::Call => Form::Temp,
            Form::Upvalue | Form::Global | Form::Vararg => Form::Reloc,
            Form::Index => {
                self.free(value.key);
                self.free(value.info as u16);
                Form::Reloc
            }
            other => other,
        };
    }
    fn load(&mut self, value: &mut Value, reg: u16, cx: &Context<'_>) -> Result<(), Error> {
        if value.form == Form::Number {
            self.number(value.number, cx)?;
        }
        value.form = Form::Temp;
        value.info = u32::from(reg);
        Ok(())
    }
    fn next_value(&mut self, value: &mut Value, cx: &Context<'_>) -> Check {
        self.discharge(value);
        self.free_value(*value);
        let reg = self.reserve(1)?;
        self.load(value, reg, cx)?;
        value.jumps = 0;
        Ok(())
    }
    fn any_value(&mut self, value: &mut Value, cx: &Context<'_>) -> Check {
        self.discharge(value);
        if value.form == Form::Temp
            && (value.jumps == 0 || value.info >= u32::from(self.function.locals))
        {
            value.jumps = 0;
            Ok(())
        } else {
            self.next_value(value, cx)
        }
    }
    fn plain_any(&mut self, value: &mut Value, cx: &Context<'_>) -> Check {
        self.discharge(value);
        if value.form != Form::Temp {
            let reg = self.reserve(1)?;
            self.load(value, reg, cx)?;
        }
        Ok(())
    }
    fn value(&mut self, value: &mut Value, cx: &Context<'_>) -> Check {
        if value.jumps != 0 {
            self.any_value(value, cx)
        } else {
            self.discharge(value);
            Ok(())
        }
    }
    fn rk(&mut self, value: &mut Value, cx: &Context<'_>) -> Check<u16> {
        self.value(value, cx)?;
        let key = match value.form {
            Form::Number => Some(Key::number(value.number)),
            Form::Nil => Some(Key::scalar(1, 0)),
            Form::False => Some(Key::scalar(2, 0)),
            Form::True => Some(Key::scalar(3, 0)),
            _ => None,
        };
        if let Some(key) = key
            && self.function.pool.entries.len() <= 255
        {
            value.info = if value.form == Form::Number {
                self.number(value.number, cx)?
            } else {
                self.function.pool.insert(key, cx)?
            };
            value.form = Form::Constant;
        }
        if value.form == Form::Constant && value.info <= 255 {
            Ok(RK + value.info as u16)
        } else {
            self.any_value(value, cx)?;
            Ok(value.info as u16)
        }
    }
    pub(super) fn next(&mut self, id: ExprId, cx: &Context<'_>) -> Check {
        let mut v = self.values[id];
        self.next_value(&mut v, cx)?;
        self.values[id] = v;
        Ok(())
    }
    pub(super) fn any(&mut self, id: ExprId, cx: &Context<'_>) -> Check {
        let mut v = self.values[id];
        self.any_value(&mut v, cx)?;
        self.values[id] = v;
        Ok(())
    }
    pub(super) fn val(&mut self, id: ExprId, cx: &Context<'_>) -> Check {
        let mut v = self.values[id];
        self.value(&mut v, cx)?;
        self.values[id] = v;
        Ok(())
    }
    fn reference(&mut self, reference: Reference, cx: &Context<'_>) -> Check<Value> {
        Ok(match reference {
            Reference::Local { register, .. } => Value::new(Form::Local, u32::from(register)),
            Reference::Upvalue => Value::new(Form::Upvalue, 0),
            Reference::Global(symbol) => {
                let index = self.function.pool.insert(
                    Key {
                        hash: cx.bindings.symbol_hash(symbol),
                        data: u64::from(symbol),
                        kind: 5,
                    },
                    cx,
                )?;
                Value::new(Form::Global, index)
            }
        })
    }
    pub(super) fn resolve(
        &mut self,
        id: ExprId,
        reference: Reference,
        cx: &Context<'_>,
    ) -> Check<Option<u32>> {
        let value = self.reference(reference, cx)?;
        self.values[id] = value;
        Ok((value.form == Form::Global).then_some(value.info))
    }
    pub(super) fn push_reference(
        &mut self,
        reference: Reference,
        cx: &Context<'_>,
    ) -> Check<Option<u32>> {
        let value = self.reference(reference, cx)?;
        self.values.try_push(value)?;
        Ok((value.form == Form::Global).then_some(value.info))
    }
    fn condition_value(&mut self, value: &mut Value, truth: bool, cx: &Context<'_>) -> Check {
        self.discharge(value);
        let no_jump = if truth {
            matches!(value.form, Form::Constant | Form::Number | Form::True)
        } else {
            matches!(value.form, Form::Nil | Form::False)
        };
        if !no_jump {
            if !matches!(value.form, Form::Jump | Form::Not) {
                self.plain_any(value, cx)?;
                self.free_value(*value);
            }
            value.jumps |= if truth { 2 } else { 1 };
        }
        value.jumps &= if truth { 2 } else { 1 };
        Ok(())
    }
    pub(super) fn condition(&mut self, id: ExprId, cx: &Context<'_>) -> Check {
        let mut v = self.values[id];
        if v.form == Form::Nil {
            v.form = Form::False;
        }
        self.condition_value(&mut v, true, cx)?;
        self.values[id] = v;
        Ok(())
    }
    pub(super) fn infix(&mut self, id: ExprId, op: Kind, cx: &Context<'_>) -> Check {
        let mut v = self.values[id];
        match op {
            Kind::And => self.condition_value(&mut v, true, cx)?,
            Kind::Or => self.condition_value(&mut v, false, cx)?,
            Kind::Concat => self.next_value(&mut v, cx)?,
            Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash | Kind::Percent | Kind::Caret
                if v.numeral() => {}
            _ => {
                self.rk(&mut v, cx)?;
            }
        }
        self.values[id] = v;
        Ok(())
    }
    fn unary(&mut self, op: Kind, mut v: Value, cx: &Context<'_>) -> Check<Value> {
        if op == Kind::Not {
            self.discharge(&mut v);
            v.form = match v.form {
                Form::Nil | Form::False => Form::True,
                Form::Constant | Form::Number | Form::True => Form::False,
                Form::Jump => Form::Jump,
                _ => {
                    self.plain_any(&mut v, cx)?;
                    self.free_value(v);
                    Form::Not
                }
            };
            v.jumps = ((v.jumps & 1) << 1) | ((v.jumps & 2) >> 1);
            return Ok(v);
        }
        if op == Kind::Minus && v.numeral() {
            return Ok(Value::number(-v.number));
        }
        self.any_value(&mut v, cx)?;
        self.rk(&mut v, cx)?;
        self.free_value(v);
        Ok(Value {
            form: Form::Reloc,
            ..v
        })
    }
    fn binary(
        &mut self,
        op: Kind,
        left: ExprId,
        right: ExprId,
        mut a: Value,
        mut b: Value,
        cx: &Context<'_>,
    ) -> Check<Value> {
        if matches!(op, Kind::And | Kind::Or) {
            self.discharge(&mut b);
            b.jumps |= a.jumps;
            return Ok(b);
        }
        if op == Kind::Concat {
            self.value(&mut b, cx)?;
            if b.form == Form::Concat {
                self.free_value(a);
                return Ok(Value { info: a.info, ..b });
            }
            self.next_value(&mut b, cx)?;
            self.rk(&mut b, cx)?;
            self.rk(&mut a, cx)?;
            self.free_value(b);
            self.free_value(a);
            return Ok(Value::new(Form::Concat, a.info));
        }
        let comparison = matches!(
            op,
            Kind::Eq | Kind::Ne | Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge
        );
        if !comparison && a.numeral() && b.numeral() {
            let x = a.number;
            let y = b.number;
            let folded = match op {
                Kind::Plus => Some(x + y),
                Kind::Minus => Some(x - y),
                Kind::Star => Some(x * y),
                Kind::Slash if y != 0.0 => Some(x / y),
                Kind::Percent if y != 0.0 => Some(x - (x / y).floor() * y),
                Kind::Caret => Some(x.powf(y)),
                _ => None,
            };
            if let Some(n) = folded
                && !n.is_nan()
            {
                return Ok(Value::number(n));
            }
        }
        let folded_left =
            (a.numeral() && !matches!(cx.ast.exprs[left].0, Expr::Number(_))).then_some(a.number);
        let (ar, br) = if comparison {
            (self.rk(&mut a, cx)?, self.rk(&mut b, cx)?)
        } else {
            let br = self.rk(&mut b, cx)?;
            (self.rk(&mut a, cx)?, br)
        };
        self.save_operand(left, ar)?;
        self.save_operand(right, br)?;
        if ar < RK
            && let Some(number) = folded_left
        {
            self.constants
                .try_push((left, ConstantOperand::Number(number, None)))?;
        }
        if ar > br {
            self.free_value(a);
            self.free_value(b);
        } else {
            self.free_value(b);
            self.free_value(a);
        }
        Ok(Value::new(
            if comparison { Form::Jump } else { Form::Reloc },
            0,
        ))
    }
    pub(super) fn method(&mut self, id: ExprId, symbol: u32, cx: &Context<'_>) -> Check<u32> {
        let index = self.function.pool.insert(
            Key {
                hash: cx.bindings.symbol_hash(symbol),
                data: u64::from(symbol),
                kind: 5,
            },
            cx,
        )?;
        let mut v = self.values[id];
        self.any_value(&mut v, cx)?;
        self.free_value(v);
        let base = self.reserve(2)?;
        let mut key = Value::new(Form::Constant, index);
        self.rk(&mut key, cx)?;
        self.free_value(key);
        self.values[id] = Value::new(Form::Temp, u32::from(base));
        Ok(index)
    }
    fn returns(&mut self, value: Value, count: Option<usize>) -> Check {
        if value.form == Form::Vararg {
            self.reserve(1)?;
        }
        if let Some(count) = count
            && count > 1
        {
            self.reserve(count - 1)?;
        }
        Ok(())
    }
    pub(super) fn adjust(&mut self, wanted: usize, values: &[ExprId], cx: &Context<'_>) -> Check {
        if let Some(&last) = values.last() {
            let v = self.values[last];
            if v.multi() {
                self.returns(
                    v,
                    Some(wanted.saturating_add(1).saturating_sub(values.len())),
                )?;
            } else {
                self.next(last, cx)?;
                if wanted > values.len() {
                    self.reserve(wanted - values.len())?;
                }
            }
        } else {
            self.reserve(wanted)?;
        }
        Ok(())
    }
    pub(super) fn return_values(&mut self, values: &[ExprId], cx: &Context<'_>) -> Check {
        if let Some(&last) = values.last() {
            if self.values[last].multi() {
                self.returns(self.values[last], None)?;
            } else if values.len() == 1 {
                self.any(last, cx)?;
            } else {
                self.next(last, cx)?;
            }
        }
        Ok(())
    }
    pub(super) fn arguments(&mut self, values: &[ExprId]) -> Check {
        if let Some(&last) = values.last()
            && self.values[last].multi()
        {
            self.returns(self.values[last], None)?;
        }
        Ok(())
    }
    pub(super) fn conflict(&mut self, targets: &[ExprId], id: ExprId) -> Check {
        let v = self.values[id];
        if v.form != Form::Local {
            return Ok(());
        }
        let reg = v.info as u16;
        let mut copy = None;
        for &id in targets {
            let mut t = self.values[id];
            if t.form == Form::Index && (t.info == u32::from(reg) || t.key == reg) {
                let fresh = if let Some(r) = copy {
                    r
                } else {
                    let r = self.reserve(1)?;
                    copy = Some(r);
                    r
                };
                if t.info == u32::from(reg) {
                    t.info = u32::from(fresh);
                }
                if t.key == reg {
                    t.key = fresh;
                }
                self.values[id] = t;
            }
        }
        Ok(())
    }
    pub(super) fn assignment(
        &mut self,
        targets: &[ExprId],
        values: &[ExprId],
        cx: &Context<'_>,
    ) -> Check {
        for &target in targets {
            if let Expr::Index(_, key) = cx.ast.exprs[target].0 {
                self.save_operand(key, self.values[target].key)?;
            }
        }
        if targets.len() == values.len() {
            let mut value = self.values[*values.last().unwrap()];
            let target = self.values[*targets.last().unwrap()];
            self.discharge(&mut value);
            if target.form == Form::Local {
                self.load(&mut value, target.info as u16, cx)?;
            } else {
                if target.form == Form::Index {
                    self.rk(&mut value, cx)?;
                } else {
                    self.any_value(&mut value, cx)?;
                }
            }
        } else {
            self.adjust(targets.len(), values, cx)?;
        }
        Ok(())
    }
    pub(super) fn begin_table(&mut self) -> Check {
        let reg = self.reserve(1)?;
        self.tables.try_push(Table {
            reg,
            saved: self.function.next,
            last: None,
            pending: 0,
            prepared: false,
        })?;
        Ok(())
    }
    pub(super) fn prepare_field(&mut self, cx: &Context<'_>) -> Check {
        let t = self.tables.last().expect("constructor");
        if t.prepared {
            return Ok(());
        }
        if let Some(last) = t.last {
            self.next(last, cx)?;
        }
        let t = self.tables.last_mut().unwrap();
        if t.pending == 50 {
            self.function.next = t.reg + 1;
            t.pending = 0;
        }
        t.last = None;
        t.saved = self.function.next;
        t.prepared = true;
        Ok(())
    }
    pub(super) fn record_key(&mut self, id: ExprId, cx: &Context<'_>) -> Check {
        let mut v = self.values[id];
        let operand = self.rk(&mut v, cx)?;
        self.save_operand(id, operand)?;
        self.values[id] = v;
        Ok(())
    }
    pub(super) fn field(&mut self, key: Option<ExprId>, value: ExprId, cx: &Context<'_>) -> Check {
        if key.is_some() {
            let mut v = self.values[value];
            let operand = self.rk(&mut v, cx)?;
            self.save_operand(value, operand)?;
            self.function.next = self.tables.last().unwrap().saved;
        }
        let t = self.tables.last_mut().unwrap();
        if key.is_none() {
            t.last = Some(value);
            t.pending += 1;
        }
        t.prepared = false;
        Ok(())
    }
    fn string_constant(&mut self, id: ExprId, text: &Lexeme, cx: &Context<'_>) -> Check<u32> {
        let slot = self.function.pool.insert(
            Key {
                hash: hash(text.bytes(cx.input)),
                data: id as u64,
                kind: 4,
            },
            cx,
        )?;
        // An identifier or method may have interned these bytes first.
        // Retain this literal as an allocation-free compiler source.
        self.function.pool.entries[slot as usize].kind = 4;
        self.function.pool.entries[slot as usize].data = id as u64;
        Ok(slot)
    }

    pub(super) fn push_number(&mut self, number: f64) -> Check {
        self.values.try_push(Value::number(number))
    }

    pub(super) fn push_string(&mut self, id: ExprId, cx: &Context<'_>) -> Check<u32> {
        let Expr::String(text) = &cx.ast.exprs[id].0 else {
            unreachable!("string expression")
        };
        debug_assert_eq!(self.values.len(), id);
        let slot = self.string_constant(id, text, cx)?;
        self.values.try_push(Value::new(Form::Constant, slot))?;
        Ok(slot)
    }

    pub(super) fn expression(&mut self, id: ExprId, cx: &Context<'_>) -> Check<Option<u32>> {
        let mut string_slot = None;
        let v = match &cx.ast.exprs[id].0 {
            Expr::Nil => Value::new(Form::Nil, 0),
            Expr::Boolean(v) => Value::new(if *v { Form::True } else { Form::False }, 0),
            Expr::Number(n) => Value::number(*n),
            Expr::Name(_) => Value::new(Form::Void, 0),
            Expr::String(bytes) => {
                let k = self.string_constant(id, bytes, cx)?;
                string_slot = Some(k);
                Value::new(Form::Constant, k)
            }
            Expr::Vararg => Value::new(Form::Vararg, 0),
            Expr::Function { .. } => Value::new(Form::Reloc, 0),
            Expr::Paren(inner) => {
                let mut v = self.values[*inner];
                self.discharge(&mut v);
                v
            }
            Expr::Unary(op, inner) => self.unary(*op, self.values[*inner], cx)?,
            Expr::Binary(op, a, b, _) => {
                self.binary(*op, *a, *b, self.values[*a], self.values[*b], cx)?
            }
            Expr::Index(table, key) => {
                let mut k = self.values[*key];
                let operand = self.rk(&mut k, cx)?;
                self.save_operand(*key, operand)?;
                let mut v = self.values[*table];
                v.form = Form::Index;
                v.key = operand;
                v
            }
            Expr::Table(..) => {
                let t = self.tables.pop().expect("constructor");
                if t.pending != 0 {
                    if let Some(last) = t.last {
                        if self.values[last].multi() {
                            self.returns(self.values[last], None)?;
                        } else {
                            self.next(last, cx)?;
                        }
                    }
                    self.function.next = t.reg + 1;
                }
                Value::new(Form::Temp, u32::from(t.reg))
            }
            Expr::Call(function, args, _) | Expr::Method(function, _, args, _) => {
                if let Some(&last) = cx.ast.ids(*args).last()
                    && !self.values[last].multi()
                {
                    self.next(last, cx)?;
                }
                let base = self.values[*function].info;
                self.function.next = base as u16 + 1;
                Value::new(Form::Call, base)
            }
        };
        debug_assert_eq!(self.values.len(), id);
        self.values.try_push(v)?;
        Ok(string_slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_restart_recovers_constant_buffers_from_unfinished_scopes() {
        let mut resources = Resources::default();
        for (depth, capacity) in [8, 16, 32].into_iter().enumerate() {
            if depth != 0 {
                resources.enter(1).unwrap();
            }
            let pool = &mut resources.function.pool;
            fallible::reserve(&mut pool.entries, capacity).unwrap();
            pool.entries.push(Key::number(depth as f64));
        }

        resources.clear();
        assert!(resources.parents.is_empty());
        for (depth, capacity) in [8, 16, 32].into_iter().enumerate() {
            if depth != 0 {
                resources.enter(1).unwrap();
            }
            let pool = &resources.function.pool;
            assert!(pool.entries.is_empty());
            assert!(pool.entries.capacity() >= capacity);
        }
    }
}
