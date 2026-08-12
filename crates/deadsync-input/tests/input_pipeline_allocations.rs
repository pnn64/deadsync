use deadsync_input::{
    GamepadCodeBinding, InputBinding, KeyCode, Keymap, PAD_ID_COUNT_CAP, PadCode, PadEvent, PadId,
    RawKeyboardEvent, VirtualAction, clear_debounce_state, drain_debounced_input_events_with,
    map_keycode_event_with, map_pad_event_with, map_raw_key_event_with, set_input_debounce_seconds,
    set_keymap,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    operations: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            operations: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn begin(&self) -> (u64, u64) {
        let before = (
            self.operations.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
        );
        self.enabled.store(true, Ordering::Relaxed);
        before
    }

    fn finish(&self, before: (u64, u64)) -> (u64, u64) {
        self.enabled.store(false, Ordering::Relaxed);
        (
            self.operations.load(Ordering::Relaxed) - before.0,
            self.bytes.load(Ordering::Relaxed) - before.1,
        )
    }
}

// SAFETY: allocation calls delegate unchanged to `System`; relaxed atomics
// only count successful operations while this single test measures.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.operations.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.operations.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.operations.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[test]
fn configured_input_pipeline_is_allocation_free() {
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
    set_input_debounce_seconds(0.2);
    set_keymap(keymap);

    let timestamp = Instant::now();
    let key_press = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 1,
    };
    let key_release = RawKeyboardEvent {
        pressed: false,
        host_nanos: 2,
        ..key_press
    };
    let key_unmapped = RawKeyboardEvent {
        code: KeyCode::KeyZ,
        ..key_press
    };
    let key_repeat = RawKeyboardEvent {
        repeat: true,
        ..key_press
    };
    let pad_id = PadId((PAD_ID_COUNT_CAP - 1) as u32);
    let pad_press = PadEvent::RawButton {
        id: pad_id,
        timestamp,
        host_nanos: 3,
        code: PadCode(77),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };
    let pad_release = PadEvent::RawButton {
        id: pad_id,
        timestamp,
        host_nanos: 4,
        code: PadCode(77),
        uuid: [7; 16],
        value: 0.0,
        pressed: false,
    };
    let pad_axis = PadEvent::RawAxis {
        id: pad_id,
        timestamp,
        host_nanos: 5,
        code: PadCode(8),
        uuid: [7; 16],
        value: 0.5,
    };
    let pad_unmapped = PadEvent::RawButton {
        id: pad_id,
        timestamp,
        host_nanos: 6,
        code: PadCode(78),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };

    let before = ALLOC.begin();
    let mut emitted = 0u64;
    map_keycode_event_with(KeyCode::ArrowLeft, true, timestamp, |_| emitted += 1);
    map_raw_key_event_with(&key_press, |_| emitted += 1);
    map_pad_event_with(&pad_press, |_| emitted += 1);
    for _ in 0..10_000 {
        map_raw_key_event_with(black_box(&key_release), |_| emitted += 1);
        map_raw_key_event_with(black_box(&key_press), |_| emitted += 1);
        map_pad_event_with(black_box(&pad_release), |_| emitted += 1);
        map_pad_event_with(black_box(&pad_press), |_| emitted += 1);
    }
    map_raw_key_event_with(&key_release, |_| emitted += 1);
    map_raw_key_event_with(&key_unmapped, |_| emitted += 1);
    map_raw_key_event_with(&key_repeat, |_| emitted += 1);
    map_pad_event_with(&pad_release, |_| emitted += 1);
    map_pad_event_with(&pad_unmapped, |_| emitted += 1);
    map_pad_event_with(&pad_axis, |_| emitted += 1);
    drain_debounced_input_events_with(|_| emitted += 1);
    clear_debounce_state();
    let allocated = ALLOC.finish(before);

    assert!(emitted >= 3, "the measured paths must emit mapped input");
    assert_eq!(allocated.0, 0, "allocation operations in input hot paths");
    assert_eq!(allocated.1, 0, "allocated bytes in input hot paths");

    map_raw_key_event_with(&key_press, |_| {});
    map_raw_key_event_with(&key_release, |_| {});
    map_pad_event_with(&pad_press, |_| {});
    map_pad_event_with(&pad_release, |_| {});
    std::thread::sleep(std::time::Duration::from_millis(210));
    let before = ALLOC.begin();
    let flushed = black_box(drain_debounced_input_events_with(|event| {
        black_box(event);
    }));
    let allocated = ALLOC.finish(before);
    assert!(flushed, "the measured drain path must flush due edges");
    assert_eq!(allocated.0, 0, "allocation operations while draining input");
    assert_eq!(allocated.1, 0, "allocated bytes while draining input");
}
