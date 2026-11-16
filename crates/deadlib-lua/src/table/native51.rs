//! Lua 5.1 node placement for hosts whose scripts depend on `pairs` order.
//!
//! Follows ITGmania's extern/lua-5.1/src/{ltable,lstring}.c: Brent collision
//! chains, descending free slots/reinsertion, and density-based array resizing.
//! The ordinary identity index still serves reads; it never determines order.
use super::*;

const EMPTY: Entry = Entry {
    key: Key::Vacant,
    value: Value::Nil,
    native_hash: 0,
    next: 0,
};
const MAX_ARRAY: usize = 1 << 26;

pub(crate) fn string_hash(bytes: &[u8]) -> u32 {
    let mut hash = bytes.len() as u32;
    let step = (bytes.len() >> 5) + 1;
    let mut index = bytes.len();
    while index >= step {
        hash ^= (hash << 5)
            .wrapping_add(hash >> 2)
            .wrapping_add(u32::from(bytes[index - 1]));
        index -= step;
    }
    hash
}

fn hash(key: Key, string_hash: Option<u32>) -> u32 {
    match key {
        Key::String(_) => string_hash.expect("new Lua 5.1 string key needs its byte hash"),
        Key::Number(bits) if bits << 1 == 0 => 0,
        Key::Number(bits) => (bits as u32).wrapping_add((bits >> 32) as u32),
        Key::Boolean(value) => u32::from(value),
        // Native pointer-key order depends on allocation addresses and cannot
        // be portable. Stable VM identities provide the corresponding hash.
        _ => key.identity().0 as u32,
    }
}

fn main_position(key: Key, hash: u32, count: usize) -> usize {
    match key {
        Key::String(_) | Key::Boolean(_) => hash as usize & (count - 1),
        _ => hash as usize % ((count - 1) | 1),
    }
}

impl Table {
    /// Lua 5.1's removeentry keeps a deleted collectable key only as a next()
    /// cursor. Even an otherwise-live string must no longer match a new write.
    pub(crate) fn retire_native_key(&mut self, key: Value) {
        if !self.native51 { return; }
        let Some(key) = Key::from_value(key) else { return; };
        let Some(dead) = key.dead() else { return; };
        let Some((_, Some(index))) = self.slot(key) else { return; };
        if self.entries[index].value != Value::Nil { return; }
        self.native_index_remove(key);
        self.entries[index].key = dead;
        self.touch_fields();
    }

    pub(crate) fn retire_native_keys_in_trace_slice(&mut self, cursor: usize, budget: usize) {
        if !self.native51 { return; }
        let start = cursor >> 1;
        let (start, end) = if cursor & 1 == 0 {
            let array_end = start.saturating_add(budget).min(self.array.len());
            let hash_end = if array_end == self.array.len() {
                budget.saturating_sub(array_end - start).min(self.entries.len())
            } else { 0 };
            (0, hash_end)
        } else {
            (start, start.saturating_add(budget).min(self.entries.len()))
        };
        for index in start..end {
            if self.entries[index].value == Value::Nil {
                self.retire_native_key(self.entries[index].key.value());
            }
        }
    }

    pub(crate) fn with_native51_capacity(
        array: usize,
        records: usize,
        memory: Memory,
    ) -> Result<Self, Error> {
        let mut table = Self::new_in(memory);
        table.native51 = true;
        table.native_resize(array, records)?;
        Ok(table)
    }

    pub(crate) fn native_string_hash(bytes: &[u8]) -> u32 {
        string_hash(bytes)
    }

    fn native_index_set(&mut self, key: Key, index: usize) {
        let slot = self.slot(key).expect("allocated identity index").0;
        self.slots[slot] = (index as u32 + 1) | Self::slot_tag(self.slots.len(), key.hash()).0;
    }

    fn native_index_remove(&mut self, key: Key) {
        let Some((mut hole, Some(_))) = self.slot(key) else {
            return;
        };
        let mask = self.slots.len() - 1;
        let index_mask = Self::slot_tag(self.slots.len(), 0).1;
        let mut next = (hole + 1) & mask;
        while self.slots[next] != 0 {
            let index = (self.slots[next] & index_mask) as usize - 1;
            let home = self.entries[index].key.hash() & mask;
            if (next.wrapping_sub(home) & mask) >= (hole.wrapping_sub(home) & mask) {
                self.slots[hole] = self.slots[next];
                hole = next;
            }
            next = (next + 1) & mask;
        }
        self.slots[hole] = 0;
    }

    pub(super) fn native_set(
        &mut self,
        key: Key,
        value: Value,
        string_hash: Option<u32>,
    ) -> Result<(), Error> {
        if let Some(index) = array_index(key.value()).filter(|&i| i < self.array.len()) {
            self.array[index] = value;
            return Ok(());
        }
        if let Some((_, Some(index))) = self.slot(key) {
            self.entries[index].value = value;
            self.touch_fields();
            if matches!(key, Key::String(_)) {
                self.remember_hash_entry(index as u32 + 1);
            }
            return Ok(());
        }
        let hash = hash(key, string_hash);
        if self.entries.is_empty() {
            self.native_rehash(key)?;
            return self.native_set(key, value, string_hash);
        }
        let mut position = main_position(key, hash, self.entries.len());
        if self.entries[position].value != Value::Nil {
            let free = (0..self.last_free)
                .rev()
                .find(|&i| self.entries[i].key == Key::Vacant);
            let Some(free) = free else {
                self.native_rehash(key)?;
                return self.native_set(key, value, string_hash);
            };
            self.last_free = free;
            let old = self.entries[position];
            let other = main_position(old.key, old.native_hash, self.entries.len());
            if other != position {
                let mut previous = other;
                while self.entries[previous].next != position as u32 + 1 {
                    previous = self.entries[previous].next as usize - 1;
                }
                self.entries[previous].next = free as u32 + 1;
                self.entries[free] = old;
                self.native_index_set(old.key, free);
                self.entries[position] = EMPTY;
            } else {
                self.entries[free].next = old.next;
                self.entries[position].next = free as u32 + 1;
                position = free;
            }
        } else if self.entries[position].key != Key::Vacant {
            // A deleted main node is reusable, but its collision chain remains.
            self.native_index_remove(self.entries[position].key);
        }
        self.entries[position].key = key;
        self.entries[position].value = value;
        self.entries[position].native_hash = hash;
        self.native_index_set(key, position);
        self.touch_fields();
        self.invalidate_trace();
        if matches!(key, Key::String(_)) {
            self.remember_hash_entry(position as u32 + 1);
        }
        Ok(())
    }

    fn native_rehash(&mut self, extra: Key) -> Result<(), Error> {
        let mut counts = [0usize; 27];
        let mut total = 1;
        let mut count = |key: Key| {
            if let Some(index) = array_index(key.value())
                .map(|i| i + 1)
                .filter(|&i| i <= MAX_ARRAY)
            {
                let bucket = if index == 1 {
                    0
                } else {
                    usize::BITS - (index - 1).leading_zeros()
                };
                counts[bucket as usize] += 1;
            }
        };
        for (index, value) in self.array.iter().enumerate() {
            if *value != Value::Nil {
                total += 1;
                count(Key::Number(((index + 1) as f64).to_bits()));
            }
        }
        for entry in &*self.entries {
            if entry.value != Value::Nil {
                total += 1;
                count(entry.key);
            }
        }
        count(extra);
        let (mut used, mut array, mut array_used) = (0, 0, 0);
        for (power, count) in counts.into_iter().enumerate() {
            used += count;
            if used > (1usize << power) / 2 {
                array = 1usize << power;
                array_used = used;
            }
        }
        self.native_resize(array, total - array_used)
    }

    pub(super) fn native_resize(&mut self, array: usize, records: usize) -> Result<(), Error> {
        if array > MAX_ARRAY || records > MAX_ARRAY {
            return Err(Error::new(ErrorKind::Runtime, 0, "table overflow"));
        }
        let records = if records == 0 {
            0
        } else {
            records.next_power_of_two()
        };
        // Build the replacement completely before changing live values or next
        // cursors. Failed allocations leave the old table and GC walk intact.
        let mut replacement = Self::new_in(self.array.memory().clone());
        replacement.native51 = true;
        replacement
            .array
            .try_reserve_exact(array)
            .map_err(|_| Error::memory())?;
        replacement.array.resize(array, Value::Nil);
        replacement
            .entries
            .try_reserve_exact(records)
            .map_err(|_| Error::memory())?;
        replacement.entries.resize(records, EMPTY);
        let slots = if records == 0 {
            0
        } else {
            (records * 2).max(8)
        };
        replacement
            .slots
            .try_reserve_exact(slots)
            .map_err(|_| Error::memory())?;
        replacement.slots.resize(slots, 0);
        replacement.last_free = records;
        let shared = array.min(self.array.len());
        replacement.array[..shared].copy_from_slice(&self.array[..shared]);
        for index in array..self.array.len() {
            if self.array[index] != Value::Nil {
                replacement.native_set(
                    Key::Number(((index + 1) as f64).to_bits()),
                    self.array[index],
                    None,
                )?;
            }
        }
        for entry in self.entries.iter().rev().filter(|e| e.value != Value::Nil) {
            replacement.native_set(entry.key, entry.value, Some(entry.native_hash))?;
        }
        self.array = replacement.array;
        self.entries = replacement.entries;
        self.slots = replacement.slots;
        self.last_free = replacement.last_free;
        self.next_hash_hint.store(0, Ordering::Relaxed);
        self.invalidate_trace();
        self.touch_fields();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation_probe::probe;

    #[test]
    fn native_resize_failures_preserve_values_order_and_gc_cursor() {
        for nth in 0..=3 {
            let mut table = Table::with_native51_capacity(2, 8, Memory::default()).unwrap();
            for i in [1, 2, 5, 7, 11] {
                table
                    .set(Value::Number(i as f64), Value::Number(i as f64))
                    .unwrap();
            }
            let before = table.pairs().collect::<Vec<_>>();
            table.trace = TableTrace::Pending(false, false);
            let (result, stats) = probe(nth, || table.native_resize(8, 16));
            if nth == 0 {
                assert_eq!(stats.attempts, 3);
                result.unwrap();
            } else {
                assert_eq!(stats.rejected, 1);
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(table.pairs().collect::<Vec<_>>(), before);
                assert_eq!(table.trace, TableTrace::Pending(false, false));
                table.native_resize(8, 16).unwrap();
            }
            for (key, value) in before {
                assert_eq!(table.get(key), value);
            }
            let expected = table.pairs().collect::<Vec<_>>();
            let mut cursor = Value::Nil;
            for pair in expected {
                assert_eq!(table.next(cursor).unwrap(), Some(pair));
                cursor = pair.0;
            }
            assert_eq!(table.next(cursor).unwrap(), None);
        }
    }
}
