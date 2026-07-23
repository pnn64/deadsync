use deadsync_online::stepmaniaonline::{PackInfo, search_catalog, search_catalog_legacy_for_bench};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACKS: usize = 10_000;
const RUNS: usize = 100;
const SAMPLES: usize = 7;
const QUERIES: [&str; 8] = [
    "technical",
    "TECHNICAL",
    "spectrum technical",
    "keyboard stamina",
    "pack 0042",
    "crossover",
    "lowercase",
    "not present",
];

type Search = fn(&[PackInfo], &str) -> Vec<usize>;

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

#[derive(Clone, Copy)]
struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let catalog = fixture_catalog();
    for query in QUERIES {
        assert_eq!(
            search_catalog_legacy_for_bench(&catalog, query),
            search_catalog(&catalog, query),
            "query {query:?}"
        );
    }

    let (old, new) = measure_pair(&catalog);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "SMO catalog search ({PACKS} packs x {} queries x {RUNS} runs, median of {SAMPLES} alternating samples)",
        QUERIES.len(),
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

fn measure_pair(catalog: &[PackInfo]) -> (BenchResult, BenchResult) {
    let mut old = Vec::with_capacity(SAMPLES);
    let mut new = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        if sample % 2 == 0 {
            old.push(measure(catalog, search_catalog_legacy_for_bench));
            new.push(measure(catalog, search_catalog));
        } else {
            new.push(measure(catalog, search_catalog));
            old.push(measure(catalog, search_catalog_legacy_for_bench));
        }
    }
    (median(old), median(new))
}

fn median(mut samples: Vec<BenchResult>) -> BenchResult {
    samples.sort_unstable_by_key(|sample| sample.cycles);
    samples[samples.len() / 2]
}

fn fixture_catalog() -> Vec<PackInfo> {
    (0..PACKS)
        .map(|index| {
            let family = match index % 5 {
                0 => "Technical Spectrum",
                1 => "Stamina Collection",
                2 => "Crossover Sessions",
                3 => "Lowercase Anthology",
                _ => "Community Mix",
            };
            PackInfo::new(
                index as u64,
                format!("{family} Pack {index:04}"),
                20,
                100_000_000,
                Some(if index % 3 == 0 { "9ms" } else { "n/a" }.to_string()),
                Some(if index % 2 == 0 { "pad" } else { "keyboard" }.to_string()),
                Some(
                    if index % 5 == 1 {
                        "stamina"
                    } else {
                        "technical"
                    }
                    .to_string(),
                ),
                Some("StepMania 5".to_string()),
            )
        })
        .collect()
}

fn measure(catalog: &[PackInfo], search: Search) -> BenchResult {
    for _ in 0..5 {
        black_box(batch_checksum(catalog, search));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7)
            ^ black_box(batch_checksum(black_box(catalog), search))
            ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn batch_checksum(catalog: &[PackInfo], search: Search) -> u64 {
    QUERIES.iter().fold(0_u64, |checksum, query| {
        let results = search(catalog, black_box(query));
        checksum.rotate_left(3)
            ^ results.len() as u64
            ^ results.first().copied().unwrap_or(0) as u64
            ^ (results.last().copied().unwrap_or(0) as u64) << 32
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let searches = (QUERIES.len() * RUNS) as f64;
    let rows = searches * PACKS as f64;
    println!(
        "  {label:<4} {:>8.2} us/search {:>9.0} cycles/search {:>7.1} Mrows/s",
        result.elapsed.as_secs_f64() * 1.0e6 / searches,
        result.cycles as f64 / searches,
        rows / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per search, {:.1} KiB/search",
        result.alloc.allocs as f64 / searches,
        result.alloc.reallocs as f64 / searches,
        result.alloc.bytes as f64 / searches / 1024.0,
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
