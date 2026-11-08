//! Separate allocation probe; never use its timings as throughput evidence.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
#[path="decode-shared-probe.rs"]
mod common;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
struct Counting;
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocated(size: usize) {
    let live = CURRENT.fetch_add(size, Relaxed) + size;
    if ACTIVE.load(Relaxed) {
        CALLS.fetch_add(1, Relaxed);
        REQUESTED.fetch_add(size, Relaxed);
        PEAK.fetch_max(live, Relaxed);
    }
}

// SAFETY: allocation, alignment, ownership and failure behavior are delegated
// unchanged to System. Layouts supplied by the allocator API account for frees.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() { allocated(layout.size()); }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Relaxed);
        if ACTIVE.load(Relaxed) { FREES.fetch_add(1, Relaxed); }
        unsafe { System.dealloc(pointer, layout); }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, size) };
        if !new_pointer.is_null() {
            CURRENT.fetch_sub(layout.size(), Relaxed);
            allocated(size);
            if ACTIVE.load(Relaxed) { REALLOCS.fetch_add(1, Relaxed); }
        }
        new_pointer
    }
}

#[derive(Default)]
struct Probe { start: usize, records: Vec<(usize, usize, usize, usize, usize, usize, i128, usize)> }
impl common::Hooks for Probe {
    fn begin(&mut self) {
        self.start = CURRENT.load(Relaxed);
        PEAK.store(self.start, Relaxed);
        CALLS.store(0, Relaxed);
        REALLOCS.store(0, Relaxed);
        FREES.store(0, Relaxed);
        REQUESTED.store(0, Relaxed);
        ACTIVE.store(true, Relaxed);
    }
    fn end(&mut self, iteration: usize) {
        ACTIVE.store(false, Relaxed);
        self.records.push((iteration, CALLS.load(Relaxed), REALLOCS.load(Relaxed), FREES.load(Relaxed),
            REQUESTED.load(Relaxed), PEAK.load(Relaxed).saturating_sub(self.start),
            CURRENT.load(Relaxed) as i128 - self.start as i128, self.start));
    }
    fn finish(&mut self) {
        for &(i, calls, reallocs, frees, requested, peak, live, baseline) in &self.records {
            eprintln!("ALLOC\t{i}\t{calls}\t{reallocs}\t{frees}\t{requested}\t{peak}\t{live}\t{baseline}");
        }
    }
}
fn main() { common::run(&mut Probe::default()); }
