use deadsync_input::fsr::{ButtonView, PadView, SensorView, SensorViews};
use deadsync_input_fsr::bench_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const INLINE_EVENTS: usize = 2_000_000;
const GROUP_EVENTS: usize = 500_000;
const NORMALIZE_EVENTS: usize = 20_000_000;
const SAMPLES: usize = 32;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

// SAFETY: every operation delegates to `System` with the caller-provided
// pointer and layout. Relaxed atomics only observe benchmark allocation churn.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
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

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.deallocs
    }
}

struct BenchResult {
    ns_per_event: f64,
    cycles_per_event: Option<f64>,
    events_per_second: f64,
    worst_ns_per_event: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(events: usize, operation: fn(usize) -> u64) -> BenchResult {
    let sample_events = events / 20;
    black_box(operation(sample_events));

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(operation(events));
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);

    let mut worst_ns_per_event = 0.0f64;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        black_box(operation(sample_events));
        worst_ns_per_event = worst_ns_per_event
            .max(started.elapsed().as_secs_f64() * 1_000_000_000.0 / sample_events as f64);
    }

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_event: seconds * 1_000_000_000.0 / events as f64,
        cycles_per_event: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / events as f64),
        events_per_second: events as f64 / seconds,
        worst_ns_per_event,
        allocated,
        checksum,
    }
}

fn print_pair(name: &str, old: &BenchResult, new: &BenchResult, old_allocates: bool) {
    println!("\n{name}");
    print_result("old", old);
    print_result("new", new);
    assert_eq!(new.checksum, old.checksum);
    if old_allocates {
        assert!(old.allocated.operations() > 0);
        assert!(old.allocated.bytes > 0);
    } else {
        assert_eq!(old.allocated.operations(), 0);
        assert_eq!(old.allocated.bytes, 0);
    }
    assert_eq!(new.allocated.operations(), 0);
    assert_eq!(new.allocated.bytes, 0);
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<4} {:>8.2} ns/event  {:>8.2} cycles/event  {:>7.2} Mevent/s  \
         worst {:>8.2} ns  {:>8} alloc  {:>3} realloc  {:>8} free  {:>12} bytes  {:016x}",
        result.ns_per_event,
        result.cycles_per_event.unwrap_or(f64::NAN),
        result.events_per_second / 1_000_000.0,
        result.worst_ns_per_event,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.bytes,
        result.checksum,
    );
}

fn main() {
    println!(
        "FSR view sizes: sensor={} inline_sensors={} button={} pad={} bytes",
        size_of::<SensorView>(),
        size_of::<SensorViews>(),
        size_of::<ButtonView>(),
        size_of::<PadView>(),
    );
    print_pair(
        "Four-sensor button storage",
        &measure(INLINE_EVENTS, bench_support::inline_sensors_old),
        &measure(INLINE_EVENTS, bench_support::inline_sensors_new),
        true,
    );
    print_pair(
        "FSRIO four-button sensor grouping",
        &measure(GROUP_EVENTS, bench_support::sensor_groups_old),
        &measure(GROUP_EVENTS, bench_support::sensor_groups_new),
        true,
    );
    print_pair(
        "FSRIO normalized sensor value",
        &measure(NORMALIZE_EVENTS, bench_support::normalization_old),
        &measure(NORMALIZE_EVENTS, bench_support::normalization_new),
        false,
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
