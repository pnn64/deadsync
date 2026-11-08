// Thread-local allocation accounting for the stream analysis benchmarks.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Churn {
    pub allocs: usize,
    pub reallocs: usize,
    pub frees: usize,
    pub allocated_bytes: usize,
    pub freed_bytes: usize,
    pub peak_bytes: usize,
}

thread_local! {
    static COUNTS: Cell<Option<Churn>> = const { Cell::new(None) };
}

struct CountedSystem;

#[global_allocator]
static ALLOCATOR: CountedSystem = CountedSystem;

fn record(update: impl FnOnce(&mut Churn)) {
    // TLS may be unavailable during thread teardown. Counting itself never
    // allocates and each test records only its own thread's work.
    let _ = COUNTS.try_with(|counts| {
        if let Some(mut current) = counts.get() {
            update(&mut current);
            current.peak_bytes = current
                .peak_bytes
                .max(current.allocated_bytes.saturating_sub(current.freed_bytes));
            counts.set(Some(current));
        }
    });
}

// SAFETY: every operation delegates its unchanged pointer/layout to System;
// thread-local counters neither access allocations nor change their lifetimes.
unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(|counts| {
            counts.allocs += 1;
            counts.allocated_bytes += layout.size();
        });
        // SAFETY: GlobalAlloc's caller supplies a valid allocation layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(|counts| {
            counts.allocs += 1;
            counts.allocated_bytes += layout.size();
        });
        // SAFETY: GlobalAlloc's caller supplies a valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(|counts| {
            counts.frees += 1;
            counts.freed_bytes += layout.size();
        });
        // SAFETY: the caller supplies this System allocation's pointer/layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(|counts| {
            counts.reallocs += 1;
            counts.allocated_bytes += new_size;
            counts.freed_bytes += layout.size();
        });
        // SAFETY: the caller supplies a live allocation and valid new size.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

struct Tracking;

impl Tracking {
    fn start() -> Self {
        COUNTS.with(|counts| {
            assert!(counts.get().is_none(), "allocation tracking cannot nest");
            counts.set(Some(Churn::default()));
        });
        Self
    }
}

impl Drop for Tracking {
    fn drop(&mut self) {
        COUNTS.set(None);
    }
}

pub fn measure<T>(work: impl FnOnce() -> T) -> (T, Churn) {
    let tracking = Tracking::start();
    let result = work();
    let counts = COUNTS.get().expect("tracking is active");
    drop(tracking);
    (result, counts)
}
