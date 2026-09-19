use super::*;
use crate::perf;
use deadlib_render_core::TexturedMeshVertex;
use std::hint::black_box;

mod baseline;

fn vertices(depths: [f32; 3]) -> [TexturedMeshVertex; 3] {
    std::array::from_fn(|i| TexturedMeshVertex {
        pos: [
            [-0.7, -0.6, depths[0]],
            [0.8, -0.4, depths[1]],
            [0.2, 0.9, depths[2]],
        ][i],
        uv: [i as f32 * 0.3, i as f32 * -0.2],
        color: [0.3 + i as f32 * 0.2, 0.2, 0.8, 0.7],
        tex_matrix_scale: [0.5, 1.5],
    })
}

fn project<const OLD: bool>(
    matrix: &Matrix4,
    vertices: &[TexturedMeshVertex],
) -> Option<([ScreenVertexTexColor; 4], usize)> {
    let project = if OLD {
        baseline::project_tmesh_polygon
    } else {
        project_tmesh_polygon
    };
    project(
        matrix,
        [0.8, 0.7, 0.6, 0.5],
        [0.4, 0.8],
        [-0.1, 0.2],
        [0.3, -0.4],
        vertices,
        640,
        480,
    )
}

fn bits(projected: Option<([ScreenVertexTexColor; 4], usize)>) -> Option<([[u32; 8]; 4], usize)> {
    projected.map(|(vertices, len)| {
        (
            vertices.map(|v| {
                [
                    v.x, v.y, v.u, v.v, v.color[0], v.color[1], v.color[2], v.color[3],
                ]
                .map(f32::to_bits)
            }),
            len,
        )
    })
}

#[test]
fn near_clip_projection_matches_previous_vertex_bits() {
    let depths = [
        0.0,
        -1.0,
        -1.0001,
        -2.0,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NAN,
    ];
    for matrix in [
        Matrix4::IDENTITY,
        glam::camera::rh::proj::opengl::perspective(0.9, 1.3, 0.1, 100.0),
        Matrix4::ZERO,
    ] {
        for a in depths {
            for b in depths {
                for c in depths {
                    let vertices = vertices([a, b, c]);
                    assert_eq!(
                        bits(project::<true>(&matrix, &vertices)),
                        bits(project::<false>(&matrix, &vertices)),
                        "depths: {a}, {b}, {c}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release CPU benchmark; --ignored --nocapture --test-threads=1"]
fn near_clip_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, depths) in [
        ("inside", [0.0; 3]),
        ("one-outside", [0.0, 0.0, -2.0]),
        ("two-outside", [-2.0, 0.0, -2.0]),
        ("behind", [-2.0; 3]),
    ] {
        let triangles: Vec<_> = (0..256)
            .map(|index| {
                let mut vertices = vertices(depths);
                vertices[0].pos[0] += index as f32 * 0.0001;
                vertices
            })
            .collect();
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("{name}/{}", if old { "old" } else { "new" }),
                1024,
                triangles.len(),
                || {
                    for triangle in black_box(&triangles) {
                        if old {
                            black_box(project::<true>(black_box(&Matrix4::IDENTITY), triangle));
                        } else {
                            black_box(project::<false>(black_box(&Matrix4::IDENTITY), triangle));
                        }
                    }
                },
            );
        }
    }
}
