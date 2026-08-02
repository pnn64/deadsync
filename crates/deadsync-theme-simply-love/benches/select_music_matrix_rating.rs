use deadsync_theme_simply_love::screens::select_music::SelectMusicMatrixRatingBench;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ITERATIONS: usize = 2_000_000;
const SAMPLES: usize = 7;

struct CountingAlloc {
    calls: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> (u64, u64) {
        (
            self.calls.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
        )
    }
}

// SAFETY: every request is forwarded unchanged to `System`; the atomics only
// observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes directly from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the live allocation and original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer and its original layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        out
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc_calls: u64,
    alloc_bytes: u64,
    checksum: u64,
}

fn main() {
    let bench = SelectMusicMatrixRatingBench::default();
    let rate = black_box(1.25);
    assert_eq!(bench.uncached(rate), bench.cached(rate));

    let old = best_sample(|| bench.uncached(black_box(rate)));
    let new = best_sample(|| bench.cached(black_box(rate)));
    assert_eq!(old.checksum, new.checksum);

    println!("settled Select Music Matrix rating ({ITERATIONS} frames, best of {SAMPLES})");
    print_result("uncached", &old);
    print_result("cached", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
    );
}

fn best_sample(mut rating: impl FnMut() -> f64) -> BenchResult {
    (0..SAMPLES)
        .map(|_| measure(&mut rating))
        .min_by_key(|result| result.cycles)
        .expect("at least one benchmark sample")
}

fn measure(rating: &mut impl FnMut() -> f64) -> BenchResult {
    let alloc_before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for index in 0..ITERATIONS {
        checksum = checksum.rotate_left(7) ^ black_box(rating()).to_bits() ^ index as u64;
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(cycles_before);
    let alloc_after = ALLOC.snapshot();
    BenchResult {
        elapsed,
        cycles,
        alloc_calls: alloc_after.0 - alloc_before.0,
        alloc_bytes: alloc_after.1 - alloc_before.1,
        checksum: black_box(checksum),
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<8} {:>8.2} cycles/frame {:>8.1} Mframes/s allocs={} bytes={}",
        result.cycles as f64 / ITERATIONS as f64,
        ITERATIONS as f64 / result.elapsed.as_secs_f64() / 1.0e6,
        result.alloc_calls,
        result.alloc_bytes,
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: `_rdtsc` and `_mm_lfence` are available on every x86-64 target.
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
