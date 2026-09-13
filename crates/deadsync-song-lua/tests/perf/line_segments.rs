use super::*;
use std::hint::black_box;

#[path = "line_segments_baseline.rs"]
mod baseline;

fn point(pos: [f32; 2], index: usize) -> SongLuaActorMultiVertexPoint {
    SongLuaActorMultiVertexPoint {
        pos,
        color: [index as f32 / 7.0, 0.25, 0.75, 1.0],
        uv: [index as f32 / 11.0, -0.0],
    }
}

fn fingerprint(vertices: &[SongLuaOverlayMeshVertex]) -> Vec<[u32; 8]> {
    vertices
        .iter()
        .map(|v| {
            [
                v.pos[0], v.pos[1], v.color[0], v.color[1], v.color[2], v.color[3], v.uv[0],
                v.uv[1],
            ]
            .map(f32::to_bits)
        })
        .collect()
}

fn compare(vertices: &[SongLuaActorMultiVertexPoint], width: f32) {
    let old = baseline::actor_multi_vertex_line_strip(vertices, width);
    let new = actor_multi_vertex_line_strip(vertices, width);
    assert_eq!(
        fingerprint(&new),
        fingerprint(&old),
        "width={width}, points={vertices:?}"
    );
}

#[test]
fn lua_stream_pass_line_segments_preserves_joins_endpoints_and_degenerate_segments() {
    for positions in [
        vec![],
        vec![[0.0, 0.0]],
        vec![[0.0, 0.0], [10.0, 0.0]],
        vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
        vec![[0.0, 0.0], [10.0, 0.0], [0.0, 0.0]],
        vec![[0.0, 0.0], [10.0, 0.0], [0.0, 0.01]],
        vec![[0.0, 0.0], [0.0, 0.0], [10.0, 5.0], [10.0, 5.0]],
        vec![[0.0, 0.0], [10.0, 5.0], [10.0, 5.0], [5.0, 10.0]],
        vec![[0.0, 0.0]; 16],
        vec![[-0.0, 0.0], [f32::EPSILON, -0.0], [f32::EPSILON * 3.0, 0.0]],
    ] {
        let vertices: Vec<_> = positions
            .into_iter()
            .enumerate()
            .map(|(i, pos)| point(pos, i))
            .collect();
        for width in [
            -1.0,
            -0.0,
            0.0,
            f32::EPSILON,
            f32::EPSILON * 2.0,
            1.0,
            4.0,
            128.0,
        ] {
            compare(&vertices, width);
        }
    }
}

#[test]
fn lua_stream_pass_line_segments_matches_seeded_finite_geometry_bit_for_bit() {
    let mut seed = 0x475a_9213_u32;
    for count in [2, 3, 8, 33, 128, 1024] {
        for width in [0.125, 1.0, 3.5, 128.0] {
            let mut vertices = Vec::new();
            for i in 0..count {
                let pos = std::array::from_fn(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed as i32 % 100_000) as f32 / 127.0
                });
                vertices.push(point(pos, i));
            }
            compare(&vertices, width);
        }
    }
}

#[test]
fn lua_stream_pass_line_segments_preserves_nonfinite_results_and_width_panics() {
    for coordinate in [
        f32::MIN_POSITIVE,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ] {
        let vertices = [
            point([0.0, -0.0], 0),
            point([coordinate, 0.5], 1),
            point([1.0, coordinate], 2),
            point([3.0, 4.0], 3),
        ];
        for width in [1.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            let old = std::panic::catch_unwind(|| {
                baseline::actor_multi_vertex_line_strip(&vertices, width)
            });
            let new = std::panic::catch_unwind(|| actor_multi_vertex_line_strip(&vertices, width));
            match (old, new) {
                (Ok(old), Ok(new)) => assert_eq!(fingerprint(&new), fingerprint(&old)),
                (Err(_), Err(_)) => {}
                _ => panic!("changed panic behavior for coordinate={coordinate}, width={width}"),
            }
        }
    }
}

#[test]
fn lua_stream_pass_line_segments_allocates_only_the_output() {
    let vertices: Vec<_> = (0..512)
        .map(|i| point([i as f32, (i % 7) as f32], i))
        .collect();
    let bytes = (vertices.len() - 1) * 6 * std::mem::size_of::<SongLuaOverlayMeshVertex>();
    crate::perf::assert_churn_budget(1, bytes, || {
        drop(black_box(actor_multi_vertex_line_strip(
            black_box(&vertices),
            4.0,
        )))
    });
    crate::perf::assert_no_churn(|| {
        drop(black_box(actor_multi_vertex_line_strip(
            black_box(&vertices),
            0.0,
        )))
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_stream_pass_bench_line_segments() {
    for (kind, count, width) in [
        ("straight", 0, 4.0),
        ("straight", 1, 4.0),
        ("straight", 2, 4.0),
        ("straight", 8, 4.0),
        ("straight", 128, 4.0),
        ("straight", 1024, 4.0),
        ("zigzag", 8, 4.0),
        ("zigzag", 128, 4.0),
        ("zigzag", 1024, 4.0),
        ("zigzag", 4096, 4.0),
        ("repeated", 128, 4.0),
        ("degenerate", 128, 4.0),
        ("zigzag", 128, 0.0),
    ] {
        let vertices: Vec<_> = (0..count)
            .map(|i| {
                let pos = match kind {
                    "straight" => [i as f32, 0.0],
                    "repeated" => [(i / 2) as f32, (i / 2 % 7) as f32],
                    "degenerate" => [0.0, 0.0],
                    _ => [i as f32, (i % 7) as f32],
                };
                point(pos, i)
            })
            .collect();
        compare(&vertices, width);
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "line_segments_{kind}_{count}_{width}/{}",
                    if old { "old" } else { "new" }
                ),
                256,
                count.saturating_sub(1).max(1),
                || {
                    let output = if black_box(old) {
                        baseline::actor_multi_vertex_line_strip(
                            black_box(&vertices),
                            black_box(width),
                        )
                    } else {
                        actor_multi_vertex_line_strip(black_box(&vertices), black_box(width))
                    };
                    drop(black_box(output));
                },
            );
        }
    }
}
