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
    allocated_bytes: usize,
    freed_bytes: usize,
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

pub fn assert_no_churn(work: impl FnOnce()) {
    let tracking = Tracking::start();
    work();
    let counts = COUNTS.get().expect("tracking is active");
    drop(tracking);
    assert_eq!(
        counts,
        Churn::default(),
        "hot path allocated or freed memory"
    );
}

#[allow(dead_code)]
pub fn assert_churn_budget(max_allocations: usize, max_bytes: usize, work: impl FnOnce()) {
    let tracking = Tracking::start();
    work();
    let counts = COUNTS.get().expect("tracking is active");
    drop(tracking);
    assert!(
        counts.allocs <= max_allocations
            && counts.frees <= max_allocations
            && counts.reallocs == 0
            && counts.allocated_bytes <= max_bytes
            && counts.freed_bytes <= max_bytes,
        "allocation budget exceeded: {counts:?} (limit {max_allocations} calls, {max_bytes} bytes)"
    );
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

/// Reports seven timing samples separately from a single allocation-counted
/// operation. `units` is the number of useful items processed by one operation.
/// On Windows, cycle counts cover the calling thread (not elapsed TSC ticks).
#[allow(dead_code)]
pub fn measure_sampled<T>(
    name: &str,
    iterations: usize,
    units: usize,
    mut work: impl FnMut() -> T,
) {
    for _ in 0..3 {
        black_box(work());
    }
    let mut nanos = [0.0; 7];
    let mut cycles = [0.0; 7];
    for (ns, cpu) in nanos.iter_mut().zip(&mut cycles) {
        let first_cycles = thread_cycles();
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(work());
        }
        *ns = started.elapsed().as_nanos() as f64 / iterations as f64;
        *cpu = (thread_cycles() - first_cycles) as f64 / iterations as f64;
    }
    nanos.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);
    let tracking = Tracking::start();
    black_box(work());
    let counts = COUNTS.get().expect("tracking is active");
    drop(tracking);
    eprintln!(
        "{name}: median {:.1} ns/op (range {:.1}..{:.1}), {:.1} thread cycles/op, {:.1} units/s; {counts:?}/op",
        nanos[3],
        nanos[0],
        nanos[6],
        cycles[3],
        units as f64 * 1e9 / nanos[3],
    );
}

#[cfg(windows)]
fn thread_cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: this thread's pseudo-handle is valid and cycles is writable.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

#[cfg(not(windows))]
fn thread_cycles() -> u64 {
    0 // Unavailable on this platform; never interpreted as a measured improvement.
}
