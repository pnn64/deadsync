use super::*;
use mlua::{Lua, Table, Value};
use std::hint::black_box;

#[path = "column_capture_baseline.rs"]
mod baseline;

const TARGETS: [SongLuaColumnTransformTarget; 4] = [
    SongLuaColumnTransformTarget::OffsetX,
    SongLuaColumnTransformTarget::OffsetY,
    SongLuaColumnTransformTarget::Zoom,
    SongLuaColumnTransformTarget::RotationZ,
];

fn params() -> SongLuaColumnOffsetBuildParams {
    SongLuaColumnOffsetBuildParams {
        unit: SongLuaTimeUnit::Beat,
        start: 2.0,
        limit: 0.25,
        span_mode: SongLuaSpanMode::Len,
        easing: None,
        sustain: Some(3.0),
        opt1: Some(-0.0),
        opt2: None,
    }
}

fn samples(columns: usize, active: bool) -> Vec<SongLuaColumnTransformSample> {
    (0..2)
        .flat_map(|player| {
            (0..columns).flat_map(move |column| {
                TARGETS.map(|target| SongLuaColumnTransformSample {
                    player,
                    column,
                    target,
                    value: target.baseline() + if active { (column + 1) as f32 } else { 0.0 },
                })
            })
        })
        .collect()
}

fn assert_windows(actual: &[SongLuaColumnOffsetWindow], expected: &[SongLuaColumnOffsetWindow]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert_eq!(
            (a.player, a.column, a.target, a.unit, a.span_mode, &a.easing),
            (b.player, b.column, b.target, b.unit, b.span_mode, &b.easing)
        );
        assert_eq!(
            [a.start, a.limit, a.from_y, a.to_y].map(f32::to_bits),
            [b.start, b.limit, b.from_y, b.to_y].map(f32::to_bits)
        );
        assert_eq!(
            [a.sustain, a.opt1, a.opt2].map(|v| v.map(f32::to_bits)),
            [b.sustain, b.opt1, b.opt2].map(|v| v.map(f32::to_bits))
        );
    }
}

fn next(rng: &mut u64) -> usize {
    *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*rng >> 32) as usize
}

#[test]
fn column_windows_match_parent_for_order_duplicates_missing_keys_and_float_edges() {
    let values = [
        -0.0,
        0.0,
        1.0,
        -1.0,
        f32::EPSILON,
        0.5,
        f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0xffc00001),
    ];
    for count in [0, 1, 16, 64, 128] {
        for seed in 1..=24 {
            let mut rng = seed;
            let mut make = || -> Vec<SongLuaColumnTransformSample> {
                (0..count)
                    .map(|_| SongLuaColumnTransformSample {
                        player: [0, 1, 7, usize::MAX][next(&mut rng) % 4],
                        column: next(&mut rng) % 8,
                        target: TARGETS[next(&mut rng) % 4],
                        value: values[next(&mut rng) % values.len()],
                    })
                    .collect()
            };
            let mut from = make();
            let mut to = make();
            for ordered in [false, true] {
                if ordered {
                    from.sort_by_key(column_transform_sample_key);
                    to.sort_by_key(column_transform_sample_key);
                }
                let mut p = params();
                p.easing = Some("linear".into());
                p.unit = SongLuaTimeUnit::Second;
                p.span_mode = SongLuaSpanMode::End;
                p.start = values[seed as usize % values.len()];
                p.opt2 = Some(f32::from_bits(0x7fc00003));
                let old = baseline::column_transform_windows_from_samples(&from, &to, p.clone());
                let new = column_transform_windows_from_samples(&from, &to, p.clone());
                assert_windows(&new, &old);
                let mut prefixed = baseline::column_transform_windows_from_samples(
                    &samples(1, true),
                    &[],
                    params(),
                );
                let prefix = prefixed.clone();
                append_column_transform_windows_from_samples(&mut prefixed, &from, &to, p);
                assert_windows(&prefixed[..prefix.len()], &prefix);
                assert_windows(&prefixed[prefix.len()..], &old);
            }
        }
    }
}

#[test]
fn column_windows_keep_first_values_zoom_baselines_and_reuse_output() {
    let from = [
        SongLuaColumnTransformSample {
            player: 0,
            column: 0,
            target: TARGETS[0],
            value: 4.0,
        },
        SongLuaColumnTransformSample {
            player: 0,
            column: 0,
            target: TARGETS[0],
            value: 9.0,
        },
        SongLuaColumnTransformSample {
            player: 0,
            column: 1,
            target: TARGETS[2],
            value: 2.0,
        },
    ];
    let to = [SongLuaColumnTransformSample {
        player: 1,
        column: 0,
        target: TARGETS[1],
        value: -3.0,
    }];
    let out = column_transform_windows_from_samples(&from, &to, params());
    assert_eq!(
        out.iter()
            .map(|w| (w.player, w.column, w.target, w.from_y, w.to_y))
            .collect::<Vec<_>>(),
        vec![
            (0, 0, TARGETS[0], 4.0, 0.0),
            (0, 1, TARGETS[2], 2.0, 1.0),
            (1, 0, TARGETS[1], 0.0, -3.0)
        ]
    );
    let active = samples(16, true);
    let neutral = samples(16, false);
    let mut buffer = Vec::with_capacity(active.len());
    crate::perf::assert_no_churn(|| {
        assert!(column_transform_windows_from_samples(&neutral, &neutral, params()).is_empty());
        for _ in 0..64 {
            buffer.clear();
            append_column_transform_windows_from_samples(&mut buffer, &active, &active, params());
        }
    });
    assert_eq!(buffer.len(), active.len());
}

fn actor(lua: &Lua, mode: &str, points: &[[f32; 3]]) -> Table {
    let actor = lua.create_table().unwrap();
    actor.set("__songlua_state_x", 12.0).unwrap();
    let handler = lua.create_table().unwrap();
    handler.set("__songlua_spline_mode", mode).unwrap();
    let spline = lua.create_table().unwrap();
    spline.set("__songlua_spline_size", points.len()).unwrap();
    let table = lua.create_table().unwrap();
    for (i, point) in points.iter().enumerate() {
        table
            .raw_set(i + 1, lua.create_sequence_from(*point).unwrap())
            .unwrap();
    }
    spline.set("__songlua_spline_points", table).unwrap();
    handler.set("__songlua_spline", spline).unwrap();
    for key in [
        "__songlua_pos_handler",
        "__songlua_zoom_handler",
        "__songlua_rot_handler",
    ] {
        actor.set(key, handler.clone()).unwrap();
    }
    actor
}

fn assert_float_result(actual: Result<Option<f32>, String>, expected: Result<Option<f32>, String>) {
    assert_eq!(
        actual.map(|v| v.map(f32::to_bits)),
        expected.map(|v| v.map(f32::to_bits))
    );
}

fn compare_actor(actor: &Table) {
    assert_float_result(
        note_column_pos_offset_y(actor),
        baseline::note_column_pos_offset_y(actor),
    );
}

#[test]
fn spline_queries_match_parent_for_modes_missing_data_and_numeric_edges() {
    let lua = Lua::new();
    let values = [
        -0.0,
        0.0,
        0.001,
        -0.001,
        0.001001,
        1.0,
        -1.0,
        f32::MAX,
        f32::NAN,
        f32::INFINITY,
    ];
    for mode in [
        "NoteColumnSplineMode_Disabled",
        "NoteColumnSplineMode_Offset",
        "NoteColumnSplineMode_Position",
        "notecolumnsplinemode_offset",
        "",
        "unknown",
    ] {
        for count in [0, 1, 2, 16] {
            for seed in 1..=16 {
                let mut rng = seed;
                let points: Vec<_> = (0..count)
                    .map(|_| std::array::from_fn(|_| values[next(&mut rng) % values.len()]))
                    .collect();
                compare_actor(&actor(&lua, mode, &points));
            }
        }
    }
    for size in [-1, 0, 1, 3] {
        let a = actor(&lua, "NoteColumnSplineMode_Offset", &[[0.0, -0.0, 0.0]; 2]);
        let handler: Table = a.get("__songlua_pos_handler").unwrap();
        let spline: Table = handler.get("__songlua_spline").unwrap();
        spline.set("__songlua_spline_size", size).unwrap();
        compare_actor(&a);
    }
    compare_actor(&lua.create_table().unwrap());
    let a = actor(&lua, "NoteColumnSplineMode_Offset", &[[0.0, 5.0, 0.0]; 4]);
    assert_eq!(note_column_pos_offset_y(&a), Ok(Some(5.0)));
}

#[test]
fn spline_reads_keep_conversion_errors_and_numeric_coercion() {
    let lua = Lua::new();
    let a = actor(&lua, "", &[[0.0, 0.0, 0.0]; 2]);
    let handler: Table = a.get("__songlua_pos_handler").unwrap();
    for value in [
        Value::Nil,
        Value::Integer(1),
        Value::Number(1.25),
        Value::Boolean(true),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string(b"bad\xffmode").unwrap()),
    ] {
        handler.set("__songlua_spline_mode", value).unwrap();
        compare_actor(&a);
    }
}

#[test]
fn streaming_preserves_later_lookup_errors_after_invalid_geometry() {
    let lua = Lua::new();
    for first in [
        [1.0, 2.0, 0.0],
        [0.0, f32::NAN, 0.0],
        [f32::INFINITY, 2.0, 0.0],
    ] {
        let a = actor(
            &lua,
            "NoteColumnSplineMode_Offset",
            &[first, [0.0, 2.0, 0.0]],
        );
        let handler: Table = a.get("__songlua_pos_handler").unwrap();
        let spline: Table = handler.get("__songlua_spline").unwrap();
        let points: Table = spline.get("__songlua_spline_points").unwrap();
        points.raw_set(2, "not a point table").unwrap();
        let expected = baseline::note_column_pos_offset_y(&a);
        if first.iter().all(|value| value.is_finite()) {
            assert!(expected.is_err());
        } else {
            assert_eq!(expected, Ok(None));
        }
        assert_eq!(note_column_pos_offset_y(&a), expected);
        // Missing coordinates still short-circuit before subsequent bad data.
        points.raw_set(1, lua.create_table().unwrap()).unwrap();
        assert_eq!(baseline::note_column_pos_offset_y(&a), Ok(None));
        assert_eq!(note_column_pos_offset_y(&a), Ok(None));
    }
}

#[test]
fn streamed_spline_reads_only_allocate_the_existing_mode_string() {
    let lua = Lua::new();
    let a = actor(&lua, "NoteColumnSplineMode_Offset", &[[0.0, 2.0, 0.0]; 256]);
    note_column_pos_offset_y(&a).unwrap();
    crate::perf::assert_churn_budget(16, 16 * "NoteColumnSplineMode_Offset".len(), || {
        for _ in 0..16 {
            assert_eq!(note_column_pos_offset_y(&a), Ok(Some(2.0)));
        }
    });
}

fn measure_pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
        crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    } else {
        crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
        crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    }
}

fn window_batch(
    old: bool,
    frames: usize,
    from: &[SongLuaColumnTransformSample],
) -> Vec<SongLuaColumnOffsetWindow> {
    let mut out = Vec::new();
    for frame in 0..frames {
        let p = SongLuaColumnOffsetBuildParams {
            start: frame as f32,
            ..params()
        };
        if old {
            out.extend(baseline::column_transform_windows_from_samples(
                from, from, p,
            ));
        } else {
            append_column_transform_windows_from_samples(&mut out, from, from, p);
        }
    }
    out
}

fn replay_context(segments: usize, rate: f32) -> SongLuaCompileContext {
    SongLuaCompileContext {
        song_music_rate: rate,
        song_display_bpms: [120.0, 180.0],
        song_timing_bpms: (0..segments)
            .map(|i| (i as f32 * 4.0, [60.0, 120.0, 240.0, 90.0][i % 4]))
            .collect(),
        ..SongLuaCompileContext::new(".", "Column capture benchmark")
    }
}

fn assert_replay(context: &SongLuaCompileContext, start: f32, end: f32) {
    let expected = baseline::update_function_replay_beats(context, start, end);
    let mut actual = crate::perframe::update_function_replay_beats(context, start, end).into_iter();
    for (beat, delta) in expected {
        let (actual_beat, actual_delta) = actual.next().expect("missing replay frame");
        assert_eq!(
            (actual_beat.to_bits(), actual_delta.to_bits()),
            (beat.to_bits(), delta.to_bits())
        );
    }
    assert!(actual.next().is_none());
    assert!(actual.next().is_none());
}

#[test]
fn replay_clock_matches_parent_at_bpm_boundaries_rates_and_fallbacks() {
    for segments in [0, 1, 8, 128] {
        for rate in [0.5, 1.0, 1.25, 2.0] {
            let context = replay_context(segments, rate);
            for (start, end) in [
                (0.0, 0.0),
                (1.0, -1.0),
                (-3.0, 3.0),
                (0.0, 0.001),
                (0.0, 4.0),
                (4.0 - f32::EPSILON, 8.0),
                (0.0, 128.0),
                (17.25, 55.125),
            ] {
                assert_replay(&context, start, end);
            }
        }
    }
    for bpms in [
        vec![(-4.0, 120.0), (0.0, 90.0), (0.0, 150.0), (4.0, 240.0)],
        vec![(0.0, 120.0), (8.0, 180.0), (4.0, 60.0)],
        vec![(0.0, f32::INFINITY)],
        vec![(f32::NAN, 120.0)],
    ] {
        let context = SongLuaCompileContext {
            song_timing_bpms: bpms,
            ..replay_context(0, 1.0)
        };
        assert_replay(&context, 0.0, 16.0);
    }
    // Invalid BPMs can expand a short beat interval to billions of frames in
    // the parent. Bound their time span while still exercising the fallback.
    for bpm in [f32::NAN, 0.0, -120.0] {
        let context = SongLuaCompileContext {
            song_timing_bpms: vec![(0.0, bpm)],
            ..replay_context(0, 1.0)
        };
        assert_replay(&context, 0.0, f32::EPSILON / 60.0);
    }
    let context = replay_context(1, 1.0);
    let frames = crate::perframe::update_function_replay_beats(&context, 0.0, 0.5);
    assert_eq!(frames.len(), 31);
    assert_eq!(frames[0], (0.0, 0.0));
    assert_eq!(frames.last().unwrap().0, 0.5);
}

#[test]
fn replay_clock_allocates_once_without_buffer_growth() {
    for segments in [1, 128] {
        let context = replay_context(segments, 1.25);
        for end in [0.0, 0.5, 512.0] {
            let expected = baseline::update_function_replay_beats(&context, 0.0, end);
            let mut count = 0;
            crate::perf::assert_churn_budget(
                1,
                expected.len() * std::mem::size_of::<(f64, f64)>(),
                || {
                    for point in crate::perframe::update_function_replay_beats(&context, 0.0, end) {
                        black_box(point);
                        count += 1;
                    }
                },
            );
            assert_eq!(count, expected.len());
        }
    }
}

fn consume_replay(points: impl IntoIterator<Item = (f64, f64)>) -> (usize, f64, f64) {
    points
        .into_iter()
        .fold((0, 0.0, 0.0), |(count, beats, elapsed), (beat, delta)| {
            let (beat, delta) = black_box((beat, delta));
            (count + 1, beats + beat, elapsed + delta)
        })
}

fn replay_bench() {
    for (segments, end, rate, ordered) in [
        (0, 0.0, 1.0, true),
        (0, 1.0, 1.0, true),
        (0, 512.0, 1.0, true),
        (1, 512.0, 1.0, true),
        (16, 512.0, 1.0, true),
        (128, 512.0, 1.0, true),
        (128, 512.0, 1.5, true),
        (16, 512.0, 1.0, false),
        (128, 16.0, 1.0, true),
    ] {
        let mut context = replay_context(segments, rate);
        if !ordered {
            context.song_timing_bpms.swap(1, 2);
        }
        assert_replay(&context, 0.0, end);
        let expected = consume_replay(baseline::update_function_replay_beats(&context, 0.0, end));
        let actual = consume_replay(crate::perframe::update_function_replay_beats(
            &context, 0.0, end,
        ));
        assert_eq!(actual, expected);
        measure_pair(
            &format!(
                "replay_{segments}_end_{}_rate_{}_ordered_{ordered}",
                end as usize,
                (rate * 100.0) as usize
            ),
            if end <= 1.0 { 1024 } else { 16 },
            expected.0,
            || {
                consume_replay(baseline::update_function_replay_beats(
                    black_box(&context),
                    0.0,
                    end,
                ))
            },
            || {
                consume_replay(crate::perframe::update_function_replay_beats(
                    black_box(&context),
                    0.0,
                    end,
                ))
            },
        );
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn column_capture_bench() {
    for (columns, active, ordered) in [
        (0, false, true),
        (1, true, true),
        (4, true, true),
        (16, true, true),
        (16, false, true),
        (16, true, false),
    ] {
        let mut from = samples(columns, active);
        if !ordered {
            from.reverse();
        }
        assert_windows(
            &column_transform_windows_from_samples(&from, &from, params()),
            &baseline::column_transform_windows_from_samples(&from, &from, params()),
        );
        measure_pair(
            &format!("windows_{columns}_active_{active}_ordered_{ordered}"),
            512,
            from.len().max(1),
            || baseline::column_transform_windows_from_samples(black_box(&from), &from, params()),
            || column_transform_windows_from_samples(black_box(&from), &from, params()),
        );
    }
    for (columns, frames, active) in [(4, 600, true), (16, 600, true), (16, 600, false)] {
        let from = samples(columns, active);
        assert_windows(
            &window_batch(false, frames, &from),
            &window_batch(true, frames, &from),
        );
        measure_pair(
            &format!("window_batch_{columns}x{frames}_active_{active}"),
            8,
            frames,
            || window_batch(true, frames, black_box(&from)),
            || window_batch(false, frames, black_box(&from)),
        );
    }
    let lua = Lua::new();
    for count in [0, 1, 16, 256, 4096] {
        let a = actor(
            &lua,
            "NoteColumnSplineMode_Offset",
            &vec![[0.0, 2.0, 0.0]; count],
        );
        compare_actor(&a);
        lua.gc_stop();
        let iterations = if count >= 256 { 32 } else { 512 };
        measure_pair(
            &format!("spline_final_{count}"),
            iterations,
            count.max(1),
            || baseline::note_column_pos_offset_y(black_box(&a)).unwrap(),
            || note_column_pos_offset_y(black_box(&a)).unwrap(),
        );
        lua.gc_restart();
        lua.gc_collect().unwrap();
    }

    replay_bench();
}
