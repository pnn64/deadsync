use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const POLLS: usize = 256;
const RUNS: usize = 5_000;

type Workload = fn() -> u64;

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
// independent atomics only observe successful allocation and growth calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this exact layout.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the allocator caller guarantees this pointer/layout is live.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the allocator caller supplied the live pointer and old layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
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
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn sample_event_time() {
    black_box(Instant::now());
    black_box(deadlib_platform::host_time::now_nanos());
}

fn sample_host_time() {
    black_box(deadlib_platform::host_time::now_nanos());
}

fn wgi_idle_legacy() -> u64 {
    let mut checksum = 0_u64;
    for poll in 0..POLLS {
        sample_event_time();
        let state_changed = poll % 127 == 0;
        checksum = checksum.rotate_left(5) ^ u64::from(state_changed);
    }
    checksum
}

fn wgi_idle_gated() -> u64 {
    let mut checksum = 0_u64;
    let mut last_raw = 0_usize;
    for poll in 0..POLLS {
        let raw = poll / 8;
        let state_changed = poll % 127 == 0;
        if state_changed {
            sample_event_time();
            last_raw = raw;
        } else if raw != last_raw {
            sample_host_time();
            last_raw = raw;
        }
        checksum = checksum.rotate_left(5) ^ u64::from(state_changed);
    }
    checksum
}

fn raw_keyboard_legacy() -> u64 {
    let mut held = false;
    let mut checksum = 0_u64;
    for message in 0..POLLS {
        sample_event_time();
        let pressed = (message / 64) % 2 == 0;
        if held != pressed {
            held = pressed;
            checksum = checksum.rotate_left(5) ^ message as u64 ^ u64::from(pressed);
        }
    }
    checksum
}

fn raw_keyboard_gated() -> u64 {
    let mut held = false;
    let mut checksum = 0_u64;
    for message in 0..POLLS {
        let pressed = (message / 64) % 2 == 0;
        if held != pressed {
            held = pressed;
            sample_event_time();
            checksum = checksum.rotate_left(5) ^ message as u64 ^ u64::from(pressed);
        }
    }
    checksum
}

fn main() {
    benchmark_pair(
        "WGI mostly-stale 1 kHz polling",
        "256 polls, hardware timestamp advances every 8 polls, 3 state changes",
        wgi_idle_legacy,
        wgi_idle_gated,
    );
    benchmark_pair(
        "Raw Input held-key repeats",
        "256 messages, 4 accepted transitions",
        raw_keyboard_legacy,
        raw_keyboard_gated,
    );
}

fn benchmark_pair(label: &str, fixture: &str, old_workload: Workload, new_workload: Workload) {
    assert_eq!(old_workload(), new_workload());
    let old = measure(old_workload);
    let new = measure(new_workload);
    assert_eq!(old.checksum, new.checksum);
    println!("{label}\n  {fixture}, {RUNS} runs");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%\n",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs,
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn measure(workload: Workload) -> BenchResult {
    for _ in 0..20 {
        black_box(workload());
    }
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(workload)() ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = RUNS as f64;
    println!(
        "  {label:<4} {:>8.2} us/run {:>10.0} cycles/run {:>8.1} Kruns/s",
        result.elapsed.as_secs_f64() * 1.0e6 / runs,
        result.cycles as f64 / runs,
        runs / result.elapsed.as_secs_f64() / 1.0e3,
    );
    println!(
        "       allocs={} reallocs={} bytes={}",
        result.alloc.allocs, result.alloc.reallocs, result.alloc.bytes,
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
