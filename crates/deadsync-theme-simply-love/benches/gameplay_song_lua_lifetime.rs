use deadsync_theme_simply_love::screens::gameplay::{
    SongLuaForegroundOwnerBenchmark, SongLuaLayerActivityBenchmark, SongLuaStatePlanBenchmark,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const LAYERS: usize = 256;
const ACTIVE_LAYERS: usize = 8;
const OVERLAYS: usize = 1_024;
const TREE_ACTORS: usize = 512;
const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 100_000;
const TREE_FRAMES: usize = 20_000;
const SAMPLE_FRAMES: usize = 500;

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

fn measure(frames: usize, mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    let mut measured = 0usize;
    while measured < frames {
        let sample_frames = SAMPLE_FRAMES.min(frames - measured);
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
    let mut allocation_checksum = 0u64;
    for _ in 0..frames {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_frame: elapsed.as_secs_f64() * 1_000_000_000.0 / frames as f64,
        worst_sample_ns,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / frames as f64),
        allocated,
        checksum,
    }
}

fn print_result(label: &str, frames: usize, result: &BenchResult) {
    let frames = frames as f64;
    println!(
        "{label:<24} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>10.2} worst ns  \
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
    let legacy_layers = SongLuaLayerActivityBenchmark::new(LAYERS, ACTIVE_LAYERS);
    let mut fast_layers = SongLuaLayerActivityBenchmark::new(LAYERS, ACTIVE_LAYERS);
    let full_layer_scans = measure(MEASURE_FRAMES, || legacy_layers.full_scans());
    let active_layer_cursor = measure(MEASURE_FRAMES, || fast_layers.active_cursor());
    assert_pair(&full_layer_scans, &active_layer_cursor);

    let foreground = SongLuaForegroundOwnerBenchmark::new(OVERLAYS);
    let foreground_scan = measure(MEASURE_FRAMES, || foreground.full_scan());
    let foreground_index = measure(MEASURE_FRAMES, || foreground.indexed());
    assert_pair(&foreground_scan, &foreground_index);

    let mut always_compose = SongLuaStatePlanBenchmark::new(TREE_ACTORS);
    let mut changed_only = SongLuaStatePlanBenchmark::new(TREE_ACTORS);
    let settled_now = 4.0;
    let full_composition = measure(TREE_FRAMES, || {
        always_compose.planned_always_compose_frame(black_box(settled_now))
    });
    let changed_composition = measure(TREE_FRAMES, || {
        changed_only.planned_frame(black_box(settled_now))
    });
    assert_pair(&full_composition, &changed_composition);

    println!("gameplay Song Lua lifetime work");
    println!("future layer routing ({LAYERS} total, {ACTIVE_LAYERS} active, five frame passes)");
    print_result("five full scans", MEASURE_FRAMES, &full_layer_scans);
    print_result("activation cursor", MEASURE_FRAMES, &active_layer_cursor);
    println!("foreground ownership ({OVERLAYS} sprites, one matching owner)");
    print_result("all sprite paths", MEASURE_FRAMES, &foreground_scan);
    print_result("path owner index", MEASURE_FRAMES, &foreground_index);
    println!("settled overlay composition ({TREE_ACTORS} actors)");
    print_result("always descendants", TREE_FRAMES, &full_composition);
    print_result("changed descendants", TREE_FRAMES, &changed_composition);
}
