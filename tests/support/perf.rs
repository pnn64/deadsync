//! Scoped allocation checks and manual benchmarks for existing hot-path tests.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Churn {
    allocs: usize,
    reallocs: usize,
    frees: usize,
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
            counts.set(Some(current));
        }
    });
}

// SAFETY: every operation delegates its unchanged pointer/layout to System;
// thread-local counters neither access allocations nor change their lifetimes.
unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(|counts| counts.allocs += 1);
        // SAFETY: GlobalAlloc's caller supplies a valid allocation layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(|counts| counts.allocs += 1);
        // SAFETY: GlobalAlloc's caller supplies a valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(|counts| counts.frees += 1);
        // SAFETY: the caller supplies this System allocation's pointer/layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(|counts| counts.reallocs += 1);
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

pub fn assert_no_churn(work: impl FnOnce()) {
    let tracking = Tracking::start();
    work();
    let counts = COUNTS.get().expect("tracking is active");
    drop(tracking);
    assert_eq!(counts, Churn::default(), "hot path allocated or freed memory");
}

pub fn measure<T>(name: &str, units: usize, mut work: impl FnMut() -> T) {
    const ITERATIONS: usize = 512;
    for _ in 0..64 {
        black_box(work());
    }
    let tracking = Tracking::start();
    let started = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(work());
    }
    let elapsed = started.elapsed();
    let counts = COUNTS.get().expect("tracking is active");
    drop(tracking);
    eprintln!(
        "{name}: {:.1} ns/unit, {counts:?} over {ITERATIONS} iterations",
        elapsed.as_nanos() as f64 / (ITERATIONS * units.max(1)) as f64,
    );
}
