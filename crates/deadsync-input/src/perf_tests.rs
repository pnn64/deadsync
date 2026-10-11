use super::*;
use crate::ALL_VIRTUAL_ACTIONS;
use crate::bindings::{self, parse_binding_token};
use std::hint::black_box;

#[path = "perf_keymap_original.rs"]
#[allow(dead_code)]
mod original_keymap;
use original_keymap::OriginalKeymap;

#[allow(dead_code)]
mod original {
    use super::OriginalKeymap as Keymap;
    use super::*;
    include!("perf_bindings_original.rs");
}

// Same frozen algorithms with current storage isolate the other two changes.
#[allow(dead_code)]
mod original_bindings {
    use super::*;
    include!("perf_bindings_original.rs");
}

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod allocations;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn assert_same(current: &Keymap, old: &OriginalKeymap) {
    for action in ALL_VIRTUAL_ACTIONS {
        assert_eq!(
            current.bindings_for_action(action),
            old.bindings_for_action(action),
            "{action:?}"
        );
        assert_eq!(
            current.first_key_binding(action),
            old.first_key_binding(action)
        );
        for index in [0, 1, 2, 5, usize::MAX] {
            assert_eq!(
                current.binding_at(action, index),
                old.binding_at(action, index)
            );
        }
    }
    assert_eq!(current.key_rev, old.key_rev);
    assert_eq!(current.key_rev_extra, old.key_rev_extra);
    assert_eq!(current.pad_dir_rev, old.pad_dir_rev);
    assert_eq!(current.pad_dir_on_rev, old.pad_dir_on_rev);
    assert_eq!(current.pad_code_rev.len(), old.pad_code_rev.len());
    for (code, values) in &current.pad_code_rev {
        let expected = &old.pad_code_rev[code];
        let tuples = |entries: &[PadCodeRev]| {
            entries
                .iter()
                .map(|e| (e.act, e.device, e.uuid))
                .collect::<Vec<_>>()
        };
        assert_eq!(tuples(values), tuples(expected));
    }
}

fn binding_pool() -> Vec<InputBinding> {
    vec![
        InputBinding::Key(KeyCode::KeyA),
        InputBinding::Key(KeyCode::ArrowLeft),
        InputBinding::Key(KeyCode::Enter),
        InputBinding::Key(KeyCode::F35),
        InputBinding::PadDir(PadDir::Left),
        InputBinding::PadDirOn {
            device: 2,
            dir: PadDir::Up,
        },
        InputBinding::PadDirOn {
            device: 80,
            dir: PadDir::Right,
        },
        InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: None,
            uuid: None,
        }),
        InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: Some(2),
            uuid: None,
        }),
        InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 333,
            device: Some(2),
            uuid: Some([7; 16]),
        }),
        InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 333,
            device: None,
            uuid: Some([8; 16]),
        }),
    ]
}

fn populated() -> (Keymap, OriginalKeymap) {
    let mut current = bindings::default_keymap();
    let mut old = original::default_keymap();
    let pool = binding_pool();
    for (index, action) in ALL_VIRTUAL_ACTIONS.into_iter().enumerate() {
        let mut values = current.bindings_for_action(action).to_vec();
        values.extend([pool[index % pool.len()], pool[(index + 3) % pool.len()]]);
        current.bind(action, &values);
        old.bind(action, &values);
    }
    (current, old)
}

#[test]
fn dense_storage_preserves_rebinding_and_reverse_order() {
    let (mut current, mut old) = populated();
    assert_same(&current, &old);
    let pool = binding_pool();
    let mut seed = 31_u64;
    for iteration in 0..512 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let action = ALL_VIRTUAL_ACTIONS[(seed >> 32) as usize % VirtualAction::COUNT];
        let values: Vec<_> = (0..iteration % 8)
            .map(|i| pool[(i + iteration) % pool.len()])
            .collect();
        if iteration % 2 == 0 {
            current.bind(action, &values);
        } else {
            current.bind_owned(action, values.clone());
        }
        old.bind(action, &values);
        assert_same(&current, &old);
    }
    assert_same(&current.clone(), &old.clone());
}

#[test]
fn defaults_and_ini_loading_match_original_for_sparse_and_duplicate_entries() {
    assert_same(&bindings::default_keymap(), &original::default_keymap());
    for action in ALL_VIRTUAL_ACTIONS {
        assert_eq!(
            bindings::default_key_for_action(action),
            original::default_key_for_action(action)
        );
    }
    assert_same(
        &bindings::load_keymap_from_ini_entries(None::<[(&str, &str); 0]>),
        &original::load_keymap_from_ini_entries(None::<[(&str, &str); 0]>),
    );
    let cases = vec![
        vec![],
        bindings::DEFAULT_KEYMAP_INI_LINES.to_vec(),
        vec![
            ("p1_LEFT", "KeyCode::KeyA, PadDir::Left"),
            ("P1_Left", "KeyCode::Enter"),
            ("P1_Operator", ""),
            ("P1_Coin", "broken"),
            ("unknown", "KeyCode::Tab"),
        ],
        vec![
            ("P1_Start", "PadCode[77]@2#07070707070707070707070707070707"),
            ("P1_Left", "KeyCode::ArrowLeft,KeyCode::ArrowLeft"),
        ],
    ];
    for entries in cases {
        assert_same(
            &bindings::load_keymap_from_ini_entries(Some(entries.clone())),
            &original::load_keymap_from_ini_entries(Some(entries)),
        );
    }
    let values = [
        "",
        "invalid",
        "KeyCode::Enter",
        "KeyCode::KeyA, KeyCode::KeyE",
        "PadDir::Left",
        "Pad2::Dir::Up",
        "PadCode[77]@2",
        "KeyCode::ArrowLeft,KeyCode::ArrowLeft",
    ];
    let mut seed = 109_u64;
    for count in 0..100 {
        let entries: Vec<_> = (0..count)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let action = ALL_VIRTUAL_ACTIONS[(seed >> 32) as usize % VirtualAction::COUNT];
                (
                    crate::action_to_ini_key(action),
                    values[(seed >> 16) as usize % values.len()],
                )
            })
            .collect();
        assert_same(
            &bindings::load_keymap_from_ini_entries(Some(entries.clone())),
            &original::load_keymap_from_ini_entries(Some(entries)),
        );
    }
}

#[test]
fn owned_edits_preserve_uniqueness_defaults_and_clear_results() {
    let pool = binding_pool();
    for (current, old) in [
        (bindings::default_keymap(), original::default_keymap()),
        populated(),
    ] {
        for action in ALL_VIRTUAL_ACTIONS {
            for index in [0, 1, 2, usize::MAX] {
                for key in [
                    KeyCode::KeyA,
                    KeyCode::ArrowLeft,
                    KeyCode::Enter,
                    KeyCode::Slash,
                ] {
                    assert_same(
                        &bindings::updated_keymap_unique_keyboard(&current, action, index, key),
                        &original::updated_keymap_unique_keyboard(&old, action, index, key),
                    );
                }
                for &binding in &pool {
                    assert_same(
                        &bindings::updated_keymap_unique_gamepad(&current, action, index, binding),
                        &original::updated_keymap_unique_gamepad(&old, action, index, binding),
                    );
                }
                let (new, changed) = bindings::cleared_keymap(&current, action, index);
                let (expected, expected_changed) = original::cleared_keymap(&old, action, index);
                assert_eq!(changed, expected_changed);
                assert_same(&new, &expected);
            }
        }
    }
}

fn original_runtime_map(old: &OriginalKeymap) -> Keymap {
    Keymap {
        map: std::array::from_fn(|i| {
            old.bindings_for_action(VirtualAction::from_ix(i).unwrap())
                .into()
        }),
        key_rev: old.key_rev.clone(),
        key_rev_extra: old.key_rev_extra.clone(),
        pad_dir_rev: old.pad_dir_rev.clone(),
        pad_dir_on_rev: old.pad_dir_on_rev.clone(),
        pad_code_rev: old.pad_code_rev.clone(),
    }
}

fn events(
    iter: impl Iterator<Item = InputEvent>,
) -> Vec<(
    VirtualAction,
    bool,
    InputSource,
    Instant,
    u64,
    Instant,
    Instant,
)> {
    iter.map(|e| {
        (
            e.action,
            e.pressed,
            e.source,
            e.timestamp,
            e.timestamp_host_nanos,
            e.stored_at,
            e.emitted_at,
        )
    })
    .collect()
}

#[test]
fn edited_maps_preserve_compiled_keyboard_and_pad_event_behavior() {
    let (current, old) = populated();
    let current = bindings::updated_keymap_unique_keyboard(
        &current,
        VirtualAction::p1_left,
        2,
        KeyCode::KeyA,
    );
    let old =
        original::updated_keymap_unique_keyboard(&old, VirtualAction::p1_left, 2, KeyCode::KeyA);
    let reference = original_runtime_map(&old);
    for debounce in [0.0, 0.02] {
        let mut actual = InputState::new(&current, debounce);
        let mut expected = InputState::new(&reference, debounce);
        let start = Instant::now();
        for step in 0..12 {
            let now = start + Duration::from_millis(step * 10);
            for code in [
                KeyCode::KeyA,
                KeyCode::ArrowLeft,
                KeyCode::Enter,
                KeyCode::Tab,
                KeyCode::F35,
            ] {
                let raw = RawKeyboardEvent {
                    code,
                    pressed: step % 3 != 2,
                    repeat: step % 3 == 1,
                    timestamp: now,
                    host_nanos: step + 100,
                };
                let a = actual.key_event(raw);
                let b = expected.key_event(raw);
                assert_eq!(a.system_mask, b.system_mask);
                assert_eq!(
                    events(actual.map_key(a, || now)),
                    events(expected.map_key(b, || now))
                );
            }
            for id in [0, 2, 80] {
                let dir = PadEvent::Dir {
                    id: PadId(id),
                    dir: PadDir::Left,
                    pressed: step % 2 == 0,
                    timestamp: now,
                    host_nanos: step,
                };
                assert_eq!(current.pad_event_mapped(&dir), old.pad_event_mapped(&dir));
                assert_eq!(
                    events(actual.map_pad(&dir, || now)),
                    events(expected.map_pad(&dir, || now))
                );
                for (code, uuid) in [(77, [0; 16]), (333, [7; 16]), (333, [8; 16]), (99, [0; 16])] {
                    let pad = PadEvent::RawButton {
                        id: PadId(id),
                        code: PadCode(code),
                        uuid,
                        value: 1.0,
                        pressed: step % 2 == 0,
                        timestamp: now,
                        host_nanos: step,
                    };
                    assert_eq!(current.pad_event_mapped(&pad), old.pad_event_mapped(&pad));
                    assert_eq!(
                        events(actual.map_pad(&pad, || now)),
                        events(expected.map_pad(&pad, || now))
                    );
                }
            }
            // Each map may assign different internal pad slots; compare the
            // observable events, ordering tied releases by action if necessary.
            let drain = |input: &mut InputState| {
                let mut out = Vec::new();
                while let Some(edge) = input.next_due(now) {
                    out.extend(events(edge));
                }
                out.sort_by_key(|e| (e.0.ix(), e.1));
                out
            };
            assert_eq!(drain(&mut actual), drain(&mut expected));
        }
    }
}

#[test]
fn owned_bind_reuses_storage_without_allocator_churn() {
    let mut current = Keymap::default();
    let values = vec![
        InputBinding::Key(KeyCode::KeyA),
        InputBinding::PadDir(PadDir::Left),
    ];
    current.bind(VirtualAction::p1_left, &values);
    current.bind(VirtualAction::p1_left, &[]); // retain reverse-list capacities
    let pointer = values.as_ptr();
    allocations::assert_no_churn(|| current.bind_owned(VirtualAction::p1_left, values));
    assert_eq!(
        current.bindings_for_action(VirtualAction::p1_left).as_ptr(),
        pointer
    );
}

#[test]
#[ignore = "paired release benchmark; run --ignored --nocapture --test-threads=1"]
fn benchmark_dense_action_storage() {
    let (current, old) = populated();
    paired::compare("lookup 32 actions", 10_000, |new| {
        for action in ALL_VIRTUAL_ACTIONS {
            if new {
                black_box(current.binding_at(black_box(action), 1));
            } else {
                black_box(old.binding_at(black_box(action), 1));
            }
        }
    });
    let (before, old_churn) = allocations::measure(original::default_keymap);
    let (after, new_churn) = allocations::measure(original_bindings::default_keymap);
    println!("default map/storage only: original {old_churn:?}, current {new_churn:?}");
    let old_bytes =
        old_churn.allocated_bytes - old_churn.freed_bytes + std::mem::size_of::<OriginalKeymap>();
    let new_bytes =
        new_churn.allocated_bytes - new_churn.freed_bytes + std::mem::size_of::<Keymap>();
    println!("default map live requested heap plus inline bytes: {old_bytes} -> {new_bytes}");
    assert!(new_bytes < old_bytes);
    assert_same(&after, &before);
    paired::compare("construct default map/storage only", 2000, |new| {
        if new {
            black_box(original_bindings::default_keymap());
        } else {
            black_box(original::default_keymap());
        }
    });
    paired::compare("construct default map/all changes", 2000, |new| {
        if new {
            black_box(bindings::default_keymap());
        } else {
            black_box(original::default_keymap());
        }
    });
    println!(
        "inline Keymap bytes: {} -> {}; empty maps pay the fixed table cost",
        std::mem::size_of::<OriginalKeymap>(),
        std::mem::size_of::<Keymap>()
    );
}

#[test]
#[ignore = "paired release benchmark; run --ignored --nocapture --test-threads=1"]
fn benchmark_static_defaults_loading() {
    let custom = [("P1_Left", "KeyCode::KeyA,PadCode[77]@2")];
    for (label, input) in [
        ("sparse INI/defaults only", custom.as_slice()),
        (
            "complete INI/defaults only",
            bindings::DEFAULT_KEYMAP_INI_LINES.as_slice(),
        ),
    ] {
        let old = allocations::measure(|| {
            original_bindings::load_keymap_from_ini_entries(Some(input.iter().copied()))
        })
        .1;
        let new = allocations::measure(|| {
            bindings::load_keymap_from_ini_entries(Some(input.iter().copied()))
        })
        .1;
        println!("{label}: original {old:?}, current {new:?}");
        assert!(new.allocs < old.allocs);
        paired::compare(label, 1000, |current| {
            if current {
                black_box(bindings::load_keymap_from_ini_entries(Some(
                    black_box(input).iter().copied(),
                )));
            } else {
                black_box(original_bindings::load_keymap_from_ini_entries(Some(
                    black_box(input).iter().copied(),
                )));
            }
        });
    }
    paired::compare("complete INI/all changes", 1000, |current| {
        if current {
            black_box(bindings::load_keymap_from_ini_entries(Some(
                bindings::DEFAULT_KEYMAP_INI_LINES,
            )));
        } else {
            black_box(original::load_keymap_from_ini_entries(Some(
                bindings::DEFAULT_KEYMAP_INI_LINES,
            )));
        }
    });
}

#[test]
#[ignore = "paired release benchmark; run --ignored --nocapture --test-threads=1"]
fn benchmark_owned_binding_edits() {
    let (current, old) = populated();
    let action = VirtualAction::p1_start;
    for (label, edit) in [
        ("keyboard edit/ownership only", 0),
        ("gamepad edit/ownership only", 1),
        ("clear binding/ownership only", 2),
    ] {
        let work = |new: bool| {
            let out = match (edit, new) {
                (0, true) => {
                    bindings::updated_keymap_unique_keyboard(&current, action, 2, KeyCode::KeyA)
                }
                (0, false) => original_bindings::updated_keymap_unique_keyboard(
                    &current,
                    action,
                    2,
                    KeyCode::KeyA,
                ),
                (1, true) => bindings::updated_keymap_unique_gamepad(
                    &current,
                    action,
                    2,
                    InputBinding::PadDir(PadDir::Left),
                ),
                (1, false) => original_bindings::updated_keymap_unique_gamepad(
                    &current,
                    action,
                    2,
                    InputBinding::PadDir(PadDir::Left),
                ),
                (2, true) => bindings::cleared_keymap(&current, action, 2).0,
                (2, false) => original_bindings::cleared_keymap(&current, action, 2).0,
                _ => unreachable!(),
            };
            black_box(out);
        };
        let before = allocations::measure(|| work(false)).1;
        let after = allocations::measure(|| work(true)).1;
        println!("{label}: original {before:?}, current {after:?}");
        assert!(after.allocs + after.reallocs < before.allocs + before.reallocs);
        paired::compare(label, 1000, work);
    }
    paired::compare("keyboard edit/all changes", 1000, |new| {
        if new {
            black_box(bindings::updated_keymap_unique_keyboard(
                &current,
                action,
                2,
                KeyCode::KeyA,
            ));
        } else {
            black_box(original::updated_keymap_unique_keyboard(
                &old,
                action,
                2,
                KeyCode::KeyA,
            ));
        }
    });
}
