//! Instructions that neither call out, fail nor allocate.
//!
//! The interpreter runs these without publishing the frame's saved pc or
//! source line: nothing can observe the frame until an instruction outside
//! this set runs, and that instruction publishes both first.
use super::{Arena, Table, TableId, Value, Vm, slot_index};
use crate::bytecode::{COMPARE_BRANCH, Instruction, OPEN, Op, RK};

/// The registers, constants and tables one simple instruction can touch.
pub(super) struct Context<'a> {
    pub registers: &'a mut [Value],
    pub constants: &'a [Value],
    pub tables: &'a Arena<Table>,
    pub code: &'a [Instruction],
    pub env: TableId,
    pub base: usize,
    /// No upvalue is open, so closing a scope has nothing to do.
    pub closed: bool,
}

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
            let value = table.get(cx.constant(i.c as usize));
            if value == Value::Nil && table.metatable().is_some() {
                return None;
            }
            cx.registers[a] = value;
        }
        Op::GetTable => {
            let Value::Table(id) = cx.registers[base + usize::from(i.b)] else {
                return None;
            };
            let table = &cx.tables[slot_index(id.0)];
            let value = table.get(cx.operand(i.c));
            if value == Value::Nil && table.metatable().is_some() {
                return None;
            }
            cx.registers[a] = value;
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
            let (Value::Number(x), Value::Number(y)) =
                (cx.operand(u32::from(i.b)), cx.operand(i.c))
            else {
                return None;
            };
            let result = match i.op {
                Op::Eq => x == y,
                Op::Ne => x != y,
                Op::Lt => x < y,
                Op::Le => x <= y,
                Op::Gt => x > y,
                _ => x >= y,
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
            let Value::Table(id) = cx.registers[base + usize::from(i.b)] else {
                return None;
            };
            cx.registers[a] = Value::Number(cx.tables[slot_index(id.0)].len() as f64);
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
