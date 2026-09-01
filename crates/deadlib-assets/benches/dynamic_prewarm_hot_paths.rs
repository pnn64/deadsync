use deadlib_assets::dynamic::{
    benchmark_dynamic_prewarm_scheduler_batched_results,
    benchmark_dynamic_prewarm_scheduler_reference, benchmark_prewarm_cache_location_prepared,
    benchmark_prewarm_cache_location_reference, benchmark_prewarm_cache_paths_reference,
    benchmark_prewarm_cache_paths_shared,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const WARMUPS: usize = 3;
const PATH_JOBS: usize = 2_048;
const LOCATION_JOBS: usize = 128;
const SCHEDULE_JOBS: usize = 4_096;
const WORKERS: usize = 4;

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

// SAFETY: allocator requests are delegated unchanged to `System`; relaxed
// counters observe one benchmark operation while the measurement gate is set.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let output = unsafe { System.realloc(pointer, old, new_size) };
        if !output.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        output
    }
}

#[derive(Clone, Copy, Default)]
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

    const fn churn(self) -> u64 {
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

fn measure_pair(
    ops: usize,
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..WARMUPS {
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
        let (old_sample, new_sample) = if sample.is_multiple_of(2) {
            (
                timed_sample(ops, &mut old_op),
                timed_sample(ops, &mut new_op),
            )
        } else {
            let new_sample = timed_sample(ops, &mut new_op);
            let old_sample = timed_sample(ops, &mut old_op);
            (old_sample, new_sample)
        };
        old_times.push(old_sample.0);
        new_times.push(new_sample.0);
        if let Some(cycles) = old_sample.1 {
            old_cycles.push(cycles);
        }
        if let Some(cycles) = new_sample.1 {
            new_cycles.push(cycles);
        }
        old_checksum ^= old_sample.2;
        new_checksum ^= new_sample.2;
    }

    let old_allocated = measured_allocations(&mut old_op);
    let new_allocated = measured_allocations(&mut new_op);
    (
        bench_result(old_times, old_cycles, old_allocated, old_checksum),
        bench_result(new_times, new_cycles, new_allocated, new_checksum),
    )
}

fn timed_sample(ops: usize, operation: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..ops {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    let elapsed = started.elapsed().as_secs_f64() * 1e9 / ops as f64;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64);
    (elapsed, cycles, checksum)
}

fn measured_allocations(operation: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn bench_result(
    mut times: Vec<f64>,
    mut cycles: Vec<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
) -> BenchResult {
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);
    BenchResult {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        allocated,
        checksum,
    }
}

fn result_checksum(values: &[u64]) -> u64 {
    values.iter().fold(values.len() as u64, |checksum, &value| {
        checksum.wrapping_mul(131).wrapping_add(value)
    })
}

fn percent_change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

fn print_comparison(label: &str, items: usize, old: &BenchResult, new: &BenchResult) {
    println!("\n{label} ({items} items)");
    println!(
        "  old: {:>10.1} ns  p95 {:>10.1} ns  {:>12.1} items/s",
        old.median_ns,
        old.p95_ns,
        items as f64 * 1e9 / old.median_ns
    );
    println!(
        "  new: {:>10.1} ns  p95 {:>10.1} ns  {:>12.1} items/s",
        new.median_ns,
        new.p95_ns,
        items as f64 * 1e9 / new.median_ns
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        println!(
            "  cycles: {:>10.1} -> {:>10.1} ({:+.2}%)",
            old_cycles,
            new_cycles,
            percent_change(old_cycles, new_cycles)
        );
    }
    println!(
        "  alloc/realloc/free: {}/{}/{} -> {}/{}/{}",
        old.allocated.allocs,
        old.allocated.reallocs,
        old.allocated.frees,
        new.allocated.allocs,
        new.allocated.reallocs,
        new.allocated.frees
    );
    println!(
        "  allocated bytes: {} -> {} ({:+.2}%), churn: {} -> {} ({:+.2}%)",
        old.allocated.allocated_bytes,
        new.allocated.allocated_bytes,
        percent_change(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        old.allocated.churn(),
        new.allocated.churn(),
        percent_change(old.allocated.churn() as f64, new.allocated.churn() as f64),
    );
    println!(
        "  median {:+.2}%, p95 {:+.2}%, throughput {:+.2}%",
        percent_change(old.median_ns, new.median_ns),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            items as f64 * 1e9 / old.median_ns,
            items as f64 * 1e9 / new.median_ns,
        )
    );
}

fn assert_improved(label: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{label} changed behavior");
    assert!(
        new.median_ns < old.median_ns,
        "{label} median regressed: {} -> {} ns",
        old.median_ns,
        new.median_ns
    );
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{label} did not reduce allocations: {} -> {}",
        old.allocated.allocs,
        new.allocated.allocs
    );
    assert!(
        new.allocated.churn() < old.allocated.churn(),
        "{label} did not reduce allocation churn: {} -> {}",
        old.allocated.churn(),
        new.allocated.churn()
    );
}

#[inline(always)]
fn cycle_counter() -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: `_rdtsc` is available on every x86_64 target.
        return Some(unsafe { core::arch::x86_64::_rdtsc() });
    }
    #[cfg(target_arch = "x86")]
    {
        // SAFETY: `_rdtsc` is available on the benchmark's supported x86 CPUs.
        return Some(unsafe { core::arch::x86::_rdtsc() });
    }
    #[allow(unreachable_code)]
    None
}

fn main() {
    let paths = (0..PATH_JOBS)
        .map(|index| PathBuf::from(format!("Pack{:03}/Song{:05}/banner.png", index % 64, index)))
        .collect::<Vec<_>>();
    let cache_dir = Path::new("cache/artwork/banners");
    assert_eq!(
        benchmark_prewarm_cache_paths_reference(&paths, cache_dir),
        benchmark_prewarm_cache_paths_shared(&paths, cache_dir)
    );

    let (old_paths, new_paths) = measure_pair(
        16,
        || benchmark_prewarm_cache_paths_reference(black_box(&paths), cache_dir),
        || benchmark_prewarm_cache_paths_shared(black_box(&paths), cache_dir),
    );
    print_comparison(
        "shared artwork cache directory",
        PATH_JOBS,
        &old_paths,
        &new_paths,
    );
    assert_improved("shared artwork cache directory", &old_paths, &new_paths);

    let existing_path = std::env::current_exe().expect("benchmark executable has a path");
    let location_paths = std::iter::repeat_n(existing_path, LOCATION_JOBS).collect::<Vec<_>>();
    assert_eq!(
        benchmark_prewarm_cache_location_reference(&location_paths, cache_dir),
        benchmark_prewarm_cache_location_prepared(&location_paths, cache_dir)
    );
    let (old_locations, new_locations) = measure_pair(
        2,
        || benchmark_prewarm_cache_location_reference(black_box(&location_paths), cache_dir),
        || benchmark_prewarm_cache_location_prepared(black_box(&location_paths), cache_dir),
    );
    print_comparison(
        "reused artwork cache locations",
        LOCATION_JOBS,
        &old_locations,
        &new_locations,
    );
    assert_improved(
        "reused artwork cache locations",
        &old_locations,
        &new_locations,
    );

    let values: Arc<[u64]> = (0..SCHEDULE_JOBS as u64)
        .map(|value| value.wrapping_mul(17).wrapping_add(3))
        .collect();
    let reference = benchmark_dynamic_prewarm_scheduler_reference(values.to_vec(), WORKERS);
    let batched = benchmark_dynamic_prewarm_scheduler_batched_results(values.to_vec(), WORKERS);
    assert_eq!(batched, reference);

    let (old_scheduler, new_scheduler) = measure_pair(
        1,
        || {
            result_checksum(&benchmark_dynamic_prewarm_scheduler_reference(
                black_box(values.to_vec()),
                WORKERS,
            ))
        },
        || {
            result_checksum(&benchmark_dynamic_prewarm_scheduler_batched_results(
                black_box(values.to_vec()),
                WORKERS,
            ))
        },
    );
    print_comparison(
        "batched artwork prewarm scheduling",
        SCHEDULE_JOBS,
        &old_scheduler,
        &new_scheduler,
    );
    assert_improved(
        "batched artwork prewarm scheduling",
        &old_scheduler,
        &new_scheduler,
    );
}
