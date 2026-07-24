use deadlib_assets::{
    registry::{
        generated_texture_workload_for_bench, generated_texture_workload_legacy_for_bench,
        texture_metadata_workload_for_bench, texture_metadata_workload_legacy_for_bench,
    },
    texture_store::{
        texture_handle_reservation_workload_for_bench,
        texture_handle_reservation_workload_legacy_for_bench,
    },
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const KEY_COUNT: usize = 96;
const REPLACEMENTS: usize = 6;
const LOOKUP_ROUNDS: usize = 12;
const RUNS: usize = 1_000;

type Workload = fn(&[&str], usize, usize) -> u64;

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
    let owned_keys = (0..KEY_COUNT)
        .map(|index| {
            format!(
                "noteskins/dance/player-{}/texture-{index:03} {}x{}.png",
                index % 2 + 1,
                index % 8 + 1,
                index % 5 + 1,
            )
        })
        .collect::<Vec<_>>();
    let keys = owned_keys.iter().map(String::as_str).collect::<Vec<_>>();

    run_benchmark(
        "texture handle reservation",
        texture_handle_reservation_workload_legacy_for_bench,
        texture_handle_reservation_workload_for_bench,
        &keys,
    );
    run_benchmark(
        "texture metadata registration and lookup",
        texture_metadata_workload_legacy_for_bench,
        texture_metadata_workload_for_bench,
        &keys,
    );
    run_benchmark(
        "generated texture replacement and lookup",
        generated_texture_workload_legacy_for_bench,
        generated_texture_workload_for_bench,
        &keys,
    );
}

fn run_benchmark(label: &str, legacy: Workload, optimized: Workload, keys: &[&str]) {
    assert_eq!(
        legacy(keys, REPLACEMENTS, LOOKUP_ROUNDS),
        optimized(keys, REPLACEMENTS, LOOKUP_ROUNDS)
    );

    let old = measure(legacy, keys);
    let new = measure(optimized, keys);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "{label} ({KEY_COUNT} keys x {REPLACEMENTS} replacements x {LOOKUP_ROUNDS} lookup rounds x {RUNS} runs)"
    );
    print_result("old", &old);
    print_result("new", &new);
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

fn measure(workload: Workload, keys: &[&str]) -> BenchResult {
    for _ in 0..20 {
        black_box(workload(keys, REPLACEMENTS, LOOKUP_ROUNDS));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7)
            ^ black_box(workload(keys, REPLACEMENTS, LOOKUP_ROUNDS))
            ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (KEY_COUNT * (REPLACEMENTS + 1 + LOOKUP_ROUNDS) * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/op {:>7.2} cycles/op {:>7.1} Mops/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.3}/{:.3} per op, {:.1} bytes/op",
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
