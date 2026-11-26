//! Numeric Lua functions with one or two straight-line results.

use super::{FunctionId, Value, Vm, slot_index};
use crate::bytecode::{Instruction, Op, PrototypeBody, RK};

#[derive(Clone, Copy)]
pub(super) enum Input {
    Argument(usize),
    Constant(f64),
}

#[derive(Clone, Copy)]
pub(super) enum Expression {
    Value(Input),
    Negate(Input),
    Binary(Op, Input, Input),
}

impl Expression {
    fn evaluate(self, arguments: &[Value]) -> Option<f64> {
        self.evaluate_with(|input| match input {
            Input::Constant(value) => Some(value),
            Input::Argument(index) => match *arguments.get(index)? {
                Value::Number(value) => Some(value),
                _ => None,
            },
        })
    }

    pub(super) fn evaluate_with(
        self,
        mut operand: impl FnMut(Input) -> Option<f64>,
    ) -> Option<f64> {
        match self {
            Self::Value(input) => operand(input),
            Self::Negate(input) => Some(-operand(input)?),
            Self::Binary(op, left, right) => {
                Some(Vm::number_op(op, operand(left)?, operand(right)?))
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Leaf {
    pub registers: usize,
    pub first: Expression,
    pub second: Option<Expression>,
    line: u32,
}

/// One live closure's decoded body. The generational ID includes its identity
/// across collections, and a cached body carries no references to Lua objects.
#[derive(Default)]
pub(super) struct Cache(Option<(FunctionId, Option<Leaf>)>);

impl Cache {
    fn get(&mut self, vm: &Vm, function: FunctionId) -> Option<Leaf> {
        if let Some((cached, leaf)) = self.0
            && cached == function
        {
            return leaf;
        }
        let leaf = vm.numeric_leaf(function);
        self.0 = Some((function, leaf));
        leaf
    }
}

impl Vm {
    pub(super) fn numeric_leaf(&self, function: FunctionId) -> Option<Leaf> {
        let closure = &self.functions[slot_index(function.0)];
        let program = &self.programs[closure.program];
        let prototype = &program.data.prototypes[closure.prototype];
        let PrototypeBody {
            code,
            params,
            registers,
            vararg,
            compat_arg,
            ..
        } = &**prototype;
        if *vararg || *compat_arg {
            return None;
        }
        let constants = program.bound_constants(closure.prototype);
        let decode = |instruction: Instruction, written: Option<u8>| {
            let operand = |index| {
                if index < u32::from(*params) {
                    (written.map(u32::from) != Some(index))
                        .then_some(Input::Argument(index as usize))
                } else if index >= RK {
                    let Value::Number(number) = constants[(index - RK) as usize] else {
                        return None;
                    };
                    Some(Input::Constant(number))
                } else {
                    None
                }
            };
            let left = u32::from(instruction.b);
            let right = instruction.c;
            Some(match instruction.op {
                Op::Load if instruction.b == 0 => {
                    let Value::Number(value) = constants[right as usize] else {
                        return None;
                    };
                    Expression::Value(Input::Constant(value))
                }
                Op::Move if right == 0 => Expression::Value(operand(left)?),
                Op::Neg => Expression::Negate(operand(left)?),
                op @ (Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow) => {
                    Expression::Binary(op, operand(left)?, operand(right)?)
                }
                _ => return None,
            })
        };
        let first = code[0];
        let (first, second, return_pc) = if first.op == Op::Return {
            if !matches!(first.c, 1 | 2) || u32::from(first.a) + first.c > u32::from(*params) {
                return None;
            }
            let argument = |index| Expression::Value(Input::Argument(index));
            let start = usize::from(first.a);
            (
                argument(start),
                (first.c == 2).then(|| argument(start + 1)),
                0,
            )
        } else {
            let expression = decode(first, None)?;
            let next = *code.get(1)?;
            if next.op == Op::Return && next.c == 1 && next.a == first.a {
                (expression, None, 1)
            } else {
                let ret = *code.get(2)?;
                if ret.op != Op::Return
                    || ret.c != 2
                    || ret.a != first.a
                    || u16::from(next.a) != u16::from(first.a) + 1
                {
                    return None;
                }
                (expression, Some(decode(next, Some(first.a))?), 2)
            }
        };
        Some(Leaf {
            registers: usize::from(*registers),
            first,
            second,
            line: prototype.instruction_line(return_pc),
        })
    }

    pub(super) fn has_numeric_call_space(&self, registers: usize) -> bool {
        self.frames.len() < self.frames.capacity()
            && self.frames.len() + 1 < usize::from(self.call_slots)
            && registers <= self.registers.capacity() - self.registers.len()
    }

    pub(super) fn numeric_call_ready(&mut self, registers: usize) -> bool {
        self.has_numeric_call_space(registers)
            && self.open_upvalues.is_empty()
            && self.sort_fast_context()
            && self.sort_gc_ready()
    }

    pub(super) fn try_numeric_leaf_call(
        &mut self,
        cache: &mut Cache,
        checkpoint_ready: bool,
        call: usize,
        count: usize,
        wanted: usize,
        error_line: &mut u32,
    ) -> bool {
        let Value::Function(function) = self.registers[call] else {
            return false;
        };
        let Some(leaf) = cache.get(self, function) else {
            return false;
        };
        let arguments = &self.registers[call + 1..call + 1 + count];
        let Some(first) = leaf.first.evaluate(arguments) else {
            return false;
        };
        // Even a discarded result can invoke a metamethod or fail. Only
        // publish the call after checking all of the callee's operations.
        let second = if let Some(expression) = leaf.second {
            let Some(second) = expression.evaluate(arguments) else {
                return false;
            };
            Value::Number(second)
        } else {
            Value::Nil
        };
        let (registers, line) = (leaf.registers, leaf.line);
        let ready = if checkpoint_ready {
            self.has_numeric_call_space(registers)
        } else {
            self.numeric_call_ready(registers)
        };
        if !ready {
            return false;
        }
        if wanted != 0 {
            self.registers[call] = Value::Number(first);
        }
        if wanted == 2 {
            self.registers[call + 1] = second;
        }
        self.top = call + wanted;
        *error_line = line;
        true
    }
}
