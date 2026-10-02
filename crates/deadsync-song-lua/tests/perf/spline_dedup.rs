use super::*;
use std::hint::black_box;
use std::sync::Arc;

#[path = "spline_dedup_baseline.rs"]
mod baseline;

fn spline(value: f32) -> deadsync_gameplay::SongLuaSplineData {
    deadsync_gameplay::SongLuaSplineData {
        coefficients: vec![[[value; 4]; 3]; 256].into(),
        constant: false,
        beats_per_t: 1.0,
        receptor_t: 0.0,
        subtract_song_beat: true,
    }
}

#[test]
fn identity_shortcut_preserves_partial_equality_and_nan_non_reflexivity() {
    for value in [1.0, -0.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        let previous = Some(spline(value));
        let reflexive = captured_spline_reflexive(&previous, None, false);
        let mut next = previous.clone();
        assert_eq!(
            captured_spline_matches(&previous, &next, reflexive),
            previous == next
        );
        let original = Arc::clone(&next.as_ref().unwrap().coefficients);
        next.as_mut().unwrap().receptor_t = 3.0;
        assert_eq!(
            captured_spline_matches(&previous, &next, reflexive),
            previous == next
        );
        assert_eq!(
            captured_spline_reflexive(&next, Some(&previous), reflexive),
            reflexive
        );
        next.as_mut().unwrap().receptor_t = 0.0;
        next.as_mut().unwrap().coefficients = original.to_vec().into();
        assert_eq!(
            captured_spline_matches(&previous, &next, reflexive),
            previous == next
        );
    }
    assert!(captured_spline_matches(&None, &None, false));
    assert!(!captured_spline_matches(&None, &Some(spline(1.0)), true));
}

#[test]
fn capture_and_finished_tracks_match_previous_pass_during_edits() {
    for size in [0, 2, 32, 1024] {
        for mode in ["steady", "metadata", "changing"] {
            let (lua, actors) = super::spline_capture_perf::fixture(2, size);
            let mut old = baseline::ColumnSplineCapture::default();
            let mut new = ColumnSplineCapture::default();
            for step in 0..20 {
                if size != 0 {
                    super::spline_capture_perf::change(&actors, step, mode);
                }
                assert_eq!(
                    old.capture(&lua, step as f32),
                    new.capture(&lua, step as f32)
                );
                assert_eq!(old.bytes, new.bytes);
                for (key, lane) in &new.lanes {
                    super::spline_capture_perf::assert_frames(&lane.frames, &old.lanes[key]);
                }
            }
            let mut old_tracks = Vec::new();
            let mut new_tracks = Vec::new();
            old.finish(&mut old_tracks);
            new.finish(&mut new_tracks);
            assert_eq!(old_tracks, new_tracks);
        }
    }
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_spline_dedup() {
    // Fixtures and edits are shared with the previous pass's behavior tests.
    // The old capture is frozen at 0.5.1682; both variants use the new reader
    // and solver, isolating the identity comparison from the other two changes.
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for size in [32, 256, 65_536] {
        let mut data = spline(1.0);
        data.coefficients = vec![[[1.0; 4]; 3]; size].into();
        let previous = Some(data);
        let next = previous.clone();
        let reflexive = captured_spline_reflexive(&previous, None, false);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("dedup_equal_{size}_{}", if old { "old" } else { "new" });
            crate::perf::measure_sampled(&label, 64, 128, || {
                for _ in 0..128 {
                    black_box(if old {
                        black_box(&previous) == black_box(&next)
                    } else {
                        captured_spline_matches(
                            black_box(&previous),
                            black_box(&next),
                            black_box(reflexive),
                        )
                    });
                }
            });
        }
    }
    for (lanes, size, mode) in [
        (1, 2, "steady"),
        (8, 32, "steady"),
        (8, 256, "steady"),
        (1, 256, "metadata"),
        (1, 32, "changing"),
        (8, 0, "disabled"),
    ] {
        let (lua, actors) = super::spline_capture_perf::fixture(lanes, size);
        let mut old = baseline::ColumnSplineCapture::default();
        let mut new = ColumnSplineCapture::default();
        for step in 0..120 {
            super::spline_capture_perf::change(&actors, step, mode);
            assert_eq!(
                old.capture(&lua, step as f32),
                new.capture(&lua, step as f32)
            );
            assert_eq!(old.bytes, new.bytes);
            for (key, lane) in &new.lanes {
                super::spline_capture_perf::assert_frames(&lane.frames, &old.lanes[key]);
            }
        }
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!(
                "dedup_{lanes}x{size}_{mode}_{}",
                if old { "old" } else { "new" }
            );
            crate::perf::measure_sampled(&label, 16, lanes * 120, || {
                if old {
                    let mut capture = baseline::ColumnSplineCapture::default();
                    for step in 0..120 {
                        super::spline_capture_perf::change(&actors, step, mode);
                        capture.capture(black_box(&lua), step as f32).unwrap();
                    }
                    black_box(capture);
                } else {
                    let mut capture = ColumnSplineCapture::default();
                    for step in 0..120 {
                        super::spline_capture_perf::change(&actors, step, mode);
                        capture.capture(black_box(&lua), step as f32).unwrap();
                    }
                    black_box(capture);
                }
                lua.gc_collect().unwrap();
            });
        }
    }
}
