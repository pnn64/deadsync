//! Map known vararg slots to numeric inputs before interpreting a short body.
use super::{Exit, MAX_REGISTERS, MAX_VALUES, NIL, Source, decode, has_stack_space};
use crate::bytecode::{OPEN, Op, RK};
use crate::vm::{
    FunctionId, Value, Vm,
    numeric_leaf::{Expression, Input},
    slot_index,
};

const MAX_OPS: usize = 12;
const MAX_INSTRUCTIONS: usize = 32;
const SCRATCH: usize = MAX_VALUES + MAX_OPS;
const ZERO: Expression = Expression::Value(Input::Constant(0.0));

#[derive(Clone, Copy)]
enum Slot {
    Numeric(Input),
    Constant(Value),
}

impl Slot {
    fn source(source: Source) -> Option<Self> {
        Some(match source {
            Source::Argument(index) => Self::Numeric(Input::Argument(index)),
            Source::Constant(value) => Self::Constant(value),
            Source::Capture(_) => return None,
        })
    }

    fn input(self) -> Option<Input> {
        match self {
            Self::Numeric(input) => Some(input),
            Self::Constant(Value::Number(number)) => Some(Input::Constant(number)),
            Self::Constant(_) => None,
        }
    }

    fn truthy(self) -> bool {
        match self {
            Self::Numeric(_) => true,
            Self::Constant(value) => value.is_truthy(),
        }
    }
}

pub(in crate::vm) struct Plan {
    operations: [Expression; MAX_OPS],
    count: usize,
    results: [Option<Input>; 2],
    registers: usize,
    tail: Option<(usize, FunctionId)>,
    pub(super) line: u32,
}

impl Plan {
    pub(in crate::vm) fn build(vm: &Vm, function: FunctionId, arity: usize) -> Option<Self> {
        let arguments: [Source; MAX_VALUES] = std::array::from_fn(Source::Argument);
        let arguments = arguments.get(..arity)?;
        let mut outer_slots = [NIL; MAX_REGISTERS];
        if let Some(outer) = decode(vm, function, arguments, &mut outer_slots)
            && let Exit::Tail(cell, range) = outer.exit
        {
            let Value::Function(target) = vm.cell_value(cell) else {
                return None;
            };
            let forwarded = outer_slots.get(range)?;
            let mut plan = Self::body(vm, target, forwarded, false)?;
            let count = forwarded.len();
            // Moving the tail arguments uses temporary space even when both
            // frame windows already fit.
            plan.registers = outer.high_water.max(plan.registers) + count;
            plan.tail = Some((cell, target));
            Some(plan)
        } else {
            Self::body(vm, function, arguments, true)
        }
    }

    fn body(vm: &Vm, function: FunctionId, arguments: &[Source], direct: bool) -> Option<Self> {
        let closure = &vm.functions[slot_index(function.0)];
        let program = &vm.programs[closure.program];
        let prototype = &program.data.prototypes[closure.prototype];
        let width = usize::from(prototype.registers);
        if width > MAX_REGISTERS || prototype.compat_arg || (direct && !prototype.vararg) {
            return None;
        }
        let fixed = usize::from(prototype.params).min(arguments.len());
        let extras = &arguments[fixed..];
        let mut slots = [Slot::Constant(Value::Nil); MAX_REGISTERS];
        for (slot, &source) in slots.iter_mut().zip(&arguments[..fixed]) {
            *slot = Slot::source(source)?;
        }
        let constants = program.bound_constants(closure.prototype);
        let code = &prototype.code;
        let mut plan = Self {
            operations: [ZERO; MAX_OPS],
            count: 0,
            results: [None; 2],
            registers: width,
            tail: None,
            line: 0,
        };
        let mut pc = 0;
        for _ in 0..MAX_INSTRUCTIONS {
            let instruction = *code.get(pc)?;
            let (a, b, c) = (
                usize::from(instruction.a),
                u32::from(instruction.b),
                instruction.c,
            );
            pc += 1;
            let slot = |key| {
                if key < RK {
                    slots[key as usize]
                } else {
                    Slot::Constant(constants[(key - RK) as usize])
                }
            };
            let operation = match instruction.op {
                Op::Vararg if prototype.vararg => {
                    let count = if c == OPEN { extras.len() } else { c as usize };
                    let end = a + count;
                    for (index, output) in slots.get_mut(a..end)?.iter_mut().enumerate() {
                        *output = Slot::source(extras.get(index).copied().unwrap_or(NIL))?;
                    }
                    plan.registers = plan.registers.max(end);
                    continue;
                }
                Op::Load if b == 0 => {
                    slots[a] = Slot::Constant(constants[c as usize]);
                    continue;
                }
                Op::Move if c == 0 => {
                    slots[a] = slot(b);
                    continue;
                }
                Op::JumpTrue | Op::JumpFalse if b == 0 && c as usize >= pc => {
                    if slots[a].truthy() == (instruction.op == Op::JumpTrue) {
                        pc = c as usize;
                    }
                    continue;
                }
                Op::Jump if b == 0 && c as usize >= pc => {
                    pc = c as usize;
                    continue;
                }
                Op::Close => continue,
                Op::Neg => Expression::Negate(slot(b).input()?),
                op @ (Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow) => {
                    Expression::Binary(op, slot(b).input()?, slot(c).input()?)
                }
                Op::Return if (1..=2).contains(&c) => {
                    for (output, value) in
                        plan.results.iter_mut().zip(slots.get(a..a + c as usize)?)
                    {
                        *output = Some(value.input()?);
                    }
                    plan.line = prototype.instruction_line(pc - 1);
                    if !direct {
                        plan.registers += arguments
                            .len()
                            .saturating_sub(usize::from(prototype.params));
                    }
                    return Some(plan);
                }
                _ => return None,
            };
            *plan.operations.get_mut(plan.count)? = operation;
            slots[a] = Slot::Numeric(Input::Argument(MAX_VALUES + plan.count));
            plan.count += 1;
        }
        None
    }

    pub(in crate::vm) fn matches(&self, vm: &Vm) -> bool {
        self.tail
            .is_none_or(|(cell, target)| vm.cell_value(cell) == Value::Function(target))
    }

    pub(in crate::vm) fn tail_target(&self) -> Option<FunctionId> {
        self.tail.map(|(_, function)| function)
    }

    pub(in crate::vm) fn has_stack_space(&self, vm: &Vm) -> bool {
        has_stack_space(vm, self.tail.is_some(), self.registers)
    }

    pub(in crate::vm) fn evaluate_values(&self, arguments: &[Value]) -> Option<[Option<f64>; 2]> {
        let mut numbers = [0.0; MAX_VALUES];
        for (output, &value) in numbers.iter_mut().zip(arguments) {
            let Value::Number(number) = value else {
                return None;
            };
            *output = number;
        }
        Some(self.evaluate(&numbers))
    }

    pub(in crate::vm) fn evaluate(&self, arguments: &[f64; MAX_VALUES]) -> [Option<f64>; 2] {
        let mut numbers = [0.0; SCRATCH];
        numbers[..MAX_VALUES].copy_from_slice(arguments);
        let read = |numbers: &[f64; SCRATCH], input| match input {
            Input::Argument(index) => numbers[index],
            Input::Constant(value) => value,
        };
        for (index, &operation) in self.operations[..self.count].iter().enumerate() {
            numbers[MAX_VALUES + index] = operation
                .evaluate_with(|input| Some(read(&numbers, input)))
                .expect("verified numeric operands");
        }
        self.results
            .map(|input| input.map(|input| read(&numbers, input)))
    }
}
