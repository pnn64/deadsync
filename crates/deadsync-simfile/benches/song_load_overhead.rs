use deadsync_simfile::app_runtime::{
    benchmark_parse_options_hoisted, benchmark_parse_options_per_song,
};
use deadsync_simfile::cache::benchmark_runtime_debug_logs;
use deadsync_simfile::scan::{
    benchmark_child_dirs_current, benchmark_child_dirs_legacy, benchmark_legacy_song_workers,
    benchmark_pooled_song_workers,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const JOB_COUNT: usize = 512;
const SONG_COUNT: usize = 2_805;
const DISCOVERY_DIRS: usize = 2_048;
const DISCOVERY_FILES: usize = 64;
const SAMPLES: usize = 9;

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

// SAFETY: every allocator request is delegated unchanged to `System`; the
// atomic counters support observations from all benchmark worker threads.
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
        // SAFETY: arguments are forwarded unchanged to `System`.
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

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Sample {
    ns: f64,
    cycles: Option<u64>,
}

struct BenchResult {
    median_ns: f64,
    max_ns: f64,
    cycles_per_item: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(item_count: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    black_box(op());
    let mut checksum = 0u64;
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        checksum ^= black_box(op());
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        samples.push(Sample {
            ns: elapsed.as_secs_f64() * 1e9,
            cycles: cycle_start
                .zip(cycle_end)
                .map(|(start, end)| end.wrapping_sub(start)),
        });
    }
    let max_ns = samples.iter().map(|sample| sample.ns).fold(0.0, f64::max);
    samples.sort_by(|left, right| left.ns.total_cmp(&right.ns));
    let median = &samples[SAMPLES / 2];

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    checksum ^= black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: median.ns,
        max_ns,
        cycles_per_item: median
            .cycles
            .map(|cycles| cycles as f64 / item_count as f64),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn main() {
    let discovery_root = discovery_fixture();
    let old_checksum = benchmark_child_dirs_legacy(&discovery_root).unwrap();
    let new_checksum = benchmark_child_dirs_current(&discovery_root).unwrap();
    assert_eq!(old_checksum, new_checksum, "directory discovery diverged");
    let discovery_entries = DISCOVERY_DIRS + DISCOVERY_FILES + 1;
    let old = measure(discovery_entries, || {
        benchmark_child_dirs_legacy(&discovery_root).unwrap()
    });
    let new = measure(discovery_entries, || {
        benchmark_child_dirs_current(&discovery_root).unwrap()
    });
    black_box(old.checksum ^ new.checksum);
    println!("song directory discovery ({discovery_entries} entries)");
    print_result("old", discovery_entries, &old);
    print_result("new", discovery_entries, &new);
    print_change(&old, &new);

    let available = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let workers = available.min(JOB_COUNT);

    for (label, work_iterations) in [("cache-like", 64), ("parse-like", 16_384)] {
        let old_checksum = benchmark_legacy_song_workers(JOB_COUNT, workers, work_iterations);
        let new_checksum = benchmark_pooled_song_workers(JOB_COUNT, workers, work_iterations);
        assert_eq!(old_checksum, new_checksum, "worker results diverged");

        let old = measure(JOB_COUNT, || {
            benchmark_legacy_song_workers(JOB_COUNT, workers, work_iterations)
        });
        let new = measure(JOB_COUNT, || {
            benchmark_pooled_song_workers(JOB_COUNT, workers, work_iterations)
        });
        black_box(old.checksum ^ new.checksum);
        println!("song worker scheduling: {label} ({JOB_COUNT} jobs, {workers} workers)");
        print_result("old", JOB_COUNT, &old);
        print_result("new", JOB_COUNT, &new);
        print_change(&old, &new);
    }

    assert_eq!(
        benchmark_parse_options_per_song(SONG_COUNT),
        benchmark_parse_options_hoisted(SONG_COUNT),
        "parse-option reuse changed its observed configuration"
    );
    let old = measure(SONG_COUNT, || {
        benchmark_parse_options_per_song(SONG_COUNT) as u64
    });
    let new = measure(SONG_COUNT, || {
        benchmark_parse_options_hoisted(SONG_COUNT) as u64
    });
    black_box(old.checksum ^ new.checksum);
    println!("per-scan parse option setup ({SONG_COUNT} songs)");
    print_result("old", SONG_COUNT, &old);
    print_result("new", SONG_COUNT, &new);
    print_change(&old, &new);

    assert_eq!(benchmark_runtime_debug_logs(SONG_COUNT, true), SONG_COUNT);
    assert_eq!(benchmark_runtime_debug_logs(SONG_COUNT, false), 0);
    let old = measure(SONG_COUNT, || {
        benchmark_runtime_debug_logs(SONG_COUNT, true) as u64
    });
    let new = measure(SONG_COUNT, || {
        benchmark_runtime_debug_logs(SONG_COUNT, false) as u64
    });
    black_box(old.checksum ^ new.checksum);
    println!("disabled per-song debug logging ({SONG_COUNT} songs)");
    print_result("old", SONG_COUNT, &old);
    print_result("new", SONG_COUNT, &new);
    print_change(&old, &new);

    fs::remove_dir_all(discovery_root).unwrap();
}

fn discovery_fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "deadsync-song-discovery-bench-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    for index in 0..DISCOVERY_DIRS {
        fs::create_dir(root.join(format!("Song {index:05}"))).unwrap();
    }
    fs::create_dir(root.join("._ignored")).unwrap();
    for index in 0..DISCOVERY_FILES {
        fs::write(root.join(format!("asset-{index:03}.png")), []).unwrap();
    }
    root
}

fn print_result(label: &str, item_count: usize, result: &BenchResult) {
    println!(
        "  {label:<3} {:>9.3} ms median  {:>9.3} ms max  {:>10.1} cycles/item  \
         {:>8.3} Mitem/s  {:>7.3} alloc/item  {:>7.3} realloc/item  \
         {:>10.1} churn B/item",
        result.median_ns / 1e6,
        result.max_ns / 1e6,
        result.cycles_per_item.unwrap_or(f64::NAN),
        item_count as f64 * 1_000.0 / result.median_ns,
        result.alloc.allocs as f64 / item_count as f64,
        result.alloc.reallocs as f64 / item_count as f64,
        result.alloc.churn_bytes() as f64 / item_count as f64,
    );
}

fn print_change(old: &BenchResult, new: &BenchResult) {
    println!(
        "  change: {:+.2}% median  {:+.2}% max  {:+.2}% cycles  {:+.2}% throughput  \
         {:+.2}% churn",
        percent_change(old.median_ns, new.median_ns),
        percent_change(old.max_ns, new.max_ns),
        percent_change(
            old.cycles_per_item.unwrap_or(f64::NAN),
            new.cycles_per_item.unwrap_or(f64::NAN),
        ),
        percent_change(1.0 / old.median_ns, 1.0 / new.median_ns),
        percent_change(
            old.alloc.churn_bytes() as f64,
            new.alloc.churn_bytes() as f64,
        ),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
