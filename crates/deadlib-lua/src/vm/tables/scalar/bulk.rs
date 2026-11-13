//! Direct partitions for callbacks whose only observable effect is a numeric
//! global update. The array contains no collector edges, the callback cannot
//! allocate or call Lua, and no hooks, API observers, budgets or GC work are due.
//! The counter can therefore be kept locally and published before success/error.
use super::*;

#[derive(Clone, Copy)]
enum Predicate {
    Always(bool),
    Numbers(bool),
}
#[derive(Clone, Copy)]
enum Input {
    Old,
    Number(f64),
}
impl Input {
    fn number(self, old: f64) -> f64 {
        match self {
            Self::Old => old,
            Self::Number(n) => n,
        }
    }
}
struct Counter {
    update: Update,
    op: Op,
    left: Input,
    right: Input,
}
impl Counter {
    fn step(&mut self) {
        let old = self.update.number;
        self.update.number = Vm::number_op(self.op, self.left.number(old), self.right.number(old));
    }
}
struct Plan {
    predicate: Predicate,
    counter: Option<Counter>,
    line: u32,
}
impl Vm {
    fn bulk_sort_plan(&self, comparator: Value) -> Option<Plan> {
        let Value::Function(function) = comparator else {
            return None;
        };
        let closure = self.functions.get(function.0)?;
        let loaded = &self.programs[closure.program];
        let prototype = &loaded.data.prototypes[closure.prototype];
        if prototype.vararg || prototype.compat_arg {
            return None;
        }
        let constants = &loaded.constants[closure.prototype];
        let mut code = &*prototype.code;
        let mut lines = &*prototype.lines;
        let mut counter = None;
        let mut changed = None;
        if Self::sort_counter_prefix(code, prototype.params) {
            let get = code[0];
            let arithmetic = code[1];
            let Value::String(key) = *constants.get(get.c as usize)? else {
                return None;
            };
            let Value::Number(number) =
                self.tables[slot_index(closure.env.0)].get(Value::String(key))
            else {
                return None;
            };
            let input = |index: u32| -> Option<Input> {
                if index == u32::from(get.a) {
                    Some(Input::Old)
                } else if index >= RK
                    && let Value::Number(n) = *constants.get((index - RK) as usize)?
                {
                    Some(Input::Number(n))
                } else {
                    None
                }
            };
            counter = Some(Counter {
                update: Update {
                    table: closure.env,
                    key,
                    number,
                },
                op: arithmetic.op,
                left: input(u32::from(arithmetic.b))?,
                right: input(arithmetic.c)?,
            });
            changed = Some(get.a);
            code = &code[3..];
            lines = &lines[3..];
        }
        let initial = |register: u16| -> Option<bool> {
            if changed.is_some_and(|r| u16::from(r) == register) {
                Some(true)
            } else if register >= prototype.params || register >= 2 {
                Some(false)
            } else {
                None
            }
        };
        let first = *code.first()?;
        if first.op == Op::Return && first.c <= 1 {
            return Some(Plan {
                predicate: Predicate::Always(first.c == 1 && initial(u16::from(first.a))?),
                counter,
                line: lines[0],
            });
        }
        let second = *code.get(1)?;
        if second.op == Op::Return && second.c == 1 && second.a == first.a {
            let value = match first.op {
                Op::Load if first.b == 0 => constants.get(first.c as usize)?.is_truthy(),
                Op::Load => initial(u16::from(first.a))?,
                Op::Move if first.c != OPEN => initial(first.b)?,
                _ => {
                    return self.bulk_numeric_plan(
                        first,
                        second,
                        code,
                        lines,
                        prototype.params,
                        counter,
                    );
                }
            };
            return Some(Plan {
                predicate: Predicate::Always(value),
                counter,
                line: lines[1],
            });
        }
        self.bulk_numeric_plan(first, second, code, lines, prototype.params, counter)
    }
    fn bulk_numeric_plan(
        &self,
        first: Instruction,
        second: Instruction,
        code: &[Instruction],
        lines: &[u32],
        params: u16,
        counter: Option<Counter>,
    ) -> Option<Plan> {
        if params < 2
            || !matches!(first.op, Op::Lt | Op::Gt)
            || !matches!((first.b, first.c), (0, 1) | (1, 0))
        {
            return None;
        }
        let line = if second.op == Op::Return && second.c == 1 && second.a == first.a {
            lines[1]
        } else if second.op == Op::Bool && second.c == 0 {
            let third = *code.get(2)?;
            if third.op != Op::Return || third.c != 1 || third.a != second.a {
                return None;
            }
            lines[2]
        } else {
            return None;
        };
        Some(Plan {
            predicate: Predicate::Numbers((first.op == Op::Gt) ^ (first.b == 1)),
            counter,
            line,
        })
    }
    pub(in crate::vm::tables) fn try_bulk_sort(
        &mut self,
        table: TableId,
        len: usize,
        comparator: Value,
        error_line: &mut Option<u32>,
    ) -> Result<bool, Error> {
        if !self.sort_fast_context() || !self.numeric_sort_gc_ready() {
            return Ok(false);
        }
        let Some(mut plan) = self.bulk_sort_plan(comparator) else {
            return Ok(false);
        };
        let numbers = matches!(plan.predicate, Predicate::Numbers(_));
        if self.tables[slot_index(table.0)]
            .scalar_sort_values(len, numbers)
            .is_none()
        {
            return Ok(false);
        }
        if len > 1 {
            if self.frames.last().is_some_and(|frame| {
                usize::from(frame.barriers)
                    + usize::from(self.threads[slot_index(self.thread.0)].depth)
                    >= 198
            }) || self.threads[slot_index(self.thread.0)].native_bias != 0
                || !self.loads.is_empty()
            {
                self.check_c_call()?;
            }
            self.check_calls()?;
            if !self.numeric_sort_gc_ready() {
                return Ok(false);
            }
        }
        let before = self.raw_index(table, 0);
        let after = self.raw_index(table, len as i64 + 1);
        let values = self.tables[slot_index(table.0)]
            .scalar_sort_values(len, numbers)
            .expect("validated primitive array");
        let mut compared = false;
        let result = crate::table::sort::scalar(values, after, before, |left, right| {
            compared = true;
            if let Some(counter) = &mut plan.counter {
                counter.step();
            }
            match plan.predicate {
                Predicate::Always(value) => value,
                Predicate::Numbers(reverse) => {
                    let (Value::Number(left), Value::Number(right)) = (left, right) else {
                        unreachable!("strict numeric predicate stays within its numeric array");
                    };
                    if reverse { left > right } else { left < right }
                }
            }
        });
        if compared {
            *error_line = Some(plan.line);
        }
        if let Some(counter) = plan.counter {
            let update = counter.update;
            self.tables[slot_index(update.table.0)]
                .set(Value::String(update.key), Value::Number(update.number))?;
        }
        result.map_err(|error| match error {
            crate::table::sort::Failure::Order => failure("invalid order function for sorting"),
            crate::table::sort::Failure::Partitions => {
                Error::new(ErrorKind::Limit, 0, "sort partition limit exceeded")
            }
        })?;
        Ok(true)
    }
}
