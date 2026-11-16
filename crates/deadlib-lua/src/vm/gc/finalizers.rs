//! Pending finalizers reuse storage reserved before publishing userdata.
use super::*;

#[derive(Debug)]
pub(in crate::vm) struct Finalizers {
    values: Buffer<UserdataId>,
    first: usize,
}
impl Finalizers {
    pub(in crate::vm) fn new_in(memory: Memory) -> Self {
        Self {
            values: Buffer::new_in(memory),
            first: 0,
        }
    }
    pub(in crate::vm) fn capacity(&self) -> usize {
        self.values.capacity()
    }
    pub(in crate::vm) fn is_empty(&self) -> bool {
        self.first == self.values.len()
    }
    pub(in crate::vm) fn front(&self) -> Option<&UserdataId> {
        self.values.get(self.first)
    }
    pub(in crate::vm) fn iter(&self) -> std::slice::Iter<'_, UserdataId> {
        self.values[self.first..].iter()
    }
    pub(in crate::vm) fn try_reserve_total(&mut self, total: usize) -> Result<(), Error> {
        self.values
            .try_reserve(total.saturating_sub(self.values.len()))
    }
    pub(in crate::vm) fn push_back(&mut self, value: UserdataId) {
        if self.values.len() == self.values.capacity() && self.first != 0 {
            let remaining = self.values.len() - self.first;
            self.values.copy_within(self.first.., 0);
            self.values.truncate(remaining);
            self.first = 0;
        }
        // Each pending ID names a distinct, still-live userdata. Its creation
        // reserved space for all live userdata, including already pending IDs.
        assert!(self.values.len() < self.values.capacity());
        self.values.push(value);
    }
    pub(in crate::vm) fn pop_front(&mut self) -> Option<UserdataId> {
        let value = self.front().copied()?;
        self.first += 1;
        if self.first == self.values.len() {
            self.values.clear();
            self.first = 0;
        }
        Some(value)
    }
    #[cfg(test)]
    pub(in crate::vm) fn contains(&self, value: &UserdataId) -> bool {
        self.iter().any(|id| id == value)
    }
}
