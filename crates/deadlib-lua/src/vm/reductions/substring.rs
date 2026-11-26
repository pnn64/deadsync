use super::{Body, Operand, Series, Value, slot_index};

impl Body<'_> {
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
