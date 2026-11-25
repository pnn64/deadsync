//! Exact reductions of straight-line numeric loop bodies.

use super::{
    TableId, Value, Vm,
    math::{MATH_MAX, MATH_MIN},
    numeric_leaf::{Expression, Input},
    slot_index,
    threads::{CO_RESUME, inline_resume::ParkedIndex},
};
use crate::bytecode::{Instruction, OPEN_ARGS, Op, Prototype, RK};

mod while_numeric;

const EXACT_INTEGER: i128 = 1 << 53;
const BODY_LIMIT: usize = 32;

pub(super) enum Reduction {
    Applied(u64),
    Retry,
}

pub(super) struct Retry {
    pub frame: usize,
    pub pc: usize,
    pub attempts: u8,
}

fn integer(value: Value) -> Option<i128> {
    let Value::Number(value) = value else {
        return None;
    };
    (value.abs() <= EXACT_INTEGER as f64 && value.fract() == 0.0).then(|| i128::from(value as i64))
}

/// The endpoints of an affine sequence over the loop's actual indices.
/// Bounding both endpoints proves that every intermediate integer is exact.
#[derive(Clone, Copy)]
struct Series {
    first: i128,
    last: i128,
}

impl Series {
    fn sum(self, count: i128) -> i128 {
        count * (self.first + self.last) / 2
    }

    fn checked(self) -> Option<Self> {
        (self.first.abs() <= EXACT_INTEGER && self.last.abs() <= EXACT_INTEGER).then_some(self)
    }

    fn constant(value: Value) -> Option<Self> {
        let n = integer(value)?;
        Some(Self { first: n, last: n })
    }

    fn arithmetic(self, op: Op, right: Self) -> Option<Self> {
        let (first, last) = match op {
            Op::Add => (self.first + right.first, self.last + right.last),
            Op::Sub => (self.first - right.first, self.last - right.last),
            Op::Mul if self.first == self.last || right.first == right.last => {
                (self.first * right.first, self.last * right.last)
            }
            _ => return None,
        };
        Self { first, last }.checked()
    }

    fn negate(self) -> Self {
        Self {
            first: -self.first,
            last: -self.last,
        }
    }
}

#[derive(Clone, Copy)]
enum Operand {
    /// An original register value, potentially the loop-carried accumulator.
    Initial(usize),
    Constant(Value),
    Series(Series),
    Sum(usize, Series),
}

struct Body<'a> {
    vm: &'a Vm,
    registers: &'a [Value],
    constants: &'a [Value],
    control: usize,
    env: Option<TableId>,
    iterations: i128,
    bindings: [u8; 256],
    values: [Operand; BODY_LIMIT + 3],
    written: u8,
    read_initial: [bool; 256],
    invariant: [bool; 256],
    call_registers: Option<usize>,
    segment_limit: Option<i128>,
    parked: Option<(ParkedIndex, f64)>,
    wait_for_yield: bool,
    assertion: bool,
    field_writes: [TableId; BODY_LIMIT],
    write_count: usize,
}

impl Body<'_> {
    fn read(&mut self, register: usize) -> Option<Operand> {
        if (self.control..self.control + 3).contains(&register) {
            return None;
        }
        let binding = self.bindings[register];
        if binding != 0 {
            return Some(self.values[usize::from(binding)]);
        }
        self.read_initial[register] = true;
        Some(Operand::Initial(register))
    }

    fn write(&mut self, register: usize, value: Operand) {
        self.written += 1;
        self.bindings[register] = self.written;
        self.values[usize::from(self.written)] = value;
    }

    fn operand(&mut self, index: u32) -> Option<Operand> {
        if index < RK {
            self.read(index as usize)
        } else {
            Some(Operand::Constant(self.constants[(index - RK) as usize]))
        }
    }

    fn series(&mut self, operand: Operand) -> Option<Series> {
        match operand {
            Operand::Initial(index) => {
                self.invariant[index] = true;
                Series::constant(self.registers[index])
            }
            Operand::Constant(value) => Series::constant(value),
            Operand::Series(series) => Some(series),
            Operand::Sum(..) => None,
        }
    }

    fn literal(&mut self, operand: Operand) -> Option<Value> {
        match operand {
            Operand::Initial(index) => {
                self.invariant[index] = true;
                Some(self.registers[index])
            }
            Operand::Constant(value) => Some(value),
            _ => None,
        }
    }

    fn table(&mut self, register: usize) -> Option<TableId> {
        let operand = self.read(register)?;
        let Value::Table(id) = self.literal(operand)? else {
            return None;
        };
        Some(id)
    }

    fn field_read(&mut self, instruction: Instruction) -> Option<Operand> {
        let id = self.table(usize::from(instruction.b))?;
        let operand = self.operand(instruction.c)?;
        let key = self.literal(operand)?;
        let table = &self.vm.tables[slot_index(id.0)];
        let mut value = table.get(key);
        if value == Value::Nil {
            value = super::simple::inherited(
                &self.vm.tables,
                self.vm.meta_names[super::Meta::Index as usize],
                None,
                id,
                table.metatable()?,
                key,
                None,
            )?;
        }
        integer(value)?;
        Some(Operand::Constant(value))
    }

    fn global(&self, index: usize) -> Option<Operand> {
        let value = self.vm.tables[slot_index(self.env?.0)].get(self.constants[index]);
        (value != Value::Nil).then_some(Operand::Constant(value))
    }

    fn field_write(&mut self, instruction: Instruction) -> Option<()> {
        let id = self.table(usize::from(instruction.a))?;
        let key = self.operand(u32::from(instruction.b))?;
        let Value::String(key) = self.literal(key)? else {
            return None;
        };
        if self.vm.tables[slot_index(id.0)].get_string(key) == Value::Nil {
            return None;
        }
        let value = self.operand(instruction.c)?;
        self.series(value)?;
        self.field_writes[self.write_count] = id;
        self.write_count += 1;
        Some(())
    }

    fn resume(&mut self, call: usize) -> Option<Operand> {
        if self.parked.is_some() || !self.vm.sort_fast_context() {
            return None;
        }
        let thread = self.read(call + 1)?;
        let Value::Thread(thread) = self.literal(thread)? else {
            return None;
        };
        let Some(parked) = self.vm.parked_numeric_yield(thread) else {
            let state = &self.vm.threads[slot_index(thread.0)];
            self.wait_for_yield = state.status == super::threads::Status::Suspended
                && state.frames.is_empty()
                && !state.api_stack.is_empty();
            return None;
        };
        let index = integer(Value::Number(parked.index))?;
        let step = integer(Value::Number(parked.step))?;
        let limit = integer(Value::Number(parked.limit))?;
        let first = index + step;
        let last = index + step * self.iterations;
        if step == 0 || (step > 0 && last > limit) || (step < 0 && last < limit) {
            return None;
        }
        let values = Series { first, last }.checked()?;
        self.parked = Some((parked, (last - step) as f64));
        self.write(call + 1, Operand::Series(values));
        Some(Operand::Constant(Value::Boolean(true)))
    }

    fn extremum(&mut self, call: usize, count: usize, minimum: bool) -> Option<Operand> {
        if count == 0 {
            return None;
        }
        let first = self.read(call + 1)?;
        let mut selected = self.series(first)?;
        for index in call + 2..=call + count {
            let operand = self.read(index)?;
            let other = self.series(operand)?;
            let (first, last) = if minimum {
                (selected.first - other.first, selected.last - other.last)
            } else {
                (other.first - selected.first, other.last - selected.last)
            };
            if (first <= 0) != (last <= 0) {
                // Both inputs are affine in the iteration number. Split just
                // before their ordering changes so each segment is affine too.
                let slope = (last - first) / (self.iterations - 1);
                let boundary = if first <= 0 {
                    -first / slope + 1
                } else {
                    (first - 1) / -slope + 1
                };
                self.segment_limit = Some(boundary);
                return None;
            }
            if first > 0 {
                selected = other;
            }
        }
        self.call_registers.get_or_insert(0);
        Some(Operand::Series(selected))
    }

    fn call(&mut self, call: usize, count: usize, wanted: u32) -> Option<Operand> {
        if self.vm.budget.is_some() {
            return None;
        }
        let operand = self.read(call)?;
        let function = match self.literal(operand)? {
            Value::Function(function) if wanted == 1 => function,
            Value::Native(id @ (MATH_MIN | MATH_MAX)) if wanted == 1 => {
                return self.extremum(call, count, id == MATH_MIN);
            }
            Value::Native(CO_RESUME) if count == 1 && wanted == 2 => return self.resume(call),
            Value::Native(0) if count == 1 && wanted <= 1 => {
                let value = self.read(call + 1)?;
                let value = self.literal(value)?;
                if !value.is_truthy() {
                    return None;
                }
                self.assertion = true;
                return Some(Operand::Constant(value));
            }
            _ => return None,
        };
        let leaf = self.vm.numeric_leaf(function)?;
        let mut argument = |input| match input {
            Input::Constant(number) => Series::constant(Value::Number(number)),
            Input::Argument(index) if index < count => {
                let operand = self.read(call + 1 + index)?;
                self.series(operand)
            }
            Input::Argument(_) => None,
        };
        let result = match leaf.expression {
            Expression::Value(input) => argument(input)?,
            Expression::Negate(index) => argument(index)?.negate(),
            Expression::Binary(op, left, right) => {
                argument(left)?.arithmetic(op, argument(right)?)?
            }
        };
        self.call_registers = Some(self.call_registers.unwrap_or(0).max(leaf.registers));
        Some(Operand::Series(result))
    }

    fn add(
        &mut self,
        left: Operand,
        right: Operand,
        result_slots: &[bool; 256],
    ) -> Option<Operand> {
        let (register, prior, term) = match (left, right) {
            (Operand::Sum(register, prior), term) | (term, Operand::Sum(register, prior)) => {
                (register, Some(prior), term)
            }
            (Operand::Initial(register), term) if result_slots[register] => (register, None, term),
            (term, Operand::Initial(register)) if result_slots[register] => (register, None, term),
            _ => {
                return Some(Operand::Series(
                    self.series(left)?
                        .arithmetic(Op::Add, self.series(right)?)?,
                ));
            }
        };
        let term = self.series(term)?;
        let sign = term.first.signum();
        if sign == 0 || sign != term.last.signum() {
            return None;
        }
        let terms = if let Some(prior) = prior {
            if prior.first.signum() != sign {
                return None;
            }
            prior.arithmetic(Op::Add, term)?
        } else {
            term
        };
        Some(Operand::Sum(register, terms))
    }

    fn accumulator(
        &mut self,
        code: &[Instruction],
        writes: bool,
    ) -> Option<Option<(usize, Series)>> {
        let mut result_slots = [false; 256];
        if !writes {
            let addition = code.iter().rposition(|i| i.op == Op::Add)?;
            result_slots[usize::from(code[addition].a)] = true;
            for i in &code[addition + 1..] {
                match i.op {
                    Op::Close => {}
                    Op::Move if i.c == 0 => {
                        result_slots[usize::from(i.a)] = result_slots[usize::from(i.b)];
                    }
                    _ => result_slots[usize::from(i.a)] = false,
                }
            }
        }
        for i in code {
            // There is no open upvalue in this frame, and the accepted
            // instructions cannot create one.
            if i.op == Op::Close {
                continue;
            }
            if i.op == Op::SetTable {
                self.field_write(*i)?;
                continue;
            }
            let destination = usize::from(i.a);
            if (self.control..self.control + 3).contains(&destination) {
                return None;
            }
            let value = match i.op {
                Op::Load if i.b == 0 => Operand::Constant(self.constants[i.c as usize]),
                // Frame entry initialized these slots. The opcode is a no-op.
                Op::Load => continue,
                Op::Move if i.c == 0 => self.read(usize::from(i.b))?,
                Op::Add | Op::Sub | Op::Mul => {
                    let left = self.operand(u32::from(i.b))?;
                    let right = self.operand(i.c)?;
                    if i.op == Op::Add {
                        self.add(left, right, &result_slots)?
                    } else {
                        Operand::Series(self.series(left)?.arithmetic(i.op, self.series(right)?)?)
                    }
                }
                Op::Neg => {
                    let operand = self.read(usize::from(i.b))?;
                    Operand::Series(self.series(operand)?.negate())
                }
                Op::Call if i.b != OPEN_ARGS => self.call(destination, usize::from(i.b), i.c)?,
                Op::GetTable => self.field_read(*i)?,
                Op::GetGlobal => self.global(i.c as usize)?,
                _ => return None,
            };
            self.write(destination, value);
        }

        let sum =
            if writes {
                None
            } else {
                Some(self.bindings.iter().enumerate().find_map(
                    |(slot, &binding)| match self.values[usize::from(binding)] {
                        Operand::Sum(register, terms) if register == slot => {
                            Some((register, terms))
                        }
                        _ => None,
                    },
                )?)
            };
        if sum.is_some_and(|(register, _)| self.invariant[register])
            || self
                .read_initial
                .iter()
                .enumerate()
                .any(|(register, &read)| {
                    read && self.bindings[register] != 0
                        && !sum.is_some_and(|(sum, _)| sum == register)
                })
        {
            return None;
        }
        Some(sum)
    }
}

impl Vm {
    /// Advance complete iterations before the last one. The ordinary
    /// interpreter runs the final body and exit test, publishing the final
    /// scratch registers, visible index, stack top and error location.
    pub(super) fn reduce_numeric_loop(
        &mut self,
        base: usize,
        control: usize,
        loop_pc: usize,
        prototype: &Prototype,
        program: usize,
        prototype_index: usize,
    ) -> Option<Reduction> {
        if self.hooks.mask & (super::hooks::LINE | super::hooks::COUNT) != 0 {
            return None;
        }
        let code = &prototype.code;
        let loop_instruction = code[loop_pc];
        if loop_instruction.op != Op::ForLoop || usize::from(loop_instruction.a) != control {
            return None;
        }
        let body = code.get(loop_instruction.c as usize..loop_pc)?;
        let writes = body.iter().any(|i| i.op == Op::SetTable);
        if body.len() > BODY_LIMIT
            || control + 3 >= 256
            || (writes && !self.sort_fast_context())
            || body.iter().any(|i| {
                !matches!(
                    i.op,
                    Op::Load
                        | Op::Move
                        | Op::Add
                        | Op::Sub
                        | Op::Mul
                        | Op::Neg
                        | Op::Call
                        | Op::Close
                        | Op::GetTable
                        | Op::GetGlobal
                        | Op::SetTable
                ) || (writes && matches!(i.op, Op::GetTable | Op::GetGlobal | Op::Call))
            })
        {
            return None;
        }

        let start = base + control;
        let first = integer(self.registers[start])? + integer(self.registers[start + 2])?;
        let limit = integer(self.registers[start + 1])?;
        let step = integer(self.registers[start + 2])?;
        let count = if step > 0 && first <= limit {
            (limit - first) / step + 1
        } else if step < 0 && first >= limit {
            (first - limit) / -step + 1
        } else {
            return None;
        };
        if count < 2 {
            return None;
        }
        let last = first + (count - 1) * step;
        let indices = Series { first, last }.checked()?;
        if (last + step).abs() > EXACT_INTEGER {
            return None;
        }

        let close_from = body
            .iter()
            .filter(|i| i.op == Op::Close)
            .map(|i| usize::from(i.a))
            .min();
        let env = self.frames.last().and_then(|frame| match frame.function {
            super::FrameKind::Lua(function) => Some(self.functions[slot_index(function.0)].env),
            _ => None,
        });
        let mut remaining = count;
        let mut segment_count = count;
        let mut segment_first = indices.first;
        let mut running_sum = None;
        let mut segmented = false;
        let mut attempts = BODY_LIMIT * 2;
        let mut call_registers = None;
        let (sum_update, parked, assertion, field_writes, write_count) = loop {
            attempts = attempts.checked_sub(1)?;
            let segment_last = segment_first + (segment_count - 1) * step;
            let mut analysis = Body {
                vm: self,
                registers: &self.registers[base..],
                constants: self.programs[program].bound_constants(prototype_index),
                control,
                env,
                iterations: segment_count,
                bindings: [0; 256],
                values: [Operand::Initial(0); BODY_LIMIT + 3],
                written: 0,
                read_initial: [false; 256],
                invariant: [false; 256],
                call_registers: None,
                segment_limit: None,
                parked: None,
                wait_for_yield: false,
                assertion: false,
                field_writes: [TableId(0); BODY_LIMIT],
                write_count: 0,
            };
            analysis.write(
                control + 3,
                Operand::Series(Series {
                    first: segment_first,
                    last: segment_last,
                }),
            );
            let Some(accumulator) = analysis.accumulator(body, writes) else {
                if let Some(boundary) = analysis.segment_limit {
                    if !(1..segment_count).contains(&boundary) {
                        return None;
                    }
                    segment_count = boundary;
                    segmented = true;
                    continue;
                }
                return (!segmented && analysis.wait_for_yield).then_some(Reduction::Retry);
            };
            if segmented && (writes || analysis.parked.is_some() || analysis.assertion) {
                return None;
            }
            // A skipped instruction cannot change a captured register or close
            // an open upvalue. Unrelated captures remain live across the loop.
            if self.open_upvalues.iter().any(|&id| {
                let super::Cell::Open(_, register) = self.upvalues[id as usize] else {
                    return false;
                };
                let Some(register) = register.checked_sub(base) else {
                    return false;
                };
                (control..control + 4).contains(&register)
                    || analysis
                        .bindings
                        .get(register)
                        .is_some_and(|&binding| binding != 0)
                    || close_from.is_some_and(|first| register >= first)
            }) {
                return None;
            }
            call_registers = call_registers.max(analysis.call_registers);
            let partial = if let Some((register, terms)) = accumulator {
                let total: i128 = match running_sum {
                    Some((previous, total)) if previous == register => total,
                    Some(_) => return None,
                    None => integer(self.registers[base + register])?,
                };
                // Same-signed terms and accumulator make every partial sum monotone.
                let sum = terms.sum(segment_count);
                if (total != 0 && total.signum() != terms.first.signum())
                    || (total + sum).abs() > EXACT_INTEGER
                {
                    return None;
                }
                Some((register, total + sum, terms.last))
            } else {
                None
            };
            if segment_count == remaining {
                break (
                    partial
                        .map(|(register, total, last_term)| (register, (total - last_term) as f64)),
                    analysis.parked,
                    analysis.assertion,
                    analysis.field_writes,
                    analysis.write_count,
                );
            }
            running_sum = partial.map(|(register, total, _)| (register, total));
            remaining -= segment_count;
            segment_first = segment_last + step;
            segment_count = remaining;
        };
        let iteration_cost = body.len() as u64 + 1;
        let count = u64::try_from(count).ok()?;
        let full_cost = count.checked_mul(iteration_cost)?.checked_add(1)?;
        if self.budget.is_some() && self.remaining < full_cost {
            return None;
        }
        if call_registers.is_some_and(|registers| !self.numeric_call_ready(registers)) {
            return self.sort_fast_context().then_some(Reduction::Retry);
        }
        if (parked.is_some() || assertion)
            && !(self.sort_fast_context() && self.resume_has_spare_stack() && self.sort_gc_ready())
        {
            return self.sort_fast_context().then_some(Reduction::Retry);
        }
        if writes && self.gc.pending && !self.sort_gc_ready() {
            return Some(Reduction::Retry);
        }
        // Each proven store replaces a live field with a number. The final
        // iteration publishes its value. Track every omitted overwrite in
        // the table version so host-side caches see the same write history.
        for table in &field_writes[..write_count] {
            self.tables[slot_index(table.0)].advance_fields_version((count - 1) as u32);
        }
        if let Some((parked, index)) = parked {
            self.advance_parked_yield(parked, index);
        }
        self.registers[start] = Value::Number((last - step) as f64);
        if let Some((register, number)) = sum_update {
            self.registers[base + register] = Value::Number(number);
        }
        Some(Reduction::Applied((count - 1) * iteration_cost))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_exact_affine_sums_to_the_last_iteration() {
        for (source, expected) in [
            (
                &b"local s=0; for i=1,100 do s=s+i end; return s"[..],
                4950.0,
            ),
            (
                &b"local s=0; for i=1,100 do s=s+(i+2)*3 end; return s"[..],
                15444.0,
            ),
        ] {
            let program = crate::compile(source).unwrap();
            let proto_index = program.data.entry as usize;
            let prototype = &program.data.prototypes[proto_index];
            let prep = prototype.code.iter().find(|i| i.op == Op::ForPrep).unwrap();
            let mut vm = Vm::new();
            let function = vm.load_program(&program);
            let program_index = vm.functions[super::super::slot_index(function.0)].program;
            let control = usize::from(prep.a);
            vm.registers
                .resize(usize::from(prototype.registers), Value::Nil);
            vm.registers[0] = Value::Number(0.0);
            vm.registers[control] = Value::Number(0.0);
            vm.registers[control + 1] = Value::Number(100.0);
            vm.registers[control + 2] = Value::Number(1.0);
            assert!(matches!(
                vm.reduce_numeric_loop(
                    0,
                    control,
                    prep.c as usize,
                    prototype,
                    program_index,
                    proto_index
                ),
                Some(Reduction::Applied(_))
            ));
            assert_eq!(vm.registers[control], Value::Number(99.0));
            assert_eq!(vm.registers[0], Value::Number(expected));
        }
    }
}
