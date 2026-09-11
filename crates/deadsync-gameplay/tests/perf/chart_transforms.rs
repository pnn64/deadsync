use super::*;
use std::hint::black_box;

type SimultaneousFilter = fn(&mut Vec<Note>, usize, usize, usize);

#[test]
fn ordinary_simultaneous_filter_keeps_storage_without_churn() {
    for limit in [0, 2, 4] {
        let mut notes = chart_fixture(512, 4, 0);
        let pointer = notes.as_ptr();
        let capacity = notes.capacity();
        crate::perf::assert_no_churn(|| enforce_max_simultaneous_notes(&mut notes, limit, 0, 4));
        assert_eq!(notes.len(), 512 * limit);
        assert_eq!(notes.as_ptr(), pointer);
        assert_eq!(notes.capacity(), capacity);
    }
}

#[test]
fn chart_attack_buffers_only_allocate_the_second_player_range() {
    let timing = test_timing(48 * 64);
    for count in [1, 2] {
        let mut notes = chart_fixture(128, 4, 0);
        let split = notes.len();
        if count == 2 {
            notes.extend(chart_fixture(128, 4, 4));
        }
        let pointer = notes.as_ptr();
        let mut ranges = if count == 1 {
            [(0, split); 2]
        } else {
            [(0, split), (split, notes.len())]
        };
        let players = [ChartAttackTransformPlayer {
            chart_attacks: Some("TIME=0:LEN=1000:MODS=mirror"),
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        }; MAX_PLAYERS];
        crate::perf::assert_churn_budget(
            count - 1,
            (count - 1) * split * size_of::<Note>(),
            || {
                apply_chart_attack_transforms(&mut notes, &mut ranges, 4, count, &players, 7, 32.0);
            },
        );
        assert_eq!(notes.as_ptr(), pointer);
        assert_eq!(ranges[0], (0, split));
        assert_eq!(
            ranges[1],
            if count == 1 {
                (0, split)
            } else {
                (split, split * 2)
            }
        );
    }
}

fn chart_fixture(rows: usize, cols: usize, offset: usize) -> Vec<Note> {
    (0..rows)
        .flat_map(|row| {
            (0..cols).map(move |column| {
                let mut note = test_note_at(NoteType::Tap, None, false, row * 12, row as f32 / 4.0);
                note.column = offset + column;
                note.quantization_idx = (row % 9) as u8;
                note
            })
        })
        .collect()
}

fn assert_notes_equal(actual: &[Note], expected: &[Note]) {
    // Note has no PartialEq; Debug covers every field, including hold state,
    // judgments, fake flags and metadata in these finite-valued fixtures.
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}

#[test]
fn simultaneous_filter_matches_legacy_for_holds_duplicates_and_foreign_lanes() {
    for cols in [0, 1, 4, 5, 10, MAX_COLS + 1] {
        for offset in [0, 4] {
            for seed in 0..12_u64 {
                let mut rng = seed + 1;
                let mut notes = Vec::new();
                for row in 0..32 {
                    // Oversized rows exercise duplicate cells and spill storage.
                    for item in 0..(if row % 5 == 0 { MAX_COLS + 7 } else { cols + 1 }) {
                        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                        let kind = [
                            NoteType::Tap,
                            NoteType::Lift,
                            NoteType::Hold,
                            NoteType::Roll,
                            NoteType::Mine,
                            NoteType::Fake,
                        ][(rng >> 32) as usize % 6];
                        let hold = matches!(kind, NoteType::Hold | NoteType::Roll).then(|| {
                            let mut hold = test_hold();
                            hold.end_row_index = (row + item % 4).saturating_sub(item % 2);
                            hold.end_beat = hold.end_row_index as f32 / 48.0;
                            hold.life = 0.75;
                            hold
                        });
                        let mut note =
                            test_note_at(kind, hold, rng & 8 != 0, row, row as f32 / 48.0);
                        note.column = (rng >> 40) as usize % (offset + cols + 3);
                        note.can_be_judged = rng & 16 != 0;
                        note.quantization_idx = item as u8;
                        note.result = Some(test_judgment(JudgeGrade::Excellent));
                        notes.push(note);
                    }
                }
                for limit in [0, 1, 2, 3, usize::MAX] {
                    let mut actual = notes.clone();
                    let mut expected = notes.clone();
                    enforce_max_simultaneous_notes(&mut actual, limit, offset, cols);
                    legacy_enforce_max_simultaneous_notes(&mut expected, limit, offset, cols);
                    assert_notes_equal(&actual, &expected);
                }
            }
        }
    }
}

#[test]
fn chart_attack_owner_paths_match_legacy_ranges_and_seeded_results() {
    let timing = test_timing(48 * 16);
    let mut source = chart_fixture(12, 4, 0);
    let split = source.len();
    source.extend(chart_fixture(12, 4, 4));
    let len = source.len();
    for ranges in [
        [(0, len), (999, 999)],
        [(0, split), (split, len)],
        [(0, split / 2), (split, len)],
        [(3, len - 1), (0, split)],
        [(0, split + 5), (4, len)],
        [(1000, 3), (1001, len)],
        [(0, len + 7), (len + 7, len)],
        [(0, 0), (0, 0)],
    ] {
        for players_count in [0, 1, 2, 3] {
            for modes in [
                [GameplayAttackMode::On; 2],
                [GameplayAttackMode::Off, GameplayAttackMode::On],
                [GameplayAttackMode::On, GameplayAttackMode::Off],
                [GameplayAttackMode::Random; 2],
            ] {
                let players = std::array::from_fn(|player| ChartAttackTransformPlayer {
                    chart_attacks: Some(if player == 0 {
                        "TIME=0:LEN=2:MODS=mirror:TIME=1:LEN=2:MODS=nojumps"
                    } else {
                        "TIME=0:LEN=3:MODS=big,shuffle"
                    }),
                    attack_mode: modes[player],
                    timing_player: &timing,
                });
                let mut actual = source.clone();
                let mut expected = source.clone();
                let mut actual_ranges = ranges;
                let mut expected_ranges = ranges;
                apply_chart_attack_transforms(
                    &mut actual,
                    &mut actual_ranges,
                    4,
                    players_count,
                    &players,
                    123,
                    20.0,
                );
                legacy_apply_chart_attack_transforms(
                    &mut expected,
                    &mut expected_ranges,
                    4,
                    players_count,
                    &players,
                    123,
                    20.0,
                );
                assert_eq!(actual_ranges, expected_ranges);
                assert_notes_equal(&actual, &expected);
            }
        }
    }
}

#[test]
fn empty_chart_and_hold_endpoints_keep_filter_policy() {
    let mut empty = vec![];
    enforce_max_simultaneous_notes(&mut empty, 0, 0, 4);
    assert!(empty.is_empty());
    let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
    hold.hold.as_mut().unwrap().end_row_index = 48;
    let mut notes = vec![hold];
    for row in [48, 49] {
        let mut tap = test_note_at(NoteType::Tap, None, false, row, row as f32 / 48.0);
        tap.column = 1;
        notes.push(tap);
    }
    enforce_max_simultaneous_notes(&mut notes, 1, 0, 4);
    assert_eq!(
        notes.iter().map(|note| note.row_index).collect::<Vec<_>>(),
        vec![0, 49]
    );
}

#[test]
#[ignore = "manual release comparison; run serially with --nocapture"]
fn chart_transform_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for rows in [128, 2048, 8192] {
        let source = chart_fixture(rows, 4, 0);
        for limit in [2, 4] {
            let mut routines: [(&str, SimultaneousFilter); 2] = [
                ("old", legacy_enforce_max_simultaneous_notes),
                ("new", enforce_max_simultaneous_notes),
            ];
            if reverse {
                routines.reverse();
            }
            for (label, routine) in routines {
                let mut notes = Vec::with_capacity(source.len());
                crate::perf::measure_sampled(
                    &format!("limit_{rows}_{limit}_{label}"),
                    32,
                    source.len(),
                    || {
                        notes.clear();
                        notes.extend_from_slice(&source);
                        routine(black_box(&mut notes), limit, 0, 4);
                        black_box(notes.len())
                    },
                );
            }
        }
        let timing = test_timing(rows * 12 + 48);
        for count in [1, 2] {
            let mut source = source.clone();
            let split = source.len();
            if count == 2 {
                source.extend(chart_fixture(rows, 4, 4));
            }
            let ranges = if count == 1 {
                [(0, split); 2]
            } else {
                [(0, split), (split, source.len())]
            };
            let players = [ChartAttackTransformPlayer {
                chart_attacks: Some("TIME=0:LEN=100000:MODS=mirror"),
                attack_mode: GameplayAttackMode::On,
                timing_player: &timing,
            }; MAX_PLAYERS];
            let mut old_first = [true, false];
            if reverse {
                old_first.reverse();
            }
            for old in old_first {
                let label = if old { "old" } else { "new" };
                let mut notes = Vec::with_capacity(source.len());
                crate::perf::measure_sampled(
                    &format!("attack_{rows}_{count}_{label}"),
                    32,
                    source.len(),
                    || {
                        notes.clear();
                        notes.extend_from_slice(&source);
                        let mut ranges = ranges;
                        if old {
                            legacy_apply_chart_attack_transforms(
                                black_box(&mut notes),
                                &mut ranges,
                                4,
                                count,
                                &players,
                                7,
                                10.0,
                            );
                        } else {
                            apply_chart_attack_transforms(
                                black_box(&mut notes),
                                &mut ranges,
                                4,
                                count,
                                &players,
                                7,
                                10.0,
                            );
                        }
                        black_box((notes.len(), ranges))
                    },
                );
            }
        }
    }
}

// Frozen 0.5.1133 routines for behavior and same-binary performance comparisons.
fn legacy_enforce_max_simultaneous_notes(
    notes: &mut Vec<Note>,
    max_simultaneous: usize,
    col_offset: usize,
    cols: usize,
) {
    if notes.is_empty() || cols == 0 || cols > MAX_COLS {
        return;
    }
    debug_assert!(notes_row_sorted(notes));

    let mut remove_idx = vec![false; notes.len()];
    let mut active_hold_ends: [Option<usize>; MAX_COLS] = [None; MAX_COLS];
    let mut row_candidates = Vec::<(usize, usize)>::with_capacity(MAX_COLS);

    let mut row_start = 0usize;
    while row_start < notes.len() {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row {
            row_end += 1;
        }

        for held in active_hold_ends.iter_mut().take(cols) {
            if held.is_some_and(|end| end < row) {
                *held = None;
            }
        }

        let active_holds = active_hold_ends
            .iter()
            .take(cols)
            .filter(|end| end.is_some())
            .count();

        row_candidates.clear();
        for (offset, note) in notes[row_start..row_end].iter().enumerate() {
            let idx = row_start + offset;
            if note.column < col_offset {
                continue;
            }
            let local_col = note.column - col_offset;
            if local_col >= cols || !note_counts_for_simultaneous_limit(note) {
                continue;
            }
            row_candidates.push((local_col, idx));
        }

        if row_candidates.is_empty() {
            row_start = row_end;
            continue;
        }

        row_candidates.sort_unstable_by_key(|(local_col, _)| *local_col);
        let mut tracks_to_remove = active_holds
            .saturating_add(row_candidates.len())
            .saturating_sub(max_simultaneous);

        if tracks_to_remove > 0 {
            for &(_, idx) in &row_candidates {
                if tracks_to_remove == 0 {
                    break;
                }
                remove_idx[idx] = true;
                tracks_to_remove -= 1;
            }
        }

        for &(local_col, idx) in &row_candidates {
            if remove_idx[idx] || !matches!(notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let end_row = notes[idx]
                .hold
                .as_ref()
                .map(|hold| hold.end_row_index)
                .unwrap_or(row);
            if active_hold_ends[local_col].is_none_or(|current| current < end_row) {
                active_hold_ends[local_col] = Some(end_row);
            }
        }

        row_start = row_end;
    }

    if remove_idx.iter().all(|remove| !*remove) {
        return;
    }

    let mut idx = 0usize;
    notes.retain(|_| {
        let keep = !remove_idx[idx];
        idx += 1;
        keep
    });
}

fn legacy_apply_chart_attack_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    players: &[ChartAttackTransformPlayer<'_>; MAX_PLAYERS],
    base_seed: u64,
    song_length_seconds: f32,
) {
    let active_players = num_players.min(MAX_PLAYERS);
    if active_players == 0
        || !players
            .iter()
            .take(active_players)
            .any(|player| player.has_chart_attacks())
    {
        return;
    }

    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];
    for player in 0..active_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let out_start = transformed.len();
        let attack_player = players[player];
        if !attack_player.has_chart_attacks() {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }

        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_chart_attacks_for_mode(
            &mut player_notes,
            attack_player.chart_attacks,
            attack_player.attack_mode,
            attack_player.timing_player,
            player.saturating_mul(cols_per_player),
            cols_per_player,
            player,
            base_seed,
            song_length_seconds,
        );
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if active_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }
    *notes = transformed;
    *note_ranges = transformed_ranges;
}
