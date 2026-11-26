//! Row-granular publication of picture planes shared between threads.
//!
//! A reference picture can be read by other pictures' decoders while its own
//! decoder is still producing it. The writer fills each plane front to back
//! (rows are raster order, so finished rows form a prefix) and publishes how
//! many macroblock rows are final; readers wait until the rows they need are
//! published and only ever look at the published prefix.
//!
//! All unsafe code for this lives in [`SharedPlane`]. Its contract:
//!
//! - There is exactly one writer per plane (`PictureWriter` in `dpb.rs`,
//!   which is not `Clone`), and it only writes bytes that are not yet
//!   published.
//! - Readers only create `&[u8]` over the published prefix, so no shared
//!   reference ever covers a byte that is still being written.
//! - Publication is a `Release` store of the row count after the writes and
//!   readers `Acquire`-load it before reading, so the writes happen-before
//!   the reads.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

/// A byte buffer written once, front to back, by a single writer while other
/// threads read its published prefix. See the module docs for the contract.
pub(crate) struct SharedPlane {
    data: Box<UnsafeCell<[u8]>>,
}

// SAFETY: concurrent access is limited to reads of the published prefix and
// writes (by the single writer) to the unpublished suffix; these never
// overlap and are ordered by `RowProgress`'s Release/Acquire pair.
unsafe impl Sync for SharedPlane {}

impl SharedPlane {
    pub(crate) fn from_vec(v: Vec<u8>) -> Self {
        let raw: *mut [u8] = Box::into_raw(v.into_boxed_slice());
        // SAFETY: `UnsafeCell<[u8]>` is `repr(transparent)` over `[u8]`, so
        // the allocation and slice metadata are valid for the new type.
        let data = unsafe { Box::from_raw(raw as *mut UnsafeCell<[u8]>) };
        Self { data }
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.data.get().len()
    }

    /// The first `len` bytes.
    ///
    /// # Safety
    /// No byte in `0..len` may be written while the returned slice is alive:
    /// `len` must not exceed the published length (or the caller must be the
    /// unique writer with no outstanding readers).
    #[inline]
    pub(crate) unsafe fn prefix(&self, len: usize) -> &[u8] {
        debug_assert!(len <= self.len());
        core::slice::from_raw_parts(self.data.get() as *const u8, len)
    }

    /// Copy `src` to `at..at + src.len()`.
    ///
    /// # Safety
    /// Caller is the unique writer and the range is unpublished and in bounds.
    #[inline]
    pub(crate) unsafe fn write(&self, at: usize, src: &[u8]) {
        debug_assert!(at + src.len() <= self.len());
        core::ptr::copy_nonoverlapping(
            src.as_ptr(),
            (self.data.get() as *mut u8).add(at),
            src.len(),
        );
    }

    /// Fill `at..at + len` with `value`.
    ///
    /// # Safety
    /// Same as [`write`](Self::write).
    #[inline]
    pub(crate) unsafe fn fill(&self, at: usize, len: usize, value: u8) {
        debug_assert!(at + len <= self.len());
        core::ptr::write_bytes((self.data.get() as *mut u8).add(at), value, len);
    }
}

/// Number of final macroblock rows of a picture, with blocking waits.
pub(crate) struct RowProgress {
    rows: AtomicU32,
    total: u32,
    #[cfg(feature = "std")]
    lock: std::sync::Mutex<()>,
    #[cfg(feature = "std")]
    cv: std::sync::Condvar,
}

impl RowProgress {
    fn with_rows(rows: u32, total: u32) -> Self {
        Self {
            rows: AtomicU32::new(rows),
            total,
            #[cfg(feature = "std")]
            lock: std::sync::Mutex::new(()),
            #[cfg(feature = "std")]
            cv: std::sync::Condvar::new(),
        }
    }

    /// Progress of a picture that is already fully decoded.
    pub(crate) fn complete(total: u32) -> Self {
        Self::with_rows(total, total)
    }

    /// Progress of a picture whose decoding has not started.
    #[cfg_attr(not(feature = "std"), allow(dead_code))]
    pub(crate) fn start(total: u32) -> Self {
        Self::with_rows(0, total)
    }

    #[inline]
    pub(crate) fn total(&self) -> u32 {
        self.total
    }

    /// Rows published so far (Acquire: published bytes are visible).
    #[inline]
    pub(crate) fn rows(&self) -> u32 {
        self.rows.load(Ordering::Acquire)
    }

    /// Block until at least `need` rows (capped at the total) are published.
    #[inline]
    pub(crate) fn wait(&self, need: u32) -> u32 {
        let need = need.min(self.total);
        let rows = self.rows();
        if rows >= need {
            return rows;
        }
        self.wait_slow(need)
    }

    #[cold]
    fn wait_slow(&self, need: u32) -> u32 {
        #[cfg(feature = "std")]
        {
            let mut guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                let rows = self.rows();
                if rows >= need {
                    return rows;
                }
                guard = self.cv.wait(guard).unwrap_or_else(|e| e.into_inner());
            }
        }
        #[cfg(not(feature = "std"))]
        loop {
            // Without std there are no decoding threads, so every picture is
            // complete at creation and this is unreachable in practice.
            let rows = self.rows();
            if rows >= need {
                return rows;
            }
            core::hint::spin_loop();
        }
    }

    /// Publish `rows` final rows (Release) and wake waiters. The caller must
    /// have finished writing everything those rows cover.
    #[cfg_attr(not(feature = "std"), allow(dead_code))]
    pub(crate) fn publish(&self, rows: u32) {
        self.rows.store(rows.min(self.total), Ordering::Release);
        #[cfg(feature = "std")]
        {
            // Taking the lock orders this notify after any waiter's check,
            // so a waiter cannot miss the wakeup.
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            self.cv.notify_all();
        }
    }
}
