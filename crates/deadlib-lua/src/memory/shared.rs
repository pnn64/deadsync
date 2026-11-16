//! Immutable, explicitly allocated shared ownership, including projected views.
//!
//! The control header remembers the original value's destructor and layout.
//! A projected trait object or field retains that entire original allocation;
//! the final view drops the original value, never just its projected subobject.
use super::{Memory, Release};
use crate::Error;
use std::{
    alloc::Layout,
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Header {
    references: AtomicUsize,
    memory: Memory,
    layout: Layout,
    offset: usize,
    drop_value: unsafe fn(*mut u8),
}

// Requiring owned, thread-safe originals prevents projection from hiding a
// shorter lifetime or non-Send/Sync owner behind a Send/Sync field or trait.
pub(crate) struct Shared<T: ?Sized + Send + Sync + 'static> {
    header: NonNull<Header>,
    value: NonNull<T>,
}
// SAFETY: every original allocation and every projected view is Send + Sync.
// Atomic references retain the original value and Memory serializes callbacks.
unsafe impl<T: ?Sized + Send + Sync + 'static> Send for Shared<T> {}
unsafe impl<T: ?Sized + Send + Sync + 'static> Sync for Shared<T> {}

impl<T: Send + Sync + 'static> Shared<T> {
    // Mutation is needed only by malformed-bytecode tests. Always copy into a
    // fresh original allocation: a projected Shared can point at an unrelated
    // static reference, so uniqueness of its header alone permits no mutation.
    #[cfg(test)]
    pub(crate) fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        *this = Self::try_new_in((**this).clone(), Memory::default()).unwrap();
        // SAFETY: try_new_in just created this unprojected, uniquely owned T.
        unsafe { this.value.as_mut() }
    }

    pub(crate) fn try_new_in(value: T, memory: Memory) -> Result<Self, Error> {
        let (layout, offset) = Layout::new::<Header>()
            .extend(Layout::new::<T>())
            .map_err(|_| Error::memory())?;
        let layout = layout.pad_to_align();
        let empty = Layout::from_size_align(0, layout.align()).expect("shared alignment");
        // SAFETY: this is a new block with no prior allocation or initialized data.
        let pointer = unsafe { memory.resize(std::ptr::null_mut(), empty, layout) }?;
        let header = NonNull::new(pointer.cast::<Header>()).expect("nonempty shared header");
        // SAFETY: Layout::extend supplies an aligned value offset, including for
        // ZSTs. The successful allocation covers the header and original value.
        let value_pointer = unsafe { NonNull::new_unchecked(pointer.add(offset).cast::<T>()) };
        unsafe fn drop_value<T>(pointer: *mut u8) {
            // SAFETY: Header stores the destructor for the original initialized T.
            unsafe { std::ptr::drop_in_place(pointer.cast::<T>()) };
        }
        // SAFETY: both disjoint parts are aligned and uninitialized. These writes
        // cannot unwind; ownership transfers from the locals into the new block.
        unsafe {
            header.as_ptr().write(Header {
                references: AtomicUsize::new(1),
                memory,
                layout,
                offset,
                drop_value: drop_value::<T>,
            });
            value_pointer.as_ptr().write(value);
        }
        Ok(Self {
            header,
            value: value_pointer,
        })
    }
}

impl<T: ?Sized + Send + Sync + 'static> Shared<T> {
    pub(crate) fn ptr_eq(this: &Self, other: &Self) -> bool {
        std::ptr::eq(this.value.as_ptr(), other.value.as_ptr())
    }

    /// Retain the original owner while viewing a field, slice or trait object.
    /// The function's borrowed result cannot outlive its input independently.
    pub(crate) fn project<U: ?Sized + Send + Sync + 'static>(
        self,
        projection: fn(&T) -> &U,
    ) -> Shared<U> {
        let value = NonNull::from(projection(&self));
        let header = self.header;
        std::mem::forget(self);
        Shared { header, value }
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        // SAFETY: this owning reference retains the immutable header layout.
        unsafe { self.header.as_ref().layout.size() }
    }
}

impl<T: ?Sized + Send + Sync + 'static> Clone for Shared<T> {
    fn clone(&self) -> Self {
        // SAFETY: self retains a live control block while increasing its count.
        if unsafe { self.header.as_ref() }
            .references
            .fetch_add(1, Ordering::Relaxed)
            >= isize::MAX as usize
        {
            std::process::abort();
        }
        Self {
            header: self.header,
            value: self.value,
        }
    }
}

impl<T: ?Sized + Send + Sync + 'static> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: a projection is borrowed from the retained original owner;
        // shared views cannot move or mutate that owner's storage through &T.
        unsafe { self.value.as_ref() }
    }
}

impl<T: ?Sized + Send + Sync + 'static> AsRef<T> for Shared<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: ?Sized + Send + Sync + 'static> std::fmt::Debug for Shared<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Shared")
            .field("heap_bytes", &self.heap_bytes())
            .finish_non_exhaustive()
    }
}

impl<T: ?Sized + Send + Sync + 'static> Drop for Shared<T> {
    fn drop(&mut self) {
        // SAFETY: this owner retains the header through the atomic decrement.
        let header = unsafe { self.header.as_ref() };
        if header.references.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }
        std::sync::atomic::fence(Ordering::Acquire);
        let (layout, offset, drop_value) = (header.layout, header.offset, header.drop_value);
        // SAFETY: no other owner remains. Move Memory out before invoking the
        // original destructor so the block and domain are freed even on unwind.
        let memory = unsafe { std::ptr::read(std::ptr::addr_of!((*self.header.as_ptr()).memory)) };
        let allocation = Release {
            pointer: self.header.cast(),
            layout,
            memory,
        };
        // SAFETY: the original T is still initialized at its recorded offset;
        // its exact destructor is independent of this last owner's projected T.
        unsafe { drop_value(allocation.pointer.as_ptr().add(offset)) };
        drop(allocation);
    }
}
