use deadsync_profile::{JudgmentGraphic, Profile};
use deadsync_theme_simply_love::screens::components::gameplay::notefield::{
    JudgmentAssetsBenchmarkCache, benchmark_resolve_judgment_assets,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_ITERATIONS: usize = 20_000;
const MEASURE_ITERATIONS: usize = 2_000_000;

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

// SAFETY: every operation delegates to `System` with the original allocation
// layout and only observes successful calls through independent atomics.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` are forwarded unchanged to `System`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: `ptr`, `old`, and `new_size` are forwarded to `System`.
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
    alloc: AllocSnapshot,
    checksum: usize,
}

fn measure(mut resolve: impl FnMut() -> usize) -> BenchResult {
    for _ in 0..WARMUP_ITERATIONS {
        black_box(resolve());
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..MEASURE_ITERATIONS {
        checksum = checksum.wrapping_add(black_box(resolve()));
    }
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn snapshot_checksum(
    snapshot: deadsync_theme_simply_love::screens::components::gameplay::notefield::JudgmentAssetsBenchmarkSnapshot,
) -> usize {
    snapshot
        .frame_cols
        .wrapping_add(snapshot.frame_rows)
        .wrapping_add(snapshot.frame_size[0].to_bits() as usize)
        .wrapping_add(snapshot.frame_size[1].to_bits() as usize)
        .wrapping_add(snapshot.hold_judgment_key.is_some() as usize)
        .wrapping_add(snapshot.held_miss_key.is_some() as usize)
}

fn print_result(label: &str, result: &BenchResult) {
    let iterations = MEASURE_ITERATIONS as f64;
    println!(
        "{label:<18} {:>9.2} ns/iteration  {:>5.2} allocs/iteration  \
         {:>7.1} bytes/iteration  {:>5.2} reallocs/iteration",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / iterations,
        result.alloc.allocs as f64 / iterations,
        result.alloc.bytes as f64 / iterations,
        result.alloc.reallocs as f64 / iterations,
    );
}

fn main() {
    deadsync_assets::register_texture_dims(JudgmentGraphic::DEFAULT_KEY, 512, 896);
    let profile = Profile::default();
    let expected = benchmark_resolve_judgment_assets(&profile);
    let cached = JudgmentAssetsBenchmarkCache::new(&profile);
    assert_eq!(cached.snapshot(), expected);

    let legacy = measure(|| {
        let snapshot = benchmark_resolve_judgment_assets(&profile);
        assert_eq!(snapshot, expected);
        snapshot_checksum(snapshot)
    });
    let optimized = measure(|| {
        let snapshot = cached.snapshot();
        assert_eq!(snapshot, expected);
        snapshot_checksum(snapshot)
    });
    black_box((legacy.checksum, optimized.checksum));

    println!("judgment asset resolution benchmark");
    print_result("legacy per-frame", &legacy);
    print_result("cached per-frame", &optimized);
}
