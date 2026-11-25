//! Numeric Lua functions whose result fits in a single straight-line operation.

use super::{FunctionId, Value, Vm, slot_index};
use crate::bytecode::{Op, Prototype, RK};

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

#[derive(Clone, Copy)]
pub(super) struct Leaf {
    pub registers: usize,
    pub expression: Expression,
    line: u32,
}

impl Leaf {
    fn evaluate(&self, arguments: &[Value]) -> Option<f64> {
        let operand = |input| match input {
            Input::Constant(value) => Some(value),
            Input::Argument(index) => match *arguments.get(index)? {
                Value::Number(value) => Some(value),
                _ => None,
            },
        };
        match self.expression {
            Expression::Value(input) => operand(input),
            Expression::Negate(input) => Some(-operand(input)?),
            Expression::Binary(op, left, right) => {
                Some(Vm::number_op(op, operand(left)?, operand(right)?))
            }
        }
    }
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
        let Prototype {
            code,
            lines,
            params,
            registers,
            vararg,
            compat_arg,
            ..
        } = &program.data.prototypes[closure.prototype];
        if *vararg || *compat_arg {
            return None;
        }
        let constants = &program.constants[closure.prototype];
        let operand = |index| {
            if index < u32::from(*params) {
                Some(Input::Argument(index as usize))
            } else if index >= RK {
                let Value::Number(number) = constants[(index - RK) as usize] else {
                    return None;
                };
                Some(Input::Constant(number))
            } else {
                None
            }
        };
        let first = code[0];
        let (expression, return_pc) = if first.op == Op::Return {
            if first.c != 1 || u16::from(first.a) >= *params {
                return None;
            }
            (Expression::Value(Input::Argument(usize::from(first.a))), 0)
        } else {
            let second = *code.get(1)?;
            if second.op != Op::Return || second.c != 1 || second.a != first.a {
                return None;
            }
            let expression = match first.op {
                Op::Load if first.b == 0 => {
                    let Value::Number(value) = constants[first.c as usize] else {
                        return None;
                    };
                    Expression::Value(Input::Constant(value))
                }
                Op::Move if first.c == 0 => Expression::Value(operand(u32::from(first.b))?),
                Op::Neg => Expression::Negate(operand(u32::from(first.b))?),
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                    Expression::Binary(first.op, operand(u32::from(first.b))?, operand(first.c)?)
                }
                _ => return None,
            };
            (expression, 1)
        };
        Some(Leaf {
            registers: usize::from(*registers),
            expression,
            line: lines[return_pc],
        })
    }

    fn has_numeric_call_space(&self, registers: usize) -> bool {
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
        keep_result: bool,
        error_line: &mut u32,
    ) -> bool {
        let Value::Function(function) = self.registers[call] else {
            return false;
        };
        let Some(leaf) = cache.get(self, function) else {
            return false;
        };
        let arguments = &self.registers[call + 1..call + 1 + count];
        let Some(result) = leaf.evaluate(arguments) else {
            return false;
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
        if keep_result {
            self.registers[call] = Value::Number(result);
        }
        self.top = call + usize::from(keep_result);
        *error_line = line;
        true
    }
}
