use deadsync_score::{
    LeaderboardPane, ScoreboxPaneKind, scorebox_pane_kind, scorebox_pane_kind_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 500_000;

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
    let panes = [
        pane("SRPG Event", false),
        pane("Weekend RPG Challenge", false),
        pane("ITL Online 2026", false),
        pane("Mixed-Case iTl Tournament", false),
        pane("Custom EX Board", true),
        pane("Community Scores", false),
        pane("Ä Custom Board", false),
        pane("No Tournament Marker", false),
    ];
    for pane in &panes {
        assert_eq!(
            scorebox_pane_kind_legacy_for_bench(pane),
            scorebox_pane_kind(pane),
            "classification changed for {:?}",
            pane.name
        );
    }

    let old = measure(&panes, scorebox_pane_kind_legacy_for_bench);
    let new = measure(&panes, scorebox_pane_kind);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "scorebox pane classification ({} panes x {RUNS} runs)",
        panes.len()
    );
    print_result("old", panes.len(), &old);
    print_result("new", panes.len(), &new);
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

fn pane(name: &str, is_ex: bool) -> LeaderboardPane {
    LeaderboardPane {
        name: name.to_owned(),
        entries: Vec::new(),
        is_ex,
        disabled: false,
        personalized: false,
        arrowcloud_kind: None,
    }
}

fn measure(
    panes: &[LeaderboardPane],
    classify: fn(&LeaderboardPane) -> ScoreboxPaneKind,
) -> BenchResult {
    for _ in 0..1_000 {
        black_box(classification_checksum(panes, classify));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7)
            ^ black_box(classification_checksum(black_box(panes), classify))
            ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn classification_checksum(
    panes: &[LeaderboardPane],
    classify: fn(&LeaderboardPane) -> ScoreboxPaneKind,
) -> u64 {
    panes.iter().fold(0_u64, |checksum, pane| {
        checksum.rotate_left(5) ^ classify(black_box(pane)) as u64
    })
}

fn print_result(label: &str, pane_count: usize, result: &BenchResult) {
    let classifications = (pane_count * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/pane {:>7.2} cycles/pane {:>7.1} Mpanes/s",
        result.elapsed.as_secs_f64() * 1.0e9 / classifications,
        result.cycles as f64 / classifications,
        classifications / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per pane, {:.1} bytes/pane",
        result.alloc.allocs as f64 / classifications,
        result.alloc.reallocs as f64 / classifications,
        result.alloc.bytes as f64 / classifications,
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
