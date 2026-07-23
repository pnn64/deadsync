use deadsync_score::leaderboard::{
    LeaderboardEntry, prioritized_leaderboard_workload_for_bench,
    prioritized_leaderboard_workload_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ENTRIES: usize = 2_048;
const ROWS: usize = 10;
const RUNS: usize = 2_000;

type Workload = fn(&[LeaderboardEntry], usize) -> u64;

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
    let entries = fixture_entries();
    assert_eq!(
        prioritized_leaderboard_workload_legacy_for_bench(&entries, ROWS),
        prioritized_leaderboard_workload_for_bench(&entries, ROWS),
    );

    let old = measure(&entries, prioritized_leaderboard_workload_legacy_for_bench);
    let new = measure(&entries, prioritized_leaderboard_workload_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("leaderboard prioritization ({ENTRIES} entries, {ROWS} rows, {RUNS} runs)");
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

fn fixture_entries() -> Vec<LeaderboardEntry> {
    let unique_entries = ENTRIES / 2;
    (0..ENTRIES)
        .map(|index| {
            let duplicate_index = index % unique_entries;
            let rank = (duplicate_index * 977 % unique_entries + 1) as u32;
            let base_name = format!("player-{duplicate_index:04}");
            LeaderboardEntry {
                rank,
                name: if index < unique_entries {
                    base_name
                } else {
                    base_name.to_ascii_uppercase()
                },
                machine_tag: None,
                score: 10_000.0 - f64::from(rank),
                date: String::new(),
                is_rival: duplicate_index.is_multiple_of(37) || index.is_multiple_of(211),
                is_self: duplicate_index.is_multiple_of(251) || index.is_multiple_of(509),
                is_fail: false,
            }
        })
        .collect()
}

fn measure(entries: &[LeaderboardEntry], workload: Workload) -> BenchResult {
    for _ in 0..5 {
        black_box(workload(entries, ROWS));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum =
            checksum.rotate_left(7) ^ black_box(workload(black_box(entries), ROWS)) ^ run as u64;
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
    let visited_entries = (ENTRIES * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.3} ms/run {:>10.0} cycles/run {:>7.1} Mentries/s",
        result.elapsed.as_secs_f64() * 1.0e3 / runs,
        result.cycles as f64 / runs,
        visited_entries / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per run, {:.3} KiB/run",
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
