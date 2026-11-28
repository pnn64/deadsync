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

pub(crate) struct Unique<T, const WORDS: usize = 0>(NonNull<Allocation<T>>);

// SAFETY: each pointer uniquely owns its value and its allocation domain
// serializes access to the underlying allocator.
unsafe impl<T: Send, const WORDS: usize> Send for Unique<T, WORDS> {}
unsafe impl<T: Sync, const WORDS: usize> Sync for Unique<T, WORDS> {}

impl<T> Unique<T> {
    pub(crate) fn try_new_in(value: T, memory: Memory) -> Result<Self, Error> {
        Self::try_new_with_storage_in(value, memory)
    }
}

impl<T, const WORDS: usize> Unique<T, WORDS> {
    fn extra_words(memory: &Memory) -> usize {
        if WORDS != 0 && memory.uses_global_allocator() {
            WORDS
        } else {
            0
        }
    }

    fn layout(memory: &Memory) -> Layout {
        let layout = Layout::new::<Allocation<T>>();
        let extra = Self::extra_words(memory);
        if extra == 0 {
            layout
        } else {
            layout
                .extend(Layout::array::<u32>(extra).expect("unique suffix size"))
                .expect("unique allocation and suffix")
                .0
                .pad_to_align()
        }
    }

    pub(crate) fn try_new_with_storage_in(value: T, memory: Memory) -> Result<Self, Error> {
        let layout = Self::layout(&memory);
        let empty = Layout::from_size_align(0, layout.align()).expect("unique alignment");
        // SAFETY: the domain allocates a new block with the requested layout.
        let pointer = unsafe { memory.resize(ptr::null_mut(), empty, layout) }?;
        let pointer = NonNull::new(pointer.cast::<Allocation<T>>()).expect("nonempty allocation");
        // SAFETY: the new block has no initialized fields and is uniquely owned.
        unsafe { pointer.as_ptr().write(Allocation { memory, value }) };
        Ok(Self(pointer))
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        if WORDS == 0 {
            size_of::<Allocation<T>>()
        } else {
            Self::layout(self.memory()).size()
        }
    }

    fn memory(&self) -> &Memory {
        // SAFETY: this owner retains the initialized allocator.
        unsafe { &self.0.as_ref().memory }
    }

    /// Raw access to the uninitialized words following the value.
    pub(crate) fn suffix_words(&self) -> Option<NonNull<u32>> {
        (Self::extra_words(self.memory()) != 0).then(|| {
            let offset = size_of::<Allocation<T>>().next_multiple_of(align_of::<u32>());
            // SAFETY: allocation reserves aligned words after its header and
            // value. References produced by dereferencing Unique exclude them.
            unsafe { NonNull::new_unchecked(self.0.as_ptr().cast::<u8>().add(offset).cast()) }
        })
    }
}

impl<T: std::fmt::Debug, const WORDS: usize> std::fmt::Debug for Unique<T, WORDS> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(formatter)
    }
}

impl<T, const WORDS: usize> Deref for Unique<T, WORDS> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: self owns the live allocation for the duration of its borrow.
        unsafe { &self.0.as_ref().value }
    }
}

impl<T, const WORDS: usize> DerefMut for Unique<T, WORDS> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: this exclusive borrow excludes all other value references.
        unsafe { &mut self.0.as_mut().value }
    }
}

impl<T, const WORDS: usize> Drop for Unique<T, WORDS> {
    fn drop(&mut self) {
        // SAFETY: the unique owner still holds the initialized allocation.
        let memory = unsafe { self.0.as_ref().memory.clone() };
        let _release = Release {
            pointer: self.0.cast(),
            layout: Self::layout(&memory),
            memory,
        };
        // SAFETY: the guard releases storage after its fields have been dropped.
        unsafe { ptr::drop_in_place(self.0.as_ptr()) };
    }
}
