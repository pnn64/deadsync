use deadlib_assets::dynamic::{
    dynamic_path_classification_for_bench, dynamic_path_classification_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 250_000;
const PATHS: [&str; 12] = [
    "Songs/Pack/Song/banner.PNG",
    "Songs/Pack/Song/background.jpeg",
    "Songs/Pack/Song/movie.MP4",
    "Songs/Pack/Song/loop.WeBm",
    "Songs/Pack/Song/legacy.AVI",
    "Songs/Pack/Song/texture.tiff",
    "Songs/Pack/Song/chart.ssc",
    "Songs/Pack/Song/script.lua",
    "Songs/Pack/Song/unknown.bin",
    "Songs/Pack/Song/extensionless",
    "Songs/Pack/Song/trailing.",
    "Songs/Pack/Song/unicode.ÉPNG",
];

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

// SAFETY: all operations are forwarded unchanged to `System`; atomics only
// observe successful allocation operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this layout to the global allocator.
        let output = unsafe { System.alloc(layout) };
        if !output.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        output
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let output = unsafe { System.realloc(ptr, old, new_size) };
        if !output.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        output
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
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    for path in PATHS.map(Path::new) {
        assert_eq!(
            dynamic_path_classification_legacy_for_bench(path),
            dynamic_path_classification_for_bench(path)
        );
    }
    let old = measure(dynamic_path_classification_legacy_for_bench);
    let new = measure(dynamic_path_classification_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "dynamic-media path classification ({} paths x {RUNS} runs)",
        PATHS.len()
    );
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn measure(classify: fn(&Path) -> u8) -> BenchResult {
    for _ in 0..1_000 {
        black_box(batch_checksum(classify));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(batch_checksum(classify)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn batch_checksum(classify: fn(&Path) -> u8) -> u64 {
    PATHS.iter().fold(0_u64, |checksum, path| {
        checksum.rotate_left(3) ^ u64::from(classify(black_box(Path::new(path))))
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (PATHS.len() * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/path {:>7.2} cycles/path {:>7.1} Mpaths/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per path, {:.1} bytes/path",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
    );
}

fn print_reduction(old: &BenchResult, new: &BenchResult) {
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads only serialize measurement.
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
