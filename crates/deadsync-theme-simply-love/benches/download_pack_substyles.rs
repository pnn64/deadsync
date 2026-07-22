use deadsync_theme_simply_love::screens::options::{
    aggregate_substyles_for_bench, aggregate_substyles_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ENTRIES: usize = 4_096;
const UNIQUE_SUBSTYLES: usize = 512;
const RUNS: usize = 100;

type Aggregator = for<'a> fn(&[Option<&'a str>]) -> (Vec<(String, usize)>, usize);

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
    let owned = (0..ENTRIES)
        .map(|index| {
            if index % 31 == 0 {
                return None;
            }
            let style = index % UNIQUE_SUBSTYLES;
            Some(if index % 7 == 0 {
                format!(" STYLE-{style:04} ")
            } else {
                format!("style-{style:04}")
            })
        })
        .collect::<Vec<_>>();
    let fixture = owned
        .iter()
        .map(|value| value.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(
        aggregate_substyles_legacy_for_bench(&fixture),
        aggregate_substyles_for_bench(&fixture)
    );

    let old = measure(aggregate_substyles_legacy_for_bench, &fixture);
    let new = measure(aggregate_substyles_for_bench, &fixture);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "download-pack substyle aggregation ({ENTRIES} entries, {UNIQUE_SUBSTYLES} unique, {RUNS} runs)"
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

fn measure(aggregate: Aggregator, fixture: &[Option<&str>]) -> BenchResult {
    for _ in 0..4 {
        black_box(checksum(aggregate(black_box(fixture))));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut digest = 0_u64;
    for run in 0..RUNS {
        digest =
            digest.rotate_left(7) ^ black_box(checksum(aggregate(black_box(fixture)))) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: digest,
    }
}

fn checksum((named, uncategorized): (Vec<(String, usize)>, usize)) -> u64 {
    named
        .into_iter()
        .fold(uncategorized as u64, |sum, (name, count)| {
            sum.rotate_left(3) ^ name.len() as u64 ^ (count as u64) << 32
        })
}

fn print_result(label: &str, result: &BenchResult) {
    let rebuilds = RUNS as f64;
    let entries = (RUNS * ENTRIES) as f64;
    println!(
        "  {label:<4} {:>8.2} us/rebuild {:>10.0} cycles/rebuild {:>7.2} Mentries/s",
        result.elapsed.as_secs_f64() * 1.0e6 / rebuilds,
        result.cycles as f64 / rebuilds,
        entries / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.1}/{:.1} per rebuild, {:.1} KiB/rebuild",
        result.alloc.allocs as f64 / rebuilds,
        result.alloc.reallocs as f64 / rebuilds,
        result.alloc.bytes as f64 / rebuilds / 1024.0,
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
