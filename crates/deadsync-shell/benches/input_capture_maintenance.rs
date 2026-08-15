use deadsync_shell::{benchmark_prepare_gameplay_capture, benchmark_sync_gameplay_capture};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 100_000;
const MEASURE_FRAMES: usize = 2_000_000;
const SAMPLE_FRAMES: usize = 10_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocator requests are delegated unchanged to `System`; relaxed
// counters observe only this single-threaded benchmark's gated interval.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
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
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    ns_per_frame: f64,
    cycles_per_frame: Option<f64>,
    worst_sample_ns: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn old_frame(frame: u64) -> u64 {
    let first = benchmark_sync_gameplay_capture(black_box(true));
    let second = benchmark_sync_gameplay_capture(black_box(true));
    frame.rotate_left(7) ^ u64::from(first) ^ u64::from(second)
}

fn event_driven_frame(frame: u64) -> u64 {
    frame.rotate_left(7) ^ u64::from(black_box(true)) ^ u64::from(black_box(true))
}

fn measure(frame: fn(u64) -> u64) -> BenchResult {
    for index in 0..WARMUP_FRAMES as u64 {
        black_box(frame(index));
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    for sample in 0..(MEASURE_FRAMES / SAMPLE_FRAMES) {
        let sample_started = Instant::now();
        let start = sample * SAMPLE_FRAMES;
        for index in start..start + SAMPLE_FRAMES {
            checksum = checksum.wrapping_add(black_box(frame(index as u64)));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1e9 / SAMPLE_FRAMES as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for index in 0..MEASURE_FRAMES as u64 {
        black_box(frame(index));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        ns_per_frame: elapsed.as_secs_f64() * 1e9 / MEASURE_FRAMES as f64,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / MEASURE_FRAMES as f64),
        worst_sample_ns,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn main() {
    benchmark_prepare_gameplay_capture();
    let old = measure(old_frame);
    let new = measure(event_driven_frame);

    assert_eq!(old.checksum, new.checksum);
    assert_eq!(old.allocated.allocs, 0);
    assert_eq!(old.allocated.reallocs, 0);
    assert_eq!(old.allocated.frees, 0);
    assert_eq!(old.allocated.bytes, 0);
    assert_eq!(new.allocated.allocs, 0);
    assert_eq!(new.allocated.reallocs, 0);
    assert_eq!(new.allocated.frees, 0);
    assert_eq!(new.allocated.bytes, 0);

    println!("steady gameplay shell input-capture maintenance");
    print_result("old (2 syncs/frame)", &old);
    print_result("new (event-driven)", &new);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% worst sample",
        change(old.ns_per_frame, new.ns_per_frame),
        change(
            old.cycles_per_frame.unwrap_or(f64::NAN),
            new.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        change(old.worst_sample_ns, new.worst_sample_ns),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<21} {:>9.2} ns/frame  {:>9.2} cycles/frame  {:>9.2} worst ns  \
         {:>8.3} Mframe/s  {:>3} alloc  {:>3} realloc  {:>3} free  {:>3} B",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        1_000.0 / result.ns_per_frame,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.bytes,
    );
}

fn change(old: f64, new: f64) -> f64 {
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
