use deadsync_core::input::InputSource;
use deadsync_input::keymap::InputState;
use deadsync_input::{
    GamepadCodeBinding, InputBinding, InputEvent, KeyCode, Keymap, PadCode, PadDir, PadEvent,
    PadId, RawKeyboardEvent, VirtualAction, any_player_has_dedicated_menu_buttons_for_mode,
    any_player_has_four_way_menu_buttons, any_player_has_three_key_menu_buttons, get_keymap,
    set_keymap, with_keymap,
};
use std::time::{Duration, Instant};

static TEST_GUARD: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

fn lock_test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_GUARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct TestReset(Option<Keymap>);

impl TestReset {
    fn capture() -> Self {
        Self(Some(get_keymap()))
    }
}

impl Drop for TestReset {
    fn drop(&mut self) {
        if let Some(original) = self.0.take() {
            set_keymap(original);
        }
    }
}

fn assert_events_eq(actual: &[InputEvent], expected: &[InputEvent]) {
    assert_eq!(actual.len(), expected.len(), "event count");
    for (actual, expected) in actual.iter().zip(expected.iter()) {
        assert_eq!(actual.action, expected.action);
        assert_eq!(actual.input_slot, expected.input_slot);
        assert_eq!(actual.pressed, expected.pressed);
        assert_eq!(actual.source, expected.source);
        assert_eq!(actual.timestamp, expected.timestamp);
        assert_eq!(actual.timestamp_host_nanos, expected.timestamp_host_nanos);
        assert_eq!(actual.stored_at, expected.stored_at);
        assert_eq!(actual.emitted_at, expected.emitted_at);
    }
}

#[test]
fn keyboard_emits_primary_action_for_pressed_arrow() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    let mut input = InputState::new(&km, 0.0);
    let timestamp = Instant::now();
    let mut actual = Vec::new();
    let raw = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 0,
    };
    input
        .map_key(input.key_event(raw), || timestamp)
        .for_each(|event| {
            actual.push(event);
        });
    let expected = [InputEvent::new(
        VirtualAction::p1_left,
        0,
        true,
        InputSource::Keyboard,
        timestamp,
        0,
        timestamp,
        timestamp,
    )];
    assert_events_eq(&actual, &expected);
}

#[test]
fn pad_emits_primary_action_for_pressed_arrow() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::PadDir(PadDir::Left)],
    );
    let mut input = InputState::new(&km, 0.02);
    let timestamp = Instant::now();
    let event = PadEvent::Dir {
        id: PadId(1),
        timestamp,
        host_nanos: 42,
        dir: PadDir::Left,
        pressed: true,
    };
    let mut actual = Vec::new();
    input
        .map_pad(&event, || timestamp)
        .for_each(|input| actual.push(input));
    assert_eq!(actual.len(), 1, "event count");
    let actual = actual[0];
    assert_eq!(actual.action, VirtualAction::p1_left);
    assert!(actual.pressed);
    assert_eq!(actual.source, InputSource::Gamepad);
    assert_eq!(actual.timestamp, timestamp);
    assert_eq!(actual.timestamp_host_nanos, 42);
    assert!(
        actual.stored_at >= timestamp,
        "debounce storage time should not precede the raw pad timestamp"
    );
    assert_eq!(
        actual.emitted_at, actual.stored_at,
        "initial pad press should emit immediately from the debounce store"
    );
}

#[test]
fn pad_emits_only_matching_device_direction() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_down,
        &[
            InputBinding::PadDirOn {
                device: 2,
                dir: PadDir::Down,
            },
            InputBinding::PadDirOn {
                device: 70,
                dir: PadDir::Down,
            },
        ],
    );
    let mut input = InputState::new(&km, 0.02);
    let timestamp = Instant::now();
    let event = |id| PadEvent::Dir {
        id: PadId(id),
        timestamp,
        host_nanos: 42,
        dir: PadDir::Down,
        pressed: true,
    };

    let mut actual = Vec::new();
    input
        .map_pad(&event(1), || timestamp)
        .for_each(|input| actual.push(input));
    assert!(actual.is_empty());
    input
        .map_pad(&event(2), || timestamp)
        .for_each(|input| actual.push(input));
    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].action, VirtualAction::p1_down);
    assert_eq!(actual[0].input_slot, 9);
    input
        .map_pad(&event(70), || timestamp)
        .for_each(|input| actual.push(input));
    assert_eq!(actual.len(), 2);
    assert_eq!(actual[1].action, VirtualAction::p1_down);
    assert_eq!(actual[1].input_slot, 281);
}

#[test]
fn keyboard_suppresses_pressed_alias_when_primary_is_bound() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    km.bind(
        VirtualAction::p1_menu_left,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    let mut input = InputState::new(&km, 0.0);
    let timestamp = Instant::now();
    let mut actual = Vec::new();
    let raw = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 0,
    };
    input
        .map_key(input.key_event(raw), || timestamp)
        .for_each(|event| {
            actual.push(event);
        });
    let expected = [InputEvent::new(
        VirtualAction::p1_left,
        0,
        true,
        InputSource::Keyboard,
        timestamp,
        0,
        timestamp,
        timestamp,
    )];
    assert_events_eq(&actual, &expected);
}

#[test]
fn keyboard_keeps_release_alias() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    let mut input = InputState::new(&km, 0.0);
    let timestamp = Instant::now();
    let mut actual = Vec::new();
    let raw = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: false,
        repeat: false,
        timestamp,
        host_nanos: 0,
    };
    input
        .map_key(
            input.key_event(RawKeyboardEvent {
                pressed: true,
                ..raw
            }),
            || timestamp,
        )
        .for_each(drop);
    input
        .map_key(input.key_event(raw), || timestamp)
        .for_each(|event| {
            actual.push(event);
        });
    let expected = [
        InputEvent::new(
            VirtualAction::p1_left,
            0,
            false,
            InputSource::Keyboard,
            timestamp,
            0,
            timestamp,
            timestamp,
        ),
        InputEvent::new(
            VirtualAction::p1_menu_left,
            0,
            false,
            InputSource::Keyboard,
            timestamp,
            0,
            timestamp,
            timestamp,
        ),
    ];
    assert_events_eq(&actual, &expected);
}

#[test]
fn dedicated_menu_button_capabilities_distinguish_three_key_from_four_way() {
    let _guard = lock_test_guard();
    let _reset = TestReset::capture();
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_menu_left,
        &[InputBinding::Key(KeyCode::KeyA)],
    );
    km.bind(
        VirtualAction::p1_menu_right,
        &[InputBinding::Key(KeyCode::KeyD)],
    );
    km.bind(
        VirtualAction::p1_start,
        &[InputBinding::Key(KeyCode::Enter)],
    );
    set_keymap(km);

    assert!(any_player_has_three_key_menu_buttons());
    assert!(!any_player_has_four_way_menu_buttons());
    assert!(any_player_has_dedicated_menu_buttons_for_mode(true));
    assert!(!any_player_has_dedicated_menu_buttons_for_mode(false));
}

#[test]
fn keycode_has_action_matches_without_allocating_action_vec() {
    let _guard = lock_test_guard();
    let _reset = TestReset::capture();
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_back,
        &[InputBinding::Key(KeyCode::Escape)],
    );
    set_keymap(km);

    with_keymap(|km| {
        assert!(km.keycode_mapped(KeyCode::Escape));
        assert!(km.keycode_has_action(KeyCode::Escape, |action| action == VirtualAction::p1_back));
        assert!(!km.keycode_has_action(KeyCode::Escape, |action| action == VirtualAction::p2_back));
    });
}

#[test]
fn pad_event_mapped_checks_device_and_uuid_without_allocating_action_vec() {
    let _guard = lock_test_guard();
    let _reset = TestReset::capture();
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_start,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: Some(3),
            uuid: Some([9; 16]),
        })],
    );
    set_keymap(km);

    let mapped = PadEvent::RawButton {
        id: PadId(3),
        timestamp: Instant::now(),
        host_nanos: 0,
        code: PadCode(77),
        uuid: [9; 16],
        value: 1.0,
        pressed: true,
    };
    let wrong_dev = PadEvent::RawButton {
        id: PadId(4),
        timestamp: Instant::now(),
        host_nanos: 0,
        code: PadCode(77),
        uuid: [9; 16],
        value: 1.0,
        pressed: true,
    };

    with_keymap(|km| {
        assert!(km.pad_event_mapped(&mapped));
        assert!(!km.pad_event_mapped(&wrong_dev));
    });
}

#[test]
fn keyboard_skips_unmapped_keys_before_debounce() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_back,
        &[InputBinding::Key(KeyCode::Escape)],
    );
    let mut input = InputState::new(&km, 0.02);

    let raw = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp: Instant::now(),
        host_nanos: 123,
    };
    let mut actual = Vec::new();
    input
        .map_key(input.key_event(raw), || raw.timestamp)
        .for_each(|event| actual.push(event));

    assert!(actual.is_empty());
}

#[test]
fn pad_skips_unmapped_pad_buttons_before_debounce() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::PadDir(PadDir::Left)],
    );
    let mut input = InputState::new(&km, 0.02);

    let timestamp = Instant::now();
    let pad = PadEvent::RawButton {
        id: PadId(1),
        timestamp: Instant::now(),
        host_nanos: 456,
        code: PadCode(77),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };
    let mut actual = Vec::new();
    input
        .map_pad(&pad, || timestamp)
        .for_each(|event| actual.push(event));

    assert!(actual.is_empty());
}

#[test]
fn pad_finds_each_sorted_raw_button_code() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 99,
            device: None,
            uuid: None,
        })],
    );
    km.bind(
        VirtualAction::p1_down,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 1,
            device: None,
            uuid: None,
        })],
    );
    let mut input = InputState::new(&km, 0.02);

    let timestamp = Instant::now();
    for (code, expected) in [
        (PadCode(1), VirtualAction::p1_down),
        (PadCode(99), VirtualAction::p1_left),
    ] {
        let event = PadEvent::RawButton {
            id: PadId(0),
            timestamp,
            host_nanos: 42,
            code,
            uuid: [0; 16],
            value: 1.0,
            pressed: true,
        };
        let mut actual = None;
        input
            .map_pad(&event, || timestamp)
            .for_each(|input| actual = Some(input.action));
        assert_eq!(actual, Some(expected));
    }
}

#[test]
fn pad_mapping_preserves_combined_filter_event_order() {
    let mut km = Keymap::default();
    for (action, device, uuid) in [
        (VirtualAction::p1_up, None, None),
        (VirtualAction::p1_down, Some(2), None),
        (VirtualAction::p1_left, None, Some([7; 16])),
        (VirtualAction::p1_right, Some(2), Some([7; 16])),
    ] {
        km.bind(
            action,
            &[InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 77,
                device,
                uuid,
            })],
        );
    }
    let mut input = InputState::new(&km, 0.02);
    let timestamp = Instant::now();
    let event = PadEvent::RawButton {
        id: PadId(2),
        timestamp,
        host_nanos: 73,
        code: PadCode(77),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };
    let mut actual = Vec::new();
    input
        .map_pad(&event, || timestamp)
        .for_each(|input| actual.push(input));

    assert_eq!(
        actual.iter().map(|input| input.action).collect::<Vec<_>>(),
        [
            VirtualAction::p1_up,
            VirtualAction::p1_down,
            VirtualAction::p1_left,
            VirtualAction::p1_right,
        ]
    );
    assert!(actual.iter().all(|input| {
        input.pressed
            && input.source == InputSource::Gamepad
            && input.timestamp == timestamp
            && input.timestamp_host_nanos == 73
            && input.input_slot == actual[0].input_slot
    }));
}

#[test]
fn pad_ignores_raw_axis_events() {
    let timestamp = Instant::now();
    let mut input = InputState::new(&Keymap::default(), 0.02);
    let event = PadEvent::RawAxis {
        id: PadId(2),
        timestamp: Instant::now(),
        host_nanos: 17,
        code: PadCode(9),
        uuid: [0x5a; 16],
        value: 0.25,
    };
    let mut actual = Vec::new();
    input
        .map_pad(&event, || timestamp)
        .for_each(|input| actual.push(input));
    assert!(actual.is_empty());
}

#[test]
fn pad_ignores_duplicate_raw_button_state() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: Some(1),
            uuid: Some([7; 16]),
        })],
    );
    let mut input = InputState::new(&km, 0.02);

    let t0 = Instant::now();
    let press = PadEvent::RawButton {
        id: PadId(1),
        timestamp: t0,
        host_nanos: 456,
        code: PadCode(77),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };
    let repeat_press = PadEvent::RawButton {
        id: PadId(1),
        timestamp: t0 + Duration::from_millis(1),
        host_nanos: 457,
        code: PadCode(77),
        uuid: [7; 16],
        value: 1.0,
        pressed: true,
    };

    let mut actual = Vec::new();
    input
        .map_pad(&press, || t0)
        .for_each(|event| actual.push(event));
    assert_eq!(actual.len(), 1, "initial press should emit once");
    assert_eq!(actual[0].action, VirtualAction::p1_left);
    assert!(actual[0].pressed);

    actual.clear();
    input
        .map_pad(&repeat_press, || t0 + Duration::from_millis(1))
        .for_each(|event| actual.push(event));
    assert!(
        actual.is_empty(),
        "duplicate raw button state should be suppressed by shared debounce"
    );
}

#[test]
fn keyboard_debounces_shared_arrow_input() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    let mut input = InputState::new(&km, 0.02);
    let t0 = Instant::now();
    let press = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp: t0,
        host_nanos: 100,
    };
    let release = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: false,
        repeat: false,
        timestamp: t0 + Duration::from_millis(1),
        host_nanos: 101,
    };
    let repress = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp: t0 + Duration::from_millis(5),
        host_nanos: 105,
    };

    let mut actual = Vec::new();
    input
        .map_key(input.key_event(press), || press.timestamp)
        .for_each(|event| actual.push(event));
    assert_eq!(actual.len(), 1, "press event count");
    assert_eq!(actual[0].action, VirtualAction::p1_left);
    assert!(actual[0].pressed);
    assert_eq!(actual[0].source, InputSource::Keyboard);
    assert_eq!(actual[0].timestamp, t0);
    assert_eq!(actual[0].timestamp_host_nanos, 100);

    actual.clear();
    input
        .map_key(input.key_event(release), || release.timestamp)
        .for_each(|event| actual.push(event));
    assert!(
        actual.is_empty(),
        "release inside debounce window should be delayed"
    );

    input
        .map_key(input.key_event(repress), || repress.timestamp)
        .for_each(|event| actual.push(event));
    assert!(
        actual.is_empty(),
        "quick release/repress chatter should not escape the shared debounce path"
    );
}

#[test]
fn combined_debounce_drain_flushes_due_keyboard_and_pad_releases() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[
            InputBinding::Key(KeyCode::ArrowLeft),
            InputBinding::PadDir(PadDir::Left),
        ],
    );
    let mut input = InputState::new(&km, 0.001);

    let timestamp = Instant::now();
    let keyboard = |pressed| RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed,
        repeat: false,
        timestamp,
        host_nanos: 100,
    };
    let pad = |pressed| PadEvent::Dir {
        id: PadId(0),
        timestamp,
        host_nanos: 200,
        dir: PadDir::Left,
        pressed,
    };
    let mut immediate = Vec::new();
    input
        .map_key(input.key_event(keyboard(true)), || timestamp)
        .for_each(|event| immediate.push(event));
    input
        .map_pad(&pad(true), || timestamp)
        .for_each(|event| immediate.push(event));
    assert_eq!(immediate.len(), 2, "both presses should emit immediately");

    immediate.clear();
    input
        .map_key(input.key_event(keyboard(false)), || timestamp)
        .for_each(|event| immediate.push(event));
    input
        .map_pad(&pad(false), || timestamp)
        .for_each(|event| immediate.push(event));
    assert!(immediate.is_empty(), "both releases should be delayed");

    let now = timestamp + Duration::from_millis(3);
    let mut delayed = Vec::new();
    while let Some(events) = input.next_due(now) {
        delayed.extend(events);
    }
    assert_eq!(
        delayed
            .iter()
            .map(|event| (event.source, event.action, event.pressed))
            .collect::<Vec<_>>(),
        [
            (InputSource::Keyboard, VirtualAction::p1_left, false),
            (InputSource::Keyboard, VirtualAction::p1_menu_left, false),
            (InputSource::Gamepad, VirtualAction::p1_left, false),
            (InputSource::Gamepad, VirtualAction::p1_menu_left, false),
        ]
    );
    assert!(input.next_due(now).is_none());
}

#[test]
fn raw_system_controls_bypass_repeat_and_debounce() {
    let mut km = Keymap::default();
    for action in [
        VirtualAction::p1_left,
        VirtualAction::system_fast_forward,
        VirtualAction::system_slow_down,
    ] {
        km.bind(action, &[InputBinding::Key(KeyCode::Tab)]);
    }
    km.bind(
        VirtualAction::system_fast_forward,
        &[
            InputBinding::Key(KeyCode::Tab),
            InputBinding::Key(KeyCode::Backquote),
        ],
    );
    let mut input = InputState::new(&km, 0.02);
    let timestamp = Instant::now();
    let raw = RawKeyboardEvent {
        code: KeyCode::Tab,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 12,
    };
    let key = input.key_event(raw);
    let system_mask =
        VirtualAction::system_fast_forward.bit() | VirtualAction::system_slow_down.bit();
    assert_eq!(key.system_mask, system_mask);
    assert_eq!(
        input
            .map_key(key, || timestamp)
            .map(|ev| ev.action)
            .collect::<Vec<_>>(),
        [VirtualAction::p1_left]
    );
    let repeated = input.key_event(RawKeyboardEvent {
        repeat: true,
        ..raw
    });
    assert_eq!(repeated.system_mask, system_mask);
    assert_eq!(input.map_key(repeated, || timestamp).count(), 0);
    let released = input.key_event(RawKeyboardEvent {
        pressed: false,
        ..raw
    });
    assert_eq!(released.system_mask, system_mask);
    assert_eq!(input.map_key(released, || timestamp).count(), 0);
    let due: Vec<_> = input
        .next_due(timestamp + Duration::from_millis(20))
        .expect("delayed release")
        .collect();
    assert_eq!(
        due.iter().map(|ev| ev.action).collect::<Vec<_>>(),
        [VirtualAction::p1_left, VirtualAction::p1_menu_left]
    );
    assert!(due.iter().all(|ev| !ev.pressed));

    let system_only = input.key_event(RawKeyboardEvent {
        code: KeyCode::Backquote,
        ..raw
    });
    assert_eq!(
        system_only.system_mask,
        VirtualAction::system_fast_forward.bit()
    );
    assert_eq!(input.map_key(system_only, || timestamp).count(), 0);
}

#[test]
fn delayed_edges_preserve_raw_and_receipt_timestamps_at_boundary() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    let mut input = InputState::new(&km, 0.02);
    let raw_time = Instant::now();
    let receipt = raw_time + Duration::from_micros(100_000);
    let press = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp: raw_time,
        host_nanos: 123_456,
    };
    let emitted: Vec<_> = input.map_key(input.key_event(press), || receipt).collect();
    assert_eq!(emitted[0].timestamp, raw_time);
    assert_eq!(emitted[0].stored_at, receipt);
    assert_eq!(emitted[0].emitted_at, receipt);
    assert_eq!(emitted[0].timestamp_host_nanos, 123_456);
    let release = RawKeyboardEvent {
        pressed: false,
        timestamp: raw_time + Duration::from_micros(1_000),
        host_nanos: 234_567,
        ..press
    };
    let stored = receipt + Duration::from_micros(2_000);
    assert_eq!(
        input.map_key(input.key_event(release), || stored).count(),
        0
    );
    let due = receipt + Duration::from_micros(20_000);
    assert!(input.next_due(due - Duration::from_nanos(1)).is_none());
    let events: Vec<_> = input
        .next_due(due)
        .expect("release at exact window")
        .collect();
    assert_eq!(events.len(), 2);
    for ev in events {
        assert!(!ev.pressed);
        assert_eq!(ev.timestamp, release.timestamp);
        assert_eq!(ev.timestamp_host_nanos, 234_567);
        assert_eq!(ev.stored_at, stored);
        assert_eq!(ev.emitted_at, due);
    }
}

#[test]
fn consumed_keys_and_independent_streams_do_not_change_debounce_state() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_start,
        &[InputBinding::Key(KeyCode::Enter)],
    );
    let mut first = InputState::new(&km, 0.02);
    let mut second = InputState::new(&km, 0.02);
    let timestamp = Instant::now();
    let raw = RawKeyboardEvent {
        code: KeyCode::Enter,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 42,
    };
    // A consumed shortcut resolves bindings but never commits a logical edge.
    let _consumed = first.key_event(raw);
    assert!(
        first
            .next_due(timestamp + Duration::from_millis(20))
            .is_none()
    );
    assert_eq!(first.map_key(first.key_event(raw), || timestamp).count(), 1);
    assert_eq!(
        second.map_key(second.key_event(raw), || timestamp).count(),
        1
    );
    first.clear();
    assert_eq!(first.map_key(first.key_event(raw), || timestamp).count(), 1);
    assert_eq!(
        second.map_key(second.key_event(raw), || timestamp).count(),
        0
    );
}

#[test]
fn rebind_replaces_system_actions_and_discards_pending_edges() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_start,
        &[
            InputBinding::Key(KeyCode::Enter),
            InputBinding::PadDir(PadDir::Up),
        ],
    );
    km.bind(
        VirtualAction::system_fast_forward,
        &[InputBinding::Key(KeyCode::Enter)],
    );
    let mut input = InputState::new(&km, 0.02);
    let timestamp = Instant::now();
    let raw = RawKeyboardEvent {
        code: KeyCode::Enter,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 42,
    };
    assert_eq!(input.map_key(input.key_event(raw), || timestamp).count(), 1);
    assert_eq!(
        input
            .map_key(
                input.key_event(RawKeyboardEvent {
                    pressed: false,
                    ..raw
                }),
                || timestamp
            )
            .count(),
        0
    );
    for pressed in [true, false] {
        input
            .map_pad(
                &PadEvent::Dir {
                    id: PadId(0),
                    dir: PadDir::Up,
                    pressed,
                    timestamp,
                    host_nanos: 100,
                },
                || timestamp,
            )
            .for_each(drop);
    }
    let mut replacement = Keymap::default();
    replacement.bind(
        VirtualAction::p2_start,
        &[InputBinding::Key(KeyCode::Enter)],
    );
    replacement.bind(
        VirtualAction::system_slow_down,
        &[InputBinding::Key(KeyCode::Enter)],
    );
    input.set_keymap(&replacement);
    assert!(
        input
            .next_due(timestamp + Duration::from_millis(20))
            .is_none()
    );
    let key = input.key_event(raw);
    assert_eq!(key.system_mask, VirtualAction::system_slow_down.bit());
    assert_eq!(
        input
            .map_key(key, || timestamp)
            .map(|ev| ev.action)
            .collect::<Vec<_>>(),
        [VirtualAction::p2_start]
    );
    input.set_debounce_seconds(0.0);
    assert_eq!(
        input
            .map_key(
                input.key_event(RawKeyboardEvent {
                    pressed: false,
                    ..raw
                }),
                || timestamp
            )
            .count(),
        1
    );
}

#[test]
fn unmapped_and_settled_events_skip_the_receipt_clock() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_start,
        &[
            InputBinding::Key(KeyCode::Enter),
            InputBinding::PadDir(PadDir::Up),
        ],
    );
    let mut input = InputState::new(&km, 0.02);
    let timestamp = Instant::now();
    let raw = RawKeyboardEvent {
        code: KeyCode::Enter,
        pressed: true,
        repeat: false,
        timestamp,
        host_nanos: 42,
    };
    assert_eq!(input.map_key(input.key_event(raw), || timestamp).count(), 1);
    for key in [
        raw,
        RawKeyboardEvent {
            repeat: true,
            ..raw
        },
        RawKeyboardEvent {
            code: KeyCode::KeyZ,
            ..raw
        },
    ] {
        assert_eq!(
            input
                .map_key(input.key_event(key), || panic!(
                    "no edge needs a receipt timestamp"
                ))
                .count(),
            0
        );
    }
    let pad = PadEvent::Dir {
        id: PadId(0),
        dir: PadDir::Up,
        pressed: true,
        timestamp,
        host_nanos: 100,
    };
    assert_eq!(input.map_pad(&pad, || timestamp).count(), 1);
    assert_eq!(
        input.map_pad(&pad, || panic!("settled duplicate")).count(),
        0
    );
    assert!(!input.has_pending(), "held inputs have no scheduled work");
}
