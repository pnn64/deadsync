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
        if id == 3 && count == 1 {
            self.registers[call] = self
                .number(self.registers[call + 1])
                .map_or(Value::Nil, Value::Number);
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

    pub(super) fn try_fast_math(&mut self, id: u64, call: usize, count: usize) -> Option<usize> {
        if count == 0 || !self.sort_fast_context() {
            return None;
        }
        let Value::Number(mut result) = self.registers[call + 1] else {
            return None;
        };
        if matches!(id, 43 | 44) {
            for &value in self.registers.get(call + 2..call + 1 + count)? {
                let Value::Number(number) = value else {
                    return None;
                };
                if if id == 43 {
                    number < result
                } else {
                    number > result
                } {
                    result = number;
                }
            }
        } else {
            result = Self::math_unary(id, result)?;
        }
        self.registers[call] = Value::Number(result);
        Some(1)
    }

    #[inline(never)]
    pub(super) fn try_fast_binary_math(
        &mut self,
        id: u64,
        call: usize,
        count: usize,
    ) -> Option<usize> {
        if count < 2 || !self.sort_fast_context() {
            return None;
        }
        let (Value::Number(left), Value::Number(right)) =
            (self.registers[call + 1], self.registers[call + 2])
        else {
            return None;
        };
        self.registers[call] = Value::Number(Self::math_binary(id, left, right));
        Some(1)
    }
}
