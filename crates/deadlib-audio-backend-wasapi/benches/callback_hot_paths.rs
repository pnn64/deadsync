use deadlib_audio_backend_wasapi::bench_support as wasapi;
use deadlib_audio_core::RenderReport;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLE_GROUPS: usize = 500;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful allocator calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
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
            self.realloc_bytes
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
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_callback: f64,
    median_ns: f64,
    p95_ns: f64,
    cycles_per_callback: Option<f64>,
    callbacks_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut callback: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(iterations / 20).max(1) {
        black_box(callback());
    }

    let callbacks_per_sample = (iterations / SAMPLE_GROUPS).max(1);
    let measured_iterations = callbacks_per_sample * SAMPLE_GROUPS;
    let mut samples = Vec::with_capacity(SAMPLE_GROUPS);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..SAMPLE_GROUPS {
        let sample_started = Instant::now();
        for _ in 0..callbacks_per_sample {
            checksum = checksum.wrapping_add(black_box(callback()));
        }
        samples.push(
            sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / callbacks_per_sample as f64,
        );
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    samples.sort_unstable_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..measured_iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(callback()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_callback: seconds * 1_000_000_000.0 / measured_iterations as f64,
        median_ns: samples[samples.len() / 2],
        p95_ns: samples[(samples.len() * 95).div_ceil(100) - 1],
        cycles_per_callback: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_iterations as f64),
        callbacks_per_second: measured_iterations as f64 / seconds,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(old.allocated.operations(), 0, "{title} old path allocated");
    assert_eq!(new.allocated.operations(), 0, "{title} new path allocated");
    assert_eq!(new.allocated.churn_bytes(), 0, "{title} new path churned");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% median  {:>7.2}% p95",
        percent_change(old.ns_per_callback, new.ns_per_callback),
        percent_change(
            old.cycles_per_callback.unwrap_or(f64::NAN),
            new.cycles_per_callback.unwrap_or(f64::NAN),
        ),
        percent_change(old.callbacks_per_second, new.callbacks_per_second),
        percent_change(old.median_ns, new.median_ns),
        percent_change(old.p95_ns, new.p95_ns),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let count = iterations as f64;
    println!(
        "  {label:<3} {:>9.2} ns/cb  {:>9.2} cycles/cb  {:>9.2} median ns  \
         {:>9.2} p95 ns  {:>8.2} Mcb/s  {:>5.2} alloc/Mcb  {:>5.2} realloc/Mcb  \
         {:>5.2} free/Mcb  {:>8.1} churn B/Mcb",
        result.ns_per_callback,
        result.cycles_per_callback.unwrap_or(f64::NAN),
        result.median_ns,
        result.p95_ns,
        result.callbacks_per_second / 1_000_000.0,
        result.allocated.allocs as f64 * 1_000_000.0 / count,
        result.allocated.reallocs as f64 * 1_000_000.0 / count,
        result.allocated.frees as f64 * 1_000_000.0 / count,
        result.allocated.churn_bytes() as f64 * 1_000_000.0 / count,
    );
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
    let report = RenderReport {
        output_underrun: false,
        callback_gap_ns: 0,
    };
    let report_iterations = 1_000_000;
    let old_report = measure(report_iterations, || {
        wasapi::clean_report_old(black_box(report), black_box(false));
        0
    });
    let new_report = measure(report_iterations, || {
        wasapi::clean_report_new(black_box(report), black_box(false));
        0
    });
    print_pair(
        "clean callback diagnostic reporting",
        report_iterations,
        &old_report,
        &new_report,
    );

    let delay_iterations = 10_000_000;
    let frame_time = wasapi::BenchFrameTime::new(48_000);
    let old_delay = measure(delay_iterations, || {
        wasapi::callback_delay_old(
            black_box(48_000),
            black_box(479),
            black_box(3_000_000),
            black_box(7_000_000),
        )
    });
    let new_delay = measure(delay_iterations, || {
        wasapi::callback_delay_new(
            black_box(frame_time),
            black_box(479),
            black_box(3_000_000),
            black_box(7_000_000),
        )
    });
    print_pair(
        "queued-delay telemetry and anchor calculation",
        delay_iterations,
        &old_delay,
        &new_delay,
    );

    let sample_iterations = 20_000_000;
    let old_samples = measure(sample_iterations, || {
        wasapi::sample_count_old(black_box(479), black_box(8), black_box(4)) as u64
    });
    let new_samples = measure(sample_iterations, || {
        wasapi::sample_count_new(black_box(479), black_box(2)) as u64
    });
    print_pair(
        "WASAPI output sample count",
        sample_iterations,
        &old_samples,
        &new_samples,
    );
}
