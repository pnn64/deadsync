//! Execute a consumer's straight-line work between individual numeric yields.
use super::*;
use crate::bytecode::COMPARE_BRANCH;

pub(super) mod pair;

type Context<'a, 'threads> = simple::Context<'a, &'threads mut Arena<Thread>>;

#[derive(Clone, Copy)]
enum Operand {
    Register(usize),
    Number(f64),
}
impl Operand {
    fn decode(cx: &Context<'_, '_>, operand: u32) -> Option<Self> {
        if operand < RK {
            return Some(Self::Register(cx.base + operand as usize));
        }
        let Value::Number(number) = cx.constants[(operand - RK) as usize] else {
            return None;
        };
        Some(Self::Number(number))
    }

    #[inline]
    fn number(self, registers: &[Value]) -> Option<f64> {
        match self {
            Self::Register(register) => match registers[register] {
                Value::Number(number) => Some(number),
                _ => None,
            },
            Self::Number(number) => Some(number),
        }
    }
}

#[derive(Clone, Copy)]
struct Numeric {
    dst: usize,
    left: Operand,
    right: Operand,
    op: Op,
}
impl Numeric {
    fn decode(cx: &Context<'_, '_>, instruction: Instruction) -> Option<Self> {
        Some(Self {
            dst: cx.base + usize::from(instruction.a),
            left: Operand::decode(cx, u32::from(instruction.b))?,
            right: Operand::decode(cx, instruction.c)?,
            op: instruction.op,
        })
    }

    #[inline]
    fn operands(self, registers: &[Value]) -> Option<(f64, f64)> {
        Some((self.left.number(registers)?, self.right.number(registers)?))
    }

    #[inline]
    fn arithmetic(self, registers: &mut [Value]) -> Option<()> {
        let (left, right) = self.operands(registers)?;
        let value = if self.op == Op::Add {
            left + right
        } else {
            left - right
        };
        registers[self.dst] = Value::Number(value);
        Some(())
    }
}

#[inline(never)]
pub(super) fn run<'a, 'threads>(
    cx: &mut Context<'a, 'threads>,
    pc: usize,
    top: &mut usize,
    thread: ThreadId,
    mut resume: impl FnMut(&mut Context<'a, 'threads>, usize, usize) -> bool,
) -> Option<(usize, bool)> {
    let resume_pc = pc.checked_sub(1)?;
    let [lookup, status, assertion, tail @ ..] = cx.code.get(pc..)? else {
        return None;
    };
    if lookup.op != Op::GetGlobal
        || status.op != Op::Move
        || status.c != 0
        || status.b == u16::from(lookup.a)
        || u16::from(status.a) != u16::from(lookup.a) + 1
        || assertion.op != Op::Call
        || assertion.a != lookup.a
        || assertion.b != 1
        || assertion.c > 1
    {
        return None;
    }
    let Value::String(name) = cx.constants[lookup.c as usize] else {
        return None;
    };
    if cx.tables[slot_index(cx.env.0)].get_string(name) != Value::Native(0) {
        return None;
    }
    let count = tail
        .iter()
        .take(5)
        .take_while(|instruction| matches!(instruction.op, Op::Add | Op::Sub))
        .count();
    let edge = *tail.get(count)?;
    if !(1..=4).contains(&count) || edge.op != Op::Jump || edge.b != 0 {
        return None;
    }
    let head = edge.c as usize;
    let [test, leave, increment, callee, argument, call] = cx.code.get(head..=resume_pc)? else {
        return None;
    };
    if !matches!(test.op, Op::Lt | Op::Le | Op::Gt | Op::Ge)
        || leave.op != Op::JumpFalse
        || leave.a != test.a
        || leave.b != COMPARE_BRANCH
        || !matches!(increment.op, Op::Add | Op::Sub)
        || callee.op != Op::Move
        || callee.c != 0
        || callee.a != call.a
        || argument.op != Op::Move
        || argument.c != 0
        || u16::from(argument.a) != u16::from(call.a) + 1
        || call.op != Op::Call
        || call.b != 1
        || !(2..=5).contains(&call.c)
        || status.b != u16::from(call.a)
    {
        return None;
    }
    let test = Numeric::decode(cx, *test)?;
    let increment = Numeric::decode(cx, *increment)?;
    let mut arithmetic = [increment; 4];
    for (slot, instruction) in arithmetic.iter_mut().zip(&tail[..count]) {
        *slot = Numeric::decode(cx, *instruction)?;
    }
    let linked_additions = count == 3
        && arithmetic[..3].iter().all(|add| add.op == Op::Add)
        && arithmetic[..3].windows(2).all(
            |pair| matches!(pair[1].left, Operand::Register(register) if register == pair[0].dst),
        );
    let base = cx.base;
    let assert_slot = base + usize::from(assertion.a);
    let resume_slot = base + usize::from(call.a);
    let callee = base + usize::from(callee.b);
    let argument = base + usize::from(argument.b);
    let wanted = call.c as usize;
    let mut resumed = false;

    loop {
        // The environment cannot change while this loop executes. On a
        // failed assertion or numeric guard, resume at the original opcode.
        let status = cx.registers[resume_slot];
        if !status.is_truthy() {
            return Some((pc, resumed));
        }
        cx.registers[assert_slot] = status;
        cx.registers[assert_slot + 1] = status;
        *top = assert_slot + assertion.c as usize;
        if linked_additions {
            let Some(mut sum) = arithmetic[0].left.number(cx.registers) else {
                return Some((pc + 3, resumed));
            };
            // Keep the accumulator in numeric form and publish every write
            // so a failed guard can resume at the exact addition it reached.
            for (index, add) in arithmetic[..3].iter().enumerate() {
                let Some(next) = add.right.number(cx.registers) else {
                    return Some((pc + 3 + index, resumed));
                };
                sum += next;
                cx.registers[add.dst] = Value::Number(sum);
            }
        } else {
            for (index, add) in arithmetic[..count].iter().enumerate() {
                if add.arithmetic(cx.registers).is_none() {
                    return Some((pc + 3 + index, resumed));
                }
            }
        }
        let Some((left, right)) = test.operands(cx.registers) else {
            return Some((head, resumed));
        };
        let keep_going = match test.op {
            Op::Lt => left < right,
            Op::Le => left <= right,
            Op::Gt => left > right,
            _ => left >= right,
        };
        cx.registers[test.dst] = Value::Boolean(keep_going);
        if !keep_going {
            return Some((leave.c as usize, resumed));
        }
        if increment.arithmetic(cx.registers).is_none() {
            return Some((head + 2, resumed));
        }
        cx.registers[resume_slot] = cx.registers[callee];
        cx.registers[resume_slot + 1] = cx.registers[argument];
        // Publish one resume's results before running its consumer again.
        if cx.registers[resume_slot] != Value::Native(CO_RESUME)
            || cx.registers[resume_slot + 1] != Value::Thread(thread)
            || !resume(cx, resume_slot, wanted)
        {
            return Some((resume_pc, resumed));
        }
        *top = resume_slot + wanted;
        resumed = true;
    }
}
