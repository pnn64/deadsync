use super::*;
use crate::life::{LIFE_HISTORY_SAME_TIME_SHIFT, record_life_change, record_life_history};
use std::hint::black_box;

#[path = "row_traversal/baseline.rs"]
mod baseline;

fn note(row: usize, column: usize, sequence: usize, mixed: bool) -> Note {
    let grade = [
        JudgeGrade::Fantastic,
        JudgeGrade::Excellent,
        JudgeGrade::Great,
        JudgeGrade::Decent,
        JudgeGrade::WayOff,
        JudgeGrade::Miss,
    ][sequence % 6];
    let error = (sequence % 101) as f32 - 50.0;
    let mut note = Note {
        beat: row as f32 / 48.0,
        quantization_idx: 0,
        column,
        note_type: NoteType::Tap,
        row_index: row,
        result: Some(Judgment {
            time_error_ms: error,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(error, 1.0),
            grade,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        }),
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    };
    note.quantization_idx = (sequence % 9) as u8;
    if mixed {
        note.note_type = [
            NoteType::Tap,
            NoteType::Hold,
            NoteType::Roll,
            NoteType::Lift,
            NoteType::Mine,
        ][sequence % 5];
        note.is_fake = sequence.is_multiple_of(17);
        note.can_be_judged = !sequence.is_multiple_of(19);
        if sequence.is_multiple_of(13) {
            note.result = None;
        } else if let Some(judgment) = &mut note.result {
            judgment.miss_because_held = sequence.is_multiple_of(3);
            judgment.time_error_music_ns = (sequence % 5) as i64 - 2;
            judgment.time_error_ms = [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -0.0,
                0.0,
                -12.0,
                12.0,
            ][sequence % 7];
        }
    }
    note
}

fn notes(rows: usize, chord: usize, mixed: bool) -> Vec<Note> {
    (0..rows)
        .flat_map(|row| {
            (0..chord).map(move |column| note(row * 48, column, row * chord + column, mixed))
        })
        .collect()
}

fn history_bits(history: &[(f32, f32)]) -> Vec<(u32, u32)> {
    history
        .iter()
        .map(|&(t, life)| (t.to_bits(), life.to_bits()))
        .collect()
}

fn stats_bits(stats: TimingStats) -> [u32; 4] {
    // Arithmetic NaNs need not retain a particular sign/payload. Check their
    // classification, but keep bit comparisons for all other values, including
    // infinities and signed zero. Stored life/scatter samples are compared raw.
    let bits = |value: f32| {
        if value.is_nan() {
            f32::NAN.to_bits()
        } else {
            value.to_bits()
        }
    };
    [
        bits(stats.mean_ms),
        bits(stats.mean_abs_ms),
        bits(stats.stddev_ms),
        bits(stats.max_abs_ms),
    ]
}

fn window_values(counts: WindowCounts) -> [u32; 7] {
    [
        counts.w0,
        counts.w1,
        counts.w2,
        counts.w3,
        counts.w4,
        counts.w5,
        counts.miss,
    ]
}

fn scatter_bits(point: &ScatterPoint) -> (u32, Option<u32>, u8, bool, usize, u8, ScatterFoot) {
    (
        point.time_sec.to_bits(),
        point.offset_ms.map(f32::to_bits),
        point.direction_code,
        point.miss_because_held,
        point.row_index,
        point.quantization_idx,
        point.parity_foot,
    )
}

#[test]
fn life_history_matches_parent_after_every_event_and_at_capacity_boundaries() {
    for capacity in [0, 1, 2, 3, 4, 16] {
        for initial in [
            vec![],
            vec![(0.0, 1.0)],
            vec![(0.0, 1.0), (1.0, 1.0)],
            vec![(1.0, 0.5), (1.0 + LIFE_HISTORY_SAME_TIME_SHIFT, 0.7)],
        ] {
            let mut old = Vec::with_capacity(capacity);
            old.extend_from_slice(&initial);
            let mut new = old.clone();
            for i in 0..4096 {
                let time = match i % 23 {
                    0 => f32::NAN,
                    1 => f32::NEG_INFINITY,
                    2 => -1.0,
                    3 => i as f32 / 8.0 - 0.000_000_5,
                    _ => (i / 8) as f32,
                };
                let life = match i % 29 {
                    0 => f32::NAN,
                    1 => f32::INFINITY,
                    2 => f32::NEG_INFINITY,
                    3 => -0.0,
                    4 => 0.0,
                    5 => 0.8,
                    6 => -1.0,
                    _ => 1.0,
                };
                baseline::record_life_history(&mut old, time, life);
                record_life_history(&mut new, time, life);
                assert_eq!(history_bits(&new), history_bits(&old), "event {i}");
            }
        }
    }
    // Replacing a plateau must keep the incoming signed-zero bit pattern.
    let mut old = vec![(0.0, -0.0), (1.0, -0.0)];
    let mut new = old.clone();
    for (time, life) in [(2.0, 0.0), (3.0, -0.0), (f32::INFINITY, 0.0)] {
        baseline::record_life_history(&mut old, time, life);
        record_life_history(&mut new, time, life);
        assert_eq!(history_bits(&new), history_bits(&old));
    }
}

#[test]
fn full_plateau_buffer_does_not_grow_or_allocate() {
    let mut history = vec![(0.0, 1.0), (1.0, 1.0)].into_boxed_slice().into_vec();
    let capacity = history.capacity();
    crate::perf::assert_no_churn(|| {
        for i in 2..10000 {
            record_life_history(black_box(&mut history), i as f32, black_box(1.0));
        }
    });
    assert_eq!(history, [(0.0, 1.0), (9999.0, 1.0)]);
    assert_eq!(history.capacity(), capacity);
}

#[test]
fn paired_life_changes_match_two_parent_calls_including_timestamp_edges() {
    let values = [-1.0, -0.0, 0.0, 0.5, 1.0, 2.0, f32::NAN, f32::INFINITY];
    let times = [
        f32::NEG_INFINITY,
        f32::NAN,
        -1.0,
        0.0,
        0.000_000_5,
        LIFE_HISTORY_SAME_TIME_SHIFT - 0.000_000_5,
        LIFE_HISTORY_SAME_TIME_SHIFT,
        LIFE_HISTORY_SAME_TIME_SHIFT + 0.000_000_5,
        1.0,
        65536.0,
        f32::INFINITY,
    ];
    for history in [
        vec![],
        vec![(0.0, 0.5)],
        vec![(-1.0, 0.5), (0.0, 0.5)],
        vec![(-1.0, 1.0), (0.0, 0.5)],
        vec![(f32::NAN, 0.5)],
    ] {
        for &time in &times {
            for &before in &values {
                for &after in &values {
                    let mut old = history.clone();
                    let mut new = history.clone();
                    baseline::record_life_history(&mut old, time, before);
                    baseline::record_life_history(&mut old, time, after);
                    record_life_change(&mut new, time, before, after);
                    assert_eq!(history_bits(&new), history_bits(&old));
                }
            }
        }
    }
    let mut old = vec![(0.0, 0.5)];
    let mut new = old.clone();
    for i in 0..4096 {
        let time = (i / 3) as f32 * LIFE_HISTORY_SAME_TIME_SHIFT;
        let before = old.last().unwrap().1;
        let after = (i % 101) as f32 / 100.0;
        baseline::record_life_history(&mut old, time, before);
        baseline::record_life_history(&mut old, time, after);
        record_life_change(&mut new, time, before, after);
        assert_eq!(history_bits(&new), history_bits(&old));
    }
}

#[test]
fn paired_life_changes_reuse_space_for_the_final_samples() {
    let mut history = Vec::with_capacity(2050);
    history.push((0.0, 0.5));
    crate::perf::assert_no_churn(|| {
        for i in 1..1024 {
            let before = history.last().unwrap().1;
            record_life_change(
                black_box(&mut history),
                i as f32,
                before,
                (i % 101) as f32 / 100.0,
            );
        }
    });
    assert!(history.len() > 1024);
}

#[test]
fn row_visitor_keeps_exact_selected_note_and_summary_bits() {
    for rows in [0, 1, 7, 257] {
        for chord in [1, 2, 4, 10, 33] {
            for mixed in [false, true] {
                let mut notes = notes(rows, chord, mixed);
                // Repeated row IDs separated by another row remain distinct runs.
                for note in &mut notes {
                    note.row_index %= 7 * 48;
                }
                let mut old = Vec::new();
                let mut new = Vec::new();
                baseline::for_each_row_final_judgment(&notes, |j| old.push(j as *const Judgment));
                for_each_row_final_judgment(&notes, |j| new.push(j as *const Judgment));
                assert_eq!(new, old);
                assert_eq!(
                    stats_bits(compute_note_timing_stats(&notes)),
                    stats_bits(baseline::compute_note_timing_stats(&notes))
                );
                for window in [-1.0, 0.0, 10.0, 23.0, f32::NAN, f32::INFINITY] {
                    assert_eq!(
                        window_values(compute_window_counts_blue_ms(&notes, window)),
                        window_values(baseline::compute_window_counts_blue_ms(&notes, window))
                    );
                }
            }
        }
    }
}

#[test]
fn scatter_keeps_representative_ties_directions_parity_and_short_time_caches() {
    for rows in [0, 1, 7, 128] {
        for chord in [1, 2, 4, 10, 300] {
            let notes = notes(rows, chord, true);
            let times: Vec<_> = (0..notes.len())
                .map(|i| (i as i64 - 10) * 123_456_789)
                .collect();
            let parity: Vec<_> = (0..rows * 2)
                .map(|i| {
                    (
                        i * 24,
                        [ScatterFoot::Unknown, ScatterFoot::Left, ScatterFoot::Right][i % 3],
                    )
                })
                .collect();
            for (offset, columns) in [(0, 0), (0, 4), (4, 4), (0, 10), (0, 300), (usize::MAX, 2)] {
                for cache in [&times[..], &times[..times.len() / 2], &[]] {
                    for parity in [None, Some(parity.as_slice())] {
                        let expected =
                            baseline::build_scatter_points(&notes, cache, offset, columns, parity);
                        let actual = build_scatter_points(&notes, cache, offset, columns, parity);
                        assert_eq!(
                            actual.iter().map(scatter_bits).collect::<Vec<_>>(),
                            expected.iter().map(scatter_bits).collect::<Vec<_>>()
                        );
                        let mut visited = Vec::new();
                        visit_scatter_points(&notes, cache, offset, columns, parity, |p| {
                            visited.push(p)
                        });
                        assert_eq!(
                            visited.iter().map(scatter_bits).collect::<Vec<_>>(),
                            expected.iter().map(scatter_bits).collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn summaries_and_scatter_visits_have_no_heap_churn() {
    let notes = notes(1024, 4, false);
    crate::perf::assert_no_churn(|| {
        black_box(compute_note_timing_stats(black_box(&notes)));
        black_box(compute_window_counts_blue_ms(black_box(&notes), 10.0));
        visit_scatter_points(black_box(&notes), &[], 0, 4, None, |point| {
            black_box(point);
        });
    });
    crate::perf::assert_churn_budget(1, notes.len() * std::mem::size_of::<ScatterPoint>(), || {
        black_box(build_scatter_points(black_box(&notes), &[], 0, 4, None));
    });
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut old =
        || crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    let mut new =
        || crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        new();
        old();
    } else {
        old();
        new();
    }
}

#[test]
#[ignore = "manual release comparison; --ignored --test-threads=1 --nocapture"]
fn benchmark_row_traversal() {
    for size in [128, 8192] {
        // Gameplay records the old and new value at each life-change timestamp.
        let changes: Vec<_> = (0..size)
            .map(|i| {
                (
                    i as f32 / 60.0,
                    (i % 101) as f32 / 100.0,
                    ((i + 1) % 101) as f32 / 100.0,
                )
            })
            .collect();
        let mut old = Vec::with_capacity(size * 2);
        let mut new = Vec::with_capacity(size * 2);
        pair(
            &format!("life_changes_{size}"),
            128,
            size,
            || {
                old.clear();
                for &(t, before, after) in black_box(&changes) {
                    baseline::record_life_history(&mut old, t, before);
                    baseline::record_life_history(&mut old, t, after);
                }
                black_box(&old);
            },
            || {
                new.clear();
                for &(t, before, after) in black_box(&changes) {
                    record_life_change(&mut new, t, before, after);
                }
                black_box(&new);
            },
        );
        assert_eq!(history_bits(&new), history_bits(&old));
        for mode in ["plateau", "changing", "mixed", "same_time"] {
            let samples: Vec<_> = (0..size)
                .map(|i| {
                    let life = match mode {
                        "plateau" => 1.0,
                        "changing" | "same_time" => (i % 101) as f32 / 100.0,
                        _ => {
                            if i % 128 < 100 {
                                1.0
                            } else {
                                (i % 13) as f32 / 13.0
                            }
                        }
                    };
                    (
                        if mode == "same_time" {
                            (i / 4) as f32 / 60.0
                        } else {
                            i as f32 / 60.0
                        },
                        life,
                    )
                })
                .collect();
            let mut old = Vec::with_capacity(size);
            let mut new = Vec::with_capacity(size);
            pair(
                &format!("life_{mode}_{size}"),
                128,
                size,
                || {
                    old.clear();
                    for &(t, life) in black_box(&samples) {
                        baseline::record_life_history(&mut old, t, life);
                    }
                    black_box(&old);
                },
                || {
                    new.clear();
                    for &(t, life) in black_box(&samples) {
                        record_life_history(&mut new, t, life);
                    }
                    black_box(&new);
                },
            );
            assert_eq!(history_bits(&new), history_bits(&old));
        }
    }
    pair(
        "life_full_plateau",
        4096,
        1,
        || {
            let mut h = vec![(0.0, 1.0), (1.0, 1.0)];
            baseline::record_life_history(black_box(&mut h), black_box(2.0), black_box(1.0));
            h
        },
        || {
            let mut h = vec![(0.0, 1.0), (1.0, 1.0)];
            record_life_history(black_box(&mut h), black_box(2.0), black_box(1.0));
            h
        },
    );
    for (rows, chord, mixed) in [
        (0, 1, false),
        (1, 1, false),
        (128, 1, false),
        (8192, 1, false),
        (4096, 2, false),
        (2048, 4, false),
        (1024, 8, false),
        (2048, 4, true),
    ] {
        let mut notes = notes(rows, chord, mixed);
        if !mixed {
            for (i, note) in notes.iter_mut().enumerate() {
                let j = note.result.as_mut().unwrap();
                (j.grade, j.window) = match (i * 37) % 100 {
                    0..=79 => (JudgeGrade::Fantastic, Some(TimingWindow::W1)),
                    80..=92 => (JudgeGrade::Excellent, Some(TimingWindow::W2)),
                    93..=97 => (JudgeGrade::Great, Some(TimingWindow::W3)),
                    98 => (JudgeGrade::Decent, Some(TimingWindow::W4)),
                    _ => (JudgeGrade::Miss, None),
                };
            }
        }
        let suffix = format!("{rows}x{chord}_{}", if mixed { "mixed" } else { "judged" });
        let iterations = if rows > 128 { 64 } else { 512 };
        pair(
            &format!("stats_{suffix}"),
            iterations,
            notes.len().max(1),
            || baseline::compute_note_timing_stats(black_box(&notes)),
            || compute_note_timing_stats(black_box(&notes)),
        );
        pair(
            &format!("windows_{suffix}"),
            iterations,
            notes.len().max(1),
            || baseline::compute_window_counts_blue_ms(black_box(&notes), black_box(10.0)),
            || compute_window_counts_blue_ms(black_box(&notes), black_box(10.0)),
        );
        let times: Vec<_> = (0..notes.len()).map(|i| i as i64 * 125_000_000).collect();
        let parity: Vec<_> = (0..rows)
            .map(|i| {
                (
                    i * 48,
                    if i % 2 == 0 {
                        ScatterFoot::Left
                    } else {
                        ScatterFoot::Right
                    },
                )
            })
            .collect();
        pair(
            &format!("scatter_{suffix}"),
            iterations,
            notes.len().max(1),
            || {
                baseline::build_scatter_points(
                    black_box(&notes),
                    black_box(&times),
                    0,
                    8,
                    Some(black_box(&parity)),
                )
            },
            || {
                build_scatter_points(
                    black_box(&notes),
                    black_box(&times),
                    0,
                    8,
                    Some(black_box(&parity)),
                )
            },
        );
    }
}
