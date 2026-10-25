//! Reusable flat object storage with generation-checked external identities.
use std::ops::{Index, IndexMut};

#[derive(Debug)]
pub(crate) struct Arena<T> {
    values: Vec<Option<T>>,
    generations: Vec<u32>,
    free: Vec<u32>,
    marks: Vec<u64>,
    live: usize,
    pub allocated: usize,
}
impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            generations: Vec::new(),
            free: Vec::new(),
            marks: Vec::new(),
            live: 0,
            allocated: 0,
        }
    }
}
pub(crate) fn index(key: u64) -> usize {
    key as u32 as usize
}
impl<T> Arena<T> {
    pub fn insert(&mut self, value: T) -> u64 {
        let before = self.storage_bytes();
        let slot = if let Some(slot) = self.free.pop() {
            self.values[slot as usize] = Some(value);
            slot as usize
        } else {
            let slot = self.values.len();
            assert!(
                slot < u32::MAX as usize,
                "Lua arena exhausted addressable slots"
            );
            self.values.push(Some(value));
            self.generations.push(1);
            if slot.is_multiple_of(64) {
                self.marks.push(0);
            }
            slot
        };
        self.live += 1;
        self.allocated = self
            .allocated
            .saturating_add(self.storage_bytes().saturating_sub(before))
            .saturating_add(std::mem::size_of::<T>());
        (u64::from(self.generations[slot]) << 32) | (slot as u64)
    }
    pub fn key(&self, slot: usize) -> Option<u64> {
        self.values.get(slot)?.as_ref()?;
        Some((u64::from(self.generations[slot]) << 32) | (slot as u64))
    }
    pub fn contains(&self, key: u64) -> bool {
        self.get(key).is_some()
    }
    pub fn get(&self, key: u64) -> Option<&T> {
        let slot = index(key);
        if self.generations.get(slot).copied() != Some((key >> 32) as u32) {
            return None;
        }
        self.values.get(slot)?.as_ref()
    }
    pub fn remove(&mut self, key: u64) -> Option<T> {
        if !self.contains(key) {
            return None;
        }
        let slot = index(key);
        let value = self.values[slot].take();
        self.live -= 1;
        // Retire a slot at generation exhaustion rather than alias an old handle.
        if let Some(next) = self.generations[slot].checked_add(1) {
            self.generations[slot] = next;
            self.free.push(slot as u32);
        }
        self.marks[slot / 64] &= !(1u64 << (slot % 64));
        value
    }
    pub fn get_mut(&mut self, key: u64) -> Option<&mut T> {
        let slot = index(key);
        if self.generations.get(slot).copied() != Some((key >> 32) as u32) {
            return None;
        }
        self.values.get_mut(slot)?.as_mut()
    }
    pub fn mark(&mut self, key: u64) -> bool {
        if !self.contains(key) {
            return false;
        }
        let slot = index(key);
        let bit = 1u64 << (slot % 64);
        let old = self.marks[slot / 64];
        self.marks[slot / 64] |= bit;
        old & bit == 0
    }
    pub fn marked(&self, key: u64) -> bool {
        let slot = index(key);
        self.contains(key) && self.marks[slot / 64] & (1u64 << (slot % 64)) != 0
    }
    pub fn clear_marks(&mut self) {
        self.marks.fill(0);
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn live_len(&self) -> usize {
        self.live
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.values.iter().filter_map(Option::as_ref)
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.values.iter_mut().filter_map(Option::as_mut)
    }
    pub fn entries(&self) -> impl Iterator<Item = (usize, &T)> {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (i, v)))
    }
    pub fn storage_bytes(&self) -> usize {
        self.values.capacity() * std::mem::size_of::<Option<T>>()
            + self.generations.capacity() * 4
            + self.free.capacity() * 4
            + self.marks.capacity() * 8
    }
    pub fn note_alloc(&mut self, bytes: usize) {
        self.allocated = self.allocated.saturating_add(bytes);
    }
}
impl<T> Index<usize> for Arena<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        self.values[index].as_ref().expect("live Lua arena slot")
    }
}
impl<T> IndexMut<usize> for Arena<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        self.values[index].as_mut().expect("live Lua arena slot")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reuse_rejects_stale_handles_and_resets_marks() {
        let mut arena = Arena::default();
        let a = arena.insert(3);
        let b = arena.insert(7);
        assert!(arena.mark(a));
        assert!(!arena.mark(a));
        assert!(arena.marked(a));
        assert!(!arena.marked(b));
        assert_eq!(arena.remove(a), Some(3));
        let c = arena.insert(11);
        assert_eq!(index(a), index(c));
        assert_ne!(a, c);
        assert!(!arena.contains(a));
        assert!(!arena.marked(c));
        assert_eq!(arena.get(c), Some(&11));
        assert_eq!(arena.live_len(), 2);
        arena.clear_marks();
        assert!(!arena.marked(b));
    }
}
