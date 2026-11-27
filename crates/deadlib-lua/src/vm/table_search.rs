//! Replace a side-effect-free traversal for one primitive key with a raw lookup.
use super::*;
use crate::bytecode::{Prototype, RK};

impl Vm {
    #[cold]
    pub(super) fn try_table_search(
        &mut self,
        function: FunctionId,
        prototype: &Prototype,
        base: usize,
    ) -> Option<(usize, Value)> {
        // Match the compiler's lowering of the entire loop, including its
        // branches and return values. Global names are resolved below.
        let code = &prototype.code;
        let next = code[2].c;
        let table = code[3].c;
        let missing = code[9].c;
        let assert = code[11].c;
        let nil = code[12].c;
        let layout = [
            (Op::Load, 1, 1, 0),
            (Op::Load, 2, 1, 0),
            (Op::GetGlobal, 3, 0, next),
            (Op::GetGlobal, 4, 0, table),
            (Op::Move, 5, 1, 0),
            (Op::Call, 3, 2, 2),
            (Op::Move, 2, 4, 0),
            (Op::Move, 1, 3, 0),
            (Op::JumpTrue, 1, 0, 11),
            (Op::GetUpvalue, 3, 0, missing),
            (Op::Return, 3, 0, 1),
            (Op::GetGlobal, 3, 0, assert),
            (Op::Ne, 5, 2, nil),
            (Op::Bool, 4, 5, 0),
            (Op::Call, 3, 1, 0),
            (Op::Eq, 4, 1, 0),
            (Op::JumpFalse, 4, 1, 2),
            (Op::Return, 2, 0, 1),
            (Op::Jump, 0, 0, 2),
            (Op::Return, 0, 0, 0),
        ];
        if code.len() != layout.len()
            || !code
                .iter()
                .zip(layout)
                .all(|(i, expected)| (i.op, i.a, i.b, i.c) == expected)
        {
            return None;
        }
        let key = self.registers[base];
        if !matches!(
            key,
            Value::String(_) | Value::Number(_) | Value::Boolean(_) | Value::Nil
        ) || self.frames.len() == self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
            || !self.sort_fast_context()
            || !self.sort_gc_ready()
        {
            return None;
        }
        let closure = &self.functions[slot_index(function.0)];
        let constants = self.programs[closure.program].bound_constants(closure.prototype);
        if *constants.get(nil.checked_sub(RK)? as usize)? != Value::Nil {
            return None;
        }
        let globals = &self.tables[slot_index(closure.env.0)];
        let raw = |index: u32| match constants[index as usize] {
            Value::String(key) => globals.get_string(key),
            _ => Value::Nil,
        };
        if !matches!(raw(next), Value::Native(NEXT | PAIRS_NEXT)) || raw(assert) != Value::Native(0)
        {
            return None;
        }
        let Value::Table(table) = raw(table) else {
            return None;
        };
        let table = &self.tables[slot_index(table.0)];
        // The truthiness branch terminates at a false key. The original scan
        // must determine whether that key precedes a match when it is present.
        if table.get(Value::Boolean(false)) != Value::Nil {
            return None;
        }
        let found = table.get(key);
        if found == Value::Nil {
            let cell = *closure.upvalues.get(missing as usize)? as usize;
            Some((10, self.cell_value(cell)))
        } else {
            Some((17, found))
        }
    }
}
