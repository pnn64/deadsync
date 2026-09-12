use super::*;
use crate::{GamepadCodeBinding, keymap::InputState};
use deadlib_platform::input::{PadCode, PadDir, PadEvent, PadId, RawKeyboardEvent};
use std::hint::black_box;
use std::time::Instant;

#[allow(dead_code)]
mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/keymap_preparation/baseline.rs"
    ));
}

fn mixed_keymap(width: usize) -> Keymap {
    let mut keymap = Keymap::default();
    for (index, action) in ALL_VIRTUAL_ACTIONS.into_iter().enumerate() {
        let bindings: Vec<_> = (0..width)
            .map(|slot| match (index + slot) % 4 {
                0 => InputBinding::Key([KeyCode::KeyA, KeyCode::KeyB, KeyCode::F35][slot % 3]),
                1 => InputBinding::PadDir(
                    [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right][slot % 4],
                ),
                2 => InputBinding::PadDirOn {
                    device: slot,
                    dir: PadDir::Left,
                },
                _ => InputBinding::GamepadCode(GamepadCodeBinding {
                    code_u32: (index * 16 + slot) as u32,
                    device: (slot % 2 == 0).then_some(slot),
                    uuid: (slot % 3 == 0).then_some([index as u8; 16]),
                }),
            })
            .collect();
        keymap.bind(action, &bindings);
    }
    keymap
}

fn assert_same(left: &Keymap, right: &Keymap) {
    assert_eq!(
        baseline::keymap_ini_lines(left),
        baseline::keymap_ini_lines(right)
    );
    assert!(left.has_same_bindings(right));
    let timestamp = Instant::now();
    let mut left_input = InputState::new(left, 0.0);
    let mut right_input = InputState::new(right, 0.0);
    for key in [
        KeyCode::ArrowUp,
        KeyCode::ArrowLeft,
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::F35,
        KeyCode::Enter,
        KeyCode::Escape,
        KeyCode::ScrollLock,
    ] {
        for action in ALL_VIRTUAL_ACTIONS {
            assert_eq!(
                left.keycode_has_action(key, |a| a == action),
                right.keycode_has_action(key, |a| a == action)
            );
        }
        let raw = RawKeyboardEvent {
            code: key,
            pressed: true,
            repeat: false,
            timestamp,
            host_nanos: 123,
        };
        let events = |state: &mut InputState| {
            state
                .map_key(state.key_event(raw), || timestamp)
                .map(|event| (event.action, event.input_slot, event.pressed, event.source))
                .collect::<Vec<_>>()
        };
        assert_eq!(events(&mut left_input), events(&mut right_input));
    }
    for device in [0, 1, 3, 17] {
        for code in [0, 3, 19, 47, 511] {
            let event = PadEvent::RawButton {
                id: PadId(device),
                timestamp,
                host_nanos: 123,
                code: PadCode(code),
                uuid: [3; 16],
                value: 1.0,
                pressed: true,
            };
            assert_eq!(
                left.pad_event_mapped(&event),
                right.pad_event_mapped(&event)
            );
        }
    }
}

#[test]
fn streamed_ini_matches_frozen_serialization_for_all_binding_forms() {
    let mut extreme = mixed_keymap(16);
    extreme.bind(
        VirtualAction::p1_up,
        &[
            InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: u32::MAX,
                device: Some(usize::MAX),
                uuid: Some([255; 16]),
            }),
            InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0,
                device: None,
                uuid: None,
            }),
            InputBinding::PadDirOn {
                device: usize::MAX,
                dir: PadDir::Down,
            },
            InputBinding::Key(KeyCode::NumpadMemorySubtract),
            InputBinding::Key(KeyCode::NumpadMemorySubtract),
        ],
    );
    for keymap in [
        Keymap::default(),
        default_keymap(),
        mixed_keymap(1),
        mixed_keymap(4),
        extreme,
    ] {
        assert_eq!(
            keymap_ini_lines(&keymap),
            baseline::keymap_ini_lines(&keymap)
        );
        let mut old = String::from("existing content\n");
        baseline::write_keymap_ini_section(&mut old, &keymap);
        let mut new = String::from("existing content\n");
        new.reserve(old.len());
        crate::perf::assert_no_churn(|| write_keymap_ini_section(&mut new, &keymap));
        assert_eq!(new, old);
        for (_, value) in keymap_ini_lines(&keymap) {
            for token in value.split(',').filter(|s| !s.is_empty()) {
                assert!(parse_binding_token(token).is_some(), "{token}");
            }
        }
    }
}

#[test]
fn binding_comparison_preserves_missing_empty_order_duplicates_and_identity() {
    let missing = Keymap::default();
    let mut empty = Keymap::default();
    for action in ALL_VIRTUAL_ACTIONS {
        empty.bind(action, &[]);
    }
    assert!(missing.has_same_bindings(&empty));
    for action in ALL_VIRTUAL_ACTIONS {
        let base = mixed_keymap(3);
        for binding in [
            InputBinding::Key(KeyCode::KeyA),
            InputBinding::PadDir(PadDir::Left),
            InputBinding::PadDirOn {
                device: 0,
                dir: PadDir::Left,
            },
            InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0,
                device: Some(0),
                uuid: Some([0; 16]),
            }),
            InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0,
                device: None,
                uuid: None,
            }),
        ] {
            let mut changed = base.clone();
            changed.bind(action, &[binding, binding]);
            assert_eq!(
                base.has_same_bindings(&changed),
                baseline::has_same_bindings(&base, &changed)
            );
            let mut reordered = base.clone();
            let mut values = base.bindings_for_action(action).to_vec();
            values.reverse();
            reordered.bind(action, &values);
            assert_eq!(
                base.has_same_bindings(&reordered),
                baseline::has_same_bindings(&base, &reordered)
            );
        }
        crate::perf::assert_no_churn(|| {
            black_box(base.has_same_bindings(&base));
        });
    }
}

#[test]
fn restored_defaults_match_frozen_mapping_after_conflicts_and_duplicates() {
    for width in [0, 1, 3, 16] {
        let mut old = mixed_keymap(width);
        for (index, action) in ALL_VIRTUAL_ACTIONS.into_iter().enumerate() {
            let Some(binding) = default_binding_for_action(action) else {
                continue;
            };
            let target = if index % 2 == 0 {
                action
            } else {
                ALL_VIRTUAL_ACTIONS[(index + 1) % ALL_VIRTUAL_ACTIONS.len()]
            };
            let mut values = old.bindings_for_action(target).to_vec();
            values.extend([binding, binding]);
            old.bind(target, &values);
        }
        let mut new = old.clone();
        baseline::restore_available_default_bindings(&mut old);
        restore_available_default_bindings(&mut new);
        assert_same(&old, &new);
        crate::perf::assert_no_churn(|| restore_available_default_bindings(&mut new));
        assert_same(&old, &new);
    }
    let mut keymap = default_keymap();
    crate::perf::assert_no_churn(|| restore_available_default_bindings(&mut keymap));
}

#[test]
fn loading_and_editing_preserve_frozen_binding_and_input_behavior() {
    for entries in [
        vec![],
        vec![("P1_Up", ""), ("P2_Start", "KeyCode::Enter")],
        vec![
            ("P1_Up", "bad, KeyCode::KeyA"),
            ("Unknown", "PadDir::Up"),
            ("P1_Up", "PadCode[0xFF]@3"),
        ],
        DEFAULT_KEYMAP_INI_LINES.to_vec(),
    ] {
        let old = baseline::load_keymap_from_ini_entries(Some(entries.iter().copied()));
        let new = load_keymap_from_ini_entries(Some(entries.iter().copied()));
        assert_same(&old, &new);
    }
    for base in [Keymap::default(), default_keymap(), mixed_keymap(3)] {
        for action in [
            VirtualAction::p1_up,
            VirtualAction::p2_start,
            VirtualAction::system_fast_forward,
        ] {
            for index in [0, 1, 2, usize::MAX] {
                assert_same(
                    &baseline::updated_keymap_unique_keyboard(&base, action, index, KeyCode::Enter),
                    &updated_keymap_unique_keyboard(&base, action, index, KeyCode::Enter),
                );
                let binding = InputBinding::GamepadCode(GamepadCodeBinding {
                    code_u32: 47,
                    device: Some(3),
                    uuid: Some([3; 16]),
                });
                assert_same(
                    &baseline::updated_keymap_unique_gamepad(&base, action, index, binding),
                    &updated_keymap_unique_gamepad(&base, action, index, binding),
                );
                let old = baseline::cleared_keymap(&base, action, index);
                let new = cleared_keymap(&base, action, index);
                assert_eq!(new.1, old.1);
                assert_same(&old.0, &new.0);
            }
        }
    }
}

#[test]
#[ignore = "manual 0.5.1151/0.5.1152 keymap CPU and allocator comparison; run in release"]
fn keymap_preparation_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (label, map) in [
        ("empty", Keymap::default()),
        ("default", default_keymap()),
        ("mixed", mixed_keymap(4)),
        ("large", mixed_keymap(16)),
    ] {
        assert_eq!(keymap_ini_lines(&map), baseline::keymap_ini_lines(&map));
        let mut text = String::new();
        baseline::write_keymap_ini_section(&mut text, &map);
        let len = text.len();
        for old in order {
            let suffix = if old { "old" } else { "new" };
            let writer = black_box(if old {
                baseline::write_keymap_ini_section
            } else {
                write_keymap_ini_section
            });
            crate::perf::measure_sampled(&format!("write_warm_{label}_{suffix}"), 256, len, || {
                text.clear();
                writer(black_box(&mut text), black_box(&map));
                black_box(&text);
            });
            crate::perf::measure_sampled(&format!("write_cold_{label}_{suffix}"), 256, len, || {
                let mut out = String::new();
                writer(black_box(&mut out), black_box(&map));
                out
            });
            let lines = black_box(if old {
                baseline::keymap_ini_lines
            } else {
                keymap_ini_lines
            });
            crate::perf::measure_sampled(
                &format!("lines_{label}_{suffix}"),
                256,
                ALL_VIRTUAL_ACTIONS.len(),
                || lines(black_box(&map)),
            );
        }
        for (comparison, other) in [
            ("late_changed", {
                let mut other = map.clone();
                other.bind(
                    *ALL_VIRTUAL_ACTIONS.last().unwrap(),
                    &[InputBinding::Key(KeyCode::F35)],
                );
                other
            }),
            ("equal", map.clone()),
            ("changed", {
                let mut other = map.clone();
                other.bind(VirtualAction::p1_up, &[InputBinding::Key(KeyCode::F35)]);
                other
            }),
        ] {
            assert_eq!(
                map.has_same_bindings(&other),
                baseline::has_same_bindings(&map, &other)
            );
            for old in order {
                let suffix = if old { "old" } else { "new" };
                let same = black_box(if old {
                    baseline::has_same_bindings
                } else {
                    Keymap::has_same_bindings
                });
                crate::perf::measure_sampled(
                    &format!("compare_{comparison}_{label}_{suffix}"),
                    256,
                    1,
                    || same(black_box(&map), black_box(&other)),
                );
            }
        }
        for old in order {
            let suffix = if old { "old" } else { "new" };
            let restore = black_box(if old {
                baseline::restore_available_default_bindings
            } else {
                restore_available_default_bindings
            });
            let mut stable = map.clone();
            restore(&mut stable);
            crate::perf::measure_sampled(
                &format!("restore_stable_{label}_{suffix}"),
                256,
                1,
                || {
                    restore(black_box(&mut stable));
                    black_box(&stable);
                },
            );
            crate::perf::measure_sampled(
                &format!("restore_fresh_{label}_{suffix}"),
                128,
                1,
                || {
                    let mut fresh = black_box(&map).clone();
                    restore(&mut fresh);
                    fresh
                },
            );
        }
    }
    let map = mixed_keymap(4);
    let entries = baseline::keymap_ini_lines(&map);
    for old in order {
        let suffix = if old { "old" } else { "new" };
        let edit = black_box(if old {
            baseline::updated_keymap_unique_keyboard
        } else {
            updated_keymap_unique_keyboard
        });
        crate::perf::measure_sampled(&format!("edit_keyboard_{suffix}"), 128, 1, || {
            edit(black_box(&map), VirtualAction::p1_start, 1, KeyCode::Enter)
        });
        let load = black_box(if old {
            baseline::load_keymap_from_ini_entries
        } else {
            load_keymap_from_ini_entries
        });
        crate::perf::measure_sampled(&format!("load_mixed_{suffix}"), 128, 1, || {
            load(Some(
                black_box(&entries).iter().map(|(k, v)| (*k, v.as_str())),
            ))
        });
    }
}
