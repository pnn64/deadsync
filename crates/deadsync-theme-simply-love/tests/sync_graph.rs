//! Exact output comparisons and paired benchmarks for sync graph preparation.
use deadlib_render_core::MeshVertex;
use deadsync_config::null_or_die::GraphOrigin;
use image::RgbaImage;
use null_or_die::GraphOrientation;
use std::{hint::black_box, sync::Arc};

#[path = "sync_graph/baseline.rs"]
#[allow(dead_code)]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/screens/select_music/sync_graph.rs"]
#[allow(dead_code)]
mod sync_graph;
use sync_graph::SyncGraphCols;

fn values(len: usize, kind: usize) -> Vec<f64> {
    let mut state = 718392476u64;
    (0..len)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            match kind {
                0 => (state % 100_003) as f64 / 997.0 - 40.0,
                1 => i as f64,
                2 => (len - i) as f64,
                3 => 7.0,
                4 => (i % 7) as f64 - 3.0,
                5 => f64::from_bits(state),
                _ => [
                    f64::NAN,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    -0.0,
                    0.0,
                    -1.0,
                    1.0,
                    f64::from_bits(0xfff8_0000_0000_0032),
                    f64::from_bits(1),
                ][i % 9],
            }
        })
        .collect()
}

fn assert_pair(expected: (f64, f64), actual: (f64, f64)) {
    assert_eq!(
        (expected.0.to_bits(), expected.1.to_bits()),
        (actual.0.to_bits(), actual.1.to_bits())
    );
}

#[test]
fn percentiles_match_sorted_reference_including_nonfinite_and_reversed_ranks() {
    for len in [0, 1, 2, 3, 4, 7, 31, 64, 65, 128, 1023, 4096] {
        for kind in 0..7 {
            let input = values(len, kind);
            let original = input.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            for (lo, hi) in [
                (0.0, 100.0),
                (10.0, 90.0),
                (3.0, 97.0),
                (99.5, 0.5),
                (50.0, 50.0),
                (0.0, 0.0),
                (100.0, 100.0),
                (25.0, 75.0),
                (f64::NAN, 90.0),
                (-1.0, 0.0),
                (f64::NEG_INFINITY, 0.0),
            ] {
                assert_pair(
                    baseline::sync_percentile_pair(&input, lo, hi),
                    sync_graph::sync_percentile_pair(&input, lo, hi),
                );
            }
            assert_eq!(
                original,
                input.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
            );
            for limits in [None, Some((10.0, 90.0)), Some((90.0, 10.0))] {
                let before = baseline::sync_heat_value_range(&input, limits);
                let after = sync_graph::sync_heat_value_range(&input, limits);
                match (before, after) {
                    (Some(before), Some(after)) => assert_pair(before, after),
                    (None, None) => {}
                    _ => panic!("range availability changed"),
                }
            }
        }
    }
}

#[test]
fn heatmap_pixels_match_for_zoom_transpose_origins_and_streaming_rows() {
    for (case, (total, rows, data_rows)) in [
        (1, 1, 1),
        (3, 7, 4),
        (7, 3, 3),
        (17, 13, 1),
        (31, 11, 11),
        (2, 0, 1),
    ]
    .into_iter()
    .enumerate()
    {
        for kind in [0, 3, 5, 6] {
            let matrix = values(total * data_rows, kind);
            for cols in [
                SyncGraphCols {
                    total,
                    first: 0,
                    end: total,
                },
                SyncGraphCols {
                    total,
                    first: total / 3,
                    end: (total * 2 / 3).max(1),
                },
            ] {
                for orientation in [GraphOrientation::Vertical, GraphOrientation::Horizontal] {
                    for origin in [GraphOrigin::Top, GraphOrigin::Bottom] {
                        for size in [[1.0, 1.0], [2.0, 3.0], [37.0, 19.0], [4.4, 7.6], [0.1, 0.1]] {
                            for limits in [None, Some((10.0, 90.0))] {
                                let before = baseline::build_sync_heat_image(
                                    &matrix,
                                    rows,
                                    data_rows,
                                    cols,
                                    size,
                                    orientation,
                                    origin,
                                    limits,
                                );
                                let after = sync_graph::build_sync_heat_image(
                                    &matrix,
                                    rows,
                                    data_rows,
                                    cols,
                                    size,
                                    orientation,
                                    origin,
                                    limits,
                                );
                                assert_eq!(
                                    before, after,
                                    "case={case}, kind={kind}, {cols:?}, {orientation:?}, {origin:?}, {size:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn assert_mesh(before: Option<Arc<[MeshVertex]>>, after: Option<Arc<Vec<MeshVertex>>>) {
    match (before, after) {
        (None, None) => {}
        (Some(before), Some(after)) => {
            assert_eq!(before.len(), after.len());
            for (a, b) in before.iter().zip(after.iter()) {
                assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
                assert_eq!(a.color.map(f32::to_bits), b.color.map(f32::to_bits));
            }
        }
        _ => panic!("mesh availability changed"),
    }
}

#[test]
fn curve_vertices_match_bitwise_for_edges_zoom_and_degenerate_segments() {
    for len in [0, 1, 2, 3, 17, 64, 1025] {
        for kind in 0..7 {
            let input = values(len, kind);
            for edge in [0, 1, len / 2, len, usize::MAX] {
                for cols in [
                    SyncGraphCols {
                        total: len,
                        first: 0,
                        end: len,
                    },
                    SyncGraphCols {
                        total: len,
                        first: len / 3,
                        end: len * 2 / 3,
                    },
                ] {
                    for orientation in [GraphOrientation::Vertical, GraphOrientation::Horizontal] {
                        for (w, h) in [(1.0, 1.0), (512.0, 132.0), (0.00001, 0.00002)] {
                            let color = [0.125, -0.0, 1.0, 0.5];
                            assert_mesh(
                                baseline::build_sync_curve_mesh(
                                    &input,
                                    edge,
                                    cols,
                                    w,
                                    h,
                                    orientation,
                                    color,
                                ),
                                new_curve(&input, edge, cols, w, h, orientation, color),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn invalid_and_empty_graphs_keep_their_early_returns() {
    for cols in [
        SyncGraphCols {
            total: 0,
            first: 0,
            end: 0,
        },
        SyncGraphCols {
            total: 2,
            first: 2,
            end: 1,
        },
        SyncGraphCols {
            total: 2,
            first: 0,
            end: 3,
        },
        SyncGraphCols {
            total: 2,
            first: 0,
            end: 2,
        },
    ] {
        for data_rows in [0, 1] {
            for size in [[0.0, 1.0], [1.0, -1.0], [2.0, 2.0]] {
                let data = [0.0, 1.0];
                assert_eq!(
                    baseline::build_sync_heat_image(
                        &data,
                        1,
                        data_rows,
                        cols,
                        size,
                        GraphOrientation::Vertical,
                        GraphOrigin::Bottom,
                        None
                    ),
                    sync_graph::build_sync_heat_image(
                        &data,
                        1,
                        data_rows,
                        cols,
                        size,
                        GraphOrientation::Vertical,
                        GraphOrigin::Bottom,
                        None
                    )
                );
                assert_mesh(
                    baseline::build_sync_curve_mesh(
                        &data,
                        0,
                        cols,
                        size[0],
                        size[1],
                        GraphOrientation::Vertical,
                        [1.0; 4],
                    ),
                    new_curve(
                        &data,
                        0,
                        cols,
                        size[0],
                        size[1],
                        GraphOrientation::Vertical,
                        [1.0; 4],
                    ),
                );
            }
        }
    }
}

#[test]
fn preparation_has_bounded_allocation_churn() {
    let input = values(4096, 0);
    perf::assert_churn_budget(1, input.len() * 8, || {
        black_box(sync_graph::sync_percentile_pair(&input, 10.0, 90.0));
    });
    perf::assert_no_churn(|| {
        black_box(sync_graph::sync_percentile_pair(&[], 10.0, 90.0));
        black_box(sync_graph::sync_percentile_pair(&[1.0], 10.0, 90.0));
    });
    let cols = SyncGraphCols {
        total: 64,
        first: 0,
        end: 64,
    };
    perf::assert_churn_budget(1, 512 * 132 * 4, || {
        black_box(sync_graph::build_sync_heat_image(
            &input,
            64,
            64,
            cols,
            [512.0, 132.0],
            GraphOrientation::Vertical,
            GraphOrigin::Bottom,
            None,
        ));
    });
}

#[test]
fn curve_storage_reuses_unique_buffers_and_preserves_shared_snapshots() {
    let mut input = values(1024, 0);
    let cols = SyncGraphCols {
        total: input.len(),
        first: 0,
        end: input.len(),
    };
    let mut mesh = new_curve(
        &input,
        0,
        cols,
        512.0,
        132.0,
        GraphOrientation::Vertical,
        [1.0; 4],
    );
    let pointer = mesh.as_ref().unwrap().as_ptr();
    for step in 0..8 {
        input[3] = step as f64;
        perf::assert_no_churn(|| {
            sync_graph::update_sync_curve_mesh(
                &mut mesh,
                &input,
                0,
                cols,
                512.0,
                132.0,
                GraphOrientation::Vertical,
                [1.0; 4],
            )
        });
        assert_eq!(mesh.as_ref().unwrap().as_ptr(), pointer);
        assert_mesh(
            baseline::build_sync_curve_mesh(
                &input,
                0,
                cols,
                512.0,
                132.0,
                GraphOrientation::Vertical,
                [1.0; 4],
            ),
            mesh.clone(),
        );
    }
    let old_snapshot = mesh.as_ref().unwrap().clone();
    let expected_snapshot = old_snapshot
        .iter()
        .map(|v| v.pos.map(f32::to_bits))
        .collect::<Vec<_>>();
    input[3] = -987.0;
    sync_graph::update_sync_curve_mesh(
        &mut mesh,
        &input,
        0,
        cols,
        512.0,
        132.0,
        GraphOrientation::Vertical,
        [1.0; 4],
    );
    assert_ne!(mesh.as_ref().unwrap().as_ptr(), pointer);
    assert_eq!(
        expected_snapshot,
        old_snapshot
            .iter()
            .map(|v| v.pos.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_mesh(
        baseline::build_sync_curve_mesh(
            &input,
            0,
            cols,
            512.0,
            132.0,
            GraphOrientation::Vertical,
            [1.0; 4],
        ),
        mesh.clone(),
    );
    for (first, end) in [(300, 800), (0, 1024), (0, 0), (0, 1024)] {
        let cols = SyncGraphCols { first, end, ..cols };
        sync_graph::update_sync_curve_mesh(
            &mut mesh,
            &input,
            0,
            cols,
            512.0,
            132.0,
            GraphOrientation::Horizontal,
            [1.0; 4],
        );
        assert_mesh(
            baseline::build_sync_curve_mesh(
                &input,
                0,
                cols,
                512.0,
                132.0,
                GraphOrientation::Horizontal,
                [1.0; 4],
            ),
            mesh.clone(),
        );
    }
}

#[test]
fn reusable_curve_actor_produces_the_same_render_frame() {
    use deadlib_present::{
        actors::{Actor, SizeSpec},
        compose::build_screen,
        font::FontMap,
        space::Metrics,
    };
    use deadlib_render_core::{BlendMode, frame_compare::compare_render_frames};
    let input = values(65, 0);
    let cols = SyncGraphCols {
        total: input.len(),
        first: 3,
        end: 61,
    };
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 480.0,
        bottom: 0.0,
    };
    for orientation in [GraphOrientation::Vertical, GraphOrientation::Horizontal] {
        let old_mesh = baseline::build_sync_curve_mesh(
            &input,
            2,
            cols,
            512.0,
            132.0,
            orientation,
            [1.0, 0.5, 0.25, 0.75],
        )
        .unwrap();
        let new_mesh = new_curve(
            &input,
            2,
            cols,
            512.0,
            132.0,
            orientation,
            [1.0, 0.5, 0.25, 0.75],
        )
        .unwrap();
        let old_actor = Actor::Mesh {
            align: [0.0; 2],
            offset: [42.0, 91.0],
            size: [SizeSpec::Px(512.0), SizeSpec::Px(132.0)],
            tint: [1.0, 0.75, 0.5, 0.25],
            vertices: old_mesh,
            visible: true,
            blend: BlendMode::Alpha,
            z: 1501,
        };
        let new_actor = Actor::ReusableMesh {
            align: [0.0; 2],
            offset: [42.0, 91.0],
            size: [SizeSpec::Px(512.0), SizeSpec::Px(132.0)],
            tint: [1.0, 0.75, 0.5, 0.25],
            vertices: new_mesh,
            visible: true,
            blend: BlendMode::Alpha,
            z: 1501,
        };
        let fonts = FontMap::default();
        let old_frame = build_screen(&[old_actor], [0.0; 4], &metrics, &fonts, 0.0);
        let new_frame = build_screen(&[new_actor], [0.0; 4], &metrics, &fonts, 0.0);
        compare_render_frames(&old_frame, &new_frame).unwrap();
    }
}

type Percentile = fn(&[f64], f64, f64) -> (f64, f64);
type Heat = fn(
    &[f64],
    usize,
    usize,
    SyncGraphCols,
    [f32; 2],
    GraphOrientation,
    GraphOrigin,
    Option<(f64, f64)>,
) -> Option<RgbaImage>;
type Curve = fn(&[f64], usize, SyncGraphCols, f32, f32, GraphOrientation, [f32; 4]) -> usize;

fn new_curve(
    values: &[f64],
    edge: usize,
    cols: SyncGraphCols,
    w: f32,
    h: f32,
    orientation: GraphOrientation,
    color: [f32; 4],
) -> Option<Arc<Vec<MeshVertex>>> {
    let mut mesh = None;
    sync_graph::update_sync_curve_mesh(&mut mesh, values, edge, cols, w, h, orientation, color);
    mesh
}

fn old_curve_bench(
    values: &[f64],
    edge: usize,
    cols: SyncGraphCols,
    w: f32,
    h: f32,
    orientation: GraphOrientation,
    color: [f32; 4],
) -> usize {
    let mesh = black_box(baseline::build_sync_curve_mesh(
        values,
        edge,
        cols,
        w,
        h,
        orientation,
        color,
    ));
    mesh.as_ref().map_or(0, |mesh| mesh.len())
}

fn new_curve_bench(
    values: &[f64],
    edge: usize,
    cols: SyncGraphCols,
    w: f32,
    h: f32,
    orientation: GraphOrientation,
    color: [f32; 4],
) -> usize {
    let mesh = black_box(new_curve(values, edge, cols, w, h, orientation, color));
    mesh.as_ref().map_or(0, |mesh| mesh.len())
}

fn paired<T: Copy>(old: T, new: T, mut run: impl FnMut(&str, T)) {
    let mut implementations = [("old", old), ("new", new)];
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        implementations.reverse();
    }
    for (name, implementation) in implementations {
        run(name, black_box(implementation));
    }
}

#[test]
#[ignore = "manual paired release benchmark; run with --nocapture --test-threads=1"]
fn sync_graph_bench() {
    for (name, len, kind) in [
        ("empty", 0, 0),
        ("tiny", 4, 0),
        ("random", 65536, 0),
        ("sorted", 65536, 1),
        ("descending", 65536, 2),
        ("constant", 65536, 3),
        ("repeated", 65536, 4),
        ("large", 524288, 0),
    ] {
        let input = values(len, kind);
        paired(
            baseline::sync_percentile_pair as Percentile,
            sync_graph::sync_percentile_pair as Percentile,
            |version, work| {
                perf::measure_sampled(
                    &format!("percentile_{name}_{version}"),
                    (524288 / len.max(1)).clamp(2, 2000),
                    len,
                    || work(black_box(&input), black_box(10.0), black_box(90.0)),
                );
            },
        );
    }
    for (name, total, rows, data_rows, first, end, orientation, limits) in [
        (
            "vertical_expand",
            64,
            32,
            32,
            0,
            64,
            GraphOrientation::Vertical,
            None,
        ),
        (
            "horizontal_zoom",
            512,
            128,
            128,
            200,
            232,
            GraphOrientation::Horizontal,
            None,
        ),
        (
            "streaming",
            256,
            512,
            32,
            0,
            256,
            GraphOrientation::Vertical,
            None,
        ),
        (
            "downsample",
            1024,
            512,
            512,
            0,
            1024,
            GraphOrientation::Vertical,
            None,
        ),
        (
            "complete",
            256,
            256,
            256,
            0,
            256,
            GraphOrientation::Vertical,
            Some((10.0, 90.0)),
        ),
        ("tiny", 2, 2, 2, 0, 2, GraphOrientation::Vertical, None),
    ] {
        let input = values(total * data_rows, 0);
        let cols = SyncGraphCols { total, first, end };
        let size = if name == "tiny" {
            [2.0, 2.0]
        } else {
            [512.0, 132.0]
        };
        paired(
            baseline::build_sync_heat_image as Heat,
            sync_graph::build_sync_heat_image as Heat,
            |version, work| {
                perf::measure_sampled(
                    &format!("heat_{name}_{version}"),
                    if name == "tiny" { 2000 } else { 8 },
                    (size[0] * size[1]) as usize,
                    || {
                        work(
                            black_box(&input),
                            rows,
                            data_rows,
                            cols,
                            size,
                            orientation,
                            GraphOrigin::Bottom,
                            limits,
                        )
                    },
                );
            },
        );
    }
    for (name, len, kind, orientation, zoom) in [
        ("vertical", 1024, 0, GraphOrientation::Vertical, false),
        ("horizontal", 1024, 0, GraphOrientation::Horizontal, false),
        ("zoom", 4096, 0, GraphOrientation::Horizontal, true),
        ("flat", 1024, 3, GraphOrientation::Vertical, false),
        ("tiny", 2, 0, GraphOrientation::Vertical, false),
    ] {
        let input = values(len, kind);
        let cols = SyncGraphCols {
            total: len,
            first: if zoom { len / 3 } else { 0 },
            end: if zoom { len * 2 / 3 } else { len },
        };
        paired(
            old_curve_bench as Curve,
            new_curve_bench as Curve,
            |version, work| {
                perf::measure_sampled(
                    &format!("curve_{name}_{version}"),
                    64,
                    cols.end.saturating_sub(cols.first + 1),
                    || {
                        work(
                            black_box(&input),
                            0,
                            cols,
                            512.0,
                            132.0,
                            orientation,
                            [1.0; 4],
                        )
                    },
                );
            },
        );
    }
    for shared in [false, true] {
        let input = values(1024, 0);
        let cols = SyncGraphCols {
            total: input.len(),
            first: 0,
            end: input.len(),
        };
        paired(false, true, |version, new| {
            let mut old_mesh: Option<Arc<[MeshVertex]>> = None;
            let mut new_mesh: Option<Arc<Vec<MeshVertex>>> = None;
            let old_fn = black_box(baseline::build_sync_curve_mesh);
            let new_fn = black_box(sync_graph::update_sync_curve_mesh);
            let name = if shared { "shared" } else { "warm" };
            perf::measure_sampled(
                &format!("refresh_{name}_{version}"),
                64,
                input.len() - 1,
                || {
                    if new {
                        let snapshot = shared.then(|| new_mesh.clone());
                        new_fn(
                            &mut new_mesh,
                            black_box(&input),
                            0,
                            cols,
                            512.0,
                            132.0,
                            GraphOrientation::Vertical,
                            [1.0; 4],
                        );
                        black_box(&new_mesh);
                        black_box(snapshot);
                    } else {
                        let snapshot = shared.then(|| old_mesh.clone());
                        old_mesh = old_fn(
                            black_box(&input),
                            0,
                            cols,
                            512.0,
                            132.0,
                            GraphOrientation::Vertical,
                            [1.0; 4],
                        );
                        black_box(&old_mesh);
                        black_box(snapshot);
                    }
                },
            );
        });
    }
}
