use deadlib_assets::upload::VideoUploadKeyBenchmark;
use deadlib_present::actors::Actor;
use deadsync_shell::bench_support::GameplayMediaFailureBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 50_000;
const SAMPLE_FRAMES: usize = 250;
const ACTOR_SONGS: usize = 256;
const ACTOR_BASE_CAPACITY: usize = 256;
const ACTORS_PER_FRAME: usize = 640;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

// SAFETY: every operation is forwarded unchanged to `System`; the relaxed
// counters only observe successful calls while the benchmark enables them.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` came from the allocation caller.
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
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this pointer/layout pair came from the delegated allocator.
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

struct BenchResult {
    ns_per_unit: f64,
    worst_sample_ns: f64,
    cycles_per_unit: Option<f64>,
    allocated: AllocSnapshot,
    checksum: usize,
    units: usize,
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

fn measure_frames(mut frame: impl FnMut() -> usize) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0usize;
    let mut worst_sample_ns = 0.0f64;
    let mut measured = 0usize;
    while measured < MEASURE_FRAMES {
        let sample_frames = SAMPLE_FRAMES.min(MEASURE_FRAMES - measured);
        let sample_started = Instant::now();
        for _ in 0..sample_frames {
            checksum = checksum.wrapping_add(black_box(frame()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1e9 / sample_frames as f64);
        measured += sample_frames;
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0usize;
    for _ in 0..MEASURE_FRAMES {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_unit: elapsed.as_secs_f64() * 1e9 / MEASURE_FRAMES as f64,
        worst_sample_ns,
        cycles_per_unit: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / MEASURE_FRAMES as f64),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
        units: MEASURE_FRAMES,
    }
}

fn actor_buffers(prewarmed: bool) -> Vec<Vec<Actor>> {
    (0..ACTOR_SONGS)
        .map(|_| {
            let mut actors = Vec::with_capacity(ACTOR_BASE_CAPACITY);
            if prewarmed {
                actors.reserve(ACTORS_PER_FRAME);
            }
            actors
        })
        .collect()
}

fn fill_actor_buffers(buffers: &mut [Vec<Actor>]) -> usize {
    buffers.iter_mut().fold(0usize, |checksum, actors| {
        actors.extend((0..ACTORS_PER_FRAME).map(|_| Actor::CameraPop));
        checksum.rotate_left(5) ^ actors.len() ^ actors.capacity()
    })
}

fn measure_actor_songs(prewarmed: bool) -> BenchResult {
    let mut timing_buffers = actor_buffers(prewarmed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0usize;
    let mut worst_sample_ns = 0.0f64;
    for sample in timing_buffers.chunks_mut(8) {
        let sample_started = Instant::now();
        checksum = checksum.rotate_left(5) ^ fill_actor_buffers(sample);
        worst_sample_ns =
            worst_sample_ns.max(sample_started.elapsed().as_secs_f64() * 1e9 / sample.len() as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    drop(timing_buffers);

    let mut allocation_buffers = actor_buffers(prewarmed);
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = fill_actor_buffers(&mut allocation_buffers);
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    drop(allocation_buffers);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_unit: elapsed.as_secs_f64() * 1e9 / ACTOR_SONGS as f64,
        worst_sample_ns,
        cycles_per_unit: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ACTOR_SONGS as f64),
        allocated,
        checksum,
        units: ACTOR_SONGS,
    }
}

fn print_result(label: &str, result: &BenchResult, unit: &str) {
    let units = result.units as f64;
    println!(
        "{label:<20} {:>10.2} ns/{unit:<5} {:>10.2} cycles/{unit:<5} {:>10.2} worst ns  \
         {:>8.3} M{unit}/s  {:>5.2} alloc  {:>5.2} realloc  {:>5.2} free  {:>9.1} B/{unit}",
        result.ns_per_unit,
        result.cycles_per_unit.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        1_000.0 / result.ns_per_unit,
        result.allocated.allocs as f64 / units,
        result.allocated.reallocs as f64 / units,
        result.allocated.deallocs as f64 / units,
        result.allocated.bytes as f64 / units,
    );
}

fn compare(title: &str, unit: &str, old: BenchResult, new: BenchResult) {
    assert_eq!(old.checksum, new.checksum, "behavior checksum changed");
    assert_eq!(new.allocated.allocs, 0, "optimized path allocated");
    assert_eq!(new.allocated.reallocs, 0, "optimized path reallocated");
    assert_eq!(new.allocated.deallocs, 0, "optimized path freed");
    println!("{title}");
    print_result("old", &old, unit);
    print_result("new", &new, unit);
    println!();
}

fn main() {
    compare(
        "first gameplay actor frame after song entry",
        "song",
        measure_actor_songs(false),
        measure_actor_songs(true),
    );

    let mut old = VideoUploadKeyBenchmark::default();
    let mut new = VideoUploadKeyBenchmark::default();
    compare(
        "decoded gameplay video upload key",
        "frame",
        measure_frames(|| old.legacy_frame()),
        measure_frames(|| new.shared_frame()),
    );

    let mut old = GameplayMediaFailureBenchmark::default();
    let new = GameplayMediaFailureBenchmark::default();
    compare(
        "failed banner-video preparation",
        "frame",
        measure_frames(|| old.legacy_banner_retry_frame()),
        measure_frames(|| new.saturated_banner_failure_frame()),
    );
}
