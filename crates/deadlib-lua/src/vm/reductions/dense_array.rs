use super::{Instruction, Op, Prototype, RK, Reduction, Value, Vm, integer, slot_index};
use crate::bytecode::Constant;

impl Vm {
    pub(super) fn reduce_dense_array_write(
        &mut self,
        base: usize,
        control: usize,
        body: &[Instruction],
        prototype: &Prototype,
    ) -> Option<Reduction> {
        let [store, rest @ ..] = body else {
            return None;
        };
        let index = control + 3;
        if store.op != Op::SetTable
            || index >= RK as usize
            || usize::from(store.b) != index
            || usize::from(store.a) >= control
            || !(rest.is_empty()
                || matches!(rest, [close] if close.op == Op::Close && usize::from(close.a) == index))
            || !self.sort_fast_context()
            || !self.open_upvalues.is_empty()
            || !self.registers.memory().uses_global_allocator()
        {
            return None;
        }

        let internal = base + control;
        if self.registers[internal + 2] != Value::Number(1.0) {
            return None;
        }
        // start is a zero-based slot; end leaves the last iteration to the VM.
        let start = u32::try_from(integer(self.registers[internal])?).ok()? as usize;
        let limit = u32::try_from(integer(self.registers[internal + 1])?).ok()? as usize;
        let end = limit.checked_sub(1)?;
        if start >= end {
            return None;
        }
        let Value::Table(id) = self.registers[base + usize::from(store.a)] else {
            return None;
        };
        let repeated = if store.c as usize == index {
            None
        } else {
            let value = if store.c >= RK {
                match prototype.constants().get((store.c - RK) as usize)? {
                    Constant::Boolean(value) => Value::Boolean(*value),
                    Constant::Number(value) => Value::Number(*value),
                    _ => return None,
                }
            } else if (store.c as usize) < control {
                self.registers[base + store.c as usize]
            } else {
                return None;
            };
            if !matches!(
                value,
                Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
            ) {
                return None;
            }
            Some(value)
        };

        let Some(growth) = self.dense_array_gc_growth() else {
            return Some(Reduction::Retry);
        };
        let table = &mut self.tables[slot_index(id.0)];
        let (stop, capacity) = table.dense_write_span(start, end, growth)?;
        if stop <= start {
            return Some(Reduction::Retry);
        }
        let before = table.heap_bytes();
        // A failed reservation leaves the array unchanged. Dispatch then
        // tries the ordinary writes, preserving their allocation error point.
        table
            .write_dense_span(start, stop, capacity, |slot| {
                repeated.unwrap_or(Value::Number(slot as f64 + 1.0))
            })
            .ok()?;
        let after = table.heap_bytes();
        self.note_table_resize(before, after);
        self.registers[internal] = Value::Number(stop as f64);
        if stop < end {
            Some(Reduction::Retry)
        } else {
            Some(Reduction::Applied(
                (stop - start) as u64 * (body.len() + 1) as u64,
            ))
        }
    }
}
