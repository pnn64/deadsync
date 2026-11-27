//! Stable slots for small, terminated Lua strings.
use super::{Buffer, Memory, Shared};
use crate::Error;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, Ordering};

const SLOT_BYTES: usize = 16;
const SLOTS: usize = u64::BITS as usize;

struct Page {
    occupied: AtomicU64,
    slots: [UnsafeCell<[u8; SLOT_BYTES]>; SLOTS],
}

// SAFETY: a successful claim gives exclusive write access to a slot. A live
// ShortString retains that slot and exposes only immutable bytes. Its final
// drop releases the slot before another claimant can write it.
unsafe impl Sync for Page {}

impl Page {
    fn new() -> Self {
        Self {
            occupied: AtomicU64::new(0),
            slots: [const { UnsafeCell::new([0; SLOT_BYTES]) }; SLOTS],
        }
    }

    fn try_copy(&self, bytes: &[u8]) -> Option<usize> {
        let mut occupied = self.occupied.load(Ordering::Relaxed);
        while occupied != u64::MAX {
            let slot = (!occupied).trailing_zeros() as usize;
            match self.occupied.compare_exchange_weak(
                occupied,
                occupied | (1 << slot),
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // SAFETY: this claim owns the slot and no handle has been
                    // published. Other writers can only claim disjoint slots.
                    let output = unsafe { &mut *self.slots[slot].get() };
                    output[..bytes.len()].copy_from_slice(bytes);
                    output[bytes.len()] = 0;
                    return Some(slot);
                }
                Err(actual) => occupied = actual,
            }
        }
        None
    }
}

#[derive(Debug)]
pub(crate) struct ShortString {
    page: Shared<Page>,
    slot: u8,
    len: u8,
}

impl ShortString {
    fn copy(page: &Shared<Page>, bytes: &[u8]) -> Option<Self> {
        Some(Self {
            slot: page.try_copy(bytes)? as u8,
            page: page.clone(),
            len: (bytes.len() + 1) as u8,
        })
    }

    pub(crate) fn c_bytes(&self) -> &[u8] {
        // SAFETY: this owning handle still owns an initialized immutable slot.
        let bytes = unsafe { &*self.page.slots[usize::from(self.slot)].get() };
        &bytes[..usize::from(self.len)]
    }
}

impl Drop for ShortString {
    fn drop(&mut self) {
        self.page
            .occupied
            .fetch_and(!(1u64 << self.slot), Ordering::Release);
    }
}

#[derive(Debug, Default)]
pub(crate) struct ShortStrings {
    pages: Buffer<Shared<Page>>,
    first_available: usize,
    page_bytes: usize,
}

impl ShortStrings {
    pub(crate) fn new_in(memory: Memory) -> Self {
        Self {
            pages: Buffer::new_in(memory),
            first_available: 0,
            page_bytes: 0,
        }
    }

    pub(crate) fn fits(bytes: &[u8]) -> bool {
        bytes.len() < SLOT_BYTES
    }

    /// Return an owned slot and the newly allocated bytes to charge to the VM.
    pub(crate) fn insert(&mut self, bytes: &[u8]) -> Result<(ShortString, usize), Error> {
        assert!(Self::fits(bytes));
        while let Some(page) = self.pages.get(self.first_available) {
            if let Some(string) = ShortString::copy(page, bytes) {
                return Ok((string, 0));
            }
            self.first_available += 1;
        }
        let page = Shared::try_new_in(Page::new(), self.pages.memory().clone())?;
        let before = self.pages.capacity();
        self.pages.try_reserve(1)?;
        let allocated = page.heap_bytes();
        self.page_bytes += allocated;
        self.pages.push(page);
        let growth = allocated + (self.pages.capacity() - before) * size_of::<Shared<Page>>();
        let string = ShortString::copy(&self.pages[self.first_available], bytes)
            .expect("a new string page has free slots");
        Ok((string, growth))
    }

    pub(crate) fn sweep(&mut self) {
        let retained = &mut self.page_bytes;
        self.pages.retain(|page| {
            if page.is_sole_owner() {
                *retained -= page.heap_bytes();
                false
            } else {
                true
            }
        });
        // Some older pages may now have reusable slots.
        self.first_available = 0;
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.page_bytes + self.pages.capacity() * size_of::<Shared<Page>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_short_strings_keep_their_addresses_across_growth_and_reuse() {
        let mut pool = ShortStrings::default();
        let (kept, growth) = pool.insert(b"a\0b").unwrap();
        assert!(growth >= SLOT_BYTES * SLOTS);
        let address = kept.c_bytes().as_ptr();
        let filled: Vec<_> = (0..SLOTS * 3)
            .map(|_| pool.insert(b"next").unwrap().0)
            .collect();
        assert_eq!(kept.c_bytes(), b"a\0b\0");
        assert_eq!(kept.c_bytes().as_ptr(), address);
        let peak = pool.heap_bytes();
        drop(filled);
        pool.sweep();
        assert!(pool.heap_bytes() < peak);
        let (replacement, growth) = pool.insert(b"reused").unwrap();
        assert_eq!(growth, 0);
        assert_eq!(replacement.c_bytes(), b"reused\0");
        assert_eq!(kept.c_bytes().as_ptr(), address);
        assert_eq!(kept.c_bytes(), b"a\0b\0");
    }
}
