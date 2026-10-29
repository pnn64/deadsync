//! Array storage and indexed hash entries with deletion-safe linear iteration.

use crate::{
    Error, ErrorKind, FunctionId, ParserPrototypeId, StringId, TableId, ThreadId, UserdataId, Value,
};

#[derive(Clone, Copy, Debug)]
enum Key {
    Boolean(bool),
    Number(u64),
    String(StringId),
    Table(TableId),
    Function(FunctionId),
    Native(u64),
    Thread(ThreadId),
    Userdata(UserdataId),
    LightUserdata(u64),
    ParserPrototype(ParserPrototypeId),
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}
impl Eq for Key {}

impl Key {
    fn from_value(value: Value) -> Option<Self> {
        Some(match value {
            Value::Nil => return None,
            Value::Boolean(v) => Self::Boolean(v),
            Value::Number(v) if v.is_nan() => return None,
            Value::Number(v) => Self::Number(v.to_bits()),
            Value::String(v) => Self::String(v),
            Value::Table(v) => Self::Table(v),
            Value::Function(v) => Self::Function(v),
            Value::Native(v) => Self::Native(v),
            Value::Thread(v) => Self::Thread(v),
            Value::Userdata(v) => Self::Userdata(v),
            Value::LightUserdata(v) => Self::LightUserdata(v),
            Value::ParserPrototype(v) => Self::ParserPrototype(v),
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
            Self::Userdata(v) => Value::Userdata(v),
            Self::LightUserdata(v) => Value::LightUserdata(v),
            Self::ParserPrototype(v) => Value::ParserPrototype(v),
        }
    }
    fn identity(self) -> (u64, u64) {
        match self {
            Self::Boolean(v) => (u64::from(v), 1),
            Self::Number(v) => (if v << 1 == 0 { 0 } else { v }, 2),
            Self::String(v) => (v.0, 3),
            Self::Table(v) => (v.0, 4),
            Self::Function(v) => (v.0, 5),
            Self::Native(v) => (v, 6),
            Self::Thread(v) => (v.0, 7),
            Self::Userdata(v) => (v.0, 8),
            Self::LightUserdata(v) => (v, 9),
            Self::ParserPrototype(v) => (v.0, 10),
        }
    }
    fn hash(self) -> usize {
        let (bits, tag) = self.identity();
        let mut h = bits
            ^ tag
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
    // Arena identities have a nonzero generation; zero denotes no metatable.
    // This leaves room for the scan state without growing the table header.
    metatable: u64,
    pub trace: TableTrace,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TableTrace {
    #[default]
    Idle,
    Pending(bool, bool),
    Restart,
}

impl Table {
    pub fn with_capacity(array: usize, records: usize) -> Result<Self, Error> {
        let failed = Error::memory;
        let mut table = Self::default();
        table.array.try_reserve_exact(array).map_err(|_| failed())?;
        table.array.resize(array, Value::Nil);
        if records > (1 << 26) {
            return Err(Error::new(ErrorKind::Runtime, 0, "table overflow"));
        }
        let slots = if records == 0 {
            0
        } else {
            records
                .checked_mul(4)
                .and_then(|n| (n / 3 + 1).max(8).checked_next_power_of_two())
                .ok_or_else(failed)?
        };
        table
            .entries
            .try_reserve_exact(records)
            .map_err(|_| failed())?;
        table.slots.try_reserve_exact(slots).map_err(|_| failed())?;
        table.slots.resize(slots, 0);
        Ok(table)
    }
    pub fn metatable(&self) -> Option<TableId> {
        (self.metatable != 0).then_some(TableId(self.metatable))
    }
    pub fn set_metatable(&mut self, table: Option<TableId>) {
        self.metatable = table.map_or(0, |id| id.0);
    }
    pub fn with_record_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::default();
        }
        // Keep the final insert below the strict 3/4 rebuild threshold.
        let slots = (capacity.checked_mul(4).expect("table capacity overflow") / 3 + 1)
            .max(8)
            .next_power_of_two();
        Self {
            entries: Vec::with_capacity(capacity),
            slots: vec![0; slots],
            ..Self::default()
        }
    }
    pub fn heap_bytes(&self) -> usize {
        self.array.capacity() * std::mem::size_of::<Value>()
            + self.entries.capacity() * std::mem::size_of::<Entry>()
            + self.slots.capacity() * 4
    }
    pub fn pairs(&self) -> impl Iterator<Item = (Value, Value)> + '_ {
        self.array
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != Value::Nil)
            .map(|(i, v)| (Value::Number((i + 1) as f64), *v))
            .chain(
                self.entries
                    .iter()
                    .filter(|e| e.value != Value::Nil)
                    .map(|e| (e.key.value(), e.value)),
            )
    }
    /// Visit at most `budget` physical slots, including holes and tombstones.
    /// The low cursor bit selects array/hash storage; the other bits are an index.
    pub fn trace_slice(
        &self,
        cursor: usize,
        budget: usize,
    ) -> (Option<usize>, impl Iterator<Item = (Value, Value)> + '_) {
        let start = cursor >> 1;
        let (array, entries, next) = if cursor & 1 == 0 {
            let end = start.saturating_add(budget).min(self.array.len());
            let hash_end = if end == self.array.len() {
                budget.saturating_sub(end - start).min(self.entries.len())
            } else {
                0
            };
            let next = if end < self.array.len() {
                Some(end << 1)
            } else {
                (hash_end < self.entries.len()).then_some((hash_end << 1) | 1)
            };
            (&self.array[start..end], &self.entries[..hash_end], next)
        } else {
            let end = start.saturating_add(budget).min(self.entries.len());
            (
                &self.array[..0],
                &self.entries[start..end],
                (end < self.entries.len()).then_some((end << 1) | 1),
            )
        };
        (
            next,
            array
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != Value::Nil)
                .map(move |(i, v)| (Value::Number((start + i + 1) as f64), *v))
                .chain(
                    entries
                        .iter()
                        .filter(|e| e.value != Value::Nil)
                        .map(|e| (e.key.value(), e.value)),
                ),
        )
    }
    fn invalidate_trace(&mut self) {
        if self.trace != TableTrace::Idle {
            self.trace = TableTrace::Restart;
        }
    }
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
        let old_len = self.entries.len();
        self.entries.retain(|entry| entry.value != Value::Nil);
        if self.entries.len() != old_len {
            self.invalidate_trace();
        }
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
        if old != Value::Nil {
            self.invalidate_trace();
        }
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
