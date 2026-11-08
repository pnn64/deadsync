use super::*;
use renderer::TexturedMeshVertex as Vertex;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
#[allow(
    unsafe_code,
    reason = "benchmark-only Windows affinity and cycle-count FFI"
)]
mod bench;
#[path = "compose_bounds_original.rs"]
mod original;

fn transform(perspective: bool) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.12,
        -0.23,
        0.31,
        if perspective { 0.001 } else { 0.0 },
        0.37,
        1.48,
        0.0,
        0.0,
        -0.12,
        0.29,
        0.83,
        0.0,
        37.0,
        -81.0,
        2.0,
        1.0,
    ])
}

fn vertices(len: usize) -> Vec<Vertex> {
    (0..len)
        .map(|i| Vertex {
            pos: [
                (i % 97) as f32 * 1.75 - 84.0,
                (i % 31) as f32 * 0.31 - 4.0,
                (i % 19) as f32 * 0.12,
            ],
            ..Vertex::default()
        })
        .collect()
}

fn values(bounds: WorldRect) -> [f32; 4] {
    [bounds.left, bounds.right, bounds.bottom, bounds.top]
}

fn classify(bounds: WorldRect, mask: WorldRect) -> (bool, bool, bool) {
    (
        mask.left >= mask.right || mask.bottom >= mask.top,
        bounds.right < mask.left
            || bounds.left > mask.right
            || bounds.top < mask.bottom
            || bounds.bottom > mask.top,
        bounds.left >= mask.left
            && bounds.right <= mask.right
            && bounds.bottom >= mask.bottom
            && bounds.top <= mask.top,
    )
}

fn portable(vertices: &[Vertex], matrix: Matrix4) -> Option<WorldRect> {
    if is_affine_world_transform(&matrix) {
        textured_mesh_world_bounds_with(vertices, |pos| affine_world_xy_3d(&matrix, pos))
    } else {
        textured_mesh_world_bounds_with(vertices, |pos| world_xy_3d(&matrix, pos))
    }
}

fn compare_bounds(vertices: &[Vertex], matrix: Matrix4) -> usize {
    let expected = original::textured_mesh_world_bounds(vertices, matrix);
    let mut decisions = 0;
    for actual in [
        textured_mesh_world_bounds(vertices, matrix),
        portable(vertices, matrix),
    ] {
        assert_eq!(actual.is_some(), expected.is_some());
        if let (Some(actual), Some(expected)) = (actual, expected) {
            for (a, b) in values(actual).into_iter().zip(values(expected)) {
                // Private bounds are used only in comparisons. Zero signs and
                // NaN payloads may vary when LLVM vectorizes the reduction.
                if !(a == 0.0 && b == 0.0) && !(a.is_nan() && b.is_nan()) {
                    assert_eq!(a.to_bits(), b.to_bits(), "matrix={matrix:?}");
                }
            }
            for mask in [
                WorldRect {
                    left: -100.0,
                    right: 100.0,
                    bottom: -50.0,
                    top: 50.0,
                },
                WorldRect {
                    left: 0.0,
                    right: 100.0,
                    bottom: -0.0,
                    top: 50.0,
                },
                WorldRect {
                    left: -0.0,
                    right: 0.0,
                    bottom: -0.0,
                    top: 0.0,
                },
                WorldRect {
                    left: f32::NEG_INFINITY,
                    right: f32::INFINITY,
                    bottom: f32::NEG_INFINITY,
                    top: f32::INFINITY,
                },
                WorldRect {
                    left: f32::NAN,
                    right: 0.0,
                    bottom: -1.0,
                    top: 1.0,
                },
                expected,
                actual,
            ] {
                assert_eq!(classify(actual, mask), classify(expected, mask));
                decisions += 1;
            }
        }
    }
    decisions
}

fn next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

#[test]
fn bounds_preserve_numeric_results_and_clipping_decisions() {
    let special = [
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        1.0,
        -1.0,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc1_2345),
        f32::from_bits(0xff81_2345),
    ];
    let mut state = 7u32;
    let mut cases = 0;
    let mut decisions = 0;
    for len in [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 31, 32, 33, 64, 96, 257,
    ] {
        for _ in 0..256 {
            let mut input = vertices(len);
            let matrix = Matrix4::from_cols_array(&[
                1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, -0.0, -0.0, 0.0, 1.0,
            ]);
            for vertex in &mut input {
                vertex.pos = [f32::from_bits(next(&mut state) & 0x8000_0000); 3];
            }
            decisions += compare_bounds(&input, matrix);
            cases += 1;
            for (i, vertex) in input.iter_mut().enumerate() {
                vertex.pos = [f32::from_bits(0x7fc0_0000 | (i + 1) as u32), 1.0, -1.0];
            }
            decisions += compare_bounds(&input, Matrix4::IDENTITY);
            cases += 1;
        }
    }
    for case in 0..200_000 {
        let len = (next(&mut state) % 70) as usize;
        let mut input = vertices(len);
        let mut matrix = [0.0; 16];
        for (i, value) in matrix.iter_mut().enumerate() {
            *value = if i % 4 == 3 {
                if i == 15 { 1.0 } else { 0.0 }
            } else if case % 2 == 0 {
                special[(next(&mut state) % special.len() as u32) as usize]
            } else {
                ((next(&mut state) >> 8) as f32 / 4096.0 - 2048.0) / 31.0
            };
        }
        if case % 7 == 0 {
            matrix[3] = 0.001;
        }
        if case % 13 == 0 {
            matrix[15] = 0.0;
        }
        for vertex in &mut input {
            for pos in &mut vertex.pos {
                *pos = if case % 3 == 0 {
                    special[(next(&mut state) % special.len() as u32) as usize]
                } else {
                    f32::from_bits(next(&mut state))
                };
            }
        }
        decisions += compare_bounds(&input, Matrix4::from_cols_array(&matrix));
        cases += 1;
    }
    println!(
        "{cases} bounds cases and {decisions} clipping decisions match runtime and portable paths"
    );
}

fn clone_builder(builder: &FrameBuilder) -> FrameBuilder {
    FrameBuilder {
        items: builder.items.clone(),
        meshes: builder.meshes.clone(),
        textured_meshes: builder.textured_meshes.clone(),
    }
}

fn finish(mut builder: FrameBuilder) -> RenderFrame {
    let mut frame = RenderFrame {
        clear_color: [0.0; 4],
        render_targets: Vec::new(),
        cameras: vec![Matrix4::IDENTITY],
        sprite_instances: Vec::new(),
        mesh_vertices: Vec::new(),
        tmesh_instances: Vec::new(),
        tmesh_geometries: Vec::new(),
        ops: Vec::new(),
    };
    finish_frame::<false>(
        &mut builder,
        &mut frame.mesh_vertices,
        &mut frame.tmesh_instances,
        &mut frame.tmesh_geometries,
        &mut frame.ops,
        &mut HashMap::default(),
    );
    frame
}

fn scene(input: &[Vertex], matrix: Matrix4, storage: usize) -> FrameBuilder {
    let mut builder = FrameBuilder::default();
    for index in 0..4 {
        let vertices = match storage {
            0 => renderer::TexturedMeshVertices::Shared(Arc::from(input)),
            1 => renderer::TexturedMeshVertices::Reusable(Arc::new(input.to_vec())),
            _ => renderer::TexturedMeshVertices::Transient(input.to_vec()),
        };
        let mut instance = renderer::TexturedMeshInstanceRaw::new(
            if index == 0 {
                Matrix4::IDENTITY
            } else {
                matrix
            },
            [0.25, 0.5, 0.75, 0.8],
            [-0.5, 0.25],
            [0.75, 0.1],
            [0.2, -0.1],
            index % 2 != 0,
        );
        instance.cull_mode = (index % 3) as f32;
        builder.push_textured_mesh(
            17,
            index as u32,
            index as i16,
            BlendMode::Alpha,
            0,
            TexturedMeshPayload {
                instance,
                vertices,
                geom_cache_key: 41 + index as u64,
                depth_test: true,
                depth_write: index % 2 == 0,
                depth_test_on_fail: index % 2 != 0,
                clear_depth: index == 1,
                clear_depth_after: index == 2,
                sampler: Some(renderer::MeshSampler {
                    filter: renderer::SamplerFilter::Nearest,
                    wrap: renderer::SamplerWrap::Clamp,
                }),
            },
        );
    }
    builder
}

#[test]
fn masked_mesh_frames_match_original_bounds() {
    let rect = |left, right, bottom, top| WorldRect {
        left,
        right,
        bottom,
        top,
    };
    let masks = [
        vec![],
        vec![rect(-1000.0, 1000.0, -1000.0, 1000.0)],
        vec![rect(4000.0, 5000.0, 4000.0, 5000.0)],
        vec![rect(-12.0, 40.0, -100.0, 12.0)],
        vec![rect(0.0, -0.0, -0.0, 0.0)],
        vec![rect(-84.0, 0.0, -4.0, 0.0)],
        vec![rect(-1000.0, 1000.0, -1000.0, 1000.0); 2],
        vec![
            rect(-12.0, 40.0, -100.0, 12.0),
            rect(-64.0, 16.0, -32.0, 4.0),
        ],
    ];
    let transforms = [
        Matrix4::IDENTITY,
        transform(false),
        transform(true),
        Matrix4::from_scale(Vector3::new(-1.0, 0.5, 1.0)),
        Matrix4::from_scale(Vector3::ZERO),
        Matrix4::from_translation(Vector3::new(4096.0, -0.0, 0.0)),
        Matrix4::from_cols_array(&[
            1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, -0.0, -0.0, 0.0, 1.0,
        ]),
    ];
    let mut frames = 0;
    for len in [0, 5, 6, 96, 1536] {
        let mut input = vertices(len);
        for (i, vertex) in input.iter_mut().enumerate() {
            vertex.uv = [(i % 3) as f32 * 0.5, (i % 2) as f32];
            vertex.color = [0.25, 0.5, 0.75, 0.8];
            vertex.tex_matrix_scale = [0.75, 1.25];
            if i % 7 == 0 {
                vertex.pos = [if i % 2 == 0 { -0.0 } else { 0.0 }; 3];
            }
        }
        for matrix in transforms {
            for storage in 0..3 {
                let source = scene(&input, matrix, storage);
                for masks in &masks {
                    for start in [0, 1] {
                        let mut old = clone_builder(&source);
                        let mut new = clone_builder(&source);
                        let (mut old_pool, mut new_pool) = (Vec::new(), Vec::new());
                        for nested in 0..2 {
                            let current_masks = if nested == 0 {
                                masks.as_slice()
                            } else {
                                &masks[..masks.len().min(1)]
                            };
                            original::clip_objects_range_to_world_masks(
                                &mut old,
                                &mut Vec::new(),
                                start,
                                0,
                                current_masks,
                                &mut old_pool,
                            );
                            clip_objects_range_to_world_masks(
                                &mut new,
                                &mut Vec::new(),
                                start,
                                0,
                                current_masks,
                                &mut new_pool,
                            );
                            renderer::frame_compare::compare_render_frames(&finish(clone_builder(&old)), &finish(clone_builder(&new)))
                        .unwrap_or_else(|error| panic!("len={len}, storage={storage}, start={start}, nested={nested}, matrix={matrix:?}: {error}"));
                            assert_eq!(old_pool.len(), new_pool.len());
                            frames += 1;
                        }
                    }
                }
            }
        }
    }
    println!(
        "{frames} completed render frames match bit-for-bit, including retained storage identity"
    );
}

#[inline(never)]
fn bounds(vertices: &[Vertex], matrix: Matrix4) -> Option<WorldRect> {
    textured_mesh_world_bounds(vertices, matrix)
}

#[test]
#[ignore = "release CPU benchmark with raw rounds for archived before/after binaries"]
fn benchmark_mesh_bounds() {
    bench::pin_thread();
    let rotation = std::env::var("MESH_BOUNDS_BENCH_ROTATION")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut cases = Vec::new();
    for n in [0, 1, 3, 4, 6, 96, 1536, 8192] {
        cases.push((format!("affine-{n}"), vertices(n), transform(false)));
    }
    for n in [6, 96, 1536] {
        cases.push((format!("identity-{n}"), vertices(n), Matrix4::IDENTITY));
        cases.push((format!("perspective-{n}"), vertices(n), transform(true)));
    }
    for index in 0..cases.len() {
        let (name, vertices, matrix) = &cases[(index + rotation) % cases.len()];
        compare_bounds(vertices, *matrix);
        let n = vertices.len();
        let iterations = (500_000 / n.max(1)).max(200);
        for round in 0..10 {
            let start_cycles = bench::cycles();
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(bounds(black_box(vertices), black_box(*matrix)));
            }
            let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
            let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
            println!("mesh-bounds,{name},{n},{round},{iterations},{cycles:.4},{ns:.4}");
        }
    }
}
