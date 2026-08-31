use deadsync_theme_simply_love::screens::components::evaluation::event_progress::{
    benchmark_event_layout_current, benchmark_event_layout_reference, benchmark_event_wrap_current,
    benchmark_event_wrap_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const REPEATS: usize = 512;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe this single-threaded benchmark while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let output = unsafe { System.realloc(pointer, old, new_size) };
        if !output.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(old.size() as u64, Ordering::Relaxed);
        }
        output
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn checksum(text: &str, zoom_bits: u32) -> u64 {
    text.bytes().fold(
        0xcbf2_9ce4_8422_2325 ^ u64::from(zoom_bits),
        |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3),
    )
}

fn run_fixture(repeats: usize, operation: &mut impl FnMut() -> (String, u32)) -> u64 {
    let mut result = 0u64;
    for _ in 0..repeats {
        let (text, zoom_bits) = operation();
        result = result
            .rotate_left(7)
            .wrapping_add(checksum(black_box(text.as_str()), zoom_bits));
    }
    result
}

fn run_timed(operation: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(operation());
    let elapsed_ns = started.elapsed().as_secs_f64() * 1_000_000_000.0;
    let elapsed_cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64);
    (elapsed_ns, elapsed_cycles, checksum)
}

fn measure_pair(
    mut old_operation: impl FnMut() -> u64,
    mut new_operation: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..4 {
        black_box(old_operation());
        black_box(new_operation());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0u64;
    let mut new_checksum = 0u64;
    for sample in 0..SAMPLES {
        let mut record_old = || {
            let (elapsed, cycles, checksum) = run_timed(&mut old_operation);
            old_times.push(elapsed);
            old_cycles.extend(cycles);
            old_checksum ^= checksum;
        };
        let mut record_new = || {
            let (elapsed, cycles, checksum) = run_timed(&mut new_operation);
            new_times.push(elapsed);
            new_cycles.extend(cycles);
            new_checksum ^= checksum;
        };
        if sample % 2 == 0 {
            record_old();
            record_new();
        } else {
            record_new();
            record_old();
        }
    }

    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    old_cycles.sort_by(f64::total_cmp);
    new_cycles.sort_by(f64::total_cmp);
    let old_allocated = measure_allocations(&mut old_operation);
    let new_allocated = measure_allocations(&mut new_operation);
    let row = |times: Vec<f64>, cycles: Vec<f64>, allocated, checksum| BenchResult {
        median_ns: percentile(&times, 50),
        p95_ns: percentile(&times, 95),
        median_cycles: (!cycles.is_empty()).then(|| percentile(&cycles, 50)),
        allocated,
        checksum,
    };
    (
        row(old_times, old_cycles, old_allocated, old_checksum),
        row(new_times, new_cycles, new_allocated, new_checksum),
    )
}

fn measure_allocations(operation: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn reduction(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn print_result(label: &str, repeats: usize, result: &BenchResult) {
    println!(
        "{label:<4} {:>10.1} ns median  {:>10.1} ns p95  {:>10.1} cycles  \
         {:>8.2} Mops/s  {:>7} alloc  {:>5} realloc  {:>7} free  \
         {:>10} B alloc  {:>10} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        repeats as f64 * 1_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
}

fn bench_case(
    title: &str,
    repeats: usize,
    old_operation: impl FnMut() -> (String, u32),
    new_operation: impl FnMut() -> (String, u32),
) {
    let mut old_operation = old_operation;
    let mut new_operation = new_operation;
    let (old, new) = measure_pair(
        || run_fixture(repeats, &mut old_operation),
        || run_fixture(repeats, &mut new_operation),
    );
    assert_eq!(old.checksum, new.checksum, "{title}: behavior diverged");

    println!("\n{title} ({repeats} layouts/sample)");
    print_result("old", repeats, &old);
    print_result("new", repeats, &new);
    println!(
        "gain {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% heap calls  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        reduction(old.median_ns, new.median_ns),
        reduction(old.p95_ns, new.p95_ns),
        reduction(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        reduction(old.allocated.calls() as f64, new.allocated.calls() as f64),
        reduction(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        reduction(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert!(new.median_ns < old.median_ns, "{title}: median regressed");
    assert!(new.p95_ns < old.p95_ns, "{title}: p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{title}: cycles regressed");
    }
    assert_eq!(
        new.allocated.allocs, repeats as u64,
        "{title}: optimized allocation count changed"
    );
    assert_eq!(new.allocated.reallocs, 0, "{title}: optimized path grew");
    assert!(
        new.allocated.calls() < old.allocated.calls(),
        "{title}: heap calls did not improve"
    );
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{title}: allocated bytes did not improve"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title}: churn did not improve"
    );
}

fn main() {
    const PARAGRAPH: &str = "Completed the Prismatic Pathfinder achievement after clearing the course with a full combo at an increased music rate";
    bench_case(
        "in-place word candidate wrapping",
        REPEATS,
        || (benchmark_event_wrap_reference(PARAGRAPH, 168.0, 0.8), 0),
        || (benchmark_event_wrap_current(PARAGRAPH, 168.0, 0.8), 0),
    );

    const DENSE: &str = "Completed the Prismatic Pathfinder achievement and earned 250 points after clearing every difficult chart in the course without missing a step";
    bench_case(
        "reused multi-zoom layout buffer",
        REPEATS,
        || benchmark_event_layout_reference(DENSE, 112.0, 34.0),
        || benchmark_event_layout_current(DENSE, 112.0, 34.0),
    );

    const MULTILINE: &str = "Quest complete! Earned 250 gold at 1.25x rate.\n\nUnlocked the Prismatic Pathfinder title and advanced 12 leaderboard positions.\nClear Type: Full Combo";
    bench_case(
        "tracked multiline layout height",
        REPEATS,
        || benchmark_event_layout_reference(MULTILINE, 144.0, 52.0),
        || benchmark_event_layout_current(MULTILINE, 144.0, 52.0),
    );
}
