//! Scalar evaluation of straight-line numeric extrema accumulations.
use super::{BODY_LIMIT, EXACT_INTEGER, MATH_MAX, MATH_MIN, Series, Value, Vm};
use crate::bytecode::{COMPARE_BRANCH, Instruction, Op, RK};

mod array;

const MAX_TERMS: usize = 8;
const MAX_ITERATIONS: usize = 16_384;
fn progression_sum(first: i128, step: i128, count: i128) -> i128 {
    Series {
        first,
        last: first + (count - 1) * step,
    }
    .sum(count)
}

fn sum_extremum(minimum: bool, threshold: i128, low: i128, step: i128, count: i128) -> i128 {
    let below = if threshold >= low {
        ((threshold - low) / step + 1).min(count)
    } else {
        0
    };
    let lower_sum = progression_sum(low, step, below);
    if minimum {
        lower_sum + (count - below) * threshold
    } else {
        progression_sum(low, step, count) - lower_sum + below * threshold
    }
}

/// Put finite binary64 inputs on a common integer grid by multiplying by an
/// exact power of two. Very fine grids use the scalar path.
fn integer_scale(values: [f64; 6]) -> Option<f64> {
    let mut shift = 0;
    for value in values {
        let bits = value.abs().to_bits();
        if bits == 0 {
            continue;
        }
        let exponent = (bits >> 52) as i32;
        if exponent == 0 || exponent == 2047 {
            return None;
        }
        let significand = (bits & ((1 << 52) - 1)) | (1 << 52);
        shift = shift.max(1075 - exponent - significand.trailing_zeros() as i32);
    }
    (shift <= 52).then(|| f64::from_bits(((1023 + shift) as u64) << 52))
}

fn scaled_integer(value: f64, scale: f64, limit: i128) -> Option<i128> {
    let value = value * scale;
    (value.abs() <= limit as f64).then(|| i128::from(value as i64))
}

#[derive(Clone, Copy)]
struct Progression {
    first: i128,
    step: i128,
}
impl Progression {
    fn split(self, threshold: i128, count: i128) -> i128 {
        let distance = if self.step > 0 {
            threshold - self.first
        } else {
            self.first - threshold
        };
        (distance.div_euclid(self.step.abs()) + 1).clamp(0, count)
    }

    fn extremum(self, minimum: bool, threshold: i128) -> Self {
        let below = if self.step > 0 {
            self.first <= threshold
        } else {
            self.first < threshold
        };
        if below == minimum {
            self
        } else {
            Self {
                first: threshold,
                step: 0,
            }
        }
    }
}

/// In the binary64 range with spacing two, an odd addend is a tie. It
/// rounds by one toward the even grid coordinate and resets its parity.
fn rounding_adjustment(terms: [Progression; 2], count: i128, sum: i128) -> i128 {
    let mut parity = (sum >> 1) & 1;
    let mut measure = |length| {
        let mut adjustment = 0;
        for index in 0..length {
            for term in terms {
                let low = (term.first + index * term.step) & 3;
                parity ^= low >> 1;
                if low & 1 != 0 {
                    adjustment += 2 * parity - 1;
                    parity = 0;
                }
            }
        }
        adjustment
    };
    let periods = count / 4;
    let mut adjustment = 0;
    if periods != 0 {
        adjustment += measure(4);
        if periods > 1 {
            // The low bits repeat every four iterations. A tie makes the
            // ending parity independent of the starting parity. A period
            // without ties needs no adjustment, even in the partial tail.
            adjustment += (periods - 1) * measure(4);
        }
    }
    adjustment + measure(count % 4)
}

fn rounded_pair(
    index: Progression,
    count: i128,
    mut sum: i128,
    (first_min, first): (bool, i128),
    (second_min, second): (bool, i128),
) -> Option<i128> {
    let end = index.first + (count - 1) * index.step;
    let lowest = index.first.min(end).min(first).min(second);
    let highest = index.first.max(end).max(first).max(second);
    let lower = sum + 2 * count * if lowest < 0 { lowest - 1 } else { 0 };
    let upper = sum + 2 * count * if highest > 0 { highest + 1 } else { 0 };
    let within_grid = if sum >= 0 {
        lower >= EXACT_INTEGER && upper < 2 * EXACT_INTEGER
    } else {
        lower > -2 * EXACT_INTEGER && upper <= -EXACT_INTEGER
    };
    if !within_grid {
        return None;
    }
    let a = index.split(first, count);
    let b = index.split(second, count);
    let mut start = 0;
    for stop in [a.min(b), a.max(b), count] {
        let count = stop - start;
        if count != 0 {
            let indices = Progression {
                first: index.first + start * index.step,
                step: index.step,
            };
            let terms = [
                indices.extremum(first_min, first),
                indices.extremum(second_min, second),
            ];
            sum += rounding_adjustment(terms, count, sum)
                + progression_sum(terms[0].first, terms[0].step, count)
                + progression_sum(terms[1].first, terms[1].step, count);
        }
        start = stop;
    }
    Some(sum)
}

#[derive(Clone, Copy)]
enum Input {
    // The iteration's index or the number read at that index.
    Current,
    Number(f64),
}
impl Input {
    fn value(self, current: f64) -> f64 {
        match self {
            Self::Current => current,
            Self::Number(value) => value,
        }
    }
}

#[derive(Clone, Copy)]
enum Term {
    Input(Input),
    Extremum {
        first: Input,
        second: Input,
        minimum: bool,
    },
}
impl Term {
    fn index_extremum(self) -> Option<(bool, f64)> {
        match self {
            Self::Extremum {
                first: Input::Current,
                second: Input::Number(number),
                minimum,
            }
            | Self::Extremum {
                first: Input::Number(number),
                second: Input::Current,
                minimum,
            } if number.is_finite() && number != 0.0 => Some((minimum, number)),
            _ => None,
        }
    }

    fn value(self, current: f64) -> f64 {
        match self {
            Self::Input(input) => input.value(current),
            Self::Extremum {
                first,
                second,
                minimum,
            } => ordered_extremum(first.value(current), second.value(current), minimum),
        }
    }
}

#[inline(always)]
fn ordered_extremum(first: f64, second: f64, minimum: bool) -> f64 {
    // Ties and unordered inputs retain the first argument.
    let replace = if minimum {
        second < first
    } else {
        second > first
    };
    if replace { second } else { first }
}

#[derive(Clone, Copy)]
enum Symbol {
    Initial(usize),
    Tracked(Term),
    Fixed(Value),
    Sum(usize),
    LoopIndex,
    LoopControl,
}

struct Registers<'a> {
    initial: &'a [Value],
    constants: &'a [Value],
    symbols: [Option<Symbol>; 256],
    read_initial: [bool; 256],
    invariant: [bool; 256],
}
impl Registers<'_> {
    fn read(&mut self, operand: u32) -> Symbol {
        if operand >= RK {
            return Symbol::Fixed(self.constants[(operand - RK) as usize]);
        }
        let slot = operand as usize;
        self.symbols[slot].unwrap_or_else(|| {
            self.read_initial[slot] = true;
            Symbol::Initial(slot)
        })
    }

    fn fixed(&mut self, symbol: Symbol) -> Option<Value> {
        match symbol {
            Symbol::Initial(slot) => {
                self.invariant[slot] = true;
                Some(self.initial[slot])
            }
            Symbol::Fixed(value) => Some(value),
            _ => None,
        }
    }

    fn input(&mut self, symbol: Symbol) -> Option<Input> {
        if let Symbol::Tracked(Term::Input(input)) = symbol {
            return Some(input);
        }
        let Value::Number(value) = self.fixed(symbol)? else {
            return None;
        };
        Some(Input::Number(value))
    }

    fn invariant_number(&mut self, operand: u32) -> Option<f64> {
        let symbol = self.read(operand);
        let Value::Number(value) = self.fixed(symbol)? else {
            return None;
        };
        Some(value)
    }
}

struct Plan {
    index: usize,
    sum: usize,
    comparison: Op,
    bound: f64,
    step: f64,
    terms: [Term; MAX_TERMS],
    count: usize,
}

struct Sum {
    terms: [Term; MAX_TERMS],
    count: usize,
    register: Option<usize>,
    calls: usize,
}

impl Sum {
    fn new() -> Self {
        Self {
            terms: [Term::Input(Input::Number(0.0)); MAX_TERMS],
            count: 0,
            register: None,
            calls: 0,
        }
    }

    fn step(&mut self, registers: &mut Registers<'_>, i: &Instruction) -> Option<Symbol> {
        Some(match i.op {
            Op::Move if i.c == 0 => registers.read(u32::from(i.b)),
            Op::Load if i.b == 0 => Symbol::Fixed(registers.constants[i.c as usize]),
            Op::Call if i.b == 2 && i.c == 1 => {
                let destination = u32::from(i.a);
                let function = registers.read(destination);
                let Value::Native(id @ (MATH_MIN | MATH_MAX)) = registers.fixed(function)? else {
                    return None;
                };
                let first = registers.read(destination + 1);
                let second = registers.read(destination + 2);
                self.calls += 1;
                Symbol::Tracked(Term::Extremum {
                    first: registers.input(first)?,
                    second: registers.input(second)?,
                    minimum: id == MATH_MIN,
                })
            }
            Op::Add if self.count < MAX_TERMS => {
                let left = registers.read(u32::from(i.b));
                let right = registers.read(i.c);
                match left {
                    Symbol::Initial(slot) if self.count == 0 => self.register = Some(slot),
                    Symbol::Sum(previous) if previous == self.count => {}
                    _ => return None,
                }
                self.terms[self.count] = match right {
                    Symbol::Tracked(term) => term,
                    value => Term::Input(registers.input(value)?),
                };
                self.count += 1;
                Symbol::Sum(self.count)
            }
            _ => return None,
        })
    }

    fn register(&self, registers: &Registers<'_>) -> Option<usize> {
        let sum = self.register?;
        if !matches!(registers.symbols[sum], Some(Symbol::Sum(size)) if size == self.count)
            || self.calls == 0
            || self.calls
                != self.terms[..self.count]
                    .iter()
                    .filter(|t| matches!(t, Term::Extremum { .. }))
                    .count()
            || registers
                .read_initial
                .iter()
                .enumerate()
                .any(|(slot, &read)| {
                    read && (slot != sum || registers.invariant[slot])
                        && registers.symbols[slot].is_some()
                })
        {
            return None;
        }
        Some(sum)
    }
}

impl Plan {
    fn decode(
        code: &[Instruction],
        exit: usize,
        initial: &[Value],
        constants: &[Value],
    ) -> Option<Self> {
        let [compare, branch, update, body @ ..] = code else {
            return None;
        };
        if body.len() > BODY_LIMIT
            || branch.op != Op::JumpFalse
            || branch.b != COMPARE_BRANCH
            || branch.a != compare.a
            || branch.c as usize != exit
            || compare.a == update.a
        {
            return None;
        }
        let index = usize::from(update.a);
        let counter = u32::from(update.a);
        let (step_operand, negate_step) = match update.op {
            Op::Add if u32::from(update.b) == counter => (update.c, false),
            Op::Add if update.c == counter => (u32::from(update.b), false),
            Op::Sub if u32::from(update.b) == counter => (update.c, true),
            _ => return None,
        };
        let (comparison, bound_operand) = if u32::from(compare.b) == counter {
            (compare.op, compare.c)
        } else if compare.c == counter {
            let opposite = match compare.op {
                Op::Lt => Op::Gt,
                Op::Le => Op::Ge,
                Op::Gt => Op::Lt,
                Op::Ge => Op::Le,
                _ => return None,
            };
            (opposite, u32::from(compare.b))
        } else {
            return None;
        };
        if step_operand == counter
            || bound_operand == counter
            || !matches!(comparison, Op::Lt | Op::Le | Op::Gt | Op::Ge)
        {
            return None;
        }
        let mut registers = Registers {
            initial,
            constants,
            symbols: [None; 256],
            read_initial: [false; 256],
            invariant: [false; 256],
        };
        let step = registers.invariant_number(step_operand)?;
        if !step.is_finite() || step == 0.0 {
            return None;
        }
        let bound = registers.invariant_number(bound_operand)?;
        registers.symbols[index] = Some(Symbol::Tracked(Term::Input(Input::Current)));
        // A comparison writes this temporary before each body invocation.
        // It is usable only after being overwritten by a supported operation.
        let comparison_slot = usize::from(compare.a);
        registers.symbols[comparison_slot] = Some(Symbol::Fixed(Value::Boolean(true)));
        let mut sum = Sum::new();
        for instruction in body {
            let destination = usize::from(instruction.a);
            if instruction.op == Op::Close || instruction.op == Op::Load && instruction.b == 1 {
                continue;
            }
            if destination == index {
                return None;
            }
            let symbol = sum.step(&mut registers, instruction)?;
            registers.symbols[destination] = Some(symbol);
        }
        let sum_register = sum.register(&registers)?;
        Some(Self {
            index,
            sum: sum_register,
            comparison,
            bound,
            step: if negate_step { -step } else { step },
            terms: sum.terms,
            count: sum.count,
        })
    }

    fn test(&self, index: f64) -> bool {
        match self.comparison {
            Op::Lt => index < self.bound,
            Op::Le => index <= self.bound,
            Op::Gt => index > self.bound,
            Op::Ge => index >= self.bound,
            _ => unreachable!("ordered numeric comparison"),
        }
    }

    fn advance(
        &self,
        mut index: f64,
        mut sum: f64,
        evaluate: impl Fn(f64, f64) -> f64,
    ) -> (f64, f64) {
        if self.test(index) {
            for _ in 0..MAX_ITERATIONS {
                let next = index + self.step;
                // The ordinary interpreter publishes the final scratch and
                // stack slots on the last iteration.
                if next == index || !self.test(next) {
                    break;
                }
                index = next;
                sum = evaluate(index, sum);
            }
        }
        (index, sum)
    }

    fn advance_pair<const FIRST_MIN: bool, const SECOND_MIN: bool>(
        &self,
        index: f64,
        sum: f64,
        first: f64,
        second: f64,
    ) -> (f64, f64) {
        // The guard has already ruled out a NaN index. Nonzero finite
        // thresholds let the pair ignore operand order on equality.
        self.advance(index, sum, |index, sum| {
            let first = if if FIRST_MIN {
                index < first
            } else {
                index > first
            } {
                index
            } else {
                first
            };
            let second = if if SECOND_MIN {
                index < second
            } else {
                index > second
            } {
                index
            } else {
                second
            };
            (sum + first) + second
        })
    }

    fn advance_exact_pair(
        &self,
        index: f64,
        sum: f64,
        (first_min, first): (bool, f64),
        (second_min, second): (bool, f64),
    ) -> Option<(f64, f64)> {
        if !self.test(index) {
            return Some((index, sum));
        }
        let scale = integer_scale([index, self.bound, self.step, sum, first, second])?;
        let index = scaled_integer(index, scale, EXACT_INTEGER)?;
        let bound = scaled_integer(self.bound, scale, EXACT_INTEGER)?;
        let step = scaled_integer(self.step, scale, EXACT_INTEGER)?;
        let sum = scaled_integer(sum, scale, 2 * EXACT_INTEGER)?;
        let first = scaled_integer(first, scale, EXACT_INTEGER)?;
        let second = scaled_integer(second, scale, EXACT_INTEGER)?;
        debug_assert_ne!(step, 0);
        let distance = match (self.comparison, step > 0) {
            (Op::Lt, true) => bound - index - 1,
            (Op::Le, true) => bound - index,
            (Op::Gt, false) => index - bound - 1,
            (Op::Ge, false) => index - bound,
            _ => step.abs() * MAX_ITERATIONS as i128,
        };
        let count = (distance / step.abs()).min(MAX_ITERATIONS as i128);
        if count == 0 {
            return None;
        }
        let start = index + step;
        let end = index + count * step;
        let largest = start
            .abs()
            .max(end.abs())
            .max(first.abs())
            .max(second.abs());
        if largest > EXACT_INTEGER {
            return None;
        }
        let sum = if sum.abs() + 2 * count * largest <= EXACT_INTEGER {
            // The scaled additions are all exactly representable.
            let low = start.min(end);
            let step = step.abs();
            sum + sum_extremum(first_min, first, low, step, count)
                + sum_extremum(second_min, second, low, step, count)
        } else {
            rounded_pair(
                Progression { first: start, step },
                count,
                sum,
                (first_min, first),
                (second_min, second),
            )?
        };
        Some((end as f64 / scale, sum as f64 / scale))
    }

    fn evaluate(&self, index: f64, sum: f64) -> (f64, f64) {
        let terms = &self.terms[..self.count];
        if let [first, second] = terms
            && let Some((first_min, first)) = first.index_extremum()
            && let Some((second_min, second)) = second.index_extremum()
        {
            if let Some(result) =
                self.advance_exact_pair(index, sum, (first_min, first), (second_min, second))
            {
                return result;
            }
            return match (first_min, second_min) {
                (true, true) => self.advance_pair::<true, true>(index, sum, first, second),
                (true, false) => self.advance_pair::<true, false>(index, sum, first, second),
                (false, true) => self.advance_pair::<false, true>(index, sum, first, second),
                (false, false) => self.advance_pair::<false, false>(index, sum, first, second),
            };
        }
        self.advance(index, sum, |index, sum| {
            terms.iter().fold(sum, |sum, term| sum + term.value(index))
        })
    }
}

impl Vm {
    pub(in crate::vm) fn run_numeric_while(
        &mut self,
        code: &[Instruction],
        head: usize,
        edge: usize,
        base: usize,
        program: usize,
        prototype: usize,
    ) -> bool {
        if !self.sort_fast_context() {
            return false;
        }
        let Some(plan) = Plan::decode(
            &code[head..edge],
            edge + 1,
            &self.registers[base..],
            self.programs[program].bound_constants(prototype),
        ) else {
            return false;
        };
        let (Value::Number(index), Value::Number(sum)) = (
            self.registers[base + plan.index],
            self.registers[base + plan.sum],
        ) else {
            return false;
        };
        if !self.numeric_call_ready(0) {
            return false;
        }
        let (next, sum) = plan.evaluate(index, sum);
        if next.to_bits() == index.to_bits() {
            return false;
        }
        self.registers[base + plan.index] = Value::Number(next);
        self.registers[base + plan.sum] = Value::Number(sum);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(comparison: Op, bound: f64, step: f64) -> Plan {
        Plan {
            index: 0,
            sum: 1,
            comparison,
            bound,
            step,
            terms: [Term::Input(Input::Current); MAX_TERMS],
            count: 0,
        }
    }

    #[test]
    fn integer_extrema_match_the_original_order() {
        let cases = [
            (Op::Lt, -15.0, 3.0, 120.0),
            (Op::Le, -15.0, 3.0, 120.0),
            (Op::Gt, 117.0, -3.0, -24.0),
            (Op::Ge, 117.0, -3.0, -24.0),
            (Op::Lt, -1.0, 1.0, 1.0),
            (Op::Le, 40.0, -1.0, 100.0),
        ];
        for (comparison, index, step, bound) in cases {
            let plan = plan(comparison, bound, step);
            for (first_min, second_min) in
                [(true, true), (true, false), (false, true), (false, false)]
            {
                for (first, second) in [(7.0, -9.0), (-25.0, 40.0), (10.0, 10.0)] {
                    for sum in [-0.0, 0.0, -23.0, 49.0] {
                        let terms = [(first_min, first), (second_min, second)].map(
                            |(minimum, threshold)| Term::Extremum {
                                first: Input::Current,
                                second: Input::Number(threshold),
                                minimum,
                            },
                        );
                        let expected = plan.advance(index, sum, |index, sum| {
                            (sum + terms[0].value(index)) + terms[1].value(index)
                        });
                        let actual = plan
                            .advance_exact_pair(
                                index,
                                sum,
                                (first_min, first),
                                (second_min, second),
                            )
                            .unwrap();
                        assert_eq!(
                            (actual.0.to_bits(), actual.1.to_bits()),
                            (expected.0.to_bits(), expected.1.to_bits())
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn dyadic_extrema_preserve_rounding_on_both_sides_of_zero() {
        let limit = EXACT_INTEGER as f64;
        for unit in [1.0, 0.25] {
            for (comparison, index, step, bound) in [
                (Op::Lt, 0.0, 1.0, 83.0),
                (Op::Le, -2.0, 2.0, 83.0),
                (Op::Ge, 83.0, -3.0, -17.0),
                (Op::Gt, 84.0, -4.0, -17.0),
            ] {
                let plan = plan(comparison, bound * unit, step * unit);
                for (first, second) in [(13.0, 19.0), (4.0, -7.0), (4.0, 6.0), (4.0, 4.0)] {
                    for first_min in [false, true] {
                        for second_min in [false, true] {
                            for sum in [250.0, limit + 32002.0, -limit - 32000.0] {
                                let index = index * unit;
                                let sum = sum * unit;
                                let first = first * unit;
                                let second = second * unit;
                                let expected = plan.advance(index, sum, |i, sum| {
                                    let a = if first_min {
                                        i.min(first)
                                    } else {
                                        i.max(first)
                                    };
                                    let b = if second_min {
                                        i.min(second)
                                    } else {
                                        i.max(second)
                                    };
                                    (sum + a) + b
                                });
                                let actual = plan
                                    .advance_exact_pair(
                                        index,
                                        sum,
                                        (first_min, first),
                                        (second_min, second),
                                    )
                                    .unwrap();
                                assert_eq!(
                                    (actual.0.to_bits(), actual.1.to_bits()),
                                    (expected.0.to_bits(), expected.1.to_bits())
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn extrema_decline_when_the_accumulator_can_leave_its_grid() {
        let positive = (true, 10.0);
        let negative = (false, -2.0);
        let plan = plan(Op::Lt, 150.0, 1.0);
        assert!(
            plan.advance_exact_pair(0.0, EXACT_INTEGER as f64, positive, negative)
                .is_none()
        );
        assert!(
            plan.advance_exact_pair(
                0.0,
                2.0 * EXACT_INTEGER as f64 - 20.0,
                positive,
                (true, 8.0)
            )
            .is_none()
        );
        assert!(
            plan.advance_exact_pair(0.0, 0.0, (true, f64::EPSILON / 2.0), negative)
                .is_none()
        );
    }
}
