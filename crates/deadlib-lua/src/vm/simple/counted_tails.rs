//! Skip self-tail calls that only advance an exact integer argument.
use super::{COMPARE_BRANCH, Cell, Context, FunctionId, OPEN, Op, RK, Value};
use crate::vm::reductions::{EXACT_INTEGER, integer};

const SCRATCH_LIMIT: usize = 32;

#[derive(Clone, Copy)]
enum Binding {
    Unset,
    Argument(usize),
    Fixed(Value),
    Step(i128),
}

impl Binding {
    fn fixed(self, args: &[Value], counter: usize) -> Option<Value> {
        match self {
            Self::Fixed(value) => Some(value),
            Self::Argument(index) if index != counter => Some(args[index]),
            _ => None,
        }
    }
}

fn tail_step(
    cx: &Context<'_>,
    start: usize,
    args: &[Value],
    counter: usize,
    function: FunctionId,
) -> Option<i128> {
    let mut bindings = [Binding::Unset; SCRATCH_LIMIT];
    for (index, binding) in bindings[..args.len()].iter_mut().enumerate() {
        *binding = Binding::Argument(index);
    }
    for i in cx.code.get(start..)?.iter().take(SCRATCH_LIMIT) {
        let dst = usize::from(i.a);
        if i.op == Op::TailCall {
            if usize::from(i.b) != args.len()
                || bindings.get(dst)?.fixed(args, counter)? != Value::Function(function)
            {
                return None;
            }
            let passed = bindings.get(dst + 1..dst + 1 + args.len())?;
            let Binding::Step(step) = passed[counter] else {
                return None;
            };
            return passed
                .iter()
                .enumerate()
                .all(|(index, value)| {
                    index == counter || matches!(value, Binding::Argument(arg) if *arg == index)
                })
                .then_some(step);
        }
        if i.op == Op::Close && cx.closed {
            continue;
        }
        if dst < args.len() {
            return None;
        }
        let operand = |id: u32| {
            if id < RK {
                bindings.get(id as usize).copied()
            } else {
                cx.constants
                    .get((id - RK) as usize)
                    .copied()
                    .map(Binding::Fixed)
            }
        };
        let binding = match i.op {
            Op::Move if i.c != OPEN => *bindings.get(usize::from(i.b))?,
            Op::Load if i.b == 0 => Binding::Fixed(*cx.constants.get(i.c as usize)?),
            Op::GetGlobal => {
                let key = *cx.constants.get(i.c as usize)?;
                let value = cx.tables.get(cx.env.0)?.get(key);
                if value != Value::Function(function) {
                    return None;
                }
                Binding::Fixed(value)
            }
            Op::GetUpvalue => {
                let slot = *cx.upvalue_ids.get(i.c as usize)? as usize;
                let Cell::Closed(value) = cx.upvalues[slot] else {
                    return None;
                };
                Binding::Fixed(value)
            }
            Op::GetTable => {
                let Value::Table(id) = bindings.get(usize::from(i.b))?.fixed(args, counter)? else {
                    return None;
                };
                let key = operand(i.c)?.fixed(args, counter)?;
                let value = cx.tables.get(id.0)?.get(key);
                if value != Value::Function(function) {
                    return None;
                }
                Binding::Fixed(value)
            }
            Op::Add | Op::Sub => {
                let left = operand(u32::from(i.b))?;
                let right = operand(i.c)?;
                let step = if matches!(left, Binding::Argument(arg) if arg == counter) {
                    let step = integer(right.fixed(args, counter)?)?;
                    if i.op == Op::Sub { -step } else { step }
                } else if i.op == Op::Add
                    && matches!(right, Binding::Argument(arg) if arg == counter)
                {
                    integer(left.fixed(args, counter)?)?
                } else {
                    return None;
                };
                Binding::Step(step)
            }
            _ => return None,
        };
        *bindings.get_mut(dst)? = binding;
    }
    None
}

fn iterations(op: Op, truth: bool, initial: i128, bound: i128, step: i128) -> Option<i128> {
    let gap = bound - initial;
    let count = match (op, truth) {
        (Op::Lt, true) | (Op::Ge, false) if step > 0 => (gap - 1) / step + 1,
        (Op::Le, true) | (Op::Gt, false) if step > 0 => gap / step + 1,
        (Op::Gt, true) | (Op::Le, false) if step < 0 => (-gap - 1) / -step + 1,
        (Op::Ge, true) | (Op::Lt, false) if step < 0 => -gap / -step + 1,
        (Op::Ne, true) | (Op::Eq, false) if step != 0 && gap % step == 0 => gap / step,
        _ => return None,
    };
    (count >= 2).then_some(count)
}

#[inline(never)]
pub(super) fn advance(cx: &mut Context<'_>, function: FunctionId, params: usize) -> Option<u32> {
    let [test, branch, ..] = cx.code else {
        return None;
    };
    if params == 0
        || params >= SCRATCH_LIMIT
        || usize::from(test.a) < params
        || u32::from(test.b) == test.c
        || !matches!(branch.op, Op::JumpFalse | Op::JumpTrue)
        || branch.a != test.a
        || branch.b != COMPARE_BRANCH
    {
        return None;
    }
    let reversed = match test.op {
        Op::Eq | Op::Ne => test.op,
        Op::Lt => Op::Gt,
        Op::Le => Op::Ge,
        Op::Gt => Op::Lt,
        Op::Ge => Op::Le,
        _ => return None,
    };
    let args = cx.registers.get(cx.base..cx.base + params)?;
    let read = |id: u32| {
        if id < RK {
            args.get(id as usize)
        } else {
            cx.constants.get((id - RK) as usize)
        }
    };
    let left = integer(*read(u32::from(test.b))?)?;
    let right = integer(*read(test.c)?)?;
    let truth = match test.op {
        Op::Eq => left == right,
        Op::Ne => left != right,
        Op::Lt => left < right,
        Op::Le => left <= right,
        Op::Gt => left > right,
        Op::Ge => left >= right,
        _ => unreachable!(),
    };
    let start = if truth == (branch.op == Op::JumpTrue) {
        branch.c as usize
    } else {
        2
    };
    if start < 2 {
        return None;
    }
    for (counter, initial, bound, op) in [
        (usize::from(test.b), left, right, test.op),
        (test.c as usize, right, left, reversed),
    ] {
        if counter >= params {
            continue;
        }
        let Some(step) = tail_step(cx, start, args, counter, function) else {
            continue;
        };
        let count = iterations(op, truth, initial, bound, step)?;
        let last = initial + count * step;
        if last.abs() > EXACT_INTEGER {
            return None;
        }
        // The current frame has already been restarted. Leave the terminal
        // activation to the dispatcher so it publishes the ordinary frame.
        cx.registers[cx.base + counter] = Value::Number(last as f64);
        return Some(u32::try_from(count).unwrap_or(u32::MAX));
    }
    None
}
