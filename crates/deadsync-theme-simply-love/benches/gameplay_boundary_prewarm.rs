use deadsync_theme_simply_love::screens::components::gameplay::gameplay_stats::GameplayStatsTextBenchmark;
use deadsync_theme_simply_love::screens::components::shared::gs_scorebox::GameplayScoreboxBenchmark;
use deadsync_theme_simply_love::screens::gameplay::GameplayBackgroundKeyBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 5_000;
const MEASURE_FRAMES: usize = 50_000;
const SAMPLE_FRAMES: usize = 250;

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

// SAFETY: allocator calls are forwarded unchanged to `System`; relaxed
// counters only observe successful operations while measurement is enabled.
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
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this pair came from the delegated allocator.
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
    ns_per_frame: f64,
    worst_sample_ns: f64,
    cycles_per_frame: Option<f64>,
    allocated: AllocSnapshot,
    checksum: usize,
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

fn measure(mut frame: impl FnMut() -> usize) -> BenchResult {
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
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / sample_frames as f64);
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
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_frame: elapsed.as_secs_f64() * 1_000_000_000.0 / MEASURE_FRAMES as f64,
        worst_sample_ns,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / MEASURE_FRAMES as f64),
        allocated,
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<25} {:>11.2} ns/frame  {:>11.2} cycles/frame  {:>11.2} worst ns  \
         {:>8.3} Mframe/s  {:>6.2} alloc  {:>6.2} realloc  {:>6.2} free  {:>9.1} B/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        1_000.0 / result.ns_per_frame,
        result.allocated.allocs as f64 / frames,
        result.allocated.reallocs as f64 / frames,
        result.allocated.deallocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
    );
}

fn assert_pair(legacy: &BenchResult, prewarmed: &BenchResult) {
    assert_eq!(legacy.checksum, prewarmed.checksum);
    assert_eq!(prewarmed.allocated.allocs, 0);
    assert_eq!(prewarmed.allocated.reallocs, 0);
    assert_eq!(prewarmed.allocated.deallocs, 0);
    assert_eq!(prewarmed.allocated.bytes, 0);
}

fn main() {
    deadsync_theme_simply_love::i18n::init(deadsync_assets::language::load_for_tests("en"));

    let mut scorebox = GameplayScoreboxBenchmark::new();
    assert!(scorebox.behavior_matches(4.25));
    let rebuilt_scorebox = measure(|| scorebox.legacy_frame(4.25));
    let prewarmed_scorebox = measure(|| scorebox.prewarmed_frame(4.25));
    assert_pair(&rebuilt_scorebox, &prewarmed_scorebox);
    assert!(rebuilt_scorebox.allocated.allocs > 0);

    let background = GameplayBackgroundKeyBenchmark::new();
    assert!(background.behavior_matches());
    let rebuilt_background = measure(|| background.legacy_frame());
    let prewarmed_background = measure(|| background.prewarmed_frame());
    assert_pair(&rebuilt_background, &prewarmed_background);
    assert!(rebuilt_background.allocated.allocs >= MEASURE_FRAMES as u64);

    let stats = GameplayStatsTextBenchmark::new();
    assert!(stats.behavior_matches());
    let resolved_stats = measure(|| stats.legacy_frame(4.25));
    let prewarmed_stats = measure(|| stats.prewarmed_frame(4.25));
    assert_pair(&resolved_stats, &prewarmed_stats);

    println!("gameplay boundary prewarm");
    println!("scorebox actor composition (four panes, five rows each)");
    print_result("rebuild panes + actor Vec", &rebuilt_scorebox);
    print_result("prewarmed direct append", &prewarmed_scorebox);
    println!("SongBgWithMovieViz background texture identity");
    print_result("rebuild path Arc", &rebuilt_background);
    print_result("clone prewarmed Arc", &prewarmed_background);
    println!("Step Statistics immutable text identities");
    print_result("resolve labels + strings", &resolved_stats);
    print_result("clone prewarmed Arcs", &prewarmed_stats);
}
