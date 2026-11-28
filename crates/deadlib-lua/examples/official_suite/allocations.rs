use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Clone, Copy)]
pub(super) struct Counts {
    pub fresh: u64,
    pub resized: u64,
    pub growth: u64,
}

impl Counts {
    pub(super) fn read() -> Self {
        Self {
            fresh: ALLOCATOR.fresh.load(Relaxed),
            resized: ALLOCATOR.resized.load(Relaxed),
            growth: ALLOCATOR.growth.load(Relaxed),
        }
    }

    pub(super) fn since(self, before: Self) -> Self {
        Self {
            fresh: self.fresh.wrapping_sub(before.fresh),
            resized: self.resized.wrapping_sub(before.resized),
            growth: self.growth.wrapping_sub(before.growth),
        }
    }
}

struct CountingAllocator {
    fresh: AtomicU64,
    resized: AtomicU64,
    growth: AtomicU64,
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator {
    fresh: AtomicU64::new(0),
    resized: AtomicU64::new(0),
    growth: AtomicU64::new(0),
};

// SAFETY: System owns every block. The counters neither allocate nor change
// the requested layout, returned pointer, or lifetime of any allocation.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the caller's valid layout to the underlying allocator.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            self.fresh.fetch_add(1, Relaxed);
            self.growth.fetch_add(layout.size() as u64, Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the caller's valid layout to the underlying allocator.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            self.fresh.fetch_add(1, Relaxed);
            self.growth.fetch_add(layout.size() as u64, Relaxed);
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: Forward the live pointer, original layout, and valid new size.
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            self.resized.fetch_add(1, Relaxed);
            self.growth
                .fetch_add(size.saturating_sub(layout.size()) as u64, Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: This block was allocated by System with the supplied layout.
        unsafe { System.dealloc(pointer, layout) };
    }
}
