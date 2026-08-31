use deadsync_shell::{
    BenchmarkSyncEventRouter, benchmark_sync_finished_owner_filter,
    benchmark_sync_finished_owner_filter_reference, benchmark_sync_route_reference,
};
use deadsync_theme_simply_love::SimplyLoveSyncOwner;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;
const ROUTE_OPS: usize = 4_096;
const FILTER_OPS: usize = 65_536;
const EVENTS_PER_BATCH: usize = 63;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocator requests are delegated unchanged to `System`; relaxed
// counters observe only this single-threaded benchmark while enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: the pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer-layout pair came from the allocator caller.
        let new_ptr = unsafe { System.realloc(ptr, old, new_size) };
        if !new_ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(old.size() as u64, Ordering::Relaxed);
        }
        new_ptr
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy)]
struct Sample {
    ns: f64,
    cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
    ops: usize,
    items: usize,
}

fn measure(ops: usize, items: usize, mut operation: impl FnMut() -> u64) -> Row {
    for _ in 0..8 {
        black_box(operation());
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        let mut checksum = 0_u64;
        for _ in 0..ops {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        samples.push(Sample {
            ns: elapsed.as_secs_f64() * 1e9 / ops as f64,
            cycles: cycle_start
                .zip(cycle_end)
                .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64),
            alloc: ALLOC.snapshot().delta(before),
            checksum,
        });
    }
    samples.sort_by(|a, b| a.ns.total_cmp(&b.ns));
    let median = samples[SAMPLES / 2];
    let mut cycles = samples
        .iter()
        .filter_map(|sample| sample.cycles)
        .collect::<Vec<_>>();
    cycles.sort_by(f64::total_cmp);
    Row {
        median_ns: median.ns,
        p95_ns: samples[SAMPLES * 95 / 100].ns,
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: median.alloc,
        checksum: median.checksum,
        ops,
        items,
    }
}

fn measure_route(owners: &[SimplyLoveSyncOwner], reference: bool) -> Row {
    let mut router = BenchmarkSyncEventRouter::default();
    measure(ROUTE_OPS, owners.len(), || {
        if reference {
            benchmark_sync_route_reference(black_box(owners))
        } else {
            router.route(black_box(owners))
        }
    })
}

fn main() {
    let song_only = vec![SimplyLoveSyncOwner::SelectMusicSong; EVENTS_PER_BATCH];
    run_pair(
        "song stream routing (63 events)",
        measure_route(&song_only, true),
        measure_route(&song_only, false),
    );

    let balanced = (0..EVENTS_PER_BATCH)
        .map(|index| match index % 3 {
            0 => SimplyLoveSyncOwner::SelectMusicSong,
            1 => SimplyLoveSyncOwner::SelectMusicPack,
            _ => SimplyLoveSyncOwner::OptionsPack,
        })
        .collect::<Vec<_>>();
    run_pair(
        "balanced owner routing (63 events)",
        measure_route(&balanced, true),
        measure_route(&balanced, false),
    );

    let active = [
        SimplyLoveSyncOwner::SelectMusicSong,
        SimplyLoveSyncOwner::SelectMusicPack,
        SimplyLoveSyncOwner::OptionsPack,
    ];
    let finished = [
        SimplyLoveSyncOwner::SelectMusicSong,
        SimplyLoveSyncOwner::OptionsPack,
    ];
    let old = measure(FILTER_OPS, active.len(), || {
        benchmark_sync_finished_owner_filter_reference(black_box(&active), black_box(&finished))
    });
    let new = measure(FILTER_OPS, active.len(), || {
        benchmark_sync_finished_owner_filter(black_box(&active), black_box(&finished))
    });
    run_pair("finished-owner filtering (3 jobs)", old, new);
}

fn run_pair(name: &str, old: Row, new: Row) {
    assert_eq!(old.checksum, new.checksum, "{name} behavior changed");
    assert!(
        new.alloc.allocs < old.alloc.allocs,
        "{name} did not reduce allocations"
    );
    assert!(
        new.alloc.churn() < old.alloc.churn(),
        "{name} did not reduce allocator churn"
    );
    assert!(
        new.median_ns < old.median_ns,
        "{name} did not improve median latency"
    );
    assert!(
        new.p95_ns < old.p95_ns,
        "{name} did not improve p95 latency"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{name} did not reduce CPU cycles");
    }

    println!("{name}");
    print_row("old", &old);
    print_row("new", &new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% allocs  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    println!(
        "  {label:<3} {:>9.1} ns  p95 {:>9.1} ns  {:>9.1} cycles  {:>8.2} Mitem/s  \
         {:>5.1} alloc {:>5.1} realloc {:>5.1} free  {:>9.1} allocated B/op  {:>9.1} churn B/op",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.items as f64 * 1e3 / row.median_ns,
        row.alloc.allocs as f64 / row.ops as f64,
        row.alloc.reallocs as f64 / row.ops as f64,
        row.alloc.frees as f64 / row.ops as f64,
        row.alloc.alloc_bytes as f64 / row.ops as f64,
        row.alloc.churn() as f64 / row.ops as f64,
    );
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: fences and timestamp reads have no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn cycle_counter() -> Option<u64> {
    None
}
