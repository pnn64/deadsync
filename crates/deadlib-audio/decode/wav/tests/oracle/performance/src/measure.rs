// Validation-only allocation, CPU-cycle and Windows process-memory observers.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
pub struct Heap;
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(n: usize) {
    CALLS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(n, Relaxed);
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}
// SAFETY: all pointer/layout contracts are forwarded unchanged to System.
// The atomic observer never allocates, and tracking brackets complete operations.
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
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() && TRACK.load(Relaxed) {
            LIVE.fetch_sub(l.size(), Relaxed);
            allocated(n);
        }
        q
    }
}
#[cfg(windows)]
pub fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut core::ffi::c_void;
        fn QueryThreadCycleTime(t: *mut core::ffi::c_void, n: *mut u64) -> i32;
    }
    let mut n = 0;
    // SAFETY: the pseudo-handle is valid and n is writable during this call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut n) },
        0
    );
    n
}
#[cfg(not(windows))]
pub fn cycles() -> u64 {
    0
}
pub fn bench(name: &str, iters: usize, wire: usize, output: usize, mut work: impl FnMut()) {
    for _ in 0..(iters / 10).clamp(1, 10000) {
        work();
    }
    CALLS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    TRACK.store(true, Relaxed);
    work();
    TRACK.store(false, Relaxed);
    let calls = CALLS.load(Relaxed);
    let requested = REQUESTED.load(Relaxed);
    let peak = PEAK.load(Relaxed);
    assert_eq!(LIVE.load(Relaxed), 0);
    for trial in 0..3 {
        let start = Instant::now();
        let c = cycles();
        for _ in 0..iters {
            work();
        }
        let cpu = cycles() - c;
        let ns = start.elapsed().as_nanos() as f64 / iters as f64;
        println!(
            "{name}\t{trial}\t{iters}\t{ns:.3}\t{:.3}\t{:.3}\t{:.3}\t{calls}\t{requested}\t{peak}",
            cpu as f64 / iters as f64,
            wire as f64 / (1 << 20) as f64 * 1e9 / ns,
            output as f64 / (1 << 20) as f64 * 1e9 / ns
        );
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Memory {
    pub cb: u32,
    pub faults: u32,
    pub peak_working_set: usize,
    pub working_set: usize,
    pub peak_paged: usize,
    pub paged: usize,
    pub peak_nonpaged: usize,
    pub nonpaged: usize,
    pub pagefile: usize,
    pub peak_pagefile: usize,
    pub private_bytes: usize,
}
#[cfg(windows)]
pub fn memory() -> Memory {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(p: *mut core::ffi::c_void, m: *mut Memory, n: u32) -> i32;
    }
    let mut m = Memory {
        cb: std::mem::size_of::<Memory>() as u32,
        ..Memory::default()
    };
    // SAFETY: the process handle is valid and m has the advertised C layout/size.
    assert_ne!(
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut m,
                std::mem::size_of::<Memory>() as u32,
            )
        },
        0
    );
    m
}
#[cfg(not(windows))]
pub fn memory() -> Memory {
    Memory::default()
}
