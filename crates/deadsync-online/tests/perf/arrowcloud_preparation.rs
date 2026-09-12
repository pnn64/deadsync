use super::*;
use crate::perf;
use deadsync_core::note::NoteType;
use deadsync_profile::{
    AccelEffectsMask, AppearanceEffectsMask, Perspective, ScrollOption, TurnOption,
    VisualEffectsMask,
};
use deadsync_rules::judgment::{JudgeGrade, Judgment};
use deadsync_rules::timing::{ScatterFoot, build_scatter_points};
use std::hint::black_box;

#[allow(dead_code)]
mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/arrowcloud_preparation/baseline.rs"
    ));
}

fn chart(rows: usize, width: usize, edge_cases: bool) -> (Vec<Note>, Vec<i64>) {
    let mut notes = Vec::new();
    let mut times = Vec::new();
    for row in 0..rows {
        for column in 0..width {
            let index = row * width + column;
            let grade = if index % 17 == 0 {
                JudgeGrade::Miss
            } else {
                JudgeGrade::Fantastic
            };
            let mut note = Note {
                beat: row as f32 / 4.0,
                row_index: row * 12,
                column,
                note_type: [
                    NoteType::Tap,
                    NoteType::Hold,
                    NoteType::Roll,
                    NoteType::Lift,
                ][index % 4],
                quantization_idx: (row % 9) as u8,
                result: Some(Judgment {
                    time_error_ms: (index as i32 % 83 - 41) as f32,
                    time_error_music_ns: (index as i64 % 83 - 41) * 1_000_000,
                    grade,
                    window: None,
                    miss_because_held: index % 2 == 0,
                }),
                early_result: None,
                hold: None,
                mine_result: None,
                is_fake: false,
                can_be_judged: true,
            };
            if edge_cases {
                match index % 19 {
                    0 => note.is_fake = true,
                    1 => note.can_be_judged = false,
                    2 => note.note_type = NoteType::Mine,
                    3 => note.result = None,
                    4 => note.result.as_mut().unwrap().time_error_ms = f32::NAN,
                    5 => note.result.as_mut().unwrap().time_error_ms = f32::INFINITY,
                    6 => note.result.as_mut().unwrap().time_error_ms = -0.0,
                    _ => {}
                }
            }
            notes.push(note);
            times.push((row as i64 - 16) * 125_000_000 + column as i64);
        }
    }
    (notes, times)
}

fn old_timing(
    notes: &[Note],
    times: &[i64],
    offset: usize,
    columns: usize,
    fail: Option<f32>,
) -> Vec<ArrowCloudTimingDatum> {
    let scatter = baseline::build_scatter_points(notes, times, offset, columns, None);
    baseline::timing_data_from_scatter(&scatter, fail)
}

fn timing_data_from_notes(
    notes: &[Note],
    times: &[i64],
    offset: usize,
    columns: usize,
    fail: Option<f32>,
) -> Vec<ArrowCloudTimingDatum> {
    let count = notes.chunk_by(|a, b| a.row_index == b.row_index).count();
    super::timing_data_from_notes(notes, times, offset, columns, fail, count)
}

fn submit_stats(notes: &[Note], times: &[i64], fail: Option<f32>) -> ArrowCloudSubmitStats {
    deadsync_score::arrowcloud_submit_stats_from_results(
        notes,
        times,
        &[],
        fail.map(|s| (f64::from(s) * 1e9) as i64),
    )
}

fn row_capacity(stats: ArrowCloudSubmitStats) -> usize {
    stats
        .judgment_counts
        .iter()
        .map(|&count| count as usize)
        .sum()
}

fn assert_timing_eq(old: &[ArrowCloudTimingDatum], new: &[ArrowCloudTimingDatum]) {
    assert_eq!(old.len(), new.len());
    for (a, b) in old.iter().zip(new) {
        assert_eq!(a.0.to_bits(), b.0.to_bits());
        match (&a.1, &b.1) {
            (ArrowCloudTimingOffset::Seconds(a), ArrowCloudTimingOffset::Seconds(b)) => {
                assert_eq!(a.to_bits(), b.to_bits())
            }
            (ArrowCloudTimingOffset::Miss(a), ArrowCloudTimingOffset::Miss(b)) => assert_eq!(a, b),
            _ => panic!("timing datum changed: {a:?} != {b:?}"),
        }
    }
}

#[test]
fn streamed_timing_matches_old_rows_cutoffs_and_missing_times() {
    for width in [1, 2, 4, 8] {
        let (notes, times) = chart(83, width, true);
        for length in [0, 1, times.len() / 2, times.len()] {
            for (offset, columns) in [(0, 4), (4, 4), (0, 0), (usize::MAX, usize::MAX)] {
                for fail in [
                    None,
                    Some(-3.0),
                    Some(0.0),
                    Some(1.125),
                    Some(f32::NAN),
                    Some(f32::INFINITY),
                    Some(f32::NEG_INFINITY),
                ] {
                    assert_timing_eq(
                        &old_timing(&notes, &times[..length], offset, columns, fail),
                        &timing_data_from_notes(&notes, &times[..length], offset, columns, fail),
                    );
                }
            }
        }
    }
    assert!(timing_data_from_notes(&[], &[], 0, 4, None).is_empty());
}

#[test]
fn timing_conversion_preserves_nonfinite_filtering() {
    let scatter: Vec<_> = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0, 1.0]
        .into_iter()
        .flat_map(|time_sec| {
            [
                None,
                Some(f32::NAN),
                Some(f32::INFINITY),
                Some(-0.0),
                Some(15.0),
            ]
            .into_iter()
            .map(move |offset_ms| ScatterPoint {
                time_sec,
                offset_ms,
                direction_code: 1,
                miss_because_held: false,
                row_index: 0,
                quantization_idx: 0,
                parity_foot: ScatterFoot::Unknown,
            })
        })
        .collect();
    for fail in [None, Some(-0.0), Some(1.0), Some(f32::NAN)] {
        assert_timing_eq(
            &baseline::timing_data_from_scatter(&scatter, fail),
            &timing_data_from_scatter(&scatter, fail),
        );
    }
}

#[test]
fn timing_capacity_is_bounded_by_rows_and_empty_results_do_not_allocate() {
    let (mut notes, times) = chart(512, 4, false);
    let result = timing_data_from_notes(&notes, &times, 0, 4, None);
    assert_eq!(result.len(), 512);
    assert_eq!(result.capacity(), 512);
    for hint in [0, 1, 7, 512, usize::MAX] {
        let new = super::timing_data_from_notes(&notes, &times, 0, 4, None, hint);
        assert_timing_eq(&result, &new);
        assert!(new.capacity() <= notes.len());
    }
    perf::assert_no_churn(|| {
        black_box(timing_data_from_notes(&notes, &times, 0, 4, Some(-3.0)));
    });
    for note in &mut notes {
        note.result = None;
    }
    perf::assert_no_churn(|| {
        black_box(timing_data_from_notes(&notes, &times, 0, 4, None));
    });
}

#[test]
fn scatter_visitor_preserves_all_fields_and_parity_joins() {
    let (notes, times) = chart(101, 8, true);
    let feet: Vec<_> = (0..101)
        .step_by(3)
        .map(|row| {
            (
                row * 12,
                [ScatterFoot::Left, ScatterFoot::Right, ScatterFoot::Both][row % 4 % 3],
            )
        })
        .collect();
    for foot_rows in [None, Some(&feet[..]), Some(&[][..])] {
        for (offset, columns) in [(0, 4), (4, 4), (0, usize::MAX)] {
            let old = baseline::build_scatter_points(&notes, &times, offset, columns, foot_rows);
            let new = build_scatter_points(&notes, &times, offset, columns, foot_rows);
            assert_eq!(old.len(), new.len());
            for (a, b) in old.iter().zip(&new) {
                assert_eq!(
                    (
                        a.time_sec.to_bits(),
                        a.offset_ms.map(f32::to_bits),
                        a.direction_code,
                        a.miss_because_held,
                        a.row_index,
                        a.quantization_idx,
                        a.parity_foot
                    ),
                    (
                        b.time_sec.to_bits(),
                        b.offset_ms.map(f32::to_bits),
                        b.direction_code,
                        b.miss_because_held,
                        b.row_index,
                        b.quantization_idx,
                        b.parity_foot
                    )
                );
            }
        }
    }
    perf::assert_no_churn(|| {
        deadsync_rules::timing::visit_scatter_points(&notes, &times, 0, 4, Some(&feet), |point| {
            black_box(point);
        });
    });
}

fn history(count: usize) -> Vec<(f32, f32)> {
    (0..count)
        .map(|i| (i as f32 * 0.125 - 4.0, (i % 117) as f32 / 100.0))
        .collect()
}

#[test]
fn prepared_lifebar_matches_old_float_bits() {
    let histories = [
        vec![],
        vec![(0.0, -0.0)],
        vec![(0.0, 0.5), (0.0, 0.9), (f32::EPSILON, -1.0), (1.0, 2.0)],
        vec![
            (f32::NEG_INFINITY, f32::NAN),
            (0.0, 0.7),
            (f32::INFINITY, 0.0),
        ],
        vec![(1.0, 0.3), (f32::NAN, 0.8), (-1.0, 0.4)],
        history(8192),
    ];
    for values in &histories {
        for start in [
            -8.0,
            0.0,
            5.0,
            2048.0,
            f32::NAN,
            f32::NEG_INFINITY,
            f32::INFINITY,
        ] {
            for (first, last) in [
                (-3.0, 100.0),
                (10.0, 4.0),
                (0.0, 0.0),
                (f32::NAN, f32::INFINITY),
            ] {
                for rate in [0.5, 1.0, 1.7, 0.0, -1.0, f32::NAN, f32::INFINITY] {
                    for count in [0, 1, 100] {
                        let old = baseline::lifebar_points(values, start, first, last, rate, count);
                        let new = lifebar_points(values, start, first, last, rate, count);
                        assert_eq!(old.len(), new.len());
                        for (a, b) in old.iter().zip(&new) {
                            assert_eq!(
                                (a.x.to_bits(), a.y.to_bits()),
                                (b.x.to_bits(), b.y.to_bits())
                            );
                        }
                    }
                }
            }
        }
    }
}

fn profile(mask: u16) -> Profile {
    let mut profile = Profile::default();
    profile.accel_effects_active_mask = AccelEffectsMask::from_bits_retain(mask as u8);
    profile.appearance_effects_active_mask = AppearanceEffectsMask::from_bits_retain(mask as u8);
    profile.visual_effects_active_mask = VisualEffectsMask::from_bits_retain(mask);
    profile
}

fn payload_input<'a>(
    notes: &'a [Note],
    times: &'a [i64],
    history: &'a [(f32, f32)],
    profile: &'a Profile,
    fail: Option<f32>,
) -> ArrowCloudGameplayPayloadInput<'a> {
    ArrowCloudGameplayPayloadInput {
        song_name: "Test Song".into(),
        artist: "Artist".into(),
        pack_group: " Test Pack ",
        music_length_seconds: 120.0,
        chart_hash: "deadbeefcafebabe".into(),
        difficulty: 12,
        stepartist: "Tester".into(),
        notes,
        note_times: times,
        col_offset: 0,
        cols_per_player: 4,
        fail_time_ns: fail.map(|s| (f64::from(s) * 1e9) as i64),
        submit_stats: ArrowCloudSubmitStats {
            judgment_counts: [10, 20, 30, 40, 50, 60],
            window_counts: WindowCounts::default(),
            holds_held: 1,
            mines_hit: 2,
            mines_avoided: 3,
            rolls_held: 4,
        },
        total_holds: 2,
        total_mines: 5,
        total_rolls: 6,
        max_nps: 20.0,
        measure_nps: &[0.0, 10.0, 20.0],
        measure_seconds: &[0.0, 5.0, 10.0],
        density_first_second: -3.0,
        density_last_second: 120.0,
        song_first_second: 0.0,
        life_history: history,
        profile,
        music_rate: 1.0,
        used_autoplay: false,
        is_failing: fail.is_some(),
        has_fail_time: fail.is_some(),
    }
}

#[test]
fn complete_payload_preserves_serialized_bytes() {
    let history = history(8192);
    for width in [1, 2, 4, 8] {
        let (notes, times) = chart(512, width, true);
        for mask in [0, 5, u16::MAX] {
            let profile = profile(mask);
            for fail in [None, Some(-3.0), Some(0.0), Some(23.125)] {
                let old = baseline::payload_from_gameplay_input(payload_input(
                    &notes, &times, &history, &profile, fail,
                ));
                let new = payload_from_gameplay_input(payload_input(
                    &notes, &times, &history, &profile, fail,
                ));
                assert_timing_eq(&old.timing_data, &new.timing_data);
                assert_eq!(
                    serde_json::to_vec(&old).unwrap(),
                    serde_json::to_vec(&new).unwrap()
                );
            }
        }
    }
}

#[test]
fn static_modifiers_preserve_json_for_every_mask_and_scalar_label() {
    let mut profile = profile(0);
    for mask in 0..=u16::MAX {
        profile.accel_effects_active_mask = AccelEffectsMask::from_bits_retain(mask as u8);
        profile.appearance_effects_active_mask =
            AppearanceEffectsMask::from_bits_retain(mask as u8);
        profile.visual_effects_active_mask = VisualEffectsMask::from_bits_retain(mask);
        let old = baseline::modifiers_from_profile(&profile);
        let new = modifiers_from_profile(&profile);
        assert_eq!(old.acceleration, new.acceleration);
        assert_eq!(old.appearance, new.appearance);
        assert_eq!(old.effect, new.effect);
        if mask < 1024 || mask == u16::MAX {
            assert_eq!(
                serde_json::to_vec(&old).unwrap(),
                serde_json::to_vec(&new).unwrap()
            );
        }
    }
    for turn in [
        TurnOption::None,
        TurnOption::Mirror,
        TurnOption::Left,
        TurnOption::Right,
        TurnOption::LRMirror,
        TurnOption::UDMirror,
        TurnOption::Shuffle,
        TurnOption::Blender,
        TurnOption::Random,
    ] {
        for perspective in [
            Perspective::Overhead,
            Perspective::Hallway,
            Perspective::Distant,
            Perspective::Incoming,
            Perspective::Space,
        ] {
            for scroll in [
                ScrollOption::Normal,
                ScrollOption::Reverse,
                ScrollOption::Split,
                ScrollOption::Alternate,
                ScrollOption::Cross,
                ScrollOption::Centered,
                ScrollOption::Reverse
                    .union(ScrollOption::Split)
                    .union(ScrollOption::Centered),
            ] {
                for mini in [-101, 0, 151] {
                    profile.turn_option = turn;
                    profile.perspective = perspective;
                    profile.scroll_option = scroll;
                    profile.mini_percent = mini;
                    profile.scroll_speed = ScrollSpeedSetting::XMod(2.345);
                    assert_eq!(
                        serde_json::to_vec(&baseline::modifiers_from_profile(&profile)).unwrap(),
                        serde_json::to_vec(&modifiers_from_profile(&profile)).unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn preparation_allocates_only_owned_outputs() {
    let (notes, times) = chart(512, 4, false);
    let history = history(8192);
    let default = profile(0);
    let full = profile(u16::MAX);
    let failed_capacity = row_capacity(submit_stats(&notes, &times, Some(3.0)));
    assert_eq!(failed_capacity, 41);
    perf::assert_churn_budget(
        1,
        failed_capacity * size_of::<ArrowCloudTimingDatum>(),
        || {
            let timing =
                super::timing_data_from_notes(&notes, &times, 0, 4, Some(3.0), failed_capacity);
            assert_eq!(timing.len(), failed_capacity);
            black_box(timing);
        },
    );
    perf::assert_churn_budget(1, notes.len() * size_of::<ArrowCloudTimingDatum>(), || {
        black_box(timing_data_from_notes(&notes, &times, 0, 4, None));
    });
    perf::assert_churn_budget(1, 100 * size_of::<ArrowCloudLifePoint>(), || {
        black_box(lifebar_points(&history, 0.0, -3.0, 120.0, 1.0, 100));
    });
    perf::assert_churn_budget(1, default.noteskin.as_str().len(), || {
        black_box(modifiers_from_profile(&default));
    });
    perf::assert_churn_budget(
        4,
        full.noteskin.as_str().len() + 20 * size_of::<&str>(),
        || {
            black_box(modifiers_from_profile(&full));
        },
    );
    perf::assert_no_churn(|| {
        black_box(timing_data_from_notes(&[], &[], 0, 4, None));
        black_box(lifebar_points(&[], 0.0, 0.0, 120.0, 1.0, 100));
    });
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut before = || perf::measure_sampled(&format!("{name}_old"), iterations, units, &mut old);
    let mut after = || perf::measure_sampled(&format!("{name}_new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        after();
        before();
    } else {
        before();
        after();
    }
}

#[test]
#[ignore = "manual release benchmark; seven samples, separate allocation accounting"]
fn arrowcloud_preparation_bench() {
    type ScatterFn =
        fn(&[Note], &[i64], usize, usize, Option<&[(usize, ScatterFoot)]>) -> Vec<ScatterPoint>;
    type LifeFn = fn(&[(f32, f32)], f32, f32, f32, f32, usize) -> Vec<ArrowCloudLifePoint>;
    type TimingFn = fn(&[Note], &[i64], usize, usize, Option<f32>) -> Vec<ArrowCloudTimingDatum>;
    type NewTimingFn =
        fn(&[Note], &[i64], usize, usize, Option<f32>, usize) -> Vec<ArrowCloudTimingDatum>;
    let old_timing = black_box(old_timing as TimingFn);
    let new_timing = black_box(super::timing_data_from_notes as NewTimingFn);
    for (name, rows, width, edge, fail) in [
        ("empty", 0, 1, false, None),
        ("short", 16, 1, false, None),
        ("taps", 8192, 1, false, None),
        ("chords", 4096, 4, false, None),
        ("failed", 8192, 2, false, Some(3.0)),
        ("mixed", 8192, 4, true, None),
        ("unjudged", 2048, 4, false, None),
        ("filtered", 2048, 4, false, Some(-3.0)),
    ] {
        let (mut notes, times) = chart(rows, width, edge);
        if name == "unjudged" {
            for note in &mut notes {
                note.result = None;
            }
        }
        let capacity = row_capacity(submit_stats(&notes, &times, fail));
        let iterations = if rows < 100 { 10_000 } else { 128 };
        pair(
            &format!("timing_{name}"),
            iterations,
            rows.max(1),
            || old_timing(black_box(&notes), black_box(&times), 0, 4, fail),
            || {
                new_timing(
                    black_box(&notes),
                    black_box(&times),
                    0,
                    4,
                    fail,
                    black_box(capacity),
                )
            },
        );
        pair(
            &format!("scatter_{name}"),
            iterations,
            rows.max(1),
            || {
                black_box(baseline::build_scatter_points as ScatterFn)(
                    black_box(&notes),
                    black_box(&times),
                    0,
                    4,
                    None,
                )
            },
            || {
                black_box(build_scatter_points as ScatterFn)(
                    black_box(&notes),
                    black_box(&times),
                    0,
                    4,
                    None,
                )
            },
        );
    }
    for (name, count, start, points) in [
        ("empty", 0, 0.0, 100),
        ("single", 1, 0.0, 100),
        ("short", 64, 0.0, 100),
        ("dense", 8192, 0.0, 100),
        ("trimmed", 8192, 512.0, 100),
        ("one_point", 8192, 0.0, 1),
    ] {
        let history = history(count);
        pair(
            &format!("life_{name}"),
            2000,
            points,
            || {
                black_box(baseline::lifebar_points as LifeFn)(
                    black_box(&history),
                    start,
                    -3.0,
                    120.0,
                    1.0,
                    points,
                )
            },
            || {
                black_box(lifebar_points as LifeFn)(
                    black_box(&history),
                    start,
                    -3.0,
                    120.0,
                    1.0,
                    points,
                )
            },
        );
    }
    for (name, mask) in [("default", 0), ("mixed", 5), ("full", u16::MAX)] {
        let profile = profile(mask);
        pair(
            &format!("modifiers_{name}"),
            10_000,
            1,
            || black_box(baseline::modifiers_from_profile as fn(_) -> _)(black_box(&profile)),
            || black_box(modifiers_from_profile as fn(_) -> _)(black_box(&profile)),
        );
        let mut buffer_old = Vec::with_capacity(4096);
        let mut buffer_new = Vec::with_capacity(4096);
        pair(
            &format!("modifiers_json_{name}"),
            4000,
            1,
            || {
                buffer_old.clear();
                serde_json::to_writer(
                    &mut buffer_old,
                    &baseline::modifiers_from_profile(black_box(&profile)),
                )
                .unwrap();
                black_box(&buffer_old);
            },
            || {
                buffer_new.clear();
                serde_json::to_writer(
                    &mut buffer_new,
                    &modifiers_from_profile(black_box(&profile)),
                )
                .unwrap();
                black_box(&buffer_new);
            },
        );
    }
    for (name, width, fail) in [
        ("taps", 1, None),
        ("chords", 4, None),
        ("failed", 2, Some(3.0)),
    ] {
        let (notes, times) = chart(8192, width, false);
        let history = history(8192);
        let profile = profile(5);
        // Runtime has already computed these statistics before payload creation.
        // Both implementations receive the same cached counts.
        let stats = submit_stats(&notes, &times, fail);
        let capacity = row_capacity(stats);
        pair(
            &format!("payload_{name}"),
            128,
            8192,
            || {
                let mut input = payload_input(
                    black_box(&notes),
                    black_box(&times),
                    black_box(&history),
                    black_box(&profile),
                    fail,
                );
                input.submit_stats = stats;
                baseline::payload_from_gameplay_input(input)
            },
            || {
                let mut input = payload_input(
                    black_box(&notes),
                    black_box(&times),
                    black_box(&history),
                    black_box(&profile),
                    fail,
                );
                input.submit_stats = stats;
                payload_from_gameplay_input(input)
            },
        );
        pair(
            &format!("combined_{name}"),
            128,
            8192,
            || {
                (
                    old_timing(black_box(&notes), black_box(&times), 0, 4, fail),
                    baseline::lifebar_points(black_box(&history), 0.0, -3.0, 120.0, 1.0, 100),
                    baseline::modifiers_from_profile(black_box(&profile)),
                )
            },
            || {
                (
                    new_timing(
                        black_box(&notes),
                        black_box(&times),
                        0,
                        4,
                        fail,
                        black_box(capacity),
                    ),
                    lifebar_points(black_box(&history), 0.0, -3.0, 120.0, 1.0, 100),
                    modifiers_from_profile(black_box(&profile)),
                )
            },
        );
    }
}
