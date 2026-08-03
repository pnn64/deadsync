use deadsync_theme_simply_love::screens::evaluation::{
    benchmark_eval_life_line, benchmark_eval_life_line_legacy,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const HISTORY_POINTS: usize = 1_200;
const WARMUP_RUNS: usize = 100;
const MEASURE_RUNS: usize = 2_000;
const RECORD_START: f32 = 2.0;
const GRAPH_FIRST: f32 = 0.0;
const GRAPH_LAST: f32 = 182.0;
const GRAPH_WIDTH: f32 = 610.0;
const GRAPH_HEIGHT: f32 = 64.0;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
    live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
        }
    }

    fn reset_peak(&self) {
        self.peak_live_bytes
            .store(self.live_bytes.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    fn add_live(&self, bytes: u64) {
        let live = self.live_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        self.peak_live_bytes.fetch_max(live, Ordering::Relaxed);
    }
}

// SAFETY: all allocation operations delegate unchanged to `System`; relaxed
// atomics only observe successful operations and track their byte totals.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let bytes = layout.size() as u64;
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(bytes, Ordering::Relaxed);
            self.add_live(bytes);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        self.live_bytes
            .fetch_sub(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: this pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: all arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            let old_size = old.size() as u64;
            let new_size = new_size as u64;
            if new_size > old_size {
                let growth = new_size - old_size;
                self.bytes.fetch_add(growth, Ordering::Relaxed);
                self.add_live(growth);
            } else {
                self.live_bytes
                    .fetch_sub(old_size - new_size, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    bytes: u64,
    live_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
            live_bytes: self.live_bytes.saturating_sub(before.live_bytes),
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    peak_live_bytes: u64,
    checksum: usize,
}

fn life_history() -> Vec<(f32, f32)> {
    let duration = GRAPH_LAST - RECORD_START;
    (0..HISTORY_POINTS)
        .map(|index| {
            let t = RECORD_START + index as f32 / (HISTORY_POINTS - 1) as f32 * duration;
            let life = ((index * 37 + index / 11 * 17) % 101) as f32 / 100.0;
            (t, life)
        })
        .collect()
}

fn measure(mut build: impl FnMut() -> usize) -> BenchResult {
    for _ in 0..WARMUP_RUNS {
        black_box(build());
    }

    ALLOC.reset_peak();
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0usize;
    for run in 0..MEASURE_RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(build()) ^ run;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        peak_live_bytes: ALLOC
            .peak_live_bytes
            .load(Ordering::Relaxed)
            .saturating_sub(before.live_bytes),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = MEASURE_RUNS as f64;
    println!(
        "  {label:<8} {:>9.1} ns/build  {:>10.0} cycles/build  {:>9.0} builds/s",
        result.elapsed.as_secs_f64() * 1.0e9 / runs,
        result.cycles as f64 / runs,
        runs / result.elapsed.as_secs_f64(),
    );
    println!(
        "           alloc/realloc/dealloc={:.2}/{:.2}/{:.2}, {:>8.1} KiB allocated/build, {:>8.1} KiB peak live",
        result.alloc.allocs as f64 / runs,
        result.alloc.reallocs as f64 / runs,
        result.alloc.deallocs as f64 / runs,
        result.alloc.bytes as f64 / runs / 1024.0,
        result.peak_live_bytes as f64 / 1024.0,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    100.0 * (1.0 - new as f64 / old as f64)
}

fn main() {
    let history = life_history();
    let legacy = measure(|| {
        let actors = benchmark_eval_life_line_legacy(
            black_box(&history),
            GRAPH_FIRST,
            GRAPH_LAST,
            GRAPH_WIDTH,
            GRAPH_HEIGHT,
        );
        let len = actors.len();
        black_box(actors);
        len
    });
    let parity = measure(|| {
        let mesh = benchmark_eval_life_line(
            black_box(&history),
            RECORD_START,
            GRAPH_FIRST,
            GRAPH_LAST,
            GRAPH_WIDTH,
            GRAPH_HEIGHT,
        );
        let len = mesh.len();
        black_box(mesh);
        len
    });
    let cached_mesh = benchmark_eval_life_line(
        black_box(&history),
        RECORD_START,
        GRAPH_FIRST,
        GRAPH_LAST,
        GRAPH_WIDTH,
        GRAPH_HEIGHT,
    );
    let cached = measure(|| {
        let mesh = Arc::clone(&cached_mesh);
        let len = mesh.len();
        black_box(mesh);
        len
    });
    black_box((legacy.checksum, parity.checksum, cached.checksum));

    assert!(parity.alloc.bytes < legacy.alloc.bytes);
    assert!(parity.peak_live_bytes < legacy.peak_live_bytes);
    assert_eq!(cached.alloc.allocs, 0);
    assert_eq!(cached.alloc.reallocs, 0);
    assert_eq!(cached.alloc.bytes, 0);

    println!(
        "evaluation lifeline construction ({HISTORY_POINTS} raw changes -> 100 GraphDisplay samples)"
    );
    print_result("legacy", &legacy);
    print_result("parity", &parity);
    print_result("cached", &cached);
    println!("  cached = prebuilt mesh handle cloned while rebuilding the life-graph actor");
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocated-byte reduction {:.1}% | peak-live reduction {:.1}%",
        legacy.elapsed.as_secs_f64() / parity.elapsed.as_secs_f64(),
        reduction(legacy.cycles, parity.cycles),
        reduction(legacy.alloc.bytes, parity.alloc.bytes),
        reduction(legacy.peak_live_bytes, parity.peak_live_bytes),
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC serialize and read this thread's timestamp counter;
    // they do not access memory.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
