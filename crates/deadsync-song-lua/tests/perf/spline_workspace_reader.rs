use super::*;
use std::hint::black_box;
use std::sync::Arc;

#[path = "spline_workspace_reader_baseline.rs"]
pub(crate) mod baseline;

fn column(size: usize) -> (Lua, Table) {
    let (lua, mut columns) = crate::perframe::spline_capture_perf::fixture(1, size);
    (lua, columns.remove(0))
}

fn point(column: &Table, index: usize) -> Table {
    column
        .get::<Table>("__songlua_pos_handler")
        .unwrap()
        .get::<Table>("__songlua_spline")
        .unwrap()
        .get::<Table>("__songlua_spline_points")
        .unwrap()
        .raw_get(index)
        .unwrap()
}

#[test]
fn repeated_geometry_shares_only_coefficients_and_preserves_metadata_and_edits() {
    let (lua, column) = column(32);
    let mut scratch = ColumnSplineReadScratch::default();
    let first = read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch)
        .unwrap()
        .unwrap();
    // A separate handler references equal points and different metadata.
    let original: Table = column.get("__songlua_pos_handler").unwrap();
    let handler = lua.create_table().unwrap();
    for pair in original.pairs::<Value, Value>() {
        let (key, value) = pair.unwrap();
        handler.set(key, value).unwrap();
    }
    handler.set("__songlua_receptor_t", 12.0).unwrap();
    column.set("__songlua_zoom_handler", handler).unwrap();
    let next = read_position_spline(&column, "__songlua_zoom_handler", None, &mut scratch)
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&first.coefficients, &next.coefficients));
    assert_eq!(next.receptor_t, 12.0);
    point(&column, 1).raw_set(1, -0.0).unwrap();
    let changed = read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch)
        .unwrap()
        .unwrap();
    assert!(!Arc::ptr_eq(&first.coefficients, &changed.coefficients));
    point(&column, 1).raw_set(1, 0.0).unwrap();
    let positive_zero = read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch)
        .unwrap()
        .unwrap();
    assert!(!Arc::ptr_eq(
        &changed.coefficients,
        &positive_zero.coefficients
    ));
    assert_eq!(
        positive_zero.coefficients[0][0][0].to_bits(),
        0.0f32.to_bits()
    );
    let reused = read_position_spline(&column, "__songlua_pos_handler", Some(&first), &mut scratch)
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&first.coefficients, &reused.coefficients));
    assert!(Arc::ptr_eq(
        scratch.recent.as_ref().unwrap(),
        &positive_zero.coefficients
    ));
    point(&column, 32).raw_set(3, f32::NAN).unwrap();
    assert_eq!(
        read_position_spline(&column, "__songlua_zoom_handler", None, &mut scratch).unwrap_err(),
        "Position spline contains a nonfinite point"
    );
}

#[test]
fn sharing_still_invokes_metadata_getters_for_each_read() {
    let (lua, column) = column(3);
    let handler: Table = column.get("__songlua_pos_handler").unwrap();
    handler.raw_set("__songlua_receptor_t", Value::Nil).unwrap();
    let meta: Table = lua.load("return {__index=function(t,k) if k == '__songlua_receptor_t' then reads=(reads or 0)+1; return reads end end}").eval().unwrap();
    handler.set_metatable(Some(meta)).unwrap();
    let mut scratch = ColumnSplineReadScratch::default();
    for expected in 1..=4 {
        let result = read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch)
            .unwrap()
            .unwrap();
        assert_eq!(result.receptor_t, expected as f32);
    }
    assert_eq!(lua.globals().get::<i32>("reads").unwrap(), 4);
}

#[test]
fn warmed_identical_geometry_needs_no_new_allocations_without_a_previous_lane() {
    let (lua, column) = column(256);
    let mut scratch = ColumnSplineReadScratch::default();
    for _ in 0..4 {
        read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch).unwrap();
    }
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        for _ in 0..16 {
            black_box(
                read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch).unwrap(),
            );
        }
    });
}

#[test]
fn changed_geometry_only_allocates_its_immutable_output_after_warmup() {
    let (lua, column) = column(256);
    let mut scratch = ColumnSplineReadScratch::default();
    let first = read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch).unwrap();
    for _ in 0..4 {
        read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch).unwrap();
    }
    point(&column, 1).raw_set(1, 123.0).unwrap();
    lua.gc_stop();
    crate::perf::assert_churn_budget(1, 48 * 256 + 2 * std::mem::size_of::<usize>(), || {
        black_box(
            read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch).unwrap(),
        );
    });
    black_box(first);
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_spline_geometry_reuse() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (size, mode) in [
        (2, "repeat"),
        (32, "repeat"),
        (256, "repeat"),
        (4096, "repeat"),
        (2, "changing"),
        (32, "changing"),
        (256, "changing"),
        (4096, "changing"),
    ] {
        let (lua, column) = column(size);
        let mut points = Vec::new();
        let mut scratch = ColumnSplineReadScratch::default();
        let old =
            baseline::read_position_spline(&column, "__songlua_pos_handler", None, &mut points)
                .unwrap();
        let new =
            read_position_spline(&column, "__songlua_pos_handler", None, &mut scratch).unwrap();
        assert_eq!(old, new);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("geometry_{mode}_{size}_{}", if old { "old" } else { "new" });
            crate::perf::measure_sampled(
                &label,
                if size < 4096 { 128 } else { 8 },
                size * 8,
                || {
                    for step in 0..8 {
                        if mode == "changing" {
                            point(&column, 1).raw_set(1, (step % 2) as f32).unwrap();
                        }
                        black_box(if old {
                            baseline::read_position_spline(
                                &column,
                                "__songlua_pos_handler",
                                None,
                                &mut points,
                            )
                            .unwrap()
                        } else {
                            read_position_spline(
                                &column,
                                "__songlua_pos_handler",
                                None,
                                &mut scratch,
                            )
                            .unwrap()
                        });
                    }
                    lua.gc_collect().unwrap();
                },
            );
        }
    }
}
