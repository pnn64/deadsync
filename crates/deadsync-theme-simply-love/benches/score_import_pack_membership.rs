use deadsync_theme_simply_love::screens::options::{
    selected_pack_group_contains_for_bench, selected_pack_group_contains_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const GROUPS: usize = 512;
const RUNS: usize = 20_000;

type Lookup = fn(&HashSet<String>, &str) -> bool;

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
    let selected = (0..GROUPS / 4)
        .map(|index| format!("pack group {index:04}"))
        .collect::<HashSet<_>>();
    let groups = (0..GROUPS)
        .map(|index| {
            if index % 2 == 0 {
                format!("Pack Group {:04}", index % (GROUPS / 2))
            } else {
                format!("pack group {:04}", index % (GROUPS / 2))
            }
        })
        .collect::<Vec<_>>();

    for group in &groups {
        assert_eq!(
            selected_pack_group_contains_legacy_for_bench(&selected, group),
            selected_pack_group_contains_for_bench(&selected, group)
        );
    }

    let old = measure(
        selected_pack_group_contains_legacy_for_bench,
        &selected,
        &groups,
    );
    let new = measure(selected_pack_group_contains_for_bench, &selected, &groups);
    assert_eq!(old.checksum, new.checksum);

    println!("score-import pack membership ({GROUPS} groups x {RUNS} runs)");
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

fn measure(lookup: Lookup, selected: &HashSet<String>, groups: &[String]) -> BenchResult {
    for _ in 0..100 {
        black_box(batch_checksum(lookup, selected, groups));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(5)
            ^ black_box(batch_checksum(lookup, selected, groups))
            ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn batch_checksum(lookup: Lookup, selected: &HashSet<String>, groups: &[String]) -> u64 {
    groups
        .iter()
        .enumerate()
        .fold(0_u64, |sum, (index, group)| {
            sum.rotate_left(1)
                ^ (lookup(black_box(selected), black_box(group.as_str())) as u64)
                ^ index as u64
        })
}

fn print_result(label: &str, result: &BenchResult) {
    let lookups = (GROUPS * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/lookup {:>7.2} cycles/lookup {:>7.1} Mlookups/s",
        result.elapsed.as_secs_f64() * 1.0e9 / lookups,
        result.cycles as f64 / lookups,
        lookups / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.3}/{:.3} per lookup, {:.1} bytes/lookup",
        result.alloc.allocs as f64 / lookups,
        result.alloc.reallocs as f64 / lookups,
        result.alloc.bytes as f64 / lookups,
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
