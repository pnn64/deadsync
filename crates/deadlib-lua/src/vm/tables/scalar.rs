//! Scalar sort callbacks with the same type, hook, budget and GC boundaries.
//!
//! Unsupported bodies or active observers use ordinary frame dispatch. The
//! comparator stays in its native argument slot; cached eligibility never roots
//! an old comparator or replaces the live function's identity check.
use super::*;
use crate::bytecode::{Instruction, OPEN, Op, RK};

#[derive(Clone, Copy)]
struct Update {
    table: TableId,
    key: StringId,
    number: f64,
}

impl Vm {
    fn sort_counter_prefix(code: &[Instruction], params: u16) -> bool {
        matches!(code, [get, arithmetic, set, ..]
            if get.op == Op::GetGlobal && u16::from(get.a) >= params
                && matches!(arithmetic.op, Op::Add | Op::Sub | Op::Mul)
                && set.op == Op::SetGlobal
                && get.a == arithmetic.a && get.a == set.a && get.c == set.c)
    }
    pub(super) fn sort_leaf_candidate(&self, comparator: Value) -> Option<FunctionId> {
        let Value::Function(function) = comparator else {
            return None;
        };
        let closure = self.functions.get(function.0)?;
        let prototype = &self.programs[closure.program].data.prototypes[closure.prototype];
        if prototype.vararg || prototype.compat_arg {
            return None;
        }
        let code = if Self::sort_counter_prefix(&prototype.code, prototype.params) {
            &prototype.code[3..]
        } else {
            &*prototype.code
        };
        let supported = match code {
            [first, ..] if first.op == Op::Return && first.c <= 1 => true,
            [first, second, ..]
                if matches!(first.op, Op::Load | Op::Move)
                    && (first.op != Op::Move || first.c != OPEN)
                    && second.op == Op::Return
                    && second.c == 1
                    && second.a == first.a =>
            {
                true
            }
            [first, second, third, ..]
                if matches!(
                    first.op,
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                ) && second.op == Op::Bool
                    && third.op == Op::Return
                    && third.c == 1
                    && third.a == second.a =>
            {
                true
            }
            [first, second, ..]
                if matches!(
                    first.op,
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                ) && second.op == Op::Return
                    && second.c == 1
                    && second.a == first.a =>
            {
                true
            }
            _ => false,
        };
        supported.then_some(function)
    }
    fn scalar_leaf(
        &self,
        function: FunctionId,
        call: usize,
        count: usize,
        update: &mut Option<Update>,
    ) -> Option<(Value, usize, u32)> {
        let closure = self.functions.get(function.0)?;
        let loaded = &self.programs[closure.program];
        let prototype = &loaded.data.prototypes[closure.prototype];
        if prototype.vararg || prototype.compat_arg {
            return None;
        }
        let constants = &loaded.constants[closure.prototype];
        let mut changed = None;
        let mut code = &*prototype.code;
        let mut lines = &*prototype.lines;
        if Self::sort_counter_prefix(code, prototype.params) {
            let get = code[0];
            let arithmetic = code[1];
            let key = *constants.get(get.c as usize)?;
            let Value::String(key_id) = key else {
                return None;
            };
            let Value::Number(old) = self.tables[slot_index(closure.env.0)].get(key) else {
                return None;
            };
            let input = |index: u32| {
                if index >= RK {
                    constants.get((index - RK) as usize).copied()
                } else if index == u32::from(get.a) {
                    Some(Value::Number(old))
                } else if index < u32::from(prototype.params) && (index as usize) < count {
                    Some(self.registers[call + 1 + index as usize])
                } else {
                    Some(Value::Nil)
                }
            };
            let (Value::Number(left), Value::Number(right)) =
                (input(u32::from(arithmetic.b))?, input(arithmetic.c)?)
            else {
                return None;
            };
            let number = Self::number_op(arithmetic.op, left, right);
            *update = Some(Update {
                table: closure.env,
                key: key_id,
                number,
            });
            changed = Some((usize::from(get.a), Value::Number(number)));
            code = &code[3..];
            lines = &lines[3..];
        }
        let first = *code.first()?;
        let initial = |index: usize| {
            if let Some((register, value)) = changed
                && index == register
            {
                value
            } else if index < usize::from(prototype.params) && index < count {
                self.registers[call + 1 + index]
            } else {
                Value::Nil
            }
        };
        let operand = |index: u32| {
            if index < RK {
                Some(initial(index as usize))
            } else {
                constants.get((index - RK) as usize).copied()
            }
        };
        if first.op == Op::Return {
            return match first.c {
                0 => Some((Value::Nil, 0, lines[0])),
                1 => Some((initial(usize::from(first.a)), 1, lines[0])),
                _ => None,
            };
        }
        let second = *code.get(1)?;
        if matches!(first.op, Op::Load | Op::Move)
            && second.op == Op::Return
            && second.c == 1
            && second.a == first.a
        {
            return match first.op {
                Op::Load => Some((
                    if first.b == 0 {
                        *constants.get(first.c as usize)?
                    } else {
                        initial(usize::from(first.a))
                    },
                    1,
                    lines[1],
                )),
                Op::Move if first.c != OPEN => Some((initial(usize::from(first.b)), 1, lines[1])),
                _ => None,
            };
        }
        if matches!(
            first.op,
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
        ) {
            let left = operand(u32::from(first.b))?;
            let right = operand(first.c)?;
            let result = if let (Value::Number(x), Value::Number(y)) = (left, right) {
                match first.op {
                    Op::Eq => x == y,
                    Op::Ne => x != y,
                    Op::Lt => x < y,
                    Op::Le => x <= y,
                    Op::Gt => x > y,
                    Op::Ge => x >= y,
                    _ => unreachable!(),
                }
            } else if matches!(first.op, Op::Eq | Op::Ne)
                && !matches!(left, Value::Table(_) | Value::Userdata(_))
                && !matches!(right, Value::Table(_) | Value::Userdata(_))
            {
                (left == right) == (first.op == Op::Eq)
            } else {
                return None;
            };
            let third = *code.get(2)?;
            if second.op == Op::Bool
                && third.op == Op::Return
                && third.c == 1
                && third.a == second.a
            {
                return Some((
                    Value::Boolean(if second.c == 0 { result } else { !result }),
                    1,
                    lines[2],
                ));
            }
            if second.op == Op::Return && second.c == 1 && second.a == first.a {
                return Some((Value::Boolean(result), 1, lines[1]));
            }
        }
        None
    }

    pub(super) fn sort_fast_context(&self) -> bool {
        if self.budget.is_some()
            || self.hooks.mask != 0
            || self.hooks.enter.is_some()
            || self.hooks.tail.is_some()
            || self.hooks.running.is_some()
            || self.hooks.resumed_top.is_some()
            || self.hooks.yield_owner.is_some()
            || !self.hooks.popped.is_empty()
            || !self.hooks.hidden.is_empty()
            || !self.api_calls.is_empty()
            || !self.native_bodies.is_empty()
            || self.native_protected
        {
            return false;
        }
        true
    }
    pub(super) fn try_sort_leaf(
        &mut self,
        function: FunctionId,
        call: usize,
        error_line: &mut Option<u32>,
    ) -> Result<Option<usize>, Error> {
        if !self.sort_fast_context() {
            return Ok(None);
        }
        let mut update = None;
        let Some((value, actual, line)) = self.scalar_leaf(function, call, 2, &mut update) else {
            return Ok(None);
        };
        if update.is_some()
            && (!matches!(self.registers[call + 1], Value::Number(_))
                || !matches!(self.registers[call + 2], Value::Number(_))
                || !matches!(value, Value::Number(_) | Value::Boolean(_) | Value::Nil))
        {
            return Ok(None);
        }
        if self.frames.last().is_some_and(|frame| {
            usize::from(frame.barriers) + usize::from(self.threads[slot_index(self.thread.0)].depth)
                >= 198
        }) || self.threads[slot_index(self.thread.0)].native_bias != 0
            || !self.loads.is_empty()
        {
            self.check_c_call()?;
        }
        self.check_calls()?;
        let ready = if update.is_some() {
            self.numeric_sort_gc_ready()
        } else {
            self.sort_gc_ready()
        };
        if !ready {
            return Ok(None);
        }
        if let Some(update) = update {
            // The key already holds a number. Replacing it neither allocates
            // nor changes any collector edge or invokes __newindex.
            self.tables[slot_index(update.table.0)]
                .set(Value::String(update.key), Value::Number(update.number))?;
        }
        if actual != 0 {
            self.registers[call] = value;
        }
        *error_line = Some(line);
        Ok(Some(actual))
    }
}
