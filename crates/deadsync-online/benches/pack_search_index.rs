use deadsync_online::stepmaniaonline::{
    pack_search_index_for_bench, pack_search_index_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 100_000;

#[derive(Clone, Copy)]
struct Fixture {
    id: u64,
    name: &'static str,
    metadata: [Option<&'static str>; 4],
}

const FIXTURES: [Fixture; 5] = [
    Fixture {
        id: 42,
        name: "Technical Spectrum",
        metadata: [Some("9MS"), Some("PAD"), Some("TECH"), Some("StepMania 5")],
    },
    Fixture {
        id: 18_492,
        name: "Community Stamina Collection",
        metadata: [Some("N/A"), Some("KEYBOARD"), Some("STAMINA"), None],
    },
    Fixture {
        id: u64::MAX,
        name: "İstanbul Über Mix",
        metadata: [Some("ÄSYNC"), None, Some("Σtyle"), None],
    },
    Fixture {
        id: 7,
        name: "Pack",
        metadata: [None, None, None, None],
    },
    Fixture {
        id: 20_000,
        name: "A Very Long Tournament Pack Name",
        metadata: [
            Some("12ms"),
            Some("pad and keyboard"),
            Some("technical crossover"),
            Some("StepMania 5.1"),
        ],
    },
];

type Indexer = fn(u64, &'static str, [Option<&'static str>; 4]) -> (String, String);

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
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
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
    for fixture in FIXTURES {
        assert_eq!(
            pack_search_index_legacy_for_bench(fixture.id, fixture.name, fixture.metadata),
            pack_search_index_for_bench(fixture.id, fixture.name, fixture.metadata)
        );
    }

    let old = measure(pack_search_index_legacy_for_bench);
    let new = measure(pack_search_index_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "SMO pack search-index construction ({} packs x {RUNS} runs)",
        FIXTURES.len()
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

fn measure(index: Indexer) -> BenchResult {
    for _ in 0..500 {
        black_box(batch_checksum(index));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(batch_checksum(index)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn batch_checksum(index: Indexer) -> u64 {
    FIXTURES.iter().fold(0_u64, |checksum, fixture| {
        let (normalized_name, search_text) = index(
            black_box(fixture.id),
            black_box(fixture.name),
            black_box(fixture.metadata),
        );
        checksum.rotate_left(3) ^ normalized_name.len() as u64 ^ (search_text.len() as u64) << 32
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (FIXTURES.len() * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/pack {:>7.2} cycles/pack {:>7.1} Mpacks/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per pack, {:.1} bytes/pack",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
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
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measurement interval.
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
