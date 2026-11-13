//! Array storage and indexed hash entries with deletion-safe linear iteration.

pub(crate) mod sort;
use crate::memory::{Buffer, Memory};
use crate::{
    Error, ErrorKind, FunctionId, ParserPrototypeId, StringId, TableId, ThreadId, UserdataId, Value,
};
use std::sync::atomic::{AtomicU64, Ordering};

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
        match (*self, *other) {
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b || (a << 1 == 0 && b << 1 == 0),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Table(a), Self::Table(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => a == b,
            (Self::Native(a), Self::Native(b)) => a == b,
            (Self::Thread(a), Self::Thread(b)) => a == b,
            (Self::Userdata(a), Self::Userdata(b)) => a == b,
            (Self::LightUserdata(a), Self::LightUserdata(b)) => a == b,
            (Self::ParserPrototype(a), Self::ParserPrototype(b)) => a == b,
            _ => false,
        }
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
    /// One multiplication mixes every identity bit into the upper half, which
    /// the fold brings down to the low bits that select a slot. Slot order is
    /// never observable: traversal follows insertion order.
    #[inline]
    fn hash(self) -> usize {
        let (bits, tag) = self.identity();
        let h = (bits ^ tag.rotate_left(59)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        (h ^ (h >> 32)) as usize
    }
}

fn array_index(value: Value) -> Option<usize> {
    if let Value::Number(n) = value
        && n >= 1.0
        && n <= f64::from(u32::MAX)
    {
        let index = n as u32;
        (f64::from(index) == n).then_some(index as usize - 1)
    } else {
        None
    }
}

#[derive(Debug)]
struct Entry {
    key: Key,
    value: Value,
}

#[derive(Debug)]
pub(crate) struct InvalidNextKey;

#[derive(Debug, Default)]
pub(crate) struct Table {
    array: Buffer<Value>,
    entries: Buffer<Entry>,
    slots: Buffer<u32>,
    // Two one-based hash entry indices, packed into one atomic word. Always
    // compare keys before reuse: mutations can move or remove entries.
    next_hash_hint: AtomicU64,
    // Arena identities have a nonzero generation; zero denotes no metatable.
    // The private attachment below uses the same optional-identity encoding.
    metatable: u64,
    // Private embedding state, traced independently of Lua weak-table modes.
    host_state: u64,
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
    pub(crate) fn new_in(memory: Memory) -> Self {
        Self {
            array: Buffer::new_in(memory.clone()),
            entries: Buffer::new_in(memory.clone()),
            slots: Buffer::new_in(memory),
            next_hash_hint: AtomicU64::new(0),
            metatable: 0,
            host_state: 0,
            trace: TableTrace::Idle,
        }
    }
    #[cfg(test)]
    pub fn with_capacity(array: usize, records: usize) -> Result<Self, Error> {
        Self::with_capacity_in(array, records, Memory::default())
    }
    pub fn with_capacity_in(array: usize, records: usize, memory: Memory) -> Result<Self, Error> {
        let failed = Error::memory;
        let mut table = Self::new_in(memory);
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
    pub fn host_state(&self) -> Option<TableId> {
        (self.host_state != 0).then_some(TableId(self.host_state))
    }
    pub fn set_host_state(&mut self, table: Option<TableId>) {
        self.host_state = table.map_or(0, |id| id.0);
    }
    pub fn heap_bytes(&self) -> usize {
        self.array.capacity() * std::mem::size_of::<Value>()
            + self.entries.capacity() * std::mem::size_of::<Entry>()
            + self.slots.capacity() * 4
    }
    #[cfg(test)]
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
    // All storage was reserved before tombstones or traversal positions change.
    fn rebuild(&mut self, slots: usize) {
        let old_len = self.entries.len();
        self.entries.retain(|entry| entry.value != Value::Nil);
        if self.entries.len() != old_len {
            self.invalidate_trace();
        }
        self.slots.resize(slots, 0);
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
    // Equal finite numbers have identical bits after excluding negative zero,
    // so an unstable numeric sort cannot expose a changed tie permutation.
    pub(crate) fn scalar_sort_values(&mut self, len: usize, numbers: bool) -> Option<&mut [Value]> {
        if self.metatable().is_some() {
            return None;
        }
        let values = self.array.get_mut(..len)?;
        values
            .iter()
            .all(|value| match value {
                Value::Number(_) => true,
                Value::Boolean(_) | Value::Nil => !numbers,
                _ => false,
            })
            .then_some(values)
    }
    pub(crate) fn sort_finite_numbers(&mut self, len: usize) -> bool {
        let Some(values) = self.array.get_mut(..len) else {
            return false;
        };
        if !values.iter().all(|value| {
            matches!(value, Value::Number(n)
            if n.is_finite() && (*n != 0.0 || !n.is_sign_negative()))
        }) {
            return false;
        }
        values.sort_unstable_by(|left, right| {
            let (Value::Number(a), Value::Number(b)) = (*left, *right) else {
                unreachable!("validated numeric array")
            };
            a.total_cmp(&b)
        });
        true
    }
    pub fn get(&self, key: Value) -> Value {
        if let Some(i) = array_index(key)
            && let Some(value) = self.array.get(i)
        {
            return *value;
        }
        let Some(key) = Key::from_value(key) else {
            return Value::Nil;
        };
        self.get_key(key)
    }
    /// String keys are already interned; field access need not classify them
    /// as a possible array index or reconstruct a generic Lua table key.
    #[inline]
    pub(crate) fn get_string(&self, key: StringId) -> Value {
        self.get_key(Key::String(key))
    }
    #[inline]
    fn get_key(&self, key: Key) -> Value {
        if self.slots.is_empty() {
            return Value::Nil;
        }
        let mask = self.slots.len() - 1;
        let mut slot = key.hash() & mask;
        loop {
            let index = self.slots[slot] as usize;
            if index == 0 {
                return Value::Nil;
            }
            let entry = &self.entries[index - 1];
            if entry.key == key {
                return entry.value;
            }
            slot = (slot + 1) & mask;
        }
    }
    fn checked_key(key: Value) -> Result<Key, Error> {
        Key::from_value(key).ok_or_else(|| {
            Error::new(
                ErrorKind::Runtime,
                0,
                if key == Value::Nil {
                    "table index is nil"
                } else {
                    "table index is NaN"
                },
            )
        })
    }
    pub fn validate_set_key(key: Value) -> Result<(), Error> {
        Self::checked_key(key).map(|_| ())
    }
    /// Overwrite an existing array slot when neither the key nor value needs
    /// tracing. This cannot grow storage or change table traversal positions.
    #[inline]
    pub(crate) fn set_existing_array_scalar(&mut self, key: Value, value: Value) -> bool {
        if !matches!(
            value,
            Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
        ) {
            return false;
        }
        if let Some(index) = array_index(key)
            && let Some(slot) = self.array.get_mut(index)
        {
            *slot = value;
            true
        } else {
            false
        }
    }
    /// A live string-keyed hash entry already owns its key. Replacing its
    /// scalar value cannot allocate, add a GC edge or change traversal order.
    #[inline]
    pub(crate) fn set_existing_string_scalar(&mut self, key: StringId, value: Value) -> bool {
        if !matches!(
            value,
            Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
        ) {
            return false;
        }
        let hints = self.next_hash_hint.load(Ordering::Relaxed);
        for hint in [hints as u32, (hints >> 32) as u32] {
            if let Some(entry) = hint
                .checked_sub(1)
                .and_then(|index| self.entries.get_mut(index as usize))
                && entry.key == Key::String(key)
                && entry.value != Value::Nil
            {
                entry.value = value;
                return true;
            }
        }
        if let Some((_, Some(index))) = self.slot(Key::String(key))
            && self.entries[index].value != Value::Nil
        {
            self.entries[index].value = value;
            self.remember_hash_entry((index + 1) as u32);
            return true;
        }
        false
    }
    #[inline]
    fn remember_hash_entry(&self, index: u32) {
        let previous = self.next_hash_hint.load(Ordering::Relaxed) as u32;
        self.next_hash_hint.store(
            (u64::from(previous) << 32) | u64::from(index),
            Ordering::Relaxed,
        );
    }
    pub fn set(&mut self, key: Value, value: Value) -> Result<(), Error> {
        let k = Self::checked_key(key)?;
        if let Some(i) = array_index(key) {
            if i < self.array.len() {
                self.array[i] = value;
                return Ok(());
            }
            if i == self.array.len() && value != Value::Nil {
                // Include every consecutive hash value that this insertion
                // will migrate. Reserve before changing any value or trace.
                let mut end = self.array.len() + 1;
                while self
                    .slot(Key::Number(((end + 1) as f64).to_bits()))
                    .and_then(|(_, i)| i)
                    .is_some_and(|i| self.entries[i].value != Value::Nil)
                {
                    end += 1;
                }
                self.array
                    .try_reserve(end - self.array.len())
                    .map_err(|_| Error::memory())?;
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
        let vacant = match self.slot(k) {
            Some((_, Some(i))) => {
                self.entries[i].value = value;
                if matches!(k, Key::String(_)) {
                    self.remember_hash_entry((i + 1) as u32);
                }
                return Ok(());
            }
            Some((slot, None)) => Some(slot),
            None => None,
        };
        if value == Value::Nil {
            return Ok(());
        }
        let rebuild = (self.entries.len() + 1) * 4 >= self.slots.len() * 3;
        let live = if rebuild {
            self.entries
                .iter()
                .filter(|e| e.value != Value::Nil)
                .count()
        } else {
            self.entries.len()
        };
        let index = u32::try_from(live + 1)
            .map_err(|_| Error::new(ErrorKind::Runtime, 0, "table overflow"))?;
        let slots = if rebuild && (live + 1) * 4 >= self.slots.len() * 3 {
            self.slots
                .len()
                .checked_mul(2)
                .ok_or_else(Error::memory)?
                .max(8)
        } else {
            self.slots.len()
        };
        self.entries
            .try_reserve((live + 1).saturating_sub(self.entries.len()))
            .map_err(|_| Error::memory())?;
        self.slots
            .try_reserve(slots.saturating_sub(self.slots.len()))
            .map_err(|_| Error::memory())?;
        if rebuild {
            self.rebuild(slots);
        }
        let slot = if rebuild {
            self.slot(k).expect("table rebuild creates slots").0
        } else {
            vacant.expect("nonempty table has a vacant hash slot")
        };
        self.slots[slot] = index;
        self.entries.push(Entry { key: k, value });
        if matches!(k, Key::String(_)) {
            self.remember_hash_entry(index);
        }
        Ok(())
    }
    #[inline]
    pub(crate) fn set_grown_list(&mut self, index: usize, value: Value) {
        self.array[index] = value;
    }
    pub fn grow_list_array(&mut self, end: usize) -> Result<(), Error> {
        let first = self.array.len();
        if end <= first {
            return Ok(());
        }
        // SETLIST includes nil fields in its array extent. Reserve before
        // changing traversal positions or moving any existing hash values.
        self.array
            .try_reserve(end - first)
            .map_err(|_| Error::memory())?;
        self.invalidate_trace();
        self.array.resize(end, Value::Nil);
        if !self.entries.is_empty() {
            for index in first..end {
                if let Some(value) = self.take_hash(Key::Number(((index + 1) as f64).to_bits())) {
                    self.array[index] = value;
                }
            }
        }
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
        self.next_entry(key)
            .map_err(|_| Error::new(ErrorKind::Runtime, 0, "invalid key to 'next'"))
    }
    pub(crate) fn next_entry(&self, key: Value) -> Result<Option<(Value, Value)>, InvalidNextKey> {
        let (array, hash) = if key == Value::Nil {
            (0, 0)
        } else if let Some(i) = array_index(key).filter(|&i| i < self.array.len()) {
            (i + 1, 0)
        } else {
            let key = Key::from_value(key).ok_or(InvalidNextKey)?;
            let hints = self.next_hash_hint.load(Ordering::Relaxed);
            let first = hints as u32 as usize;
            let second = (hints >> 32) as u32 as usize;
            let index = if first != 0
                && self
                    .entries
                    .get(first - 1)
                    .is_some_and(|entry| entry.key == key)
            {
                first - 1
            } else if second != 0
                && self
                    .entries
                    .get(second - 1)
                    .is_some_and(|entry| entry.key == key)
            {
                second - 1
            } else {
                self.slot(key)
                    .and_then(|(_, index)| index)
                    .ok_or(InvalidNextKey)?
            };
            (self.array.len(), index + 1)
        };
        for (i, &value) in self.array.iter().enumerate().skip(array) {
            if value != Value::Nil {
                return Ok(Some((Value::Number((i + 1) as f64), value)));
            }
        }
        for (index, entry) in self.entries.iter().enumerate().skip(hash) {
            if entry.value != Value::Nil {
                // Hash entries are indexed by u32 in slots, so index+1 fits.
                // A sequential next cursor always uses this primary hint;
                // discarding the secondary avoids a read-modify-write per key.
                self.next_hash_hint
                    .store((index + 1) as u64, Ordering::Relaxed);
                return Ok(Some((entry.key.value(), entry.value)));
            }
        }
        Ok(None)
    }
}
#[cfg(test)]
mod allocation_tests {
    use super::*;
    use crate::allocation_probe::probe;
    fn hash_key(n: i32) -> Value {
        Value::Number(-f64::from(n))
    }
    #[test]
    fn key_equality_preserves_each_variant_and_numeric_signed_zero() {
        let pairs = [
            (Key::Boolean(false), Key::Boolean(true)),
            (
                Key::Number(1.0_f64.to_bits()),
                Key::Number(2.0_f64.to_bits()),
            ),
            (Key::String(StringId(1)), Key::String(StringId(2))),
            (Key::Table(TableId(1)), Key::Table(TableId(2))),
            (Key::Function(FunctionId(1)), Key::Function(FunctionId(2))),
            (Key::Native(1), Key::Native(2)),
            (Key::Thread(ThreadId(1)), Key::Thread(ThreadId(2))),
            (Key::Userdata(UserdataId(1)), Key::Userdata(UserdataId(2))),
            (Key::LightUserdata(1), Key::LightUserdata(2)),
            (
                Key::ParserPrototype(ParserPrototypeId(1)),
                Key::ParserPrototype(ParserPrototypeId(2)),
            ),
        ];
        for (left, right) in pairs {
            assert_eq!(left, left);
            assert_eq!(right, right);
            assert_ne!(left, right, "{left:?} vs {right:?}");
        }

        let zero = Key::Number(0.0_f64.to_bits());
        let negative_zero = Key::Number((-0.0_f64).to_bits());
        assert_eq!(zero, negative_zero);
        assert_eq!(zero.hash(), negative_zero.hash());

        let distinct_tags = pairs.map(|(key, _)| key);
        for (i, left) in distinct_tags.iter().enumerate() {
            for right in &distinct_tags[i + 1..] {
                assert_ne!(left, right, "different key variants must not alias");
            }
        }
    }
    #[test]
    fn array_index_accepts_only_exact_positive_u32_keys() {
        for (number, expected) in [
            (f64::NEG_INFINITY, None),
            (-10.0, None),
            (-0.0, None),
            (0.0, None),
            (0.5, None),
            (1.0 - f64::EPSILON, None),
            (1.0, Some(0)),
            (1.0 + f64::EPSILON, None),
            (1.5, None),
            (2.0, Some(1)),
            (2.5, None),
            (f64::from(u32::MAX - 1), Some(u32::MAX as usize - 2)),
            (f64::from(u32::MAX), Some(u32::MAX as usize - 1)),
            (f64::from(u32::MAX) + 0.5, None),
            (f64::INFINITY, None),
            (f64::NAN, None),
        ] {
            assert_eq!(
                array_index(Value::Number(number)),
                expected,
                "key {number:?}"
            );
        }
        assert_eq!(array_index(Value::Nil), None);
        assert_eq!(array_index(Value::Boolean(true)), None);
    }
    #[test]
    fn numeric_hash_keys_and_array_keys_preserve_next_order() {
        let mut table = Table::with_capacity(3, 8).unwrap();
        let keys = [
            1.0,
            2.0,
            3.0,
            -0.0,
            -3.0,
            0.5,
            f64::from(u32::MAX),
            f64::INFINITY,
        ];
        for (i, &key) in keys.iter().enumerate() {
            table
                .set(Value::Number(key), Value::Number(i as f64))
                .unwrap();
        }
        assert_eq!(table.get(Value::Number(0.0)), Value::Number(3.0));
        table.set(Value::Number(0.0), Value::Number(30.0)).unwrap();
        assert_eq!(table.get(Value::Number(-0.0)), Value::Number(30.0));
        let mut seen = Vec::new();
        let mut current = Value::Nil;
        while let Some((key, value)) = table.next(current).unwrap() {
            seen.push((key, value));
            current = key;
        }
        let expected: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(i, &key)| {
                (
                    Value::Number(key),
                    Value::Number(if i == 3 { 30.0 } else { i as f64 }),
                )
            })
            .collect();
        assert_eq!(seen, expected);
        assert_eq!(table.get(Value::Number(f64::NAN)), Value::Nil);
        assert_eq!(
            table
                .set(Value::Number(f64::NAN), Value::Nil)
                .unwrap_err()
                .kind(),
            ErrorKind::Runtime
        );
        assert!(table.next(Value::Number(f64::NAN)).is_err());
    }
    #[test]
    fn next_hint_preserves_mixed_key_order_and_interleaved_cursors() {
        let mut table = Table::with_capacity(2, 8).unwrap();
        table.set(Value::Number(1.), Value::Number(11.)).unwrap();
        table.set(Value::Number(2.), Value::Number(22.)).unwrap();
        let keys = [
            Value::String(StringId((1u64 << 32) | 7)),
            Value::Table(TableId((2u64 << 32) | 3)),
            Value::Number(-0.0),
            Value::Boolean(false),
            Value::Number(-7.5),
        ];
        for (index, &key) in keys.iter().enumerate() {
            table.set(key, Value::Number((index + 30) as f64)).unwrap();
        }

        let expected = table.pairs().collect::<Vec<_>>();
        let mut actual = Vec::new();
        let mut key = Value::Nil;
        while let Some(pair) = table.next_entry(key).unwrap() {
            actual.push(pair);
            key = pair.0;
        }
        assert_eq!(actual, expected);
        assert_eq!(table.next_hash_hint.load(Ordering::Relaxed) as u32, 5);
        // Another iterator can overwrite the hint; the original cursor still
        // locates its exact key through the ordinary hash lookup.
        assert_eq!(
            table.next(keys[0]).unwrap(),
            Some((keys[1], Value::Number(31.)))
        );
        assert_eq!(
            table.next(Value::Number(0.)).unwrap(),
            Some((keys[3], Value::Number(33.)))
        );
        assert_eq!(
            table.next(Value::Number(2.)).unwrap(),
            Some((keys[0], Value::Number(30.)))
        );
        table
            .next_hash_hint
            .store(u64::from(u32::MAX), Ordering::Relaxed);
        assert_eq!(
            table.next(keys[1]).unwrap(),
            Some((keys[2], Value::Number(32.)))
        );
        assert!(table.next(Value::String(StringId(999))).is_err());
        assert!(table.next(Value::Table(TableId((3u64 << 32) | 3))).is_err());
        assert!(table.next(Value::Number(f64::NAN)).is_err());

        let (visited, stats) = probe(1, || {
            let mut current = Value::Nil;
            let mut visited = 0;
            while let Some((next, _)) = table.next_entry(current).unwrap() {
                visited += 1;
                current = next;
            }
            visited
        });
        assert_eq!(visited, expected.len());
        assert_eq!(stats.attempts, 0);
    }
    #[test]
    fn recent_hash_hint_shortcuts_only_exact_live_string_key_overwrites() {
        let a = StringId((1_u64 << 32) | 11);
        let b = StringId((1_u64 << 32) | 12);
        let mut table = Table::with_capacity(0, 4).unwrap();
        assert_eq!(table.next_hash_hint.load(Ordering::Relaxed), 0);
        assert!(!table.set_existing_string_scalar(a, Value::Number(1.)));
        table.set(Value::String(a), Value::Number(2.)).unwrap();
        assert_eq!(table.next_hash_hint.load(Ordering::Relaxed), 1);
        let (updated, stats) = probe(1, || table.set_existing_string_scalar(a, Value::Number(3.)));
        assert!(updated);
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.get(Value::String(a)), Value::Number(3.));

        table.set(Value::String(b), Value::Number(4.)).unwrap();
        assert!(table.set_existing_string_scalar(a, Value::Number(5.)));
        assert!(table.set_existing_string_scalar(b, Value::Number(4.)));
        table.set(Value::String(a), Value::Number(5.)).unwrap();
        assert!(table.set_existing_string_scalar(a, Value::Number(6.)));
        table.set(Value::String(a), Value::Nil).unwrap();
        assert!(!table.set_existing_string_scalar(a, Value::Number(7.)));
        table.set(Value::String(a), Value::Number(8.)).unwrap();
        table
            .next_hash_hint
            .store(u64::from(u32::MAX), Ordering::Relaxed);
        assert!(table.set_existing_string_scalar(a, Value::Number(9.)));
        table.set(Value::String(a), Value::Number(9.)).unwrap();
        assert!(table.set_existing_string_scalar(a, Value::Number(10.)));
        assert_eq!(table.get(Value::String(a)), Value::Number(10.));
        assert_eq!(
            table.next(Value::String(a)).unwrap(),
            Some((Value::String(b), Value::Number(4.)))
        );
    }
    #[test]
    fn scalar_overwrites_find_four_rotating_string_keys_without_allocating() {
        let keys = [11, 12, 13, 14].map(|slot| StringId((1_u64 << 32) | slot));
        let mut table = Table::with_capacity(0, keys.len()).unwrap();
        for &key in &keys {
            table.set(Value::String(key), Value::Number(0.)).unwrap();
        }
        let original_bytes = table.heap_bytes();
        let ((), stats) = probe(1, || {
            for i in 1..=64 {
                for &key in &keys {
                    assert!(table.set_existing_string_scalar(key, Value::Number(i as f64)));
                }
            }
        });
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.heap_bytes(), original_bytes);
        assert_eq!(
            table.pairs().collect::<Vec<_>>(),
            keys.map(|key| (Value::String(key), Value::Number(64.)))
        );
        table.set(Value::String(keys[0]), Value::Nil).unwrap();
        assert!(!table.set_existing_string_scalar(keys[0], Value::Number(65.)));
        assert_eq!(table.get(Value::String(keys[0])), Value::Nil);
    }
    #[test]
    fn next_hint_allows_deleted_controls_until_tombstones_are_compacted() {
        let mut table = Table::with_capacity(0, 5).unwrap();
        for n in 1..=5 {
            table.set(hash_key(n), Value::Number(n as f64)).unwrap();
        }
        assert_eq!(table.next(Value::Nil).unwrap().unwrap().0, hash_key(1));
        assert_eq!(table.next(hash_key(1)).unwrap().unwrap().0, hash_key(2));
        table.set(hash_key(2), Value::Nil).unwrap();
        assert_eq!(
            table.next(hash_key(2)).unwrap(),
            Some((hash_key(3), Value::Number(3.)))
        );
        table.set(hash_key(1), Value::Nil).unwrap();
        assert_eq!(
            table.next(hash_key(1)).unwrap(),
            Some((hash_key(3), Value::Number(3.)))
        );

        // The next insertion compacts tombstones. A stale physical index must
        // not make a removed key valid after it leaves the hash slots.
        table.set(hash_key(6), Value::Number(6.)).unwrap();
        assert!(table.next(hash_key(1)).is_err());
        assert!(table.next(hash_key(2)).is_err());
        assert_eq!(
            table.next(hash_key(3)).unwrap(),
            Some((hash_key(4), Value::Number(4.)))
        );
        let expected = table.pairs().collect::<Vec<_>>();
        let mut current = Value::Nil;
        for &pair in &expected {
            assert_eq!(table.next(current).unwrap(), Some(pair));
            current = pair.0;
        }
        assert_eq!(table.next(current).unwrap(), None);
    }
    fn check_hash(table: &Table, count: i32) {
        for n in 1..=count {
            assert_eq!(table.get(hash_key(n)), Value::Number(f64::from(n)));
        }
    }
    #[test]
    fn hash_reservations_preserve_values_and_pending_traversal_on_failure() {
        for initialized in [false, true] {
            let mut points = 0;
            for nth in 0..=2 {
                let mut table = if initialized {
                    Table::with_capacity(0, 5).unwrap()
                } else {
                    Table::default()
                };
                let count = if initialized { 5 } else { 0 };
                for n in 1..=count {
                    table.set(hash_key(n), Value::Number(f64::from(n))).unwrap();
                }
                table.trace = TableTrace::Pending(false, false);
                let pairs = table.pairs().collect::<Vec<_>>();
                let (result, stats) = probe(nth, || table.set(hash_key(6), Value::Number(42.)));
                if nth == 0 {
                    points = stats.attempts;
                    assert_eq!(points, 2);
                    result.unwrap();
                } else {
                    assert!(nth <= points);
                    assert_eq!(stats.rejected, 1);
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(table.get(hash_key(6)), Value::Nil);
                    assert_eq!(table.pairs().collect::<Vec<_>>(), pairs);
                    assert_eq!(table.trace, TableTrace::Pending(false, false));
                    table.set(hash_key(6), Value::Number(42.)).unwrap();
                }
                check_hash(&table, count);
                assert_eq!(table.get(hash_key(6)), Value::Number(42.));
            }
            eprintln!("TABLE_MUTATION_FAILURE_POINTS {points}");
        }
    }
    #[test]
    fn a_failed_list_array_growth_keeps_nil_holes_and_existing_hash_values() {
        let mut table = Table::with_capacity(1, 3).unwrap();
        for index in [1, 3, 5, 9] {
            table
                .set(Value::Number(index as f64), Value::Number(index as f64))
                .unwrap();
        }
        table.trace = TableTrace::Pending(false, false);
        let before = table.pairs().collect::<Vec<_>>();
        let (result, stats) = probe(1, || table.grow_list_array(6));
        assert_eq!(stats.rejected, 1);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
        assert_eq!(table.array.len(), 1);
        assert_eq!(table.pairs().collect::<Vec<_>>(), before);
        assert_eq!(table.trace, TableTrace::Pending(false, false));
        table.grow_list_array(6).unwrap();
        for index in [1, 3, 5, 9] {
            assert_eq!(
                table.get(Value::Number(index as f64)),
                Value::Number(index as f64)
            );
        }
        for index in [2, 4, 6] {
            assert_eq!(table.get(Value::Number(index as f64)), Value::Nil);
        }
    }
    #[test]
    fn a_failed_array_migration_keeps_every_hash_entry_and_the_array_length() {
        for nth in 0..=1 {
            let mut table = Table::with_capacity(4, 0).unwrap();
            for n in (1..=4).chain(6..=20) {
                table
                    .set(Value::Number(f64::from(n)), Value::Number(f64::from(n)))
                    .unwrap();
            }
            table.trace = TableTrace::Pending(false, false);
            let pairs = table.pairs().collect::<Vec<_>>();
            let (result, stats) = probe(nth, || table.set(Value::Number(5.), Value::Number(5.)));
            if nth == 0 {
                result.unwrap();
                assert_eq!(stats.attempts, 1);
            } else {
                assert_eq!(stats.rejected, 1);
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(table.array.len(), 4);
                assert_eq!(table.get(Value::Number(5.)), Value::Nil);
                assert_eq!(table.pairs().collect::<Vec<_>>(), pairs);
                assert_eq!(table.trace, TableTrace::Pending(false, false));
                table.set(Value::Number(5.), Value::Number(5.)).unwrap();
            }
            assert_eq!(table.array.len(), 20);
            for n in 1..=20 {
                assert_eq!(
                    table.get(Value::Number(f64::from(n))),
                    Value::Number(f64::from(n))
                );
            }
        }
        eprintln!("TABLE_MUTATION_FAILURE_POINTS 1");
    }
    #[test]
    fn tombstone_compaction_reuses_storage_when_the_allocator_rejects_every_request() {
        let mut table = Table::with_capacity(0, 5).unwrap();
        for n in 1..=5 {
            table.set(hash_key(n), Value::Number(f64::from(n))).unwrap();
        }
        table.set(hash_key(3), Value::Nil).unwrap();
        assert!(table.next(hash_key(3)).is_ok());
        table.trace = TableTrace::Pending(false, false);
        let (result, stats) = probe(1, || table.set(hash_key(6), Value::Number(42.)));
        result.unwrap();
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.trace, TableTrace::Restart);
        assert!(table.next(hash_key(3)).is_err());
        assert_eq!(table.get(hash_key(6)), Value::Number(42.));
        for n in [1, 2, 4, 5] {
            assert_eq!(table.get(hash_key(n)), Value::Number(f64::from(n)));
        }
    }
    #[test]
    fn updates_and_deletions_need_no_allocation() {
        let mut table = Table::default();
        table.set(Value::Number(1.), Value::Number(17.)).unwrap();
        table.set(hash_key(2), Value::Number(25.)).unwrap();
        let (result, stats) = probe(1, || -> Result<(), Error> {
            table.set(Value::Number(1.), Value::Number(42.))?;
            table.set(hash_key(2), Value::Number(42.))?;
            table.set(hash_key(2), Value::Nil)?;
            table.set(hash_key(3), Value::Nil)
        });
        result.unwrap();
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.get(Value::Number(1.)), Value::Number(42.));
        assert_eq!(table.get(hash_key(2)), Value::Nil);
    }
}
