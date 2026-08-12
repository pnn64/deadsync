use deadsync_input::{
    GamepadCodeBinding, InputBinding, KeyCode, Keymap, PadCode, PadEvent, PadId, RawKeyboardEvent,
    VirtualAction, clear_debounce_state, drain_debounced_input_events_with, map_keycode_event_with,
    map_pad_event_with, map_raw_key_event_with, set_input_debounce_seconds, set_keymap,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Instant, SystemTime};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CHATTER_PAIRS: usize = 250_000;
const DIRECT_EVENTS: usize = 1_000_000;
const PROBE_EVENTS: usize = 1_000_000;

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
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }

    fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.deallocs
    }
}

struct BenchResult {
    ns_per_event: f64,
    cycles_per_event: Option<f64>,
    events_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(events: usize, operation: impl FnOnce() -> u64) -> BenchResult {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(operation());
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_event: seconds * 1_000_000_000.0 / events as f64,
        cycles_per_event: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / events as f64),
        events_per_second: events as f64 / seconds,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<30} {:>9.2} ns/event  {:>9.2} cycles/event  {:>8.2} Mevent/s  \
         {:>5} alloc  {:>5} realloc  {:>5} free  {:>9} bytes  {:016x}",
        result.ns_per_event,
        result.cycles_per_event.unwrap_or(f64::NAN),
        result.events_per_second / 1_000_000.0,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.bytes,
        result.checksum,
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocated.operations(), 0);
    assert_eq!(result.allocated.bytes, 0);
}

fn input_keymap() -> Keymap {
    let mut keymap = Keymap::default();
    keymap.bind(
        VirtualAction::p1_left,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    keymap.bind(
        VirtualAction::p1_down,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: None,
            uuid: None,
        })],
    );
    keymap
}

fn cache_refresh_allocation() {
    set_keymap(input_keymap());
    let timestamp = Instant::now();
    let result = measure(1, || {
        let mut checksum = 0;
        map_keycode_event_with(KeyCode::ArrowLeft, true, timestamp, |event| {
            checksum ^= event.action as u64;
        });
        checksum
    });
    println!("\ncompiled-keymap refresh after configuration");
    print_result("first mapped key", &result);
    assert_zero_alloc(&result);
}

fn keyboard_chatter() {
    set_input_debounce_seconds(0.2);
    clear_debounce_state();
    let timestamp = Instant::now();
    let press = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 1,
    };
    let release = RawKeyboardEvent {
        pressed: false,
        host_nanos: 2,
        ..press
    };
    map_raw_key_event_with(&press, |_| {});
    let result = measure(CHATTER_PAIRS * 2, || {
        let mut checksum = 0u64;
        for _ in 0..CHATTER_PAIRS {
            map_raw_key_event_with(black_box(&release), |event| {
                checksum = checksum.wrapping_add(event.pressed as u64 + 1);
            });
            map_raw_key_event_with(black_box(&press), |event| {
                checksum = checksum.wrapping_add(event.pressed as u64 + 1);
            });
        }
        checksum
    });
    println!("\nkeyboard release/repress chatter ({CHATTER_PAIRS} pairs)");
    print_result("mapped + debounced", &result);
    assert_zero_alloc(&result);
}

fn pad_chatter() {
    clear_debounce_state();
    let timestamp = Instant::now();
    let press = PadEvent::RawButton {
        id: PadId(64),
        timestamp,
        host_nanos: 1,
        code: PadCode(77),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };
    let release = PadEvent::RawButton {
        id: PadId(64),
        timestamp,
        host_nanos: 2,
        code: PadCode(77),
        uuid: [7; 16],
        value: 0.0,
        pressed: false,
    };
    let cold = measure(1, || {
        let mut checksum = 0;
        map_pad_event_with(&press, |event| checksum ^= event.action as u64);
        checksum
    });
    println!("\nfirst event from highest native pad ID");
    print_result("mapped + debounced", &cold);
    assert_zero_alloc(&cold);

    let result = measure(CHATTER_PAIRS * 2, || {
        let mut checksum = 0u64;
        for _ in 0..CHATTER_PAIRS {
            map_pad_event_with(black_box(&release), |event| {
                checksum = checksum.wrapping_add(event.pressed as u64 + 1);
            });
            map_pad_event_with(black_box(&press), |event| {
                checksum = checksum.wrapping_add(event.pressed as u64 + 1);
            });
        }
        checksum
    });
    println!("\npad release/repress chatter ({CHATTER_PAIRS} pairs)");
    print_result("mapped + debounced", &result);
    assert_zero_alloc(&result);
}

fn direct_mapping() {
    let timestamp = Instant::now();
    let result = measure(DIRECT_EVENTS, || {
        let mut checksum = 0u64;
        for index in 0..DIRECT_EVENTS {
            map_keycode_event_with(
                black_box(KeyCode::ArrowLeft),
                index & 1 == 0,
                timestamp,
                |event| {
                    checksum = checksum.wrapping_add(event.pressed as u64 + 1);
                },
            );
        }
        checksum
    });
    println!("\ndirect compiled keyboard mapping ({DIRECT_EVENTS} events)");
    print_result("mapped + normalized", &result);
    assert_zero_alloc(&result);
}

fn pipeline_probes() {
    clear_debounce_state();
    let timestamp = Instant::now();
    let mapped = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 1,
    };
    let unmapped = RawKeyboardEvent {
        code: KeyCode::KeyZ,
        ..mapped
    };
    let repeat = RawKeyboardEvent {
        repeat: true,
        ..mapped
    };
    map_raw_key_event_with(&mapped, |_| {});

    let mapped_pad = PadEvent::RawButton {
        id: PadId(64),
        timestamp,
        host_nanos: 2,
        code: PadCode(77),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };
    let unmapped_pad = PadEvent::RawButton {
        id: PadId(64),
        timestamp,
        host_nanos: 2,
        code: PadCode(78),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };
    map_pad_event_with(&mapped_pad, |_| {});

    for (label, event) in [
        ("raw key duplicate", mapped),
        ("raw key unmapped", unmapped),
        ("raw key repeat", repeat),
    ] {
        let result = measure(PROBE_EVENTS, || {
            let mut checksum = 0u64;
            for _ in 0..PROBE_EVENTS {
                map_raw_key_event_with(black_box(&event), |input| {
                    checksum ^= input.action as u64 + 1;
                });
            }
            checksum
        });
        print_result(label, &result);
        assert_zero_alloc(&result);
    }

    for (label, event) in [
        ("raw pad duplicate", mapped_pad),
        ("raw pad unmapped", unmapped_pad),
    ] {
        let result = measure(PROBE_EVENTS, || {
            let mut checksum = 0u64;
            for _ in 0..PROBE_EVENTS {
                map_pad_event_with(black_box(&event), |input| {
                    checksum ^= input.action as u64 + 1;
                });
            }
            checksum
        });
        print_result(label, &result);
        assert_zero_alloc(&result);
    }

    let result = measure(PROBE_EVENTS, || {
        let mut checksum = 0u64;
        for _ in 0..PROBE_EVENTS {
            checksum ^= drain_debounced_input_events_with(|input| {
                black_box(input);
            }) as u64;
        }
        checksum
    });
    print_result("empty debounce drain", &result);
    assert_zero_alloc(&result);

    let result = measure(PROBE_EVENTS, || {
        let mut checksum = 0u64;
        for _ in 0..PROBE_EVENTS {
            checksum ^= black_box(Instant::now())
                .duration_since(timestamp)
                .as_nanos() as u64;
        }
        checksum
    });
    print_result("Instant::now", &result);
    assert_zero_alloc(&result);
}

fn main() {
    // Make wall-clock initialization happen before allocation measurement.
    black_box(SystemTime::now());
    cache_refresh_allocation();
    keyboard_chatter();
    pad_chatter();
    direct_mapping();
    println!("\ninput pipeline probes ({PROBE_EVENTS} calls each)");
    pipeline_probes();
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
