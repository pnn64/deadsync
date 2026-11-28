//! Decode the numeric body of a parked loop once per interpreter checkpoint.
use super::*;
use crate::vm::numeric_leaf::{Expression, Input};
mod recipes;

const MAX_WRITES: usize = 8;

#[derive(Clone, Copy)]
struct Write {
    register: u8,
    expression: Expression,
}

const EMPTY: Write = Write {
    register: 0,
    expression: Expression::Value(Input::Constant(0.0)),
};

enum Writes {
    Recipes(recipes::Recipes),
    General {
        values: [Write; MAX_WRITES],
        len: usize,
    },
}

pub(super) struct YieldBody {
    control: usize,
    function: usize,
    count: usize,
    capture_register: Option<usize>,
    writes: Writes,
    index: f64,
    limit: f64,
    step: f64,
    pending: bool,
}

impl YieldBody {
    pub(super) fn decode(
        vm: &Vm,
        closure: &Closure,
        program: &LoadedProgram,
        pc: usize,
        registers: &[Value],
    ) -> Option<Self> {
        let prototype = &program.data.prototypes[closure.prototype];
        let code = prototype.code();
        let constants = program.bound_constants(closure.prototype);
        let loop_pc = pc + usize::from(code.get(pc)?.op == Op::Close);
        let loop_instruction = code.get(loop_pc)?;
        if loop_instruction.op != Op::ForLoop {
            return None;
        }
        let (call, body) = code.get(loop_instruction.c as usize..pc)?.split_last()?;
        let (get_yield, body) = body.split_first()?;
        let control = usize::from(loop_instruction.a);
        let function = usize::from(call.a);
        let count = usize::from(call.b);
        if call.op != Op::Call
            || function < control + 4
            || count == 0
            || function + 1 + count > usize::from(prototype.registers)
            || get_yield.op != Op::GetUpvalue
            || get_yield.a != call.a
            || body.len() > MAX_WRITES
        {
            return None;
        }
        let [
            Value::Number(index),
            Value::Number(limit),
            Value::Number(step),
        ] = registers[control..control + 3]
        else {
            return None;
        };
        let upvalue = usize::try_from(*closure.upvalues.get(get_yield.c as usize)?).ok()?;
        let capture_register = match vm.upvalues[upvalue] {
            Cell::Closed(Value::Native(CO_YIELD)) => None,
            Cell::Open(owner, register) if owner == vm.thread => Some(register),
            _ => return None,
        };
        let mut writes = [EMPTY; MAX_WRITES];
        let mut known = 1u32 << (control + 3);
        for (output, instruction) in writes.iter_mut().zip(body) {
            let register = usize::from(instruction.a);
            if register < control + 4 || register == function {
                return None;
            }
            let constant = |id| match constants.get(id) {
                Some(Value::Number(value)) => Some(Input::Constant(*value)),
                _ => None,
            };
            let operand = |id: u32| {
                if id >= RK {
                    constant((id - RK) as usize)
                } else if id < REGISTER_LIMIT as u32 && known & (1 << id) != 0 {
                    Some(Input::Argument(id as usize))
                } else {
                    None
                }
            };
            let left = u32::from(instruction.b);
            let expression = match instruction.op {
                Op::Load if instruction.b == 0 => {
                    Expression::Value(constant(instruction.c as usize)?)
                }
                Op::Move if instruction.c == 0 => Expression::Value(operand(left)?),
                Op::Neg => Expression::Negate(operand(left)?),
                Op::Add | Op::Sub | Op::Mul | Op::Div => {
                    Expression::Binary(instruction.op, operand(left)?, operand(instruction.c)?)
                }
                _ => return None,
            };
            *output = Write {
                register: instruction.a,
                expression,
            };
            known |= 1 << register;
        }
        if (function + 1..=function + count).any(|register| known & (1 << register) == 0) {
            return None;
        }
        let writes = if let Some(recipes) =
            recipes::Recipes::decode(&writes[..body.len()], control + 3, function + 1, count)
        {
            Writes::Recipes(recipes)
        } else {
            Writes::General {
                values: writes,
                len: body.len(),
            }
        };
        Some(Self {
            control,
            function,
            count,
            capture_register,
            writes,
            index,
            limit,
            step,
            pending: false,
        })
    }

    pub(super) fn resume(
        &mut self,
        thread: ThreadId,
        base: usize,
        cx: &mut simple::Context<'_, &mut Arena<Thread>>,
        call: usize,
        wanted: usize,
    ) -> bool {
        if self
            .capture_register
            .is_some_and(|register| cx.registers[register] != Value::Native(CO_YIELD))
        {
            return false;
        }
        let index = self.index + self.step;
        if !index_in_range(index, self.limit, self.step) {
            return false;
        }
        self.index = index;

        let args = self.function + 1;
        match &self.writes {
            Writes::Recipes(recipes) if self.count == 2 && wanted == 3 => {
                let [first, second] = recipes.evaluate(index);
                self.pending = true;
                cx.registers[call] = Value::Boolean(true);
                cx.registers[call + 1] = first;
                cx.registers[call + 2] = second;
            }
            Writes::Recipes(recipes) if self.count == 3 && wanted == 4 => {
                let values = recipes.evaluate::<3>(index);
                self.pending = true;
                cx.registers[call] = Value::Boolean(true);
                cx.registers[call + 1..call + wanted].copy_from_slice(&values);
            }
            Writes::Recipes(recipes) => {
                let mut values = [Value::Nil; recipes::MAX_RESULTS];
                recipes.write(index, &mut values[..self.count]);
                self.pending = true;
                write_resume_results(cx.registers, call, wanted, &values[..self.count]);
            }
            Writes::General { values, len } => {
                let state = &mut cx.threads[slot_index(thread.0)];
                let registers = &mut state.registers[base..];
                self.write_control(registers);
                Self::write_registers(&values[..*len], registers, self.control + 3, index);
                write_resume_results(
                    cx.registers,
                    call,
                    wanted,
                    &registers[args..args + self.count],
                );
            }
        }
        true
    }

    /// Publish the last iteration before any instruction can inspect the thread.
    pub(super) fn write_back(
        &mut self,
        threads: &mut Arena<Thread>,
        thread: ThreadId,
        base: usize,
    ) {
        if std::mem::take(&mut self.pending)
            && let Writes::Recipes(recipes) = &self.writes
        {
            let registers = &mut threads[slot_index(thread.0)].registers[base..];
            self.write_control(registers);
            let args = self.function + 1;
            recipes.write(self.index, &mut registers[args..args + self.count]);
        }
    }

    fn write_control(&self, registers: &mut [Value]) {
        registers[self.control] = Value::Number(self.index);
        registers[self.control + 3] = Value::Number(self.index);
        registers[self.function] = Value::Native(CO_YIELD);
    }

    fn write_registers(writes: &[Write], registers: &mut [Value], control: usize, index: f64) {
        let mut values = [0.0; REGISTER_LIMIT];
        values[control] = index;
        // Every input reads a constant, this iteration's index, or an earlier
        // write. No operation past the loop guard can require a fallback.
        for write in writes {
            let operand = |input| match input {
                Input::Argument(register) => values[register],
                Input::Constant(value) => value,
            };
            let value = match write.expression {
                Expression::Value(input) => operand(input),
                Expression::Negate(input) => -operand(input),
                Expression::Binary(op, a, b) => Vm::number_op(op, operand(a), operand(b)),
            };
            let register = usize::from(write.register);
            values[register] = value;
            registers[register] = Value::Number(value);
        }
    }
}
