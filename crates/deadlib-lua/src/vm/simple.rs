//! Instructions that neither call out, fail nor allocate.
//!
//! The interpreter runs these without publishing the frame's saved pc or
//! source line: nothing can observe the frame until an instruction outside
//! this set runs, and that instruction publishes both first.
use super::{
    Arena, ByteString, Cell, StringId, Table, TableId, ThreadId, Value, Vm, slot_index, threads,
};
use crate::bytecode::{COMPARE_BRANCH, Instruction, OPEN, Op, RK};

/// The registers, constants and tables one simple instruction can touch.
pub(super) struct Context<'a> {
    pub registers: &'a mut [Value],
    pub constants: &'a [Value],
    pub tables: &'a mut Arena<Table>,
    pub strings: &'a Arena<ByteString>,
    pub upvalue_ids: &'a [u32],
    pub upvalues: &'a Arena<Cell>,
    pub threads: &'a Arena<threads::Thread>,
    pub thread: ThreadId,
    pub code: &'a [Instruction],
    pub env: TableId,
    pub base: usize,
    /// A pending collection must still be polled by the normal SetTable path.
    pub gc_pending: bool,
    /// No upvalue is open, so closing a scope has nothing to do.
    pub closed: bool,
    /// The interned `__index` name, for inherited field reads.
    pub index_name: StringId,
}

/// Table-valued `__index` handlers followed before deferring to the general
/// interpreter, which owns the 100-step `loop in gettable` diagnostic.
const INDEX_CHAIN: usize = 8;

impl Context<'_> {
    #[inline(always)]
    fn constant(&self, index: usize) -> Value {
        self.constants[index]
    }
    #[inline(always)]
    fn operand(&self, operand: u32) -> Value {
        if operand < RK {
            self.registers[self.base + operand as usize]
        } else {
            self.constant((operand - RK) as usize)
        }
    }
}

/// Read `key` through table-valued `__index` handlers after a raw miss.
///
/// Such handlers neither call out nor fail. `None` defers a function handler,
/// a non-table handler or a long chain to the general interpreter.
#[inline]
pub(super) fn inherited(
    tables: &Arena<Table>,
    index_name: StringId,
    mut metatable: TableId,
    key: Value,
) -> Option<Value> {
    for _ in 0..INDEX_CHAIN {
        let parent = match tables[slot_index(metatable.0)].get_string(index_name) {
            Value::Nil => return Some(Value::Nil),
            Value::Table(parent) => &tables[slot_index(parent.0)],
            _ => return None,
        };
        let value = match key {
            Value::String(id) => parent.get_string(id),
            key => parent.get(key),
        };
        if value != Value::Nil {
            return Some(value);
        }
        match parent.metatable() {
            Some(next) => metatable = next,
            None => return Some(Value::Nil),
        }
    }
    None
}

/// Execute the instruction at `pc` if it is simple, returning the next pc.
/// `None` leaves every register unchanged for the general interpreter.
#[inline(always)]
pub(super) fn step(cx: &mut Context<'_>, pc: usize) -> Option<usize> {
    let i = cx.code[pc];
    let base = cx.base;
    let a = base + usize::from(i.a);
    let mut next = pc + 1;
    match i.op {
        Op::Move if i.c != OPEN => cx.registers[a] = cx.registers[base + usize::from(i.b)],
        Op::Load => {
            if i.b == 0 {
                cx.registers[a] = cx.constant(i.c as usize);
            }
        }
        Op::GetGlobal => {
            let table = &cx.tables[slot_index(cx.env.0)];
            let key = cx.constant(i.c as usize);
            let mut value = match key {
                Value::String(id) => table.get_string(id),
                key => table.get(key),
            };
            if value == Value::Nil
                && let Some(metatable) = table.metatable()
            {
                value = inherited(cx.tables, cx.index_name, metatable, key)?;
            }
            cx.registers[a] = value;
        }
        Op::SetGlobal => {
            let Value::String(name) = cx.constant(i.c as usize) else {
                return None;
            };
            // An existing, non-nil scalar can be overwritten without
            // invoking __newindex, allocating, or adding a GC edge.
            if !cx.tables[slot_index(cx.env.0)].set_existing_string_scalar(name, cx.registers[a]) {
                return None;
            }
        }
        Op::GetTable => {
            let Value::Table(id) = cx.registers[base + usize::from(i.b)] else {
                return None;
            };
            let table = &cx.tables[slot_index(id.0)];
            let key = cx.operand(i.c);
            let mut value = match key {
                Value::String(id) => table.get_string(id),
                key => table.get(key),
            };
            if value == Value::Nil
                && let Some(metatable) = table.metatable()
            {
                value = inherited(cx.tables, cx.index_name, metatable, key)?;
            }
            cx.registers[a] = value;
        }
        Op::GetUpvalue => {
            let id = cx.upvalue_ids[i.c as usize] as usize;
            cx.registers[a] = match cx.upvalues[id] {
                Cell::Closed(value) => value,
                Cell::Open(thread, register) if thread == cx.thread => cx.registers[register],
                Cell::Open(thread, register) => {
                    cx.threads[slot_index(thread.0)].registers[register]
                }
            };
        }
        Op::SetTable => {
            if cx.gc_pending {
                return None;
            }
            let Value::Table(id) = cx.registers[a] else {
                return None;
            };
            let key = cx.operand(u32::from(i.b));
            let value = cx.operand(i.c);
            let table = &mut cx.tables[slot_index(id.0)];
            // An existing non-nil string field skips __newindex even with a
            // metatable. Array writes need an absent metatable because a nil
            // slot can trigger __newindex. Scalar writes create no GC edge.
            let written = match key {
                Value::String(name) => table.set_existing_string_scalar(name, value),
                _ if table.metatable().is_none() => table.set_existing_array_scalar(key, value),
                _ => false,
            };
            if !written {
                return None;
            }
        }
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
            let (Value::Number(x), Value::Number(y)) =
                (cx.operand(u32::from(i.b)), cx.operand(i.c))
            else {
                return None;
            };
            cx.registers[a] = Value::Number(Vm::number_op(i.op, x, y));
        }
        Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
            let (left, right) = (cx.operand(u32::from(i.b)), cx.operand(i.c));
            let result = match (left, right) {
                (Value::Number(x), Value::Number(y)) => match i.op {
                    Op::Eq => x == y,
                    Op::Ne => x != y,
                    Op::Lt => x < y,
                    Op::Le => x <= y,
                    Op::Gt => x > y,
                    _ => x >= y,
                },
                _ if matches!(i.op, Op::Eq | Op::Ne) => {
                    let equal = left == right;
                    // A shared __eq handler needs metatables on both objects.
                    if !equal {
                        match (left, right) {
                            (Value::Table(a), Value::Table(b))
                                if cx.tables[slot_index(a.0)].metatable().is_some()
                                    && cx.tables[slot_index(b.0)].metatable().is_some() =>
                            {
                                return None;
                            }
                            (Value::Userdata(_), Value::Userdata(_)) => return None,
                            _ => {}
                        }
                    }
                    equal == (i.op == Op::Eq)
                }
                _ => return None,
            };
            cx.registers[a] = Value::Boolean(result);
            // The consumer of a comparison runs with it.
            if let Some(following) = cx.code.get(next) {
                match following.op {
                    Op::Bool => {
                        cx.registers[base + usize::from(following.a)] =
                            Value::Boolean(result ^ (following.c != 0));
                        next += 1;
                    }
                    Op::JumpFalse | Op::JumpTrue if following.b == COMPARE_BRANCH => {
                        if result == (following.op == Op::JumpTrue) {
                            next = following.c as usize;
                        } else {
                            next += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        Op::Neg => {
            let Value::Number(x) = cx.registers[base + usize::from(i.b)] else {
                return None;
            };
            cx.registers[a] = Value::Number(-x);
        }
        Op::Len => {
            let length = match cx.registers[base + usize::from(i.b)] {
                Value::Table(id) => cx.tables[slot_index(id.0)].len(),
                Value::String(id) => cx.strings.get(id.0).map_or(0, |s| s.len()),
                _ => return None,
            };
            cx.registers[a] = Value::Number(length as f64);
        }
        Op::Not => {
            cx.registers[a] = Value::Boolean(!cx.registers[base + usize::from(i.b)].is_truthy());
        }
        Op::Bool => {
            let source = cx.registers[base + usize::from(i.b)];
            cx.registers[a] = if i.c == 0 {
                source
            } else {
                Value::Boolean(!source.is_truthy())
            };
        }
        Op::Close if cx.closed => {}
        Op::Jump => next = i.c as usize,
        Op::JumpFalse => {
            if !cx.registers[a].is_truthy() {
                next = i.c as usize;
            }
        }
        Op::JumpTrue => {
            if cx.registers[a].is_truthy() {
                next = i.c as usize;
            }
        }
        Op::ForLoop => {
            let (Value::Number(index), Value::Number(limit), Value::Number(step)) =
                (cx.registers[a], cx.registers[a + 1], cx.registers[a + 2])
            else {
                return None;
            };
            let value = index + step;
            if if step > 0.0 {
                value <= limit
            } else {
                value >= limit
            } {
                cx.registers[a] = Value::Number(value);
                cx.registers[a + 3] = Value::Number(value);
                next = i.c as usize;
            }
        }
        _ => return None,
    }
    Some(next)
}
