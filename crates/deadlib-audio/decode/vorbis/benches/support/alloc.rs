//! Benchmark-only requested-size accounting; never linked into production.
use std::alloc::Layout;
#[cfg(feature = "bench-alloc")]
use std::alloc::{GlobalAlloc, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

#[cfg(feature = "bench-alloc")]
pub struct Counting;
static LIVE: AtomicU64 = AtomicU64::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static BASE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
static OPERATIONS: AtomicU64 = AtomicU64::new(0);
static REQUESTED: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "bench-alloc")]
fn added(bytes: usize, previous: usize) {
    let after = if bytes >= previous {
        LIVE.fetch_add((bytes - previous) as u64, Relaxed) + (bytes - previous) as u64
    } else {
        LIVE.fetch_sub((previous - bytes) as u64, Relaxed) - (previous - bytes) as u64
    };
    if ACTIVE.load(Relaxed) {
        OPERATIONS.fetch_add(1, Relaxed);
        REQUESTED.fetch_add(bytes as u64, Relaxed);
        PEAK.fetch_max(after, Relaxed);
    }
}
// Forwarding preserves the original pointer/layout/alignment contract. Only
// successful allocations change accounting; failed realloc keeps the old live
// allocation. Global live sizes are tracked outside windows as well, so freeing
// a preexisting buffer during a seek cannot underflow a window-local counter.
#[cfg(feature = "bench-alloc")]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            added(layout.size(), 0);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            added(layout.size(), 0);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(p, layout);
        }
        LIVE.fetch_sub(layout.size() as u64, Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(p, layout, size) };
        if !next.is_null() {
            added(size, layout.size());
        }
        next
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub operations: u64,
    pub requested_bytes: u64,
    pub peak_extra_bytes: u64,
    pub retained_delta_bytes: i128,
}
pub fn begin() {
    assert!(!ACTIVE.load(Relaxed));
    let live = LIVE.load(Relaxed);
    BASE.store(live, Relaxed);
    PEAK.store(live, Relaxed);
    OPERATIONS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    ACTIVE.store(true, Relaxed);
}
pub fn end() -> Stats {
    assert!(ACTIVE.swap(false, Relaxed));
    Stats {
        operations: OPERATIONS.load(Relaxed),
        requested_bytes: REQUESTED.load(Relaxed),
        peak_extra_bytes: PEAK.load(Relaxed).saturating_sub(BASE.load(Relaxed)),
        retained_delta_bytes: LIVE.load(Relaxed) as i128 - BASE.load(Relaxed) as i128,
    }
}
pub fn self_test() {
    begin();
    let small = Layout::from_size_align(17, 8).unwrap();
    // Exercise the actual global allocator, including zero-fill and realloc.
    unsafe {
        let p = std::alloc::alloc_zeroed(small);
        assert!(!p.is_null());
        assert!((0..17).all(|i| *p.add(i) == 0));
        let p = std::alloc::realloc(p, small, 41);
        assert!(!p.is_null());
        std::hint::black_box(p);
        std::alloc::dealloc(p, Layout::from_size_align(41, 8).unwrap());
    }
    let s = end();
    assert_eq!(
        (
            s.operations,
            s.requested_bytes,
            s.peak_extra_bytes,
            s.retained_delta_bytes
        ),
        (2, 58, 41, 0)
    );
}
