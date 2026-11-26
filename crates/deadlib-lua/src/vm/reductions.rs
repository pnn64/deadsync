//! Exact reductions of numeric loops with a proven instruction path.

use super::{
    Cell, FrameKind, FunctionId, TableId, Value, Vm, forwarding,
    math::{MATH_MAX, MATH_MIN},
    numeric_leaf::{Expression, Input},
    slot_index,
    strings::STR_SUB,
    threads::{CO_RESUME, inline_resume::ParkedIndex},
};
use crate::bytecode::{COMPARE_BRANCH, Instruction, OPEN_ARGS, Op, Prototype, RK};

mod dense_array;
pub(super) use dense_array::dense_loop_store;
mod periodic;
mod substring;
pub(super) mod while_numeric;

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
        Some(Self::splat(integer(value)?))
    }

    fn splat(value: i128) -> Self {
        Self {
            first: value,
            last: value,
        }
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
    Substring(Series),
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
    forwarded_calls_fit: Option<bool>,
    segment_limit: Option<i128>,
    period: Option<i128>,
    parked: Option<(ParkedIndex, f64)>,
    wait_for_yield: bool,
    assertion: bool,
    field_writes: [TableId; BODY_LIMIT],
    write_count: usize,
}

impl<'a> Body<'a> {
    fn new(
        vm: &'a Vm,
        registers: &'a [Value],
        constants: &'a [Value],
        control: usize,
        env: Option<TableId>,
        indices: Series,
        iterations: i128,
    ) -> Option<Self> {
        let mut body = Self {
            vm,
            registers,
            constants,
            control,
            env,
            iterations,
            bindings: [0; 256],
            values: [Operand::Initial(0); BODY_LIMIT + 3],
            written: 0,
            read_initial: [false; 256],
            invariant: [false; 256],
            call_registers: None,
            forwarded_calls_fit: None,
            segment_limit: None,
            period: None,
            parked: None,
            wait_for_yield: false,
            assertion: false,
            field_writes: [TableId(0); BODY_LIMIT],
            write_count: 0,
        };
        body.write(control + 3, Operand::Series(indices))?;
        Some(body)
    }

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

    fn write(&mut self, register: usize, value: Operand) -> Option<()> {
        let binding = &mut self.bindings[register];
        if *binding == 0 {
            // The analysis only needs the latest value of each register.
            self.written += 1;
            *binding = self.written;
        }
        *self.values.get_mut(usize::from(*binding))? = value;
        Some(())
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
            Operand::Sum(..) | Operand::Substring(_) => None,
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

    fn closed_upvalue(&self, index: usize) -> Option<Operand> {
        let FrameKind::Lua(function) = self.vm.frames.last()?.function else {
            return None;
        };
        let cell = self.vm.functions[slot_index(function.0)].upvalues[index];
        let Cell::Closed(value) = self.vm.upvalues[cell as usize] else {
            return None;
        };
        Some(Operand::Constant(value))
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
        self.write(call + 1, Operand::Series(values))?;
        Some(Operand::Constant(Value::Boolean(true)))
    }

    fn le_throughout(&mut self, left: Series, right: Series) -> Option<bool> {
        let first = left.first - right.first;
        let last = left.last - right.last;
        if (first <= 0) == (last <= 0) {
            return Some(first <= 0);
        }
        // A change in the ordering of affine inputs starts a new segment.
        let slope = (last - first) / (self.iterations - 1);
        let boundary = if first <= 0 {
            -first / slope + 1
        } else {
            (first - 1) / -slope + 1
        };
        self.segment_limit = Some(boundary);
        None
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
            let keep = if minimum {
                self.le_throughout(selected, other)?
            } else {
                self.le_throughout(other, selected)?
            };
            if !keep {
                selected = other;
            }
        }
        self.call_registers.get_or_insert(0);
        Some(Operand::Series(selected))
    }

    fn call(&mut self, call: usize, count: usize, wanted: u32) -> Option<Operand> {
        if self.vm.budget.is_some()
            || (1..=forwarding::MAX_VALUES as u32).contains(&wanted)
                && (call + 1..call + wanted as usize)
                    .any(|output| (self.control..self.control + 3).contains(&output))
        {
            return None;
        }
        let operand = self.read(call)?;
        match self.literal(operand)? {
            Value::Function(function) => self.lua_call(function, call, count, wanted as usize),
            Value::Native(id @ (MATH_MIN | MATH_MAX)) if wanted == 1 => {
                self.extremum(call, count, id == MATH_MIN)
            }
            Value::Native(STR_SUB) if matches!(count, 2 | 3) && wanted == 1 => {
                self.substring(call, count)
            }
            Value::Native(CO_RESUME) if count == 1 && wanted == 2 => self.resume(call),
            Value::Native(0) if count == 1 && wanted <= 1 => {
                let value = self.read(call + 1)?;
                let value = self.literal(value)?;
                if !value.is_truthy() {
                    return None;
                }
                self.assertion = true;
                Some(Operand::Constant(value))
            }
            _ => None,
        }
    }

    fn lua_call(
        &mut self,
        function: FunctionId,
        call: usize,
        count: usize,
        wanted: usize,
    ) -> Option<Operand> {
        if !matches!(wanted, 1 | 2) {
            return self.forwarded_call(function, call, count, wanted);
        }
        let Some(leaf) = self.vm.numeric_leaf(function) else {
            return self.forwarded_call(function, call, count, wanted);
        };
        let first = self.call_expression(leaf.first, call, count)?;
        let second = if let Some(expression) = leaf.second {
            Operand::Series(self.call_expression(expression, call, count)?)
        } else {
            Operand::Constant(Value::Nil)
        };
        if wanted == 2 {
            self.write(call + 1, second)?;
        }
        self.call_registers = Some(self.call_registers.unwrap_or(0).max(leaf.registers));
        Some(Operand::Series(first))
    }

    fn forwarded_call(
        &mut self,
        function: FunctionId,
        call: usize,
        count: usize,
        wanted: usize,
    ) -> Option<Operand> {
        if !(1..=forwarding::MAX_VALUES).contains(&wanted) {
            return None;
        }
        let plan = forwarding::Plan::build(self.vm, function, count)?;
        let fits = plan.has_stack_space(self.vm);
        self.forwarded_calls_fit = Some(self.forwarded_calls_fit.unwrap_or(true) && fits);
        let nil = Operand::Constant(Value::Nil);
        let mut results = [nil; forwarding::MAX_VALUES];
        for (result, &source) in results.iter_mut().zip(plan.sources(wanted)) {
            *result = match source {
                forwarding::Source::Argument(index) => self.read(call + 1 + index)?,
                forwarding::Source::Constant(value) => Operand::Constant(value),
                forwarding::Source::Capture(_) => return None,
            };
        }
        for (index, &value) in results[1..wanted].iter().enumerate() {
            self.write(call + 1 + index, value)?;
        }
        Some(results[0])
    }

    fn is_nil(&mut self, operand: Operand) -> Option<bool> {
        match operand {
            Operand::Series(_) => Some(false),
            _ => self.literal(operand).map(|value| value == Value::Nil),
        }
    }

    fn nil_comparison(&mut self, i: Instruction) -> Option<Operand> {
        let left = self.operand(u32::from(i.b))?;
        let right = self.operand(i.c)?;
        let left = self.is_nil(left)?;
        let right = self.is_nil(right)?;
        // A comparison with nil is independent of the other value's type
        // and cannot invoke an equality metamethod.
        (left || right).then_some(Operand::Constant(Value::Boolean((left == right) == (i.op == Op::Eq))))
    }

    fn call_expression(
        &mut self,
        expression: Expression,
        call: usize,
        count: usize,
    ) -> Option<Series> {
        let mut argument = |input| match input {
            Input::Constant(number) => Series::constant(Value::Number(number)),
            Input::Argument(index) if index < count => {
                let operand = self.read(call + 1 + index)?;
                self.series(operand)
            }
            Input::Argument(_) => None,
        };
        match expression {
            Expression::Value(input) => argument(input),
            Expression::Negate(index) => Some(argument(index)?.negate()),
            Expression::Binary(op, left, right) => argument(left)?.arithmetic(op, argument(right)?),
        }
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
        start_pc: usize,
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
        let mut offset = 0;
        while let Some(i) = code.get(offset) {
            offset += 1;
            if matches!(i.op, Op::JumpFalse | Op::JumpTrue) {
                let comparison = code.get(offset.checked_sub(2)?)?;
                if i.b != COMPARE_BRANCH
                    || !matches!(comparison.op, Op::Eq | Op::Ne)
                    || comparison.a != i.a
                {
                    return None;
                }
                let target = (i.c as usize).checked_sub(start_pc)?;
                if !(offset..=code.len()).contains(&target) {
                    return None;
                }
                let guard = self.read(usize::from(i.a))?;
                if self.literal(guard)?.is_truthy() == (i.op == Op::JumpTrue) {
                    offset = target;
                }
                continue;
            }
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
                Op::Add | Op::Sub | Op::Mul | Op::Mod => {
                    let left = self.operand(u32::from(i.b))?;
                    let right = self.operand(i.c)?;
                    if i.op == Op::Add {
                        self.add(left, right, &result_slots)?
                    } else if i.op == Op::Mod {
                        let numerator = self.series(left)?;
                        let divisor = self.series(right)?;
                        Operand::Series(self.remainder(numerator, divisor)?)
                    } else {
                        Operand::Series(self.series(left)?.arithmetic(i.op, self.series(right)?)?)
                    }
                }
                Op::Neg => {
                    let operand = self.read(usize::from(i.b))?;
                    Operand::Series(self.series(operand)?.negate())
                }
                Op::Eq | Op::Ne => self.nil_comparison(*i)?,
                Op::Call if i.b != OPEN_ARGS => self.call(destination, usize::from(i.b), i.c)?,
                Op::GetTable => self.field_read(*i)?,
                Op::GetGlobal => self.global(i.c as usize)?,
                Op::GetUpvalue => self.closed_upvalue(i.c as usize)?,
                Op::Len => {
                    let Operand::Substring(length) = self.read(usize::from(i.b))? else {
                        return None;
                    };
                    Operand::Series(length)
                }
                _ => return None,
            };
            self.write(destination, value)?;
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

fn add_exact_sum(total: i128, terms: Series, count: i128) -> Option<i128> {
    // Same-signed terms and accumulator make every partial sum monotone.
    if total != 0 && total.signum() != terms.first.signum() {
        return None;
    }
    let total = total + terms.sum(count);
    (total.abs() <= EXACT_INTEGER).then_some(total)
}

impl Vm {
    /// Advance complete iterations with proven effects. Reductions with
    /// observable scratch registers leave the last body to ordinary dispatch.
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
        let start_pc = loop_instruction.c as usize;
        let body = code.get(start_pc..loop_pc)?;
        if let Some(reduction) = self.reduce_dense_array_write(base, control, body, prototype) {
            return Some(reduction);
        }
        if let Some(reduction) =
            self.reduce_branched_array_calls(base, control, start_pc, loop_pc, prototype)
        {
            return Some(reduction);
        }
        if body.len() <= BODY_LIMIT && body.iter().any(|i| i.op == Op::GetTable) {
            if let Some(skipped) =
                self.reduce_array_loop(base, control, body, program, prototype_index)
            {
                return Some(Reduction::Applied(skipped));
            }
            if let Some(reduction) =
                self.reduce_array_calls(base, control, body, program, prototype_index)
            {
                return Some(reduction);
            }
        }
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
                        | Op::Mod
                        | Op::Neg
                        | Op::Call
                        | Op::Close
                        | Op::GetTable
                        | Op::GetGlobal
                        | Op::GetUpvalue
                        | Op::Len
                        | Op::SetTable
                        | Op::Eq
                        | Op::Ne
                        | Op::JumpFalse
                        | Op::JumpTrue
                ) || (writes
                    && matches!(
                        i.op,
                        Op::GetTable | Op::GetGlobal | Op::GetUpvalue | Op::Call | Op::Len
                    ))
                    || (matches!(i.op, Op::JumpFalse | Op::JumpTrue)
                        && (writes || self.budget.is_some()))
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
        let mut forwarded_calls_fit = None;
        let (sum_update, parked, assertion, field_writes, write_count) = loop {
            attempts = attempts.checked_sub(1)?;
            let segment_last = segment_first + (segment_count - 1) * step;
            let mut analysis = Body::new(
                self,
                &self.registers[base..],
                self.programs[program].bound_constants(prototype_index),
                control,
                env,
                Series {
                    first: segment_first,
                    last: segment_last,
                },
                segment_count,
            )?;
            let Some(accumulator) = analysis.accumulator(body, start_pc, writes) else {
                if let Some(boundary) = analysis.segment_limit {
                    if !(1..segment_count).contains(&boundary) {
                        return None;
                    }
                    segment_count = boundary;
                    segmented = true;
                    continue;
                }
                if !segmented
                    && !writes
                    && let Some(period) = analysis.period
                    && self.sort_fast_context()
                    && self.open_upvalues.is_empty()
                {
                    let (sum, registers) =
                        analysis.periodic_sum(body, start_pc, indices, step, count, period)?;
                    call_registers = registers;
                    break (Some(sum), None, false, analysis.field_writes, 0);
                }
                return (!segmented && analysis.wait_for_yield).then_some(Reduction::Retry);
            };
            if segmented && (writes || analysis.parked.is_some() || analysis.assertion) {
                return None;
            }
            if let Some(fits) = analysis.forwarded_calls_fit {
                if analysis.parked.is_some() {
                    return None;
                }
                forwarded_calls_fit = Some(forwarded_calls_fit.unwrap_or(true) && fits);
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
                Some((register, add_exact_sum(total, terms, segment_count)?, terms.last))
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
        if let Some(fits) = forwarded_calls_fit {
            if !self.sort_fast_context() {
                return None;
            }
            if !fits || !self.sort_gc_ready() {
                return Some(Reduction::Retry);
            }
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
