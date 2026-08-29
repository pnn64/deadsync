use deadsync_shell::{
    benchmark_stale_extract, benchmark_stale_extract_reference, benchmark_video_membership,
    benchmark_video_membership_reference,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;
const MEMBERSHIP_OPS: usize = 16_384;
const EXTRACT_OPS: usize = 1_024;
const ACTIVE_KEYS: usize = 8;
const RETAINED_KEYS: usize = 2;

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

fn measure(ops: usize, items: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..8 {
        black_box(op());
    }
    collect_samples(ops, items, |_| {
        let mut checksum = 0u64;
        for _ in 0..ops {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        checksum
    })
}

fn active_map() -> FxHashMap<String, u64> {
    (0..ACTIVE_KEYS)
        .map(|index| (format!("video/overlay-{index:02}.mp4"), index as u64 + 1))
        .collect()
}

fn measure_extract(reference: bool, desired: &FxHashSet<String>) -> Row {
    let mut warmup = active_map();
    if reference {
        black_box(benchmark_stale_extract_reference(&mut warmup, desired));
    } else {
        black_box(benchmark_stale_extract(&mut warmup, desired));
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let mut fixtures = (0..EXTRACT_OPS).map(|_| active_map()).collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        let mut checksum = 0u64;
        for active in &mut fixtures {
            checksum = checksum.wrapping_add(black_box(if reference {
                benchmark_stale_extract_reference(active, desired)
            } else {
                benchmark_stale_extract(active, desired)
            }));
        }
        let ns = started.elapsed().as_secs_f64() * 1e9 / EXTRACT_OPS as f64;
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        samples.push(Sample {
            ns,
            cycles: cycle_start
                .zip(cycle_end)
                .map(|(start, end)| end.wrapping_sub(start) as f64 / EXTRACT_OPS as f64),
            alloc: ALLOC.snapshot().delta(before),
            checksum,
        });
    }
    row_from_samples(samples, EXTRACT_OPS, ACTIVE_KEYS)
}

fn collect_samples(ops: usize, items: usize, mut sample: impl FnMut(usize) -> u64) -> Row {
    let mut samples = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        let checksum = sample(index);
        let ns = started.elapsed().as_secs_f64() * 1e9 / ops as f64;
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        samples.push(Sample {
            ns,
            cycles: cycle_start
                .zip(cycle_end)
                .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64),
            alloc: ALLOC.snapshot().delta(before),
            checksum,
        });
    }
    row_from_samples(samples, ops, items)
}

fn row_from_samples(mut samples: Vec<Sample>, ops: usize, items: usize) -> Row {
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

fn main() {
    let paths = (0..8)
        .map(|index| {
            if index < 6 {
                PathBuf::from(format!("video/overlay-{index:02}.mp4"))
            } else {
                PathBuf::from(format!("video/static-{index:02}.png"))
            }
        })
        .collect::<Vec<_>>();
    let active = (0..8)
        .map(|index| format!("video/overlay-{index:02}.mp4"))
        .collect::<Vec<_>>();
    let failed = (8..12)
        .map(|index| format!("video/overlay-{index:02}.mp4"))
        .collect::<Vec<_>>();
    let old_membership = measure(MEMBERSHIP_OPS, active.len() + failed.len(), || {
        benchmark_video_membership_reference(&paths, &active, &failed)
    });
    let new_membership = measure(MEMBERSHIP_OPS, active.len() + failed.len(), || {
        benchmark_video_membership(&paths, &active, &failed)
    });
    assert_eq!(old_membership.checksum, new_membership.checksum);
    print_pair(
        "song-video desired membership (8 paths, 12 probes)",
        &old_membership,
        &new_membership,
    );

    let desired = (0..RETAINED_KEYS)
        .map(|index| format!("video/overlay-{index:02}.mp4"))
        .collect::<FxHashSet<_>>();
    let old_extract = measure_extract(true, &desired);
    let new_extract = measure_extract(false, &desired);
    assert_eq!(old_extract.checksum, new_extract.checksum);
    print_pair(
        "stale media extraction (6 of 8 entries)",
        &old_extract,
        &new_extract,
    );
}

fn print_pair(name: &str, old: &Row, new: &Row) {
    println!("{name}");
    print_row("old", old);
    print_row("new", new);
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
        "  {label:<3} {:>10.1} ns  p95 {:>10.1} ns  {:>10.1} cycles  {:>10.0} item/s  \
         {:>6.1} alloc  {:>6.1} realloc  {:>6.1} free  {:>10.1} churn B/op",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.items as f64 * 1e9 / row.median_ns,
        row.alloc.allocs as f64 / row.ops as f64,
        row.alloc.reallocs as f64 / row.ops as f64,
        row.alloc.frees as f64 / row.ops as f64,
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
