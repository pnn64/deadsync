use deadsync_theme_simply_love::screens::select_music::{
    sync_heat_value_range_for_bench, sync_heat_value_range_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const VALUES: usize = 32_768;
const RUNS: usize = 40;
type RangeFn = fn(&[f64], Option<(f64, f64)>) -> Option<(f64, f64)>;

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

// SAFETY: allocation requests are forwarded unchanged; atomics only observe them.
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
    let values = (0..VALUES)
        .map(|index| {
            let shuffled = index.wrapping_mul(15_485_863) % VALUES;
            ((shuffled as f64 * 0.071).sin() * 500.0) + (index % 97) as f64
        })
        .collect::<Vec<_>>();

    assert_eq!(
        sync_heat_value_range_legacy_for_bench(&values, Some((3.0, 97.0))),
        sync_heat_value_range_for_bench(&values, Some((3.0, 97.0)))
    );
    let old = measure(&values, sync_heat_value_range_legacy_for_bench);
    let new = measure(&values, sync_heat_value_range_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("sync heatmap percentile range ({VALUES} values x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
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

fn measure(values: &[f64], range: RangeFn) -> BenchResult {
    for _ in 0..2 {
        black_box(range(black_box(values), Some((3.0, 97.0))));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        let (lo, hi) = range(black_box(values), Some((3.0, 97.0))).unwrap();
        checksum = checksum.rotate_left(7) ^ lo.to_bits() ^ hi.to_bits() ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<4} {:>8.2} ms/run {:>12.0} cycles/run {:>7.1} Mrange-values/s",
        result.elapsed.as_secs_f64() * 1.0e3 / RUNS as f64,
        result.cycles as f64 / RUNS as f64,
        (VALUES * RUNS) as f64 / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per run, {:.1} KiB/run",
        result.alloc.allocs as f64 / RUNS as f64,
        result.alloc.reallocs as f64 / RUNS as f64,
        result.alloc.bytes as f64 / RUNS as f64 / 1024.0,
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
