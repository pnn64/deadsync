use deadsync_input::keymap::InputState;
use deadsync_input::{
    GamepadCodeBinding, InputBinding, KeyCode, Keymap, PAD_ID_COUNT_CAP, PadCode, PadDir, PadEvent,
    PadId, RawKeyboardEvent, VirtualAction,
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
    keymap.bind(
        VirtualAction::p1_up,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: Some(PAD_ID_COUNT_CAP - 1),
            uuid: None,
        })],
    );
    keymap.bind(
        VirtualAction::p1_right,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: None,
            uuid: Some([7; 16]),
        })],
    );
    keymap.bind(
        VirtualAction::p2_up,
        &[InputBinding::PadDirOn {
            device: PAD_ID_COUNT_CAP - 1,
            dir: PadDir::Up,
        }],
    );
    let mut input = InputState::new(&keymap, 0.2);

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
    let dir_press = PadEvent::Dir {
        id: pad_id,
        timestamp,
        host_nanos: 7,
        dir: PadDir::Up,
        pressed: true,
    };
    let dir_release = PadEvent::Dir {
        id: pad_id,
        timestamp,
        pressed: false,
        host_nanos: 8,
        dir: PadDir::Up,
    };

    let before = ALLOC.begin();
    let mut emitted = 0u64;
    input
        .map_key(input.key_event(key_press), || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_pad(&pad_press, || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_pad(&dir_press, || timestamp)
        .for_each(|_| emitted += 1);
    for _ in 0..10_000 {
        input
            .map_key(input.key_event(black_box(key_release)), || timestamp)
            .for_each(|_| emitted += 1);
        input
            .map_key(input.key_event(black_box(key_press)), || timestamp)
            .for_each(|_| emitted += 1);
        input
            .map_pad(black_box(&pad_release), || timestamp)
            .for_each(|_| emitted += 1);
        input
            .map_pad(black_box(&pad_press), || timestamp)
            .for_each(|_| emitted += 1);
        input
            .map_pad(black_box(&dir_release), || timestamp)
            .for_each(|_| emitted += 1);
        input
            .map_pad(black_box(&dir_press), || timestamp)
            .for_each(|_| emitted += 1);
    }
    input
        .map_key(input.key_event(key_release), || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_key(input.key_event(key_unmapped), || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_key(input.key_event(key_repeat), || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_pad(&pad_release, || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_pad(&dir_release, || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_pad(&pad_unmapped, || timestamp)
        .for_each(|_| emitted += 1);
    input
        .map_pad(&pad_axis, || timestamp)
        .for_each(|_| emitted += 1);
    while let Some(events) = input.next_due(timestamp) {
        emitted += events.count() as u64;
    }
    input.clear();
    let allocated = ALLOC.finish(before);

    assert!(emitted >= 3, "the measured paths must emit mapped input");
    assert_eq!(allocated.0, 0, "allocation operations in input hot paths");
    assert_eq!(allocated.1, 0, "allocated bytes in input hot paths");

    input
        .map_key(input.key_event(key_press), || timestamp)
        .for_each(|_| {});
    input
        .map_key(input.key_event(key_release), || timestamp)
        .for_each(|_| {});
    input.map_pad(&pad_press, || timestamp).for_each(|_| {});
    input.map_pad(&pad_release, || timestamp).for_each(|_| {});
    let now = timestamp + std::time::Duration::from_millis(210);
    let before = ALLOC.begin();
    let mut flushed = false;
    while let Some(events) = input.next_due(now) {
        flushed = true;
        events.for_each(|event| {
            black_box(event);
        });
    }
    let allocated = ALLOC.finish(before);
    assert!(flushed, "the measured drain path must flush due edges");
    assert_eq!(allocated.0, 0, "allocation operations while draining input");
    assert_eq!(allocated.1, 0, "allocated bytes while draining input");
}
