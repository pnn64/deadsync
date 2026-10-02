use super::*;
use crate::{HudLayoutYs, ZmodLayoutYs};
use deadsync_gameplay::{SongLuaPositionSpline, solve_song_lua_spline};
use std::hint::black_box;

#[allow(dead_code)]
mod baseline;

fn prepared<'a>(
    spline: SongLuaPositionSpline<'a>,
    song_beat: f32,
    zoom: f32,
) -> PreparedNotefield<'a, ()> {
    PreparedNotefield {
        frame_plan: NotefieldFramePlan {
            player_idx: 0,
            col_start: 0,
            num_cols: MAX_COLS,
            field_actor_reserve: 0,
            hud_actor_reserve: 0,
        },
        field: FieldLayout {
            playfield_center_x: 320.0,
            layout_center_x: 320.0,
            notefield_offset_x: 0.0,
            notefield_offset_y: 0.0,
            receptor_y_normal: 80.0,
            receptor_y_reverse: 400.0,
            receptor_y_centered: 240.0,
            centered_percent: 0.0,
            column_reverse_percent: [0.0; MAX_COLS],
            column_dirs: [1.0; MAX_COLS],
            column_receptor_ys: [80.0; MAX_COLS],
            hud_reverse: false,
            judgment_x: 320.0,
            combo_x: 320.0,
            error_bar_x: 320.0,
            hud_layout: HudLayoutYs {
                judgment_y: 200.0,
                error_bar_y: 240.0,
                error_bar_max_h: 16.0,
                zmod_layout: ZmodLayoutYs {
                    measure_counter_y: None,
                    subtractive_scoring_y: 200.0,
                    subtractive_scoring_addx: 0.0,
                    combo_y: 240.0,
                },
            },
        },
        field_zoom: zoom,
        scroll_speed: ScrollSpeedSetting::default(),
        arrow_effect_time_s: 0.0,
        current_time_s: 0.0,
        current_beat: song_beat,
        is_in_delay: false,
        mini: 0.0,
        draw_range: NoteDrawRange {
            after: -320.0,
            before: 960.0,
            zoom,
        },
        receptor_alphas: [1.0; MAX_COLS],
        blind_active: false,
        column_x_offsets: [0.0; MAX_COLS],
        column_position_splines: [spline; MAX_COLS],
        column_zoom_splines: [SongLuaPositionSpline::default(); MAX_COLS],
        column_spline_receptors: [spline.receptor(song_beat); MAX_COLS],
        spline_origin_y: 240.0,
        column_zooms: [1.0; MAX_COLS],
        column_rotations_deg: [0.0; MAX_COLS],
        notes: None,
    }
}

fn assert_vector(a: [f32; 3], b: [f32; 3]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!(
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
            "{a:?} != {b:?}"
        );
    }
}

#[test]
fn cached_receptors_and_combined_paths_match_parent_at_boundaries_and_float_edges() {
    let authored = [
        [-32.0, 0.0, -0.0],
        [64.0, 80.0, 20.0],
        [-48.0, 160.0, -30.0],
        [0.0, 240.0, 0.0],
    ];
    let solved = solve_song_lua_spline(&authored);
    let overflow = solve_song_lua_spline(&[[f32::MAX; 3], [-f32::MAX; 3], [f32::MAX; 3]]);
    let mut splines = vec![SongLuaPositionSpline::default()];
    for absolute in [false, true] {
        for subtract_song_beat in [false, true] {
            for coefficients in [&[][..], solved.as_slice(), overflow.as_slice()] {
                splines.push(SongLuaPositionSpline {
                    enabled: true,
                    absolute,
                    coefficients,
                    points: [authored[0], authored[1]],
                    beats_per_t: 0.5,
                    receptor_t: -0.25,
                    subtract_song_beat,
                    constant: false,
                });
            }
        }
    }
    let queries = [
        f32::NEG_INFINITY,
        -f32::MAX,
        -1.001,
        -1.0,
        -0.75,
        -0.0,
        0.0,
        0.25,
        0.999,
        1.0,
        1.001,
        2.999,
        3.0,
        30.0,
        f32::MAX,
        f32::INFINITY,
        f32::NAN,
    ];
    for spline in splines {
        for beats_per_t in [0.0, -0.0, 0.5, -2.0, f32::INFINITY, f32::NAN] {
            for song_beat in [-1.0, -0.0, 1.75, 16.0, f32::NAN] {
                for zoom in [-1.5, -0.0, 0.75, f32::INFINITY, f32::NAN] {
                    let prepared = prepared(
                        SongLuaPositionSpline {
                            beats_per_t,
                            ..spline
                        },
                        song_beat,
                        zoom,
                    );
                    let old = baseline::Baseline(&prepared);
                    for col in [0, MAX_COLS - 1] {
                        for beat in queries {
                            let base = [-0.0, 120.0, -30.0];
                            let (offset, derivative) = prepared.spline_offsets(col, beat);
                            let (old_offset, old_derivative) = old.spline_offsets(col, beat);
                            assert_vector(offset, old_offset);
                            assert_vector(derivative, old_derivative);
                            let (position, derivative) = prepared.spline_path(col, beat, base);
                            assert_vector(position, old.spline_position(col, beat, base));
                            assert_vector(derivative, old_derivative);
                            assert_vector(prepared.spline_position(col, beat, base), position);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn spline_sampling_has_no_rust_heap_churn() {
    let points = [[0.0, 0.0, 0.0], [32.0, 64.0, 16.0], [-16.0, 128.0, 0.0]];
    let solved = solve_song_lua_spline(&points);
    let spline = SongLuaPositionSpline {
        enabled: true,
        coefficients: &solved,
        constant: false,
        ..Default::default()
    };
    let prepared = prepared(spline, 0.125, 0.75);
    crate::perf::assert_no_churn(|| {
        for i in 0..256 {
            let col = i % MAX_COLS;
            let beat = i as f32 / 64.0;
            black_box(prepared.spline_offsets(col, beat));
            black_box(prepared.spline_path(col, beat, [120.0, beat * 64.0, 20.0]));
        }
    });
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_spline_sampling() {
    const SAMPLES: usize = 256;
    let points: Vec<_> = (0..32)
        .map(|i| [((i * 17) % 71) as f32, i as f32 * 64.0, -((i * 11) as f32)])
        .collect();
    let coefficients = solve_song_lua_spline(&points);
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for mode in ["disabled", "linear", "cubic_offset", "cubic_absolute"] {
        let spline = SongLuaPositionSpline {
            enabled: mode != "disabled",
            absolute: mode == "cubic_absolute",
            coefficients: if mode.starts_with("cubic") {
                &coefficients
            } else {
                &[]
            },
            constant: false,
            points: [points[0], points[1]],
            beats_per_t: 0.5,
            receptor_t: 0.25,
            subtract_song_beat: false,
        };
        let prepared = prepared(spline, 1.125, 0.75);
        let beats: Vec<_> = (0..SAMPLES)
            .map(|i| -0.25 + (i % 64) as f32 * 0.03125)
            .collect();
        macro_rules! measure_offsets {
            ($label:expr, $method:expr) => {{
                crate::perf::measure_sampled($label, 16_384, SAMPLES, || {
                    let prepared = black_box(&prepared);
                    for (i, &beat) in black_box(&beats).iter().enumerate() {
                        black_box(($method)(prepared, i % MAX_COLS, beat));
                    }
                });
            }};
        }
        macro_rules! measure_path {
            ($label:expr, $method:expr) => {{
                crate::perf::measure_sampled($label, 16_384, SAMPLES, || {
                    let prepared = black_box(&prepared);
                    for (i, &beat) in black_box(&beats).iter().enumerate() {
                        let col = i % MAX_COLS;
                        let base = [120.0, beat * 64.0, 20.0];
                        black_box(($method)(prepared, col, beat, base));
                    }
                });
            }};
        }
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("offsets_{mode}_{}", if old { "old" } else { "new" });
            if old {
                measure_offsets!(&label, |p: &PreparedNotefield<'_, ()>, col, beat| {
                    baseline::Baseline(p).spline_offsets(col, beat)
                });
            } else {
                measure_offsets!(&label, PreparedNotefield::spline_offsets);
            }
        }
        // The cached-only middle variant isolates the combined hold evaluator
        // from receptor caching. Each output is the full position/direction pair.
        for variant in if reverse {
            ["new", "cached", "old"]
        } else {
            ["old", "cached", "new"]
        } {
            let label = format!("hold_path_{mode}_{variant}");
            match variant {
                "old" => measure_path!(&label, |p: &PreparedNotefield<'_, ()>, col, beat, base| {
                    let old = baseline::Baseline(p);
                    let derivative = old.spline_offsets(col, beat).1;
                    (old.spline_position(col, beat, base), derivative)
                }),
                "cached" => {
                    measure_path!(&label, |p: &PreparedNotefield<'_, ()>, col, beat, base| {
                        let derivative = p.spline_offsets(col, beat).1;
                        (p.spline_position(col, beat, base), derivative)
                    })
                }
                _ => measure_path!(&label, PreparedNotefield::spline_path),
            }
        }
    }
}
