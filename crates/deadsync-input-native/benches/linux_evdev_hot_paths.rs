use deadsync_input::{KeyCode, PadEvent, PadId};
use deadsync_input_native::{
    emit_dir_edges, emit_hat_axis_edges,
    unix_time::{EventTimeCache, EventTimeSample, event_time},
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Instant;
use winit::keyboard::PhysicalKey;
use winit::platform::scancode::PhysicalKeyExtScancode;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const EVENTS: usize = 4_000_000;
const SAMPLE_EVENTS: usize = 100_000;
const SAMPLES: usize = 32;
const KEY_BATCH: usize = 64;
const KEY_CODES: [u16; 8] = [1, 16, 30, 44, 57, 59, 96, 103];
static KEYBOARD_WINDOW_FOCUSED: AtomicBool = AtomicBool::new(false);
static KEYBOARD_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(true);
static KEYBOARD_CAPTURE_STATE: AtomicU8 = AtomicU8::new(1);
static KEYBOARD_BACKEND_ACTIVE: AtomicBool = AtomicBool::new(true);

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
    black_box(operation(events / 20));
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(operation(events));
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);

    let mut worst_ns_per_event = 0.0f64;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        black_box(operation(SAMPLE_EVENTS));
        worst_ns_per_event = worst_ns_per_event
            .max(started.elapsed().as_secs_f64() * 1_000_000_000.0 / SAMPLE_EVENTS as f64);
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
    assert_eq!(new.allocated.operations(), 0);
    assert_eq!(old.allocated.bytes, 0);
    assert_eq!(new.allocated.bytes, 0);
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<4} {:>8.2} ns/event  {:>8.2} cycles/event  {:>7.2} Mevent/s  \
         worst {:>8.2} ns  {:>3} alloc  {:>3} realloc  {:>3} free  {:>6} bytes  {:016x}",
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

fn timestamp_old(events: usize, sample: EventTimeSample) -> u64 {
    let mut checksum = 0u64;
    for index in 0..events {
        let usec = 400_000 + ((index / 8) % 1_000) as i64;
        let (_, host_nanos) = event_time(black_box(sample), 100, usec);
        checksum = checksum.wrapping_add(host_nanos);
    }
    checksum
}

fn timestamp_new(events: usize, sample: EventTimeSample) -> u64 {
    let mut cache = EventTimeCache::new();
    let mut checksum = 0u64;
    for index in 0..events {
        let usec = 400_000 + ((index / 8) % 1_000) as i64;
        let (_, host_nanos) = cache.event_time(black_box(sample), 100, usec);
        checksum = checksum.wrapping_add(host_nanos);
    }
    checksum
}

const fn event_checksum(checksum: &mut u64, event: PadEvent) {
    if let PadEvent::Dir { dir, pressed, .. } = event {
        *checksum = checksum
            .rotate_left(5)
            .wrapping_add((dir.ix() as u64) << 1 | pressed as u64);
    }
}

fn hat_old(events: usize, timestamp: Instant) -> u64 {
    let mut checksum = 0u64;
    let mut state = [false; 4];
    let mut x = 0;
    let mut y = 0;
    for index in 0..events {
        let horizontal = index & 1 == 0;
        let value = [-1, 0, 1, 0][(index / 2) & 3];
        if horizontal {
            x = value;
        } else {
            y = value;
        }
        emit_dir_edges(
            &mut |event| event_checksum(&mut checksum, event),
            PadId(3),
            &mut state,
            timestamp,
            55,
            [y < 0, y > 0, x < 0, x > 0],
        );
    }
    checksum
        ^ state
            .into_iter()
            .fold(0, |mask, pressed| (mask << 1) | u64::from(pressed))
}

fn hat_new(events: usize, timestamp: Instant) -> u64 {
    let mut checksum = 0u64;
    let mut state = [false; 4];
    for index in 0..events {
        let horizontal = index & 1 == 0;
        let value = [-1, 0, 1, 0][(index / 2) & 3];
        emit_hat_axis_edges(
            &mut |event| event_checksum(&mut checksum, event),
            PadId(3),
            &mut state,
            timestamp,
            55,
            horizontal,
            value,
        );
    }
    checksum
        ^ state
            .into_iter()
            .fold(0, |mask, pressed| (mask << 1) | u64::from(pressed))
}

#[inline(always)]
fn keyboard_capture_active_old() -> bool {
    black_box(&KEYBOARD_WINDOW_FOCUSED).load(Ordering::Relaxed)
        && black_box(&KEYBOARD_CAPTURE_ENABLED).load(Ordering::Relaxed)
}

#[inline(always)]
fn keyboard_capture_active_new() -> bool {
    black_box(&KEYBOARD_CAPTURE_STATE).load(Ordering::Relaxed) == 3
}

#[inline(always)]
fn host_key_code(code: u16) -> Option<KeyCode> {
    let PhysicalKey::Code(code) = PhysicalKey::from_scancode(u32::from(code)) else {
        return None;
    };
    Some(code)
}

fn keyboard_batch_old(events: usize) -> u64 {
    let mut checksum = 0u64;
    for batch_start in (0..events).step_by(KEY_BATCH) {
        for index in batch_start..(batch_start + KEY_BATCH).min(events) {
            let Some(code) = black_box(host_key_code(black_box(KEY_CODES[index & 7]))) else {
                continue;
            };
            if !keyboard_capture_active_old() {
                continue;
            }
            checksum = checksum.wrapping_add(code as u64);
        }
        black_box(&KEYBOARD_BACKEND_ACTIVE).store(true, Ordering::Relaxed);
    }
    checksum
}

fn keyboard_batch_new(events: usize) -> u64 {
    let mut checksum = 0u64;
    for batch_start in (0..events).step_by(KEY_BATCH) {
        for index in batch_start..(batch_start + KEY_BATCH).min(events) {
            if !keyboard_capture_active_new() {
                continue;
            }
            let Some(code) = black_box(host_key_code(black_box(KEY_CODES[index & 7]))) else {
                continue;
            };
            checksum = checksum.wrapping_add(code as u64);
        }
    }
    checksum
}

fn main() {
    let base = Instant::now();
    let sample = EventTimeSample {
        instant: base,
        host_nanos: 200_500_000_000,
        clock_nanos: Some(100_500_000_000),
    };

    let old = measure(EVENTS, |events| timestamp_old(events, sample));
    let new = measure(EVENTS, |events| timestamp_new(events, sample));
    print_pair("same-timestamp evdev report batches", &old, &new);

    let old = measure(EVENTS, |events| hat_old(events, base));
    let new = measure(EVENTS, |events| hat_new(events, base));
    print_pair("hat-axis direction edge filtering", &old, &new);

    KEYBOARD_WINDOW_FOCUSED.store(false, Ordering::Relaxed);
    KEYBOARD_CAPTURE_STATE.store(1, Ordering::Relaxed);
    let old = measure(EVENTS, keyboard_batch_old);
    let new = measure(EVENTS, keyboard_batch_new);
    print_pair("unfocused keyboard event batches", &old, &new);

    KEYBOARD_WINDOW_FOCUSED.store(true, Ordering::Relaxed);
    KEYBOARD_CAPTURE_STATE.store(3, Ordering::Relaxed);
    let old = measure(EVENTS, keyboard_batch_old);
    let new = measure(EVENTS, keyboard_batch_new);
    print_pair("focused keyboard event batches", &old, &new);
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
