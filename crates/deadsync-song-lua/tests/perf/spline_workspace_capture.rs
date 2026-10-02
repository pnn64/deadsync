use super::*;
use std::hint::black_box;
use std::sync::Arc;

#[path = "spline_workspace_capture_baseline.rs"]
mod baseline;

fn spline(value: f32, size: usize) -> Option<deadsync_gameplay::SongLuaSplineData> {
    Some(deadsync_gameplay::SongLuaSplineData {
        coefficients: vec![[[value; 4]; 3]; size].into(),
        constant: false,
        beats_per_t: 1.0,
        receptor_t: 0.0,
        subtract_song_beat: true,
    })
}

fn detach_zoom(lua: &Lua, actors: &[Table]) {
    for actor in actors {
        let original: Table = actor.get("__songlua_pos_handler").unwrap();
        let handler = lua.create_table().unwrap();
        for pair in original.pairs::<Value, Value>() {
            let (key, value) = pair.unwrap();
            handler.set(key, value).unwrap();
        }
        let original: Table = original.get("__songlua_spline").unwrap();
        let spline = lua.create_table().unwrap();
        let size: usize = original.get("__songlua_spline_size").unwrap();
        spline.set("__songlua_spline_size", size).unwrap();
        let original: Table = original.get("__songlua_spline_points").unwrap();
        let points = lua.create_table().unwrap();
        for index in 1..=size {
            let original: Table = original.raw_get(index).unwrap();
            let xyz = [
                original.raw_get::<f32>(1).unwrap() + 3.0,
                original.raw_get(2).unwrap(),
                original.raw_get(3).unwrap(),
            ];
            points
                .raw_set(index, lua.create_sequence_from(xyz).unwrap())
                .unwrap();
        }
        spline.set("__songlua_spline_points", points).unwrap();
        handler.set("__songlua_spline", spline).unwrap();
        actor.set("__songlua_zoom_handler", handler).unwrap();
    }
}

fn change(actors: &[Table], step: usize, mode: &str) {
    super::spline_capture_perf::change(
        actors,
        step,
        mode.strip_suffix("_distinct").unwrap_or(mode),
    );
}

#[test]
fn lazy_reflexivity_preserves_nan_equality_and_invalidates_on_buffer_changes() {
    for value in [1.0, -0.0, f32::INFINITY, f32::NAN] {
        let previous = spline(value, 32);
        let mut next = previous.clone();
        let mut status = None;
        next.as_mut().unwrap().receptor_t = 2.0;
        assert!(!captured_spline_matches(&previous, &next, &mut status));
        assert_eq!(
            status, None,
            "metadata edits must not force a coefficient scan"
        );
        next.as_mut().unwrap().receptor_t = 0.0;
        for _ in 0..3 {
            assert_eq!(
                captured_spline_matches(&previous, &next, &mut status),
                previous == next
            );
            assert_eq!(status, Some(!value.is_nan()));
        }
        assert_eq!(
            captured_spline_reflexive(&next, Some(&previous), status),
            status
        );
        next.as_mut().unwrap().coefficients =
            previous.as_ref().unwrap().coefficients.to_vec().into();
        assert_eq!(
            captured_spline_matches(&previous, &next, &mut status),
            previous == next
        );
        assert_eq!(
            captured_spline_reflexive(&next, Some(&previous), status),
            None
        );
        assert_eq!(
            captured_spline_reflexive(&None, Some(&previous), status),
            None
        );
    }
    let mut previous = spline(f32::NAN, 4);
    let mut status = Some(false);
    for value in [1.0, f32::NAN, -0.0] {
        let next = spline(value, 4);
        status = captured_spline_reflexive(&next, Some(&previous), status);
        assert_eq!(status, None);
        previous = next;
        assert_eq!(
            captured_spline_matches(&previous, &previous, &mut status),
            !value.is_nan()
        );
    }
}

#[test]
fn complete_capture_matches_current_parent_through_edits_and_track_budget() {
    for size in [0, 1, 2, 32, 256] {
        for mode in [
            "steady",
            "metadata",
            "changing",
            "steady_distinct",
            "changing_distinct",
        ] {
            let (lua, actors) = super::spline_capture_perf::fixture(2, size);
            if mode.ends_with("_distinct") {
                detach_zoom(&lua, &actors);
            }
            let mut old = baseline::ColumnSplineCapture::default();
            let mut new = ColumnSplineCapture::default();
            for step in 0..20 {
                if size != 0 {
                    change(&actors, step, mode);
                }
                assert_eq!(
                    old.capture(&lua, step as f32),
                    new.capture(&lua, step as f32)
                );
                assert_eq!(old.bytes, new.bytes);
                for (key, lane) in &new.lanes {
                    super::spline_capture_perf::assert_frames(&lane.frames, &old.lanes[key].frames);
                }
            }
            let mut old_tracks = Vec::new();
            let mut new_tracks = Vec::new();
            old.finish(&mut old_tracks);
            new.finish(&mut new_tracks);
            assert_eq!(old_tracks, new_tracks);
        }
    }
    let (lua, _) = super::spline_capture_perf::fixture(1, 32);
    let mut old = baseline::ColumnSplineCapture::default();
    old.bytes = 128 * 1024 * 1024;
    let mut new = ColumnSplineCapture {
        bytes: 128 * 1024 * 1024,
        ..Default::default()
    };
    assert_eq!(old.capture(&lua, 0.0), new.capture(&lua, 0.0));
    assert_eq!(old.bytes, new.bytes);
}

#[test]
fn independently_read_position_and_zoom_share_equal_geometry() {
    let (lua, _) = super::spline_capture_perf::fixture(1, 256);
    let mut capture = ColumnSplineCapture::default();
    capture.capture(&lua, 0.0).unwrap();
    let frame = &capture.lanes[&(0, 0)].frames[0];
    assert!(Arc::ptr_eq(
        &frame.position.as_ref().unwrap().coefficients,
        &frame.zoom.as_ref().unwrap().coefficients
    ));
    assert_eq!(capture.lanes[&(0, 0)].reflexive, [None, None]);
    capture.capture(&lua, 1.0).unwrap();
    assert_eq!(capture.lanes[&(0, 0)].reflexive, [Some(true), Some(true)]);
}

#[test]
fn shared_overflowed_coefficients_remain_non_reflexive_during_capture() {
    let (lua, actors) = super::spline_capture_perf::fixture(1, 3);
    let points: Table = actors[0]
        .get::<Table>("__songlua_pos_handler")
        .unwrap()
        .get::<Table>("__songlua_spline")
        .unwrap()
        .get("__songlua_spline_points")
        .unwrap();
    for index in 1..=3 {
        let value = if index == 2 { -f32::MAX } else { f32::MAX };
        points
            .raw_set(index, lua.create_sequence_from([value; 3]).unwrap())
            .unwrap();
    }
    let mut old = baseline::ColumnSplineCapture::default();
    let mut new = ColumnSplineCapture::default();
    for step in 0..5 {
        old.capture(&lua, step as f32).unwrap();
        new.capture(&lua, step as f32).unwrap();
        assert_eq!(old.bytes, new.bytes);
        super::spline_capture_perf::assert_frames(
            &old.lanes[&(0, 0)].frames,
            &new.lanes[&(0, 0)].frames,
        );
    }
    assert_eq!(new.lanes[&(0, 0)].frames.len(), 5);
    assert_eq!(new.lanes[&(0, 0)].reflexive[0], Some(false));
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_spline_capture_workspace() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for size in [32, 256, 65_536] {
        let next = spline(1.0, size);
        let previous = spline(2.0, size);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!(
                "reflexivity_fresh_{size}_{}",
                if old { "old" } else { "new" }
            );
            crate::perf::measure_sampled(&label, 64, 128, || {
                for _ in 0..128 {
                    if old {
                        black_box(baseline::captured_spline_reflexive(
                            black_box(&next),
                            Some(black_box(&previous)),
                            true,
                        ));
                    } else {
                        black_box(captured_spline_reflexive(
                            black_box(&next),
                            Some(black_box(&previous)),
                            None,
                        ));
                    }
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
        (8, 256, "changing"),
        (8, 256, "steady_distinct"),
        (8, 256, "changing_distinct"),
        (8, 0, "disabled"),
    ] {
        let (lua, actors) = super::spline_capture_perf::fixture(lanes, size);
        if mode.ends_with("_distinct") {
            detach_zoom(&lua, &actors);
        }
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!(
                "capture_{lanes}x{size}_{mode}_{}",
                if old { "old" } else { "new" }
            );
            crate::perf::measure_sampled(&label, 8, lanes * 120, || {
                if old {
                    let mut capture = baseline::ColumnSplineCapture::default();
                    for step in 0..120 {
                        change(&actors, step, mode);
                        capture.capture(black_box(&lua), step as f32).unwrap();
                    }
                    black_box(capture);
                } else {
                    let mut capture = ColumnSplineCapture::default();
                    for step in 0..120 {
                        change(&actors, step, mode);
                        capture.capture(black_box(&lua), step as f32).unwrap();
                    }
                    black_box(capture);
                }
                lua.gc_collect().unwrap();
            });
        }
    }
}
