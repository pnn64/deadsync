use deadsync_input_native::{AxisCache, HostInstantMap, PadValueKind, classify_pad_value};
use rustc_hash::FxHashMap;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const EVENTS: usize = 4_000_000;
const SAMPLE_EVENTS: usize = 100_000;
const SAMPLES: usize = 32;
const CACHE_BUILDS: usize = 100_000;

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

fn print_pair(name: &str, old: &BenchResult, new: &BenchResult, old_may_allocate: bool) {
    println!("\n{name}");
    print_result("old", old);
    print_result("new", new);
    assert_eq!(new.checksum, old.checksum);
    if !old_may_allocate {
        assert_eq!(old.allocated.operations(), 0);
        assert_eq!(old.allocated.bytes, 0);
    }
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

#[derive(Clone, Copy)]
enum BenchDev {
    Pad(usize),
    Keyboard,
}

fn dispatch_old(events: usize, pads: &HashMap<usize, u64>, keyboards: &HashMap<usize, u64>) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let key = black_box(if index & 3 == 3 {
            100 + (index & 3)
        } else {
            index & 7
        });
        if let Some(value) = pads.get(&key) {
            checksum = checksum.wrapping_add(*value);
        } else if let Some(value) = keyboards.get(&key) {
            checksum = checksum.wrapping_add(*value);
        }
    }
    checksum
}

fn dispatch_new(events: usize, devices: &FxHashMap<usize, BenchDev>, pads: &[u64]) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let key = black_box(if index & 3 == 3 {
            100 + (index & 3)
        } else {
            index & 7
        });
        match devices.get(&key) {
            Some(BenchDev::Pad(index)) => {
                checksum = checksum.wrapping_add(pads[*index]);
            }
            Some(BenchDev::Keyboard) => checksum = checksum.wrapping_add(key as u64 + 1),
            None => {}
        }
    }
    checksum
}

fn instant_nanos(epoch: Instant, at: Instant) -> u64 {
    at.duration_since(epoch).as_nanos() as u64
}

fn map_from_sample(target: u64, sample_host: u64, sample: Instant) -> Instant {
    if target >= sample_host {
        sample
            .checked_add(Duration::from_nanos(target - sample_host))
            .unwrap_or(sample)
    } else {
        sample
            .checked_sub(Duration::from_nanos(sample_host - target))
            .unwrap_or(sample)
    }
}

fn timestamp_old(events: usize, epoch: Instant, target_base: u64) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let target = target_base + (index & 1_023) as u64;
        let sample = Instant::now();
        let sample_host = instant_nanos(epoch, Instant::now());
        black_box(map_from_sample(target, sample_host, sample));
        checksum = checksum.wrapping_add(target);
    }
    checksum
}

fn timestamp_new(events: usize, map: HostInstantMap, target_base: u64) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let target = target_base + (index & 1_023) as u64;
        black_box(map.instant(target));
        checksum = checksum.wrapping_add(target);
    }
    checksum
}

#[derive(Clone, Copy)]
struct OldAxisState {
    code: u32,
    value: i64,
}

fn old_axis_changed(axes: &mut Vec<OldAxisState>, code: u32, value: i64) -> bool {
    for axis in axes.iter_mut() {
        if axis.code != code {
            continue;
        }
        if axis.value == value {
            return false;
        }
        axis.value = value;
        return true;
    }
    axes.push(OldAxisState { code, value });
    true
}

fn axis_old(events: usize) -> u64 {
    let mut axes = Vec::with_capacity(8);
    let mut checksum = 0u64;
    for index in 0..events {
        let usage = 0x30 + (index & 7) as u16;
        let code = 0x0001_0000 | u32::from(usage);
        let value = ((index / 16) & 1) as i64;
        checksum += old_axis_changed(&mut axes, black_box(code), value) as u64;
    }
    checksum
}

fn axis_new(events: usize) -> u64 {
    let mut axes = AxisCache::default();
    let mut checksum = 0u64;
    for index in 0..events {
        let usage = 0x30 + (index & 7) as u16;
        let code = 0x0001_0000 | u32::from(usage);
        checksum += matches!(
            classify_pad_value(
                &mut axes,
                0x01,
                usage,
                black_box(code),
                ((index / 16) & 1) as i64
            ),
            Some(PadValueKind::Axis)
        ) as u64;
    }
    checksum
}

fn axis_cache_old(builds: usize) -> u64 {
    let mut checksum = 0;
    for _ in 0..builds {
        let axes: Vec<OldAxisState> = Vec::with_capacity(8);
        checksum += black_box(axes.len()) as u64;
    }
    checksum
}

fn axis_cache_new(builds: usize) -> u64 {
    let checksum = 0;
    for _ in 0..builds {
        let axes = AxisCache::default();
        black_box(axes);
    }
    checksum
}

fn main() {
    let mut pads = HashMap::new();
    let mut keyboards = HashMap::new();
    let mut devices = FxHashMap::default();
    let mut pad_values = Vec::new();
    for key in 0..8 {
        pads.insert(key, key as u64 + 1);
        pad_values.push(key as u64 + 1);
        devices.insert(key, BenchDev::Pad(key));
    }
    for key in 100..104 {
        keyboards.insert(key, key as u64 + 1);
        devices.insert(key, BenchDev::Keyboard);
    }

    let old = measure(EVENTS, |events| dispatch_old(events, &pads, &keyboards));
    let new = measure(EVENTS, |events| dispatch_new(events, &devices, &pad_values));
    print_pair("IOHID callback device dispatch", &old, &new, false);

    let epoch = Instant::now() - Duration::from_secs(1);
    let anchor = Instant::now();
    let anchor_host = instant_nanos(epoch, anchor);
    let map = HostInstantMap::new(anchor, anchor_host);
    let target_base = anchor_host.saturating_sub(500_000);
    let old = measure(EVENTS, |events| timestamp_old(events, epoch, target_base));
    let new = measure(EVENTS, |events| timestamp_new(events, map, target_base));
    print_pair("Mach timestamp to Instant mapping", &old, &new, false);

    let old = measure(EVENTS, axis_old);
    let new = measure(EVENTS, axis_new);
    print_pair("standard HID axis filtering", &old, &new, true);

    let old = measure(CACHE_BUILDS, axis_cache_old);
    let new = measure(CACHE_BUILDS, axis_cache_new);
    print_pair("axis-cache construction", &old, &new, true);
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
