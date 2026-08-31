use deadsync_audio_stream::sola_bench_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FRAMES: usize = 4_096;
const CHANNELS: usize = 2;
const WINDOW_FRAMES: usize = 1_440;
const FADE_START: usize = 137;
const FADE_FRAMES: usize = 1_024;
const CORRELATE_FRAMES: usize = 360;
const DEINTERLEAVE_RUNS: usize = 5_000;
const CROSSFADE_RUNS: usize = 10_000;
const CORRELATION_RUNS: usize = 20_000;
const CAPACITY_RUNS: usize = 5_000;
const CURSOR_QUERY_RUNS: usize = 2_000_000;
const DEAD_PREFIX: usize = 4_096;
const LIVE_FRAMES: usize = 3_072;
const APPEND_FRAMES: usize = 1_024;
const SAMPLES: usize = 32;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    dealloc_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            dealloc_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            dealloc_bytes: self.dealloc_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocator operation delegates unchanged to `System`; relaxed
// counters only observe successful calls while the benchmark gate is enabled.
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
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.dealloc_bytes
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
    deallocs: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    dealloc_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            dealloc_bytes: self.dealloc_bytes - before.dealloc_bytes,
        }
    }

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.deallocs
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.dealloc_bytes
    }
}

struct BenchResult {
    ns_per_unit: f64,
    cycles_per_unit: Option<f64>,
    units_per_second: f64,
    median_ns_per_unit: f64,
    p95_ns_per_unit: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(runs: usize, units_per_run: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    let sample_runs = (runs / 20).max(1);
    for _ in 0..sample_runs {
        black_box(operation());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..runs {
        checksum = checksum.rotate_left(5) ^ black_box(operation());
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..runs {
        black_box(operation());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);

    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        for _ in 0..sample_runs {
            black_box(operation());
        }
        samples.push(
            started.elapsed().as_secs_f64() * 1_000_000_000.0
                / (sample_runs * units_per_run) as f64,
        );
    }
    samples.sort_unstable_by(f64::total_cmp);
    let median_ns_per_unit = samples[samples.len() / 2];
    let p95_index = (samples.len() * 95).div_ceil(100).saturating_sub(1);
    let p95_ns_per_unit = samples[p95_index];

    let units = (runs * units_per_run) as f64;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_unit: seconds * 1_000_000_000.0 / units,
        cycles_per_unit: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / units),
        units_per_second: units / seconds,
        median_ns_per_unit,
        p95_ns_per_unit,
        allocated,
        checksum,
    }
}

fn print_pair(name: &str, unit: &str, runs: usize, old: &BenchResult, new: &BenchResult) {
    println!("\n{name}");
    print_result("old", unit, runs, old);
    print_result("new", unit, runs, new);
    assert_eq!(new.checksum, old.checksum, "{name} output diverged");
    assert_eq!(old.allocated.operations(), 0, "{name} old path allocated");
    assert_eq!(new.allocated.operations(), 0, "{name} new path allocated");
    assert_eq!(new.allocated.churn_bytes(), 0, "{name} new path churned");
}

fn print_capacity_pair(name: &str, unit: &str, runs: usize, old: &BenchResult, new: &BenchResult) {
    println!("\n{name}");
    print_result("old", unit, runs, old);
    print_result("new", unit, runs, new);
    assert_eq!(new.checksum, old.checksum, "{name} output diverged");
    assert!(
        old.allocated.reallocs > new.allocated.reallocs,
        "{name} did not eliminate legacy reallocations"
    );
    assert_eq!(
        new.allocated.reallocs, 0,
        "{name} retained path reallocated"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{name} did not reduce allocation churn"
    );
}

fn print_result(label: &str, unit: &str, runs: usize, result: &BenchResult) {
    let count = runs as f64;
    println!(
        "{label:<4} {:>8.3} ns/{unit}  {:>8.3} cycles/{unit}  {:>8.3} M{unit}/s  \
         median {:>8.3} ns  p95 {:>8.3} ns  {:>5.2} alloc/run  {:>5.2} realloc/run  \
         {:>5.2} free/run  {:>8.1} alloc B/run  {:>8.1} churn B/run  {:016x}",
        result.ns_per_unit,
        result.cycles_per_unit.unwrap_or(f64::NAN),
        result.units_per_second / 1_000_000.0,
        result.median_ns_per_unit,
        result.p95_ns_per_unit,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.deallocs as f64 / count,
        result.allocated.alloc_bytes as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
        result.checksum,
    );
}

fn output_checksum(output: &[Vec<f32>]) -> u64 {
    output.iter().fold(0u64, |checksum, channel| {
        checksum.rotate_left(7)
            ^ u64::from(channel.first().copied().unwrap_or_default().to_bits())
            ^ (u64::from(channel.last().copied().unwrap_or_default().to_bits()) << 32)
            ^ channel.len() as u64
    })
}

fn main() {
    let positions = [(120usize, 115usize), (137, 142)];
    let max_correlated = positions
        .iter()
        .map(|&(correlated, _)| correlated)
        .max()
        .unwrap();
    let min_correlated = positions
        .iter()
        .map(|&(correlated, _)| correlated)
        .min()
        .unwrap();
    let max_last = positions.iter().map(|&(_, last)| last).max().unwrap();

    let cursor_avail_old = measure(CURSOR_QUERY_RUNS, 1, || {
        sola_bench_support::cursor_avail_old(
            black_box(WINDOW_FRAMES),
            black_box(71),
            black_box(2_048),
            black_box(&positions),
        ) as u64
    });
    let cursor_avail_new = measure(CURSOR_QUERY_RUNS, 1, || {
        sola_bench_support::cursor_avail_new(
            black_box(WINDOW_FRAMES),
            black_box(71),
            black_box(2_048),
            black_box(max_correlated),
            black_box(max_last),
        ) as u64
    });
    print_pair(
        "cached SOLA readable-window bound",
        "query",
        CURSOR_QUERY_RUNS,
        &cursor_avail_old,
        &cursor_avail_new,
    );

    let max_needed_old = measure(CURSOR_QUERY_RUNS, 1, || {
        sola_bench_support::max_needed_old(
            black_box(180),
            black_box(360),
            black_box(WINDOW_FRAMES),
            black_box(71),
            black_box(&positions),
        ) as u64
    });
    let max_needed_new = measure(CURSOR_QUERY_RUNS, 1, || {
        sola_bench_support::max_needed_new(
            black_box(180),
            black_box(360),
            black_box(WINDOW_FRAMES),
            black_box(max_correlated),
            black_box(71),
        ) as u64
    });
    print_pair(
        "cached SOLA source-sufficiency bound",
        "query",
        CURSOR_QUERY_RUNS,
        &max_needed_old,
        &max_needed_new,
    );

    let earliest_old = measure(CURSOR_QUERY_RUNS, 1, || {
        sola_bench_support::earliest_old(black_box(180), black_box(&positions)) as u64
    });
    let earliest_new = measure(CURSOR_QUERY_RUNS, 1, || {
        sola_bench_support::earliest_new(black_box(180), black_box(min_correlated)) as u64
    });
    print_pair(
        "cached SOLA erase-front boundary",
        "query",
        CURSOR_QUERY_RUNS,
        &earliest_old,
        &earliest_new,
    );

    let interleaved = (0..FRAMES * CHANNELS)
        .map(|index| index.wrapping_mul(25_173) as i16)
        .collect::<Vec<_>>();
    let mut old_planar = [Vec::with_capacity(FRAMES), Vec::with_capacity(FRAMES)];
    let mut new_planar = [Vec::with_capacity(FRAMES), Vec::with_capacity(FRAMES)];
    let deinterleave_old = measure(DEINTERLEAVE_RUNS, FRAMES, || {
        sola_bench_support::deinterleave_old(black_box(&interleaved), &mut old_planar);
        output_checksum(&old_planar)
    });
    let deinterleave_new = measure(DEINTERLEAVE_RUNS, FRAMES, || {
        sola_bench_support::deinterleave_new(black_box(&interleaved), &mut new_planar);
        output_checksum(&new_planar)
    });
    print_pair(
        "stereo i16 deinterleave and normalize",
        "frame",
        DEINTERLEAVE_RUNS,
        &deinterleave_old,
        &deinterleave_new,
    );

    let prev = (0..FADE_FRAMES)
        .map(|index| (index as f32 * 0.017).sin())
        .collect::<Vec<_>>();
    let current = (0..FADE_FRAMES)
        .map(|index| (index as f32 * 0.029).cos())
        .collect::<Vec<_>>();
    let weights = (0..WINDOW_FRAMES)
        .map(|frame| frame as f32 / WINDOW_FRAMES as f32)
        .collect::<Vec<_>>();
    let mut old_fade = Vec::with_capacity(FADE_FRAMES);
    let mut new_fade = Vec::with_capacity(FADE_FRAMES);
    let crossfade_old = measure(CROSSFADE_RUNS, FADE_FRAMES, || {
        sola_bench_support::crossfade_old(
            black_box(&prev),
            black_box(&current),
            FADE_START,
            WINDOW_FRAMES,
            &mut old_fade,
        );
        output_checksum(std::slice::from_ref(&old_fade))
    });
    let crossfade_new = measure(CROSSFADE_RUNS, FADE_FRAMES, || {
        sola_bench_support::crossfade_new(
            black_box(&prev),
            black_box(&current),
            black_box(&weights[FADE_START..FADE_START + FADE_FRAMES]),
            &mut new_fade,
        );
        output_checksum(std::slice::from_ref(&new_fade))
    });
    print_pair(
        "SOLA cached crossfade weights",
        "frame",
        CROSSFADE_RUNS,
        &crossfade_old,
        &crossfade_new,
    );

    let mut identity_fade_old = Vec::with_capacity(FADE_FRAMES);
    let mut identity_fade_new = Vec::with_capacity(FADE_FRAMES);
    let identity_crossfade_old = measure(CROSSFADE_RUNS, FADE_FRAMES, || {
        sola_bench_support::identity_crossfade_old(
            black_box(&prev),
            black_box(&weights[FADE_START..FADE_START + FADE_FRAMES]),
            &mut identity_fade_old,
        );
        output_checksum(std::slice::from_ref(&identity_fade_old))
    });
    let identity_crossfade_new = measure(CROSSFADE_RUNS, FADE_FRAMES, || {
        sola_bench_support::identity_crossfade_new(
            black_box(&prev),
            black_box(&weights[FADE_START..FADE_START + FADE_FRAMES]),
            &mut identity_fade_new,
        );
        output_checksum(std::slice::from_ref(&identity_fade_new))
    });
    print_pair(
        "identity-window SOLA crossfade",
        "frame",
        CROSSFADE_RUNS,
        &identity_crossfade_old,
        &identity_crossfade_new,
    );

    let silent_search = vec![0.0f32; CORRELATE_FRAMES * 2];
    let silent_pattern = vec![0.0f32; CORRELATE_FRAMES];
    let correlation_old = measure(CORRELATION_RUNS, 1, || {
        sola_bench_support::closest_match_old(black_box(&silent_search), black_box(&silent_pattern))
            as u64
    });
    let correlation_new = measure(CORRELATION_RUNS, 1, || {
        sola_bench_support::closest_match_new(black_box(&silent_search), black_box(&silent_pattern))
            as u64
    });
    print_pair(
        "silent SOLA correlation search",
        "search",
        CORRELATION_RUNS,
        &correlation_old,
        &correlation_new,
    );

    let music_pattern = (0..CORRELATE_FRAMES)
        .map(|index| ((index % 17) as f32).mul_add(0.001, (index as f32 * 0.071).sin() * 0.7))
        .collect::<Vec<_>>();
    let music_search = (0..CORRELATE_FRAMES * 2)
        .map(|index| ((index % 13) as f32).mul_add(0.001, (index as f32 * 0.069).sin() * 0.7))
        .collect::<Vec<_>>();
    let music_old = measure(CORRELATION_RUNS, 1, || {
        sola_bench_support::closest_match_old(black_box(&music_search), black_box(&music_pattern))
            as u64
    });
    let music_new = measure(CORRELATION_RUNS, 1, || {
        sola_bench_support::closest_match_new(black_box(&music_search), black_box(&music_pattern))
            as u64
    });
    print_pair(
        "nonmatching music-like SOLA correlation",
        "search",
        CORRELATION_RUNS,
        &music_old,
        &music_new,
    );

    let right_pattern = (0..CORRELATE_FRAMES)
        .map(|index| ((index % 11) as f32).mul_add(0.001, (index as f32 * 0.047).cos() * 0.6))
        .collect::<Vec<_>>();
    let right_search = (0..CORRELATE_FRAMES * 2)
        .map(|index| ((index % 19) as f32).mul_add(0.001, (index as f32 * 0.043).cos() * 0.6))
        .collect::<Vec<_>>();
    let stereo_correlation_old = measure(CORRELATION_RUNS, 1, || {
        let (left, right) = sola_bench_support::stereo_closest_match_old(
            black_box(&music_search),
            black_box(&music_pattern),
            black_box(&right_search),
            black_box(&right_pattern),
        );
        left as u64 ^ (right as u64).rotate_left(32)
    });
    let stereo_correlation_new = measure(CORRELATION_RUNS, 1, || {
        let (left, right) = sola_bench_support::stereo_closest_match_new(
            black_box(&music_search),
            black_box(&music_pattern),
            black_box(&right_search),
            black_box(&right_pattern),
        );
        left as u64 ^ (right as u64).rotate_left(32)
    });
    print_pair(
        "fused stereo music-like SOLA correlation",
        "stereo-search",
        CORRELATION_RUNS,
        &stereo_correlation_old,
        &stereo_correlation_new,
    );

    let capacity_old = measure(CAPACITY_RUNS, APPEND_FRAMES, || {
        sola_bench_support::capacity_reuse_old(DEAD_PREFIX, LIVE_FRAMES, APPEND_FRAMES)
    });
    let capacity_new = measure(CAPACITY_RUNS, APPEND_FRAMES, || {
        sola_bench_support::capacity_reuse_new(DEAD_PREFIX, LIVE_FRAMES, APPEND_FRAMES)
    });
    print_capacity_pair(
        "SOLA source-buffer dead-prefix reuse",
        "appended-frame",
        CAPACITY_RUNS,
        &capacity_old,
        &capacity_new,
    );

    println!(
        "\none-time 48 kHz fade table: 1 allocation, {} bytes",
        WINDOW_FRAMES * size_of::<f32>()
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
