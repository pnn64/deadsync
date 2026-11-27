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
const MAX_ARRAY: usize = LUA51_MAX_ARRAY;

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
    pub(crate) fn clear_weak_hash_slots(&mut self, cursor: usize, mut mask: u64) {
        let start = if cursor & 1 == 0 { 0 } else { cursor >> 1 };
        while mask != 0 {
            let index = start + mask.trailing_zeros() as usize;
            let entry = &mut self.hash_part_mut().entries[index];
            debug_assert_ne!(entry.value, Value::Nil);
            let key = entry.key;
            entry.value = Value::Nil;
            self.touch_fields();
            if matches!(key, Key::String(_)) {
                self.remember_hash_entry(index as u32 + 1);
            }
            if self.native51
                && let Some(dead) = key.dead()
            {
                let slot = self.native_identity_slot(index);
                self.retire_native_entry(slot, index, dead);
            }
            mask &= mask - 1;
        }
    }

    // Lua 5.1 keeps a deleted collectable key only as a next() cursor.
    fn retire_native_entry(&mut self, slot: usize, index: usize, dead: Key) {
        self.native_index_remove_at(slot);
        self.hash_part_mut().entries[index].key = dead;
        self.touch_fields();
    }

    pub(crate) fn retire_native_keys_in_trace_slice(&mut self, cursor: usize, budget: usize) {
        if self.hash_part().native_dead_keys == 0 {
            return;
        }
        let start = cursor >> 1;
        let (start, end) = if cursor & 1 == 0 {
            let array_end = start.saturating_add(budget).min(self.array.len());
            let hash_end = if array_end == self.array.len() {
                budget
                    .saturating_sub(array_end - start)
                    .min(self.hash_part().entries.len())
            } else {
                0
            };
            (0, hash_end)
        } else {
            (
                start,
                start
                    .saturating_add(budget)
                    .min(self.hash_part().entries.len()),
            )
        };
        for index in start..end {
            let entry = &self.hash_part().entries[index];
            if entry.value == Value::Nil
                && let Some(dead) = entry.key.dead()
            {
                let slot = self.native_identity_slot(index);
                self.retire_native_entry(slot, index, dead);
                self.hash_part_mut().native_dead_keys -= 1;
                if self.hash_part().native_dead_keys == 0 {
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
        if records != 0 {
            let mut table = Self::new_in(memory);
            table.native51 = true;
            table.native_resize(array, records)?;
            return Ok(table);
        }
        if array > MAX_ARRAY {
            return Err(Error::new(ErrorKind::Runtime, 0, "table overflow"));
        }
        let array = if array == 3 && memory.allocator().is_none() {
            ArrayStorage::Triplet {
                memory,
                values: [Value::Nil; 3],
            }
        } else {
            let mut values = Buffer::new_in(memory);
            values.try_reserve_exact(array)?;
            values.resize(array, Value::Nil);
            ArrayStorage::heap(values)
        };
        Ok(Self {
            array,
            native51: true,
            fields_version: 1,
            ..Self::default()
        })
    }

    pub(crate) fn native_string_hash(bytes: &[u8]) -> u32 {
        string_hash(bytes)
    }

    // An indexed entry is uniquely identified by its encoded slot word.
    fn native_identity_slot(&self, index: usize) -> usize {
        let len = self.hash_part().slots.len();
        let mask = len - 1;
        let hash = self.hash_part().entries[index].key.hash();
        let word = (index as u32 + 1) | Self::slot_tag(len, hash).0;
        let mut slot = hash & mask;
        while self.hash_part().slots[slot] != word {
            debug_assert_ne!(
                self.hash_part().slots[slot],
                0,
                "native entry must be indexed"
            );
            slot = (slot + 1) & mask;
        }
        slot
    }

    fn native_index_remove_at(&mut self, mut hole: usize) {
        let mask = self.hash_part().slots.len() - 1;
        let index_mask = Self::slot_tag(self.hash_part().slots.len(), 0).1;
        let mut next = (hole + 1) & mask;
        while self.hash_part().slots[next] != 0 {
            let index = (self.hash_part().slots[next] & index_mask) as usize - 1;
            let home = self.hash_part().entries[index].key.hash() & mask;
            if (next.wrapping_sub(home) & mask) >= (hole.wrapping_sub(home) & mask) {
                self.hash_part_mut().slots[hole] = self.hash_part().slots[next];
                hole = next;
            }
            next = (next + 1) & mask;
        }
        self.hash_part_mut().slots[hole] = 0;
    }

    pub(super) fn native_set(
        &mut self,
        key: Key,
        value: Value,
        string_hash: impl FnOnce(StringId) -> u32,
    ) -> Result<(), Error> {
        if let Some(index) = array_index(key.value()).filter(|&i| i < self.array.len()) {
            self.array.try_make_writable(index + 1)?;
            self.array[index] = value;
            self.touch_array_at(index);
            return Ok(());
        }
        let identity_hash = key.hash();
        let vacant = match self.slot_hashed(key, identity_hash) {
            Some((_, Some(index))) => {
                if value == Value::Nil {
                    if self.hash_part().entries[index].value != Value::Nil && key.dead().is_some() {
                        self.hash_part_mut().native_dead_keys += 1;
                    }
                } else if self.hash_part().native_dead_keys != 0
                    && self.hash_part().entries[index].value == Value::Nil
                    && key.dead().is_some()
                {
                    self.hash_part_mut().native_dead_keys -= 1;
                }
                self.hash_part_mut().entries[index].value = value;
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
        let (mut vacant, mut position) = match vacant {
            Some(slot) => (
                slot,
                main_position(key, hash, self.hash_part().entries.len()),
            ),
            None if self.array.is_empty() => {
                debug_assert!(self.hash_part().entries.is_empty());
                // With one key, only array index 1 can exceed half occupancy.
                let array =
                    usize::from(matches!(key, Key::Number(bits) if bits == 1.0_f64.to_bits()));
                self.native_resize(array, 1 - array)?;
                if array != 0 {
                    self.array[0] = value;
                    return Ok(());
                }
                (identity_hash & (self.hash_part().slots.len() - 1), 0)
            }
            None => {
                self.native_rehash(key)?;
                return self.native_set(key, value, |_| hash);
            }
        };
        if self.hash_part().entries[position].value != Value::Nil {
            let Some(new_position) = self.native_resolve_collision::<true>(position) else {
                self.native_rehash(key)?;
                return self.native_set(key, value, |_| hash);
            };
            position = new_position;
        } else if self.hash_part().entries[position].key != Key::Vacant
            && let Some((slot, Some(_))) = self.slot(self.hash_part().entries[position].key)
        {
            // A deleted main node is reusable, but its collision chain remains.
            if self.hash_part().entries[position].key.dead().is_some() {
                self.hash_part_mut().native_dead_keys -= 1;
            }
            self.native_index_remove_at(slot);
            // Deletion can shift the identity probe chain before insertion.
            vacant = self
                .slot_hashed(key, identity_hash)
                .expect("allocated identity index")
                .0;
        }
        self.hash_part_mut().entries[position].key = key;
        self.hash_part_mut().entries[position].value = value;
        if value == Value::Nil && key.dead().is_some() {
            self.hash_part_mut().native_dead_keys += 1;
        }
        self.hash_part_mut().entries[position].native_hash = hash;
        self.hash_part_mut().slots[vacant] =
            (position as u32 + 1) | Self::slot_tag(self.hash_part().slots.len(), identity_hash).0;
        self.touch_fields();
        self.invalidate_trace();
        if matches!(key, Key::String(_)) {
            self.remember_hash_entry(position as u32 + 1);
        }
        Ok(())
    }

    #[inline]
    fn native_resolve_collision<const INDEXED: bool>(&mut self, position: usize) -> Option<usize> {
        let part = self.hash_part();
        let free = part.entries[..part.last_free as usize]
            .iter()
            .rposition(|entry| matches!(entry.key, Key::Vacant))?;
        let old = part.entries[position];
        let other = main_position(old.key, old.native_hash, part.entries.len());
        let part = self.hash_part_mut();
        part.last_free = free as u32;
        if other == position {
            part.entries[free].next = old.next;
            part.entries[position].next = free as u32 + 1;
            return Some(free);
        }
        let mut previous = other;
        while part.entries[previous].next != position as u32 + 1 {
            previous = part.entries[previous].next as usize - 1;
        }
        if INDEXED {
            let slot = self.native_identity_slot(position);
            let word = &mut self.hash_part_mut().slots[slot];
            *word = *word - position as u32 + free as u32;
        }
        let part = self.hash_part_mut();
        part.entries[previous].next = free as u32 + 1;
        part.entries[free] = old;
        part.entries[position] = EMPTY;
        Some(position)
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
        for entry in &*self.hash_part().entries {
            if entry.value != Value::Nil {
                total += 1;
                count(entry.key);
            }
        }
        count(extra);
        // An array of twice the total population or more cannot be over half full.
        let powers = (usize::BITS + 1 - (total - 1).leading_zeros()) as usize;
        let (mut used, mut array, mut array_used) = (0, 0, 0);
        for (power, &count) in counts[..powers.min(counts.len())].iter().enumerate() {
            used += count;
            if used > (1usize << power) / 2 {
                array = 1usize << power;
                array_used = used;
            }
        }
        self.native_resize(array, total - array_used)
    }

    fn native_reinsert(&mut self, key: Key, value: Value, hash: u32) {
        if let Some(index) = array_index(key.value()).filter(|&i| i < self.array.len()) {
            self.array[index] = value;
            return;
        }
        let mut position = main_position(key, hash, self.hash_part().entries.len());
        if self.hash_part().entries[position].value != Value::Nil {
            position = self
                .native_resolve_collision::<false>(position)
                .expect("space for live keys in replacement hash");
        }
        let entry = &mut self.hash_part_mut().entries[position];
        entry.key = key;
        entry.value = value;
        entry.native_hash = hash;
    }

    fn native_build_index(&mut self) {
        let Some(part) = self.hash.as_deref_mut() else {
            return;
        };
        let len = part.slots.len();
        let mask = len - 1;
        for (index, entry) in part.entries.iter().enumerate() {
            if entry.value != Value::Nil {
                let hash = entry.key.hash();
                let mut slot = hash & mask;
                while part.slots[slot] != 0 {
                    slot = (slot + 1) & mask;
                }
                part.slots[slot] = (index as u32 + 1) | Self::slot_tag(len, hash).0;
            }
        }
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
            .heap_mut()
            .try_reserve_exact(array)
            .map_err(|_| Error::memory())?;
        let shared = array.min(self.array.len());
        replacement
            .array
            .heap_mut()
            .extend_from_slice(&self.array[..shared]);
        replacement.array.heap_mut().resize(array, Value::Nil);
        if records != 0 {
            let part = replacement.ensure_hash_part()?;
            part.entries
                .try_reserve_exact(records)
                .map_err(|_| Error::memory())?;
            part.entries.resize(records, EMPTY);
            let slots = (records * 2).max(8);
            part.slots
                .try_reserve_exact(slots)
                .map_err(|_| Error::memory())?;
            part.slots.resize(slots, 0);
            part.last_free = records as u32;
        }
        for (index, &value) in self.array.iter().enumerate().skip(array) {
            if value != Value::Nil {
                let key = Key::Number(((index + 1) as f64).to_bits());
                let hash = hash(key, |_| {
                    unreachable!("numeric keys do not use a string hash")
                });
                replacement.native_reinsert(key, value, hash);
            }
        }
        for entry in self
            .hash_part()
            .entries
            .iter()
            .rev()
            .filter(|e| e.value != Value::Nil)
        {
            replacement.native_reinsert(entry.key, entry.value, entry.native_hash);
        }
        // The keys have settled into their Lua 5.1 positions. Index them once.
        replacement.native_build_index();
        self.array = replacement.array;
        self.hash = replacement.hash;
        if let Some(part) = self.hash.as_deref_mut() {
            part.native_dead_keys = 0;
            part.next_hash_hint.store(0, Ordering::Relaxed);
        }
        self.invalidate_trace();
        self.touch_array();
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
        assert_eq!(table.hash_part().native_dead_keys, 1);
        table.native_set(key(1), Value::Number(1.0), hash).unwrap();
        assert_eq!(table.hash_part().native_dead_keys, 0);
        table.native_set(key(1), Value::Nil, hash).unwrap();
        table.retire_native_keys_in_trace_slice(1, 512);
        assert_eq!(table.hash_part().native_dead_keys, 0);
        assert!(table.slot(key(1)).unwrap().1.is_none());

        for index in 2..=256 {
            table.native_set(key(index), Value::Nil, hash).unwrap();
        }
        assert_eq!(table.hash_part().native_dead_keys, 255);
        table.retire_native_keys_in_trace_slice(1, 512);
        assert_eq!(table.hash_part().native_dead_keys, 0);
        assert!((2..=256).all(|index| table.slot(key(index)).unwrap().1.is_none()));
        table.native_set(key(256), Value::Nil, hash).unwrap();
        assert_eq!(table.hash_part().native_dead_keys, 1);
        table.native_resize(0, 512).unwrap();
        assert_eq!(table.hash_part().native_dead_keys, 0);
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
        // Array, hash header, hash entries and the identity index.
        for nth in 0..=4 {
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
                assert_eq!(stats.attempts, 4);
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
