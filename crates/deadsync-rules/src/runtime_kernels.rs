use super::*;
use crate::note::{HoldData, PlayerTotals, recompute_player_totals};
use crate::scroll::ScrollSpeedSetting;
use std::hint::black_box;

#[path = "runtime_kernels_original.rs"]
mod original;

fn note(row: usize, column: usize, kind: NoteType, end: Option<usize>) -> Note {
    Note {
        participant: None,
        beat: row as f32,
        quantization_idx: 0,
        column,
        note_type: kind,
        row_index: row,
        result: None,
        early_result: None,
        hold: end.map(|end_row_index| HoldData {
            end_row_index,
            end_beat: end_row_index as f32,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 0.0,
            last_held_row_index: row,
            last_held_beat: row as f32,
        }),
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

fn chart(kind: &str, rows: usize) -> Vec<Note> {
    let mut notes = Vec::new();
    for row in 0..rows {
        if kind == "dense" && row == 0 {
            for col in 0..MAX_COLS {
                notes.push(note(row, col, NoteType::Hold, Some(rows)));
            }
        } else if matches!(kind, "holds" | "wide") && row % 64 == 0 {
            for col in 0..if kind == "wide" { MAX_COLS } else { 1 } {
                notes.push(note(row, col, NoteType::Hold, Some(row + 32)));
            }
        } else {
            notes.push(note(row, 1 + row % 3, NoteType::Tap, None));
        }
        if row % 7 == 0 {
            notes.push(note(row, 6, NoteType::Mine, None));
        }
    }
    notes
}

fn bits(stats: TimingStats) -> [u32; 4] {
    [
        stats.mean_ms,
        stats.mean_abs_ms,
        stats.stddev_ms,
        stats.max_abs_ms,
    ]
    // Arithmetic NaN payloads may differ even for unchanged source builds.
    .map(|value| {
        if value.is_nan() {
            f32::NAN.to_bits()
        } else {
            value.to_bits()
        }
    })
}

#[test]
fn runtime_totals_match_for_sparse_dense_sentinel_and_out_of_domain_holds() {
    for end in [None, Some(0), Some(4), Some(7), Some(usize::MAX)] {
        let mut notes = vec![
            note(0, 0, NoteType::Hold, Some(4)),
            note(4, 0, NoteType::Hold, end),
        ];
        for row in [5, 7, 8] {
            notes.push(note(row, 1, NoteType::Tap, None));
            notes.push(note(row, 2, NoteType::Tap, None));
        }
        assert_eq!(
            recompute_player_totals(&notes, (0, notes.len())),
            original::recompute_player_totals(&notes, (0, notes.len()))
        );
    }
    for kind in ["tap", "holds", "wide", "dense"] {
        let notes = chart(kind, 1024);
        for range in [
            (0, notes.len()),
            (3, notes.len() / 2),
            (0, 0),
            (usize::MAX, 0),
        ] {
            assert_eq!(
                recompute_player_totals(&notes, range),
                original::recompute_player_totals(&notes, range)
            );
        }
    }
    let mut notes = vec![
        note(0, 0, NoteType::Hold, Some(usize::MAX)),
        note(1, 0, NoteType::Tap, None),
    ];
    for row in 2..256 {
        for col in [0, 3, MAX_COLS - 1, MAX_COLS, 31, usize::MAX] {
            let mut n = note(
                row,
                col,
                if row % 16 == 0 {
                    NoteType::Roll
                } else {
                    NoteType::Tap
                },
                (row % 16 == 0).then_some(row + 7),
            );
            n.can_be_judged = row % 5 != 0;
            n.is_fake = row % 3 == 0;
            notes.push(n);
        }
    }
    notes.push(note(usize::MAX, 0, NoteType::Tap, None));
    assert_eq!(
        recompute_player_totals(&notes, (0, notes.len())),
        original::recompute_player_totals(&notes, (0, notes.len()))
    );
}

#[test]
fn runtime_snapshots_match_bits_for_all_lengths_and_special_offsets() {
    let mut seed = 0x1234_5678_9abc_def0u64;
    for trace in 0..1000 {
        let mut current = LiveTimingStats::default();
        let mut old = current;
        for i in 0..256 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let error = if trace % 3 == 0 {
                [
                    0.0,
                    -0.0,
                    12.5,
                    -180.5,
                    f32::from_bits(1),
                    f32::NAN,
                    f32::from_bits(0xffc0_1234),
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                ][i % 9]
            } else if trace % 3 == 1 && i % 65 == 0 {
                1000.0
            } else {
                f32::from_bits(seed as u32)
            };
            let judgment = Judgment {
                time_error_ms: error,
                time_error_music_ns: i as i64,
                grade: if i % 17 == 0 {
                    JudgeGrade::Miss
                } else {
                    JudgeGrade::Fantastic
                },
                window: None,
                miss_because_held: false,
            };
            record_live_timing_stats(&mut current, &judgment);
            original::record_live_timing_stats(&mut old, &judgment);
            let actual = live_timing_stats_snapshot(&current);
            let expected = original::live_timing_stats_snapshot(&old);
            assert_eq!(
                bits(actual.recent),
                bits(expected.recent),
                "trace={trace} i={i}"
            );
            assert_eq!(bits(actual.all), bits(expected.all));
            if i == 255 {
                assert_eq!(format!("{current:?}"), format!("{old:?}"));
                assert_eq!(format!("{current:#?}"), format!("{old:#?}"));
            }
        }
    }
}

#[test]
fn runtime_scroll_pixels_match_float_bits_for_all_mods_and_special_values() {
    let values = [
        0.0,
        -0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        60.0,
        180.0,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    let check = |value, bpm, reference, rate| {
        for speed in [
            ScrollSpeedSetting::CMod(value),
            ScrollSpeedSetting::XMod(value),
            ScrollSpeedSetting::MMod(value),
        ] {
            assert_eq!(
                speed.pixels_per_second(bpm, reference, rate).to_bits(),
                original::pixels_per_second(speed, bpm, reference, rate).to_bits()
            );
        }
    };
    for value in values {
        for bpm in values {
            for reference in values {
                check(value, bpm, reference, 1.5);
            }
        }
    }
    let mut seed = 0x1234_5678_9abc_def0u64;
    for _ in 0..100_000 {
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            f32::from_bits(seed as u32)
        };
        check(next(), next(), next(), next());
    }
}

fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    let (_, old) = crate::perf_alloc::measure(|| work(false));
    let (_, new) = crate::perf_alloc::measure(|| work(true));
    assert_eq!(old, new);
    assert_eq!(new, crate::perf_alloc::Churn::default());
    println!("{label}: allocation {new:?}");
    crate::perf_bench::compare(label, iterations * 5, work);
}

#[test]
#[ignore = "release paired CPU and allocation benchmark"]
fn benchmark_rules_runtime() {
    println!(
        "runtime/state_bytes {}",
        std::mem::size_of::<LiveTimingStats>()
    );
    println!("runtime/default_debug {:?}", LiveTimingStats::default());
    for snapshots in [0, 1, 4] {
        let mut states = [LiveTimingStats::default(); 2];
        let judgments: Vec<_> = (0..256)
            .map(|i| Judgment {
                time_error_ms: if i % 65 == 0 {
                    180.0
                } else {
                    (i as f32 % 40.0) - 20.0
                },
                time_error_music_ns: i as i64,
                grade: JudgeGrade::Fantastic,
                window: None,
                miss_because_held: false,
            })
            .collect();
        compare(
            &format!("runtime/record/{snapshots}snapshots/256"),
            200,
            |current| {
                let record: fn(&mut LiveTimingStats, &Judgment) = black_box(if current {
                    record_live_timing_stats
                } else {
                    original::record_live_timing_stats
                });
                let snapshot: fn(&LiveTimingStats) -> LiveTimingSnapshot = black_box(if current {
                    live_timing_stats_snapshot
                } else {
                    original::live_timing_stats_snapshot
                });
                let state = &mut states[usize::from(current)];
                for judgment in black_box(&judgments) {
                    record(black_box(state), black_box(judgment));
                    for _ in 0..snapshots {
                        black_box(snapshot(black_box(state)));
                    }
                }
            },
        );
    }
    for kind in ["tap", "holds", "wide", "dense"] {
        let notes = chart(kind, 4096);
        compare(&format!("runtime/totals/{kind}/4096rows"), 100, |current| {
            let count: fn(&[Note], (usize, usize)) -> PlayerTotals = black_box(if current {
                recompute_player_totals
            } else {
                original::recompute_player_totals
            });
            black_box(count(black_box(&notes), black_box((0, notes.len()))));
        });
    }
    for (kind, len) in [
        ("empty", 0),
        ("partial", 8),
        ("full", 1024),
        ("nonfinite", 1024),
    ] {
        let mut state = LiveTimingStats::default();
        for i in 0..len {
            let error = if kind == "nonfinite" && i % 5 == 0 {
                f32::NAN
            } else {
                (i as f32 - 32.0) * 1.5
            };
            record_live_timing_stats(
                &mut state,
                &Judgment {
                    time_error_ms: error,
                    time_error_music_ns: i as i64,
                    grade: JudgeGrade::Fantastic,
                    window: None,
                    miss_because_held: false,
                },
            );
        }
        compare(&format!("runtime/snapshot/{kind}/256"), 500, |current| {
            let snapshot: fn(&LiveTimingStats) -> LiveTimingSnapshot = black_box(if current {
                live_timing_stats_snapshot
            } else {
                original::live_timing_stats_snapshot
            });
            for _ in 0..256 {
                black_box(snapshot(black_box(&state)));
            }
        });
    }
    for kind in ["cmod", "mixed", "invalid"] {
        let inputs: Vec<_> = (0..256)
            .map(|i| {
                let value = if kind == "invalid" {
                    [f32::NAN, f32::INFINITY, -1.0, 0.0][i % 4]
                } else {
                    1.0 + (i % 4) as f32 * 0.1
                };
                let speed = if kind == "cmod" {
                    ScrollSpeedSetting::CMod(600.0 + i as f32)
                } else if i % 2 == 0 {
                    ScrollSpeedSetting::XMod(value)
                } else {
                    ScrollSpeedSetting::MMod(value * 600.0)
                };
                (speed, 180.0, 120.0, 1.5)
            })
            .collect();
        compare(&format!("runtime/pixels/{kind}/256"), 1000, |current| {
            let pixels: fn(ScrollSpeedSetting, f32, f32, f32) -> f32 = black_box(if current {
                ScrollSpeedSetting::pixels_per_second
            } else {
                original::pixels_per_second
            });
            for &(setting, bpm, reference, rate) in black_box(&inputs) {
                black_box(pixels(
                    black_box(setting),
                    black_box(bpm),
                    black_box(reference),
                    black_box(rate),
                ));
            }
        });
    }
}
