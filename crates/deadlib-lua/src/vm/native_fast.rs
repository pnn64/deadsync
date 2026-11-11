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
        if count != 0
            && let Value::Number(x) = self.registers[call + 1]
            && let Some(result) = Self::math_unary(id, x)
        {
            self.registers[call] = Value::Number(result);
            return Some(1);
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
