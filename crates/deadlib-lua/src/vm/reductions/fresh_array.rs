//! Fold reads from a temporary table filled with one loop-invariant scalar.
use super::{Instruction, Op, Prototype, RK, Value, Vm, dense_loop_store, integer};
use crate::table::{LUA51_MAX_ARRAY, array_index};

const MAX_REGISTERS: usize = 32;
const MAX_SETUP: usize = 8;
const MAX_READOUT: usize = 16;
const MIN_FILL: usize = 32;

struct Registers<'a> {
    values: &'a mut [Value],
    constants: &'a [Value],
    table: usize,
}

impl Registers<'_> {
    fn read(&self, operand: u32) -> Option<Value> {
        if operand >= RK {
            self.constants.get((operand - RK) as usize).copied()
        } else {
            let register = operand as usize;
            (register != self.table)
                .then(|| self.values.get(register).copied())
                .flatten()
        }
    }

    fn number(&self, operand: u32) -> Option<f64> {
        let Value::Number(value) = self.read(operand)? else {
            return None;
        };
        Some(value)
    }

    fn write(&mut self, register: u8, value: Value) -> Option<()> {
        let register = usize::from(register);
        if register == self.table {
            return None;
        }
        *self.values.get_mut(register)? = value;
        Some(())
    }

    fn scalar(&mut self, instruction: Instruction) -> Option<()> {
        let b = u32::from(instruction.b);
        let c = instruction.c;
        let value = match instruction.op {
            Op::Load if b == 0 => *self.constants.get(c as usize)?,
            Op::Move if c == 0 => self.read(b)?,
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => Value::Number(
                Vm::number_op(instruction.op, self.number(b)?, self.number(c)?),
            ),
            Op::Neg => Value::Number(-self.number(b)?),
            _ => return None,
        };
        self.write(instruction.a, value)
    }
}

fn fill_loop(code: &[Instruction], start: usize, table: u8) -> Option<(usize, usize, Instruction)> {
    for pc in start..start + MAX_SETUP {
        let instruction = *code.get(pc)?;
        match instruction.op {
            Op::ForPrep => {
                let end = instruction.c as usize;
                let backedge = *code.get(end)?;
                let control = usize::from(instruction.a);
                if backedge.op != Op::ForLoop
                    || backedge.a != instruction.a
                    || backedge.c as usize != pc + 1
                {
                    return None;
                }
                let store = dense_loop_store(control, code.get(pc + 1..end)?)?;
                return (store.a == table && (store.c >= RK || (store.c as usize) < control))
                    .then_some((pc, end, store));
            }
            Op::Load
            | Op::Move
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::Pow
            | Op::Neg => {}
            _ => return None,
        }
    }
    None
}

#[inline]
fn table_dead(prototype: &Prototype, pc: usize, table: usize) -> bool {
    let Some(metadata) = prototype.metadata.get(pc) else {
        return false;
    };
    let before = metadata.gc_live as u16 as usize;
    prototype.gc_maps[before][table / 64] & (1 << (table % 64)) == 0
        && !prototype.debug.locals().iter().any(|local| {
            usize::from(local.register) == table
                && local.start as usize <= pc
                && pc < local.end as usize
        })
}

impl Vm {
    #[inline(never)]
    pub(in crate::vm) fn elide_fresh_dense_fill(
        &mut self,
        base: usize,
        start: usize,
        prototype: &Prototype,
        program: usize,
        prototype_index: usize,
    ) -> Option<usize> {
        let code = &prototype.code;
        let table = code[start - 1].a;
        let (prep, edge, store) = fill_loop(code, start, table)?;
        let count = usize::from(prototype.registers);
        if count > MAX_REGISTERS
            || !self.sort_fast_context()
            || !self.open_upvalues.is_empty()
            || !self.registers.memory().uses_global_allocator()
            || self.gc_may_call_out()
        {
            return None;
        }

        // None of these instructions can publish the table or reenter the VM.
        // Keep all register changes private until the last use is proven.
        let mut scratch = [Value::Nil; MAX_REGISTERS];
        let destination = base..base + count;
        scratch[..count].copy_from_slice(self.registers.get(destination.clone())?);
        let mut registers = Registers {
            values: &mut scratch[..count],
            constants: self.programs[program].bound_constants(prototype_index),
            table: usize::from(table),
        };
        for &instruction in &code[start..prep] {
            registers.scalar(instruction)?;
        }
        let control = usize::from(code[prep].a);
        let slots = registers.values.get(control..control + 4)?;
        let [
            Value::Number(first),
            limit @ Value::Number(_),
            Value::Number(step),
            _,
        ] = *slots
        else {
            return None;
        };
        let length = usize::try_from(integer(limit)?).ok()?;
        if first != 1.0 || step != 1.0 || !(MIN_FILL..=LUA51_MAX_ARRAY).contains(&length) {
            return None;
        }
        let repeated = registers.read(store.c)?;
        if !matches!(
            repeated,
            Value::Number(_) | Value::Boolean(_) | Value::LightUserdata(_)
        ) {
            return None;
        }
        registers.values[control..control + 4].copy_from_slice(&[
            limit,
            limit,
            Value::Number(step),
            limit,
        ]);

        let mut read_table = false;
        for pc in edge + 1..=edge + MAX_READOUT {
            if read_table && table_dead(prototype, pc, registers.table) {
                registers.values[registers.table] = Value::Nil;
                self.registers[destination].copy_from_slice(registers.values);
                return Some(pc);
            }
            let instruction = *code.get(pc)?;
            let value = match instruction.op {
                Op::Len if instruction.b == u16::from(table) => limit,
                Op::GetTable if instruction.b == u16::from(table) => {
                    let offset = array_index(registers.read(instruction.c)?)?;
                    if offset >= length {
                        return None;
                    }
                    repeated
                }
                Op::Close => continue,
                _ => {
                    registers.scalar(instruction)?;
                    continue;
                }
            };
            registers.write(instruction.a, value)?;
            read_table = true;
        }
        None
    }
}
