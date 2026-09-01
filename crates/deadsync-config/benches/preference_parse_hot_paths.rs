use deadsync_config::bools::{parse_bool_str, parse_reference as bool_reference};
use deadsync_config::theme::{
    LanguageFlag, SelectMusicSort, SyncGraphMode, auto_screenshot_mask_from_str,
    auto_screenshot_mask_to_str, parse_reference as theme_reference,
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
const NORMALIZED_ITERS: usize = 75_000;
const SCREENSHOT_ITERS: usize = 75_000;

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

// SAFETY: all operations delegate to `System` with the allocator caller's
// pointer and layout. Relaxed counters are diagnostics behind a bench feature.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid allocation layout.
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
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
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
    items: usize,
    old: TimeSample,
    new: TimeSample,
    old_alloc: AllocSnapshot,
    new_alloc: AllocSnapshot,
) {
    println!("\n{name}");
    for (label, time, alloc) in [("old", old, old_alloc), ("new", new, new_alloc)] {
        println!(
            "  {label} {:>8.2} ns/item {:>8.2} cycles/item {:>8.2} Mitem/s {:>6.2} calls/item {:>9.2} churn B/item",
            time.ns_per_item,
            time.cycles_per_item,
            time.items_per_second / 1.0e6,
            alloc.calls() as f64 / items as f64,
            alloc.churn_bytes() as f64 / items as f64,
        );
    }
    println!(
        "  change {:+.2}% latency {:+.2}% cycles {:+.2}% throughput {:+.2}% calls {:+.2}% churn",
        percent(new.ns_per_item, old.ns_per_item),
        percent(new.cycles_per_item, old.cycles_per_item),
        percent(new.items_per_second, old.items_per_second),
        percent(new_alloc.calls() as f64, old_alloc.calls() as f64),
        percent(
            new_alloc.churn_bytes() as f64,
            old_alloc.churn_bytes() as f64
        ),
    );
}

fn percent(new: f64, old: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

const BOOL_INPUTS: [&str; 14] = [
    "1", "TRUE", " yes ", "On", "true", "YES", "0", "FALSE", " no ", "Off", "false", "NO", "maybe",
    "2",
];

fn bool_checksum(reference: bool) -> u64 {
    BOOL_INPUTS.iter().fold(0u64, |sum, value| {
        let parsed = if reference {
            bool_reference::bool_str(black_box(value))
        } else {
            parse_bool_str(black_box(value))
        };
        sum.rotate_left(3) ^ parsed.map_or(2, u64::from)
    })
}

const SORT_INPUTS: [&str; 5] = [
    "Series",
    " MOST-popular ",
    "recently_played",
    "Machine Top Scores",
    "artist",
];
const SYNC_INPUTS: [&str; 4] = [
    "Frequency",
    "Beat-Digest",
    " POST kernel fingerprint ",
    "fingerprint",
];
const LANGUAGE_INPUTS: [&str; 5] = ["English", "pt-BR", "Brazilian Portuguese", "JA", "pseudo"];
const NORMALIZED_ITEMS: usize = SORT_INPUTS.len() + SYNC_INPUTS.len() + LANGUAGE_INPUTS.len();

fn normalized_checksum(reference: bool) -> u64 {
    let sorts = SORT_INPUTS.iter().fold(0u64, |sum, value| {
        let parsed = if reference {
            theme_reference::select_music_sort(black_box(value))
        } else {
            SelectMusicSort::from_str(black_box(value)).ok()
        }
        .expect("benchmark sort input must be valid");
        sum.rotate_left(4) ^ parsed as u64
    });
    let sync = SYNC_INPUTS.iter().fold(sorts, |sum, value| {
        let parsed = if reference {
            theme_reference::sync_graph_mode(black_box(value))
        } else {
            SyncGraphMode::from_str(black_box(value)).ok()
        }
        .expect("benchmark sync input must be valid");
        sum.rotate_left(4) ^ parsed as u64
    });
    LANGUAGE_INPUTS.iter().fold(sync, |sum, value| {
        let parsed = if reference {
            theme_reference::language(black_box(value))
        } else {
            LanguageFlag::from_str(black_box(value)).ok()
        }
        .expect("benchmark language input must be valid");
        sum.rotate_left(4) ^ parsed as u64
    })
}

const SCREENSHOT_MASKS: [u8; 8] = [0, 1, 2, 3, 7, 13, 21, 31];

fn screenshot_checksum(reference: bool) -> u64 {
    SCREENSHOT_MASKS.iter().fold(0u64, |sum, &mask| {
        let encoded = if reference {
            theme_reference::auto_screenshot_mask_to_str(black_box(mask))
        } else {
            auto_screenshot_mask_to_str(black_box(mask))
        };
        let parsed = if reference {
            theme_reference::auto_screenshot_mask_from_str(black_box(&encoded))
        } else {
            auto_screenshot_mask_from_str(black_box(&encoded))
        };
        assert_eq!(parsed, mask);
        sum.rotate_left(5) ^ u64::from(parsed) ^ encoded.len() as u64
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
        "1. borrowed boolean tokens",
        BOOL_INPUTS.len(),
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let (old, new, old_alloc, new_alloc) = measure_pair(
        NORMALIZED_ITEMS,
        NORMALIZED_ITERS,
        || normalized_checksum(true),
        || normalized_checksum(false),
    );
    print_pair(
        "2. stack-normalized theme preference keys",
        NORMALIZED_ITEMS,
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let (old, new, old_alloc, new_alloc) = measure_pair(
        SCREENSHOT_MASKS.len(),
        SCREENSHOT_ITERS,
        || screenshot_checksum(true),
        || screenshot_checksum(false),
    );
    print_pair(
        "3. streamed auto-screenshot flag codec",
        SCREENSHOT_MASKS.len(),
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
