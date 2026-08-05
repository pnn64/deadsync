use deadsync_theme_simply_love::screens::gameplay::GameplayFrameOrchestrationBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const VISUAL_LAYERS: usize = 8;
const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 200_000;
const SAMPLE_FRAMES: usize = 1_000;

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

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful calls while measurement is enabled.
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
    checksum: u64,
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

fn measure(mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    for _ in 0..(MEASURE_FRAMES / SAMPLE_FRAMES) {
        let sample_started = Instant::now();
        for _ in 0..SAMPLE_FRAMES {
            checksum = checksum.wrapping_add(black_box(frame()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / SAMPLE_FRAMES as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
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
        "{label:<24} {:>9.2} ns/frame  {:>9.2} cycles/frame  {:>9.2} worst ns  \
         {:>8.3} Mframe/s  {:>5.2} alloc  {:>5.2} realloc  {:>5.2} free  {:>7.1} B/frame",
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

fn assert_pair(legacy: &BenchResult, fast: &BenchResult) {
    assert_eq!(legacy.checksum, fast.checksum);
    assert_eq!(fast.allocated.allocs, 0);
    assert_eq!(fast.allocated.reallocs, 0);
    assert_eq!(fast.allocated.deallocs, 0);
    assert_eq!(fast.allocated.bytes, 0);
}

fn main() {
    let mut legacy = GameplayFrameOrchestrationBenchmark::new(VISUAL_LAYERS);
    let mut fast = GameplayFrameOrchestrationBenchmark::new(VISUAL_LAYERS);
    let legacy_scratch = measure(|| legacy.scratch_fields_legacy());
    let fast_scratch = measure(|| fast.scratch_pointer());
    assert_pair(&legacy_scratch, &fast_scratch);

    let mut legacy = GameplayFrameOrchestrationBenchmark::new(VISUAL_LAYERS);
    let fast = GameplayFrameOrchestrationBenchmark::new(VISUAL_LAYERS);
    let legacy_layers = measure(|| legacy.layer_resize_legacy());
    let fast_layers = measure(|| fast.fixed_layer_lengths());
    assert_pair(&legacy_layers, &fast_layers);

    let legacy = GameplayFrameOrchestrationBenchmark::new(VISUAL_LAYERS);
    let fast = GameplayFrameOrchestrationBenchmark::new(VISUAL_LAYERS);
    let legacy_stats = measure(|| legacy.step_stats_legacy());
    let fast_stats = measure(|| fast.cached_step_stats());
    assert_pair(&legacy_stats, &fast_stats);

    println!("gameplay frame orchestration ({VISUAL_LAYERS} visual layers)");
    println!("song-lifetime reusable scratch");
    print_result("36 field take/restores", &legacy_scratch);
    print_result("one boxed owner", &fast_scratch);
    println!("immutable layer scratch widths");
    print_result("six resize checks", &legacy_layers);
    print_result("prewarmed invariant", &fast_layers);
    println!("step-statistics option routing");
    print_result("profile checks", &legacy_stats);
    print_result("cached mode", &fast_stats);
}
