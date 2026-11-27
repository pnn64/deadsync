//! Replace a side-effect-free traversal for one primitive key with a raw lookup.
use super::*;
use crate::bytecode::{COMPARE_BRANCH, Instruction, Prototype, RK};

impl Vm {
    #[cold]
    pub(super) fn try_table_search(
        &mut self,
        function: FunctionId,
        prototype: &Prototype,
        base: usize,
    ) -> Option<(usize, Value)> {
        if prototype.code.len() == 19 {
            return self.try_pairs_search(function, prototype, base);
        }
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
            (Op::JumpFalse, 4, COMPARE_BRANCH, 2),
            (Op::Return, 2, 0, 1),
            (Op::Jump, 0, 0, 2),
            (Op::Return, 0, 0, 0),
        ];
        if !matches_layout(code, &layout) {
            return None;
        }
        let key = self.table_search_key(base)?;
        let closure = &self.functions[slot_index(function.0)];
        let constants = self.programs[closure.program].bound_constants(closure.prototype);
        if *constants.get(nil.checked_sub(RK)? as usize)? != Value::Nil {
            return None;
        }
        let globals = &self.tables[slot_index(closure.env.0)];
        if !matches!(
            raw_global(globals, constants, next),
            Value::Native(NEXT | PAIRS_NEXT)
        ) || raw_global(globals, constants, assert) != Value::Native(0)
        {
            return None;
        }
        let Value::Table(table) = raw_global(globals, constants, table) else {
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

    fn try_pairs_search(
        &mut self,
        function: FunctionId,
        prototype: &Prototype,
        base: usize,
    ) -> Option<(usize, Value)> {
        let code = &prototype.code;
        let pairs = code[0].c;
        let table = code[1].c;
        let nil = code[7].c;
        let layout = [
            (Op::GetGlobal, 1, 0, pairs),
            (Op::GetGlobal, 2, 0, table),
            (Op::Call, 1, 1, 3),
            (Op::Move, 4, 1, 0),
            (Op::Move, 5, 2, 0),
            (Op::Move, 6, 3, 0),
            (Op::CallIter, 4, 2, 2),
            (Op::Load, 7, 0, nil),
            (Op::Eq, 7, 4, 7),
            (Op::JumpTrue, 7, 0, 16),
            (Op::Move, 3, 4, 0),
            (Op::Eq, 9, 4, 0),
            (Op::JumpFalse, 9, COMPARE_BRANCH, 14),
            (Op::Return, 5, 0, 1),
            (Op::Close, 4, 0, 0),
            (Op::Jump, 0, 0, 3),
            (Op::Load, 1, 0, nil),
            (Op::Return, 1, 0, 1),
            (Op::Return, 0, 0, 0),
        ];
        if !matches_layout(code, &layout) {
            return None;
        }
        let thread = &self.threads[slot_index(self.thread.0)];
        if usize::from(self.frames.last()?.barriers) + usize::from(thread.depth) >= 197
            || thread.native_bias != 0
            || !self.loads.is_empty()
        {
            return None;
        }
        let key = self.table_search_key(base)?;
        let closure = &self.functions[slot_index(function.0)];
        let constants = self.programs[closure.program].bound_constants(closure.prototype);
        if *constants.get(nil as usize)? != Value::Nil {
            return None;
        }
        let globals = &self.tables[slot_index(closure.env.0)];
        let Value::Native(pairs) = raw_global(globals, constants, pairs) else {
            return None;
        };
        let pairs = self.native_closures.get(pairs)?;
        if !matches!(pairs.kind, threads::NativeKind::Builtin(PAIRS))
            || !matches!(&*pairs.captures, [Value::Native(NEXT | PAIRS_NEXT)])
        {
            return None;
        }
        let Value::Table(table) = raw_global(globals, constants, table) else {
            return None;
        };
        let value = self.tables[slot_index(table.0)].get(key);
        Some((if value == Value::Nil { 17 } else { 13 }, value))
    }

    fn table_search_key(&mut self, base: usize) -> Option<Value> {
        let key = self.registers[base];
        (matches!(
            key,
            Value::String(_) | Value::Number(_) | Value::Boolean(_) | Value::Nil
        ) && self.frames.len() < self.frames.capacity()
            && self.frames.len() + 1 < usize::from(self.call_slots)
            && self.sort_fast_context()
            && self.sort_gc_ready())
        .then_some(key)
    }
}

fn raw_global(globals: &Table, constants: &[Value], index: u32) -> Value {
    match constants[index as usize] {
        Value::String(key) => globals.get_string(key),
        _ => Value::Nil,
    }
}

fn matches_layout(code: &[Instruction], layout: &[(Op, u8, u16, u32)]) -> bool {
    code.len() == layout.len()
        && code
            .iter()
            .zip(layout)
            .all(|(i, &expected)| (i.op, i.a, i.b, i.c) == expected)
}
