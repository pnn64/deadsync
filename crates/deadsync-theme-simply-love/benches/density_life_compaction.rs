use deadsync_theme_simply_love::screens::components::gameplay::gameplay_stats::{
    benchmark_clip_density_life_points, benchmark_clip_density_life_points_legacy,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PLAYERS: usize = 2;
const VISIBLE_POINTS: usize = 961;
const POINT_CAPACITY: usize = 1_024;
const WARMUP_REFRESHES: usize = 2_000;
const MEASURE_REFRESHES: usize = 50_000;
const SAMPLE_REFRESHES: usize = 10_000;

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

// SAFETY: every operation delegates to `System` unchanged; the atomics only
// observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: all arguments are forwarded unchanged to `System`.
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

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    moved_points: usize,
    checksum: u64,
}

#[derive(Clone, Copy)]
struct CycleSamples {
    p50: u64,
    p95: u64,
    p99: u64,
    worst: u64,
}

fn initial_points() -> [Vec<[f32; 2]>; PLAYERS] {
    std::array::from_fn(|player| {
        let mut points = Vec::with_capacity(POINT_CAPACITY);
        points.extend(
            (0..VISIBLE_POINTS)
                .map(|index| [index as f32, ((index * 17 + player * 13) % 101) as f32]),
        );
        points
    })
}

fn visible_checksum(points: &[[f32; 2]], offset: f32) -> u64 {
    let start = points.partition_point(|point| point[0] < offset);
    let visible = &points[start..];
    let Some(first) = visible.first() else {
        return 0;
    };
    let Some(last) = visible.last() else {
        return 0;
    };
    (visible.len() as u64).rotate_left(7)
        ^ u64::from(first[0].to_bits()).rotate_left(19)
        ^ u64::from(first[1].to_bits()).rotate_left(31)
        ^ u64::from(last[0].to_bits()).rotate_left(43)
}

fn run_refresh(
    points: &mut [Vec<[f32; 2]>; PLAYERS],
    refresh: usize,
    clip: fn(&mut Vec<[f32; 2]>, f32) -> usize,
) -> (usize, u64) {
    let x = (VISIBLE_POINTS + refresh) as f32;
    let offset = refresh as f32 + 1.25;
    let mut moved_points = 0usize;
    let mut checksum = 0u64;
    for (player, history) in points.iter_mut().enumerate() {
        history.push([x, ((refresh * 29 + player * 7) % 101) as f32]);
        moved_points += clip(history, offset);
        checksum = checksum.rotate_left(11) ^ visible_checksum(history, offset);
    }
    (moved_points, checksum)
}

fn measure(clip: fn(&mut Vec<[f32; 2]>, f32) -> usize) -> BenchResult {
    let mut points = initial_points();
    for refresh in 0..WARMUP_REFRESHES {
        black_box(run_refresh(&mut points, refresh, clip));
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut moved_points = 0usize;
    let mut checksum = 0u64;
    for refresh in WARMUP_REFRESHES..WARMUP_REFRESHES + MEASURE_REFRESHES {
        let (moved, frame_checksum) = run_refresh(&mut points, refresh, clip);
        moved_points += moved;
        checksum = checksum.rotate_left(13) ^ black_box(frame_checksum);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        moved_points,
        checksum,
    }
}

fn sample_cycles(clip: fn(&mut Vec<[f32; 2]>, f32) -> usize) -> CycleSamples {
    let mut points = initial_points();
    for refresh in 0..WARMUP_REFRESHES {
        black_box(run_refresh(&mut points, refresh, clip));
    }

    let mut samples = Vec::with_capacity(SAMPLE_REFRESHES);
    for refresh in WARMUP_REFRESHES..WARMUP_REFRESHES + SAMPLE_REFRESHES {
        let started = read_cycles();
        black_box(run_refresh(&mut points, refresh, clip));
        samples.push(read_cycles().saturating_sub(started));
    }
    samples.sort_unstable();
    CycleSamples {
        p50: samples[SAMPLE_REFRESHES * 50 / 100],
        p95: samples[SAMPLE_REFRESHES * 95 / 100],
        p99: samples[SAMPLE_REFRESHES * 99 / 100],
        worst: samples[SAMPLE_REFRESHES - 1],
    }
}

fn print_result(label: &str, result: &BenchResult, samples: CycleSamples) {
    let refreshes = MEASURE_REFRESHES as f64;
    let moved_kib = result.moved_points * size_of::<[f32; 2]>();
    println!(
        "{label:<17} {:>8.1} ns/refresh  {:>8.0} cycles/refresh  \
         {:>5.2} allocs  {:>5.2} reallocs  {:>6.2} KiB heap  \
         {:>7.2} KiB moved/refresh",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / refreshes,
        result.cycles as f64 / refreshes,
        result.allocated.allocs as f64 / refreshes,
        result.allocated.reallocs as f64 / refreshes,
        result.allocated.bytes as f64 / refreshes / 1024.0,
        moved_kib as f64 / refreshes / 1024.0,
    );
    println!(
        "{:17} p50 {:>5}  p95 {:>5}  p99 {:>5}  worst {:>7} cycles",
        "", samples.p50, samples.p95, samples.p99, samples.worst,
    );
}

fn main() {
    let per_step = measure(benchmark_clip_density_life_points_legacy);
    let batched = measure(benchmark_clip_density_life_points);
    let per_step_samples = sample_cycles(benchmark_clip_density_life_points_legacy);
    let batched_samples = sample_cycles(benchmark_clip_density_life_points);
    assert_eq!(per_step.checksum, batched.checksum);
    black_box((per_step.checksum, batched.checksum));

    println!(
        "gameplay density-life history trim benchmark \
         ({PLAYERS} players, {VISIBLE_POINTS}-point scrolling windows)"
    );
    print_result("per-step drain", &per_step, per_step_samples);
    print_result("batched compact", &batched, batched_samples);
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
