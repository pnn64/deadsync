use deadsync_score::{
    collect_unique_import_chart_hashes_for_bench,
    collect_unique_import_chart_hashes_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 5_000;
const HASH_COUNT: usize = 512;
type Collector = for<'a> fn(&[&'a str], &HashSet<String>) -> Vec<String>;

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
    let owned = fixture_hashes();
    let hashes = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let existing_scores = (0..32)
        .map(|index| format!("chart-{index:04}"))
        .collect::<HashSet<_>>();

    assert_eq!(
        collect_unique_import_chart_hashes_legacy_for_bench(&hashes, &existing_scores),
        collect_unique_import_chart_hashes_for_bench(&hashes, &existing_scores)
    );
    let old = measure(
        &hashes,
        &existing_scores,
        collect_unique_import_chart_hashes_legacy_for_bench,
    );
    let new = measure(
        &hashes,
        &existing_scores,
        collect_unique_import_chart_hashes_for_bench,
    );
    assert_eq!(old.checksum, new.checksum);

    println!("score-import hash deduplication ({HASH_COUNT} hashes x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn fixture_hashes() -> Vec<String> {
    (0..HASH_COUNT)
        .map(|index| match index % 17 {
            0 => String::new(),
            1 => "   ".to_string(),
            2 => format!(" chart-{:04} ", index % 160),
            _ => format!("chart-{:04}", index % 160),
        })
        .collect()
}

fn measure(hashes: &[&str], existing_scores: &HashSet<String>, collect: Collector) -> BenchResult {
    for _ in 0..100 {
        black_box(collection_checksum(collect(hashes, existing_scores)));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7)
            ^ black_box(collection_checksum(collect(
                black_box(hashes),
                black_box(existing_scores),
            )))
            ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn collection_checksum(hashes: Vec<String>) -> u64 {
    hashes.iter().fold(hashes.len() as u64, |checksum, hash| {
        checksum.rotate_left(3) ^ hash.len() as u64
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (HASH_COUNT * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/hash {:>7.2} cycles/hash {:>7.1} Mhashes/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.3}/{:.3} per hash, {:.1} bytes/hash",
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
