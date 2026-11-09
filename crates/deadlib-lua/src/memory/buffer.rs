//! Small internal vector with explicit allocator ownership and fallible growth.
use super::Memory;
use crate::Error;
use std::{
    alloc::Layout,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

struct Raw<T> {
    pointer: NonNull<T>,
    capacity: usize,
    memory: Memory,
    marker: PhantomData<T>,
}
impl<T> Raw<T> {
    fn layout(capacity: usize) -> Result<Layout, Error> {
        Layout::array::<T>(capacity).map_err(|_| Error::memory())
    }
    fn grow(&mut self, capacity: usize) -> Result<(), Error> {
        let old = Self::layout(self.capacity)?;
        let new = Self::layout(capacity)?;
        let pointer = if self.capacity == 0 {
            std::ptr::null_mut()
        } else {
            self.pointer.as_ptr().cast()
        };
        // SAFETY: Raw owns the old allocation with this exact Layout. No slices
        // survive the exclusive borrow; initialized elements remain byte-identical.
        let pointer = unsafe { self.memory.resize(pointer, old, new) }?;
        self.pointer = NonNull::new(pointer.cast()).expect("nonzero buffer growth");
        self.capacity = capacity;
        Ok(())
    }
}
impl<T> Drop for Raw<T> {
    fn drop(&mut self) {
        if self.capacity != 0 && size_of::<T>() != 0 {
            let old = Self::layout(self.capacity).expect("allocated buffer layout");
            let new = Self::layout(0).expect("empty layout");
            // SAFETY: all elements have been dropped, including when their Drop
            // unwinds. This field is the sole allocation owner, with exact sizes.
            unsafe { self.memory.resize(self.pointer.as_ptr().cast(), old, new) }
                .expect("free owned buffer");
        }
    }
}

pub(crate) struct Buffer<T> {
    raw: Raw<T>,
    len: usize,
}
// SAFETY: the sole element/allocation owner follows the buffer. Memory serializes
// callback access across owners; element access obeys the usual slice bounds.
unsafe impl<T: Send> Send for Buffer<T> {}
unsafe impl<T: Sync> Sync for Buffer<T> {}
impl<T> Default for Buffer<T> {
    fn default() -> Self {
        Self::new_in(Memory::default())
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for Buffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}
impl<T> Buffer<T> {
    pub(crate) fn new_in(memory: Memory) -> Self {
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
        self.raw.capacity
    }
    pub(crate) fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, true)
    }
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, false)
    }
    fn reserve(&mut self, additional: usize, exact: bool) -> Result<(), Error> {
        let needed = self.len.checked_add(additional).ok_or_else(Error::memory)?;
        if needed <= self.capacity() {
            return Ok(());
        }
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
        self.raw.grow(capacity)
    }
    pub(crate) fn push(&mut self, value: T) {
        self.try_reserve(1).expect("reserve Lua buffer before push");
        // SAFETY: capacity covers the next, uninitialized slot (also for ZSTs).
        unsafe {
            self.raw.pointer.as_ptr().add(self.len).write(value);
        }
        self.len += 1;
    }
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
            self.push(value.clone());
        }
    }
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
        self.try_reserve(range.len())
            .expect("reserve Lua buffer before extend");
        for index in range {
            self.push(self[index]);
        }
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
impl<T> Deref for Buffer<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // SAFETY: exactly len elements are initialized, and Raw owns the buffer.
        unsafe { std::slice::from_raw_parts(self.raw.pointer.as_ptr(), self.len) }
    }
}
impl<T> DerefMut for Buffer<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: the exclusive borrow covers every initialized element.
        unsafe { std::slice::from_raw_parts_mut(self.raw.pointer.as_ptr(), self.len) }
    }
}
impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        self.truncate(0);
    }
}
impl<'a, T> IntoIterator for &'a Buffer<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut Buffer<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
