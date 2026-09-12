use super::*;
use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/preparation/baseline.rs"
    ));
}

fn timing_fixture(kind: usize, count: usize) -> TimingData {
    let mut segments = TimingSegments {
        bpms: vec![(0.0, 150.0)],
        ..TimingSegments::default()
    };
    if kind > 0 {
        for i in 1..count {
            let beat = i as f32 * 8.0;
            segments.bpms.push((beat, 90.0 + (i % 9) as f32 * 20.0));
            segments.stops.push(StopSegment {
                beat: beat + 1.0,
                duration: 0.125,
            });
            segments.delays.push(DelaySegment {
                beat: beat + 2.0,
                duration: 0.0625,
            });
            segments.warps.push(WarpSegment {
                beat: beat + 3.0,
                length: 1.0,
            });
            segments.fakes.push(FakeSegment {
                beat: beat + 5.0,
                length: 0.5,
            });
        }
    }
    if kind == 2 {
        segments.bpms.extend([(0.001, 130.0), (0.001, 170.0)]);
    }
    TimingData::from_segments(-0.125, 0.037, &segments, &[])
}

fn annotations(count: usize) -> Vec<CrossoverRow> {
    (0..count)
        .map(|i| CrossoverRow {
            beat: i as f32 * 0.25,
            column_mask: 1 << [1, 0, 2, 3][i % 4],
            crossover: i % 2 == 1,
            bracket: i % 11 == 0,
        })
        .collect()
}

fn cue_signature(cues: &[ColumnCue]) -> Vec<(u32, u32, ColumnCueColumns)> {
    cues.iter()
        .map(|cue| {
            (
                cue.start_time.to_bits(),
                cue.duration.to_bits(),
                cue.columns,
            )
        })
        .collect()
}

#[test]
fn crossover_timing_matches_old_for_boundaries_rewinds_and_options() {
    for count in [0, 1, 2, 15, 16, 257] {
        for kind in 0..3 {
            let timing = timing_fixture(kind, 16);
            let mut rows = annotations(count);
            for order in 0..3 {
                if order == 1 {
                    rows.reverse();
                }
                if order == 2 {
                    for (i, row) in rows.iter_mut().enumerate() {
                        row.beat = [
                            -2.0,
                            0.001,
                            0.002,
                            f32::NAN,
                            f32::INFINITY,
                            f32::NEG_INFINITY,
                            f32::MAX,
                            8.0,
                        ][i % 8];
                        row.column_mask = [0, 255, 16, 128, 3][i % 5];
                    }
                }
                for duration in [0, 500, u16::MAX] {
                    for quantization in [0, 4, 192, 255] {
                        for brackets in [false, true] {
                            for visible in [-4.0, 0.0, f32::NAN, f32::NEG_INFINITY] {
                                let old = baseline::build_crossover_cues_from_annotations(
                                    &rows,
                                    &timing,
                                    4,
                                    duration,
                                    quantization,
                                    brackets,
                                    visible,
                                );
                                let new = build_crossover_cues_from_annotations(
                                    &rows,
                                    &timing,
                                    4,
                                    duration,
                                    quantization,
                                    brackets,
                                    visible,
                                );
                                assert_eq!(
                                    cue_signature(&old),
                                    cue_signature(&new),
                                    "count={count} kind={kind} order={order}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn column_fixture(rows: usize, chord: usize, gap_ns: i64, ignored: bool) -> (Vec<Note>, Vec<i64>) {
    let mut notes = Vec::with_capacity(rows * chord);
    let mut times = Vec::with_capacity(rows * chord);
    for row in 0..rows {
        for column in 0..chord {
            let mut note = test_note_at(NoteType::Tap, None, ignored, row * 12, row as f32 * 0.25);
            note.column = column;
            notes.push(note);
            times.push((row as i64 + 1) * gap_ns);
        }
    }
    (notes, times)
}

#[test]
fn column_cues_match_old_for_types_ranges_times_and_filtered_rows() {
    for rows in [0, 1, 4, 257] {
        for chord in [1, 4, MAX_COLS + 2] {
            let (mut notes, mut times) = column_fixture(rows, chord, 100_000_000, false);
            for variant in 0..4 {
                if variant == 1 {
                    for (i, (note, time)) in notes.iter_mut().zip(&mut times).enumerate() {
                        note.note_type = [
                            NoteType::Tap,
                            NoteType::Hold,
                            NoteType::Roll,
                            NoteType::Mine,
                            NoteType::Lift,
                            NoteType::Fake,
                        ][i % 6];
                        note.is_fake = i % 7 == 0;
                        *time = [
                            INVALID_SONG_TIME_NS,
                            -1_000_000_000,
                            0,
                            1_499_999_999,
                            1_500_000_000,
                            1_500_000_001,
                            i64::MAX,
                            42,
                        ][i % 8];
                    }
                }
                if variant == 2 {
                    notes.reverse();
                    times.reverse();
                }
                if variant == 3 {
                    for note in &mut notes {
                        note.is_fake = true;
                    }
                }
                for range in [
                    (0, notes.len()),
                    (notes.len() / 3, notes.len() * 2 / 3),
                    (notes.len(), 0),
                ] {
                    for cols in [(0, MAX_COLS), (2, 6), (MAX_COLS, MAX_COLS + 8), (8, 0)] {
                        for visible in [-4.0, 0.0, f32::NAN, f32::NEG_INFINITY] {
                            let old = baseline::build_column_cues_for_player(
                                &notes, range, &times, cols.0, cols.1, visible,
                            );
                            let new = build_column_cues_for_player(
                                &notes, range, &times, cols.0, cols.1, visible,
                            );
                            assert_eq!(cue_signature(&old), cue_signature(&new));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn no_cue_inputs_have_no_heap_churn() {
    let (notes, times) = column_fixture(4096, 4, 100_000_000, true);
    assert_no_churn(|| {
        assert!(
            build_column_cues_for_player(&notes, (0, notes.len()), &times, 0, 4, 0.0).is_empty()
        )
    });
    let timing = timing_fixture(1, 1024);
    let mut rows = annotations(4096);
    for row in &mut rows {
        row.crossover = false;
    }
    assert_no_churn(|| {
        assert!(
            build_crossover_cues_from_annotations(&rows, &timing, 0, 500, 4, false, 0.0).is_empty()
        )
    });
}

fn replay_fixture(count: usize, kind: usize) -> Vec<ReplayInputEdge> {
    (0..count)
        .map(|i| ReplayInputEdge {
            lane_index: if kind == 2 {
                255
            } else if kind == 1 && i % 5 == 0 {
                10
            } else {
                (i % 8) as u8
            },
            pressed: i % 3 != 0,
            source: if i % 2 == 0 {
                InputSource::Gamepad
            } else {
                InputSource::Keyboard
            },
            event_music_time_ns: if kind == 1 && i % 7 == 0 {
                INVALID_SONG_TIME_NS
            } else {
                (i / 4) as i64 * 1_000_000
            },
        })
        .collect()
}

#[test]
fn owned_replays_match_old_filtering_shifts_saturation_and_stable_ties() {
    for count in [0, 1, 4, 65, 4096] {
        for kind in 0..3 {
            let mut input = replay_fixture(count, kind);
            for order in 0..3 {
                if order == 1 {
                    input.reverse();
                }
                if order == 2 {
                    for (i, edge) in input.iter_mut().enumerate() {
                        edge.event_music_time_ns =
                            [i64::MIN, i64::MIN + 1, -30, 0, 42, i64::MAX][i % 6];
                    }
                }
                for (players, width, cols) in
                    [(0, 0, 0), (1, 4, 4), (2, 4, 8), (2, 0, 8), (2, 8, 16)]
                {
                    for recorded in [INVALID_SONG_TIME_NS, i64::MIN + 1, -10, 0, i64::MAX] {
                        for current in [
                            [0, 0],
                            [30, 80],
                            [i64::MAX, i64::MIN + 1],
                            [INVALID_SONG_TIME_NS, 0],
                        ] {
                            let old = baseline::build_replay_input_edges(
                                &input, players, width, cols, recorded, current,
                            );
                            let new = build_replay_input_edges_owned(
                                input.clone(),
                                players,
                                width,
                                cols,
                                recorded,
                                current,
                            );
                            assert_eq!(old, new, "count={count} kind={kind} order={order}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn owned_ordered_replay_conversion_reuses_input_allocation() {
    assert_eq!(size_of::<ReplayInputEdge>(), size_of::<RecordedLaneEdge>());
    assert_eq!(
        align_of::<ReplayInputEdge>(),
        align_of::<RecordedLaneEdge>()
    );
    for count in [1, 4, 65, 65_536] {
        for kind in 0..3 {
            let input = replay_fixture(count, kind);
            let pointer = input.as_ptr() as usize;
            let capacity = input.capacity();
            let mut output = None;
            assert_no_churn(|| {
                output = Some(build_replay_input_edges_owned(input, 2, 4, 8, 0, [0, 0]));
            });
            let output = output.unwrap();
            assert_eq!(output.as_ptr() as usize, pointer);
            assert_eq!(output.capacity(), capacity);
        }
    }
}

#[test]
fn prepared_replay_playback_batches_match_old() {
    let input = replay_fixture(4096, 1);
    let mut old = GameplayReplayInputState::new(baseline::build_replay_input_edges(
        &input,
        2,
        4,
        8,
        0,
        [0, -10_000_000],
    ));
    let mut new = GameplayReplayInputState::new(build_replay_input_edges_owned(
        input,
        2,
        4,
        8,
        0,
        [0, -10_000_000],
    ));
    assert_eq!(old, new);
    for time in [-1, 0, 10_000_000, 500_000_000, i64::MAX] {
        loop {
            let mut old_events = [None; 17];
            let mut new_events = [None; 17];
            let count = old.collect_ready(time, 8, &mut old_events);
            assert_eq!(count, new.collect_ready(time, 8, &mut new_events));
            assert_eq!(old_events, new_events);
            if count == 0 {
                break;
            }
        }
        assert_eq!(old, new);
    }
    old.reset_cursor();
    new.reset_cursor();
    assert_eq!(old, new);
}

fn pair<T>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> T,
    mut new: impl FnMut() -> T,
) {
    let old_name = format!("{name}_old");
    let new_name = format!("{name}_new");
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        measure_sampled(&new_name, iterations, units, &mut new);
        measure_sampled(&old_name, iterations, units, &mut old);
    } else {
        measure_sampled(&old_name, iterations, units, &mut old);
        measure_sampled(&new_name, iterations, units, &mut new);
    }
}

#[test]
#[ignore = "manual release old/new CPU, churn and throughput comparison"]
fn preparation_bench() {
    for (name, count, kind, reverse, iterations) in [
        ("cross_empty", 0, 0, false, 50_000),
        ("cross_tiny", 4, 0, false, 20_000),
        ("cross_simple", 4096, 0, false, 100),
        ("cross_dense", 4096, 1, false, 8),
        ("cross_ambiguous", 1024, 2, false, 10),
        ("cross_reverse", 1024, 1, true, 10),
        ("cross_inactive", 4096, 1, false, 2000),
    ] {
        let mut rows = annotations(count);
        if reverse {
            rows.reverse();
        }
        if name == "cross_inactive" {
            for row in &mut rows {
                row.crossover = false;
            }
        }
        let timing = timing_fixture(kind, count.div_ceil(32));
        pair(
            name,
            iterations,
            count.max(1),
            || {
                baseline::build_crossover_cues_from_annotations(
                    black_box(&rows),
                    black_box(&timing),
                    0,
                    500,
                    4,
                    false,
                    -4.0,
                )
            },
            || {
                build_crossover_cues_from_annotations(
                    black_box(&rows),
                    black_box(&timing),
                    0,
                    500,
                    4,
                    false,
                    -4.0,
                )
            },
        );
    }
    for (name, rows, chord, gap, ignored, iterations) in [
        ("column_empty", 0, 4, 100_000_000, false, 50_000),
        ("column_tiny", 1, 1, 100_000_000, false, 50_000),
        ("column_single", 4096, 1, 100_000_000, false, 1000),
        ("column_chords", 4096, 4, 100_000_000, false, 500),
        ("column_wide", 4096, 16, 100_000_000, false, 100),
        ("column_sparse", 4096, 4, 2_000_000_000, false, 100),
        ("column_ignored", 4096, 4, 100_000_000, true, 500),
    ] {
        let (notes, times) = column_fixture(rows, chord, gap, ignored);
        pair(
            name,
            iterations,
            notes.len().max(1),
            || {
                baseline::build_column_cues_for_player(
                    black_box(&notes),
                    (0, notes.len()),
                    black_box(&times),
                    0,
                    16,
                    -4.0,
                )
            },
            || {
                build_column_cues_for_player(
                    black_box(&notes),
                    (0, notes.len()),
                    black_box(&times),
                    0,
                    16,
                    -4.0,
                )
            },
        );
    }
    for (name, count, kind, reverse, current, iterations) in [
        ("replay_empty", 0, 0, false, [0, 0], 50_000),
        ("replay_tiny", 4, 0, false, [0, 0], 50_000),
        ("replay_ordered", 65_536, 0, false, [0, 0], 80),
        ("replay_filtered", 65_536, 1, false, [0, 0], 80),
        ("replay_ignored", 65_536, 2, false, [0, 0], 100),
        ("replay_shifted", 65_536, 0, false, [0, -10_000_000], 40),
        ("replay_reverse", 65_536, 0, true, [0, 0], 40),
    ] {
        let mut input = replay_fixture(count, kind);
        if reverse {
            input.reverse();
        }
        pair(
            name,
            iterations,
            count.max(1),
            || {
                let owned = black_box(input.clone());
                baseline::build_replay_input_edges(&owned, 2, 4, 8, 0, current)
            },
            || build_replay_input_edges_owned(black_box(input.clone()), 2, 4, 8, 0, current),
        );
    }
}
