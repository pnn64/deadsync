//! Project the results of short argument-only Lua calls and tail relays.

use super::{FunctionId, Value, Vm, slot_index};
use crate::bytecode::{OPEN, OPEN_ARGS, Op};
use std::ops::Range;

const MAX_VALUES: usize = 8;
const MAX_REGISTERS: usize = 32;
const MAX_INSTRUCTIONS: usize = 12;

#[derive(Clone, Copy)]
enum Source {
    Argument(usize),
    Constant(Value),
    Capture(usize),
}

const NIL: Source = Source::Constant(Value::Nil);

type Registers = [Source; MAX_REGISTERS];

enum Exit {
    Return(Range<usize>, u32),
    Tail(usize, Range<usize>),
}

struct Decoded {
    exit: Exit,
    registers: usize,
    high_water: usize,
    parameters: usize,
    vararg: bool,
}

fn decode(
    vm: &Vm,
    function: FunctionId,
    arguments: &[Source],
    registers: &mut Registers,
) -> Option<Decoded> {
    let closure = &vm.functions[slot_index(function.0)];
    let program = &vm.programs[closure.program];
    let prototype = &program.data.prototypes[closure.prototype];
    let size = usize::from(prototype.registers);
    let parameters = usize::from(prototype.params);
    if size > registers.len()
        || prototype.compat_arg
        || !matches!(
            prototype.code[0].op,
            Op::Move | Op::Load | Op::GetUpvalue | Op::Vararg | Op::Return
        )
    {
        return None;
    }
    registers.fill(NIL);
    let fixed = parameters.min(arguments.len());
    registers[..fixed].copy_from_slice(&arguments[..fixed]);
    let extras = &arguments[fixed..];
    let constants = program.bound_constants(closure.prototype);
    let mut top = 0;
    let mut high_water = size;
    for (pc, instruction) in prototype.code.iter().take(MAX_INSTRUCTIONS).enumerate() {
        let a = usize::from(instruction.a);
        let b = usize::from(instruction.b);
        let c = instruction.c as usize;
        match instruction.op {
            Op::Move if c == 0 => registers[a] = registers[b],
            Op::Load if b == 0 => registers[a] = Source::Constant(constants[c]),
            Op::GetUpvalue => registers[a] = Source::Capture(closure.upvalues[c] as usize),
            Op::Vararg if prototype.vararg => {
                let wanted = if instruction.c == OPEN {
                    extras.len()
                } else {
                    c
                };
                top = a.checked_add(wanted)?;
                let destinations = registers.get_mut(a..top)?;
                let copied = wanted.min(extras.len());
                destinations[..copied].copy_from_slice(&extras[..copied]);
                destinations[copied..].fill(NIL);
                high_water = high_water.max(top);
            }
            Op::Return => {
                let end = if instruction.c == OPEN {
                    top
                } else {
                    a.checked_add(c)?
                };
                let range = a..end;
                registers.get(range.clone())?;
                return Some(Decoded {
                    exit: Exit::Return(range, prototype.instruction_line(pc)),
                    registers: size,
                    high_water,
                    parameters,
                    vararg: prototype.vararg,
                });
            }
            Op::TailCall => {
                let Source::Capture(cell) = registers[a] else {
                    return None;
                };
                let start = a + 1;
                let end = if instruction.b == OPEN_ARGS {
                    top
                } else {
                    start.checked_add(b)?
                };
                let arguments = start..end;
                registers.get(arguments.clone())?;
                return Some(Decoded {
                    exit: Exit::Tail(cell, arguments),
                    registers: size,
                    high_water,
                    parameters,
                    vararg: prototype.vararg,
                });
            }
            _ => return None,
        }
    }
    None
}

pub(super) struct Plan {
    values: [Source; MAX_VALUES],
    count: usize,
    registers: usize,
    tail: Option<(usize, FunctionId)>,
    line: u32,
}

impl Plan {
    fn build(vm: &Vm, function: FunctionId, count: usize) -> Option<Self> {
        let arguments: [Source; MAX_VALUES] = std::array::from_fn(Source::Argument);
        let mut registers = [NIL; MAX_REGISTERS];
        let outer = decode(vm, function, arguments.get(..count)?, &mut registers)?;
        let (exit, needed, tail) = match outer.exit {
            Exit::Tail(cell, arguments) => {
                let Value::Function(target) = vm.cell_value(cell) else {
                    return None;
                };
                if arguments.len() > MAX_VALUES {
                    return None;
                }
                let mut forwarded = [NIL; MAX_VALUES];
                let count = arguments.len();
                forwarded[..count].copy_from_slice(&registers[arguments]);
                let inner = decode(vm, target, &forwarded[..count], &mut registers)?;
                let extras = count.saturating_sub(inner.parameters);
                // tail_frame reserves the argument scratch even when it can
                // perform the move in place. Preserve allocations and limits.
                let scratch = outer.high_water.max(extras + inner.registers) + count;
                (
                    inner.exit,
                    scratch.max(extras + inner.high_water),
                    Some((cell, target)),
                )
            }
            exit @ Exit::Return(..) if outer.vararg => (exit, outer.high_water, None),
            _ => return None,
        };
        let Exit::Return(range, line) = exit else {
            return None;
        };
        let saved = registers.get(range)?;
        if saved.len() > MAX_VALUES
            || saved
                .iter()
                .any(|source| matches!(source, Source::Capture(_)))
        {
            return None;
        }
        let mut values = [NIL; MAX_VALUES];
        values[..saved.len()].copy_from_slice(saved);
        Some(Self {
            values,
            count: saved.len(),
            registers: needed,
            tail,
            line,
        })
    }

    fn matches(&self, vm: &Vm) -> bool {
        self.tail
            .is_none_or(|(cell, target)| vm.cell_value(cell) == Value::Function(target))
    }

    fn has_stack_space(&self, vm: &Vm) -> bool {
        let depth = vm.frames.len() + usize::from(self.tail.is_some());
        depth < vm.frames.capacity()
            && depth + 1 < usize::from(vm.call_slots)
            && self.registers <= vm.registers.capacity() - vm.registers.len()
    }
}

pub(super) struct Cache {
    entry: Option<(FunctionId, usize, Plan)>,
    rejected: [(FunctionId, u16); 2],
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            entry: None,
            rejected: [(FunctionId(0), 0); 2],
        }
    }
}

impl Cache {
    pub(super) fn rejects(&self, function: FunctionId, count: usize) -> bool {
        count > MAX_VALUES
            || self
                .rejected
                .iter()
                .any(|&(id, arities)| id == function && arities & (1 << count) != 0)
    }

    fn reject(&mut self, function: FunctionId, count: usize) {
        let bit = 1 << count;
        if let Some((_, arities)) = self.rejected.iter_mut().find(|(id, _)| *id == function) {
            *arities |= bit;
        } else {
            self.rejected[1] = self.rejected[0];
            self.rejected[0] = (function, bit);
        }
    }

    fn get(&mut self, vm: &Vm, function: FunctionId, count: usize) -> Option<&Plan> {
        if !self.entry.as_ref().is_some_and(|(cached, arity, plan)| {
            *cached == function && *arity == count && plan.matches(vm)
        }) {
            let Some(plan) = Plan::build(vm, function, count) else {
                self.reject(function, count);
                return None;
            };
            self.entry = Some((function, count, plan));
        }
        self.entry.as_ref().map(|(_, _, plan)| plan)
    }
}

impl Vm {
    #[inline(never)]
    pub(super) fn try_forwarded_call(
        &mut self,
        cache: &mut Cache,
        function: FunctionId,
        call: usize,
        count: usize,
        wanted: usize,
        error_line: &mut u32,
    ) -> bool {
        if count > MAX_VALUES
            || wanted > MAX_VALUES
            || call + (count + 1).max(wanted) > self.registers.len()
        {
            return false;
        }
        let Some(plan) = cache.get(self, function, count) else {
            return false;
        };
        if !plan.has_stack_space(self) || !self.sort_fast_context() || !self.sort_gc_ready() {
            return false;
        }
        let mut values = [Value::Nil; MAX_VALUES];
        for (output, &source) in values[..wanted.min(plan.count)]
            .iter_mut()
            .zip(&plan.values)
        {
            *output = match source {
                Source::Argument(index) => self.registers[call + 1 + index],
                Source::Constant(value) => value,
                Source::Capture(_) => unreachable!("verified forwarding result"),
            };
        }
        self.registers[call..call + wanted].copy_from_slice(&values[..wanted]);
        self.top = call + wanted;
        *error_line = plan.line;
        true
    }
}
