use deadsync_score::itl::{
    itl_classification_mask_for_bench, itl_classification_mask_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 500_000;
const CASES: [(&str, &str, &str); 8] = [
    ("ITL Online 2026", "(NO CMOD)", "dance-double"),
    ("Some itl 2026 Folder", "", "DANCE-SINGLE"),
    ("Custom Pack", "No marker", "pump-double"),
    ("Prélude ITL ONLINE 2026", "No Cmod α", "DOUBLE-β"),
    ("itl online 2025", "nocmod", "couple"),
    ("Regional ITL 2026 Unlocks", "NO CMOD", "routine"),
    ("International League", "mods allowed", "dance-solo"),
    (
        "Long Custom Tournament Folder Name",
        "This chart has no cmod marker",
        "pump-halfdouble",
    ),
];

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
    for (group_name, subtitle, chart_type) in CASES {
        assert_eq!(
            itl_classification_mask_legacy_for_bench(group_name, subtitle, chart_type),
            itl_classification_mask_for_bench(group_name, subtitle, chart_type),
        );
    }

    let old = measure(itl_classification_mask_legacy_for_bench);
    let new = measure(itl_classification_mask_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "ITL catalog classification ({} cases x {RUNS} runs)",
        CASES.len()
    );
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn measure(classify: fn(&str, &str, &str) -> u8) -> BenchResult {
    for _ in 0..1_000 {
        black_box(batch_checksum(classify));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(batch_checksum(classify)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn batch_checksum(classify: fn(&str, &str, &str) -> u8) -> u64 {
    CASES
        .iter()
        .fold(0_u64, |checksum, (group, subtitle, chart_type)| {
            checksum.rotate_left(3)
                ^ u64::from(classify(
                    black_box(group),
                    black_box(subtitle),
                    black_box(chart_type),
                ))
        })
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (CASES.len() * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/case {:>7.2} cycles/case {:>7.1} Mcases/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per case, {:.1} bytes/case",
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
            new.alloc.allocs + new.alloc.reallocs,
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
