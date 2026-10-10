use deadlib_render_core::{TexturedMeshInstanceRaw, TexturedMeshVertex, textured_mesh_uv_mapper};
use glam::Mat4;
use std::hint::black_box;

#[path = "support/uv_mapper_original.rs"]
mod original;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

fn instance() -> TexturedMeshInstanceRaw {
    let mut instance = TexturedMeshInstanceRaw::new(
        Mat4::IDENTITY,
        [0.8; 4],
        [-0.5, 2.0],
        [0.25, -0.75],
        [0.2, 0.1],
        false,
    );
    instance.sphere_rows = [
        [1.2, 0.3, 0.1, 5.0],
        [0.0, -0.75, 0.4, -3.0],
        [0.3, 0.1, 2.0, -100.0],
    ];
    instance.additive_uv = [0.75, -0.25, 0.1, 0.3];
    instance
}

fn vertices(count: usize) -> Vec<TexturedMeshVertex> {
    (0..count)
        .map(|i| TexturedMeshVertex {
            pos: [(i % 17) as f32 - 8.0, (i % 29) as f32 * 0.3, (i % 7) as f32],
            normal: [(i % 3) as f32 - 1.0, 0.5, 1.0, (i % 8) as f32],
            uv: [0.2, -0.4],
            color: [1.0; 4],
            tex_matrix_scale: [1.5, -2.0],
        })
        .collect()
}

#[test]
fn prepared_uvs_preserve_modes_masks_and_degenerate_transforms() {
    let base = instance();
    for rows in [
        base.sphere_rows,
        [[0.0; 4]; 3],
        [[-0.0; 4]; 3],
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
        [[f32::NAN; 4]; 3],
        [[f32::INFINITY; 4]; 3],
    ] {
        for mask in [0.0, 0.5, 0.6, 1.0, f32::NAN] {
            let mut instance = base;
            instance.sphere_rows = rows;
            instance.texture_mask = mask;
            let mapper = textured_mesh_uv_mapper(instance);
            for vertex in vertices(1024) {
                let expected = original::textured_mesh_uvs(vertex, instance);
                for actual in [
                    mapper(vertex),
                    deadlib_render_core::textured_mesh_uvs(vertex, instance),
                ] {
                    for (expected, actual) in expected
                        .into_iter()
                        .flatten()
                        .zip(actual.into_iter().flatten())
                    {
                        assert!(
                            expected.to_bits() == actual.to_bits()
                                || expected.is_nan() && actual.is_nan(),
                            "{rows:?}, mask={mask}: {expected:?} != {actual:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "paired release throughput benchmark"]
fn benchmark_prepared_sphere_uvs() {
    for count in [96, 3072] {
        let mut vertices = vertices(count);
        for vertex in &mut vertices {
            vertex.normal[3] = 3.0;
        }
        paired_bench::compare(&format!("sphere UVs ({count} vertices)"), 2000, |current| {
            let instance = black_box(instance());
            let vertices = black_box(vertices.as_slice());
            if current {
                let mapper = textured_mesh_uv_mapper(instance);
                for &vertex in vertices {
                    black_box(mapper(vertex));
                }
            } else {
                for &vertex in vertices {
                    black_box(original::textured_mesh_uvs(vertex, instance));
                }
            }
        });
    }
}
