//! Fallible, uniquely owned allocations in a Lua memory domain.
use super::{Memory, Release};
use crate::Error;
use std::{
    alloc::Layout,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

struct Allocation<T> {
    memory: Memory,
    value: T,
}

pub(crate) struct Unique<T>(NonNull<Allocation<T>>);

// SAFETY: each pointer uniquely owns its value and its allocation domain
// serializes access to the underlying allocator.
unsafe impl<T: Send> Send for Unique<T> {}
unsafe impl<T: Sync> Sync for Unique<T> {}

impl<T> Unique<T> {
    pub(crate) fn try_new_in(value: T, memory: Memory) -> Result<Self, Error> {
        let layout = Layout::new::<Allocation<T>>();
        let empty = Layout::from_size_align(0, layout.align()).expect("unique alignment");
        // SAFETY: the domain allocates a new block with the requested layout.
        let pointer = unsafe { memory.resize(ptr::null_mut(), empty, layout) }?;
        let pointer = NonNull::new(pointer.cast::<Allocation<T>>()).expect("nonempty allocation");
        // SAFETY: the new block has no initialized fields and is uniquely owned.
        unsafe { pointer.as_ptr().write(Allocation { memory, value }) };
        Ok(Self(pointer))
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        size_of::<Allocation<T>>()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Unique<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(formatter)
    }
}

impl<T> Deref for Unique<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: self owns the live allocation for the duration of its borrow.
        unsafe { &self.0.as_ref().value }
    }
}

impl<T> DerefMut for Unique<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: this exclusive borrow excludes all other value references.
        unsafe { &mut self.0.as_mut().value }
    }
}

impl<T> Drop for Unique<T> {
    fn drop(&mut self) {
        // SAFETY: the unique owner still holds the initialized allocation.
        let memory = unsafe { self.0.as_ref().memory.clone() };
        let _release = Release {
            pointer: self.0.cast(),
            layout: Layout::new::<Allocation<T>>(),
            memory,
        };
        // SAFETY: the guard releases storage after its fields have been dropped.
        unsafe { ptr::drop_in_place(self.0.as_ptr()) };
    }
}
