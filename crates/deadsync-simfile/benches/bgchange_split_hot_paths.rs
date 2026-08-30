use deadsync_simfile::bgchanges::bench_support::{
    split_bgchange_sets_new, split_bgchange_sets_old,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation calls delegate unchanged to `System`; relaxed counters
// observe successful operations only while this single-threaded benchmark
// enables them.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn measure(ops_per_sample: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..4 {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..ops_per_sample {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        times.push(elapsed.as_secs_f64() * 1_000_000_000.0 / ops_per_sample as f64);
        cycles.push(cycle_start.zip(cycle_end).map_or(f64::NAN, |(start, end)| {
            end.wrapping_sub(start) as f64 / ops_per_sample as f64
        }));
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: percentile(&times, 0.5),
        p95_ns: percentile(&times, 0.95),
        median_cycles: percentile(&cycles, 0.5),
        allocated: ALLOC.snapshot().delta(before),
        checksum: checksum.wrapping_add(allocation_checksum),
    }
}

fn split_checksum(sets: &[Vec<String>]) -> u64 {
    let fields = sets.iter().map(Vec::len).sum::<usize>();
    let bytes = sets
        .iter()
        .flat_map(|set| set.iter())
        .map(String::len)
        .sum::<usize>();
    let first = sets
        .first()
        .and_then(|set| set.first())
        .and_then(|field| field.as_bytes().first())
        .copied()
        .unwrap_or_default();
    let last = sets
        .last()
        .and_then(|set| set.last())
        .and_then(|field| field.as_bytes().last())
        .copied()
        .unwrap_or_default();
    (sets.len() as u64)
        ^ (fields as u64).rotate_left(13)
        ^ (bytes as u64).rotate_left(29)
        ^ u64::from(first).rotate_left(41)
        ^ u64::from(last).rotate_left(53)
}

fn print_result(label: &str, result: &BenchResult, sets: usize) {
    println!(
        "{label:<9} {:>10.1} ns median  {:>10.1} ns p95  {:>10.1} cycles  \
         {:>9.1} Kset/s  {:>4} alloc  {:>3} realloc  {:>4} free  {:>7} B alloc  {:>7} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles,
        sets as f64 * 1_000_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
}

fn benchmark_pair(title: &str, changes: &str, entries: &[String], ops: usize) {
    let expected = split_bgchange_sets_old(changes, entries);
    let actual = split_bgchange_sets_new(changes, entries);
    assert_eq!(actual, expected, "{title} behavior diverged");
    let set_count = actual.len();

    let old = measure(ops, || {
        let sets = split_bgchange_sets_old(black_box(changes), black_box(entries));
        black_box(&sets);
        split_checksum(&sets)
    });
    let new = measure(ops, || {
        let sets = split_bgchange_sets_new(black_box(changes), black_box(entries));
        black_box(&sets);
        split_checksum(&sets)
    });
    assert_eq!(old.checksum, new.checksum, "{title} checksum diverged");

    println!("\n{title}");
    print_result("old", &old, set_count);
    print_result("new", &new, set_count);
    println!(
        "change    {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        100.0 * (1.0 - new.median_ns / old.median_ns),
        100.0 * (1.0 - new.p95_ns / old.p95_ns),
        100.0 * (1.0 - new.median_cycles / old.median_cycles),
        100.0 * (1.0 - new.allocated.allocated_bytes as f64 / old.allocated.allocated_bytes as f64),
        100.0 * (1.0 - new.allocated.churn_bytes() as f64 / old.allocated.churn_bytes() as f64),
    );

    assert!(new.median_ns < old.median_ns, "{title}: median regressed");
    assert!(new.p95_ns < old.p95_ns, "{title}: p95 regressed");
    if old.median_cycles.is_finite() && new.median_cycles.is_finite() {
        assert!(
            new.median_cycles < old.median_cycles,
            "{title}: cycles regressed"
        );
    }
    assert!(
        new.allocated.allocs <= old.allocated.allocs,
        "{title}: allocations regressed"
    );
    assert!(
        new.allocated.reallocs < old.allocated.reallocs,
        "{title}: reallocations did not improve"
    );
    assert!(
        new.allocated.deallocs <= old.allocated.deallocs,
        "{title}: frees regressed"
    );
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{title}: allocated bytes did not improve"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title}: churn did not improve"
    );
}

fn build_changes(sets: usize, multiline: bool, long_fields: bool) -> String {
    let mut changes = String::with_capacity(sets * if long_fields { 240 } else { 96 });
    for index in 0..sets {
        if index != 0 {
            changes.push(',');
            if multiline {
                changes.push('\n');
            }
        }
        let file = if long_fields {
            format!(
                "BackgroundAnimations/PackName/Very-Long-Song-Directory-{index:03}/layer-{index:03}.mp4"
            )
        } else {
            format!("clip-{index:03}.mp4")
        };
        write!(changes, "{index}=").unwrap();
        if multiline {
            changes.push('\n');
        }
        write!(
            changes,
            "{file}=1=0=0=0=0=overlay-{index:03}.png=CrossFade=#ffffff=#000000"
        )
        .unwrap();
    }
    changes
}

fn main() {
    let entries = Vec::<String>::new();
    let ordinary = build_changes(47, false, false);
    benchmark_pair("ordinary BG-change sets", &ordinary, &entries, 96);

    let multiline = build_changes(31, true, false);
    benchmark_pair("multiline BG-change sets", &multiline, &entries, 96);

    let long_fields = build_changes(47, false, true);
    benchmark_pair("long-field BG-change sets", &long_fields, &entries, 64);
}
