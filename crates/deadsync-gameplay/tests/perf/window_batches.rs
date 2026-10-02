//! Behavior and measured CPU/allocation controls for batched window work.
use super::*;
use std::hint::black_box;

include!("window_batches_baseline.rs");

fn hide_fixture(size: usize, count: usize, mode: &str) -> SongLuaNoteHideWindows {
    SongLuaNoteHideWindows::new(
        (0..count)
            .map(|i| {
                let start = if mode == "overlap" {
                    (i % 31) as f32 * 0.25
                } else {
                    (i * size / count.max(1)) as f32 * 0.25
                };
                SongLuaNoteHideWindowRuntime {
                    column: 0,
                    start_beat: start,
                    end_beat: if mode == "overlap" {
                        size as f32 * 0.25 - 0.5
                    } else {
                        start + 0.25
                    },
                }
            })
            .collect(),
    )
}

#[test]
fn hide_range_union_preserves_every_coefficient_and_sample_bit() {
    for size in [1, 2, 3, 64, 513, 4096] {
        for mode in ["overlap", "disjoint"] {
            let mut windows = hide_fixture(size, 257, mode).as_slice().to_vec();
            for (i, (start, end)) in [
                (-4.0, -2.0),
                (-0.0, 0.0),
                (0.125, 0.375),
                (8.0, -1.0),
                (f32::MIN, f32::MAX),
                (f32::NAN, 5.0),
                (0.0, f32::INFINITY),
                (f32::INFINITY, f32::MAX),
            ]
            .into_iter()
            .enumerate()
            {
                windows.push(SongLuaNoteHideWindowRuntime {
                    column: if i == 4 { MAX_COLS - 1 } else { i % 2 },
                    start_beat: start,
                    end_beat: end,
                });
            }
            windows.reverse();
            let source = SongLuaNoteHideWindows::new(windows);
            for spacing in [0.25, 1.0, f32::MIN_POSITIVE] {
                let mut old = source.clone();
                let mut new = source.clone();
                for column in [0, 1, MAX_COLS - 1] {
                    old_set_zoom_spline(&mut old, column, spacing, size);
                    new.set_zoom_spline(column, spacing, size);
                    for (a, b) in old.zoom_splines[column]
                        .coefficients
                        .iter()
                        .zip(&new.zoom_splines[column].coefficients)
                    {
                        assert_eq!(
                            a.map(f32::to_bits),
                            b.map(f32::to_bits),
                            "{size} {mode} {spacing}"
                        );
                    }
                    for i in -8..size as i32 * 2 + 8 {
                        let beat = i as f32 * spacing * 0.5;
                        assert_eq!(
                            old.zoom_offset(column, beat).to_bits(),
                            new.zoom_offset(column, beat).to_bits()
                        );
                    }
                }
            }
        }
    }
}

fn ease_fixture(size: usize, mode: &str) -> Vec<SongLuaEaseMaskWindow> {
    (0..size)
        .map(|i| {
            let start = match mode {
                "reversed" => (size - i) as f32 * 0.03125,
                "ties" => (i % 4) as f32 * 0.00025,
                _ => i as f32 * 0.03125,
            };
            SongLuaEaseMaskWindow {
                approach_speed: Some(1.0),
                start_second: start,
                end_second: start + 0.015625,
                sustain_end_second: start + 0.03125,
                target: match mode {
                    "mixed" => match i % 4 {
                        0 => SongLuaEaseMaskTarget::VisualDrunk,
                        1 => SongLuaEaseMaskTarget::ScrollReverse,
                        2 => SongLuaEaseMaskTarget::PlayerX,
                        _ => SongLuaEaseMaskTarget::VisualBumpyColumn(i % MAX_COLS),
                    },
                    _ => SongLuaEaseMaskTarget::VisualDrunk,
                },
                from: 0.0,
                to: 1.0,
                easing: SongLuaEase::Linear,
                opt1: None,
                opt2: None,
            }
        })
        .collect()
}

fn constant_fixture(count: usize, mode: &str) -> Vec<AttackMaskWindow> {
    (0..count)
        .map(|i| {
            let mut window = build_course_modifier_mask_window("50% drunk").unwrap();
            let key = (i * 101 + 17) % count.max(1);
            window.start_second = key as f32 * 0.125;
            window.end_second = window.start_second + 0.05 + (i % 11) as f32 * 0.125;
            if mode == "mixed" {
                window.clear_all = i % 7 == 0;
                window.visual.drunk = (i % 3 == 0).then_some(0.5);
                window.scroll.reverse = (i % 3 == 1).then_some(0.25);
                window.visual.bumpy_cols[i % MAX_COLS] = Some(1.0);
            }
            window
        })
        .collect()
}

fn assert_tail_bits(old: &[SongLuaEaseMaskWindow], new: &[SongLuaEaseMaskWindow]) {
    assert_eq!(old.len(), new.len());
    for (a, b) in old.iter().zip(new) {
        assert_eq!(
            a.sustain_end_second.to_bits(),
            b.sustain_end_second.to_bits()
        );
        assert_eq!(a.start_second.to_bits(), b.start_second.to_bits());
        assert_eq!(a.end_second.to_bits(), b.end_second.to_bits());
        assert_eq!(a.target, b.target);
    }
}

#[test]
fn indexed_constant_cutoffs_preserve_scalar_results_and_fallbacks() {
    for count in [0, 1, 7, 8, 64, 256, 257] {
        for size in [0, 1, 31, 32, 257, 1024] {
            for mode in ["plain", "mixed", "ties", "reversed"] {
                let input = ease_fixture(size, mode);
                let mut constants = constant_fixture(count, mode);
                if count > 8 {
                    for (i, (start, end)) in [
                        (-0.0, 0.0),
                        (0.0, 0.001),
                        (-1.0, f32::MAX),
                        (f32::NAN, 3.0),
                        (1.0, f32::INFINITY),
                        (2.0, -2.0),
                        (f32::MIN, -0.0),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        constants[i].start_second = start;
                        constants[i].end_second = end;
                    }
                }
                let mut old = input.clone();
                let mut new = input.clone();
                old_extend_ease_tails(&mut old, &constants);
                song_lua_extend_ease_tails(&mut new, &constants);
                assert_tail_bits(&old, &new);
                // Repeated extension and altered/nonfinite ends retain scalar behavior.
                for (i, window) in old.iter_mut().enumerate() {
                    let end = [
                        f32::NAN,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                        -0.0,
                        0.0,
                        -0.002,
                        f32::MAX,
                        f32::MIN,
                    ][i % 8];
                    window.end_second = end;
                    new[i].end_second = end;
                }
                old_extend_ease_tails(&mut old, &constants);
                song_lua_extend_ease_tails(&mut new, &constants);
                assert_tail_bits(&old, &new);
                if size > 0 {
                    old[0].start_second = f32::NAN;
                    new[0].start_second = f32::NAN;
                    old_extend_ease_tails(&mut old, &constants);
                    song_lua_extend_ease_tails(&mut new, &constants);
                    assert_tail_bits(&old, &new);
                }
            }
        }
    }
    // Isolate opposite signed zeros so another negative constant or next ease
    // cannot mask the original fold-order behavior of the indexed path.
    for reversed in [false, true] {
        let mut constants = constant_fixture(8, "plain");
        for (i, constant) in constants.iter_mut().enumerate() {
            constant.start_second = if i % 2 == 0 { -0.0 } else { 0.0 };
            constant.end_second = [0.0, 0.001, 0.002, 2.0][i % 4];
        }
        if reversed {
            constants.reverse();
        }
        for end in [-0.002, -0.001, -0.0, 0.0, 0.001, 0.002, 2.0] {
            let mut old = ease_fixture(64, "ties");
            for window in &mut old {
                window.start_second = -1.0;
                window.end_second = end;
                window.sustain_end_second = f32::MAX;
            }
            let mut new = old.clone();
            old_extend_ease_tails(&mut old, &constants);
            song_lua_extend_ease_tails(&mut new, &constants);
            assert_tail_bits(&old, &new);
        }
    }
}

#[derive(Clone, Copy)]
struct Window {
    start: f32,
    end: f32,
}
fn window_active(window: &Window, now: f32) -> bool {
    now >= window.start && now < window.end
}
fn window_fixture(size: usize, mode: &str) -> Vec<Window> {
    (0..size)
        .map(|i| {
            let key = match mode {
                "reverse" => size - 1 - i,
                "shuffled" => (i * 7919 + 101) % size,
                "ties" => i % 17,
                _ => i,
            };
            Window {
                start: key as f32 * 0.0001,
                end: 10.0 + (i % 5) as f32,
            }
        })
        .collect()
}
fn assert_index_equal(old: &ActiveWindowIndex, new: &ActiveWindowIndex) {
    assert_eq!(old.start_order, new.start_order);
    assert_eq!(old.active, new.active);
    assert_eq!(old.next_start, new.next_start);
    assert_eq!(
        old.next_start_second.to_bits(),
        new.next_start_second.to_bits()
    );
    assert_eq!(
        old.next_expiry_second.to_bits(),
        new.next_expiry_second.to_bits()
    );
    assert_eq!(
        old.last_now.map(f32::to_bits),
        new.last_now.map(f32::to_bits)
    );
    assert_eq!(old.stats, new.stats);
}

#[test]
fn activation_batches_preserve_order_expiry_seeks_stats_and_source_rebuilds() {
    for size in [0, 1, 8, 9, 32, 257, 8192] {
        for mode in ["ordered", "reverse", "shuffled", "ties"] {
            let mut windows = window_fixture(size, mode);
            for window in windows.iter_mut().step_by(17) {
                window.end = window.start + 0.0001;
            }
            let mut old = ActiveWindowIndex::new(&windows, |w| w.start);
            let mut new = old.clone();
            for now in [
                -1.0,
                0.0005,
                0.0007,
                0.25,
                2.0,
                10.0,
                11.0,
                14.0,
                -0.0,
                0.0,
                f32::NAN,
                0.5,
                f32::INFINITY,
                1.0,
            ] {
                old_update_active(
                    &mut old,
                    &windows,
                    now,
                    |w| w.start,
                    |w| w.end,
                    window_active,
                );
                new.update(&windows, now, |w| w.start, |w| w.end, window_active);
                assert_index_equal(&old, &new);
            }
            // A replacement source also contains expired and nonfinite entries.
            windows.push(Window {
                start: f32::NAN,
                end: f32::MAX,
            });
            windows.push(Window {
                start: -1.0,
                end: 0.25,
            });
            windows.push(Window {
                start: 0.0,
                end: f32::INFINITY,
            });
            for now in [-2.0, 0.5, 2.0, 20.0] {
                old_update_active(
                    &mut old,
                    &windows,
                    now,
                    |w| w.start,
                    |w| w.end,
                    window_active,
                );
                new.update(&windows, now, |w| w.start, |w| w.end, window_active);
                assert_index_equal(&old, &new);
            }
            old.reset_time();
            new.reset_time();
            old_update_active(
                &mut old,
                &windows,
                1.0,
                |w| w.start,
                |w| w.end,
                window_active,
            );
            new.update(&windows, 1.0, |w| w.start, |w| w.end, window_active);
            assert_index_equal(&old, &new);
        }
    }
}

#[test]
fn batched_work_keeps_heap_budgets_and_activation_has_no_churn() {
    let hides = hide_fixture(4096, 1024, "overlap");
    let mut new = hides.clone();
    perf::assert_churn_budget(1, 4096 * 16, || new.set_zoom_spline(0, 0.25, 4096));
    let constants = constant_fixture(256, "plain");
    let mut eases = ease_fixture(128, "plain");
    perf::assert_no_churn(|| song_lua_extend_ease_tails(&mut eases, &constants));
    let mut eases = ease_fixture(4096, "plain");
    perf::assert_no_churn(|| {
        song_lua_extend_ease_tails(&mut eases, &constants);
    });
    let mut old = ease_fixture(4096, "plain");
    perf::assert_reduced_churn(
        || old_extend_ease_tails(&mut old, &constants),
        || song_lua_extend_ease_tails(&mut eases, &constants),
    );
    eases.reverse();
    perf::assert_churn_budget(1, 4096 * std::mem::size_of::<usize>(), || {
        song_lua_extend_ease_tails(&mut eases, &constants);
    });
    let windows = window_fixture(8192, "shuffled");
    let mut index = ActiveWindowIndex::new(&windows, |w| w.start);
    index.update(&windows, -1.0, |w| w.start, |w| w.end, window_active);
    perf::assert_no_churn(|| {
        for now in [0.1, 0.5, 1.0, 11.0, 15.0, -0.5, 1.0] {
            index.update(&windows, now, |w| w.start, |w| w.end, window_active);
        }
    });
}

#[test]
#[ignore = "manual release benchmark; CPU timing and heap counts are separate"]
fn benchmark_window_batches() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (size, count, mode) in [
        (64, 8, "disjoint"),
        (4096, 64, "disjoint"),
        (4096, 1024, "overlap"),
        (65536, 256, "overlap"),
        (65536, 1024, "overlap"),
    ] {
        let source = hide_fixture(size, count, mode);
        let name = format!("hide_{size}_{count}_{mode}");
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled_with_setup(
                &format!("{name}_{}", if old { "old" } else { "new" }),
                if size >= 65536 { 32 } else { 128 },
                size,
                || source.clone(),
                |state| {
                    if old {
                        old_set_zoom_spline(state, 0, black_box(0.25), size);
                    } else {
                        state.set_zoom_spline(0, black_box(0.25), size);
                    }
                    // Count destruction of the owning output, but keep fixture
                    // cloning and destruction outside the measured operation.
                    black_box(std::mem::take(&mut state.zoom_splines[0].coefficients));
                },
            );
        }
    }
    for (size, count, mode) in [
        (16, 8, "plain"),
        (128, 0, "plain"),
        (1024, 1, "plain"),
        (1024, 64, "plain"),
        (4096, 256, "plain"),
        (1024, 64, "mixed"),
        (1024, 64, "reversed"),
        (1024, 257, "plain"),
    ] {
        let input = ease_fixture(size, mode);
        let constants = constant_fixture(count, mode);
        let name = format!("tails_{size}_{count}_{mode}");
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled_with_setup(
                &format!("{name}_{}", if old { "old" } else { "new" }),
                if size >= 4096 { 64 } else { 128 },
                size,
                || input.clone(),
                |out| {
                    if old {
                        old_extend_ease_tails(black_box(out), black_box(&constants));
                    } else {
                        song_lua_extend_ease_tails(black_box(out), black_box(&constants));
                    }
                },
            );
        }
    }
    for (size, mode, steps) in [
        (8, "shuffled", 1),
        (1024, "ordered", 1),
        (1024, "reverse", 1),
        (8192, "reverse", 1),
        (8192, "shuffled", 1),
        (1024, "shuffled", 1024),
        (1024, "idle", 128),
    ] {
        let windows = window_fixture(size, mode);
        let mut source = ActiveWindowIndex::new(&windows, |w| w.start);
        source.update(&windows, -1.0, |w| w.start, |w| w.end, window_active);
        let name = format!("active_{size}_{mode}_{steps}");
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled_with_setup(
                &format!("{name}_{}", if old { "old" } else { "new" }),
                if steps > 1 { 32 } else { 128 },
                if mode == "idle" { steps } else { size },
                || {
                    let mut index = source.clone();
                    // Vec::clone of an empty active set loses spare capacity.
                    // Restore the song-setup reservation outside measurements.
                    index.active.reserve(windows.len());
                    index
                },
                |index| {
                    for step in 1..=steps {
                        let now = if mode == "idle" {
                            -0.5
                        } else if steps == 1 {
                            1.0
                        } else {
                            step as f32 * 0.0001
                        };
                        if old {
                            old_update_active(
                                black_box(index),
                                &windows,
                                now,
                                |w| w.start,
                                |w| w.end,
                                window_active,
                            );
                        } else {
                            index.update(&windows, now, |w| w.start, |w| w.end, window_active);
                        }
                    }
                    black_box(&index.active);
                },
            );
        }
    }
}
