//! Allocator-owned trace storage; publication requires a completed reservation.
use super::*;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Default)]
pub(super) struct Work(Buffer<Trace>);
impl Work {
    pub(super) fn new_in(memory: Memory) -> Self {
        Self(Buffer::new_in(memory))
    }
    pub(super) fn push(&mut self, item: Trace) {
        assert!(
            self.len() < self.capacity(),
            "reserve trace work before publication"
        );
        self.0.push(item);
    }
    #[cfg(test)]
    pub(super) fn with_capacity(capacity: usize) -> Self {
        let mut work = Self::default();
        work.try_reserve_exact(capacity).unwrap();
        work
    }
}
impl Deref for Work {
    type Target = Buffer<Trace>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Work {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Extend<Trace> for Work {
    fn extend<I: IntoIterator<Item = Trace>>(&mut self, items: I) {
        for item in items {
            self.push(item);
        }
    }
}
#[cfg(test)]
impl<const N: usize> From<[Trace; N]> for Work {
    fn from(items: [Trace; N]) -> Self {
        let mut work = Self::with_capacity(N);
        work.extend(items);
        work
    }
}
