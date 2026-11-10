//! Compiler-only index map whose allocations use the fallible workspace path.
use super::{repeat, reserve};
use crate::Error;
use std::{
    borrow::Borrow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

#[derive(Debug)]
pub(crate) struct Map<K, V> {
    entries: Vec<(K, V)>,
    slots: Vec<usize>,
}
impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            slots: Vec::new(),
        }
    }
}
fn hash(key: &(impl Hash + ?Sized)) -> usize {
    let mut state = DefaultHasher::new();
    key.hash(&mut state);
    state.finish() as usize
}
impl<K: Eq + Hash, V> Map<K, V> {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.slots.clear();
    }
    pub(crate) fn get<Q: Eq + Hash + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        if self.slots.is_empty() {
            return None;
        }
        let mut slot = hash(key) & (self.slots.len() - 1);
        loop {
            let index = self.slots[slot];
            if index == usize::MAX {
                return None;
            }
            if self.entries[index].0.borrow() == key {
                return Some(&self.entries[index].1);
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        let needed = self
            .entries
            .len()
            .checked_add(additional)
            .ok_or_else(Error::memory)?;
        let capacity = needed
            .checked_mul(2)
            .and_then(usize::checked_next_power_of_two)
            .ok_or_else(Error::memory)?
            .max(8);
        reserve(&mut self.entries, additional)?;
        if capacity <= self.slots.len() {
            return Ok(());
        }
        let mut slots = repeat(usize::MAX, capacity)?;
        for (index, (key, _)) in self.entries.iter().enumerate() {
            let mut slot = hash(key) & (capacity - 1);
            while slots[slot] != usize::MAX {
                slot = (slot + 1) & (capacity - 1);
            }
            slots[slot] = index;
        }
        self.slots = slots;
        Ok(())
    }
    /// The compiler reserves one insertion before calling this method.
    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        assert!(self.entries.len() < self.entries.capacity() && !self.slots.is_empty());
        let mut slot = hash(&key) & (self.slots.len() - 1);
        loop {
            let index = self.slots[slot];
            if index == usize::MAX {
                self.slots[slot] = self.entries.len();
                self.entries.push((key, value));
                return None;
            }
            if self.entries[index].0 == key {
                return Some(std::mem::replace(&mut self.entries[index].1, value));
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }
}
impl<K, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}
