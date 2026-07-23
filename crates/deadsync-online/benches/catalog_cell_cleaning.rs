use deadsync_online::srpg_shop::{
    clean_catalog_cell_for_bench, clean_catalog_cell_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 100_000;
const CELLS: [&str; 8] = [
    "<span class=\"song\">Black &amp; White</span>",
    "Difficulty: 14 | Speed Tier: 180 BPM",
    "<b>Purchase</b> &quot;technical&quot; song &apos;mix&apos;",
    "  Multiple\tspaces\r\nand an em\u{2003}space  ",
    "&lt;hidden&amp;gt; text after encoded markup",
    "&amp;lt;i&amp;gt;Nested entity tag&amp;lt;/i&amp;gt;",
    "Plain catalog title",
    "prefix > stray marker <unfinished",
];

type Cleaner = fn(&str) -> String;

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

// SAFETY: every operation is forwarded unchanged to `System`; atomics only
// observe successful allocations.
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
    for cell in CELLS {
        assert_eq!(
            clean_catalog_cell_legacy_for_bench(cell),
            clean_catalog_cell_for_bench(cell),
            "cell {cell:?}"
        );
    }

    let old = measure(clean_catalog_cell_legacy_for_bench);
    let new = measure(clean_catalog_cell_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "SRPG catalog cell cleaning ({} cells x {RUNS} runs)",
        CELLS.len()
    );
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

fn measure(clean: Cleaner) -> BenchResult {
    for _ in 0..500 {
        black_box(batch_checksum(clean));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(batch_checksum(clean)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn batch_checksum(clean: Cleaner) -> u64 {
    CELLS.iter().fold(0_u64, |sum, cell| {
        let cleaned = clean(black_box(cell));
        sum.rotate_left(3)
            ^ cleaned.len() as u64
            ^ cleaned.as_bytes().first().copied().unwrap_or(0) as u64
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let cells = (CELLS.len() * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/cell {:>7.2} cycles/cell {:>7.1} Mcells/s",
        result.elapsed.as_secs_f64() * 1.0e9 / cells,
        result.cycles as f64 / cells,
        cells / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per cell, {:.1} bytes/cell",
        result.alloc.allocs as f64 / cells,
        result.alloc.reallocs as f64 / cells,
        result.alloc.bytes as f64 / cells,
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
