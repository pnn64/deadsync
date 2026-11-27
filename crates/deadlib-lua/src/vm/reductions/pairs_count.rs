//! Batch iterator steps whose only effect is advancing a separate numeric field.
use super::{EXACT_INTEGER, integer};
use crate::bytecode::{Instruction, Op, RK};
use crate::vm::{Cell, FrameKind, NEXT, PAIRS_NEXT, Value, Vm, slot_index};

impl Vm {
    #[cold]
    pub(in crate::vm) fn batch_checked_pairs(
        &mut self,
        code: &[Instruction],
        pc: usize,
        base: usize,
    ) -> Option<()> {
        let call = *code.get(pc)?;
        let a = usize::from(call.a);
        if call.op != Op::CallIter || call.b != 2 || call.c != 2 || a < 3 || a + 6 >= 256 {
            return None;
        }
        let slot = base + a;
        let Value::Table(source) = self.registers[slot + 1] else {
            return None;
        };
        if !matches!(self.registers[slot], Value::Native(NEXT | PAIRS_NEXT))
            || self.registers[slot + 2] != Value::Nil
            || self.registers[slot - 3..slot] != self.registers[slot..slot + 3]
        {
            return None;
        }
        let head = pc.checked_sub(3)?;
        let window = code.get(head..pc + 21)?;
        // Decode constant and local operands independently of their source
        // names. Match every branch before treating the loop as a batch.
        let counter = code[pc + 5].c;
        let field = code[pc + 7].c;
        let increment = code[pc + 8].c;
        let assert = code[pc + 10].c;
        let nil = code[pc + 1].c;
        let owner = usize::from(code[pc + 15].b);
        if owner >= a - 3 || self.registers[base + owner] != Value::Table(source) {
            return None;
        }
        let field_rk = u16::try_from(field).ok()?;
        let a = call.a;
        let b = u16::from(a);
        let c = u32::from(a);
        let expected = [
            (Op::Move, a, b - 3, 0),
            (Op::Move, a + 1, b - 2, 0),
            (Op::Move, a + 2, b - 1, 0),
            (Op::CallIter, a, 2, 2),
            (Op::Load, a + 3, 0, nil),
            (Op::Eq, a + 3, b, c + 3),
            (Op::JumpTrue, a + 3, 0, (pc + 21) as u32),
            (Op::Move, a - 1, b, 0),
            (Op::GetGlobal, a + 4, 0, counter),
            (Op::GetGlobal, a + 5, 0, counter),
            (Op::GetTable, a + 5, b + 5, field),
            (Op::Add, a + 5, b + 5, increment),
            (Op::SetTable, a + 4, field_rk, c + 5),
            (Op::GetGlobal, a + 4, 0, assert),
            (Op::Move, a + 5, b, 0),
            (Op::JumpFalse, a + 5, 0, (pc + 14) as u32),
            (Op::Move, a + 5, b + 1, 0),
            (Op::JumpFalse, a + 5, 0, (pc + 18) as u32),
            (Op::GetTable, a + 5, owner as u16, c),
            (Op::Eq, a + 6, b + 5, c + 1),
            (Op::Bool, a + 5, b + 6, 0),
            (Op::Call, a + 4, 1, 0),
            (Op::Close, a, 0, 0),
            (Op::Jump, 0, 0, head as u32),
        ];
        if !window
            .iter()
            .zip(expected)
            .all(|(i, expected)| (i.op, i.a, i.b, i.c) == expected)
            || self.open_upvalues.iter().any(|&id| {
                matches!(self.upvalues[id as usize],
                         Cell::Open(thread, register) if thread == self.thread && register >= slot - 1)
            })
            || self.frames.len() == self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
            || !self.sort_fast_context()
            || !self.gc_checkpoint_idle()
        {
            return None;
        }
        // The loop performs no allocations and only overwrites one existing
        // scalar. Even in an active GC phase, each omitted poll is idle.
        let FrameKind::Lua(function) = self.frames.last()?.function else {
            return None;
        };
        let closure = &self.functions[slot_index(function.0)];
        let globals = &self.tables[slot_index(closure.env.0)];
        let constants = self.programs[closure.program].bound_constants(closure.prototype);
        let increment = integer(*constants.get(increment.checked_sub(RK)? as usize)?)?;
        let Value::String(field) = *constants.get(field.checked_sub(RK)? as usize)? else {
            return None;
        };
        let (&Value::String(counter_name), &Value::String(assert_name)) = (
            constants.get(counter as usize)?,
            constants.get(assert as usize)?,
        ) else {
            return None;
        };
        if *constants.get(nil as usize)? != Value::Nil
            || increment == 0
            || globals.get_string(assert_name) != Value::Native(0)
        {
            return None;
        }
        let Value::Table(counter) = globals.get_string(counter_name) else {
            return None;
        };
        if counter == source || counter == closure.env {
            return None;
        }
        let old = integer(self.tables[slot_index(counter.0)].get_string(field))?;
        let table = &self.tables[slot_index(source.0)];
        let mut previous = Value::Nil;
        let mut penultimate = Value::Nil;
        let mut count = 0u32;
        for (key, value) in table.pairs() {
            // The same raw value is read twice. Equality is reflexive except
            // for NaN. If an assertion would fail, preserve every preceding
            // counter update via ordinary dispatch.
            if !key.is_truthy()
                || !value.is_truthy()
                || matches!(value, Value::Number(number) if number.is_nan())
            {
                return None;
            }
            penultimate = previous;
            previous = key;
            count = count.checked_add(1)?;
        }
        let skipped = count.checked_sub(1)?;
        if skipped == 0 {
            return None;
        }
        let total = old + i128::from(skipped) * increment;
        if total.abs() > EXACT_INTEGER {
            return None;
        }
        let counter = &mut self.tables[slot_index(counter.0)];
        if !counter.set_existing_string_scalar(field, Value::Number(total as f64)) {
            return None;
        }
        counter.advance_fields_version(skipped - 1);
        // Run the last body and the terminating iterator call normally, so
        // enclosing bytecode sees the original final registers and stack top.
        self.registers[slot - 1] = penultimate;
        self.registers[slot + 2] = penultimate;
        Some(())
    }
}
