use deadlib_present::compose::RenderSortBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const OBJECTS: usize = 1_024;
const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 20_000;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all calls delegate to `System`; atomics only observe successful
// allocations and never alter ownership or layout.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` came from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

fn main() {
    let legacy = measure(RenderSortBenchmark::sort_legacy_frame);
    let sparse = measure(RenderSortBenchmark::sort_frame);
    assert_eq!(legacy.checksum, sparse.checksum);
    black_box((legacy.checksum, sparse.checksum));

    println!("render-object sparse-z sort benchmark ({OBJECTS} objects)");
    print_result("legacy direct", &legacy);
    print_result("sparse buckets", &sparse);
}

struct BenchResult {
    elapsed: std::time::Duration,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(sort_frame: fn(&mut RenderSortBenchmark, usize) -> u64) -> BenchResult {
    let mut sort = RenderSortBenchmark::new(OBJECTS);
    for frame in 0..WARMUP_FRAMES {
        black_box(sort_frame(&mut sort, frame));
    }

    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0u64;
    for frame in 0..MEASURE_FRAMES {
        checksum ^= black_box(sort_frame(&mut sort, frame));
    }
    BenchResult {
        elapsed: started.elapsed(),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<14} {:>9.2} us/frame  {:>5.2} allocs/frame  {:>7.1} bytes/frame  \
         {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}
