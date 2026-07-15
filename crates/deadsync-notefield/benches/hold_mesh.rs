use deadsync_notefield::{
    HoldMeshScratch, bench_fresh_hold_mesh_frame, bench_reused_hold_mesh_frame,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const HOLDS: usize = 4;
const BODY_VERTICES: usize = 1920;
const WARMUP_FRAMES: usize = 32;
const MEASURE_FRAMES: usize = 2000;

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

// SAFETY: every operation delegates to `System` with the original allocation
// layout and only observes successful calls through independent atomics.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees `ptr` and `layout` identify a live
        // allocation made by this allocator, which delegates to `System`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live
        // allocation; all allocation operations delegate to `System`.
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

struct BenchResult {
    elapsed: Duration,
    alloc: AllocSnapshot,
    checksum: usize,
}

fn main() {
    let texture: Arc<str> = Arc::from("hold-bench");
    let fresh = run_fresh(&texture);
    let reused = run_reused(&texture);
    assert_eq!(fresh.checksum, reused.checksum);

    println!("hold mesh ownership microbenchmark");
    println!("{HOLDS} visible holds, {BODY_VERTICES} vertices per body pass");
    print_result("fresh Vec -> Arc<[T]>", &fresh);
    print_result("reused Arc<Vec<T>>", &reused);
    println!(
        "speedup: {:.2}x, allocation reduction: {:.2}%",
        fresh.elapsed.as_secs_f64() / reused.elapsed.as_secs_f64(),
        100.0 * (1.0 - reused.alloc.allocs as f64 / fresh.alloc.allocs as f64),
    );
}

fn run_fresh(texture: &Arc<str>) -> BenchResult {
    let mut actors = Vec::with_capacity(HOLDS * 6);
    for _ in 0..WARMUP_FRAMES {
        black_box(bench_fresh_hold_mesh_frame(
            &mut actors,
            texture,
            HOLDS,
            BODY_VERTICES,
        ));
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(bench_fresh_hold_mesh_frame(
            &mut actors,
            texture,
            HOLDS,
            BODY_VERTICES,
        )));
    }
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn run_reused(texture: &Arc<str>) -> BenchResult {
    let mut actors = Vec::with_capacity(HOLDS * 6);
    let mut scratch = HoldMeshScratch::with_columns(HOLDS);
    for _ in 0..WARMUP_FRAMES {
        black_box(bench_reused_hold_mesh_frame(
            &mut actors,
            &mut scratch,
            texture,
            HOLDS,
            BODY_VERTICES,
        ));
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(bench_reused_hold_mesh_frame(
            &mut actors,
            &mut scratch,
            texture,
            HOLDS,
            BODY_VERTICES,
        )));
    }
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{name:<22} {:>9.1} us/frame  {:>8.1} allocs/frame  {:>9.1} KiB/frame  \
         {:>5.1} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.alloc.allocs as f64 / frames,
        result.alloc.bytes as f64 / frames / 1024.0,
        result.alloc.reallocs as f64 / frames,
    );
}
