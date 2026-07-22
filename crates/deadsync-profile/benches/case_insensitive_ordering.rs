use deadsync_profile::favorites_view::{ascii_case_insensitive_cmp, unicode_case_insensitive_cmp};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cmp::Ordering as CmpOrdering;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const NAMES: usize = 1_024;
const RUNS: usize = 200;
type Comparator = fn(&str, &str) -> CmpOrdering;

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

// SAFETY: allocation requests are forwarded unchanged; atomics only observe them.
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
    let names = (0..NAMES)
        .map(|index| {
            let source = index.wrapping_mul(509) % NAMES;
            match source % 4 {
                0 => format!("PROFILE {source:04} – İstanbul"),
                1 => format!("profile {source:04} – Σteps"),
                2 => format!("Pack {source:04} – Über Mix"),
                _ => format!("pack {source:04} – CAFÉ"),
            }
        })
        .collect::<Vec<_>>();

    run_pair(
        "Unicode profile ordering",
        &names,
        unicode_legacy_cmp,
        unicode_case_insensitive_cmp,
    );
    run_pair(
        "ASCII pack ordering",
        &names,
        ascii_legacy_cmp,
        ascii_case_insensitive_cmp,
    );
}

fn run_pair(label: &str, names: &[String], old_cmp: Comparator, new_cmp: Comparator) {
    let mut expected = names.to_vec();
    expected.sort_by(|left, right| old_cmp(left, right));
    let mut actual = names.to_vec();
    actual.sort_by(|left, right| new_cmp(left, right));
    assert_eq!(expected, actual, "{label} changed");

    let old = measure(batch_fixture(names), old_cmp);
    let new = measure(batch_fixture(names), new_cmp);
    assert_eq!(old.checksum, new.checksum);
    println!("{label} ({NAMES} names x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn batch_fixture(names: &[String]) -> Vec<Vec<String>> {
    (0..RUNS).map(|_| names.to_vec()).collect()
}

fn measure(mut batches: Vec<Vec<String>>, cmp: Comparator) -> BenchResult {
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for names in &mut batches {
        names.sort_by(|left, right| cmp(black_box(left), black_box(right)));
        checksum = checksum.rotate_left(7) ^ names.first().map_or(0, String::len) as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn unicode_legacy_cmp(left: &str, right: &str) -> CmpOrdering {
    left.to_lowercase().cmp(&right.to_lowercase())
}

fn ascii_legacy_cmp(left: &str, right: &str) -> CmpOrdering {
    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (NAMES * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.1} ns/name {:>8.1} cycles/name {:>7.2} Mnames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per name, {:.1} bytes/name",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
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
