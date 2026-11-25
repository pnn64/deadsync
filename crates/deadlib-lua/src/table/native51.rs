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

fn hash(key: Key, string_hash: impl FnOnce(StringId) -> u32) -> u32 {
    match key {
        Key::String(id) => string_hash(id),
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
        if self.native51
            && let Some(key) = Key::from_value(key)
        {
            self.retire_indexed_native_key(key);
        }
    }

    fn retire_indexed_native_key(&mut self, key: Key) {
        if let Some(dead) = key.dead()
            && let Some((slot, Some(index))) = self.slot(key)
            && self.entries[index].value == Value::Nil
        {
            self.native_index_remove_at(slot);
            self.entries[index].key = dead;
            self.native_dead_keys -= 1;
            self.touch_fields();
        }
    }

    pub(crate) fn retire_native_keys_in_trace_slice(&mut self, cursor: usize, budget: usize) {
        if self.native_dead_keys == 0 {
            return;
        }
        let start = cursor >> 1;
        let (start, end) = if cursor & 1 == 0 {
            let array_end = start.saturating_add(budget).min(self.array.len());
            let hash_end = if array_end == self.array.len() {
                budget
                    .saturating_sub(array_end - start)
                    .min(self.entries.len())
            } else {
                0
            };
            (0, hash_end)
        } else {
            (start, start.saturating_add(budget).min(self.entries.len()))
        };
        for index in start..end {
            if self.entries[index].value == Value::Nil {
                self.retire_indexed_native_key(self.entries[index].key);
                if self.native_dead_keys == 0 {
                    break;
                }
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
        let hash = key.hash();
        let slot = self
            .slot_hashed(key, hash)
            .expect("allocated identity index")
            .0;
        self.slots[slot] = (index as u32 + 1) | Self::slot_tag(self.slots.len(), hash).0;
    }

    fn native_index_remove_at(&mut self, mut hole: usize) {
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
        string_hash: impl FnOnce(StringId) -> u32,
    ) -> Result<(), Error> {
        if let Some(index) = array_index(key.value()).filter(|&i| i < self.array.len()) {
            self.array[index] = value;
            return Ok(());
        }
        let identity_hash = key.hash();
        let vacant = match self.slot_hashed(key, identity_hash) {
            Some((_, Some(index))) => {
                if value == Value::Nil {
                    if self.entries[index].value != Value::Nil && key.dead().is_some() {
                        self.native_dead_keys += 1;
                    }
                } else if self.native_dead_keys != 0
                    && self.entries[index].value == Value::Nil
                    && key.dead().is_some()
                {
                    self.native_dead_keys -= 1;
                }
                self.entries[index].value = value;
                self.touch_fields();
                if matches!(key, Key::String(_)) {
                    self.remember_hash_entry(index as u32 + 1);
                }
                return Ok(());
            }
            Some((slot, None)) => Some(slot),
            None => None,
        };
        self.native_insert(key, value, hash(key, string_hash), identity_hash, vacant)
    }

    fn native_insert(
        &mut self,
        key: Key,
        value: Value,
        hash: u32,
        identity_hash: usize,
        vacant: Option<usize>,
    ) -> Result<(), Error> {
        let Some(mut vacant) = vacant else {
            self.native_rehash(key)?;
            return self.native_set(key, value, |_| hash);
        };
        let mut position = main_position(key, hash, self.entries.len());
        if self.entries[position].value != Value::Nil {
            let free = (0..self.last_free as usize)
                .rev()
                .find(|&i| self.entries[i].key == Key::Vacant);
            let Some(free) = free else {
                self.native_rehash(key)?;
                return self.native_set(key, value, |_| hash);
            };
            self.last_free = free as u32;
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
        } else if self.entries[position].key != Key::Vacant
            && let Some((slot, Some(_))) = self.slot(self.entries[position].key)
        {
            // A deleted main node is reusable, but its collision chain remains.
            if self.entries[position].key.dead().is_some() {
                self.native_dead_keys -= 1;
            }
            self.native_index_remove_at(slot);
            // Deletion can shift the identity probe chain before insertion.
            vacant = self
                .slot_hashed(key, identity_hash)
                .expect("allocated identity index")
                .0;
        }
        self.entries[position].key = key;
        self.entries[position].value = value;
        if value == Value::Nil && key.dead().is_some() {
            self.native_dead_keys += 1;
        }
        self.entries[position].native_hash = hash;
        self.slots[vacant] =
            (position as u32 + 1) | Self::slot_tag(self.slots.len(), identity_hash).0;
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
        // Each bucket covers the array slots added by the corresponding power of two.
        let mut start = 0;
        for (power, count) in counts.iter_mut().enumerate() {
            if start == self.array.len() {
                break;
            }
            let end = self.array.len().min(1 << power);
            *count = self.array[start..end]
                .iter()
                .filter(|&&value| value != Value::Nil)
                .count();
            total += *count;
            start = end;
        }
        let mut count = |key: Key| {
            if let Some(index) = array_index(key.value()).filter(|&i| i < MAX_ARRAY) {
                let bucket = usize::BITS - index.leading_zeros();
                counts[bucket as usize] += 1;
            }
        };
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
        let shared = array.min(self.array.len());
        replacement.array.extend_from_slice(&self.array[..shared]);
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
        replacement.last_free = records as u32;
        for index in array..self.array.len() {
            if self.array[index] != Value::Nil {
                replacement.native_set(
                    Key::Number(((index + 1) as f64).to_bits()),
                    self.array[index],
                    |_| unreachable!("numeric keys do not use a string hash"),
                )?;
            }
        }
        for entry in self.entries.iter().rev().filter(|e| e.value != Value::Nil) {
            replacement.native_set(entry.key, entry.value, |_| entry.native_hash)?;
        }
        self.array = replacement.array;
        self.entries = replacement.entries;
        self.slots = replacement.slots;
        self.last_free = replacement.last_free;
        self.native_dead_keys = 0;
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
    fn native_retirement_count_tracks_revivals_and_large_deletions() {
        let mut table = Table::with_native51_capacity(0, 512, Memory::default()).unwrap();
        let key = |index| Key::String(StringId(index));
        let hash = |id: StringId| id.0 as u32;
        table.native_set(key(1), Value::Nil, hash).unwrap();
        table.native_set(key(1), Value::Nil, hash).unwrap();
        assert_eq!(table.native_dead_keys, 1);
        table.native_set(key(1), Value::Number(1.0), hash).unwrap();
        assert_eq!(table.native_dead_keys, 0);
        table.native_set(key(1), Value::Nil, hash).unwrap();
        table.retire_native_keys_in_trace_slice(1, 512);
        assert_eq!(table.native_dead_keys, 0);
        assert!(table.slot(key(1)).unwrap().1.is_none());

        for index in 2..=256 {
            table.native_set(key(index), Value::Nil, hash).unwrap();
        }
        assert_eq!(table.native_dead_keys, 255);
        table.retire_native_keys_in_trace_slice(1, 512);
        assert_eq!(table.native_dead_keys, 0);
        assert!((2..=256).all(|index| table.slot(key(index)).unwrap().1.is_none()));
        table.native_set(key(256), Value::Nil, hash).unwrap();
        assert_eq!(table.native_dead_keys, 1);
        table.native_resize(0, 512).unwrap();
        assert_eq!(table.native_dead_keys, 0);
    }
    #[test]
    fn reusing_a_deleted_main_node_keeps_the_new_key_reachable() {
        let mut table = Table::with_native51_capacity(0, 8, Memory::default()).unwrap();
        // These keys collide in both the native node array and the identity index.
        let old = Value::Number(-3.0);
        let new = Value::Number(-39.0);
        let value = Value::Boolean(true);
        table.set(old, value).unwrap();
        table.set(old, Value::Nil).unwrap();
        table.set(new, value).unwrap();
        assert_eq!(table.get(old), Value::Nil);
        assert_eq!(table.get(new), value);
        assert_eq!(table.next(Value::Nil).unwrap(), Some((new, value)));
        assert_eq!(table.next(new).unwrap(), None);
    }

    #[test]
    fn native_length_searches_the_hash_and_falls_back_across_the_array() {
        let mut table = Table::with_native51_capacity(3, 64, Memory::default()).unwrap();
        for index in [1, 2, 3, 4, 8, 16, 24, 25] {
            table
                .set(Value::Number(f64::from(index)), Value::Boolean(true))
                .unwrap();
        }
        assert_eq!(table.array.len(), 3);
        assert_eq!(table.len(), 25);
        for shift in 5..=30 {
            table
                .set(Value::Number(f64::from(1 << shift)), Value::Boolean(true))
                .unwrap();
        }
        assert_eq!(table.array.len(), 3);
        assert_eq!(table.len(), 4);
    }

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
