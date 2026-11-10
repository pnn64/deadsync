//! Scalar sort callbacks with the same type, hook, budget and GC boundaries.
//!
//! Unsupported bodies or active observers use ordinary frame dispatch. The
//! comparator stays in its native argument slot; cached eligibility never roots
//! an old comparator or replaces the live function's identity check.
use super::*;
use crate::bytecode::{OPEN, Op, RK};
impl Vm {
    pub(super) fn sort_leaf_candidate(&self, comparator: Value) -> Option<FunctionId> {
        let Value::Function(function) = comparator else {
            return None;
        };
        let closure = self.functions.get(function.0)?;
        let prototype = &self.programs[closure.program].data.prototypes[closure.prototype];
        if prototype.vararg || prototype.compat_arg {
            return None;
        }
        let code = &*prototype.code;
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
    ) -> Option<(Value, usize, u32)> {
        let closure = self.functions.get(function.0)?;
        let loaded = &self.programs[closure.program];
        let prototype = &loaded.data.prototypes[closure.prototype];
        if prototype.vararg || prototype.compat_arg {
            return None;
        }
        let first = *prototype.code.first()?;
        let initial = |index: usize| {
            if index < usize::from(prototype.params) && index < count {
                self.registers[call + 1 + index]
            } else {
                Value::Nil
            }
        };
        let constants = &loaded.constants[closure.prototype];
        let operand = |index: u32| {
            if index < RK {
                Some(initial(index as usize))
            } else {
                constants.get((index - RK) as usize).copied()
            }
        };
        if first.op == Op::Return {
            return match first.c {
                0 => Some((Value::Nil, 0, prototype.lines[0])),
                1 => Some((initial(usize::from(first.a)), 1, prototype.lines[0])),
                _ => None,
            };
        }
        let second = *prototype.code.get(1)?;
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
                    prototype.lines[1],
                )),
                Op::Move if first.c != OPEN => {
                    Some((initial(usize::from(first.b)), 1, prototype.lines[1]))
                }
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
            let third = *prototype.code.get(2)?;
            if second.op == Op::Bool
                && third.op == Op::Return
                && third.c == 1
                && third.a == second.a
            {
                return Some((
                    Value::Boolean(if second.c == 0 { result } else { !result }),
                    1,
                    prototype.lines[2],
                ));
            }
            if second.op == Op::Return && second.c == 1 && second.a == first.a {
                return Some((Value::Boolean(result), 1, prototype.lines[1]));
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
        let Some((value, actual, line)) = self.scalar_leaf(function, call, 2) else {
            return Ok(None);
        };
        if self.frames.last().is_some_and(|frame| {
            usize::from(frame.barriers) + usize::from(self.threads[slot_index(self.thread.0)].depth)
                >= 198
        }) || self.threads[slot_index(self.thread.0)].native_bias != 0
            || !self.loads.is_empty()
        {
            self.check_c_call()?;
        }
        self.check_calls()?;
        if !self.sort_gc_ready() {
            return Ok(None);
        }
        if actual != 0 {
            self.registers[call] = value;
        }
        *error_line = Some(line);
        Ok(Some(actual))
    }
}
