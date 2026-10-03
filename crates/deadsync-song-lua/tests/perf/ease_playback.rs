use super::*;
use std::hint::black_box;

#[path = "ease_playback/baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "../../../../tests/support/perf.rs"]
mod perf;

fn ease(start: f32, dense: bool) -> SongLuaOverlayEaseWindowRuntime {
    let from = crate::SongLuaOverlayStateDelta {
        x: Some(10.0),
        y: Some(20.0),
        sprite_state_index: Some(1),
        ..Default::default()
    };
    let to = crate::SongLuaOverlayStateDelta {
        x: Some(100.0),
        y: Some(-50.0),
        sprite_state_index: Some(5),
        zoom: dense.then_some(1.5),
        rot_z_deg: dense.then_some(45.0),
        diffuse: dense.then_some([0.2, 0.4, 0.6, 0.8]),
        size: dense.then_some([100.0, 80.0]),
        vertex_colors: dense.then_some([[0.3, 0.5, 0.7, 0.9]; 4]),
        texture_filtering: dense.then_some(false),
        depth_test: dense.then_some(true),
        ..Default::default()
    };
    SongLuaOverlayEaseWindowRuntime {
        overlay_index: 0,
        start_second: start,
        end_second: start + 1.0,
        sustain_end_second: start + 2.0,
        cutoff_second: None,
        from: SongLuaRuntimeOverlayStateDelta {
            overlap_mask: 0,
            delta: from,
        },
        to: SongLuaRuntimeOverlayStateDelta {
            overlap_mask: 0,
            delta: to,
        },
        easing: SongLuaEase::Linear,
        opt1: None,
        opt2: None,
    }
}

#[test]
fn in_place_eases_preserve_all_state_and_sprite_epochs_at_boundaries() {
    for dense in [false, true] {
        for cutoff in [None, Some(0.5), Some(1.5)] {
            let mut first = ease(0.0, dense);
            first.cutoff_second = cutoff;
            for duration in [1.0, 0.0, -1.0] {
                first.end_second = duration;
                let eases = [first.clone(), ease(3.0, !dense)];
                for range in [vec![], vec![0..0], vec![0..2]] {
                    for stretch_rect in [None, Some([-5.0, 6.0, 20.0, 40.0])] {
                        let initial = SongLuaOverlayState {
                            x: 7.0,
                            y: -3.0,
                            stretch_rect,
                            size: Some([20.0, 30.0]),
                            vertex_colors: Some([[0.1, 0.2, 0.3, 0.4]; 4]),
                            sprite_animation_epoch: Some(-2.0),
                            ..Default::default()
                        };
                        for now in [-1.0, 0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 3.25, 5.0, f32::NAN] {
                            let old = baseline::apply_song_lua_overlay_runtime_eases_for(
                                now, 0, &eases, &range, initial,
                            );
                            let mut new = initial;
                            apply_song_lua_overlay_runtime_eases_for(
                                now, 0, &eases, &range, &mut new,
                            );
                            // Debug includes every field and permits matching NaN states.
                            assert_eq!(format!("{new:?}"), format!("{old:?}"));
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release benchmark"]
fn ease_playback_benchmark() {
    eprintln!(
        "overlay state: {} bytes",
        std::mem::size_of::<SongLuaOverlayState>()
    );
    let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (name, dense, now, count) in [
        ("missing", false, 0.375, 0),
        ("dormant", false, -1.0, 1),
        ("sparse", false, 0.375, 1),
        ("dense", true, 0.375, 1),
        ("completed", true, 3.0, 1),
    ] {
        let eases = [ease(0.0, dense)];
        let ranges = vec![0..1; count];
        let initial = SongLuaOverlayState {
            size: Some([20.0, 30.0]),
            ..Default::default()
        };
        let mut states = vec![initial; 1024];
        for old in order {
            states.fill(initial);
            let label = format!("ease/{name}/{}", if old { "original" } else { "current" });
            if old {
                perf::measure_sampled(&label, 256, states.len(), || {
                    for state in black_box(&mut states) {
                        *state = baseline::apply_song_lua_overlay_runtime_eases_for(
                            black_box(now),
                            0,
                            black_box(&eases),
                            black_box(&ranges),
                            *state,
                        );
                        black_box(&*state);
                    }
                });
            } else {
                perf::measure_sampled(&label, 256, states.len(), || {
                    for state in black_box(&mut states) {
                        apply_song_lua_overlay_runtime_eases_for(
                            black_box(now),
                            0,
                            black_box(&eases),
                            black_box(&ranges),
                            state,
                        );
                        black_box(&*state);
                    }
                });
            }
        }
    }
}
