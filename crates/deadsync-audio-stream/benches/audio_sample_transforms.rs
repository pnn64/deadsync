use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use deadsync_audio::f32_to_i16;

const FRAMES: usize = 4_096;
const CHANNELS: usize = 2;
const SAMPLES: usize = FRAMES * CHANNELS;
const WARMUP_RUNS: usize = 1_000;
const MEASURE_RUNS: usize = 20_000;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// atomics only observe successful operations.
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
        self.frees.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller supplies the allocation's original pointer and layout.
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
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
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

fn callback_old(sample: f32) -> i16 {
    let sample = sample.clamp(-1.0, 1.0);
    if sample >= 1.0 {
        i16::MAX
    } else if sample <= -1.0 {
        i16::MIN
    } else {
        (sample * 32_768.0) as i16
    }
}

fn measure_convert(input: &[f32], convert: fn(f32) -> i16) -> BenchResult {
    let mut out = vec![0i16; input.len()];
    let mut checksum = 0u64;
    for _ in 0..WARMUP_RUNS {
        for (dst, &src) in out.iter_mut().zip(black_box(input)) {
            *dst = convert(src);
        }
        black_box(&out);
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    for run in 0..MEASURE_RUNS {
        for (dst, &src) in out.iter_mut().zip(black_box(input)) {
            *dst = convert(src);
        }
        checksum = checksum.rotate_left(5) ^ out[run % out.len()] as u16 as u64;
        black_box(&out);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let samples = (SAMPLES * MEASURE_RUNS) as f64;
    println!(
        "  {label:<9} {:>7.3} ns/sample  {:>7.3} cycles/sample  {:>7.1} Msamples/s  \
         {:>5.3} alloc/realloc/free per run  {:>6.1} B/run",
        result.elapsed.as_secs_f64() * 1.0e9 / samples,
        result.cycles as f64 / samples,
        samples / result.elapsed.as_secs_f64() / 1.0e6,
        (result.alloc.allocs + result.alloc.reallocs + result.alloc.frees) as f64
            / MEASURE_RUNS as f64,
        result.alloc.bytes as f64 / MEASURE_RUNS as f64,
    );
}

fn print_pair(name: &str, old: &BenchResult, candidate: &BenchResult) {
    assert_eq!(old.checksum, candidate.checksum);
    println!("{name}");
    print_result("old", old);
    print_result("candidate", candidate);
    println!(
        "  speedup {:.2}x | cycle reduction {:.1}%",
        old.elapsed.as_secs_f64() / candidate.elapsed.as_secs_f64(),
        100.0 * (1.0 - candidate.cycles as f64 / old.cycles as f64),
    );
}

fn main() {
    let float_input = (0..SAMPLES)
        .map(|index| {
            let unit = (index.wrapping_mul(1_103_515_245) as u32) as f32 / u32::MAX as f32;
            unit.mul_add(3.0, -1.5)
        })
        .collect::<Vec<_>>();
    println!("audio sample transforms ({FRAMES} stereo frames x {MEASURE_RUNS} runs)");
    print_pair(
        "callback f32 -> i16",
        &measure_convert(&float_input, callback_old),
        &measure_convert(&float_input, f32_to_i16),
    );
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
