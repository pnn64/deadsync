use super::{
    Cell, FrameKind, Instruction, Op, RK, Reduction, Value, Vm, integer, integer_loop_bounds,
    slot_index,
};

impl Vm {
    #[inline(never)]
    pub(super) fn reduce_indexed_comparisons(
        &mut self,
        base: usize,
        control: usize,
        body: &[Instruction],
        program: usize,
        prototype_index: usize,
    ) -> Option<Reduction> {
        let [load, get, compare, boolean, call, tail @ ..] = body else {
            return None;
        };
        let index = control + 3;
        if get.op != Op::GetTable
            || get.c as usize != index
            || usize::from(get.b) >= control
            || !matches!(
                compare.op,
                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
            )
            || boolean.op != Op::Bool
            || boolean.b != u16::from(compare.a)
            || u16::from(boolean.a) != u16::from(call.a) + 1
            || call.op != Op::Call
            || call.b != 1
            || call.c > 1
            || call.a != load.a
            || call.a == get.a
            || call.a == compare.a
            || usize::from(call.a.min(get.a).min(compare.a)) <= index
            || !(tail.is_empty()
                || matches!(tail, [close] if close.op == Op::Close && usize::from(close.a) == index))
        {
            return None;
        }

        let indexed_on_left = compare.b == u16::from(get.a);
        let operand = if indexed_on_left {
            compare.c
        } else if compare.c == u32::from(get.a) {
            u32::from(compare.b)
        } else {
            return None;
        };
        let constants = self.programs[program].bound_constants(prototype_index);
        let expected = if operand as usize == index {
            None
        } else {
            let value = if operand >= RK {
                *constants.get((operand - RK) as usize)?
            } else if (operand as usize) < control {
                self.registers[base + operand as usize]
            } else {
                return None;
            };
            let Value::Number(value) = value else {
                return None;
            };
            Some(value)
        };

        let internal = base + control;
        let previous = integer(self.registers[internal])?;
        let limit = integer(self.registers[internal + 1])?;
        let step = integer(self.registers[internal + 2])?;
        let (first, count) = integer_loop_bounds(previous, limit, step)?;
        let Value::Table(id) = self.registers[base + usize::from(get.b)] else {
            return None;
        };
        let table = &self.tables[slot_index(id.0)];
        if table.metatable().is_some()
            || !self.open_upvalues.is_empty()
            || !self.sort_fast_context()
        {
            return None;
        }
        let assert = match load.op {
            Op::Move if usize::from(load.b) < control => self.registers[base + usize::from(load.b)],
            Op::GetGlobal | Op::GetUpvalue => {
                let FrameKind::Lua(function) = self.frames.last()?.function else {
                    return None;
                };
                let closure = &self.functions[slot_index(function.0)];
                if load.op == Op::GetGlobal {
                    let Value::String(name) = *constants.get(load.c as usize)? else {
                        return None;
                    };
                    self.tables[slot_index(closure.env.0)].get_string(name)
                } else {
                    let id = *closure.upvalues.get(load.c as usize)?;
                    let Cell::Closed(value) = self.upvalues[id as usize] else {
                        return None;
                    };
                    value
                }
            }
            _ => return None,
        };
        if !matches!(assert, Value::Native(0)) {
            return None;
        }
        // A successful native assertion needs no extra registers. A prefix
        // made of scalar reads can leave an incremental GC cycle parked.
        if self.frames.len() == self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
            || !self.gc_checkpoint_idle()
        {
            return Some(Reduction::Retry);
        }

        // Dispatch the last body so scratch registers and the stack top are
        // written normally. A prefix we cannot prove leaves the VM untouched.
        let mut current = first as i64;
        for _ in 1..count as u64 {
            let Value::Number(value) = table.get_integer(current) else {
                return None;
            };
            let expected = expected.unwrap_or(current as f64);
            let (left, right) = if indexed_on_left {
                (value, expected)
            } else {
                (expected, value)
            };
            let passes = match compare.op {
                Op::Eq => left == right,
                Op::Ne => left != right,
                Op::Lt => left < right,
                Op::Le => left <= right,
                Op::Gt => left > right,
                Op::Ge => left >= right,
                _ => unreachable!(),
            };
            if passes == (boolean.c != 0) {
                return None;
            }
            current += step as i64;
        }
        self.gc.pending = false;
        self.registers[internal] = Value::Number((current - step as i64) as f64);
        Some(Reduction::Applied(
            (count as u64 - 1) * (body.len() as u64 + 1),
        ))
    }
}
