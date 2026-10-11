//! Scoped allocation checks for production hot-path regression tests.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Churn {
    pub allocs: usize,
    pub reallocs: usize,
    pub frees: usize,
    pub allocated_bytes: usize,
    pub freed_bytes: usize,
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

pub fn measure<R>(work: impl FnOnce() -> R) -> (R, Churn) {
    let tracking = Tracking::start();
    let result = work();
    let counts = COUNTS.get().expect("tracking is active");
    drop(tracking);
    (result, counts)
}

pub fn compare(label: &str, mut original: impl FnMut(), mut current: impl FnMut()) {
    let mut elapsed = [Vec::with_capacity(9), Vec::with_capacity(9)];
    let mut counts = [1usize; 2];
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            let count = counts[variant];
            let start = std::time::Instant::now();
            for _ in 0..count {
                if variant == 0 {
                    original();
                } else {
                    current();
                }
            }
            let nanos = start.elapsed().as_nanos().max(1);
            if sample == 0 {
                counts[variant] = 5_000_000_u128.div_ceil(nanos).clamp(1, 1_000_000) as usize;
            } else {
                elapsed[variant].push(nanos as f64 / count as f64);
            }
        }
    }
    for samples in &mut elapsed {
        samples.sort_by(f64::total_cmp);
    }
    println!(
        "{label}: original {:.2} ns/op, current {:.2} ns/op, {:.3}x throughput",
        elapsed[0][4],
        elapsed[1][4],
        elapsed[0][4] / elapsed[1][4]
    );
    let (_, before) = measure(&mut original);
    let (_, after) = measure(&mut current);
    println!(
        "{label}: allocations {} -> {}, reallocations {} -> {}, bytes {} -> {}",
        before.allocs,
        after.allocs,
        before.reallocs,
        after.reallocs,
        before.allocated_bytes,
        after.allocated_bytes
    );
}

pub struct Directory(pub std::path::PathBuf);
impl Directory {
    pub fn new(label: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().canonicalize().unwrap();
        let path = base.join(format!(
            "deadsync-resource-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert_eq!(path.parent(), Some(base.as_path()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
