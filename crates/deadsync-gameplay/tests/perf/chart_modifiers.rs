use super::*;
use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;

#[allow(clippy::too_many_arguments)]
mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/chart_modifiers/baseline.rs"
    ));
}

fn fixture(count: usize, stride: usize, cols: usize, offset: usize) -> Vec<Note> {
    (0..count)
        .map(|i| {
            let row = i * stride;
            let mut note = test_note_at(NoteType::Tap, None, false, row, row as f32 / 48.0);
            note.column = offset + i % cols;
            note
        })
        .collect()
}

fn mixed_fixture(count: usize, stride: usize, seed: usize) -> Vec<Note> {
    let mut notes = fixture(count, stride, 8, 0);
    for (i, note) in notes.iter_mut().enumerate() {
        note.note_type = [
            NoteType::Tap,
            NoteType::Lift,
            NoteType::Hold,
            NoteType::Roll,
            NoteType::Mine,
            NoteType::Fake,
        ][(i + seed) % 6];
        if matches!(note.note_type, NoteType::Hold | NoteType::Roll)
            && !(i + seed).is_multiple_of(7)
        {
            let mut hold = test_hold();
            hold.end_row_index = note.row_index + (i * 11 + seed) % 192;
            hold.end_beat = hold.end_row_index as f32 / 48.0;
            hold.life = 0.75;
            note.hold = Some(hold);
        }
        note.is_fake = (i + seed).is_multiple_of(11);
        note.can_be_judged = !(i + seed).is_multiple_of(13);
        note.quantization_idx = (i % 9) as u8;
        note.result = Some(test_judgment(JudgeGrade::Excellent));
        note.early_result = Some(test_judgment(JudgeGrade::Great));
    }
    notes
}

fn assert_same(actual: &[Note], expected: &[Note]) {
    // Compare all fields, including hold/judgment state and untouched notes.
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}

#[test]
fn intelligent_batch_preserves_known_insertions_and_owned_storage() {
    let timing = test_timing(4096);
    let mut notes = fixture(3, 48, 4, 0);
    notes.reserve(3);
    let pointer = notes.as_ptr();
    let capacity = notes.capacity();
    assert_no_churn(|| apply_insert_intelligent_taps(&mut notes, &timing, 0, 4, 48, 24, 48, false));
    assert_eq!(
        notes
            .iter()
            .map(|n| (n.row_index, n.column))
            .collect::<Vec<_>>(),
        [(0, 0), (24, 2), (48, 1), (72, 0), (96, 2)]
    );
    assert_eq!(notes.as_ptr(), pointer);
    assert_eq!(notes.capacity(), capacity);
    let mut dense = fixture(128, 12, 4, 0);
    let before = dense.clone();
    assert_no_churn(|| apply_insert_intelligent_taps(&mut dense, &timing, 0, 4, 48, 24, 48, false));
    assert_same(&dense, &before);
}

#[test]
fn intelligent_batch_matches_parent_for_windows_holds_and_compatibility() {
    for timing in [test_timing(48), test_timing(8192)] {
        for shape in 0..8 {
            let mut source = if shape < 3 {
                fixture(64, [24, 48, 96][shape], 4, 0)
            } else {
                mixed_fixture(64, 24, shape)
            };
            if shape == 6 {
                source.extend(source.clone());
                sort_player_notes(&mut source);
            }
            if shape == 7 {
                source.reverse();
            }
            for (window, insert, stride) in [
                (48, 24, 48),
                (24, 12, 48),
                (48, 36, 48),
                (96, 24, 48),
                (48, 48, 48),
                (48, 1, 48),
                (4, 2, 5),
            ] {
                for skippy in [false, true] {
                    for (offset, cols) in
                        [(0, 1), (0, 4), (4, 4), (0, 10), (0, 0), (0, MAX_COLS + 1)]
                    {
                        let mut actual = source.clone();
                        let mut expected = source.clone();
                        baseline::apply_insert_intelligent_taps(
                            &mut expected,
                            &timing,
                            offset,
                            cols,
                            window,
                            insert,
                            stride,
                            skippy,
                        );
                        apply_insert_intelligent_taps(
                            &mut actual,
                            &timing,
                            offset,
                            cols,
                            window,
                            insert,
                            stride,
                            skippy,
                        );
                        assert_same(&actual, &expected);
                    }
                }
            }
        }
    }
}

fn mine_fixture(count: usize, backwards: bool, column: usize) -> Vec<Note> {
    let mut notes = fixture(count, 192, 1, column);
    for (i, note) in notes.iter_mut().enumerate() {
        note.note_type = if i % 2 == 0 {
            NoteType::Hold
        } else {
            NoteType::Roll
        };
        let mut hold = test_hold();
        hold.end_row_index = if backwards {
            (count - i - 1) * 192 + 48
        } else {
            note.row_index + 48
        };
        hold.end_beat = hold.end_row_index as f32 / 48.0;
        note.hold = Some(hold);
    }
    notes
}

#[test]
fn mine_summary_preserves_tail_placement_and_reuses_storage() {
    let timing = test_timing(1024);
    let mut notes = mine_fixture(3, false, 0);
    notes.reserve(3);
    let pointer = notes.as_ptr();
    assert_no_churn(|| apply_mines_insert(&mut notes, &[], &timing, 0, 4, 0, 1024));
    assert_eq!(
        notes[3..]
            .iter()
            .map(|n| (n.row_index, n.column, n.note_type))
            .collect::<Vec<_>>(),
        [
            (72, 0, NoteType::Mine),
            (264, 0, NoteType::Mine),
            (456, 0, NoteType::Mine)
        ]
    );
    assert_eq!(notes.as_ptr(), pointer);
}

#[test]
fn mine_summary_matches_parent_for_backward_tails_collisions_and_foreign_lanes() {
    let timing = test_timing(8192);
    for seed in 0..32 {
        for column in [0, 4, MAX_COLS - 1, MAX_COLS, usize::MAX] {
            let mut source = mine_fixture(32, seed % 2 == 0, column);
            let mut context = Vec::new();
            for (i, note) in source.iter_mut().enumerate() {
                let hold = note.hold.as_mut().unwrap();
                if seed % 3 == 0 {
                    hold.end_row_index = 48 + i % 3 * 12;
                }
                if seed % 5 == 0 && i % 3 == 0 {
                    hold.end_row_index = usize::MAX;
                }
                if seed % 7 == 0 && i % 4 == 0 {
                    note.hold = None;
                }
                note.is_fake = i % 5 == 0;
                if i % 8 == 0 {
                    let mut blocker = test_note_at(NoteType::Tap, None, false, i * 192 + 72, 0.0);
                    blocker.column = column;
                    context.push(blocker);
                }
            }
            source.extend(mixed_fixture(40, 96, seed));
            sort_player_notes(&mut source);
            for bounds in [(0, 8192), (200, 2048), (2048, 200)] {
                let mut actual = source.clone();
                let mut expected = source.clone();
                baseline::apply_mines_insert(
                    &mut expected,
                    &context,
                    &timing,
                    0,
                    4,
                    bounds.0,
                    bounds.1,
                );
                apply_mines_insert(&mut actual, &context, &timing, 0, 4, bounds.0, bounds.1);
                assert_same(&actual, &expected);
            }
        }
    }
}

#[test]
fn simple_attack_masks_match_parent_for_all_mask_combinations_and_turns() {
    let timing = test_timing(2048);
    let source = mixed_fixture(64, 24, 0);
    let bits = [
        REMOVE_MASK_BIT_LITTLE,
        REMOVE_MASK_BIT_NO_HOLDS,
        REMOVE_MASK_BIT_NO_MINES,
        REMOVE_MASK_BIT_NO_FAKES,
        REMOVE_MASK_BIT_NO_LIFTS,
    ];
    for selected in 0..32 {
        let remove_mask = bits.iter().enumerate().fold(0, |mask, (i, bit)| {
            mask | if selected & (1 << i) != 0 { *bit } else { 0 }
        });
        for holds_mask in [
            0,
            HOLDS_MASK_BIT_NO_ROLLS,
            HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
            HOLDS_MASK_BIT_NO_ROLLS | HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
        ] {
            for turn in [
                GameplayTurnOption::None,
                GameplayTurnOption::Mirror,
                GameplayTurnOption::Shuffle,
                GameplayTurnOption::Blender,
                GameplayTurnOption::Random,
            ] {
                let mods = ParsedAttackMods {
                    remove_mask,
                    holds_mask,
                    turn_option: turn,
                    ..ParsedAttackMods::default()
                };
                for bounds in [(0, 2048), (144, 768), (5000, 6000), (24, 0)] {
                    let mut actual = source.clone();
                    let mut expected = source.clone();
                    baseline::apply_chart_attack_window(
                        &mut expected,
                        &timing,
                        4,
                        4,
                        1,
                        bounds,
                        mods,
                        27,
                    );
                    apply_chart_attack_window(&mut actual, &timing, 4, 4, 1, bounds, mods, 27);
                    assert_same(&actual, &expected);
                }
            }
        }
    }
}

#[test]
fn simple_attacks_keep_buffers_boundaries_and_conversion_order() {
    let timing = test_timing(4096);
    let source = mixed_fixture(128, 24, 0);
    for raw in [
        "nomines",
        "noholds,norolls,holdrolls",
        "little,nofakes,nolifts,mirror",
    ] {
        let mods = parse_attack_mods(raw);
        let mut actual = source.clone();
        let mut expected = source.clone();
        let pointer = actual.as_ptr();
        let capacity = actual.capacity();
        for bounds in [(144, 1536), (1008, 2048), (0, 4096)] {
            baseline::apply_chart_attack_window(&mut expected, &timing, 0, 4, 0, bounds, mods, 7);
            assert_no_churn(|| {
                apply_chart_attack_window(&mut actual, &timing, 0, 4, 0, bounds, mods, 7)
            });
            assert_same(&actual, &expected);
        }
        assert_eq!(actual.as_ptr(), pointer);
        assert_eq!(actual.capacity(), capacity);
    }
    let mods = ParsedAttackMods {
        remove_mask: REMOVE_MASK_BIT_NO_HOLDS,
        holds_mask: HOLDS_MASK_BIT_NO_ROLLS | HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
        ..ParsedAttackMods::default()
    };
    let mut rolls = mine_fixture(3, false, 0);
    apply_chart_attack_window(&mut rolls, &timing, 0, 4, 0, (0, 4096), mods, 0);
    assert!(
        rolls
            .iter()
            .all(|n| n.note_type == NoteType::Tap && n.hold.is_none())
    );
}

#[test]
fn modifier_combinations_preserve_duplicates_unsorted_and_interdependent_masks() {
    let timing = test_timing(8192);
    for shape in 0..4 {
        let mut source = mixed_fixture(128, 24, shape);
        if shape == 1 {
            source.extend(source.clone());
            sort_player_notes(&mut source);
        }
        if shape == 2 {
            source.reverse();
        }
        if shape == 3 {
            source.clear();
        }
        for raw in [
            "big,quick,skippy",
            "mines",
            "nomines,noholds,mirror",
            "nojumps,nofakes",
            "planted,nolifts",
            "bmrize,mines,norolls,wide",
            "stomp,echo,noquads",
        ] {
            let mods = parse_attack_mods(raw);
            // Simultaneous-note limits require row-sorted input in both
            // implementations; exercise their combinations on sorted shapes.
            if shape == 2
                && mods.remove_mask
                    & (REMOVE_MASK_BIT_NO_JUMPS
                        | REMOVE_MASK_BIT_NO_HANDS
                        | REMOVE_MASK_BIT_NO_QUADS)
                    != 0
            {
                continue;
            }
            // Mines require sorted context in the parent too. A full-chart
            // window leaves no external context when the source is reversed.
            let bounds = if shape == 2 && mods.insert_mask & INSERT_MASK_BIT_MINES != 0 {
                (0, 8192)
            } else {
                (144, 2304)
            };
            let mut actual = source.clone();
            let mut expected = source.clone();
            baseline::apply_chart_attack_window(&mut expected, &timing, 0, 4, 0, bounds, mods, 91);
            apply_chart_attack_window(&mut actual, &timing, 0, 4, 0, bounds, mods, 91);
            assert_same(&actual, &expected);
        }
    }
}

fn measure_pair(
    name: &str,
    source: &[Note],
    iterations: usize,
    old: impl Fn(&mut Vec<Note>),
    new: impl Fn(&mut Vec<Note>),
) {
    let mut expected = source.to_vec();
    let mut actual = source.to_vec();
    old(&mut expected);
    new(&mut actual);
    assert_same(&actual, &expected);
    let old_work: &dyn Fn(&mut Vec<Note>) = &old;
    let new_work: &dyn Fn(&mut Vec<Note>) = &new;
    let mut variants = [("old", old_work), ("new", new_work)];
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        variants.reverse();
    }
    for (version, function) in variants {
        measure_sampled(
            &format!("{name}_{version}"),
            iterations,
            source.len().max(1),
            || {
                // Give both implementations a fresh owned input and include its
                // clone plus output destruction in both measurements.
                let mut notes = black_box(source).to_vec();
                function(black_box(&mut notes));
                black_box(notes)
            },
        );
    }
}

#[test]
#[ignore = "manual paired release benchmark; run alone with --nocapture --test-threads=1"]
fn chart_modifier_bench() {
    let timing = test_timing(4096 * 192 + 192);
    for (name, count, spacing, window, insert, stride, skippy, iterations) in [
        ("intelligent_empty", 0, 48, 48, 24, 48, false, 20000),
        ("intelligent_tiny", 4, 48, 48, 24, 48, false, 10000),
        ("big_512", 512, 48, 48, 24, 48, false, 100),
        ("big_4096", 4096, 48, 48, 24, 48, false, 10),
        ("quick_4096", 4096, 24, 24, 12, 48, false, 10),
        ("skippy_4096", 4096, 48, 48, 36, 48, true, 10),
        ("intelligent_dense", 4096, 12, 48, 24, 48, false, 100),
        ("intelligent_overlap", 512, 96, 96, 24, 48, false, 100),
    ] {
        let source = fixture(count, spacing, 4, 0);
        measure_pair(
            name,
            &source,
            iterations,
            |notes| {
                baseline::apply_insert_intelligent_taps(
                    notes,
                    black_box(&timing),
                    0,
                    4,
                    window,
                    insert,
                    stride,
                    skippy,
                )
            },
            |notes| {
                apply_insert_intelligent_taps(
                    notes,
                    black_box(&timing),
                    0,
                    4,
                    window,
                    insert,
                    stride,
                    skippy,
                )
            },
        );
    }
    for (name, count, backwards, column, iterations) in [
        ("mines_empty", 0, false, 0, 20000),
        ("mines_tiny", 4, false, 0, 10000),
        ("mines_512", 512, false, 0, 100),
        ("mines_4096", 4096, false, 0, 10),
        ("mines_backward", 512, true, 0, 100),
        ("mines_foreign", 512, false, MAX_COLS, 100),
    ] {
        let source = mine_fixture(count, backwards, column);
        measure_pair(
            name,
            &source,
            iterations,
            |notes| {
                baseline::apply_mines_insert(
                    notes,
                    &[],
                    black_box(&timing),
                    0,
                    4,
                    0,
                    4096 * 192 + 192,
                )
            },
            |notes| apply_mines_insert(notes, &[], black_box(&timing), 0, 4, 0, 4096 * 192 + 192),
        );
    }
    for (name, count, raw, bounds, iterations) in [
        ("attack_empty", 0, "nomines", (0, 200000), 20000),
        ("attack_tiny", 4, "nomines", (0, 200000), 10000),
        ("attack_nomines", 4096, "nomines", (0, 200000), 100),
        ("attack_noholds", 4096, "noholds,norolls", (0, 200000), 100),
        (
            "attack_filter",
            4096,
            "little,nofakes,nolifts,mirror",
            (0, 200000),
            100,
        ),
        ("attack_middle", 4096, "nomines", (40000, 50000), 100),
        ("attack_simultaneous", 4096, "nojumps", (0, 200000), 100),
    ] {
        let source = mixed_fixture(count, 24, 0);
        let mods = parse_attack_mods(raw);
        measure_pair(
            name,
            &source,
            iterations,
            |notes| {
                baseline::apply_chart_attack_window(
                    notes,
                    black_box(&timing),
                    0,
                    4,
                    0,
                    bounds,
                    black_box(mods),
                    7,
                )
            },
            |notes| {
                apply_chart_attack_window(
                    notes,
                    black_box(&timing),
                    0,
                    4,
                    0,
                    bounds,
                    black_box(mods),
                    7,
                )
            },
        );
    }
}
