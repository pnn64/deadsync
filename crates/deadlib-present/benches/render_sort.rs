use deadlib_present::compose::RenderSortBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const OBJECTS: usize = 1_024;
const WARMUP_FRAMES: usize = 1_000;
const MEASURE_FRAMES: usize = 10_000;
const BENCH_RUNS: usize = 7;

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
    let legacy = median(RenderSortBenchmark::sort_legacy_frame);
    println!(
        "render-object sparse-z sort benchmark ({OBJECTS} objects, median of {BENCH_RUNS} runs)"
    );
    print_result("legacy direct", &legacy);
    compare(
        "generic sparse",
        RenderSortBenchmark::sort_frame,
        RenderSortBenchmark::sort_composed_frame,
    );

    println!(
        "\nrender-object dense-z sort benchmark ({OBJECTS} objects, median of {BENCH_RUNS} runs)"
    );
    compare(
        "generic dense",
        RenderSortBenchmark::sort_dense_frame,
        RenderSortBenchmark::sort_composed_dense_frame,
    );
}

fn compare(
    generic_label: &str,
    generic_plan: fn(&mut RenderSortBenchmark, usize) -> u64,
    composed_plan: fn(&mut RenderSortBenchmark, usize) -> u64,
) {
    let mut generic_runs = Vec::with_capacity(BENCH_RUNS);
    let mut composed_runs = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (generic, composed) = if run % 2 == 0 {
            let composed = measure(composed_plan);
            let generic = measure(generic_plan);
            (generic, composed)
        } else {
            let generic = measure(generic_plan);
            let composed = measure(composed_plan);
            (generic, composed)
        };
        assert_eq!(generic.checksum, composed.checksum);
        for result in [&generic, &composed] {
            assert_eq!(result.allocated.allocs, 0);
            assert_eq!(result.allocated.reallocs, 0);
            assert_eq!(result.allocated.bytes, 0);
        }
        generic_runs.push(generic);
        composed_runs.push(composed);
    }
    let generic = take_median(generic_runs);
    let composed = take_median(composed_runs);
    black_box((generic.checksum, composed.checksum));

    print_result(generic_label, &generic);
    print_result("composed order", &composed);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}%",
        generic.elapsed.as_secs_f64() / composed.elapsed.as_secs_f64(),
        100.0 * (1.0 - composed.cycles as f64 / generic.cycles as f64),
    );
}

struct BenchResult {
    elapsed: std::time::Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn median(sort_frame: fn(&mut RenderSortBenchmark, usize) -> u64) -> BenchResult {
    let runs = (0..BENCH_RUNS)
        .map(|_| measure(sort_frame))
        .collect::<Vec<_>>();
    take_median(runs)
}

fn take_median(mut runs: Vec<BenchResult>) -> BenchResult {
    runs.sort_unstable_by_key(|result| result.elapsed);
    runs.swap_remove(BENCH_RUNS / 2)
}

fn measure(sort_frame: fn(&mut RenderSortBenchmark, usize) -> u64) -> BenchResult {
    let mut sort = RenderSortBenchmark::new(OBJECTS);
    for frame in 0..WARMUP_FRAMES {
        black_box(sort_frame(&mut sort, frame));
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for frame in 0..MEASURE_FRAMES {
        checksum ^= black_box(sort_frame(&mut sort, frame));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<14} {:>9.2} us/frame  {:>9.0} cycles/frame  {:>7.1} M objects/s  \
         {:>5.2} allocs/frame  {:>7.1} bytes/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * OBJECTS as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter without
    // dereferencing memory.
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
