use deadsync_gameplay::bench_recycle_pending_mine_hit_batch;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 1_024;
const MEASURE_FRAMES: usize = 50_000;
const HIT_INTERVAL: usize = 8;
const MINES_PER_BATCH: usize = 8;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocator calls are forwarded to `System` with their original
// pointer/layout; independent atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this exact allocation layout.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller guarantees this pointer/layout identifies a live allocation.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplied the live pointer and its current layout.
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
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BatchOutput {
    checksum: u64,
    hits: usize,
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    frame_ns: Vec<u64>,
    output: BatchOutput,
}

fn main() {
    let old = run(false);
    let new = run(true);
    assert_eq!(new.output, old.output, "old/new output checksum mismatch");

    println!("gameplay pending-mine batch microbenchmark");
    println!(
        "{MINES_PER_BATCH} simultaneous mine hits every {HIT_INTERVAL} frames, {MEASURE_FRAMES} frames"
    );
    print_result("old: drop batch", &old);
    print_result("new: recycle batch", &new);
    println!(
        "speedup {:.2}x | cycles reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
    );
}

fn run(recycle: bool) -> BenchResult {
    let mut pending = Vec::new();
    for frame in 0..WARMUP_FRAMES {
        black_box(process_frame(&mut pending, frame, recycle));
    }

    let mut frame_ns = Vec::with_capacity(MEASURE_FRAMES);
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output = BatchOutput::default();
    for frame in WARMUP_FRAMES..WARMUP_FRAMES + MEASURE_FRAMES {
        let frame_started = Instant::now();
        let current = black_box(process_frame(&mut pending, frame, recycle));
        frame_ns.push(frame_started.elapsed().as_nanos() as u64);
        output.checksum = output.checksum.rotate_left(9) ^ current.checksum;
        output.hits += current.hits;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        frame_ns,
        output,
    }
}

fn process_frame(pending: &mut Vec<usize>, frame: usize, recycle: bool) -> BatchOutput {
    if frame % HIT_INTERVAL != 0 {
        return BatchOutput::default();
    }
    pending.extend((0..MINES_PER_BATCH).map(|index| frame ^ index));
    let processed = std::mem::take(pending);
    let checksum = processed
        .iter()
        .fold(0_u64, |acc, &index| acc.rotate_left(5) ^ index as u64);
    let hits = processed.len();
    if recycle {
        bench_recycle_pending_mine_hit_batch(pending, processed);
    }
    BatchOutput { checksum, hits }
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    let mut samples = result.frame_ns.clone();
    samples.sort_unstable();
    println!(
        "{name:<22} {:>9.1} ns/frame {:>9.0} cycles/frame {:>10.0} frames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
    );
    println!(
        "{:<22} p50 {:>6} ns p95 {:>6} ns p99 {:>6} ns worst {:>7} ns",
        "sampled frame cost",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples.last().copied().unwrap_or_default(),
    );
    println!(
        "{:<22} allocs={} reallocs={} frees={} bytes={}",
        "memory",
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.deallocs,
        result.alloc.bytes,
    );
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let index = samples.len().saturating_mul(percentile).saturating_sub(1) / 100;
    samples.get(index).copied().unwrap_or_default()
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC only serialize and read this thread's timestamp
    // counter; they do not dereference memory.
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
