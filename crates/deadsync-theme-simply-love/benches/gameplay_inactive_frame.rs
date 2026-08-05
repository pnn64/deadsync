use deadsync_theme_simply_love::screens::gameplay::GameplayInactiveFrameBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const LAYER2_EVENTS: usize = 256;
const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 200_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

// SAFETY: every operation delegates unchanged to `System`; the relaxed
// counters only observe successful calls while measurement is enabled.
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
    ns_per_frame: f64,
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
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(frame()));
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
        "{label:<22} {:>9.2} ns/frame  {:>9.2} cycles/frame  {:>8.3} Mframe/s  \
         {:>5.2} allocs/frame  {:>7.1} bytes/frame  {:>5.2} reallocs/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        1_000.0 / result.ns_per_frame,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn assert_pair(legacy: &BenchResult, fast: &BenchResult) {
    assert_eq!(legacy.checksum, fast.checksum);
    assert_eq!(fast.allocated.allocs, 0);
    assert_eq!(fast.allocated.reallocs, 0);
    assert_eq!(fast.allocated.bytes, 0);
}

fn main() {
    let legacy = GameplayInactiveFrameBenchmark::new(LAYER2_EVENTS);
    let fast = GameplayInactiveFrameBenchmark::new(LAYER2_EVENTS);
    let expired_transition_now = 10.0;
    let legacy_transition =
        measure(|| legacy.expired_transition_legacy(black_box(expired_transition_now)));
    let fast_transition =
        measure(|| fast.expired_transition_compiled(black_box(expired_transition_now)));
    assert_pair(&legacy_transition, &fast_transition);

    let legacy = GameplayInactiveFrameBenchmark::new(LAYER2_EVENTS);
    let fast = GameplayInactiveFrameBenchmark::new(LAYER2_EVENTS);
    let expired_layer2_now = legacy.expired_layer2_now();
    let legacy_layer2 = measure(|| legacy.expired_layer2_legacy(black_box(expired_layer2_now)));
    let fast_layer2 = measure(|| fast.expired_layer2_cursor(black_box(expired_layer2_now)));
    assert_pair(&legacy_layer2, &fast_layer2);

    let mut legacy = GameplayInactiveFrameBenchmark::new(LAYER2_EVENTS);
    let mut fast = GameplayInactiveFrameBenchmark::new(LAYER2_EVENTS);
    let legacy_empty_layer = measure(|| legacy.empty_layer_legacy());
    let fast_empty_layer = measure(|| fast.empty_layer_fast());
    assert_pair(&legacy_empty_layer, &fast_empty_layer);

    println!("inactive gameplay frame work");
    println!("expired background transition");
    print_result("parse transition name", &legacy_transition);
    print_result("compiled expired flag", &fast_transition);
    println!("expired layer-2 animation ({LAYER2_EVENTS} prior events)");
    print_result("reverse history scan", &legacy_layer2);
    print_result("song cursor", &fast_layer2);
    println!("ordinary song empty Song Lua layer");
    print_result("prepare empty layer", &legacy_empty_layer);
    print_result("return before prepare", &fast_empty_layer);
}
