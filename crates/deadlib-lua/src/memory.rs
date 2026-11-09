//! State-owned allocations. No global allocator replacement or ambient owner.
//!
//! Every buffer retains its allocation domain. Replacing a callback changes the
//! allocator used to resize/free existing blocks, as lua_setallocf requires.
//! The callback and userdata must therefore accept every outstanding block.
use crate::Error;
use std::{
    alloc::Layout,
    ffi::c_void,
    ptr::NonNull,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
};

mod buffer;
pub(crate) use buffer::Buffer;
pub(crate) mod bytes;
mod shared;
pub(crate) use shared::Shared;
pub(crate) mod compilation;
pub(crate) mod fallible;

pub(crate) type Callback =
    unsafe extern "C" fn(*mut c_void, *mut c_void, usize, usize) -> *mut c_void;

/// Alignment required by Lua 5.1's pointer/double object representation.
const ALIGNMENT: usize = if align_of::<usize>() > align_of::<f64>() {
    align_of::<usize>()
} else {
    align_of::<f64>()
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Allocator {
    pub callback: Callback,
    pub userdata: *mut c_void,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    pub bytes: usize,
    pub blocks: usize,
}

struct Owner {
    references: AtomicUsize,
    state: Mutex<State>,
}
struct State {
    allocator: Option<Allocator>,
    usage: Usage,
    limit: Option<usize>,
    peak: usize,
}
const _: () = assert!(align_of::<Owner>() <= ALIGNMENT);

/// One serialized allocation domain. A default domain delegates to Rust's global allocator
/// and needs no control allocation. Custom control storage uses its own callback.
#[derive(Debug, Default)]
pub(crate) struct Memory(Option<NonNull<Owner>>);

/// A charge for immutable storage shared with an external owner. Releasing the
/// last VM binding releases its charge even if another VM still shares storage.
#[derive(Debug, Default)]
pub(crate) struct Reservation {
    memory: Memory,
    bytes: usize,
}
impl Reservation {
    fn grow(&mut self, bytes: usize) -> Result<(), Error> {
        let owner = self.memory.owner().expect("bounded reservation");
        let mut state = owner.lock();
        let total = state
            .usage
            .bytes
            .checked_add(bytes)
            .ok_or_else(Error::memory)?;
        if state.limit.is_some_and(|limit| total > limit) {
            return Err(Error::memory());
        }
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(Error::memory)?;
        state.usage.bytes = total;
        state.peak = state.peak.max(total);
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.bytes != 0 {
            self.memory
                .owner()
                .expect("reserved domain")
                .lock()
                .usage
                .bytes -= self.bytes;
        }
    }
}
// SAFETY: references are atomic and all callback/data/accounting accesses are
// serialized by the owner's mutex. custom() requires movable userdata; an
// embedding caller must not move thread-affine C allocator data between threads.
unsafe impl Send for Memory {}
unsafe impl Sync for Memory {}

/// Release raw storage after its values have been destroyed, even on unwind.
struct Release {
    pointer: NonNull<u8>,
    layout: Layout,
    memory: Memory,
}
impl Drop for Release {
    fn drop(&mut self) {
        let empty = Layout::from_size_align(0, self.layout.align()).expect("owned alignment");
        // SAFETY: this guard owns the exact allocation and its original Layout.
        // Its caller has destroyed all values before the guard can run.
        unsafe {
            self.memory
                .resize(self.pointer.as_ptr(), self.layout, empty)
        }
        .expect("free owned allocation");
    }
}

impl Memory {
    /// A Rust-owned allocation domain with a strict byte ceiling. Unlike the
    /// internal callback allocator, it supports each allocation's full alignment.
    pub(crate) fn bounded(limit: usize) -> Result<Self, Error> {
        let layout = Layout::new::<Owner>();
        if layout.size() > limit {
            return Err(Error::memory());
        }
        // SAFETY: the global allocator returns suitably aligned uninitialized
        // storage for Owner; the final domain reference releases this layout.
        let pointer = unsafe { std::alloc::alloc(layout) };
        let owner = NonNull::new(pointer.cast::<Owner>()).ok_or_else(Error::memory)?;
        // SAFETY: the successful allocation has no initialized fields yet.
        unsafe {
            owner.as_ptr().write(Owner {
                references: AtomicUsize::new(1),
                state: Mutex::new(State {
                    allocator: None,
                    usage: Usage {
                        bytes: layout.size(),
                        blocks: 1,
                    },
                    limit: Some(limit),
                    peak: layout.size(),
                }),
            });
        }
        Ok(Self(Some(owner)))
    }

    pub(crate) fn budget_usage(&self) -> Option<(usize, usize, usize)> {
        let state = self.owner()?.lock();
        state
            .limit
            .map(|limit| (state.usage.bytes, state.peak, limit))
    }

    pub(crate) fn reserve_external(
        &self,
        bytes: impl FnOnce() -> usize,
    ) -> Result<Reservation, Error> {
        let Some(owner) = self.owner() else {
            return Ok(Reservation::default());
        };
        let mut state = owner.lock();
        let Some(limit) = state.limit else {
            return Ok(Reservation::default());
        };
        let bytes = bytes();
        let total = state
            .usage
            .bytes
            .checked_add(bytes)
            .ok_or_else(Error::memory)?;
        if total > limit {
            return Err(Error::memory());
        }
        state.usage.bytes = total;
        state.peak = state.peak.max(total);
        Ok(Reservation {
            memory: self.clone(),
            bytes,
        })
    }

    /// # Safety
    /// The callback obeys lua_Alloc's realloc/free contract, returns storage
    /// aligned for pointers/doubles, never unwinds, and keeps userdata valid
    /// through the last owner/buffer drop. Null on failed growth leaves the old
    /// block intact. No live block may be freed outside this domain. Callbacks
    /// support serialized calls on any thread using the domain and must not
    /// reenter this same owner while its allocation is in progress.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "C allocator entry points await complete storage migration"
        )
    )]
    pub(crate) unsafe fn custom(allocator: Allocator) -> Result<Self, Error> {
        // SAFETY: the caller supplies the allocation contract above.
        let pointer = unsafe {
            (allocator.callback)(
                allocator.userdata,
                std::ptr::null_mut(),
                0,
                size_of::<Owner>(),
            )
        };
        let owner = NonNull::new(pointer.cast::<Owner>()).ok_or_else(Error::memory)?;
        // SAFETY: successful allocation owns enough aligned, uninitialized storage.
        unsafe {
            owner.as_ptr().write(Owner {
                references: AtomicUsize::new(1),
                state: Mutex::new(State {
                    allocator: Some(allocator),
                    usage: Usage {
                        bytes: size_of::<Owner>(),
                        blocks: 1,
                    },
                    limit: None,
                    peak: size_of::<Owner>(),
                }),
            });
        }
        Ok(Self(Some(owner)))
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "C allocator entry points await complete storage migration"
        )
    )]
    pub(crate) fn allocator(&self) -> Option<Allocator> {
        self.owner().and_then(|owner| owner.lock().allocator)
    }

    /// # Safety
    /// The replacement obeys custom()'s contract and can resize/free every
    /// existing block, including the original control allocation.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "C allocator entry points await complete storage migration"
        )
    )]
    pub(crate) unsafe fn replace(&self, allocator: Allocator) {
        self.owner()
            .expect("custom allocation domain")
            .lock()
            .allocator = Some(allocator);
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "full accounting awaits complete storage migration"
        )
    )]
    pub(crate) fn usage(&self) -> Option<Usage> {
        self.owner().map(|owner| owner.lock().usage)
    }

    fn owner(&self) -> Option<&Owner> {
        // SAFETY: each Memory retains one reference to the live control block.
        self.0.map(|pointer| unsafe { pointer.as_ref() })
    }

    /// Layouts use one stable alignment; zero sizes denote absent/freed blocks.
    /// The caller retains the old block/layout if growth fails.
    unsafe fn resize(&self, pointer: *mut u8, old: Layout, new: Layout) -> Result<*mut u8, Error> {
        assert_eq!(old.align(), new.align());
        if old.size() == 0 && new.size() == 0 {
            return Ok(std::ptr::null_mut());
        }
        if let Some(owner) = self.owner() {
            let mut state = owner.lock();
            let allocator = state.allocator;
            if allocator.is_some() && new.align() > ALIGNMENT {
                return Err(Error::memory());
            }
            let bytes = state
                .usage
                .bytes
                .checked_sub(old.size())
                .and_then(|bytes| bytes.checked_add(new.size()))
                .ok_or_else(Error::memory)?;
            if new.size() > old.size() && state.limit.is_some_and(|limit| bytes > limit) {
                return Err(Error::memory());
            }
            // SAFETY: the buffer owns this block and supplies its exact prior
            // request size. The domain contract also covers allocator replacement.
            let result = if let Some(allocator) = allocator {
                unsafe {
                    (allocator.callback)(allocator.userdata, pointer.cast(), old.size(), new.size())
                }
                .cast::<u8>()
            } else {
                // SAFETY: this domain allocated the original layout using the
                // same Rust allocator, and resizing preserves live contents.
                unsafe { rust_resize(pointer, old, new) }
            };
            if result.is_null() && new.size() != 0 {
                return Err(Error::memory());
            }
            let usage = state.usage;
            state.usage = Usage {
                bytes,
                blocks: usage.blocks - usize::from(old.size() != 0) + usize::from(new.size() != 0),
            };
            state.peak = state.peak.max(bytes);
            return Ok(result);
        }
        // Only compiler workspace activation charges unowned Rust storage.
        // Ordinary default domains and frees retain their previous behavior.
        compilation::charge(new.size())?;
        // SAFETY: the same standard allocator and original Layout own pointer;
        // the buffer updates its pointer/capacity only after a successful resize.
        let result = unsafe { rust_resize(pointer, old, new) };
        if result.is_null() && new.size() != 0 {
            Err(Error::memory())
        } else {
            Ok(result)
        }
    }
}

/// The caller owns pointer with exactly old, or supplies a zero-sized old for a
/// fresh allocation. A zero-sized new releases the old block.
unsafe fn rust_resize(pointer: *mut u8, old: Layout, new: Layout) -> *mut u8 {
    // SAFETY: the caller's allocation contract supplies the original layout;
    // all callers retain alignment across resizing and skip two empty layouts.
    unsafe {
        if old.size() == 0 {
            std::alloc::alloc(new)
        } else if new.size() == 0 {
            std::alloc::dealloc(pointer, old);
            std::ptr::null_mut()
        } else {
            std::alloc::realloc(pointer, old, new.size())
        }
    }
}

impl Clone for Memory {
    fn clone(&self) -> Self {
        if let Some(owner) = self.owner()
            && owner.references.fetch_add(1, Ordering::Relaxed) >= isize::MAX as usize
        {
            std::process::abort();
        }
        Self(self.0)
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        let Some(pointer) = self.0 else { return };
        // SAFETY: this reference keeps the control block alive until decrement.
        let owner = unsafe { pointer.as_ref() };
        if owner.references.fetch_sub(1, Ordering::Release) == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
            let state = owner.lock();
            debug_assert_eq!(
                state.usage,
                Usage {
                    bytes: size_of::<Owner>(),
                    blocks: 1
                }
            );
            let allocator = state.allocator;
            drop(state);
            // SAFETY: there are no references or lock guards left. Destroy the
            // mutex before handing its backing control block to the callback.
            unsafe {
                std::ptr::drop_in_place(pointer.as_ptr());
            }
            // SAFETY: no buffer or Memory reference remains. Owner has no
            // live fields; its final callback owns this exact block.
            unsafe {
                if let Some(allocator) = allocator {
                    (allocator.callback)(
                        allocator.userdata,
                        pointer.as_ptr().cast(),
                        size_of::<Owner>(),
                        0,
                    );
                } else {
                    std::alloc::dealloc(pointer.as_ptr().cast(), Layout::new::<Owner>());
                }
            }
        }
    }
}

impl Owner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
pub(crate) mod tests;
