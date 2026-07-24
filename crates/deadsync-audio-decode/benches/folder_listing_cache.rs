use deadsync_audio_decode::folder::OggListingBenchFixture;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FILES: usize = 64;
const OPERATIONS: usize = 100_000;

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
    checksum: usize,
}

fn main() {
    let fixture = OggListingBenchFixture::new(FILES);
    assert_eq!(fixture.legacy_key_probe(), fixture.shared_key_probe());
    assert_eq!(
        fixture.legacy_listing_snapshot(),
        *fixture.shared_listing_snapshot()
    );
    for index in 0..FILES {
        assert_eq!(
            fixture.legacy_random_pick(index),
            fixture.shared_random_pick(index)
        );
    }

    run_case(
        "borrowed cache-key probe",
        || fixture.legacy_key_probe(),
        || fixture.shared_key_probe(),
    );
    run_case(
        "shared listing snapshot",
        || path_checksum(black_box(fixture.legacy_listing_snapshot())),
        || shared_path_checksum(black_box(fixture.shared_listing_snapshot())),
    );
    let legacy_index = Cell::new(0);
    let shared_index = Cell::new(0);
    run_case(
        "random track selection",
        || {
            let index = legacy_index.get();
            let picked = fixture
                .legacy_random_pick(index)
                .expect("non-empty fixture");
            legacy_index.set(index + 1);
            path_checksum_one(black_box(picked))
        },
        || {
            let index = shared_index.get();
            let picked = fixture
                .shared_random_pick(index)
                .expect("non-empty fixture");
            shared_index.set(index + 1);
            path_checksum_one(black_box(picked))
        },
    );
}

fn run_case(label: &str, mut legacy: impl FnMut() -> usize, mut optimized: impl FnMut() -> usize) {
    for _ in 0..1_000 {
        black_box(legacy());
        black_box(optimized());
    }
    let old = measure(&mut legacy);
    let new = measure(&mut optimized);
    assert_eq!(old.checksum, new.checksum);

    println!("{label} ({FILES} cached paths, {OPERATIONS} operations)");
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

fn measure(operation: &mut impl FnMut() -> usize) -> BenchResult {
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_usize;
    for index in 0..OPERATIONS {
        checksum = checksum
            .rotate_left(5)
            .wrapping_add(black_box(operation()))
            .wrapping_add(index);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn path_checksum(paths: Vec<PathBuf>) -> usize {
    paths.iter().fold(paths.len(), |sum, path| {
        sum.wrapping_add(path.as_os_str().len())
    })
}

fn shared_path_checksum(paths: Arc<Vec<PathBuf>>) -> usize {
    paths.iter().fold(paths.len(), |sum, path| {
        sum.wrapping_add(path.as_os_str().len())
    })
}

fn path_checksum_one(path: PathBuf) -> usize {
    path.as_os_str().len()
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = OPERATIONS as f64;
    println!(
        "  {label:<4} {:>8.1} ns/op {:>8.1} cycles/op {:>7.2} Mops/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per op, {:.1} bytes/op",
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
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measurement interval.
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
