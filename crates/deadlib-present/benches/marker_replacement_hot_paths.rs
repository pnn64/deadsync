use deadlib_present::font::bench_support::{replace_markers_new, replace_markers_old};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 25;
const OPS_PER_SAMPLE: usize = 25_000;

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

// SAFETY: all allocation operations delegate unchanged to `System`; the
// relaxed counters only observe successful calls while this benchmark's
// single thread enables measurement.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
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

fn measure(mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..8 {
        black_box(op());
    }

    let mut ns = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..OPS_PER_SAMPLE {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ns.push(elapsed.as_secs_f64() * 1_000_000_000.0 / OPS_PER_SAMPLE as f64);
        cycles.push(cycle_start.zip(cycle_end).map_or(f64::NAN, |(start, end)| {
            end.wrapping_sub(start) as f64 / OPS_PER_SAMPLE as f64
        }));
    }
    ns.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);

    BenchResult {
        median_ns: percentile(&ns, 0.5),
        p95_ns: percentile(&ns, 0.95),
        median_cycles: percentile(&cycles, 0.5),
        allocated,
        checksum: checksum.wrapping_add(allocation_checksum),
    }
}

fn replacement_checksum(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let first = bytes.first().copied().unwrap_or_default() as u64;
    let middle = bytes.get(bytes.len() / 2).copied().unwrap_or_default() as u64;
    let last = bytes.last().copied().unwrap_or_default() as u64;
    (bytes.len() as u64) ^ first.rotate_left(13) ^ middle.rotate_left(29) ^ last.rotate_left(47)
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<9} {:>9.1} ns median  {:>9.1} ns p95  {:>9.1} cycles  \
         {:>10.1} text/s  {:>3} alloc  {:>3} realloc  {:>3} free  {:>5} B alloc  {:>5} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles,
        1_000_000_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "change    {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        100.0 * (1.0 - new.median_ns / old.median_ns),
        100.0 * (1.0 - new.p95_ns / old.p95_ns),
        100.0 * (1.0 - new.median_cycles / old.median_cycles),
        if old.allocated.allocated_bytes == 0 {
            0.0
        } else {
            100.0
                * (1.0
                    - new.allocated.allocated_bytes as f64 / old.allocated.allocated_bytes as f64)
        },
        if old.allocated.churn_bytes() == 0 {
            0.0
        } else {
            100.0 * (1.0 - new.allocated.churn_bytes() as f64 / old.allocated.churn_bytes() as f64)
        },
    );
}

fn assert_cpu_improvement(title: &str, old: &BenchResult, new: &BenchResult) {
    assert!(
        new.median_ns < old.median_ns,
        "{title}: median latency did not improve"
    );
    assert!(
        new.p95_ns < old.p95_ns,
        "{title}: p95 latency did not improve"
    );
    if old.median_cycles.is_finite() && new.median_cycles.is_finite() {
        assert!(
            new.median_cycles < old.median_cycles,
            "{title}: CPU cycles did not improve"
        );
    }
}

fn assert_churn_improvement(title: &str, old: &BenchResult, new: &BenchResult) {
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{title}: allocation count did not improve"
    );
    assert!(
        new.allocated.reallocs <= old.allocated.reallocs,
        "{title}: reallocation count regressed"
    );
    assert!(
        new.allocated.deallocs < old.allocated.deallocs,
        "{title}: free count did not improve"
    );
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{title}: allocated bytes did not improve"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title}: memory churn did not improve"
    );
}

fn benchmark_pair(title: &str, input: &str, require_less_churn: bool) {
    let expected = replace_markers_old(input);
    let actual = replace_markers_new(input);
    assert_eq!(
        actual.as_ref(),
        expected.as_ref(),
        "{title} output diverged"
    );

    let old = measure(|| {
        let output = replace_markers_old(black_box(input));
        black_box(output.as_ref());
        replacement_checksum(output.as_ref())
    });
    let new = measure(|| {
        let output = replace_markers_new(black_box(input));
        black_box(output.as_ref());
        replacement_checksum(output.as_ref())
    });

    print_pair(title, &old, &new);
    assert_cpu_improvement(title, &old, &new);
    if require_less_churn {
        assert_churn_improvement(title, &old, &new);
    } else {
        assert_eq!(new.allocated.allocs, old.allocated.allocs);
        assert_eq!(new.allocated.reallocs, old.allocated.reallocs);
        assert_eq!(new.allocated.deallocs, old.allocated.deallocs);
        assert_eq!(new.allocated.churn_bytes(), old.allocated.churn_bytes());
    }
}

fn main() {
    benchmark_pair(
        "mixed numeric and alias markers",
        "Press &START; for &#9654;, then &MENULEFT; or &x2605;.",
        true,
    );
    benchmark_pair(
        "alias marker batch",
        "&START; &MENULEFT; &MENURIGHT; &UP; &DOWN; &BLACKSTAR; &OMEGA;",
        false,
    );
    benchmark_pair(
        "invalid ampersand text",
        "Rock & Roll && unknown &missing; plus trailing & and malformed &xZZ;",
        true,
    );
}
