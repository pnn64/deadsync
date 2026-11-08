use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
struct Heap;
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(bytes: usize) {
    CALLS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(bytes, Relaxed);
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}
// SAFETY: every operation forwards the unchanged allocation/layout contract to
// System. Counters do not allocate; all measured operations drop what they own.
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() && TRACK.load(Relaxed) {
            allocated(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() && TRACK.load(Relaxed) {
            allocated(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if TRACK.load(Relaxed) {
            LIVE.fetch_sub(l.size(), Relaxed);
        }
        unsafe {
            System.dealloc(p, l);
        }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let result = unsafe { System.realloc(p, l, n) };
        if !result.is_null() && TRACK.load(Relaxed) {
            LIVE.fetch_sub(l.size(), Relaxed);
            allocated(n);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Heap = Heap;

#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut core::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut core::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut n = 0;
    // SAFETY: valid pseudo-handle and writable u64 for this synchronous call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut n) },
        0
    );
    n
}
#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}

pub fn bench(name: &str, iters: usize, mut work: impl FnMut()) {
    work();
    CALLS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    TRACK.store(true, Relaxed);
    work();
    TRACK.store(false, Relaxed);
    assert_eq!(
        LIVE.load(Relaxed),
        0,
        "operation must drop its own allocations"
    );
    let calls = CALLS.load(Relaxed);
    let requested = REQUESTED.load(Relaxed);
    let peak = PEAK.load(Relaxed);
    for trial in 0..5 {
        let start = Instant::now();
        let cpu = cycles();
        for _ in 0..iters {
            work();
        }
        let cpu = cycles() - cpu;
        let elapsed = start.elapsed().as_nanos() as f64;
        println!(
            "{name}\t{trial}\t{iters}\t{:.3}\t{:.3}\t{calls}\t{requested}\t{peak}",
            elapsed / iters as f64,
            cpu as f64 / iters as f64
        );
    }
}
