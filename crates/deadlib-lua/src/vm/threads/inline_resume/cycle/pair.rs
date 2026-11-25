//! Decode the register-only caller loop around two parked numeric threads.
use super::*;

const LOOP_LIMIT: usize = 48;

#[derive(Clone, Copy)]
enum ComparisonConsumer {
    None,
    Bool { dst: usize, invert: bool },
    Branch { target: usize, on: bool },
}

#[derive(Clone, Copy)]
enum Step {
    Nop,
    Move {
        dst: usize,
        src: usize,
    },
    Load {
        dst: usize,
        value: Value,
    },
    Arithmetic(Numeric),
    Neg {
        dst: usize,
        src: usize,
    },
    Compare(Numeric, ComparisonConsumer),
    Bool {
        dst: usize,
        src: usize,
        invert: bool,
    },
    Branch {
        src: usize,
        target: usize,
        on: bool,
    },
    Jump(usize),
    Assert {
        slot: usize,
        wanted: usize,
    },
    Resume(usize),
    PrepareResume {
        slot: usize,
        callee: usize,
        thread: usize,
    },
}

fn builtin_assert(cx: &Context<'_, '_>, constant: u32) -> Option<Value> {
    let Value::String(name) = cx.constants[constant as usize] else {
        return None;
    };
    let value = cx.tables[slot_index(cx.env.0)].get_string(name);
    (value == Value::Native(0)).then_some(value)
}

impl Step {
    fn decode(cx: &Context<'_, '_>, pc: usize) -> Option<Self> {
        let i = cx.code[pc];
        let dst = cx.base + usize::from(i.a);
        let src = cx.base + usize::from(i.b);
        let step = match i.op {
            Op::Move if i.c != OPEN => {
                if let Some([argument, call]) = cx.code.get(pc + 1..pc + 3)
                    && argument.op == Op::Move
                    && argument.c != OPEN
                    && usize::from(argument.a) == usize::from(i.a) + 1
                    && call.op == Op::Call
                    && call.a == i.a
                    && call.b == 1
                    && call.c == 2
                {
                    Self::PrepareResume {
                        slot: dst,
                        callee: src,
                        thread: cx.base + usize::from(argument.b),
                    }
                } else {
                    Self::Move { dst, src }
                }
            }
            Op::Load if i.b == 0 => Self::Load {
                dst,
                value: cx.constants[i.c as usize],
            },
            Op::Load => Self::Nop,
            Op::GetGlobal => Self::Load {
                dst,
                value: builtin_assert(cx, i.c)?,
            },
            Op::Add | Op::Sub => Self::Arithmetic(Numeric::decode(cx, i)?),
            Op::Neg => Self::Neg { dst, src },
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                let following = match cx.code.get(pc + 1) {
                    Some(next) if next.op == Op::Bool => ComparisonConsumer::Bool {
                        dst: cx.base + usize::from(next.a),
                        invert: next.c != 0,
                    },
                    Some(next)
                        if matches!(next.op, Op::JumpFalse | Op::JumpTrue)
                            && next.b == COMPARE_BRANCH =>
                    {
                        ComparisonConsumer::Branch {
                            target: next.c as usize,
                            on: next.op == Op::JumpTrue,
                        }
                    }
                    _ => ComparisonConsumer::None,
                };
                Self::Compare(Numeric::decode(cx, i)?, following)
            }
            Op::Bool => Self::Bool {
                dst,
                src,
                invert: i.c != 0,
            },
            Op::Jump if i.b != NUMERIC_WHILE => Self::Jump(i.c as usize),
            Op::JumpFalse | Op::JumpTrue => Self::Branch {
                src: dst,
                target: i.c as usize,
                on: i.op == Op::JumpTrue,
            },
            Op::Close if cx.closed => Self::Nop,
            Op::Call if i.b == 1 && i.c <= 1 => Self::Assert {
                slot: dst,
                wanted: i.c as usize,
            },
            Op::Call if i.b == 1 && i.c == 2 => Self::Resume(dst),
            _ => return None,
        };
        Some(step)
    }
}

#[inline]
fn resume(
    cx: &mut Context<'_, '_>,
    slot: usize,
    first: &mut IndexResume,
    second: &mut IndexResume,
) -> bool {
    if cx.registers[slot] != Value::Native(CO_RESUME) {
        return false;
    }
    let Value::Thread(thread) = cx.registers[slot + 1] else {
        return false;
    };
    if thread == first.thread {
        first.resume(cx, slot)
    } else {
        thread == second.thread && second.resume(cx, slot)
    }
}

#[inline(never)]
pub(in crate::vm::threads::inline_resume) fn run(
    cx: &mut Context<'_, '_>,
    mut pc: usize,
    top: &mut usize,
    first: &mut IndexResume,
    second: &mut IndexResume,
) -> Option<(usize, bool)> {
    let (offset, edge) = cx.code[pc..]
        .iter()
        .take(LOOP_LIMIT)
        .enumerate()
        .find(|(_, i)| i.op == Op::Jump && i.b == 0 && (i.c as usize) < pc)?;
    let head = edge.c as usize;
    let code = &cx.code[head..=pc + offset];
    if code.len() > LOOP_LIMIT
        || code
            .iter()
            .filter(|i| i.op == Op::Call && i.b == 1 && i.c == 2)
            .take(2)
            .count()
            != 2
    {
        return None;
    }
    let mut steps = [Step::Nop; LOOP_LIMIT];
    let steps = &mut steps[..code.len()];
    for (index, step) in steps.iter_mut().enumerate() {
        *step = Step::decode(cx, head + index)?;
    }
    // Decoding admits only register operations and guarded native calls.
    // Nothing can replace the cached globals or observe a parked register
    // until the VM writes both threads back at the end of the fast path.
    let mut resumed = false;
    while let Some(&step) = steps.get(pc.wrapping_sub(head)) {
        let mut next = pc + 1;
        match step {
            Step::Nop => {}
            Step::Move { dst, src } => cx.registers[dst] = cx.registers[src],
            Step::Load { dst, value } => cx.registers[dst] = value,
            Step::Arithmetic(op) => {
                if op.arithmetic(cx.registers).is_none() {
                    break;
                }
            }
            Step::Neg { dst, src } => {
                let Value::Number(number) = cx.registers[src] else {
                    break;
                };
                cx.registers[dst] = Value::Number(-number);
            }
            Step::Compare(op, following) => {
                let Some((left, right)) = op.operands(cx.registers) else {
                    break;
                };
                let result = match op.op {
                    Op::Eq => left == right,
                    Op::Ne => left != right,
                    Op::Lt => left < right,
                    Op::Le => left <= right,
                    Op::Gt => left > right,
                    _ => left >= right,
                };
                cx.registers[op.dst] = Value::Boolean(result);
                match following {
                    ComparisonConsumer::None => {}
                    ComparisonConsumer::Bool { dst, invert } => {
                        cx.registers[dst] = Value::Boolean(result ^ invert);
                        next += 1;
                    }
                    ComparisonConsumer::Branch { target, on } => {
                        next = if result == on { target } else { next + 1 };
                    }
                }
            }
            Step::Bool { dst, src, invert } => {
                let value = cx.registers[src];
                cx.registers[dst] = if invert {
                    Value::Boolean(!value.is_truthy())
                } else {
                    value
                };
            }
            Step::Branch { src, target, on } => {
                if cx.registers[src].is_truthy() == on {
                    next = target;
                }
            }
            Step::Jump(target) => next = target,
            Step::Assert { slot, wanted } => {
                let value = cx.registers[slot + 1];
                if cx.registers[slot] != Value::Native(0) || !value.is_truthy() {
                    break;
                }
                cx.registers[slot] = value;
                *top = slot + wanted;
            }
            Step::PrepareResume {
                slot,
                callee,
                thread,
            } => {
                cx.registers[slot] = cx.registers[callee];
                cx.registers[slot + 1] = cx.registers[thread];
                if !resume(cx, slot, first, second) {
                    // The two argument moves are already visible to the VM.
                    pc += 2;
                    break;
                }
                *top = slot + 2;
                resumed = true;
                next += 2;
            }
            Step::Resume(slot) => {
                if !resume(cx, slot, first, second) {
                    break;
                }
                *top = slot + 2;
                resumed = true;
            }
        }
        pc = next;
    }
    Some((pc, resumed))
}
