//! Non-allocating builtin results when no frame observer is active.
use super::*;
impl Vm {
    #[inline(never)]
    pub(super) fn try_fast_native(&mut self, id: u64, call: usize, count: usize) -> Option<usize> {
        if !self.sort_fast_context() {
            return None;
        }
        match id {
            0 if count != 0 && self.registers[call + 1].is_truthy() => {
                self.registers.copy_within(call + 1..call + 1 + count, call);
                Some(count)
            }
            2 if count != 0 => {
                let name = self.registers[call + 1].type_name();
                let id = self.strings.find(name.as_bytes())?;
                self.strings.bytes.preserve_new_reference(id.0);
                self.registers[call] = Value::String(id);
                Some(1)
            }
            3 if count == 1 => {
                self.registers[call] = self
                    .number(self.registers[call + 1])
                    .map_or(Value::Nil, Value::Number);
                Some(1)
            }
            4 if count >= 2 => {
                self.registers[call] =
                    Value::Boolean(self.registers[call + 1] == self.registers[call + 2]);
                Some(1)
            }
            5 if count >= 2
                && let Value::Table(id) = self.registers[call + 1] =>
            {
                self.registers[call] = self.tables.get(id.0)?.get(self.registers[call + 2]);
                Some(1)
            }
            tables::TAB_GETN | tables::TAB_MAXN
                if count != 0
                    && let Value::Table(table) = self.registers[call + 1] =>
            {
                let table = self.tables.get(table.0)?;
                let size = if id == tables::TAB_GETN {
                    table.len() as f64
                } else {
                    table.max_numeric_key()
                };
                self.registers[call] = Value::Number(size);
                Some(1)
            }
            // Other arities, coercion, and errors use ordinary dispatch.
            strings::STR_LEN | strings::STR_BYTE
                if count == 1
                    && let Value::String(source) = self.registers[call + 1] =>
            {
                let bytes = self.strings.bytes.get(source.0)?;
                let number = if id == strings::STR_LEN {
                    bytes.len() as f64
                } else if let Some(&byte) = bytes.first() {
                    f64::from(byte)
                } else {
                    return Some(0);
                };
                self.registers[call] = Value::Number(number);
                Some(1)
            }
            IPAIRS_STEP if count >= 2 => {
                let (Value::Table(table), Value::Number(previous)) =
                    (self.registers[call + 1], self.registers[call + 2])
                else {
                    return None;
                };
                let next = (previous as i32).wrapping_add(1);
                let value = self.tables.get(table.0)?.get_integer(i64::from(next));
                if value == Value::Nil {
                    return Some(0);
                }
                self.registers[call] = Value::Number(f64::from(next));
                self.registers[call + 1] = value;
                Some(2)
            }
            NEXT | PAIRS_NEXT
                if count != 0
                    && let Value::Table(table) = self.registers[call + 1] =>
            {
                let key = if count > 1 {
                    self.registers[call + 2]
                } else {
                    Value::Nil
                };
                if let Some((key, value)) = self.tables.get(table.0)?.next_entry(key).ok()? {
                    self.registers[call] = key;
                    self.registers[call + 1] = value;
                    Some(2)
                } else {
                    self.registers[call] = Value::Nil;
                    Some(1)
                }
            }
            _ => None,
        }
    }

    #[inline(never)]
    pub(super) fn try_fast_select(&mut self, call: usize, count: usize) -> Option<usize> {
        if count == 0 || !self.sort_fast_context() {
            return None;
        }
        let available = count - 1;
        match self.registers[call + 1] {
            Value::String(id)
                if self
                    .string_bytes(id)
                    .is_some_and(|bytes| bytes.first() == Some(&b'#')) =>
            {
                self.registers[call] = Value::Number(available as f64);
                Some(1)
            }
            Value::Number(index) => {
                let start = native::select_start(lua_integer(index) as i32, available)?;
                self.registers
                    .copy_within(call + 2 + start..call + 2 + available, call);
                Some(available - start)
            }
            _ => None,
        }
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
