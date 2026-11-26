use super::{
    BODY_LIMIT, Body, Instruction, Op, Operand, Reduction, STR_SUB, Series, Value, Vm, integer,
    slot_index,
};
use crate::{StringId, TableId};

struct SliceSpan {
    source: StringId,
    table: TableId,
    first: i64,
    last: i64,
    first_step: i64,
    last_step: i64,
}

impl Vm {
    #[inline(never)]
    pub(super) fn reduce_substring_array_loop(
        &mut self,
        base: usize,
        control: usize,
        code: &[Instruction],
        program: usize,
        prototype: usize,
    ) -> Option<Reduction> {
        let body = if matches!(code.last(), Some(i) if i.op == Op::Close) {
            &code[..code.len() - 1]
        } else {
            code
        };
        let [prefix @ .., call, store] = body else {
            return None;
        };
        if store.op != Op::SetTable
            || call.op != Op::Call
            || call.b != 3
            || call.c != 1
            || store.c != u32::from(call.a)
            || usize::from(store.b) != control + 3
            || usize::from(call.a) <= control + 3
            || body.len() > BODY_LIMIT
            || !self.sort_fast_context()
            || !self.loads.is_empty()
            || !self.open_upvalues.is_empty()
            || !self.registers.memory().uses_global_allocator()
        {
            return None;
        }
        let internal = base + control;
        if self.registers[internal + 2] != Value::Number(1.0) {
            return None;
        }
        let start = u32::try_from(integer(self.registers[internal])?).ok()? as usize;
        let end = u32::try_from(integer(self.registers[internal + 1])?).ok()? as usize;
        if end.saturating_sub(start) < 2 {
            return None;
        }
        if self.frames.len() == self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
            || self.dense_array_gc_growth().is_none()
        {
            return Some(Reduction::Retry);
        }

        let indices = Series {
            first: start as i128 + 1,
            last: end as i128,
        };
        let mut analysis = Body::new(
            self,
            &self.registers[base..],
            self.programs[program].bound_constants(prototype),
            control,
            None,
            indices,
            indices.last - indices.first + 1,
        )?;
        let mut span = analysis.substring_store(prefix, *call, *store)?;

        // Leave the last iteration to dispatch to materialize its scratch
        // registers and stack top. At a GC checkpoint, resume ordinary calls
        // before a boundary where allocation or a collector callback can run.
        let stop = self.write_substring_spans(start, end - 1, &mut span);
        if stop != start {
            let index = Value::Number(stop as f64);
            self.registers[internal] = index;
            self.registers[internal + 3] = index;
            self.gc.pending = false;
        }
        if stop == end - 1 {
            Some(Reduction::Applied(
                (stop - start) as u64 * (code.len() + 1) as u64,
            ))
        } else {
            Some(Reduction::Retry)
        }
    }

    fn write_substring_spans(&mut self, start: usize, end: usize, span: &mut SliceSpan) -> usize {
        let table = slot_index(span.table.0);
        let mut values = [Value::Nil; 128];
        let mut written = start;
        while written < end {
            let Some(growth) = self.dense_array_gc_growth() else {
                break;
            };
            let target = &self.tables[table];
            let Some((stop, capacity)) = target.dense_write_span(
                written,
                end.min(written.saturating_add(values.len())),
                growth,
            ) else {
                break;
            };
            let before = target.heap_bytes();
            let growth = growth - target.dense_write_growth(capacity);
            let initial_debt = self.strings.bytes.allocated;
            let mut count = 0;
            for value in &mut values[..stop - written] {
                let range = (span.first - 1) as usize..span.last as usize;
                let Ok(id) = self.strings.substring(span.source, range) else {
                    break;
                };
                if self.strings.bytes.allocated - initial_debt > growth {
                    break;
                }
                *value = Value::String(id);
                span.first += span.first_step;
                span.last += span.last_step;
                count += 1;
            }
            if count == 0 {
                break;
            }
            let target = &mut self.tables[table];
            let actual_stop = written + count;
            let capacity = if actual_stop == stop {
                capacity
            } else {
                target
                    .dense_write_span(written, actual_stop, usize::MAX)
                    .expect("shorter dense write fits")
                    .1
            };
            // The intern lookup protects each returned string during marking
            // and sweep, so stores need no additional marking barrier.
            if target
                .write_dense_span(written, actual_stop, capacity, |i| values[i - written])
                .is_err()
            {
                break;
            }
            let after = target.heap_bytes();
            self.note_table_resize(before, after);
            written = actual_stop;
            if actual_stop < stop {
                break;
            }
        }
        written
    }
}

impl Body<'_> {
    fn substring_store(
        &mut self,
        prefix: &[Instruction],
        call: Instruction,
        store: Instruction,
    ) -> Option<SliceSpan> {
        for i in prefix {
            let destination = usize::from(i.a);
            if destination <= self.control + 3 {
                return None;
            }
            let value = match i.op {
                Op::Load if i.b == 0 => Operand::Constant(self.constants[i.c as usize]),
                Op::GetUpvalue => self.closed_upvalue(i.c as usize)?,
                Op::Move if i.c == 0 => self.read(usize::from(i.b))?,
                Op::Add | Op::Sub | Op::Mul => {
                    let left = self.operand(u32::from(i.b))?;
                    let right = self.operand(i.c)?;
                    Operand::Series(self.series(left)?.arithmetic(i.op, self.series(right)?)?)
                }
                _ => return None,
            };
            self.write(destination, value)?;
        }
        let call = usize::from(call.a);
        let callee = self.read(call)?;
        if self.literal(callee)? != Value::Native(STR_SUB) {
            return None;
        }
        let source = self.read(call + 1)?;
        let Value::String(source) = self.literal(source)? else {
            return None;
        };
        let first = self.read(call + 2)?;
        let last = self.read(call + 3)?;
        let first = self.series(first)?;
        let last = self.series(last)?;
        let table = self.table(usize::from(store.a))?;
        let bytes = self.vm.strings.bytes[slot_index(source.0)].len() as i128;
        if self.read_initial[call]
            || self
                .read_initial
                .iter()
                .zip(self.bindings)
                .any(|(&read, bound)| read && bound != 0)
            || first.first.min(first.last) < 1
            || last.first.max(last.last) > bytes
            || first.first > last.first
            || first.last > last.last
        {
            return None;
        }
        // The divisor is positive and exact series already bound all operands.
        let intervals = self.iterations - 1;
        Some(SliceSpan {
            source,
            table,
            first: first.first as i64,
            last: last.first as i64,
            first_step: ((first.last - first.first) / intervals) as i64,
            last_step: ((last.last - last.first) / intervals) as i64,
        })
    }

    pub(super) fn substring(&mut self, call: usize, count: usize) -> Option<Operand> {
        if !self.vm.registers.memory().uses_global_allocator() {
            return None;
        }
        let source = self.read(call + 1)?;
        let Value::String(source) = self.literal(source)? else {
            return None;
        };
        let bytes = self.vm.strings.bytes[slot_index(source.0)].len() as i128;
        let start = self.read(call + 2)?;
        let start = self.series(start)?;
        let default_end = Series::splat(-1);
        let end = if count == 2 {
            default_end
        } else {
            let operand = self.read(call + 3)?;
            if let Some(end) = self.series(operand) {
                end
            } else {
                (self.literal(operand)? == Value::Nil).then_some(default_end)?
            }
        };
        let start = self.relative_index(start, bytes)?;
        let end = self.relative_index(end, bytes)?;
        let one = Series::splat(1);
        let limit = Series::splat(bytes);
        let start = if self.le_throughout(one, start)? {
            start
        } else {
            one
        };
        let end = if self.le_throughout(end, limit)? {
            end
        } else {
            limit
        };
        let length = Series {
            first: end.first - start.first + 1,
            last: end.last - start.last + 1,
        };
        // An in-range, nonempty slice has this affine byte length. Empty
        // slices run through the regular accumulator and string operations.
        if length.first.min(length.last) <= 0 {
            return None;
        }
        self.call_registers.get_or_insert(0);
        // The analysis can pass this value through moves and measure its length.
        // Dispatch materializes the final iteration's ordinary string result.
        Some(Operand::Substring(length.checked()?))
    }

    fn relative_index(&mut self, index: Series, bytes: i128) -> Option<Series> {
        // Match Lua's native integer conversion. The affine endpoints prove
        // every intervening value is also representable.
        isize::try_from(index.first).ok()?;
        isize::try_from(index.last).ok()?;
        if self.le_throughout(index, Series::splat(-1))? {
            // The scalar relative-index helper floors at zero. Such a floor
            // cannot affect the length of a nonempty slice with start >= 1.
            Some(Series {
                first: index.first + bytes + 1,
                last: index.last + bytes + 1,
            })
        } else {
            Some(index)
        }
    }
}
