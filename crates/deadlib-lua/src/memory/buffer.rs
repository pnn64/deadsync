//! Small internal vector with explicit allocator ownership and fallible growth.
use super::Memory;
use crate::Error;
use std::{
    alloc::Layout,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

struct Raw<T, const EMBEDDED: bool> {
    pointer: NonNull<T>,
    capacity: usize,
    memory: Memory,
    marker: PhantomData<T>,
}
impl<T, const EMBEDDED: bool> Raw<T, EMBEDDED> {
    fn layout(capacity: usize) -> Result<Layout, Error> {
        Layout::array::<T>(capacity).map_err(|_| Error::memory())
    }
    const EMBEDDED_CAPACITY: usize = 1 << (usize::BITS - 1);

    fn embedded(&self) -> bool {
        EMBEDDED && self.capacity & Self::EMBEDDED_CAPACITY != 0
    }

    fn grow(&mut self, capacity: usize, initialized: usize) -> Result<(), Error> {
        let embedded = self.embedded();
        let old_capacity = if embedded { 0 } else { self.capacity };
        let old = Self::layout(old_capacity)?;
        let new = Self::layout(capacity)?;
        let pointer = if old_capacity == 0 {
            std::ptr::null_mut()
        } else {
            self.pointer.as_ptr().cast()
        };
        // SAFETY: an independently owned allocation has this exact layout.
        // Borrowed storage stays in place until the new allocation succeeds.
        let pointer = unsafe { self.memory.resize(pointer, old, new) }?.cast::<T>();
        if embedded {
            // SAFETY: a fresh block and the initialized borrowed prefix are
            // disjoint. The raw copy transfers ownership to the new block.
            unsafe { std::ptr::copy_nonoverlapping(self.pointer.as_ptr(), pointer, initialized) };
        }
        self.pointer = NonNull::new(pointer).expect("nonzero buffer growth");
        self.capacity = capacity;
        Ok(())
    }
}
impl<T, const EMBEDDED: bool> Drop for Raw<T, EMBEDDED> {
    fn drop(&mut self) {
        if self.capacity != 0 && size_of::<T>() != 0 && !self.embedded() {
            let old = Self::layout(self.capacity).expect("allocated buffer layout");
            let new = Self::layout(0).expect("empty layout");
            // SAFETY: all elements have been dropped, including when their Drop
            // unwinds. This field is the sole allocation owner, with exact sizes.
            unsafe { self.memory.resize(self.pointer.as_ptr().cast(), old, new) }
                .expect("free owned buffer");
        }
    }
}

pub(crate) struct Buffer<T, const EMBEDDED: bool = false> {
    raw: Raw<T, EMBEDDED>,
    len: usize,
}
// SAFETY: the sole element/allocation owner follows the buffer. Memory serializes
// callback access across owners; element access obeys the usual slice bounds.
unsafe impl<T: Send, const EMBEDDED: bool> Send for Buffer<T, EMBEDDED> {}
unsafe impl<T: Sync, const EMBEDDED: bool> Sync for Buffer<T, EMBEDDED> {}
impl<T> Default for Buffer<T> {
    fn default() -> Self {
        Self::new_raw(Memory::default())
    }
}
impl<T: std::fmt::Debug, const EMBEDDED: bool> std::fmt::Debug for Buffer<T, EMBEDDED> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}
impl<T> Buffer<T> {
    pub(crate) const fn new_in(memory: Memory) -> Self {
        Self::new_raw(memory)
    }
}

impl<T, const EMBEDDED: bool> Buffer<T, EMBEDDED> {
    const fn new_raw(memory: Memory) -> Self {
        Self {
            raw: Raw {
                pointer: NonNull::dangling(),
                capacity: if size_of::<T>() == 0 { usize::MAX } else { 0 },
                memory,
                marker: PhantomData,
            },
            len: 0,
        }
    }
    pub(crate) fn memory(&self) -> &Memory {
        &self.raw.memory
    }
    pub(crate) fn capacity(&self) -> usize {
        if EMBEDDED && size_of::<T>() != 0 {
            self.raw.capacity & !Raw::<T, EMBEDDED>::EMBEDDED_CAPACITY
        } else {
            self.raw.capacity
        }
    }
    #[inline]
    pub(crate) fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, true)
    }
    #[inline]
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, false)
    }
    #[inline]
    fn reserve(&mut self, additional: usize, exact: bool) -> Result<(), Error> {
        if additional <= self.capacity() - self.len {
            return Ok(());
        }
        self.grow(additional, exact)
    }
    #[cold]
    fn grow(&mut self, additional: usize, exact: bool) -> Result<(), Error> {
        let needed = self.len.checked_add(additional).ok_or_else(Error::memory)?;
        let minimum = if size_of::<T>() == 1 {
            8
        } else if size_of::<T>() <= 1024 {
            4
        } else {
            1
        };
        let capacity = if exact {
            needed
        } else {
            needed.max(self.capacity().saturating_mul(2)).max(minimum)
        };
        self.raw.grow(capacity, self.len)
    }
    #[inline]
    pub(crate) fn push(&mut self, value: T) {
        self.try_reserve(1).expect("reserve Lua buffer before push");
        // SAFETY: capacity covers the next, uninitialized slot (also for ZSTs).
        unsafe {
            self.raw.pointer.as_ptr().add(self.len).write(value);
        }
        self.len += 1;
    }
    #[inline]
    pub(crate) fn try_push(&mut self, value: T) -> Result<(), Error> {
        self.try_reserve(1)?;
        self.push(value);
        Ok(())
    }
    pub(crate) fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: remove the last initialized element, transferring its ownership.
        Some(unsafe { self.raw.pointer.as_ptr().add(self.len).read() })
    }
    pub(crate) fn pop_if(&mut self, predicate: impl FnOnce(&mut T) -> bool) -> Option<T> {
        if self.last_mut().is_some_and(predicate) {
            self.pop()
        } else {
            None
        }
    }
    pub(crate) fn clear(&mut self) {
        self.truncate(0);
    }
    pub(crate) fn swap_remove(&mut self, index: usize) -> T {
        assert!(index < self.len);
        let last = self.len - 1;
        self.swap(index, last);
        self.pop().expect("nonempty buffer")
    }
    pub(crate) fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        let removed = self.len - len;
        self.len = len;
        // SAFETY: the removed initialized tail is no longer visible. Slice drop
        // glue drops remaining elements if one destructor unwinds.
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                self.raw.pointer.as_ptr().add(len),
                removed,
            ));
        }
    }
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        // Swapping keeps every slot initialized if the predicate unwinds.
        // Retained entries keep their original order; rejected entries move
        // behind them and are dropped only after predicate evaluation finishes.
        let mut write = 0;
        for read in 0..self.len {
            if keep(&self[read]) {
                self.swap(write, read);
                write += 1;
            }
        }
        self.truncate(write);
    }
    #[inline]
    pub(crate) fn resize(&mut self, len: usize, value: T)
    where
        T: Clone,
    {
        if len <= self.len {
            self.truncate(len);
            return;
        }
        self.try_reserve(len - self.len)
            .expect("reserve Lua buffer before resize");
        while self.len < len {
            let item = value.clone();
            // SAFETY: the complete tail was reserved before cloning. No
            // references to this exclusive buffer survive growth. Advance the
            // initialized length after each write so unwinding drops its tail.
            unsafe { self.raw.pointer.as_ptr().add(self.len).write(item) };
            self.len += 1;
        }
    }
    #[inline]
    pub(crate) fn extend_with(&mut self, end: usize, mut value_at: impl FnMut(usize) -> T)
    where
        T: Copy,
    {
        assert!(end <= self.capacity());
        for index in self.len..end {
            // SAFETY: the slot is within the reserved tail and is uninitialized.
            // If value_at panics, initialized tail elements have no destructor.
            unsafe { self.raw.pointer.as_ptr().add(index).write(value_at(index)) };
        }
        self.len = end.max(self.len);
    }
    #[inline]
    pub(crate) fn extend_from_slice(&mut self, values: &[T])
    where
        T: Copy,
    {
        self.try_reserve(values.len())
            .expect("reserve Lua buffer before extend");
        // SAFETY: exclusive access excludes overlap, capacity covers the copy,
        // and Copy elements can be duplicated without transferring drop ownership.
        unsafe {
            std::ptr::copy_nonoverlapping(
                values.as_ptr(),
                self.raw.pointer.as_ptr().add(self.len),
                values.len(),
            );
        }
        self.len += values.len();
    }
    pub(crate) fn extend_from_within(&mut self, range: std::ops::Range<usize>)
    where
        T: Copy,
    {
        assert!(range.start <= range.end && range.end <= self.len);
        let count = range.len();
        self.try_reserve(count)
            .expect("reserve Lua buffer before extend");
        let pointer = self.raw.pointer.as_ptr();
        // SAFETY: the initialized source ends at or before len. The reserved
        // destination starts at len, so the spans do not overlap. The pointer
        // is taken after any growth, and Copy elements can be duplicated.
        unsafe {
            std::ptr::copy_nonoverlapping(pointer.add(range.start), pointer.add(self.len), count);
        }
        self.len += count;
    }
    pub(crate) fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), Error>
    where
        T: Copy,
    {
        self.try_reserve(values.len())?;
        self.extend_from_slice(values);
        Ok(())
    }
}
impl<T> Buffer<T, true> {
    pub(crate) const fn empty_embeddable_global() -> Self {
        Self::new_raw(Memory(None))
    }

    pub(crate) const fn new_embeddable_in(memory: Memory) -> Self {
        Self::new_raw(memory)
    }

    /// The caller owns this aligned, uninitialized storage and keeps it
    /// exclusive until the buffer grows or is dropped. Only initialized
    /// elements belong to the buffer.
    pub(crate) unsafe fn attach(&mut self, pointer: NonNull<T>, capacity: usize) {
        debug_assert_ne!(size_of::<T>(), 0);
        debug_assert_eq!(self.raw.capacity, 0);
        debug_assert!(capacity < Raw::<T, true>::EMBEDDED_CAPACITY);
        self.raw.pointer = pointer;
        self.raw.capacity = capacity | Raw::<T, true>::EMBEDDED_CAPACITY;
    }

    pub(crate) fn allocated_capacity(&self) -> usize {
        if self.raw.embedded() {
            0
        } else {
            self.capacity()
        }
    }
}

impl<T, const EMBEDDED: bool> Deref for Buffer<T, EMBEDDED> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // SAFETY: exactly len elements are initialized, and Raw owns the buffer.
        unsafe { std::slice::from_raw_parts(self.raw.pointer.as_ptr(), self.len) }
    }
}
impl<T, const EMBEDDED: bool> DerefMut for Buffer<T, EMBEDDED> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: the exclusive borrow covers every initialized element.
        unsafe { std::slice::from_raw_parts_mut(self.raw.pointer.as_ptr(), self.len) }
    }
}
impl<T, const EMBEDDED: bool> Drop for Buffer<T, EMBEDDED> {
    fn drop(&mut self) {
        self.truncate(0);
    }
}
impl<'a, T, const EMBEDDED: bool> IntoIterator for &'a Buffer<T, EMBEDDED> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T, const EMBEDDED: bool> IntoIterator for &'a mut Buffer<T, EMBEDDED> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
