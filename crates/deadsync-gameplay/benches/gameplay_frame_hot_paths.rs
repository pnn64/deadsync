use deadsync_gameplay::{
    ActiveColumnScanBench, CrossoverCueCursorBench, DisabledAssistClapBench,
    GameplayFrameHotPathBenchOutput, IdleAttackRefreshBench, IdleHoldPhaseBench, IdleLaneScanBench,
    LiveNotefieldOptionsBench, OptionalFrameWorkBench, SharedMissCutoffBench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 2_048;
const MEASURE_FRAMES: usize = 50_000;

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

// SAFETY: allocation requests are forwarded to `System` unchanged. The
// independent atomics only observe successful allocation and growth calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this exact layout.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the allocator caller guarantees this pointer/layout is live.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the allocator caller supplied the live pointer and old layout.
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
    output: GameplayFrameHotPathBenchOutput,
}

fn main() {
    println!("gameplay frame hot-path microbenchmarks");

    let mut old_attacks = IdleAttackRefreshBench::default();
    let mut new_attacks = old_attacks.clone();
    run_pair(
        "idle attack refresh",
        "ordinary gameplay with no attack or easing windows",
        move |frame| old_attacks.old_frame(frame),
        move |frame| new_attacks.new_frame(frame),
    );

    let mut old_assist = DisabledAssistClapBench::default();
    let mut new_assist = old_assist.clone();
    run_pair(
        "disabled assist-clap bookkeeping",
        "8192 clap rows while timing ticks are disabled",
        move |frame| old_assist.old_frame(frame),
        move |frame| new_assist.new_frame(frame),
    );

    let columns = ActiveColumnScanBench::default();
    run_pair(
        "active-column input/mine scans",
        "4 active columns in fixed-capacity lane state",
        |frame| columns.old_frame(frame),
        |frame| columns.new_frame(frame),
    );

    let mut old_options = LiveNotefieldOptionsBench::default();
    let mut new_options = old_options.clone();
    run_pair(
        "live notefield option refresh",
        "2 players, 8 columns, animated attack scroll overrides",
        move |frame| old_options.old_frame(frame),
        move |frame| new_options.new_frame(frame),
    );

    let optional_work = OptionalFrameWorkBench;
    run_pair(
        "inactive replay/offset setup",
        "live play, replay every 1009 frames and offset repeat every 257 frames",
        |frame| optional_work.old_frame(frame),
        |frame| optional_work.new_frame(frame),
    );

    let mut old_holds = IdleHoldPhaseBench::default();
    let mut new_holds = old_holds.clone();
    run_pair(
        "idle hold-phase scaffolding",
        "4 lanes, sparse active, decaying, and pending holds",
        move |frame| old_holds.old_frame(frame),
        move |frame| new_holds.new_frame(frame),
    );

    let mut old_lane_scan = IdleLaneScanBench::default();
    let mut new_lane_scan = old_lane_scan.clone();
    run_pair(
        "idle held-lane scanning",
        "4 lanes and 24 nearby notes, with input active every 257 frames",
        move |frame| old_lane_scan.old_frame(frame),
        move |frame| new_lane_scan.new_frame(frame),
    );

    let mut old_cutoff = SharedMissCutoffBench::default();
    let mut new_cutoff = old_cutoff.clone();
    run_pair(
        "shared mine/tap miss cutoff",
        "2 players sharing one per-frame timing cutoff",
        move |frame| old_cutoff.old_frame(frame),
        move |frame| new_cutoff.new_frame(frame),
    );

    let mut old_cues = CrossoverCueCursorBench::default();
    let mut new_cues = old_cues.clone();
    run_pair(
        "crossover cue cursor reuse",
        "8192 crossover cues during normal 120 Hz playback and rendering",
        move |frame| old_cues.old_frame(frame),
        move |frame| new_cues.new_frame(frame),
    );
}

fn run_pair(
    name: &str,
    fixture: &str,
    mut old_frame: impl FnMut(usize) -> GameplayFrameHotPathBenchOutput,
    mut new_frame: impl FnMut(usize) -> GameplayFrameHotPathBenchOutput,
) {
    let old = run(&mut old_frame);
    let new = run(&mut new_frame);
    assert_eq!(old.output, new.output, "{name} output checksum mismatch");
    println!("\n{name}\n  {fixture}, {MEASURE_FRAMES} frames");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
    );
}

fn run(frame: &mut impl FnMut(usize) -> GameplayFrameHotPathBenchOutput) -> BenchResult {
    for index in 0..WARMUP_FRAMES {
        black_box(frame(index));
    }
    let mut frame_ns = Vec::with_capacity(MEASURE_FRAMES);
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output = GameplayFrameHotPathBenchOutput::default();
    for index in WARMUP_FRAMES..WARMUP_FRAMES + MEASURE_FRAMES {
        let frame_started = Instant::now();
        let current = black_box(frame(index));
        frame_ns.push(frame_started.elapsed().as_nanos() as u64);
        output.checksum = output.checksum.rotate_left(11) ^ current.checksum;
        output.samples = output.samples.wrapping_add(current.samples);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        frame_ns,
        output,
    }
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    let mut samples = result.frame_ns.clone();
    samples.sort_unstable();
    println!(
        "  {name:<4} {:>10.1} ns/frame {:>10.0} cycles/frame {:>10.0} frames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
    );
    println!(
        "       p50 {:>8} ns p95 {:>8} ns p99 {:>8} ns worst {:>8} ns",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples.last().copied().unwrap_or_default(),
    );
    println!(
        "       allocs={} reallocs={} bytes={}",
        result.alloc.allocs, result.alloc.reallocs, result.alloc.bytes,
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
