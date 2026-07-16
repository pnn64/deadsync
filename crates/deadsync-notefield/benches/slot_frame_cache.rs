use deadsync_notefield::{SlotFrameBench, SlotFrameBenchOutput};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 256;
const MEASURE_FRAMES: usize = 4_000;

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

// SAFETY: operations delegate to `System` with the original pointer/layout;
// the independent atomics only observe successful allocation operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller provides the live allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
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

#[derive(Clone, Copy, Default)]
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
    cycles: u64,
    alloc: AllocSnapshot,
    frame_ns: Vec<u64>,
    output: SlotFrameBenchOutput,
    fixed_bytes: usize,
}

fn main() {
    let old = run_old();
    let new = run_new();
    assert_eq!(new.output, old.output, "old/new output mismatch");

    println!("noteskin slot frame-state microbenchmark");
    println!(
        "{} visible model notes, {} unique slots, 64 tween segments, {MEASURE_FRAMES} frames",
        SlotFrameBench::visible_notes(),
        SlotFrameBench::unique_slots(),
    );
    print_result("old: per-note scan/map", &old);
    print_result("new: slot frame array", &new);
    println!(
        "speedup {:.2}x | cycles reduction {:.1}% | retained frame cache {:.1} KiB",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
        new.fixed_bytes as f64 / 1024.0,
    );
}

fn run_old() -> BenchResult {
    let mut bench = SlotFrameBench::default();
    for frame in 0..WARMUP_FRAMES {
        black_box(bench.old_frame(frame));
    }
    run(0, |frame| bench.old_frame(frame))
}

fn run_new() -> BenchResult {
    let mut bench = SlotFrameBench::default();
    for frame in 0..WARMUP_FRAMES {
        black_box(bench.new_frame(frame));
    }
    let fixed_bytes = bench.fixed_bytes();
    run(fixed_bytes, |frame| bench.new_frame(frame))
}

fn run(fixed_bytes: usize, mut frame_fn: impl FnMut(usize) -> SlotFrameBenchOutput) -> BenchResult {
    let mut frame_ns = Vec::with_capacity(MEASURE_FRAMES);
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output = SlotFrameBenchOutput::default();
    for frame in WARMUP_FRAMES..WARMUP_FRAMES + MEASURE_FRAMES {
        let frame_started = Instant::now();
        let current = black_box(frame_fn(frame));
        frame_ns.push(frame_started.elapsed().as_nanos() as u64);
        output.checksum = output.checksum.rotate_left(11) ^ current.checksum;
        output.draws += current.draws;
        output.geometry_clones += current.geometry_clones;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        frame_ns,
        output,
        fixed_bytes,
    }
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    let mut samples = result.frame_ns.clone();
    samples.sort_unstable();
    println!(
        "{name:<24} {:>9.1} ns/frame {:>10.0} cycles/frame {:>10.0} frames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
    );
    println!(
        "{:<24} p50 {:>7} ns p95 {:>7} ns p99 {:>7} ns worst {:>7} ns",
        "sampled frame cost",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples.last().copied().unwrap_or_default(),
    );
    println!(
        "{:<24} allocs={} reallocs={} bytes={} fixed={} bytes",
        "memory",
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.bytes,
        result.fixed_bytes,
    );
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let index = samples.len().saturating_mul(percentile).saturating_sub(1) / 100;
    samples.get(index).copied().unwrap_or_default()
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measured instruction interval.
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
