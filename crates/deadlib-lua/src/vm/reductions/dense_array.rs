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
        // The current internal index is the zero-based slot for the next write.
        let start = u32::try_from(integer(self.registers[internal])?).ok()? as usize;
        let end = u32::try_from(integer(self.registers[internal + 1])?).ok()? as usize;
        if end.saturating_sub(start) < 2 {
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
        let stop = if start == 0 && repeated.is_none() && table.share_dense_sequence(end) {
            end
        } else {
            let (stop, capacity) = table.dense_write_span(start, end, growth)?;
            if stop <= start {
                return Some(Reduction::Retry);
            }
            let before = table.heap_bytes();
            // A failed reservation leaves the array unchanged. Dispatch then
            // tries the ordinary writes, preserving their allocation error point.
            if let Some(value) = repeated {
                table.write_dense_span(start, stop, capacity, |_| value)
            } else {
                table.write_dense_span(start, stop, capacity, |slot| {
                    // The span is bounded by an exact u32 array index.
                    Value::Number((slot as i64 + 1) as f64)
                })
            }
            .ok()?;
            let after = table.heap_bytes();
            self.note_table_resize(before, after);
            stop
        };
        // Planning proved that every skipped checkpoint is below threshold.
        self.gc.pending = false;
        let index = Value::Number(stop as f64);
        self.registers[internal] = index;
        self.registers[internal + 3] = index;
        if stop < end {
            Some(Reduction::Retry)
        } else {
            Some(Reduction::Applied(
                (stop - start) as u64 * (body.len() + 1) as u64,
            ))
        }
    }
}
