//! Guarded numeric calls through dense arrays inside exact-index loops.

use super::super::super::{Reduction, integer};
use crate::bytecode::{Instruction, Op, RK};
use crate::vm::{
    Cell, FrameKind, FunctionId, TableId, Value, Vm,
    numeric_leaf::{Expression, Input},
    slot_index,
};

const MAX_ARGUMENTS: usize = 2;
const MAX_ARRAYS: usize = MAX_ARGUMENTS + 1;
const MAX_TARGETS: usize = 4;
const MAX_OPS: usize = 4;

#[derive(Clone, Copy)]
enum Slot {
    Initial(usize),
    Fixed(Value),
    Index,
    Control,
    Array(usize),
    Result,
    Total(usize),
}

struct Registers<'a> {
    initial: &'a [Value],
    constants: &'a [Value],
    slots: [Option<Slot>; 256],
    read_initial: [bool; 256],
    invariant: [bool; 256],
}

impl Registers<'_> {
    fn read(&mut self, key: u32) -> Slot {
        if key >= RK {
            return Slot::Fixed(self.constants[(key - RK) as usize]);
        }
        let slot = key as usize;
        self.slots[slot].unwrap_or_else(|| {
            self.read_initial[slot] = true;
            Slot::Initial(slot)
        })
    }

    fn fixed(&mut self, slot: Slot) -> Option<Value> {
        match slot {
            Slot::Fixed(value) => Some(value),
            Slot::Initial(slot) => {
                self.invariant[slot] = true;
                self.initial.get(slot).copied()
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Argument {
    Array(usize),
    Number(f64),
}

struct Call {
    target: usize,
    arguments: [Argument; MAX_ARGUMENTS],
    arity: usize,
}

struct Plan {
    tables: [TableId; MAX_ARRAYS],
    table_count: usize,
    call: Call,
    sum: usize,
}

impl Plan {
    fn decode(
        vm: &Vm,
        body: &[Instruction],
        control: usize,
        initial: &[Value],
        constants: &[Value],
    ) -> Option<Self> {
        let FrameKind::Lua(id) = vm.frames.last()?.function else {
            return None;
        };
        let closure = &vm.functions[slot_index(id.0)];
        let mut r = Registers {
            initial,
            constants,
            slots: [None; 256],
            read_initial: [false; 256],
            invariant: [false; 256],
        };
        r.slots[control..control + 3].fill(Some(Slot::Control));
        r.slots[control + 3] = Some(Slot::Index);
        let mut tables = [TableId(0); MAX_ARRAYS];
        let mut table_count = 0;
        let mut call = None;
        let mut sum = None;
        for instruction in body {
            let a = usize::from(instruction.a);
            let b = u32::from(instruction.b);
            let c = instruction.c;
            if instruction.op == Op::Close {
                continue;
            }
            if (control..control + 4).contains(&a) {
                return None;
            }
            r.slots[a] = Some(match instruction.op {
                Op::Load if b == 0 => Slot::Fixed(constants[c as usize]),
                Op::Move if c == 0 => r.read(b),
                Op::GetUpvalue => {
                    let cell = closure.upvalues[c as usize];
                    let Cell::Closed(value) = vm.upvalues[cell as usize] else {
                        return None;
                    };
                    Slot::Fixed(value)
                }
                Op::GetTable => {
                    let table = r.read(b);
                    let Value::Table(table) = r.fixed(table)? else {
                        return None;
                    };
                    if !matches!(r.read(c), Slot::Index) {
                        return None;
                    }
                    let index = if let Some(index) =
                        tables[..table_count].iter().position(|&id| id == table)
                    {
                        index
                    } else {
                        *tables.get_mut(table_count)? = table;
                        table_count += 1;
                        table_count - 1
                    };
                    Slot::Array(index)
                }
                Op::Call if call.is_none() && b <= MAX_ARGUMENTS as u32 && c == 1 => {
                    let Slot::Array(target) = r.read(a as u32) else {
                        return None;
                    };
                    let arity = b as usize;
                    let mut arguments = [Argument::Number(0.0); MAX_ARGUMENTS];
                    for (offset, output) in arguments[..arity].iter_mut().enumerate() {
                        let slot = r.read((a + 1 + offset) as u32);
                        *output = if let Slot::Array(index) = slot {
                            Argument::Array(index)
                        } else {
                            let Value::Number(number) = r.fixed(slot)? else {
                                return None;
                            };
                            Argument::Number(number)
                        };
                    }
                    call = Some(Call {
                        target,
                        arguments,
                        arity,
                    });
                    Slot::Result
                }
                Op::Add if sum.is_none() => {
                    let (Slot::Initial(slot), Slot::Result) = (r.read(b), r.read(c)) else {
                        return None;
                    };
                    sum = Some(slot);
                    Slot::Total(slot)
                }
                _ => return None,
            });
        }
        let sum = sum?;
        let call = call?;
        if r.invariant[sum]
            || !matches!(r.slots[sum], Some(Slot::Total(slot)) if slot == sum)
            || r.read_initial
                .iter()
                .enumerate()
                .any(|(slot, &read)| read && slot != sum && r.slots[slot].is_some())
        {
            return None;
        }
        // Check all array reads, including reads whose destination was later
        // overwritten. An unchecked nil could invoke an index metamethod.
        let mut used = 1 << call.target;
        for argument in &call.arguments[..call.arity] {
            if let Argument::Array(index) = *argument {
                used |= 1 << index;
            }
        }
        (used == (1 << table_count) - 1).then_some(Self {
            tables,
            table_count,
            call,
            sum,
        })
    }

    fn scan(
        &self,
        vm: &Vm,
        bounds: (i64, i64),
        ascending: bool,
        mut sum: f64,
    ) -> Option<(f64, usize)> {
        let mut arrays = [&[][..]; MAX_ARRAYS];
        for (slice, id) in arrays.iter_mut().zip(&self.tables[..self.table_count]) {
            *slice = vm.tables[slot_index(id.0)].array_slice(bounds.0, bounds.1)?;
        }
        let len = arrays[self.call.target].len();
        let mut targets = Targets::new();
        let mut term = |index| self.call.value(vm, &arrays, index, &mut targets);
        if ascending {
            for index in 0..len - 1 {
                sum += term(index)?;
            }
            term(len - 1)?;
        } else {
            for index in (1..len).rev() {
                sum += term(index)?;
            }
            term(0)?;
        }
        Some((sum, targets.registers))
    }
}

impl Call {
    fn value(
        &self,
        vm: &Vm,
        arrays: &[&[Value]; MAX_ARRAYS],
        index: usize,
        targets: &mut Targets,
    ) -> Option<f64> {
        let Value::Function(function) = arrays[self.target][index] else {
            return None;
        };
        let mut numbers = [0.0; MAX_ARGUMENTS + MAX_OPS];
        for (number, argument) in numbers.iter_mut().zip(&self.arguments[..self.arity]) {
            *number = match *argument {
                Argument::Number(value) => value,
                Argument::Array(array) => {
                    let Value::Number(value) = arrays[array][index] else {
                        return None;
                    };
                    value
                }
            };
        }
        Some(
            targets
                .recipe(vm, function, self.arity)?
                .value(&mut numbers),
        )
    }
}

#[derive(Clone, Copy)]
struct Recipe {
    operations: [Expression; MAX_OPS],
    count: usize,
    result: Input,
    registers: usize,
}

impl Recipe {
    fn decode(vm: &Vm, function: FunctionId, arity: usize) -> Option<Self> {
        let closure = &vm.functions[slot_index(function.0)];
        let program = &vm.programs[closure.program];
        let prototype = &program.data.prototypes[closure.prototype];
        let size = usize::from(prototype.registers);
        let mut slots = [None; 16];
        if prototype.vararg || prototype.compat_arg || size > slots.len() {
            return None;
        }
        let fixed = usize::from(prototype.params).min(arity);
        for (index, slot) in slots[..fixed].iter_mut().enumerate() {
            *slot = Some(Input::Argument(index));
        }
        let zero = Expression::Value(Input::Constant(0.0));
        let mut recipe = Self {
            operations: [zero; MAX_OPS],
            count: 0,
            result: Input::Constant(0.0),
            registers: size,
        };
        let constants = program.bound_constants(closure.prototype);
        let constant = |index| match constants[index] {
            Value::Number(value) => Some(Input::Constant(value)),
            _ => None,
        };
        for instruction in prototype.code.iter().take(MAX_OPS + 1) {
            let a = usize::from(instruction.a);
            let b = u32::from(instruction.b);
            let c = instruction.c;
            let operand = |key| {
                if key < RK {
                    slots[key as usize]
                } else {
                    constant((key - RK) as usize)
                }
            };
            let operation = match instruction.op {
                Op::Load if b == 0 => {
                    slots[a] = Some(constant(c as usize)?);
                    continue;
                }
                Op::Move if c == 0 => {
                    slots[a] = Some(operand(b)?);
                    continue;
                }
                Op::Neg => Expression::Negate(operand(b)?),
                op @ (Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow) => {
                    Expression::Binary(op, operand(b)?, operand(c)?)
                }
                Op::Return if c == 1 => {
                    recipe.result = slots[a]?;
                    return Some(recipe);
                }
                _ => return None,
            };
            *recipe.operations.get_mut(recipe.count)? = operation;
            slots[a] = Some(Input::Argument(MAX_ARGUMENTS + recipe.count));
            recipe.count += 1;
        }
        None
    }

    #[inline]
    fn value(&self, numbers: &mut [f64; MAX_ARGUMENTS + MAX_OPS]) -> f64 {
        let read = |numbers: &[f64; MAX_ARGUMENTS + MAX_OPS], operand| match operand {
            Input::Argument(index) => numbers[index],
            Input::Constant(number) => number,
        };
        for (index, operation) in self.operations[..self.count].iter().enumerate() {
            numbers[MAX_ARGUMENTS + index] = operation
                .evaluate_with(|operand| Some(read(numbers, operand)))
                .expect("verified numeric operation");
        }
        read(numbers, self.result)
    }
}

struct Targets {
    recipes: [Option<(FunctionId, Recipe)>; MAX_TARGETS],
    registers: usize,
}

impl Targets {
    fn new() -> Self {
        Self {
            recipes: [None; MAX_TARGETS],
            registers: 0,
        }
    }

    fn recipe<'a>(&'a mut self, vm: &Vm, function: FunctionId, arity: usize) -> Option<&'a Recipe> {
        let slot = self
            .recipes
            .iter()
            .position(|entry| entry.as_ref().is_none_or(|(id, _)| *id == function))?;
        if self.recipes[slot].is_none() {
            let recipe = Recipe::decode(vm, function, arity)?;
            self.registers = self.registers.max(recipe.registers);
            self.recipes[slot] = Some((function, recipe));
        }
        self.recipes[slot].as_ref().map(|(_, recipe)| recipe)
    }
}

impl Vm {
    pub(in crate::vm) fn reduce_array_calls(
        &mut self,
        base: usize,
        control: usize,
        body: &[Instruction],
        program: usize,
        prototype: usize,
    ) -> Option<Reduction> {
        if control + 3 >= 256
            || !self.sort_fast_context()
            || !self.open_upvalues.is_empty()
            || !body.iter().any(|i| i.op == Op::Call && i.c == 1)
        {
            return None;
        }
        let start = base + control;
        let index = integer(self.registers[start])?;
        let last = integer(self.registers[start + 1])?;
        let Some(step @ (-1 | 1)) = integer(self.registers[start + 2]) else {
            return None;
        };
        let first = index + step;
        let (lower, upper) = if step > 0 {
            (first, last)
        } else {
            (last, first)
        };
        if lower < 1 || upper > i128::from(u32::MAX) || lower >= upper {
            return None;
        }
        let plan = Plan::decode(
            self,
            body,
            control,
            &self.registers[base..],
            self.programs[program].bound_constants(prototype),
        )?;
        let Value::Number(initial) = self.registers[base + plan.sum] else {
            return None;
        };
        // Numeric calls through strong arrays do no collector work. An
        // incremental phase can stay parked across their frame checkpoints.
        if plan.tables[..plan.table_count]
            .iter()
            .any(|id| self.tables[slot_index(id.0)].metatable().is_some())
        {
            return None;
        }
        if !self.numeric_sort_gc_ready() {
            return Some(Reduction::Retry);
        }
        let (sum, registers) = plan.scan(self, (lower as i64, upper as i64), step > 0, initial)?;
        if !self.has_numeric_call_space(registers) {
            return Some(Reduction::Retry);
        }
        self.gc.pending = false;
        // The actual final iteration publishes all remaining register values.
        self.registers[start] = Value::Number((last - step) as f64);
        self.registers[base + plan.sum] = Value::Number(sum);
        Some(Reduction::Applied(
            (upper - lower) as u64 * (body.len() as u64 + 1),
        ))
    }
}
