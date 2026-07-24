use deadsync_score::{
    ac_score_cache_snapshot_for_bench, ac_score_cache_snapshot_legacy_for_bench,
    gs_score_cache_snapshot_for_bench, gs_score_cache_snapshot_legacy_for_bench,
    local_score_cache_snapshot_for_bench, local_score_cache_snapshot_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ENTRIES: usize = 1_024;
const UPDATES: usize = 32;
const RUNS: usize = 64;

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
    compare(
        "GrooveStats cache snapshots",
        gs_score_cache_snapshot_legacy_for_bench,
        gs_score_cache_snapshot_for_bench,
    );
    compare(
        "ArrowCloud cache snapshots",
        ac_score_cache_snapshot_legacy_for_bench,
        ac_score_cache_snapshot_for_bench,
    );
    compare(
        "local score cache snapshots",
        local_score_cache_snapshot_legacy_for_bench,
        local_score_cache_snapshot_for_bench,
    );
}

fn compare(label: &str, old_work: fn(usize, usize) -> u64, new_work: fn(usize, usize) -> u64) {
    assert_eq!(
        old_work(ENTRIES, UPDATES),
        new_work(ENTRIES, UPDATES),
        "{label} behavior changed"
    );

    let old = measure(old_work);
    let new = measure(new_work);
    assert_eq!(old.checksum, new.checksum);

    println!("{label} ({ENTRIES} entries, {UPDATES} updates x {RUNS} runs)");
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

fn measure(work: fn(usize, usize) -> u64) -> BenchResult {
    for _ in 0..2 {
        black_box(work(ENTRIES, UPDATES));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(work(ENTRIES, UPDATES)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let snapshots = (UPDATES * RUNS) as f64;
    println!(
        "  {label:<4} {:>9.1} ns/snapshot {:>9.1} cycles/snapshot {:>7.1} Ksnapshots/s",
        result.elapsed.as_secs_f64() * 1.0e9 / snapshots,
        result.cycles as f64 / snapshots,
        snapshots / result.elapsed.as_secs_f64() / 1.0e3,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per snapshot, {:.1} bytes/snapshot",
        result.alloc.allocs as f64 / snapshots,
        result.alloc.reallocs as f64 / snapshots,
        result.alloc.bytes as f64 / snapshots,
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
