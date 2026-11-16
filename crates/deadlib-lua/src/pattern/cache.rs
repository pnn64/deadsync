//! Allocator-owned pattern lookup. Removal never allocates; the next fallible
//! insertion can rebuild tombstones or grow before publishing a new pattern.
use super::{Buffer, Memory, Pattern};
use crate::{Error, StringId};
use std::ops::Index;

type Key = (StringId, bool);

#[derive(Debug)]
enum Slot {
    Empty,
    Removed,
    Occupied(Key, Pattern),
}

#[derive(Debug)]
pub(crate) struct Cache {
    slots: Buffer<Slot>,
    len: usize,
    removed: usize,
}

impl Cache {
    pub(crate) fn new_in(memory: Memory) -> Self {
        Self {
            slots: Buffer::new_in(memory),
            len: 0,
            removed: 0,
        }
    }
    pub(crate) fn storage_bytes(&self) -> usize {
        self.slots.capacity() * size_of::<Slot>()
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &Pattern> {
        self.slots.iter().filter_map(|slot| match slot {
            Slot::Occupied(_, value) => Some(value),
            _ => None,
        })
    }
    fn bucket(key: Key, capacity: usize) -> usize {
        let mut hash = key.0.0 ^ (u64::from(key.1) << 63);
        hash ^= hash >> 30;
        hash = hash.wrapping_mul(0xbf58476d1ce4e5b9);
        hash ^= hash >> 27;
        hash = hash.wrapping_mul(0x94d049bb133111eb);
        hash ^= hash >> 31;
        hash as usize & (capacity - 1)
    }
    fn find(&self, key: Key) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let mut index = Self::bucket(key, self.slots.len());
        loop {
            match &self.slots[index] {
                Slot::Empty => return None,
                Slot::Occupied(found, _) if *found == key => return Some(index),
                _ => index = (index + 1) & (self.slots.len() - 1),
            }
        }
    }
    pub(crate) fn contains_key(&self, key: &Key) -> bool {
        self.find(*key).is_some()
    }
    pub(crate) fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = Slot::Empty;
        }
        self.len = 0;
        self.removed = 0;
    }
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&Key, &Pattern) -> bool) {
        for slot in &mut self.slots {
            if let Slot::Occupied(key, value) = slot
                && !keep(key, value)
            {
                *slot = Slot::Removed;
                self.len -= 1;
                self.removed += 1;
            }
        }
        if self.len == 0 {
            self.clear();
        }
    }
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        if additional == 0 {
            return Ok(());
        }
        let needed = self.len.checked_add(additional).ok_or_else(Error::memory)?;
        let occupied = needed.checked_add(self.removed).ok_or_else(Error::memory)?;
        let mut capacity = self.slots.len().max(8);
        if !self.slots.is_empty() && occupied <= capacity - capacity / 4 {
            return Ok(());
        }
        while needed > capacity - capacity / 4 {
            capacity = capacity.checked_mul(2).ok_or_else(Error::memory)?;
        }
        let mut next = Self::new_in(self.slots.memory().clone());
        next.slots.try_reserve_exact(capacity)?;
        for _ in 0..capacity {
            next.slots.push(Slot::Empty);
        }
        for slot in &mut self.slots {
            if let Slot::Occupied(key, value) = std::mem::replace(slot, Slot::Empty) {
                next.insert(key, value);
            }
        }
        *self = next;
        Ok(())
    }
    /// Caller reserved space before preparing the pattern. No allocation can
    /// occur between its successful compilation and publication here.
    pub(crate) fn insert(&mut self, key: Key, value: Pattern) {
        if let Some(index) = self.find(key) {
            self.slots[index] = Slot::Occupied(key, value);
            return;
        }
        assert!(self.len + self.removed < self.slots.len());
        let mut index = Self::bucket(key, self.slots.len());
        loop {
            match self.slots[index] {
                Slot::Empty => break,
                Slot::Removed => {
                    self.removed -= 1;
                    break;
                }
                Slot::Occupied(_, _) => index = (index + 1) & (self.slots.len() - 1),
            }
        }
        self.slots[index] = Slot::Occupied(key, value);
        self.len += 1;
    }
}

impl Index<&Key> for Cache {
    type Output = Pattern;
    fn index(&self, key: &Key) -> &Pattern {
        let index = self.find(*key).expect("prepared pattern");
        let Slot::Occupied(_, value) = &self.slots[index] else {
            unreachable!()
        };
        value
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;
    use crate::{ErrorKind, memory::tests::with_memory};

    #[test]
    fn collision_chains_survive_removal_and_failed_tombstone_rebuilds() {
        with_memory(|memory, attempts, reject| {
            let mut cache = Cache::new_in(memory.clone());
            cache.try_reserve(0).unwrap();
            assert_eq!(cache.storage_bytes(), 0);
            cache.try_reserve(24).unwrap();
            assert_eq!(cache.slots.len(), 32);
            let keys: Vec<_> = (0..)
                .map(|n| (StringId(n), n & 1 != 0))
                .filter(|&key| Cache::bucket(key, 32) == 31)
                .take(24)
                .collect();
            for &key in &keys {
                cache.insert(key, Pattern::compile(b"z", false, memory.clone()).unwrap());
            }
            let start = attempts.get();
            reject.set(start + 1);
            cache.retain(|key, _| keys[18..].contains(key));
            assert_eq!((cache.len, cache.removed), (6, 18));
            for (i, key) in keys.iter().enumerate() {
                assert_eq!(cache.contains_key(key), i >= 18);
            }
            cache.try_reserve(0).unwrap();
            assert_eq!(attempts.get(), start);
            let error = cache.try_reserve(1).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Memory);
            assert_eq!((cache.len, cache.removed), (6, 18));
            for key in &keys[18..] {
                assert!(cache.contains_key(key));
            }
            reject.set(0);
            cache.try_reserve(1).unwrap();
            assert_eq!((cache.len, cache.removed, cache.slots.len()), (6, 0, 32));
            cache.insert(
                keys[0],
                Pattern::compile(b"y", false, memory.clone()).unwrap(),
            );
            assert_eq!(cache.len, 7);
            for _ in 0..128 {
                cache.retain(|key, _| *key != keys[0]);
                cache.try_reserve(1).unwrap();
                cache.insert(
                    keys[0],
                    Pattern::compile(b"", false, memory.clone()).unwrap(),
                );
                assert_eq!(cache.slots.len(), 32);
                for key in &keys[18..] {
                    assert!(cache.contains_key(key));
                }
            }
            let start = attempts.get();
            reject.set(start + 1);
            cache.clear();
            assert_eq!(cache.values().count(), 0);
            assert_eq!(attempts.get(), start);
            reject.set(0);
        });
        eprintln!("PATTERN_REJECTION_POINTS 1");
    }
}
