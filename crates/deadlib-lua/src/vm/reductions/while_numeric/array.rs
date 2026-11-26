//! Ordered numeric reductions over the array part of a table.
use super::super::integer;
use super::{Input, Registers, Sum, Symbol, Term, Value, Vm, ordered_extremum};
use crate::bytecode::{Instruction, Op};
use crate::vm::{Cell, FrameKind, TableId, slot_index};

mod exact;
pub(in crate::vm) use exact::ArrayReduction;

struct Plan {
    table: TableId,
    sum: usize,
    terms: Sum,
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
        let mut registers = Registers {
            initial,
            constants,
            symbols: [None; 256],
            read_initial: [false; 256],
            invariant: [false; 256],
        };
        registers.symbols[control..control + 3].fill(Some(Symbol::LoopControl));
        registers.symbols[control + 3] = Some(Symbol::LoopIndex);
        let mut terms = Sum::new();
        let mut array = None;
        for instruction in body {
            let destination = usize::from(instruction.a);
            if instruction.op == Op::Close || instruction.op == Op::Load && instruction.b == 1 {
                continue;
            }
            if (control..control + 4).contains(&destination) {
                return None;
            }
            let value = match instruction.op {
                Op::GetUpvalue => {
                    let id = closure.upvalues[instruction.c as usize];
                    let Cell::Closed(value) = vm.upvalues[id as usize] else {
                        return None;
                    };
                    Symbol::Fixed(value)
                }
                Op::GetGlobal => {
                    let key = constants[instruction.c as usize];
                    let value = vm.tables[slot_index(closure.env.0)].get(key);
                    if value == Value::Nil {
                        return None;
                    }
                    Symbol::Fixed(value)
                }
                Op::GetTable => {
                    let table = registers.read(u32::from(instruction.b));
                    let Value::Table(id) = registers.fixed(table)? else {
                        return None;
                    };
                    let key = registers.read(instruction.c);
                    if matches!(key, Symbol::LoopIndex) {
                        if array.is_some_and(|array| array != id) {
                            return None;
                        }
                        array = Some(id);
                        Symbol::Tracked(Term::Input(Input::Current))
                    } else {
                        let key = registers.fixed(key)?;
                        let value = vm.tables[slot_index(id.0)].get(key);
                        if value == Value::Nil {
                            return None;
                        }
                        Symbol::Fixed(value)
                    }
                }
                _ => terms.step(&mut registers, instruction)?,
            };
            registers.symbols[destination] = Some(value);
        }
        Some(Self {
            table: array?,
            sum: terms.register(&registers)?,
            terms,
        })
    }

    fn evaluate(&self, sum: f64, values: &[Value], ascending: bool) -> Option<f64> {
        // Check the final element too, so the ordinary final iteration cannot
        // call an index handler or change the expected math call.
        let (last, prefix) = if ascending {
            values.split_last()?
        } else {
            values.split_first()?
        };
        if !matches!(last, Value::Number(_)) {
            return None;
        }
        let terms = &self.terms.terms[..self.terms.count];
        if let [term] = terms
            && let Some((order, bound)) = array_extremum(*term)
        {
            let evaluate: fn(f64, f64, &[Value], bool) -> Option<f64> = match order {
                MIN_ELEMENT_FIRST => evaluate_single::<MIN_ELEMENT_FIRST>,
                MIN_BOUND_FIRST => evaluate_single::<MIN_BOUND_FIRST>,
                MAX_ELEMENT_FIRST => evaluate_single::<MAX_ELEMENT_FIRST>,
                MAX_BOUND_FIRST => evaluate_single::<MAX_BOUND_FIRST>,
                _ => unreachable!("array extremum order"),
            };
            return evaluate(sum, bound, prefix, ascending);
        }
        if let [first, second] = terms
            && let Some((first, first_bound)) = array_extremum(*first)
            && let Some((second, second_bound)) = array_extremum(*second)
        {
            let evaluate = match first {
                MIN_ELEMENT_FIRST => pair_for_second::<MIN_ELEMENT_FIRST>(second),
                MIN_BOUND_FIRST => pair_for_second::<MIN_BOUND_FIRST>(second),
                MAX_ELEMENT_FIRST => pair_for_second::<MAX_ELEMENT_FIRST>(second),
                MAX_BOUND_FIRST => pair_for_second::<MAX_BOUND_FIRST>(second),
                _ => unreachable!("array extremum order"),
            };
            return evaluate(sum, [first_bound, second_bound], prefix, ascending);
        }
        walk(sum, prefix, ascending, |sum, number| {
            terms.iter().fold(sum, |sum, term| sum + term.value(number))
        })
    }
}

#[inline(always)]
fn walk(
    sum: f64,
    values: &[Value],
    ascending: bool,
    evaluate: impl Fn(f64, f64) -> f64,
) -> Option<f64> {
    #[inline(always)]
    fn advance<'a>(
        mut sum: f64,
        values: impl Iterator<Item = &'a Value>,
        evaluate: impl Fn(f64, f64) -> f64,
    ) -> Option<f64> {
        for value in values {
            let Value::Number(number) = *value else {
                return None;
            };
            sum = evaluate(sum, number);
        }
        Some(sum)
    }
    if ascending {
        advance(sum, values.iter(), evaluate)
    } else {
        advance(sum, values.iter().rev(), evaluate)
    }
}

const MIN_ELEMENT_FIRST: u8 = 0;
const MIN_BOUND_FIRST: u8 = 1;
const MAX_ELEMENT_FIRST: u8 = 2;
const MAX_BOUND_FIRST: u8 = 3;

fn array_extremum(term: Term) -> Option<(u8, f64)> {
    let Term::Extremum {
        first,
        second,
        minimum,
    } = term
    else {
        return None;
    };
    match (first, second, minimum) {
        (Input::Current, Input::Number(bound), true) => Some((MIN_ELEMENT_FIRST, bound)),
        (Input::Number(bound), Input::Current, true) => Some((MIN_BOUND_FIRST, bound)),
        (Input::Current, Input::Number(bound), false) => Some((MAX_ELEMENT_FIRST, bound)),
        (Input::Number(bound), Input::Current, false) => Some((MAX_BOUND_FIRST, bound)),
        _ => None,
    }
}

#[inline(always)]
fn extremum<const ORDER: u8>(element: f64, bound: f64) -> f64 {
    let (first, second) = if ORDER == MIN_ELEMENT_FIRST || ORDER == MAX_ELEMENT_FIRST {
        (element, bound)
    } else {
        (bound, element)
    };
    ordered_extremum(
        first,
        second,
        ORDER == MIN_ELEMENT_FIRST || ORDER == MIN_BOUND_FIRST,
    )
}

fn evaluate_single<const ORDER: u8>(
    sum: f64,
    bound: f64,
    values: &[Value],
    ascending: bool,
) -> Option<f64> {
    walk(sum, values, ascending, |sum, element| {
        sum + extremum::<ORDER>(element, bound)
    })
}

type Pair = fn(f64, [f64; 2], &[Value], bool) -> Option<f64>;

fn pair_for_second<const FIRST: u8>(second: u8) -> Pair {
    match second {
        MIN_ELEMENT_FIRST => evaluate_pair::<FIRST, MIN_ELEMENT_FIRST>,
        MIN_BOUND_FIRST => evaluate_pair::<FIRST, MIN_BOUND_FIRST>,
        MAX_ELEMENT_FIRST => evaluate_pair::<FIRST, MAX_ELEMENT_FIRST>,
        MAX_BOUND_FIRST => evaluate_pair::<FIRST, MAX_BOUND_FIRST>,
        _ => unreachable!("array extremum order"),
    }
}

fn evaluate_pair<const FIRST: u8, const SECOND: u8>(
    sum: f64,
    [first, second]: [f64; 2],
    values: &[Value],
    ascending: bool,
) -> Option<f64> {
    walk(sum, values, ascending, |sum, element| {
        sum + extremum::<FIRST>(element, first) + extremum::<SECOND>(element, second)
    })
}

impl Vm {
    pub(in crate::vm) fn reduce_array_loop(
        &mut self,
        base: usize,
        control: usize,
        body: &[Instruction],
        program: usize,
        prototype: usize,
    ) -> Option<u64> {
        if !body.iter().any(|i| i.op == Op::Call)
            || !self.open_upvalues.is_empty()
            || !self.sort_fast_context()
            || !self.can_skip_gc_checkpoint()
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
        let Value::Number(sum) = self.registers[base + plan.sum] else {
            return None;
        };
        let bounds = (lower as i64, upper as i64);
        let table = &mut self.tables[slot_index(plan.table.0)];
        let before = self.array_reduction.storage_bytes();
        let cached = self
            .array_reduction
            .evaluate(table, &plan, bounds, step > 0, sum);
        let growth = self.array_reduction.storage_bytes() - before;
        if growth != 0 {
            self.gc_note_control(growth);
        }
        let sum = match cached {
            Some(sum) => sum,
            None => {
                let table = &self.tables[slot_index(plan.table.0)];
                let values = table.array_slice(bounds.0, bounds.1)?;
                plan.evaluate(sum, values, step > 0)?
            }
        };
        if !self.numeric_call_ready(0) {
            return None;
        }
        // Re-run the final body with the regular interpreter to publish the
        // last index, call arguments and temporaries in their original slots.
        self.registers[start] = Value::Number((last - step) as f64);
        self.registers[base + plan.sum] = Value::Number(sum);
        Some((upper - lower) as u64 * (body.len() as u64 + 1))
    }
}
