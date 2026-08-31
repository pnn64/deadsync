use deadlib_present::anim::bench_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const EVALUATIONS: usize = 500_000;
const SAMPLES: usize = 21;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    reallocated_bytes: AtomicU64,
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
            reallocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            reallocated_bytes: self.reallocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all requests are delegated unchanged to `System`; relaxed counters
// observe this single-threaded benchmark only while measurement is enabled.
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
        // SAFETY: the delegated allocator produced this pointer-layout pair.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let output = unsafe { System.realloc(pointer, old, new_size) };
        if !output.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.reallocated_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        output
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated_bytes: u64,
    reallocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            reallocated_bytes: self.reallocated_bytes - before.reallocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn(self) -> u64 {
        self.allocated_bytes + self.reallocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(operation: fn(usize) -> u64) -> BenchResult {
    black_box(operation(EVALUATIONS / 10));
    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        checksum = black_box(operation(EVALUATIONS));
        times.push(started.elapsed().as_secs_f64() * 1e9 / EVALUATIONS as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / EVALUATIONS as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation(EVALUATIONS));
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(
    title: &str,
    legacy_operation: fn(usize) -> u64,
    current_operation: fn(usize) -> u64,
) {
    let legacy = measure(legacy_operation);
    let current = measure(current_operation);
    assert_eq!(
        current.checksum, legacy.checksum,
        "{title} behavior changed"
    );
    assert_eq!(legacy.allocated.operations(), 0, "legacy path allocated");
    assert_eq!(current.allocated.operations(), 0, "current path allocated");
    println!("\n{title}");
    print_result("old", &legacy);
    print_result("new", &current);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% throughput  {:+.2}% churn",
        change(legacy.median_ns, current.median_ns),
        change(legacy.p95_ns, current.p95_ns),
        change(
            legacy.median_cycles.unwrap_or(f64::NAN),
            current.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(&legacy), throughput(&current)),
        change(
            legacy.allocated.churn() as f64,
            current.allocated.churn() as f64
        ),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<3} {:>8.2} ns/eval  p95 {:>8.2} ns  {:>8.2} cycles/eval  {:>8.2} Meval/s  {:>3} alloc  {:>3} realloc  {:>3} free  {:>6} churn B",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        throughput(result) / 1e6,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.churn(),
    );
}

fn throughput(result: &BenchResult) -> f64 {
    1e9 / result.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    print_pair(
        "fused XY tween operation",
        bench_support::xy_pair_legacy,
        bench_support::xy_pair_current,
    );
    print_pair(
        "fused width-height tween operation",
        bench_support::size_pair_legacy,
        bench_support::size_pair_current,
    );
    print_pair(
        "fused equal-axis zoom operation",
        bench_support::scale_pair_legacy,
        bench_support::scale_pair_current,
    );
    print_pair(
        "fused non-uniform zoom operation",
        bench_support::scale_xy_pair_legacy,
        bench_support::scale_xy_pair_current,
    );
    print_pair(
        "fused zoomto operation",
        bench_support::zoom_to_pair_legacy,
        bench_support::zoom_to_pair_current,
    );
    print_pair(
        "single-pass segment completion",
        bench_support::segment_completion_legacy,
        bench_support::segment_completion_current,
    );
    print_pair(
        "specialized alpha-only tint",
        bench_support::tint_alpha_legacy,
        bench_support::tint_alpha_current,
    );
    print_pair(
        "specialized RGB-only tint",
        bench_support::tint_rgb_legacy,
        bench_support::tint_rgb_current,
    );
    print_pair(
        "specialized RGB-only glow",
        bench_support::glow_rgb_legacy,
        bench_support::glow_rgb_current,
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC only serialize and read the timestamp counter.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC only serialize and read the timestamp counter.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
