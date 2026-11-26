use super::{Body, Operand, Series, Value, slot_index};

impl Body<'_> {
    pub(super) fn substring(&mut self, call: usize) -> Option<Operand> {
        if !self.vm.registers.memory().uses_global_allocator() {
            return None;
        }
        let source = self.read(call + 1)?;
        let Value::String(source) = self.literal(source)? else {
            return None;
        };
        let bytes = self.vm.strings.bytes[slot_index(source.0)].len();
        let start = self.read(call + 2)?;
        let end = self.read(call + 3)?;
        let start = self.series(start)?;
        let end = self.series(end)?;
        let length = in_bounds_length(start, end, bytes)?;
        self.call_registers.get_or_insert(0);
        // The analysis can pass this value through moves and measure its length.
        // Dispatch materializes the final iteration's ordinary string result.
        Some(Operand::Substring(length))
    }
}

fn in_bounds_length(start: Series, end: Series, bytes: usize) -> Option<Series> {
    let limit = bytes.min(isize::MAX as usize) as i128;
    if start.first.min(start.last) < 1 || end.first.max(end.last) > limit {
        return None;
    }
    let first = end.first - start.first + 1;
    let last = end.last - start.last + 1;
    if first.min(last) <= 0 {
        return None;
    }
    Series { first, last }.checked()
}
