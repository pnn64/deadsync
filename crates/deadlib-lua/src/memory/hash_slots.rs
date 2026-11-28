//! Table lookup words, optionally backed by space after the hash header.
use super::Memory;
use crate::Error;
use std::{
    alloc::Layout,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

pub(crate) struct HashSlots {
    pointer: NonNull<u32>,
    // The high bit marks words which are owned by the table's hash header.
    allocation: usize,
    len: usize,
    memory: Memory,
}

// SAFETY: the index has one owner. Its backing span is disjoint from the
// header, and reads and writes obey the same borrowing rules as a slice.
unsafe impl Send for HashSlots {}
unsafe impl Sync for HashSlots {}

impl HashSlots {
    pub(crate) const INITIAL: usize = 8;
    const BORROWED: usize = 1 << (usize::BITS - 1);

    pub(crate) const fn empty_global() -> Self {
        Self::new_in(Memory(None))
    }

    pub(crate) const fn new_in(memory: Memory) -> Self {
        Self {
            pointer: NonNull::dangling(),
            allocation: 0,
            len: 0,
            memory,
        }
    }

    /// The caller retains exclusive use of INITIAL aligned words until this
    /// index spills to its own allocation or is dropped.
    pub(crate) unsafe fn attach(&mut self, pointer: NonNull<u32>) {
        debug_assert_eq!(self.allocation, 0);
        self.pointer = pointer;
        self.allocation = Self::BORROWED | Self::INITIAL;
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        if self.allocation & Self::BORROWED == 0 {
            self.allocation * size_of::<u32>()
        } else {
            0
        }
    }

    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, false)
    }

    pub(crate) fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, true)
    }

    #[inline]
    fn reserve(&mut self, additional: usize, exact: bool) -> Result<(), Error> {
        let capacity = self.allocation & !Self::BORROWED;
        if additional <= capacity - self.len {
            return Ok(());
        }
        let needed = self.len.checked_add(additional).ok_or_else(Error::memory)?;
        let capacity = if exact {
            needed
        } else {
            needed.max(capacity.saturating_mul(2)).max(4)
        };
        self.grow(capacity)
    }

    #[cold]
    fn grow(&mut self, capacity: usize) -> Result<(), Error> {
        let owned = self.allocation & Self::BORROWED == 0;
        let old_capacity = if owned { self.allocation } else { 0 };
        let old = Layout::array::<u32>(old_capacity).expect("allocated table index layout");
        let new = Layout::array::<u32>(capacity).map_err(|_| Error::memory())?;
        let allocation = if old_capacity == 0 {
            ptr::null_mut()
        } else {
            self.pointer.as_ptr().cast()
        };
        // SAFETY: the original index is either owned with this exact layout or
        // left in its header. No references survive this exclusive borrow.
        let pointer = unsafe { self.memory.resize(allocation, old, new)? }.cast::<u32>();
        if !owned {
            // SAFETY: a fresh block receives exactly the initialized prefix.
            unsafe { ptr::copy_nonoverlapping(self.pointer.as_ptr(), pointer, self.len) };
        }
        self.pointer = NonNull::new(pointer).expect("grown table index");
        self.allocation = capacity;
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn resize(&mut self, len: usize, value: u32) {
        if len > self.len {
            self.try_reserve(len - self.len)
                .expect("reserved table index");
            // SAFETY: reservation covers all newly initialized words.
            for i in self.len..len {
                unsafe { self.pointer.as_ptr().add(i).write(value) };
            }
        }
        self.len = len;
    }

    pub(crate) fn extend_from_slice(&mut self, words: &[u32]) {
        self.try_reserve(words.len()).expect("reserved table index");
        // SAFETY: the reserved tail is disjoint from the supplied slice.
        unsafe {
            ptr::copy_nonoverlapping(
                words.as_ptr(),
                self.pointer.as_ptr().add(self.len),
                words.len(),
            )
        };
        self.len += words.len();
    }
}

impl Deref for HashSlots {
    type Target = [u32];

    #[inline]
    fn deref(&self) -> &[u32] {
        // SAFETY: both forms of backing storage retain the initialized prefix.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.len) }
    }
}

impl DerefMut for HashSlots {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u32] {
        // SAFETY: this exclusive borrow covers the initialized prefix.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.len) }
    }
}

impl std::fmt::Debug for HashSlots {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(formatter)
    }
}

impl Drop for HashSlots {
    fn drop(&mut self) {
        if self.heap_bytes() != 0 {
            let old = Layout::array::<u32>(self.allocation).expect("allocated table index layout");
            let empty = Layout::new::<[u32; 0]>();
            // SAFETY: only the index's independent allocation is released here.
            unsafe { self.memory.resize(self.pointer.as_ptr().cast(), old, empty) }
                .expect("free table index");
        }
    }
}
