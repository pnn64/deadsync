use deadsync_score::{LeaderboardEntry, LeaderboardPane};
use deadsync_theme_simply_love::screens::components::shared::gs_scorebox::{
    leaderboard_rows_checksum_for_bench, leaderboard_rows_checksum_legacy_for_bench,
};
use deadsync_theme_simply_love::views::ScoreboxSideView;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 20_000;
const ENTRY_COUNT: usize = 128;
type Select = fn(&ScoreboxSideView, &LeaderboardPane) -> u64;

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
    let view = ScoreboxSideView::default();
    let pane = fixture_pane();
    assert_eq!(
        leaderboard_rows_checksum_legacy_for_bench(&view, &pane),
        leaderboard_rows_checksum_for_bench(&view, &pane),
    );

    let old = measure(&view, &pane, leaderboard_rows_checksum_legacy_for_bench);
    let new = measure(&view, &pane, leaderboard_rows_checksum_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("live leaderboard row selection ({ENTRY_COUNT} entries x {RUNS} frames)");
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn fixture_pane() -> LeaderboardPane {
    let entries = (0..ENTRY_COUNT)
        .map(|index| LeaderboardEntry {
            rank: index as u32 + 1,
            name: format!("Player Name {index:03}"),
            machine_tag: Some(format!("M{index:03}")),
            score: 10_000.0 - index as f64,
            date: "2026-07-19 12:34:56".to_string(),
            is_rival: index % 17 == 0,
            is_self: index == 97,
            is_fail: false,
        })
        .collect();
    LeaderboardPane {
        name: "GrooveStats".to_string(),
        entries,
        is_ex: false,
        disabled: false,
        personalized: true,
        arrowcloud_kind: None,
    }
}

fn measure(view: &ScoreboxSideView, pane: &LeaderboardPane, select: Select) -> BenchResult {
    for _ in 0..100 {
        black_box(select(black_box(view), black_box(pane)));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7)
            ^ black_box(select(black_box(view), black_box(pane)))
            ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = RUNS as f64;
    println!(
        "  {label:<4} {:>8.2} ns/frame {:>8.2} cycles/frame {:>7.1} Kframes/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e3,
    );
    println!(
        "       alloc/realloc={:.1}/{:.1} per frame, {:.1} KiB/frame",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations / 1024.0,
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
