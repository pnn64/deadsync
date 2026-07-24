use deadsync_audio_replaygain::{
    disk_cache_lookup_workload_for_bench, disk_cache_lookup_workload_legacy_for_bench,
    memory_cache_lookup_workload_for_bench, memory_cache_lookup_workload_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const KEY_COUNT: usize = 64;
const LOOKUP_ROUNDS: usize = 16;
const FILESYSTEM_RUNS: usize = 100;
const MEMORY_RUNS: usize = 10_000;

type PathWorkload = fn(&[PathBuf], usize) -> u64;
type KeyWorkload = fn(&[u64], usize) -> u64;

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
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
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
    let root = benchmark_root();
    let child = root.join("child");
    std::fs::create_dir_all(&child).expect("create benchmark directory");
    let paths = (0..KEY_COUNT)
        .map(|index| {
            let path = root.join(format!("track-{index:03}.ogg"));
            std::fs::write(&path, [index as u8]).expect("write benchmark file");
            path
        })
        .collect::<Vec<_>>();
    let canonical = paths
        .iter()
        .map(|path| std::fs::canonicalize(path).expect("canonical benchmark path"))
        .collect::<Vec<_>>();
    let aliases = paths
        .iter()
        .map(|path| child.join("..").join(path.file_name().expect("file name")))
        .collect::<Vec<_>>();
    let keys = (0..KEY_COUNT)
        .map(|index| (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .collect::<Vec<_>>();

    run_path_benchmark(
        "ready path cache lookup",
        memory_cache_lookup_workload_legacy_for_bench,
        memory_cache_lookup_workload_for_bench,
        &canonical,
    );
    run_path_benchmark(
        "non-canonical alias cache lookup",
        memory_cache_lookup_workload_legacy_for_bench,
        memory_cache_lookup_workload_for_bench,
        &aliases,
    );
    run_key_benchmark(
        "disk cache hash lookup",
        disk_cache_lookup_workload_legacy_for_bench,
        disk_cache_lookup_workload_for_bench,
        &keys,
    );

    let _ = std::fs::remove_dir_all(root);
}

fn benchmark_root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "deadsync-replaygain-bench-{}-{stamp}",
        std::process::id()
    ))
}

fn run_path_benchmark(
    label: &str,
    legacy: PathWorkload,
    optimized: PathWorkload,
    paths: &[PathBuf],
) {
    assert_eq!(
        legacy(paths, LOOKUP_ROUNDS),
        optimized(paths, LOOKUP_ROUNDS)
    );
    let old = measure_paths(legacy, paths);
    let new = measure_paths(optimized, paths);
    print_comparison(label, FILESYSTEM_RUNS, &old, &new);
}

fn run_key_benchmark(label: &str, legacy: KeyWorkload, optimized: KeyWorkload, keys: &[u64]) {
    assert_eq!(legacy(keys, LOOKUP_ROUNDS), optimized(keys, LOOKUP_ROUNDS));
    let old = measure_keys(legacy, keys);
    let new = measure_keys(optimized, keys);
    print_comparison(label, MEMORY_RUNS, &old, &new);
}

fn measure_paths(workload: PathWorkload, paths: &[PathBuf]) -> BenchResult {
    for _ in 0..2 {
        black_box(workload(paths, LOOKUP_ROUNDS));
    }
    measure(FILESYSTEM_RUNS, || workload(paths, LOOKUP_ROUNDS))
}

fn measure_keys(workload: KeyWorkload, keys: &[u64]) -> BenchResult {
    for _ in 0..20 {
        black_box(workload(keys, LOOKUP_ROUNDS));
    }
    measure(MEMORY_RUNS, || workload(keys, LOOKUP_ROUNDS))
}

fn measure(runs: usize, mut workload: impl FnMut() -> u64) -> BenchResult {
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..runs {
        checksum = checksum.rotate_left(7) ^ black_box(workload()) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_comparison(label: &str, runs: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum);
    let operations = (KEY_COUNT * LOOKUP_ROUNDS * runs) as f64;
    println!("{label} ({KEY_COUNT} keys x {LOOKUP_ROUNDS} rounds x {runs} runs)");
    print_result("old", old, operations);
    print_result("new", new, operations);
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

fn print_result(label: &str, result: &BenchResult, operations: f64) {
    println!(
        "  {label:<4} {:>8.2} ns/op {:>8.2} cycles/op {:>7.2} Mops/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.4}/{:.4} per op, {:.1} bytes/op",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
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
    // SAFETY: timestamp reads and fences do not access memory.
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
