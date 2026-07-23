use deadlib_assets::upload::{
    texture_upload_queue_workload_for_bench, texture_upload_queue_workload_legacy_for_bench,
};
use image::RgbaImage;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 10_000;
const REPLACEMENTS: usize = 6;
const KEYS: [&str; 16] = [
    "dynamic/video/banner-00",
    "dynamic/video/banner-01",
    "dynamic/video/background-00",
    "dynamic/video/background-01",
    "generated/notes/player-1/tap",
    "generated/notes/player-1/hold",
    "generated/notes/player-2/tap",
    "generated/notes/player-2/hold",
    "runtime/overlay/lifebar-1",
    "runtime/overlay/lifebar-2",
    "runtime/overlay/progress-1",
    "runtime/overlay/progress-2",
    "cache/song/banner-current",
    "cache/song/background-current",
    "cache/song/cdtitle-current",
    "cache/song/preview-current",
];

type Workload = fn(&[&str], usize, &Arc<RgbaImage>) -> u64;

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

// SAFETY: allocation operations are forwarded unchanged to `System`; the
// independent atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
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
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let image = Arc::new(RgbaImage::new(64, 32));
    assert_eq!(
        texture_upload_queue_workload_legacy_for_bench(&KEYS, REPLACEMENTS, &image),
        texture_upload_queue_workload_for_bench(&KEYS, REPLACEMENTS, &image)
    );

    let old = measure(texture_upload_queue_workload_legacy_for_bench, &image);
    let new = measure(texture_upload_queue_workload_for_bench, &image);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "texture upload queue ({} keys x {} updates x one budget deferral x {RUNS} runs)",
        KEYS.len(),
        REPLACEMENTS + 1,
    );
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs,
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn measure(workload: Workload, image: &Arc<RgbaImage>) -> BenchResult {
    for _ in 0..100 {
        black_box(workload(&KEYS, REPLACEMENTS, image));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum =
            checksum.rotate_left(7) ^ black_box(workload(&KEYS, REPLACEMENTS, image)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let lifecycles = (KEYS.len() * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/key {:>7.2} cycles/key {:>7.1} Mkeys/s",
        result.elapsed.as_secs_f64() * 1.0e9 / lifecycles,
        result.cycles as f64 / lifecycles,
        lifecycles / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per key, {:.1} bytes/key",
        result.alloc.allocs as f64 / lifecycles,
        result.alloc.reallocs as f64 / lifecycles,
        result.alloc.bytes as f64 / lifecycles,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        return 0.0;
    }
    100.0 * (1.0 - new as f64 / old as f64)
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: timestamp reads and fences do not access memory.
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
