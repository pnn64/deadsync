use deadsync_profile::{
    Perspective, ScrollOption, StepStatisticsMask, TurnOption, option_parse_reference,
    parse_profile_bool,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 7;
const BOOL_ITERS: usize = 100_000;
const SCROLL_ITERS: usize = 100_000;
const OPTION_ITERS: usize = 50_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
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
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the caller-provided
// pointer and layout. Relaxed counters are diagnostics behind a bench-only gate.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied a valid layout.
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
        // SAFETY: the arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy)]
struct TimeSample {
    ns_per_item: f64,
    cycles_per_item: f64,
    items_per_second: f64,
    checksum: u64,
}

fn measure_time(items: usize, iterations: usize, mut operation: impl FnMut() -> u64) -> TimeSample {
    for _ in 0..(iterations / 20).max(1) {
        black_box(operation());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    let elapsed = started.elapsed();
    let count = (items * iterations) as f64;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map_or(f64::NAN, |(start, end)| end.wrapping_sub(start) as f64);
    TimeSample {
        ns_per_item: elapsed.as_secs_f64() * 1.0e9 / count,
        cycles_per_item: cycles / count,
        items_per_second: count / elapsed.as_secs_f64(),
        checksum,
    }
}

fn measure_alloc(mut operation: impl FnMut() -> u64) -> (AllocSnapshot, u64) {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let checksum = black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (ALLOC.snapshot().delta(before), checksum)
}

fn measure_pair(
    items: usize,
    iterations: usize,
    mut old: impl FnMut() -> u64,
    mut new: impl FnMut() -> u64,
) -> (TimeSample, TimeSample, AllocSnapshot, AllocSnapshot) {
    let mut old_samples = Vec::with_capacity(SAMPLES);
    let mut new_samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let (old_sample, new_sample) = if sample % 2 == 0 {
            (
                measure_time(items, iterations, &mut old),
                measure_time(items, iterations, &mut new),
            )
        } else {
            let new_sample = measure_time(items, iterations, &mut new);
            let old_sample = measure_time(items, iterations, &mut old);
            (old_sample, new_sample)
        };
        assert_eq!(old_sample.checksum, new_sample.checksum);
        old_samples.push(old_sample);
        new_samples.push(new_sample);
    }
    old_samples.sort_by(|left, right| left.ns_per_item.total_cmp(&right.ns_per_item));
    new_samples.sort_by(|left, right| left.ns_per_item.total_cmp(&right.ns_per_item));
    let old_time = old_samples[SAMPLES / 2];
    let new_time = new_samples[SAMPLES / 2];
    let (old_alloc, old_checksum) = measure_alloc(&mut old);
    let (new_alloc, new_checksum) = measure_alloc(&mut new);
    assert_eq!(old_checksum, new_checksum);
    (old_time, new_time, old_alloc, new_alloc)
}

fn print_pair(
    name: &str,
    old: TimeSample,
    new: TimeSample,
    old_alloc: AllocSnapshot,
    new_alloc: AllocSnapshot,
) {
    println!("\n{name}");
    for (label, time, alloc) in [("old", old, old_alloc), ("new", new, new_alloc)] {
        println!(
            "  {label} {:>8.2} ns/item {:>8.2} cycles/item {:>8.2} Mitem/s {:>4} calls {:>9} churn B/op",
            time.ns_per_item,
            time.cycles_per_item,
            time.items_per_second / 1.0e6,
            alloc.calls(),
            alloc.churn_bytes(),
        );
    }
    println!(
        "  change {:+.2}% latency/cycles {:+.2}% throughput {:+.2}% churn bytes",
        percent(new.ns_per_item, old.ns_per_item),
        percent(new.items_per_second, old.items_per_second),
        percent(
            new_alloc.churn_bytes() as f64,
            old_alloc.churn_bytes() as f64
        ),
    );
}

fn percent(new: f64, old: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

const BOOL_INPUTS: [&str; 12] = [
    "1", "TRUE", " yes ", "On", "true", "YES", "0", "FALSE", " no ", "Off", "false", "NO",
];

fn bool_checksum(reference: bool) -> u64 {
    BOOL_INPUTS.iter().fold(0u64, |sum, value| {
        let parsed = if reference {
            option_parse_reference::profile_bool(black_box(value))
        } else {
            parse_profile_bool(black_box(value))
        };
        sum.rotate_left(3) ^ parsed.map_or(2, u64::from)
    })
}

const SCROLL_INPUTS: [&str; 6] = [
    "Normal",
    "REVERSE",
    "Reverse+Cross Centered",
    "split,alternate",
    " centered + reverse ",
    "Normal Reverse Cross",
];

fn scroll_bits(value: ScrollOption) -> u64 {
    [
        ScrollOption::Reverse,
        ScrollOption::Split,
        ScrollOption::Alternate,
        ScrollOption::Cross,
        ScrollOption::Centered,
    ]
    .into_iter()
    .enumerate()
    .fold(value.is_normal() as u64, |bits, (index, flag)| {
        bits | ((value.contains(flag) as u64) << (index + 1))
    })
}

fn scroll_checksum(reference: bool) -> u64 {
    SCROLL_INPUTS.iter().fold(0u64, |sum, value| {
        let parsed = if reference {
            option_parse_reference::scroll_option(black_box(value))
        } else {
            ScrollOption::from_str(black_box(value))
        }
        .expect("benchmark scroll input must be valid");
        sum.rotate_left(5) ^ scroll_bits(parsed)
    })
}

const PERSPECTIVE_INPUTS: [&str; 4] = ["OVERHEAD", " Hallway ", "INCOMING", "space"];
const TURN_INPUTS: [&str; 4] = ["Mirror", "LR-Mirror", "super shuffle", "HYPER SHUFFLE"];
const STEP_STATS_INPUTS: [&str; 4] = [
    "None",
    "stepstats",
    "Judgements Counter, Peak NPS",
    "Step Counts, GS Box",
];
const OPTION_ITEMS: usize = PERSPECTIVE_INPUTS.len() + TURN_INPUTS.len() + STEP_STATS_INPUTS.len();

fn option_checksum(reference: bool) -> u64 {
    let perspective = PERSPECTIVE_INPUTS.iter().fold(0u64, |sum, value| {
        let parsed = if reference {
            option_parse_reference::perspective(black_box(value))
        } else {
            Perspective::from_str(black_box(value))
        }
        .expect("benchmark perspective must be valid");
        sum.rotate_left(3) ^ parsed as u64
    });
    let turns = TURN_INPUTS.iter().fold(perspective, |sum, value| {
        let parsed = if reference {
            option_parse_reference::turn_option(black_box(value))
        } else {
            TurnOption::from_str(black_box(value))
        }
        .expect("benchmark turn must be valid");
        sum.rotate_left(3) ^ parsed as u64
    });
    STEP_STATS_INPUTS.iter().fold(turns, |sum, value| {
        let parsed = if reference {
            option_parse_reference::step_statistics(black_box(value))
        } else {
            StepStatisticsMask::from_str(black_box(value))
        }
        .expect("benchmark step statistics must be valid");
        sum.rotate_left(3) ^ u64::from(parsed.bits())
    })
}

fn main() {
    let (old, new, old_alloc, new_alloc) = measure_pair(
        BOOL_INPUTS.len(),
        BOOL_ITERS,
        || bool_checksum(true),
        || bool_checksum(false),
    );
    print_pair(
        "1. borrowed profile boolean matching",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let (old, new, old_alloc, new_alloc) = measure_pair(
        SCROLL_INPUTS.len(),
        SCROLL_ITERS,
        || scroll_checksum(true),
        || scroll_checksum(false),
    );
    print_pair(
        "2. borrowed compound scroll tokens",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let (old, new, old_alloc, new_alloc) = measure_pair(
        OPTION_ITEMS,
        OPTION_ITERS,
        || option_checksum(true),
        || option_checksum(false),
    );
    print_pair(
        "3. stack-normalized scalar and list option keys",
        old,
        new,
        old_alloc,
        new_alloc,
    );
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the x86-64 timestamp counter.
    Some(unsafe {
        core::arch::x86_64::_mm_lfence();
        let value = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        value
    })
}

#[cfg(not(target_arch = "x86_64"))]
fn cycle_counter() -> Option<u64> {
    None
}
