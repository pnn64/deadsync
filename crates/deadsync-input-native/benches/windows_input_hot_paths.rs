use deadsync_input_native::{BackendHost, InputThreadPolicy, PadOrderBackend};
use rustc_hash::FxHashMap;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const EVENTS: usize = 4_000_000;
const SAMPLE_EVENTS: usize = 100_000;
const SAMPLES: usize = 32;
static HOST_EPOCH: OnceLock<Instant> = OnceLock::new();

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

fn measure(events: usize, mut operation: impl FnMut(usize) -> u64) -> BenchResult {
    black_box(operation((events / 20).max(1)));
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(operation(events));
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);

    let sample_events = SAMPLE_EVENTS.min(events).max(1);
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
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(name: &str, old: &BenchResult, new: &BenchResult) {
    println!("\n{name}");
    print_result("old", old);
    print_result("new", new);
    assert_eq!(new.checksum, old.checksum);
    assert_eq!(old.allocated.operations(), 0);
    assert_eq!(old.allocated.bytes, 0);
    assert_eq!(new.allocated.operations(), 0);
    assert_eq!(new.allocated.bytes, 0);
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<4} {:>8.2} ns/event  {:>8.2} cycles/event  {:>7.2} Mevent/s  \
         worst {:>8.2} ns  {:>6} alloc  {:>3} realloc  {:>6} free  {:>10} bytes  {:016x}",
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

fn device_lookup_old(events: usize, devices: &HashMap<isize, u64>) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let key = black_box((index & 7) as isize + 1);
        checksum = checksum.wrapping_add(devices.get(&key).copied().unwrap_or_default());
    }
    checksum
}

fn device_lookup_new(events: usize, devices: &FxHashMap<isize, u64>) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let key = black_box((index & 7) as isize + 1);
        checksum = checksum.wrapping_add(devices.get(&key).copied().unwrap_or_default());
    }
    checksum
}

const fn pad_idx(_: PadOrderBackend, _: [u8; 16]) -> u32 {
    0
}

const fn smx_owns(_: Option<u16>, _: Option<u16>) -> bool {
    false
}

fn host_instant_nanos(at: Instant) -> u64 {
    at.duration_since(*HOST_EPOCH.get().expect("benchmark epoch initialized"))
        .as_nanos() as u64
}

fn host_now_nanos() -> u64 {
    host_instant_nanos(Instant::now())
}

const fn qpc_nanos(_: u64) -> Option<u64> {
    None
}

const fn no_thread_boost() -> InputThreadPolicy {
    InputThreadPolicy::none()
}

fn event_time_old(events: usize, host: BackendHost) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let timestamp = Instant::now();
        let host_nanos = host.now_nanos();
        black_box((timestamp, host_nanos));
        checksum = checksum.wrapping_add(index as u64);
    }
    checksum
}

fn event_time_new(events: usize, host: BackendHost) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let (timestamp, host_nanos) = host.sample_time();
        black_box((timestamp, host_nanos));
        checksum = checksum.wrapping_add(index as u64);
    }
    checksum
}

const BUTTON_MASKS: [u16; 10] = [
    1 << 0,
    1 << 1,
    1 << 2,
    1 << 3,
    1 << 4,
    1 << 5,
    1 << 6,
    1 << 7,
    1 << 8,
    1 << 9,
];

fn wgi_buttons_old(events: usize, previous: u16, current: u16) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..events {
        let current = black_box(current);
        for mask in BUTTON_MASKS {
            let new_pressed = current & mask != 0;
            let old_pressed = previous & mask != 0;
            if new_pressed != old_pressed {
                checksum = checksum.wrapping_add(u64::from(mask) | (u64::from(new_pressed) << 32));
            }
        }
    }
    checksum
}

fn wgi_buttons_new(events: usize, previous: u16, current: u16) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..events {
        let current = black_box(current);
        let changed = current ^ previous;
        if changed == 0 {
            continue;
        }
        for mask in BUTTON_MASKS {
            if changed & mask == 0 {
                continue;
            }
            let new_pressed = current & mask != 0;
            checksum = checksum.wrapping_add(u64::from(mask) | (u64::from(new_pressed) << 32));
        }
    }
    checksum
}

fn main() {
    let mut old_devices = HashMap::new();
    let mut new_devices = FxHashMap::default();
    for key in 1..=8 {
        old_devices.insert(key, key as u64);
        new_devices.insert(key, key as u64);
    }
    print_pair(
        "Raw Input device lookup",
        &measure(EVENTS, |events| device_lookup_old(events, &old_devices)),
        &measure(EVENTS, |events| device_lookup_new(events, &new_devices)),
    );

    HOST_EPOCH.get_or_init(Instant::now);
    let host = BackendHost::new(
        pad_idx,
        smx_owns,
        host_now_nanos,
        host_instant_nanos,
        qpc_nanos,
        no_thread_boost,
    );
    print_pair(
        "Windows event-time sample",
        &measure(EVENTS, |events| event_time_old(events, host)),
        &measure(EVENTS, |events| event_time_new(events, host)),
    );

    print_pair(
        "WGI unchanged-button gate on axis readings",
        &measure(EVENTS, |events| wgi_buttons_old(events, 0, 0)),
        &measure(EVENTS, |events| wgi_buttons_new(events, 0, 0)),
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
