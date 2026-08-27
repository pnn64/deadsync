use deadsync_theme_simply_love::screens::gameplay::{
    SongLuaUpdateLookupBenchmark, SongLuaUpdateSnapBenchmark,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    churn_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            churn_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            churn_bytes: self.churn_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all operations delegate unchanged to `System`; relaxed counters only
// observe successful calls while the single-threaded benchmark gate is active.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.churn_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.churn_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.churn_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    churn_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            churn_bytes: self.churn_bytes - before.churn_bytes,
        }
    }
}

struct BenchResult {
    ns_per_frame: f64,
    cycles_per_frame: Option<f64>,
    throughput: f64,
    allocations: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(iterations / 20).max(20) {
        black_box(frame());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        black_box(frame());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocations = ALLOC.snapshot().delta(before);
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_frame: seconds * 1e9 / iterations as f64,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        throughput: iterations as f64 / seconds,
        allocations,
        checksum,
    }
}

fn run(title: &str, iterations: usize, mut old: impl FnMut() -> u64, mut new: impl FnMut() -> u64) {
    let old = measure(iterations, &mut old);
    let new = measure(iterations, &mut new);
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);
    println!("\n{title}");
    print_result("old", iterations, &old);
    print_result("new", iterations, &new);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% throughput  {:+.2}% churn",
        percent_change(old.ns_per_frame, new.ns_per_frame),
        percent_change(
            old.cycles_per_frame.unwrap_or(f64::NAN),
            new.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        percent_change(old.throughput, new.throughput),
        percent_change(
            old.allocations.churn_bytes as f64,
            new.allocations.churn_bytes as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let frames = iterations as f64;
    println!(
        "  {label:<3} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>8.3} Mframe/s  \
         {:>5.2} alloc/frame  {:>5.2} realloc/frame  {:>5.2} free/frame  {:>8.1} churn B/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.throughput / 1e6,
        result.allocations.allocs as f64 / frames,
        result.allocations.reallocs as f64 / frames,
        result.allocations.frees as f64 / frames,
        result.allocations.churn_bytes as f64 / frames,
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocations.allocs, 0);
    assert_eq!(result.allocations.reallocs, 0);
    assert_eq!(result.allocations.frees, 0);
    assert_eq!(result.allocations.churn_bytes, 0);
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
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

fn main() {
    let mut old_snap = SongLuaUpdateSnapBenchmark::new(512, 512);
    let mut new_snap = SongLuaUpdateSnapBenchmark::new(512, 512);
    run(
        "visible-track snap lookup (512 tracks)",
        500_000,
        || old_snap.reference_frame(),
        || new_snap.current_frame(),
    );

    let mut old_lookup = SongLuaUpdateLookupBenchmark::new(256, 512);
    let mut new_lookup = SongLuaUpdateLookupBenchmark::new(256, 512);
    run(
        "dense update-track sampling (512 tracks)",
        20_000,
        || old_lookup.reference_frame(),
        || new_lookup.current_frame(),
    );
}
