//! Exact reductions of allocation-free numeric loop bodies.

use crate::{
    Value,
    bytecode::{Instruction, OPEN, Op},
};

const INDEX: u16 = 256;
const SUM: u16 = 512;
const CONSTANT: u16 = u16::MAX;
const EXACT_INTEGER: i128 = 1 << 53;

fn read(
    register: usize,
    values: &[u16; 256],
    written: &[bool; 256],
    before: &mut [bool; 256],
) -> Option<u16> {
    let value = *values.get(register)?;
    before[register] |= !written[register];
    Some(value)
}

fn accumulator(body: &[Instruction], control: usize) -> Option<usize> {
    if body.is_empty()
        || body.len() > 32
        || control + 3 >= 256
        || body
            .iter()
            .any(|i| !matches!(i.op, Op::Load | Op::Move | Op::Add | Op::Close))
    {
        return None;
    }
    let mut values = std::array::from_fn(|i| i as u16);
    let mut written = [false; 256];
    let mut before = [false; 256];
    values[control + 3] = INDEX;
    let mut sum = None;
    for i in body {
        // The caller excludes open upvalues in this frame. This body cannot
        // create any, so all of its Close instructions have no effect.
        if i.op == Op::Close {
            continue;
        }
        let destination = usize::from(i.a);
        if (control..control + 3).contains(&destination) {
            return None;
        }
        let value = match i.op {
            Op::Load if i.b == 0 => CONSTANT,
            // These slots were initialized on frame entry; this instruction
            // intentionally does not overwrite a subsequent register edit.
            Op::Load => continue,
            Op::Move if i.c != OPEN => read(usize::from(i.b), &values, &written, &mut before)?,
            Op::Add if sum.is_none() => {
                let left = read(usize::from(i.b), &values, &written, &mut before)?;
                let right = read(i.c as usize, &values, &written, &mut before)?;
                let register = match (left, right) {
                    (INDEX, r) | (r, INDEX) if r < 256 => usize::from(r),
                    _ => return None,
                };
                if (control..control + 4).contains(&register) {
                    return None;
                }
                sum = Some(register);
                SUM + register as u16
            }
            _ => return None,
        };
        values[destination] = value;
        written[destination] = true;
    }
    let sum = sum?;
    if values[sum] != SUM + sum as u16
        || (0..256).any(|r| written[r] && before[r] && r != sum && r != control + 3)
    {
        return None;
    }
    Some(sum)
}

fn integer(registers: &[Value], index: usize) -> Option<i128> {
    let Value::Number(value) = *registers.get(index)? else {
        return None;
    };
    if !value.is_finite() || value.abs() > EXACT_INTEGER as f64 || value.fract() != 0.0 {
        return None;
    }
    Some(i128::from(value as i64))
}

/// Advance complete iterations before the last one. The normal interpreter
/// still executes the last body and failed loop test, preserving scratch
/// registers, the visible index, the final PC, and error-line state.
pub(super) fn sum_loop(
    registers: &mut [Value],
    base: usize,
    control: usize,
    loop_pc: usize,
    code: &[Instruction],
    remaining: Option<u64>,
) -> Option<u64> {
    let loop_instruction = code.get(loop_pc)?;
    if loop_instruction.op != Op::ForLoop || usize::from(loop_instruction.a) != control {
        return None;
    }
    let body = code.get(loop_instruction.c as usize..loop_pc)?;
    let accumulator = accumulator(body, control)?;
    let start = base.checked_add(control)?;
    let old_index = integer(registers, start)?;
    let limit = integer(registers, start + 1)?;
    let step = integer(registers, start + 2)?;
    let total = integer(registers, base + accumulator)?;
    if step == 0 {
        return None;
    }
    let first = old_index + step;
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
    // Same-signed terms and accumulator make every partial sum monotone.
    // Bounds on the last sum and every index therefore prove that each
    // omitted floating-point addition would have an exact integer result.
    if first == 0
        || last == 0
        || first.signum() != last.signum()
        || (total != 0 && total.signum() != first.signum())
        || first.abs() > EXACT_INTEGER
        || last.abs() > EXACT_INTEGER
        || (last + step).abs() > EXACT_INTEGER
    {
        return None;
    }
    let sum = count * (first + last) / 2;
    if (total + sum).abs() > EXACT_INTEGER {
        return None;
    }
    let iteration_cost = body.len() as u64 + 1;
    let count = u64::try_from(count).ok()?;
    let full_cost = count.checked_mul(iteration_cost)?.checked_add(1)?;
    if remaining.is_some_and(|fuel| fuel < full_cost) {
        return None;
    }
    let skipped = (count - 1) * iteration_cost;
    registers[start] = Value::Number((last - step) as f64);
    registers[base + accumulator] = Value::Number((total + sum - last) as f64);
    Some(skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_the_compilers_numeric_sum_loop() {
        let program = crate::compile(b"local s=0; for i=1,100 do s=s+i end; return s").unwrap();
        let prototype = &program.data.prototypes[program.data.entry as usize];
        let prep = prototype.code.iter().find(|i| i.op == Op::ForPrep).unwrap();
        let mut registers = vec![Value::Nil; usize::from(prototype.registers)];
        let control = usize::from(prep.a);
        registers[0] = Value::Number(0.0);
        registers[control] = Value::Number(0.0);
        registers[control + 1] = Value::Number(100.0);
        registers[control + 2] = Value::Number(1.0);
        assert!(
            sum_loop(
                &mut registers,
                0,
                control,
                prep.c as usize,
                &prototype.code,
                None
            )
            .is_some()
        );
        assert_eq!(registers[control], Value::Number(99.0));
        assert_eq!(registers[0], Value::Number(4950.0));
    }
}
