use deadsync_gameplay::{error_avg_capacity, life_history_capacity};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::VecDeque;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CASES: usize = 128;
const NOTE_COUNT: usize = 6_000;
const ROW_COUNT: usize = 4_000;
const HOLD_ROLL_COUNT: usize = 1_000;
const LIFE_POINTS: usize = life_history_capacity(NOTE_COUNT, ROW_COUNT, HOLD_ROLL_COUNT);
const ERROR_SAMPLES: usize = error_avg_capacity(NOTE_COUNT, ROW_COUNT);

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed counters only observe successful calls while measurement is active.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
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
    ns_per_case: f64,
    cycles_per_case: Option<f64>,
    allocated: AllocSnapshot,
    checksum: usize,
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

fn measure<T>(make: impl Fn() -> Vec<T>, mut fill: impl FnMut(&mut T) -> usize) -> BenchResult {
    let mut warmup = make();
    let mut timed = make();
    let mut allocated = make();
    for case in &mut warmup {
        black_box(fill(case));
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0usize;
    for case in &mut timed {
        checksum = checksum.wrapping_add(black_box(fill(case)));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0usize;
    for case in &mut allocated {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(fill(case)));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocation_delta = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_case: elapsed.as_secs_f64() * 1_000_000_000.0 / CASES as f64,
        cycles_per_case: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / CASES as f64),
        allocated: allocation_delta,
        checksum,
    }
}

fn measure_life(prewarmed: bool) -> BenchResult {
    measure(
        || {
            (0..CASES)
                .map(|_| {
                    Vec::<(f32, f32)>::with_capacity(if prewarmed { LIFE_POINTS } else { 10_000 })
                })
                .collect()
        },
        |history| {
            for index in 0..LIFE_POINTS {
                history.push((index as f32 * 0.01, (index & 1) as f32));
            }
            history.len()
        },
    )
}

fn measure_error(prewarmed: bool) -> BenchResult {
    measure(
        || {
            (0..CASES)
                .map(|_| {
                    VecDeque::<(f32, f32)>::with_capacity(if prewarmed {
                        ERROR_SAMPLES
                    } else {
                        64
                    })
                })
                .collect()
        },
        |samples| {
            for index in 0..ERROR_SAMPLES {
                samples.push_back((index as f32 * 0.01, (index & 1) as f32));
            }
            samples.len()
        },
    )
}

fn print_result(label: &str, result: &BenchResult, samples: usize) {
    let cases = CASES as f64;
    println!(
        "{label:<18} {:>10.2} us/song  {:>10.0} cycles/song  {:>8.2} Msample/s  \
         {:>5.2} allocs/song  {:>8.1} KiB/song  {:>5.2} reallocs/song",
        result.ns_per_case / 1_000.0,
        result.cycles_per_case.unwrap_or(f64::NAN),
        samples as f64 * 1_000.0 / result.ns_per_case,
        result.allocated.allocs as f64 / cases,
        result.allocated.bytes as f64 / cases / 1024.0,
        result.allocated.reallocs as f64 / cases,
    );
}

fn main() {
    let old_life = measure_life(false);
    let chart_life = measure_life(true);
    let old_error = measure_error(false);
    let chart_error = measure_error(true);
    assert_eq!(old_life.checksum, chart_life.checksum);
    assert_eq!(old_error.checksum, chart_error.checksum);

    println!(
        "Gameplay chart buffer prewarm ({NOTE_COUNT} notes, {ROW_COUNT} rows, \
         {HOLD_ROLL_COUNT} holds/rolls)"
    );
    println!("life history ({LIFE_POINTS} worst-case points)");
    print_result("fixed cap 10000", &old_life, LIFE_POINTS);
    print_result("chart-prewarmed", &chart_life, LIFE_POINTS);
    println!("error average ({ERROR_SAMPLES} worst-case samples)");
    print_result("fixed cap 64", &old_error, ERROR_SAMPLES);
    print_result("chart-prewarmed", &chart_error, ERROR_SAMPLES);
}
