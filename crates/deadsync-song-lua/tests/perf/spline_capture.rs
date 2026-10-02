use super::ColumnSplineCapture;
use deadsync_gameplay::{SongLuaColumnSplineFrame, SongLuaSplineData};
use mlua::{Lua, Table};
use std::hint::black_box;
use std::sync::Arc;

#[path = "spline_capture_baseline.rs"]
mod baseline;

pub(crate) fn fixture(lanes: usize, size: usize) -> (Lua, Vec<Table>) {
    let lua = Lua::new();
    let columns = lua.create_table().unwrap();
    let mut actors = Vec::new();
    for lane in 0..lanes {
        let column = lua.create_table().unwrap();
        column.set("__songlua_player_index", 0).unwrap();
        column.set("__songlua_column_index", lane).unwrap();
        let handler = lua.create_table().unwrap();
        handler
            .set("__songlua_spline_mode", "NoteColumnSplineMode_Position")
            .unwrap();
        handler.set("__songlua_beats_per_t", 0.5).unwrap();
        handler.set("__songlua_receptor_t", 0.25).unwrap();
        handler.set("__songlua_subtract_song_beat", true).unwrap();
        let spline = lua.create_table().unwrap();
        spline.set("__songlua_spline_size", size).unwrap();
        let points = lua.create_table().unwrap();
        for i in 0..size {
            points
                .raw_set(
                    i + 1,
                    lua.create_sequence_from([
                        ((i * 17 + lane) % 71) as f32,
                        i as f32 * 64.0,
                        -((i * 11) as f32),
                    ])
                    .unwrap(),
                )
                .unwrap();
        }
        spline.set("__songlua_spline_points", points).unwrap();
        handler.set("__songlua_spline", spline).unwrap();
        column
            .set("__songlua_pos_handler", handler.clone())
            .unwrap();
        column.set("__songlua_zoom_handler", handler).unwrap();
        columns.raw_set(lane + 1, column.clone()).unwrap();
        actors.push(column);
    }
    let field = lua.create_table().unwrap();
    field.set("__songlua_note_columns", columns).unwrap();
    let children = lua.create_table().unwrap();
    children.set("NoteField", field).unwrap();
    let player = crate::lua_util::create_dummy_actor(&lua, "PlayerActor", |_, _| Ok(())).unwrap();
    player.set("__songlua_children", children).unwrap();
    lua.globals()
        .set("__songlua_top_screen_player_1", player)
        .unwrap();
    (lua, actors)
}

fn handler(column: &Table) -> Table {
    column.get("__songlua_pos_handler").unwrap()
}

fn spline(column: &Table) -> Table {
    handler(column).get("__songlua_spline").unwrap()
}

fn point(column: &Table, index: usize) -> Table {
    spline(column)
        .get::<Table>("__songlua_spline_points")
        .unwrap()
        .raw_get(index)
        .unwrap()
}

fn assert_float(a: f32, b: f32) {
    assert!(
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
        "{a:?} != {b:?}"
    );
}

fn assert_spline(a: &Option<SongLuaSplineData>, b: &Option<SongLuaSplineData>) {
    match (a, b) {
        (Some(a), Some(b)) => {
            assert_eq!(
                (a.constant, a.subtract_song_beat),
                (b.constant, b.subtract_song_beat)
            );
            assert_float(a.beats_per_t, b.beats_per_t);
            assert_float(a.receptor_t, b.receptor_t);
            assert_eq!(a.coefficients.len(), b.coefficients.len());
            for (a, b) in a.coefficients.iter().zip(b.coefficients.iter()) {
                for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
                    assert_float(*a, *b);
                }
            }
        }
        (None, None) => {}
        pair => panic!("spline presence changed: {pair:?}"),
    }
}

pub(super) fn assert_frames(a: &[SongLuaColumnSplineFrame], b: &[SongLuaColumnSplineFrame]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_float(a.second, b.second);
        assert_spline(&a.position, &b.position);
        assert_spline(&a.zoom, &b.zoom);
    }
}

fn compare(
    new: &mut ColumnSplineCapture,
    old: &mut baseline::ColumnSplineCapture,
    lua: &Lua,
    second: f32,
) {
    assert_eq!(new.capture(lua, second), old.capture(lua, second));
    assert_eq!(new.bytes, old.bytes);
    assert_eq!(new.lanes.len(), old.lanes.len());
    for (key, frames) in &new.lanes {
        assert_frames(&frames.frames, &old.lanes[key]);
    }
}

#[test]
fn unchanged_capture_and_metadata_changes_reuse_spline_storage() {
    let (lua, actors) = fixture(8, 64);
    let mut new = ColumnSplineCapture::default();
    let mut old = baseline::ColumnSplineCapture::default();
    compare(&mut new, &mut old, &lua, 0.0);
    let coefficients = Arc::clone(
        &new.lanes[&(0, 0)].frames[0]
            .position
            .as_ref()
            .unwrap()
            .coefficients,
    );
    // Warm mlua's reference slots before counting Rust allocator calls.
    new.capture(&lua, 0.5).unwrap();
    // Stop collection so previous Lua garbage cannot enter the budget.
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        for i in 1..=16 {
            new.capture(&lua, i as f32).unwrap();
        }
    });
    compare(&mut new, &mut old, &lua, 17.0);
    assert_eq!(new.lanes[&(0, 0)].frames.len(), 1);
    handler(&actors[0])
        .set("__songlua_receptor_t", -0.5)
        .unwrap();
    compare(&mut new, &mut old, &lua, 18.0);
    assert_eq!(new.lanes[&(0, 0)].frames.len(), 2);
    assert!(Arc::ptr_eq(
        &coefficients,
        &new.lanes[&(0, 0)].frames[1]
            .position
            .as_ref()
            .unwrap()
            .coefficients
    ));
}

#[test]
fn capture_preserves_in_place_edits_replacements_resize_and_mode_transitions() {
    for size in [0, 1, 2, 3, 16, 256] {
        let (lua, actors) = fixture(2, size);
        let mut new = ColumnSplineCapture::default();
        let mut old = baseline::ColumnSplineCapture::default();
        compare(&mut new, &mut old, &lua, -1.0);
        for i in 0..24 {
            if size != 0 {
                point(&actors[i % 2], (i % size) + 1)
                    .raw_set((i % 3) + 1, i as f32 - 10.0)
                    .unwrap();
            }
            handler(&actors[0])
                .set("__songlua_beats_per_t", if i % 2 == 0 { 0.5 } else { -2.0 })
                .unwrap();
            handler(&actors[1])
                .set("__songlua_subtract_song_beat", i % 3 == 0)
                .unwrap();
            compare(&mut new, &mut old, &lua, i as f32 * 0.25);
        }
        handler(&actors[0])
            .set("__songlua_spline_mode", "NoteColumnSplineMode_Disabled")
            .unwrap();
        compare(&mut new, &mut old, &lua, 8.0);
        handler(&actors[0])
            .set("__songlua_spline_mode", "notecolumnsplinemode_position")
            .unwrap();
        compare(&mut new, &mut old, &lua, 9.0);
        if size > 0 {
            let points: Table = spline(&actors[0]).get("__songlua_spline_points").unwrap();
            points
                .raw_set(1, lua.create_sequence_from([-0.0, 0.0, -0.0]).unwrap())
                .unwrap();
            compare(&mut new, &mut old, &lua, 10.0);
            points
                .raw_set(1, lua.create_sequence_from([0.0, -0.0, 0.0]).unwrap())
                .unwrap();
            handler(&actors[0])
                .set("__songlua_receptor_t", 1.5)
                .unwrap();
            compare(&mut new, &mut old, &lua, 11.0);
            spline(&actors[0])
                .set("__songlua_spline_size", size / 2)
                .unwrap();
            compare(&mut new, &mut old, &lua, 12.0);
        }
    }
}

#[test]
fn cached_capture_still_validates_all_points_and_retains_error_messages() {
    for (key, value) in [
        ("__songlua_beats_per_t", 0.0),
        ("__songlua_receptor_t", f32::INFINITY),
    ] {
        let (lua, actors) = fixture(1, 16);
        let mut new = ColumnSplineCapture::default();
        let mut old = baseline::ColumnSplineCapture::default();
        compare(&mut new, &mut old, &lua, 0.0);
        handler(&actors[0]).set(key, value).unwrap();
        assert_eq!(new.capture(&lua, 1.0), old.capture(&lua, 1.0));
        assert!(new.capture(&lua, 1.0).is_err());
    }
    for size in [16, 65_537] {
        let (lua, actors) = fixture(1, 16);
        let mut new = ColumnSplineCapture::default();
        let mut old = baseline::ColumnSplineCapture::default();
        compare(&mut new, &mut old, &lua, 0.0);
        spline(&actors[0])
            .set("__songlua_spline_size", size)
            .unwrap();
        point(&actors[0], 16).raw_set(3, f32::NAN).unwrap();
        assert_eq!(new.capture(&lua, 1.0), old.capture(&lua, 1.0));
        assert!(new.capture(&lua, 1.0).is_err());
    }
    let (lua, actors) = fixture(1, 4);
    let mut new = ColumnSplineCapture::default();
    let mut old = baseline::ColumnSplineCapture::default();
    compare(&mut new, &mut old, &lua, 0.0);
    // A late malformed point must still fail, even after an earlier mismatch.
    point(&actors[0], 1).raw_set(1, -25.0).unwrap();
    let points: Table = spline(&actors[0]).get("__songlua_spline_points").unwrap();
    points.raw_set(4, false).unwrap();
    assert_eq!(new.capture(&lua, 1.0), old.capture(&lua, 1.0));
}

#[test]
fn overflowing_coefficients_keep_parent_dedup_and_track_budget_behavior() {
    let (lua, actors) = fixture(1, 4);
    for i in 1..=4 {
        point(&actors[0], i)
            .raw_set(1, if i % 2 == 0 { f32::MAX } else { -f32::MAX })
            .unwrap();
    }
    let mut new = ColumnSplineCapture::default();
    let mut old = baseline::ColumnSplineCapture::default();
    for i in 0..4 {
        compare(&mut new, &mut old, &lua, i as f32);
    }
    new.bytes = 128 * 1024 * 1024;
    old.bytes = new.bytes;
    handler(&actors[0])
        .set("__songlua_receptor_t", 1.0)
        .unwrap();
    assert_eq!(new.capture(&lua, 5.0), old.capture(&lua, 5.0));
    assert_eq!(
        new.capture(&lua, 5.0).unwrap_err(),
        "Position spline tracks exceed 128 MiB per layer"
    );
}

#[test]
fn capture_keeps_player_lane_identity_missing_axes_and_mode_conversion_errors() {
    let (lua, actors) = fixture(2, 3);
    let player: Table = lua.globals().get("__songlua_top_screen_player_1").unwrap();
    lua.globals()
        .set("__songlua_top_screen_player_2", player)
        .unwrap();
    actors[1].set("__songlua_player_index", 1).unwrap();
    actors[1].set("__songlua_column_index", 0).unwrap();
    point(&actors[0], 2).raw_set(3, mlua::Value::Nil).unwrap();
    let mut new = ColumnSplineCapture::default();
    let mut old = baseline::ColumnSplineCapture::default();
    compare(&mut new, &mut old, &lua, 0.0);
    assert_eq!(new.lanes.len(), 2);
    for mode in [
        mlua::Value::String(lua.create_string("NoteColumnSplineMode_Position").unwrap()),
        mlua::Value::String(lua.create_string("x".repeat(256)).unwrap()),
        mlua::Value::Integer(7),
        mlua::Value::Boolean(false),
        mlua::Value::Nil,
        mlua::Value::String(lua.create_string([0xff, 0xfe]).unwrap()),
    ] {
        handler(&actors[0])
            .set("__songlua_spline_mode", mode)
            .unwrap();
        assert_eq!(new.capture(&lua, 1.0), old.capture(&lua, 1.0));
    }
}

pub(super) fn change(actors: &[Table], step: usize, mode: &str) {
    match mode {
        "metadata" => {
            for actor in actors {
                handler(actor)
                    .set("__songlua_receptor_t", (step % 2) as f32 * 0.25)
                    .unwrap();
            }
        }
        "changing" => {
            for actor in actors {
                point(actor, 1)
                    .raw_set(1, (step % 2) as f32 * 10.0)
                    .unwrap();
            }
        }
        _ => {}
    }
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_spline_capture() {
    const SAMPLES: usize = 120;
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (lanes, size, mode) in [
        (1, 2, "steady"),
        (8, 32, "steady"),
        (8, 256, "steady"),
        (1, 256, "metadata"),
        (1, 32, "changing"),
        (8, 0, "disabled"),
    ] {
        let (lua, actors) = fixture(lanes, size);
        let mut new = ColumnSplineCapture::default();
        let mut old = baseline::ColumnSplineCapture::default();
        for i in 0..SAMPLES {
            change(&actors, i, mode);
            compare(&mut new, &mut old, &lua, i as f32 / 120.0);
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
            crate::perf::measure_sampled(&label, 16, lanes * SAMPLES, || {
                if old {
                    let mut capture = baseline::ColumnSplineCapture::default();
                    for i in 0..SAMPLES {
                        change(&actors, i, mode);
                        capture.capture(black_box(&lua), i as f32 / 120.0).unwrap();
                    }
                    black_box(capture);
                } else {
                    let mut capture = ColumnSplineCapture::default();
                    for i in 0..SAMPLES {
                        change(&actors, i, mode);
                        capture.capture(black_box(&lua), i as f32 / 120.0).unwrap();
                    }
                    black_box(capture);
                }
                // Include final Lua garbage collection in both complete capture
                // operations so allocation/free accounting starts clean and
                // does not depend on the preceding variant's collection phase.
                lua.gc_collect().unwrap();
            });
        }
    }
}
