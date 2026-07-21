use deadsync_theme_simply_love::screens::gameplay::{
    GameplayHudTextBenchmarkCache, GameplayHudTextBenchmarkSnapshot,
    benchmark_gameplay_hud_text_legacy,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 20_000;
const MEASURE_FRAMES: usize = 2_000_000;

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

// SAFETY: all operations delegate to `System` with their original layouts;
// the atomics only observe successful allocation calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this layout to the global allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the arguments are forwarded unchanged to `System`.
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

fn inputs(frame: usize) -> (f64, f32) {
    let bpm = if frame % 20_000 < 10_000 {
        150.0
    } else {
        175.25
    };
    let life = if frame % 120 < 60 { 87.3 } else { 85.2 };
    (bpm, life)
}

struct BenchResult {
    elapsed: Duration,
    allocated: AllocSnapshot,
    checksum: usize,
}

fn checksum(snapshot: &GameplayHudTextBenchmarkSnapshot) -> usize {
    snapshot
        .bpm
        .len()
        .wrapping_add(snapshot.life.len())
        .wrapping_add(snapshot.overlay.len())
        .wrapping_add(snapshot.overlay_line_count)
}

fn measure(mut frame: impl FnMut(usize) -> GameplayHudTextBenchmarkSnapshot) -> BenchResult {
    for index in 0..WARMUP_FRAMES {
        let snapshot = frame(index);
        assert_eq!(snapshot.overlay.as_ref(), "AutoPlay");
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for index in 0..MEASURE_FRAMES {
        output_checksum = output_checksum.wrapping_add(checksum(&black_box(frame(index))));
    }
    BenchResult {
        elapsed: started.elapsed(),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<13} {:>9.2} ns/frame  {:>5.2} allocs/frame  {:>7.1} bytes/frame  \
         {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn main() {
    let legacy = measure(|frame| {
        let (bpm, life) = inputs(frame);
        benchmark_gameplay_hud_text_legacy(bpm, true, life, "AutoPlay")
    });
    let mut cache = GameplayHudTextBenchmarkCache::new("AutoPlay");
    let optimized = measure(|frame| {
        let (bpm, life) = inputs(frame);
        let snapshot = cache.snapshot(bpm, true, life);
        assert_eq!(
            snapshot.bpm.as_ref(),
            if bpm == 150.0 { "150" } else { "175.25" }
        );
        assert_eq!(
            snapshot.life.as_ref(),
            if life == 87.3 { "87.3%" } else { "85.2%" }
        );
        assert_eq!(snapshot.overlay.as_ref(), "AutoPlay");
        assert_eq!(snapshot.overlay_line_count, 1);
        snapshot
    });
    black_box((legacy.checksum, optimized.checksum));

    println!("gameplay HUD text benchmark");
    print_result("legacy frame", &legacy);
    print_result("cached frame", &optimized);
}
