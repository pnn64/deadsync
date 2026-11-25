//! Non-allocating builtin results when no frame observer is active.
use super::*;
impl Vm {
    pub(super) fn try_inline_extremum(
        &mut self,
        call: usize,
        keep: bool,
        minimum: bool,
        checkpoint_ready: bool,
    ) -> bool {
        let (Value::Number(first), Value::Number(second)) =
            (self.registers[call + 1], self.registers[call + 2])
        else {
            return false;
        };
        if self.frames.len() == self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
            || (!checkpoint_ready && !(self.sort_fast_context() && self.sort_gc_ready()))
        {
            return false;
        }
        let replace = if minimum {
            second < first
        } else {
            second > first
        };
        self.registers[call] = Value::Number(if replace { second } else { first });
        self.top = call + usize::from(keep);
        true
    }

    pub(super) fn try_inline_assert(
        &mut self,
        call: usize,
        keep: bool,
        checkpoint_ready: bool,
    ) -> bool {
        let value = self.registers[call + 1];
        if !value.is_truthy()
            || self.frames.len() == self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
            || (!checkpoint_ready && !(self.sort_fast_context() && self.sort_gc_ready()))
        {
            return false;
        }
        self.registers[call] = value;
        self.top = call + usize::from(keep);
        true
    }

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
                let table = self.tables.get(id.0)?;
                self.registers[call] = match self.registers[call + 2] {
                    Value::String(key) => table.get_string(key),
                    key => table.get(key),
                };
                Some(1)
            }
            6 if count >= 3
                && let Value::Table(id) = self.registers[call + 1] =>
            {
                let key = self.registers[call + 2];
                let value = self.registers[call + 3];
                if !self
                    .tables
                    .get_mut(id.0)?
                    .set_array_scalar_in_place(key, value)
                {
                    return None;
                }
                self.registers[call] = Value::Table(id);
                Some(1)
            }
            GET_META | DEBUG_GET_META if count != 0 => {
                self.registers[call] =
                    self.metatable_result(self.registers[call + 1], id == GET_META);
                Some(1)
            }
            55 | 56
                if count != 0
                    && let Value::Number(value) = self.registers[call + 1] =>
            {
                let (first, second) = Self::math_split(id, value);
                self.registers[call] = Value::Number(first);
                self.registers[call + 1] = Value::Number(second);
                Some(2)
            }
            57 if count >= 2
                && let (Value::Number(value), Value::Number(exponent)) =
                    (self.registers[call + 1], self.registers[call + 2]) =>
            {
                self.registers[call] =
                    Value::Number(math::ldexp(value, lua_integer(exponent) as i32));
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
    pub(super) fn try_fast_string(&mut self, id: u64, call: usize, count: usize) -> Option<usize> {
        if !matches!(
            (id, count),
            (strings::STR_LEN, 1) | (strings::STR_BYTE, 1 | 2)
        ) || !self.sort_fast_context()
        {
            return None;
        }
        let Value::String(source) = self.registers[call + 1] else {
            return None;
        };
        let bytes = self.strings.bytes.get(source.0)?;
        let number = if id == strings::STR_LEN {
            bytes.len() as f64
        } else {
            let offset = if count == 1 {
                0
            } else {
                let Value::Number(index) = self.registers[call + 2] else {
                    return None;
                };
                (strings::relative(lua_integer(index), bytes.len()) as usize).wrapping_sub(1)
            };
            let Some(&byte) = bytes.get(offset) else {
                return Some(0);
            };
            f64::from(byte)
        };
        self.registers[call] = Value::Number(number);
        Some(1)
    }

    #[inline(never)]
    pub(super) fn try_fast_find_byte(&mut self, call: usize, count: usize) -> Option<usize> {
        // Let ordinary dispatch reserve the native argument window if it needs growth.
        if !(2..=4).contains(&count) || self.registers.capacity() - self.registers.len() <= count {
            return None;
        }
        let (Value::String(source), Value::String(pattern)) =
            (self.registers[call + 1], self.registers[call + 2])
        else {
            return None;
        };
        let [byte] = self.string_bytes(pattern)? else {
            return None;
        };
        if (!self.argument(call, count, 3).is_truthy() && crate::pattern::has_magic(&[*byte]))
            || !self.sort_fast_context()
        {
            return None;
        }
        let raw_start = match self.argument(call, count, 2) {
            Value::Nil => 1,
            Value::Number(index) => lua_integer(index),
            _ => return None,
        };
        let source = self.string_bytes(source)?;
        let start = strings::find_start(raw_start, source.len());
        if let Some(offset) = crate::pattern::find_byte(&source[start..], *byte) {
            let position = Value::Number((start + offset + 1) as f64);
            self.registers[call] = position;
            self.registers[call + 1] = position;
            Some(2)
        } else {
            self.registers[call] = Value::Nil;
            Some(1)
        }
    }

    #[inline(never)]
    pub(super) fn try_fast_char(&mut self, call: usize, count: usize) -> Option<usize> {
        // Preserve the allocation behavior of reserving and taking scratch storage.
        if self.buffers.len() == self.buffers.capacity()
            || self.buffers.last()?.capacity() < count
            || !self.sort_fast_context()
        {
            return None;
        }
        let id = if count == 0 {
            self.strings.find(b"")?
        } else {
            let Value::Number(number) = self.registers[call + 1] else {
                return None;
            };
            self.strings.find(&[strings::char_byte(number)?])?
        };
        self.strings.bytes.preserve_new_reference(id.0);
        self.registers[call] = Value::String(id);
        Some(1)
    }

    #[inline(never)]
    pub(super) fn try_fast_repeat(&mut self, call: usize, repeat: f64) -> Option<usize> {
        let Value::String(source) = self.registers[call + 1] else {
            return None;
        };
        let repeat = lua_integer(repeat) as i32;
        if repeat > 1 || !self.sort_fast_context() || !self.strings.bytes.contains(source.0) {
            return None;
        }
        let result = if repeat == 1 {
            source
        } else {
            self.strings.find(b"")?
        };
        self.strings.bytes.preserve_new_reference(result.0);
        self.registers[call] = Value::String(result);
        Some(1)
    }

    #[inline(never)]
    pub(super) fn try_fast_whole_substring(&mut self, call: usize) -> Option<usize> {
        let Value::String(source) = self.registers[call + 1] else {
            return None;
        };
        if !self.sort_fast_context() || !self.strings.bytes.contains(source.0) {
            return None;
        }
        self.strings.bytes.preserve_new_reference(source.0);
        self.registers[call] = Value::String(source);
        Some(1)
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
