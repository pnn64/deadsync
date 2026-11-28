use super::{
    Cell, EXACT_INTEGER, FrameKind, Instruction, Op, Prototype, RK, Reduction, Value, Vm, integer,
    slot_index,
};
use crate::bytecode::Constant;
use crate::table::Table;

fn write_reserved_indices(
    table: &mut Table,
    previous: i128,
    limit: i128,
    step: i128,
    repeated: Option<Value>,
) -> Option<(i128, u64, bool)> {
    let first = previous + step;
    let count = if step > 0 && first <= limit {
        (limit - first) / step + 1
    } else if step < 0 && first >= limit {
        (first - limit) / -step + 1
    } else {
        return None;
    };
    // Check the values in the loop as well as the final backedge addition.
    let after_last = first + count * step;
    if count < 2 || first.abs() > EXACT_INTEGER || after_last.abs() > EXACT_INTEGER {
        return None;
    }
    let written =
        table.write_reserved_number_span(first as i64, step as i64, count as u64, repeated)?;
    Some((
        previous + i128::from(written) * step,
        written,
        i128::from(written) == count,
    ))
}

pub(in crate::vm) fn dense_loop_store(control: usize, body: &[Instruction]) -> Option<Instruction> {
    let [store, rest @ ..] = body else {
        return None;
    };
    let index = control + 3;
    (store.op == Op::SetTable
        && index < RK as usize
        && usize::from(store.b) == index
        && usize::from(store.a) < control
        && (rest.is_empty()
            || matches!(rest, [close] if close.op == Op::Close && usize::from(close.a) == index)))
    .then_some(*store)
}

impl Vm {
    pub(super) fn reduce_scalar_index_writes(
        &mut self,
        base: usize,
        control: usize,
        body: &[Instruction],
        prototype: &Prototype,
    ) -> Option<Reduction> {
        let index = control + 3;
        let (store, table, scratch) = if let Some(store) = dense_loop_store(control, body) {
            (store, self.registers[base + usize::from(store.a)], None)
        } else {
            let [global, store, rest @ ..] = body else {
                return None;
            };
            if global.op != Op::GetGlobal
                || global.b != 0
                || usize::from(global.a) <= index
                || store.op != Op::SetTable
                || store.a != global.a
                || usize::from(store.b) != index
                || !(rest.is_empty()
                    || matches!(rest, [close] if close.op == Op::Close && usize::from(close.a) == index))
            {
                return None;
            }
            let FrameKind::Lua(function) = self.frames.last()?.function else {
                return None;
            };
            let closure = &self.functions[slot_index(function.0)];
            let constants = self.programs[closure.program].bound_constants(closure.prototype);
            let Value::String(name) = *constants.get(global.c as usize)? else {
                return None;
            };
            let table = self.tables[slot_index(closure.env.0)].get_string(name);
            (*store, table, Some(base + usize::from(global.a)))
        };
        if !self.sort_fast_context()
            || self.open_upvalues.iter().any(|&id| {
                matches!(self.upvalues[id as usize],
                         Cell::Open(thread, register)
                             if thread == self.thread && register >= base + control)
            })
            || !self.registers.memory().uses_global_allocator()
        {
            return None;
        }

        let internal = base + control;
        let previous = integer(self.registers[internal])?;
        let limit = integer(self.registers[internal + 1])?;
        let step = integer(self.registers[internal + 2])?;
        let Value::Table(id) = table else {
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
        // With step one, the current internal index is the zero-based slot
        // for the next write. Preserve the existing dense-array path.
        let dense = if step == 1
            && let (Ok(start), Ok(end)) = (u32::try_from(previous), u32::try_from(limit))
        {
            let (start, end) = (start as usize, end as usize);
            if end.saturating_sub(start) < 2 {
                return None;
            }
            if start == 0 && repeated.is_none() && table.share_dense_sequence(end) {
                Some((limit, end as u64, true))
            } else if let Some((stop, capacity)) = table.dense_write_span(start, end, growth) {
                if stop <= start {
                    return Some(Reduction::Retry);
                }
                let before = table.heap_bytes();
                // Dispatch retries any failed reservation at the ordinary store.
                if let Some(value) = repeated {
                    table.write_dense_span(start, stop, capacity, |_| value)
                } else {
                    table.write_dense_indices(start, stop, capacity)
                }
                .ok()?;
                let after = table.heap_bytes();
                self.note_table_resize(before, after);
                Some((stop as i128, (stop - start) as u64, stop == end))
            } else {
                None
            }
        } else {
            None
        };
        let (last, written, complete) = if let Some(dense) = dense {
            dense
        } else {
            write_reserved_indices(
                &mut self.tables[slot_index(id.0)],
                previous,
                limit,
                step,
                repeated,
            )?
        };
        if written == 0 {
            return Some(Reduction::Retry);
        }
        // Planning proved that every skipped checkpoint is below threshold.
        self.gc.pending = false;
        let index = Value::Number(last as f64);
        self.registers[internal] = index;
        self.registers[internal + 3] = index;
        if let Some(scratch) = scratch {
            self.registers[scratch] = Value::Table(id);
        }
        if complete {
            Some(Reduction::Applied(written * (body.len() + 1) as u64))
        } else {
            Some(Reduction::Partial)
        }
    }
}
