use deadsync_assets::dynamic_media::{
    dynamic_video_key_set, dynamic_video_key_set_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 5_000;
const PATH_COUNT: usize = 256;

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
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
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
    let paths = fixture_paths();
    assert_eq!(old_checksum(&paths), new_checksum(&paths));

    let old = measure(&paths, old_checksum);
    let new = measure(&paths, new_checksum);
    assert_eq!(old.checksum, new.checksum);

    println!("dynamic video desired-key construction ({PATH_COUNT} paths x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn fixture_paths() -> Vec<PathBuf> {
    (0..PATH_COUNT)
        .map(|index| {
            let extension = match index % 8 {
                0 => "png",
                1 => "jpg",
                2 => "MP4",
                3 => "avi",
                4 => "webm",
                5 => "mpeg",
                6 => "mov",
                _ => "mkv",
            };
            PathBuf::from(format!(
                "Songs/Pack {}/{:03}/movie-{index:03}.{extension}",
                index % 12,
                index % 80
            ))
        })
        .collect()
}

fn old_checksum(paths: &[PathBuf]) -> u64 {
    let keys = dynamic_video_key_set_legacy_for_bench(paths);
    keys.iter().fold(keys.len() as u64, |checksum, key| {
        checksum.wrapping_add(key_checksum(key))
    })
}

fn new_checksum(paths: &[PathBuf]) -> u64 {
    let keys = dynamic_video_key_set(paths);
    keys.iter().fold(keys.len() as u64, |checksum, key| {
        checksum.wrapping_add(key_checksum(key))
    })
}

fn key_checksum(key: &str) -> u64 {
    key.bytes().fold(key.len() as u64, |checksum, byte| {
        checksum.wrapping_mul(31).wrapping_add(u64::from(byte))
    })
}

fn measure(paths: &[PathBuf], checksum: fn(&[PathBuf]) -> u64) -> BenchResult {
    for _ in 0..100 {
        black_box(checksum(black_box(paths)));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut total = 0_u64;
    for run in 0..RUNS {
        total = total.rotate_left(7) ^ black_box(checksum(black_box(paths))) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: total,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (PATH_COUNT * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/path {:>7.2} cycles/path {:>7.1} Mpaths/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.3}/{:.3} per path, {:.1} bytes/path",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
    );
}

fn print_reduction(old: &BenchResult, new: &BenchResult) {
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

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads only serialize measurement.
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
