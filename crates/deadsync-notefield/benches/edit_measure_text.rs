use deadlib_present::actors::TextContent;
use deadsync_notefield::{benchmark_edit_measure_text, benchmark_edit_measure_text_legacy};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FRAMES: usize = 200_000;
const LABELS_PER_FRAME: usize = 16;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation requests are forwarded to `System` unchanged; atomics
// only observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this layout.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller supplied a live pointer and its original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplied a live pointer and its original layout.
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
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
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
    let old = measure(benchmark_edit_measure_text_legacy);
    let new = measure(benchmark_edit_measure_text);
    assert_eq!(old.checksum, new.checksum);

    println!("edit/practice measure labels: {FRAMES} frames x {LABELS_PER_FRAME} labels");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  speedup={:.2}x allocation reduction={:.2}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        old.alloc.allocs.saturating_sub(new.alloc.allocs) as f64 * 100.0
            / old.alloc.allocs.max(1) as f64,
    );
}

fn measure(make: fn(u64) -> TextContent) -> BenchResult {
    black_box(run(make, 128));
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let before_cycles = read_cycles();
    let checksum = black_box(run(make, FRAMES));
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn run(make: fn(u64) -> TextContent, frames: usize) -> u64 {
    let mut checksum = 0u64;
    for frame in 0..frames {
        let first_measure = (frame as u64).wrapping_mul(7) % 100_000;
        for visible in 0..LABELS_PER_FRAME {
            let text = black_box(make(first_measure + visible as u64));
            let bytes = black_box(text.as_str().as_bytes());
            checksum = checksum
                .wrapping_add(bytes.len() as u64)
                .wrapping_add(bytes.first().copied().unwrap_or_default() as u64)
                .wrapping_add(bytes.last().copied().unwrap_or_default() as u64);
        }
    }
    checksum
}

fn print_result(label: &str, result: &BenchResult) {
    let labels = (FRAMES * LABELS_PER_FRAME) as f64;
    println!(
        "  {label:<3} {:>7.2} ns/label {:>10.0} labels/s {:>8.1} cycles/label alloc/realloc/dealloc={:.2}/{:.2}/{:.2} per frame bytes={:.0}/frame",
        result.elapsed.as_secs_f64() * 1.0e9 / labels,
        labels / result.elapsed.as_secs_f64(),
        result.cycles as f64 / labels,
        result.alloc.allocs as f64 / FRAMES as f64,
        result.alloc.reallocs as f64 / FRAMES as f64,
        result.alloc.deallocs as f64 / FRAMES as f64,
        result.alloc.bytes as f64 / FRAMES as f64,
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: `_rdtsc` only reads the processor timestamp counter.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
