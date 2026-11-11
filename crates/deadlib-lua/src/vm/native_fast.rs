//! Non-allocating builtin results when no frame observer is active.
use super::*;
impl Vm {
    #[inline(never)]
    pub(super) fn try_fast_native(&mut self, id: u64, call: usize, count: usize) -> Option<usize> {
        if !self.sort_fast_context() {
            return None;
        }
        if id == 0 && count != 0 && self.registers[call + 1].is_truthy() {
            self.registers.copy_within(call + 1..call + 1 + count, call);
            return Some(count);
        }
        if id == 3
            && count == 1
            && let Value::Number(number) = self.registers[call + 1]
        {
            self.registers[call] = Value::Number(number);
            return Some(1);
        }
        if id == 4 && count >= 2 {
            self.registers[call] =
                Value::Boolean(self.registers[call + 1] == self.registers[call + 2]);
            return Some(1);
        }
        if id == 5
            && count >= 2
            && let Value::Table(id) = self.registers[call + 1]
            && let Some(table) = self.tables.get(id.0)
        {
            self.registers[call] = table.get(self.registers[call + 2]);
            return Some(1);
        }
        if count != 0
            && let Value::Number(x) = self.registers[call + 1]
            && let Some(result) = Self::math_unary(id, x)
        {
            self.registers[call] = Value::Number(result);
            return Some(1);
        }
        // Additional arguments, number-to-string coercion, and errors use
        // ordinary dispatch. An empty string.byte result has zero values.
        if count == 1
            && matches!(id, strings::STR_LEN | strings::STR_BYTE)
            && let Value::String(source) = self.registers[call + 1]
            && let Some(bytes) = self.strings.bytes.get(source.0)
        {
            if id == strings::STR_LEN {
                self.registers[call] = Value::Number(bytes.len() as f64);
                return Some(1);
            }
            return match bytes.first() {
                Some(&byte) => {
                    self.registers[call] = Value::Number(f64::from(byte));
                    Some(1)
                }
                None => Some(0),
            };
        }
        if matches!(id, NEXT | PAIRS_NEXT) && count != 0 {
            let Value::Table(table) = self.registers[call + 1] else {
                return None;
            };
            let key = if count > 1 {
                self.registers[call + 2]
            } else {
                Value::Nil
            };
            let next = self.tables.get(table.0)?.next_entry(key).ok()?;
            return match next {
                Some((key, value)) => {
                    self.registers[call] = key;
                    self.registers[call + 1] = value;
                    Some(2)
                }
                None => {
                    self.registers[call] = Value::Nil;
                    Some(1)
                }
            };
        }
        None
    }
}
