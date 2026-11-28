//! Guarded numeric calls through dense arrays inside exact-index loops.

use super::super::super::{Reduction, integer};
use super::exact::{Exact, MIN_LENGTH, repair_blocks};
use crate::arena::Arena;
use crate::bytecode::{Instruction, Op, RK};
use crate::table::{ARRAY_WATCH_BLOCKS, ArrayChange, Table};
use crate::vm::{
    Cell, FrameKind, FunctionId, TableId, Value, Vm,
    numeric_leaf::{Expression, Input},
    slot_index,
};

const MAX_ARGUMENTS: usize = 2;
const MAX_ARRAYS: usize = MAX_ARGUMENTS + 1;
const MAX_RESULTS: usize = 2;
const MAX_TARGETS: usize = 4;
const MAX_OPS: usize = 4;

#[derive(Clone, Copy)]
enum Slot {
    Initial(usize),
    Fixed(Value),
    Index,
    Control,
    Array(usize),
    Result(u8),
    Total { register: usize, terms: usize },
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Argument {
    Array(usize),
    Number(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Array(usize),
    Fixed(FunctionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Call {
    target: Target,
    arguments: [Argument; MAX_ARGUMENTS],
    arity: usize,
    results: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Plan {
    tables: [TableId; MAX_ARRAYS],
    table_count: usize,
    call: Call,
    sum: usize,
    addends: [u8; MAX_RESULTS],
}

#[derive(Debug, Eq, PartialEq)]
struct Key {
    plan: Plan,
    bounds: (i64, i64),
    ascending: bool,
}

#[derive(Debug)]
pub(super) struct CachedCall {
    key: Key,
    exact: Option<Exact>,
    registers: usize,
}

struct Scan {
    sum: f64,
    exact: Option<Exact>,
    registers: usize,
}

struct Accumulator {
    sum: f64,
    exact: Option<Exact>,
}

impl Accumulator {
    #[inline]
    fn add(&mut self, value: f64) {
        self.sum += value;
        if let Some(proof) = &mut self.exact
            && proof.push(value).is_none()
        {
            self.exact = None;
        }
    }

    fn finish(self, iterations: usize, terms: usize, registers: usize) -> Scan {
        Scan {
            sum: self.sum,
            exact: self
                .exact
                .and_then(|proof| proof.check_count(iterations, terms)),
            registers,
        }
    }
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
        let mut terms = 0;
        let mut addends = [0; MAX_RESULTS];
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
                Op::Call if call.is_none() && b <= MAX_ARGUMENTS as u32 && matches!(c, 1 | 2) => {
                    let target = match r.read(a as u32) {
                        Slot::Array(array) => Target::Array(array),
                        slot if c == 2 => {
                            let Value::Function(function) = r.fixed(slot)? else {
                                return None;
                            };
                            Target::Fixed(function)
                        }
                        _ => return None,
                    };
                    let arity = b as usize;
                    let mut arguments = [Argument::Number(0); MAX_ARGUMENTS];
                    for (offset, output) in arguments[..arity].iter_mut().enumerate() {
                        let slot = r.read((a + 1 + offset) as u32);
                        *output = if let Slot::Array(index) = slot {
                            Argument::Array(index)
                        } else {
                            let Value::Number(number) = r.fixed(slot)? else {
                                return None;
                            };
                            Argument::Number(number.to_bits())
                        };
                    }
                    call = Some(Call {
                        target,
                        arguments,
                        arity,
                        results: c as usize,
                    });
                    if c == 2 {
                        if (control..control + 4).contains(&(a + 1)) {
                            return None;
                        }
                        *r.slots.get_mut(a + 1)? = Some(Slot::Result(1));
                    }
                    Slot::Result(0)
                }
                Op::Add => {
                    let (total, Slot::Result(result)) = (r.read(b), r.read(c)) else {
                        return None;
                    };
                    let register = match total {
                        Slot::Initial(register) if sum.is_none() => register,
                        Slot::Total {
                            register,
                            terms: depth,
                        } if sum == Some(register) && depth == terms => register,
                        _ => return None,
                    };
                    *addends.get_mut(terms)? = result;
                    terms += 1;
                    sum = Some(register);
                    Slot::Total { register, terms }
                }
                _ => return None,
            });
        }
        let sum = sum?;
        let call = call?;
        if table_count == 0
            || terms != call.results
            || r.invariant[sum]
            || !matches!(r.slots[sum], Some(Slot::Total { register, terms: depth })
                if register == sum && depth == terms)
            || r.read_initial
                .iter()
                .enumerate()
                .any(|(slot, &read)| read && slot != sum && r.slots[slot].is_some())
        {
            return None;
        }
        // Check all array reads, including reads whose destination was later
        // overwritten. An unchecked nil could invoke an index metamethod.
        let mut used = match call.target {
            Target::Array(array) => 1 << array,
            Target::Fixed(_) => 0,
        };
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
            addends,
        })
    }

    fn arrays<'a>(
        &self,
        tables: &'a Arena<Table>,
        bounds: (i64, i64),
    ) -> Option<[&'a [Value]; MAX_ARRAYS]> {
        let mut arrays = [&[][..]; MAX_ARRAYS];
        for (slice, id) in arrays.iter_mut().zip(&self.tables[..self.table_count]) {
            *slice = tables[slot_index(id.0)].array_slice(bounds.0, bounds.1)?;
        }
        Some(arrays)
    }

    fn scan<const RESULTS: usize>(
        &self,
        vm: &Vm,
        bounds: (i64, i64),
        ascending: bool,
        sum: f64,
        measure_exactness: bool,
    ) -> Option<Scan> {
        let arrays = self.arrays(&vm.tables, bounds)?;
        let len = arrays[0].len();
        let fixed = match self.call.target {
            Target::Fixed(function) => Some(Recipe::decode(
                vm,
                function,
                self.call.arity,
                self.call.results,
            )?),
            Target::Array(_) => None,
        };
        let mut accumulator = Accumulator {
            sum,
            exact: measure_exactness.then_some(Exact::ZERO),
        };
        if RESULTS == 2
            && let Some(recipe) = &fixed
            && let Some(pair) = self.additive_pair(recipe, &arrays)
        {
            return scan_additive_pair(pair, ascending, accumulator, recipe.registers);
        }
        let mut targets = Targets::new();
        let mut accumulate = |index| {
            let values =
                self.call
                    .values::<RESULTS>(vm, &arrays, index, fixed.as_ref(), &mut targets)?;
            for &addend in &self.addends[..RESULTS] {
                accumulator.add(values[usize::from(addend)]);
            }
            Some(())
        };
        if ascending {
            for index in 0..len - 1 {
                accumulate(index)?;
            }
        } else {
            for index in (1..len).rev() {
                accumulate(index)?;
            }
        }
        let last = if ascending { len - 1 } else { 0 };
        self.call
            .values::<RESULTS>(vm, &arrays, last, fixed.as_ref(), &mut targets)?;
        Some(accumulator.finish(
            len - 1,
            RESULTS,
            fixed.map_or(targets.registers, |recipe| recipe.registers),
        ))
    }

    fn additive_pair<'a>(
        &self,
        recipe: &Recipe,
        arrays: &[&'a [Value]; MAX_ARRAYS],
    ) -> Option<[(&'a [Value], f64); 2]> {
        if self.call.arity != 2
            || self.call.results != 2
            || recipe.count != 2
            || self.addends[0] == self.addends[1]
        {
            return None;
        }
        let term = |result: u8| {
            let Input::Argument(output) = recipe.results[usize::from(result)] else {
                return None;
            };
            let Expression::Binary(Op::Add, Input::Argument(argument), Input::Constant(offset)) =
                *recipe.operations.get(output.checked_sub(MAX_ARGUMENTS)?)?
            else {
                return None;
            };
            let Argument::Array(array) = *self.call.arguments.get(argument)? else {
                return None;
            };
            Some((argument, (arrays[array], offset)))
        };
        let (first_arg, first) = term(self.addends[0])?;
        let (second_arg, second) = term(self.addends[1])?;
        (first_arg != second_arg).then_some([first, second])
    }
}

fn additive_prefix(
    mut pair: [(&[Value], f64); 2],
    ascending: bool,
) -> Option<[(&[Value], f64); 2]> {
    for (array, _) in &mut pair {
        let (last, prefix) = if ascending {
            array.split_last()?
        } else {
            array.split_first()?
        };
        if !matches!(last, Value::Number(_)) {
            return None;
        }
        *array = prefix;
    }
    Some(pair)
}

fn scan_additive_pair(
    pair: [(&[Value], f64); 2],
    ascending: bool,
    mut accumulator: Accumulator,
    registers: usize,
) -> Option<Scan> {
    // Validate the last iteration's inputs as well, then leave it to dispatch.
    let [(first, first_offset), (second, second_offset)] = additive_prefix(pair, ascending)?;
    let mut add = |(&first, &second)| {
        let (Value::Number(first), Value::Number(second)) = (first, second) else {
            return None;
        };
        accumulator.add(first + first_offset);
        accumulator.add(second + second_offset);
        Some(())
    };
    let mut rows = first.iter().zip(second);
    if ascending {
        rows.try_for_each(&mut add)?;
    } else {
        rows.rev().try_for_each(add)?;
    }
    Some(accumulator.finish(first.len(), 2, registers))
}

impl Call {
    fn values<const RESULTS: usize>(
        &self,
        vm: &Vm,
        arrays: &[&[Value]; MAX_ARRAYS],
        index: usize,
        fixed: Option<&Recipe>,
        targets: &mut Targets,
    ) -> Option<[f64; RESULTS]> {
        let recipe = if let Some(fixed) = fixed {
            fixed
        } else {
            let Target::Array(target) = self.target else {
                return None;
            };
            let Value::Function(function) = arrays[target][index] else {
                return None;
            };
            targets.recipe(vm, function, self.arity, self.results)?
        };
        let mut numbers = [0.0; MAX_ARGUMENTS + MAX_OPS];
        for (number, argument) in numbers.iter_mut().zip(&self.arguments[..self.arity]) {
            *number = match *argument {
                Argument::Number(bits) => f64::from_bits(bits),
                Argument::Array(array) => {
                    let Value::Number(value) = arrays[array][index] else {
                        return None;
                    };
                    value
                }
            };
        }
        Some(recipe.values::<RESULTS>(&mut numbers))
    }
}

#[derive(Clone, Copy)]
struct Recipe {
    operations: [Expression; MAX_OPS],
    count: usize,
    results: [Input; MAX_RESULTS],
    registers: usize,
}

impl Recipe {
    fn decode(vm: &Vm, function: FunctionId, arity: usize, results: usize) -> Option<Self> {
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
            results: [Input::Constant(0.0); MAX_RESULTS],
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
                Op::Return if c as usize == results => {
                    for (output, &slot) in recipe.results[..results]
                        .iter_mut()
                        .zip(slots.get(a..a + results)?)
                    {
                        *output = slot?;
                    }
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
    fn values<const RESULTS: usize>(
        &self,
        numbers: &mut [f64; MAX_ARGUMENTS + MAX_OPS],
    ) -> [f64; RESULTS] {
        let read = |numbers: &[f64; MAX_ARGUMENTS + MAX_OPS], operand| match operand {
            Input::Argument(index) => numbers[index],
            Input::Constant(number) => number,
        };
        for (index, operation) in self.operations[..self.count].iter().enumerate() {
            numbers[MAX_ARGUMENTS + index] = operation
                .evaluate_with(|operand| Some(read(numbers, operand)))
                .expect("verified numeric operation");
        }
        std::array::from_fn(|index| read(numbers, self.results[index]))
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

    fn recipe<'a>(
        &'a mut self,
        vm: &Vm,
        function: FunctionId,
        arity: usize,
        results: usize,
    ) -> Option<&'a Recipe> {
        let slot = self
            .recipes
            .iter()
            .position(|entry| entry.as_ref().is_none_or(|(id, _)| *id == function))?;
        if self.recipes[slot].is_none() {
            let recipe = Recipe::decode(vm, function, arity, results)?;
            self.registers = self.registers.max(recipe.registers);
            self.recipes[slot] = Some((function, recipe));
        }
        self.recipes[slot].as_ref().map(|(_, recipe)| recipe)
    }
}

impl Vm {
    fn repair_array_call(&mut self, key: &Key) -> Option<()> {
        let plan = &key.plan;
        let watched = &plan.tables[..plan.table_count];
        let mut dirty = [0; MAX_ARRAYS * ARRAY_WATCH_BLOCKS];
        let mut count = 0;
        for id in watched {
            match self.tables[slot_index(id.0)].array_change() {
                ArrayChange::Clean => {}
                ArrayChange::Dirty { blocks, len } => {
                    for &block in &blocks[..len] {
                        if !dirty[..count].contains(&block) {
                            dirty[count] = block;
                            count += 1;
                        }
                    }
                }
                ArrayChange::Unwatched => return None,
            }
        }
        let Target::Fixed(function) = plan.call.target else {
            return None;
        };
        let recipe = Recipe::decode(self, function, plan.call.arity, plan.call.results)?;
        let arrays = plan.arrays(&self.tables, key.bounds)?;
        let pair = plan.additive_pair(&recipe, &arrays)?;
        let [(first, first_offset), (second, second_offset)] =
            additive_prefix(pair, key.ascending)?;
        let start = key.bounds.0 as usize - usize::from(key.ascending);
        let (cached, blocks) = self.array_reduction.call_blocks()?;
        let exact = repair_blocks(blocks, start, first.len(), &dirty[..count], 2, |range| {
            let mut exact = Exact::ZERO;
            for (&first, &second) in first[range.clone()].iter().zip(&second[range]) {
                let (Value::Number(first), Value::Number(second)) = (first, second) else {
                    return None;
                };
                exact.push(first + first_offset)?;
                exact.push(second + second_offset)?;
            }
            Some(exact)
        });
        let Some(exact) = exact else {
            blocks.clear();
            return None;
        };
        cached.exact = Some(exact);
        for id in watched {
            self.tables[slot_index(id.0)].watch_array();
        }
        Some(())
    }

    pub(in crate::vm) fn reduce_array_calls(
        &mut self,
        base: usize,
        control: usize,
        body: &[Instruction],
        program: usize,
        prototype: usize,
    ) -> Option<Reduction> {
        if matches!(body, [_, get, _, _, _, ..] if body.len() <= 6 && get.op == Op::GetTable)
            && let Some(reduction) =
                self.reduce_indexed_comparisons(base, control, body, program, prototype)
        {
            return Some(reduction);
        }
        if control + 3 >= 256
            || !self.sort_fast_context()
            || !self.open_upvalues.is_empty()
            || !body
                .iter()
                .any(|i| i.op == Op::Call && matches!(i.c, 1 | 2))
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
        if !self.gc_checkpoint_idle() {
            return Some(Reduction::Retry);
        }
        let bounds = (lower as i64, upper as i64);
        let key = Key {
            plan,
            bounds,
            ascending: step > 0,
        };
        let watched = &plan.tables[..plan.table_count];
        let cacheable = bounds.1 - bounds.0 + 1 >= MIN_LENGTH;
        let matching = self
            .array_reduction
            .cached_call()
            .filter(|cached| cached.key == key);
        let mut current = matching.is_some()
            && watched
                .iter()
                .all(|id| self.tables[slot_index(id.0)].array_change() == ArrayChange::Clean);
        if !current && plan.call.results == 2 && matching.is_some_and(|entry| entry.exact.is_some())
        {
            let before = self.array_reduction.storage_bytes();
            current = self.repair_array_call(&key).is_some();
            let growth = self.array_reduction.storage_bytes() - before;
            if growth != 0 {
                self.gc_note_control(growth);
                if !self.gc_checkpoint_idle() {
                    return Some(Reduction::Retry);
                }
            }
        }
        let cached = current
            .then(|| self.array_reduction.cached_call())
            .flatten();
        let hit = cached
            .and_then(|cached| Some((cached.exact.as_ref()?.apply(initial)?, cached.registers)));
        let (sum, registers) = if let Some(hit) = hit {
            hit
        } else {
            let remember = cacheable && cached.is_none();
            let scan = if plan.call.results == 1 {
                plan.scan::<1>(self, bounds, step > 0, initial, remember)
            } else {
                plan.scan::<2>(self, bounds, step > 0, initial, remember)
            }?;
            if remember {
                self.array_reduction.cache_call(CachedCall {
                    key,
                    exact: scan.exact,
                    registers: scan.registers,
                });
                for id in watched {
                    self.tables[slot_index(id.0)].watch_array();
                }
            }
            (scan.sum, scan.registers)
        };
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
