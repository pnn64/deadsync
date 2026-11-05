use deadsync_core::note::NoteType;
use deadsync_rules::judgment::{self as current, ExScoreData, ExScoreTotals, JudgeGrade, Judgment};
use deadsync_rules::note::{HoldData, HoldResult, MineResult, Note};
use deadsync_rules::timing::WindowCounts;
use std::hint::black_box;

#[path = "../../../tests/perf/core_alloc.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "support/scoring_original.rs"]
mod original;

fn values(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}

fn data(seed: &mut u64) -> ExScoreData {
    ExScoreData {
        counts: windows(seed),
        counts_10ms: windows(seed),
        holds_held: values(seed),
        holds_resolved: values(seed),
        rolls_held: values(seed),
        rolls_resolved: values(seed),
        mines_hit: values(seed),
        total_steps: values(seed),
        holds_total: values(seed),
        rolls_total: values(seed),
        mines_total: values(seed),
    }
}

fn windows(seed: &mut u64) -> WindowCounts {
    WindowCounts {
        w0: values(seed),
        w1: values(seed),
        w2: values(seed),
        w3: values(seed),
        w4: values(seed),
        w5: values(seed),
        miss: values(seed),
    }
}

fn assert_scores(data: &ExScoreData) {
    for (a, b) in [
        (
            current::ex_score_points(data),
            original::ex_score_points(data),
        ),
        (
            current::hard_ex_score_points(data),
            original::hard_ex_score_points(data),
        ),
        (
            current::ex_score_percent(data),
            original::ex_score_percent(data),
        ),
        (
            current::hard_ex_score_percent(data),
            original::hard_ex_score_percent(data),
        ),
        (
            current::ex_current_possible_ratio(data),
            original::ex_current_possible_ratio(data),
        ),
    ] {
        assert_eq!(a.to_bits(), b.to_bits(), "{data:?}");
    }
    for (a, b) in [
        (
            current::predictive_ex_score_percents(data),
            original::predictive_ex_score_percents(data),
        ),
        (
            current::predictive_hard_ex_score_percents(data),
            original::predictive_hard_ex_score_percents(data),
        ),
    ] {
        assert_eq!(
            [a.0.to_bits(), a.1.to_bits(), a.2.to_bits()],
            [b.0.to_bits(), b.1.to_bits(), b.2.to_bits()]
        );
    }
}

#[test]
fn weighted_scores_preserve_exact_bits_at_extremes_and_random_counts() {
    for n in [0, 1, 2, 127, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
        let all = WindowCounts {
            w0: n,
            w1: n,
            w2: n,
            w3: n,
            w4: n,
            w5: n,
            miss: n,
        };
        assert_scores(&ExScoreData {
            counts: all,
            counts_10ms: all,
            holds_held: n,
            holds_resolved: n,
            rolls_held: n,
            rolls_resolved: n,
            mines_hit: n,
            total_steps: n,
            holds_total: n,
            rolls_total: n,
            mines_total: n,
        });
        for field in 0..11 {
            let mut x = ExScoreData {
                total_steps: 1,
                mines_total: u32::MAX,
                ..ExScoreData::default()
            };
            match field {
                0 => x.counts.w0 = n,
                1 => x.counts.w1 = n,
                2 => x.counts.w2 = n,
                3 => x.counts.w3 = n,
                4 => x.holds_held = n,
                5 => x.rolls_held = n,
                6 => x.mines_hit = n,
                7 => x.counts_10ms.w0 = n,
                8 => x.holds_total = n,
                9 => x.rolls_total = n,
                _ => x.total_steps = n,
            }
            assert_scores(&x);
        }
    }
    let mut seed = 0x4913_328b_f213_9e37;
    for _ in 0..20_000 {
        assert_scores(&data(&mut seed));
    }
}

#[test]
fn blue_window_validation_and_counts_preserve_boundaries() {
    let mut bits = vec![
        0,
        1,
        0x007f_ffff,
        0x0080_0000,
        0x3f80_0000,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7f80_0001,
        0x7fc0_0000,
        0x8000_0000,
        0xffff_ffff,
    ];
    let mut seed = 0xf323_9039_9138_2831;
    for _ in 0..20_000 {
        bits.push(values(&mut seed));
    }
    for bits in bits {
        let ms = f32::from_bits(bits);
        assert_eq!(
            current::normalized_blue_window_ms(ms).to_bits(),
            original::normalized_blue_window_ms(ms).to_bits()
        );
        for grade in [
            JudgeGrade::Fantastic,
            JudgeGrade::Excellent,
            JudgeGrade::Great,
            JudgeGrade::Decent,
            JudgeGrade::WayOff,
            JudgeGrade::Miss,
        ] {
            for error in [ms, -ms, 0.0, 10.0, 15.0, f32::NAN] {
                let judgment = Judgment {
                    time_error_ms: error,
                    time_error_music_ns: 0,
                    grade,
                    window: None,
                    miss_because_held: false,
                };
                let mut a = WindowCounts {
                    w0: u32::MAX,
                    w1: u32::MAX - 1,
                    ..WindowCounts::default()
                };
                let mut b = a;
                current::add_judgment_to_window_counts(&mut a, &judgment, ms);
                original::add_judgment_to_window_counts(&mut b, &judgment, ms);
                assert_eq!(
                    [a.w0, a.w1, a.w2, a.w3, a.w4, a.w5, a.miss],
                    [b.w0, b.w1, b.w2, b.w3, b.w4, b.w5, b.miss]
                );
            }
        }
    }
}

fn notes(kind: &str, count: usize) -> Vec<Note> {
    (0..count)
        .map(|i| {
            let note_type = match i % 13 {
                0 => NoteType::Hold,
                1 => NoteType::Roll,
                2 => NoteType::Mine,
                3 => NoteType::Fake,
                _ => NoteType::Tap,
            };
            let grade = match i % 7 {
                0 => JudgeGrade::Miss,
                1 => JudgeGrade::Excellent,
                2 => JudgeGrade::Great,
                _ => JudgeGrade::Fantastic,
            };
            Note {
                participant: None,
                beat: i as f32,
                quantization_idx: 0,
                column: i % 4,
                note_type,
                row_index: i / 4,
                result: (i % 11 != 0).then_some(Judgment {
                    time_error_ms: (i % 31) as f32 - 15.0,
                    time_error_music_ns: i as i64,
                    grade,
                    window: None,
                    miss_because_held: false,
                }),
                early_result: None,
                hold: matches!(note_type, NoteType::Hold | NoteType::Roll).then_some(HoldData {
                    end_row_index: i / 4 + 8,
                    end_beat: i as f32 + 8.0,
                    result: Some(if i % 3 == 0 {
                        HoldResult::Held
                    } else {
                        HoldResult::LetGo
                    }),
                    life: 1.0,
                    let_go_started_at: None,
                    let_go_starting_life: 1.0,
                    last_held_row_index: 0,
                    last_held_beat: 0.0,
                }),
                mine_result: Some(if i % 3 == 0 {
                    MineResult::Hit
                } else {
                    MineResult::Avoided
                }),
                is_fake: kind == "filtered" && i % 2 == 0,
                can_be_judged: kind != "filtered" || i % 5 != 0,
            }
        })
        .collect()
}

#[test]
fn reconstruction_preserves_failures_filtering_missing_times_and_row_order() {
    for kind in ["mixed", "filtered"] {
        let mut notes = notes(kind, 137);
        for (i, note) in notes.iter_mut().enumerate() {
            if i % 5 == 0 {
                note.hold = None;
            }
            if let Some(hold) = &mut note.hold {
                hold.result = match i % 4 {
                    0 => None,
                    1 => Some(HoldResult::Held),
                    2 => Some(HoldResult::LetGo),
                    _ => Some(HoldResult::Missed),
                };
            }
            note.mine_result = match i % 3 {
                0 => None,
                1 => Some(MineResult::Hit),
                _ => Some(MineResult::Avoided),
            };
        }
        for shuffled in [false, true] {
            if shuffled {
                notes.reverse();
            }
            for len in [0, 1, 64, 137] {
                let times: Vec<_> = (0..len).map(|i| i as i64 - 32).collect();
                let ends: Vec<_> = times
                    .iter()
                    .enumerate()
                    .map(|(i, &t)| if i % 3 == 0 { i64::MIN } else { t + 8 })
                    .collect();
                for fail in [
                    None,
                    Some(i64::MIN),
                    Some(-1),
                    Some(0),
                    Some(31),
                    Some(i64::MAX),
                ] {
                    let totals = ExScoreTotals {
                        total_steps: 34,
                        holds_total: u32::MAX,
                        rolls_total: 12,
                        mines_total: 3,
                    };
                    let a = current::calculate_ex_score_percents_from_notes(
                        &notes, &times, &ends, totals, fail,
                    );
                    let b = original::calculate_ex_score_percents_from_notes(
                        &notes, &times, &ends, totals, fail,
                    );
                    assert_eq!(
                        [a.0.to_bits(), a.1.to_bits()],
                        [b.0.to_bits(), b.1.to_bits()]
                    );
                }
            }
        }
    }
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    for variant in [false, true] {
        let (_, counts) = alloc::measure(|| work(variant));
        assert_eq!(counts, alloc::Churn::default());
        println!(
            "{label}/{}: allocation {counts:?}",
            if variant { "current" } else { "original" }
        );
    }
    bench::compare(
        label,
        if label.starts_with("points/") || label.starts_with("windows/") {
            3000
        } else {
            500
        },
        work,
    );
}

#[test]
#[ignore = "release paired scoring CPU/allocation benchmarks"]
fn benchmark_scoring_kernels() {
    let mut seed = 0x9183_8247_1238_4567;
    for kind in ["typical", "extreme"] {
        let inputs: Vec<_> = (0..256)
            .map(|i| {
                if kind == "extreme" {
                    data(&mut seed)
                } else {
                    ExScoreData {
                        counts: WindowCounts {
                            w0: i * 13,
                            w1: i * 3,
                            w2: i * 2,
                            w3: i,
                            w4: 1,
                            w5: 1,
                            miss: 1,
                        },
                        counts_10ms: WindowCounts {
                            w0: i * 12,
                            ..WindowCounts::default()
                        },
                        holds_held: i,
                        rolls_held: i / 2,
                        holds_resolved: i + 1,
                        rolls_resolved: i / 2 + 1,
                        mines_hit: i % 7,
                        total_steps: 8192,
                        holds_total: 256,
                        rolls_total: 128,
                        mines_total: 256,
                    }
                }
            })
            .collect();
        for hard in [false, true] {
            compare(
                &format!("points/{}/{kind}/256", if hard { "hard" } else { "ex" }),
                |updated| {
                    let f: fn(&ExScoreData) -> f64 = black_box(match (updated, hard) {
                        (false, false) => original::ex_score_points,
                        (false, true) => original::hard_ex_score_points,
                        (true, false) => current::ex_score_points,
                        (true, true) => current::hard_ex_score_points,
                    });
                    for x in black_box(&inputs) {
                        black_box(f(black_box(x)));
                    }
                },
            );
        }
    }
    let windows: Vec<_> = (0..256)
        .map(|i| {
            if i % 5 == 0 {
                f32::from_bits(values(&mut seed))
            } else {
                10.0 + i as f32
            }
        })
        .collect();
    compare("windows/normalize/256", |updated| {
        let f: fn(f32) -> f32 = black_box(if updated {
            current::normalized_blue_window_ms
        } else {
            original::normalized_blue_window_ms
        });
        for &ms in black_box(&windows) {
            black_box(f(black_box(ms)));
        }
    });
    for kind in ["mixed", "filtered", "failed", "failed_early", "failed_late"] {
        let notes = notes(kind, 4096);
        let times: Vec<_> = (0..notes.len()).map(|i| i as i64).collect();
        let ends: Vec<_> = times.iter().map(|&t| t + 32).collect();
        let totals = ExScoreTotals {
            total_steps: 1024,
            holds_total: 300,
            rolls_total: 300,
            mines_total: 300,
        };
        let fail = match kind {
            "failed" => Some(1024),
            "failed_early" => Some(0),
            "failed_late" => Some(8192),
            _ => None,
        };
        compare(&format!("reconstruction/{kind}/4096"), |updated| {
            let f = black_box(if updated {
                current::calculate_ex_score_percents_from_notes
            } else {
                original::calculate_ex_score_percents_from_notes
            });
            black_box(f(
                black_box(&notes),
                black_box(&times),
                black_box(&ends),
                black_box(totals),
                black_box(fail),
            ));
        });
    }
}
