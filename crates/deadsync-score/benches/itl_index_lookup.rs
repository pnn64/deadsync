use deadsync_score::itl::{
    ItlIndexBenchEntry, itl_index_lookup_workload_for_bench,
    itl_index_lookup_workload_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ENTRIES: usize = 2_048;
const QUERY_PATHS: usize = 129;
const PASSES: usize = 64;
const RUNS: usize = 300;

type Workload = fn(&[ItlIndexBenchEntry<'_>], &[&str], usize) -> u64;

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

struct OwnedEntry {
    path: String,
    hash: String,
    ex: u32,
    points: u32,
    unlocked: bool,
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let owned_entries = fixture_entries();
    let entries = owned_entries
        .iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.hash.as_str(),
                entry.ex,
                entry.points,
                entry.unlocked,
            )
        })
        .collect::<Vec<_>>();
    let owned_queries = fixture_queries(&owned_entries);
    let queries = owned_queries.iter().map(String::as_str).collect::<Vec<_>>();

    assert_eq!(entries.len(), ENTRIES);
    assert_eq!(queries.len(), QUERY_PATHS);
    assert_eq!(
        itl_index_lookup_workload_legacy_for_bench(&entries, &queries, PASSES),
        itl_index_lookup_workload_for_bench(&entries, &queries, PASSES),
    );

    let old = measure(
        &entries,
        &queries,
        itl_index_lookup_workload_legacy_for_bench,
    );
    let new = measure(&entries, &queries, itl_index_lookup_workload_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "ITL profile indexes ({ENTRIES} entries, {QUERY_PATHS} query paths x {PASSES} passes x {RUNS} runs)"
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

fn fixture_entries() -> Vec<OwnedEntry> {
    (0..ENTRIES)
        .map(|index| OwnedEntry {
            path: format!(
                "/Songs/ITL Online 2026/Pack {:02}/Song {index:04}",
                index % 48
            ),
            hash: format!("{:016x}{:016x}", index, index.wrapping_mul(0x9e37_79b9)),
            ex: 7_500 + (index as u32 * 37) % 2_501,
            points: 5_000 + (index as u32 * 113) % 20_000,
            unlocked: index % 5 != 0,
        })
        .collect()
}

fn fixture_queries(entries: &[OwnedEntry]) -> Vec<String> {
    let mut queries = Vec::with_capacity(QUERY_PATHS);
    for index in 0..QUERY_PATHS - 1 {
        let entry = &entries[index * 977 % entries.len()];
        queries.push(entry.path.clone());
    }
    queries.push("/Songs/ITL Online 2026/Missing Pack/Ghost Song".to_owned());
    queries
}

fn measure(
    entries: &[ItlIndexBenchEntry<'_>],
    queries: &[&str],
    workload: Workload,
) -> BenchResult {
    for _ in 0..5 {
        black_box(workload(entries, queries, PASSES));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum =
            checksum.rotate_left(7) ^ black_box(workload(entries, queries, PASSES)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = RUNS as f64;
    let queries = (QUERY_PATHS * PASSES * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ms/run {:>9.0} cycles/run {:>7.1} Mqueries/s",
        result.elapsed.as_secs_f64() * 1.0e3 / runs,
        result.cycles as f64 / runs,
        queries / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.1}/{:.1} per run, {:.1} KiB/run",
        result.alloc.allocs as f64 / runs,
        result.alloc.reallocs as f64 / runs,
        result.alloc.bytes as f64 / runs / 1024.0,
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
