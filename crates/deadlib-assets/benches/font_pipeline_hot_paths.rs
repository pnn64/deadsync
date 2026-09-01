use deadlib_assets::{
    benchmark_font_contexts_borrowed_roots, benchmark_font_contexts_owned_roots,
    benchmark_font_texture_keys_borrowed_roots, benchmark_font_texture_keys_cloned_roots,
    benchmark_required_font_texture_keys_recomputed, benchmark_required_font_texture_keys_reused,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const WARMUPS: usize = 3;
const TEXTURES: usize = 2_048;

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

fn checksum(values: &[String]) -> u64 {
    values.iter().fold(values.len() as u64, |checksum, value| {
        value.bytes().fold(checksum.wrapping_mul(131), |sum, byte| {
            sum.wrapping_mul(31).wrapping_add(u64::from(byte))
        })
    })
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
    let mut old_checksum = 0;
    let mut new_checksum = 0;
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

fn change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

fn print_comparison(label: &str, old: &BenchResult, new: &BenchResult) {
    println!("\n{label} ({TEXTURES} textures)");
    println!(
        "  old: {:>10.1} ns  p95 {:>10.1} ns  {:>12.1} textures/s",
        old.median_ns,
        old.p95_ns,
        TEXTURES as f64 * 1e9 / old.median_ns
    );
    println!(
        "  new: {:>10.1} ns  p95 {:>10.1} ns  {:>12.1} textures/s",
        new.median_ns,
        new.p95_ns,
        TEXTURES as f64 * 1e9 / new.median_ns
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        println!(
            "  cycles: {:>10.1} -> {:>10.1} ({:+.2}%)",
            old_cycles,
            new_cycles,
            change(old_cycles, new_cycles)
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
        change(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        old.allocated.churn(),
        new.allocated.churn(),
        change(old.allocated.churn() as f64, new.allocated.churn() as f64),
    );
    println!(
        "  median {:+.2}%, p95 {:+.2}%, throughput {:+.2}%",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            TEXTURES as f64 * 1e9 / old.median_ns,
            TEXTURES as f64 * 1e9 / new.median_ns,
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
    let roots = (0..4)
        .map(|index| PathBuf::from(format!("C:/deadsync/data{index}/assets")))
        .collect::<Vec<_>>();
    let paths = (0..TEXTURES)
        .map(|index| {
            roots[index % roots.len()]
                .join("fonts")
                .join(format!("Font{index:05} 8x8.png"))
        })
        .collect::<Vec<_>>();

    run_pair(
        "borrowed canonical asset roots",
        || checksum(&benchmark_font_texture_keys_cloned_roots(&paths, &roots)),
        || checksum(&benchmark_font_texture_keys_borrowed_roots(&paths, &roots)),
    );
    run_pair(
        "shared font parse roots",
        || checksum(&benchmark_font_contexts_owned_roots(&paths, &roots)),
        || checksum(&benchmark_font_contexts_borrowed_roots(&paths, &roots)),
    );
    run_pair(
        "single-pass required texture keys",
        || {
            checksum(&benchmark_required_font_texture_keys_recomputed(
                &paths, &roots,
            ))
        },
        || checksum(&benchmark_required_font_texture_keys_reused(&paths, &roots)),
    );
}

fn run_pair(label: &str, mut old_op: impl FnMut() -> u64, mut new_op: impl FnMut() -> u64) {
    assert_eq!(old_op(), new_op(), "{label} changed behavior");
    let (old, new) = measure_pair(8, old_op, new_op);
    print_comparison(label, &old, &new);
    assert_improved(label, &old, &new);
}
