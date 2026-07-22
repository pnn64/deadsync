use deadlib_present::font::{
    font_import_candidate_indices_for_bench, font_import_candidate_indices_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CANDIDATES: usize = 8_192;
const RUNS: usize = 100;
const TARGET: &str = "_game chars 36px";
type Matcher = fn(&str, &[PathBuf]) -> Vec<usize>;

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
    let paths = (0..CANDIDATES)
        .map(|index| {
            let source = index.wrapping_mul(4_099) % CANDIDATES;
            match source % 5 {
                0 => PathBuf::from(format!("_GAME CHARS 36PX {source}x1 (doubleres).INI")),
                1 => PathBuf::from(format!("_game chars 36px-other-{source}.ini")),
                2 => PathBuf::from(format!("fallback font {source} 4x1.ini")),
                3 => PathBuf::from(format!("_game chars 36px (hint-{source}).InI")),
                _ => PathBuf::from(format!("_game chars 36px {source}.png")),
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(
        font_import_candidate_indices_legacy_for_bench(TARGET, &paths),
        font_import_candidate_indices_for_bench(TARGET, &paths)
    );
    let old = measure(&paths, font_import_candidate_indices_legacy_for_bench);
    let new = measure(&paths, font_import_candidate_indices_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("font import fallback scan ({CANDIDATES} paths x {RUNS} runs)");
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

fn measure(paths: &[PathBuf], matcher: Matcher) -> BenchResult {
    for _ in 0..3 {
        black_box(matcher(TARGET, black_box(paths)));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        let matches = matcher(black_box(TARGET), black_box(paths));
        checksum = checksum.rotate_left(7) ^ matches.len() as u64 ^ run as u64;
        black_box(matches);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let paths = (CANDIDATES * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.1} ns/path {:>8.1} cycles/path {:>7.2} Mpaths/s",
        result.elapsed.as_secs_f64() * 1.0e9 / paths,
        result.cycles as f64 / paths,
        paths / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per path, {:.1} bytes/path",
        result.alloc.allocs as f64 / paths,
        result.alloc.reallocs as f64 / paths,
        result.alloc.bytes as f64 / paths,
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
