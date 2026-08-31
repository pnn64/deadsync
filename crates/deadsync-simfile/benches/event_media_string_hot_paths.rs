use deadsync_simfile::event_intro::{
    event_intro_name_for_pack, event_intro_name_for_pack_reference_for_bench,
    itl_event_intro_name_for_bench, itl_event_intro_name_reference_for_bench,
};
use deadsync_simfile::media::{song_art_file_key, song_art_file_key_reference_for_bench};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const REPEATS: usize = 256;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation is delegated unchanged to `System`; the relaxed
// counters only observe this single-threaded benchmark while its gate is set.
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
            self.frees.fetch_add(1, Ordering::Relaxed);
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
    frees: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
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
    median_cycles: Option<f64>,
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

fn run_timed(op: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(op());
    let elapsed_ns = started.elapsed().as_secs_f64() * 1_000_000_000.0;
    let elapsed_cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64);
    (elapsed_ns, elapsed_cycles, checksum)
}

fn measure_pair(
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..4 {
        black_box(old_op());
        black_box(new_op());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0u64;
    let mut new_checksum = 0u64;
    for sample in 0..SAMPLES {
        let mut record_old = || {
            let (elapsed, cycles, checksum) = run_timed(&mut old_op);
            old_times.push(elapsed);
            if let Some(cycles) = cycles {
                old_cycles.push(cycles);
            }
            old_checksum ^= checksum;
        };
        let mut record_new = || {
            let (elapsed, cycles, checksum) = run_timed(&mut new_op);
            new_times.push(elapsed);
            if let Some(cycles) = cycles {
                new_cycles.push(cycles);
            }
            new_checksum ^= checksum;
        };
        if sample % 2 == 0 {
            record_old();
            record_new();
        } else {
            record_new();
            record_old();
        }
    }

    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    old_cycles.sort_by(f64::total_cmp);
    new_cycles.sort_by(f64::total_cmp);

    let old_allocated = measure_allocations(&mut old_op);
    let new_allocated = measure_allocations(&mut new_op);
    let row = |times: Vec<f64>, cycles: Vec<f64>, allocated, checksum| BenchResult {
        median_ns: percentile(&times, 50),
        p95_ns: percentile(&times, 95),
        median_cycles: (!cycles.is_empty()).then(|| percentile(&cycles, 50)),
        allocated,
        checksum,
    };
    (
        row(old_times, old_cycles, old_allocated, old_checksum),
        row(new_times, new_cycles, new_allocated, new_checksum),
    )
}

fn measure_allocations(op: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn string_checksum(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn option_checksum(value: Option<String>) -> u64 {
    value.as_deref().map_or(0, string_checksum)
}

fn run_option_fixture(fixture: &[&str], mut classify: impl FnMut(&str) -> Option<String>) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..REPEATS {
        for value in fixture {
            checksum = checksum
                .rotate_left(7)
                .wrapping_add(option_checksum(classify(black_box(value))));
        }
    }
    checksum
}

fn run_path_fixture(fixture: &[&str], mut key: impl FnMut(&Path) -> String) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..REPEATS {
        for value in fixture {
            let normalized = key(black_box(Path::new(value)));
            checksum = checksum
                .rotate_left(7)
                .wrapping_add(string_checksum(&normalized));
        }
    }
    checksum
}

fn print_result(label: &str, result: &BenchResult, calls: usize) {
    println!(
        "{label:<4} {:>10.1} ns median  {:>10.1} ns p95  {:>10.1} cycles  \
         {:>8.2} Mops/s  {:>6} alloc  {:>4} realloc  {:>6} free  \
         {:>9} B alloc  {:>9} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        calls as f64 * 1_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
}

fn gate_pair(
    title: &str,
    old: &BenchResult,
    new: &BenchResult,
    calls: usize,
    expected_new_allocs: u64,
) {
    assert_eq!(old.checksum, new.checksum, "{title}: behavior diverged");
    println!("\n{title} ({calls} calls/sample)");
    print_result("old", old, calls);
    print_result("new", new, calls);
    println!(
        "gain {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% allocs  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        reduction(old.median_ns, new.median_ns),
        reduction(old.p95_ns, new.p95_ns),
        reduction(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        reduction(old.allocated.allocs as f64, new.allocated.allocs as f64),
        reduction(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        reduction(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert!(new.median_ns < old.median_ns, "{title}: median regressed");
    assert!(new.p95_ns < old.p95_ns, "{title}: p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{title}: cycles regressed");
    }
    assert_eq!(
        new.allocated.allocs, expected_new_allocs,
        "{title}: optimized allocation count changed"
    );
    assert_eq!(new.allocated.reallocs, 0, "{title}: optimized path grew");
    assert_eq!(
        new.allocated.frees, expected_new_allocs,
        "{title}: optimized free count changed"
    );
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{title}: allocations did not improve"
    );
    assert!(
        new.allocated.frees < old.allocated.frees,
        "{title}: frees did not improve"
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

fn reduction(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn main() {
    let itl_packs = [
        "ITL Online 2026",
        "ITL ONLINE 2026 Unlocks",
        "ITL 2025",
        "prefix ITL Online 2024",
        "ITL Online 2026 Unlocks - Player",
        "ITL Online",
        "Regular Pack 12",
        "Stamina RPG 10",
    ];
    let itl_matches = itl_packs
        .iter()
        .filter(|pack| itl_event_intro_name_for_bench(pack).is_some())
        .count();
    let (old, new) = measure_pair(
        || run_option_fixture(&itl_packs, itl_event_intro_name_reference_for_bench),
        || run_option_fixture(&itl_packs, itl_event_intro_name_for_bench),
    );
    gate_pair(
        "ITL event-name recognition",
        &old,
        &new,
        itl_packs.len() * REPEATS,
        (itl_matches * REPEATS) as u64,
    );

    let event_packs = [
        "Stamina RPG 10 Unlocks",
        "prefix SRPG10 suffix",
        "STAMINA RPG 9",
        "srpg9 unlocks",
        "ITL Online 2026 Unlocks",
        "ITL 2025",
        "prefix ITL Online 2024",
        "Regular Pack 12",
        "Stamina RPG Songs",
        "\u{00c9}t\u{00e9} Pack 2026",
    ];
    let event_matches = event_packs
        .iter()
        .filter(|pack| event_intro_name_for_pack(pack).is_some())
        .count();
    let (old, new) = measure_pair(
        || run_option_fixture(&event_packs, event_intro_name_for_pack_reference_for_bench),
        || run_option_fixture(&event_packs, event_intro_name_for_pack),
    );
    gate_pair(
        "public event-name classification",
        &old,
        &new,
        event_packs.len() * REPEATS,
        (event_matches * REPEATS) as u64,
    );

    let art_paths = [
        "Visuals/Banner.PNG",
        r"Visuals\Pack\BANNER.JpG",
        "already/lower.png",
        r"MIXED\slashes/Art.BMP",
        "\u{00c9}t\u{00e9}/ART.PNG",
        "Songs/Very Long Pack Name/Very Long Song Name/banner image.JPEG",
        r"Songs\Pack\Song\background file.PNG",
        "A/B/C/D/E/F/G/H/I/banner.webp",
    ];
    let (old, new) = measure_pair(
        || run_path_fixture(&art_paths, song_art_file_key_reference_for_bench),
        || run_path_fixture(&art_paths, song_art_file_key),
    );
    gate_pair(
        "song-art path key normalization",
        &old,
        &new,
        art_paths.len() * REPEATS,
        (art_paths.len() * REPEATS) as u64,
    );
}
