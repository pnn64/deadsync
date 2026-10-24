//! Array storage and indexed hash entries with deletion-safe linear iteration.

use crate::{Error, ErrorKind, FunctionId, StringId, TableId, ThreadId, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Key {
    Boolean(bool),
    Number(u64),
    String(StringId),
    Table(TableId),
    Function(FunctionId),
    Native(u32),
    Thread(ThreadId),
}

impl Key {
    fn from_value(value: Value) -> Option<Self> {
        Some(match value {
            Value::Nil => return None,
            Value::Boolean(v) => Self::Boolean(v),
            Value::Number(v) if v.is_nan() => return None,
            Value::Number(v) => Self::Number(if v == 0.0 { 0 } else { v.to_bits() }),
            Value::String(v) => Self::String(v),
            Value::Table(v) => Self::Table(v),
            Value::Function(v) => Self::Function(v),
            Value::Native(v) => Self::Native(v),
            Value::Thread(v) => Self::Thread(v),
        })
    }
    fn value(self) -> Value {
        match self {
            Self::Boolean(v) => Value::Boolean(v),
            Self::Number(v) => Value::Number(f64::from_bits(v)),
            Self::String(v) => Value::String(v),
            Self::Table(v) => Value::Table(v),
            Self::Function(v) => Value::Function(v),
            Self::Native(v) => Value::Native(v),
            Self::Thread(v) => Value::Thread(v),
        }
    }
    fn hash(self) -> usize {
        let (bits, tag) = match self {
            Self::Boolean(v) => (u64::from(v), 1),
            Self::Number(v) => (v, 2),
            Self::String(v) => (u64::from(v.0), 3),
            Self::Table(v) => (u64::from(v.0), 4),
            Self::Function(v) => (u64::from(v.0), 5),
            Self::Native(v) => (u64::from(v), 6),
            Self::Thread(v) => (u64::from(v.0), 7),
        };
        let mut h = bits
            ^ (tag as u64)
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add(bits.rotate_left(17));
        h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
        (h ^ (h >> 31)) as usize
    }
}

fn array_index(value: Value) -> Option<usize> {
    if let Value::Number(n) = value
        && n >= 1.0
        && n <= f64::from(u32::MAX)
        && n.fract() == 0.0
    {
        Some(n as usize - 1)
    } else {
        None
    }
}

#[derive(Debug)]
struct Entry {
    key: Key,
    value: Value,
}

#[derive(Debug, Default)]
pub(crate) struct Table {
    array: Vec<Value>,
    entries: Vec<Entry>,
    slots: Vec<u32>,
    pub metatable: Option<TableId>,
}

impl Table {
    fn slot(&self, key: Key) -> Option<(usize, Option<usize>)> {
        if self.slots.is_empty() {
            return None;
        }
        let mut slot = key.hash() & (self.slots.len() - 1);
        loop {
            let index = self.slots[slot];
            if index == 0 {
                return Some((slot, None));
            }
            if self.entries[index as usize - 1].key == key {
                return Some((slot, Some(index as usize - 1)));
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }
    fn rebuild(&mut self) {
        self.entries.retain(|entry| entry.value != Value::Nil);
        if (self.entries.len() + 1) * 4 >= self.slots.len() * 3 {
            self.slots.resize((self.slots.len() * 2).max(8), 0);
        }
        self.slots.fill(0);
        for (i, entry) in self.entries.iter().enumerate() {
            let mut slot = entry.key.hash() & (self.slots.len() - 1);
            while self.slots[slot] != 0 {
                slot = (slot + 1) & (self.slots.len() - 1);
            }
            self.slots[slot] =
                u32::try_from(i + 1).expect("Lua table exhausted addressable entries");
        }
    }
    fn take_hash(&mut self, key: Key) -> Option<Value> {
        let (_, Some(i)) = self.slot(key)? else {
            return None;
        };
        let old = std::mem::replace(&mut self.entries[i].value, Value::Nil);
        (old != Value::Nil).then_some(old)
    }
    pub fn get(&self, key: Value) -> Value {
        if let Some(i) = array_index(key)
            && let Some(value) = self.array.get(i)
        {
            return *value;
        }
        Key::from_value(key)
            .and_then(|key| self.slot(key))
            .and_then(|(_, i)| i)
            .map_or(Value::Nil, |i| self.entries[i].value)
    }
    pub fn set(&mut self, key: Value, value: Value) -> Result<(), Error> {
        let k = Key::from_value(key).ok_or_else(|| {
            Error::new(
                ErrorKind::Runtime,
                0,
                if key == Value::Nil {
                    "table index is nil"
                } else {
                    "table index is NaN"
                },
            )
        })?;
        if let Some(i) = array_index(key) {
            if i < self.array.len() {
                self.array[i] = value;
                return Ok(());
            }
            if i == self.array.len() && value != Value::Nil {
                self.array.push(value);
                self.take_hash(k);
                while let Some(v) =
                    self.take_hash(Key::Number(((self.array.len() + 1) as f64).to_bits()))
                {
                    self.array.push(v);
                }
                return Ok(());
            }
        }
        if let Some((_, Some(i))) = self.slot(k) {
            self.entries[i].value = value;
            return Ok(());
        }
        if value == Value::Nil {
            return Ok(());
        }
        if (self.entries.len() + 1) * 4 >= self.slots.len() * 3 {
            self.rebuild();
        }
        let (slot, _) = self.slot(k).expect("table rebuild creates slots");
        self.slots[slot] =
            u32::try_from(self.entries.len() + 1).expect("Lua table exhausted addressable entries");
        self.entries.push(Entry { key: k, value });
        Ok(())
    }
    pub fn len(&self) -> usize {
        let mut hi = self.array.len();
        if hi > 0 && self.array[hi - 1] == Value::Nil {
            let mut lo = 0;
            while hi - lo > 1 {
                let mid = (hi + lo) / 2;
                if self.array[mid - 1] == Value::Nil {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            return lo;
        }
        hi
    }
    pub fn next(&self, key: Value) -> Result<Option<(Value, Value)>, Error> {
        let (array, hash) = if key == Value::Nil {
            (0, 0)
        } else if let Some(i) = array_index(key).filter(|&i| i < self.array.len()) {
            (i + 1, 0)
        } else if let Some((_, Some(i))) = Key::from_value(key).and_then(|k| self.slot(k)) {
            (self.array.len(), i + 1)
        } else {
            return Err(Error::new(ErrorKind::Runtime, 0, "invalid key to 'next'"));
        };
        for (i, &value) in self.array.iter().enumerate().skip(array) {
            if value != Value::Nil {
                return Ok(Some((Value::Number((i + 1) as f64), value)));
            }
        }
        for entry in self.entries.iter().skip(hash) {
            if entry.value != Value::Nil {
                return Ok(Some((entry.key.value(), entry.value)));
            }
        }
        Ok(None)
    }
}
