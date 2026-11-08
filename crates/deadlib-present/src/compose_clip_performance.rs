use super::*;
use renderer::TexturedMeshVertex as Vertex;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
#[allow(
    unsafe_code,
    reason = "benchmark-only Windows affinity and cycle-count FFI"
)]
mod bench;
#[path = "compose_clip_original.rs"]
mod original;

thread_local! { static NAN_DIFFS: Cell<usize> = const { Cell::new(0) }; }

#[derive(Clone, Copy)]
struct Settings {
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_shift: [f32; 2],
    texture_mask: bool,
}
const SETTINGS: Settings = Settings {
    tint: [0.25, 0.5, 0.75, 0.8],
    uv_scale: [0.75, 1.25],
    uv_offset: [0.125, -0.25],
    uv_shift: [0.375, -0.125],
    texture_mask: false,
};
type Pool = Vec<Vec<Vertex>>;
type Clip =
    fn(&[Vertex], Matrix4, WorldRect, Settings, Option<&mut Pool>) -> Option<ClippedSpriteObject>;

#[inline(never)]
fn production(
    v: &[Vertex],
    m: Matrix4,
    r: WorldRect,
    s: Settings,
    p: Option<&mut Pool>,
) -> Option<ClippedSpriteObject> {
    clip_textured_mesh_to_world_rect(
        s.tint,
        v,
        m,
        s.uv_scale,
        s.uv_offset,
        s.uv_shift,
        r,
        s.texture_mask,
        p,
    )
}
fn portable(
    v: &[Vertex],
    m: Matrix4,
    r: WorldRect,
    s: Settings,
    p: Option<&mut Pool>,
) -> Option<ClippedSpriteObject> {
    clip_textured_mesh_to_world_rect_inner::<false>(
        s.tint,
        v,
        m,
        s.uv_scale,
        s.uv_offset,
        s.uv_shift,
        r,
        s.texture_mask,
        p,
    )
}
fn reference(
    v: &[Vertex],
    m: Matrix4,
    r: WorldRect,
    s: Settings,
    p: Option<&mut Pool>,
) -> Option<ClippedSpriteObject> {
    original::clip_textured_mesh_to_world_rect(
        s.tint,
        v,
        m,
        s.uv_scale,
        s.uv_offset,
        s.uv_shift,
        r,
        s.texture_mask,
        p,
    )
}

fn next(s: &mut u32) -> u32 {
    *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
    *s
}

fn vertices(n: usize) -> Vec<Vertex> {
    (0..n)
        .map(|i| {
            let base = (i / 3) % 16;
            let corner = [[-8.0, -6.0], [8.0, -6.0], [0.0, 10.0]][i % 3];
            Vertex {
                pos: [
                    corner[0] + base as f32 * 1.25 - 10.0,
                    corner[1] + ((i / 48) % 8) as f32 - 4.0,
                    0.25,
                ],
                uv: [(i % 3) as f32 * 0.5, (i % 2) as f32],
                color: [0.125, 0.25, 0.75, 0.8],
                tex_matrix_scale: [0.75, 1.25],
                ..Vertex::default()
            }
        })
        .collect()
}

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
        3.0,
        -1.0,
        2.0,
        1.0,
    ])
}

fn bits(v: &[Vertex]) -> Vec<u32> {
    v.iter()
        .flat_map(|v| {
            v.pos
                .into_iter()
                .chain(v.normal)
                .chain(v.uv)
                .chain(v.tex_matrix_scale)
                .chain(v.color)
        })
        .map(f32::to_bits)
        .collect()
}
fn snapshot(obj: &ClippedSpriteObject) -> Vec<u32> {
    assert!(obj.sprite.is_none());
    let EditablePayload::TexturedMesh {
        instance,
        vertices,
        geom_cache_key,
        depth_test,
        depth_write,
        depth_test_on_fail,
        clear_depth,
        clear_depth_after,
        sampler,
    } = &obj.object_type
    else {
        panic!("expected textured mesh")
    };
    assert!(matches!(
        vertices,
        renderer::TexturedMeshVertices::Transient(_)
    ));
    assert_eq!(*geom_cache_key, renderer::INVALID_TMESH_CACHE_KEY);
    assert!(
        !depth_test && !depth_write && !depth_test_on_fail && !clear_depth && !clear_depth_after
    );
    assert!(sampler.is_none());
    assert_eq!(instance.additive_texture, 0);
    let mut output = instance
        .model_col0
        .into_iter()
        .chain(instance.model_col1)
        .chain(instance.model_col2)
        .chain(instance.model_col3)
        .chain(instance.tint)
        .chain(instance.uv_scale)
        .chain(instance.uv_offset)
        .chain(instance.uv_tex_shift)
        .chain([instance.texture_mask, instance.cull_mode])
        .chain(instance.sphere_rows.into_iter().flatten())
        .chain(instance.additive_uv)
        .map(f32::to_bits)
        .collect::<Vec<_>>();
    output.extend(bits(vertices));
    output
}
fn pool(input_len: usize) -> Pool {
    vec![vertices(1), Vec::with_capacity(input_len * 7 + 3)]
}
fn compare(v: &[Vertex], m: Matrix4, r: [f32; 4]) {
    let r = WorldRect {
        left: r[0],
        right: r[1],
        bottom: r[2],
        top: r[3],
    };
    let settings = [
        SETTINGS,
        Settings {
            tint: [-0.0, 0.0, 1.0, 0.25],
            uv_scale: [-1.25, 0.0],
            uv_offset: [-0.0, 0.5],
            uv_shift: [0.0, 0.25],
            texture_mask: true,
        },
    ];
    for s in settings {
        let mut expected_pool = pool(v.len());
        let expected = reference(v, m, r, s, Some(&mut expected_pool))
            .as_ref()
            .map(snapshot);
        for f in [production as Clip, portable] {
            let mut actual_pool = pool(v.len());
            let original_ptr = actual_pool.last().unwrap().as_ptr();
            let actual = f(v, m, r, s, Some(&mut actual_pool));
            let values = actual.as_ref().map(snapshot);
            assert_eq!(
                values.as_ref().map(Vec::len),
                expected.as_ref().map(Vec::len),
                "vertex count n={},m={m:?},r={r:?}",
                v.len()
            );
            if let (Some(a), Some(b)) = (&values, &expected) {
                for (i, (&a, &b)) in a.iter().zip(b).enumerate() {
                    if a != b && f32::from_bits(a).is_nan() && f32::from_bits(b).is_nan() {
                        NAN_DIFFS.with(|v| v.set(v.get() + 1));
                        continue;
                    }
                    assert_eq!(a, b, "n={},output={i},m={m:?},r={r:?}", v.len());
                }
            }
            assert_eq!(actual_pool.len(), expected_pool.len());
            for (a, b) in actual_pool.iter().zip(&expected_pool) {
                assert_eq!(bits(a), bits(b));
            }
            if let Some(ClippedSpriteObject {
                object_type:
                    EditablePayload::TexturedMesh {
                        vertices: renderer::TexturedMeshVertices::Transient(out),
                        ..
                    },
                ..
            }) = actual
            {
                assert_eq!(
                    out.as_ptr(),
                    original_ptr,
                    "reuse preallocated vertex storage"
                );
            }
        }
    }
}

#[test]
fn clipping_preserves_vertices_and_recycled_storage() {
    let mut cases = 0;
    let mut state = 7;
    for n in [0, 1, 2, 3, 4, 5, 6, 9, 15, 24, 96, 1536] {
        for m in [
            Matrix4::IDENTITY,
            transform(false),
            transform(true),
            Matrix4::ZERO,
        ] {
            for r in [
                [-1000.0, 1000.0, -1000.0, 1000.0],
                [-4.0, 4.0, -4.0, 4.0],
                [2000.0, 3000.0, 2000.0, 3000.0],
                [-0.0, 0.0, -0.0, 0.0],
            ] {
                compare(&vertices(n), m, r);
                cases += 1;
            }
        }
    }
    for case in 0..20_000 {
        let mut v = vertices((next(&mut state) % 100) as usize);
        for vertex in &mut v {
            for x in vertex
                .pos
                .iter_mut()
                .chain(vertex.uv.iter_mut())
                .chain(vertex.color.iter_mut())
                .chain(vertex.tex_matrix_scale.iter_mut())
            {
                *x = if case % 5 == 0 {
                    f32::from_bits(next(&mut state))
                } else {
                    (next(&mut state) >> 8) as f32 / 65536.0 - 128.0
                };
            }
        }
        compare(
            &v,
            if case % 3 == 0 {
                transform(true)
            } else {
                transform(false)
            },
            [-8.0, 8.0, -8.0, 8.0],
        );
        cases += 1;
    }
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
        f32::from_bits(0x7fc12345),
        f32::from_bits(0xff812345),
    ];
    for case in 0..50_000 {
        let mut v = vertices((next(&mut state) % 70) as usize);
        let mut m = transform(false).to_cols_array();
        for (i, x) in m.iter_mut().enumerate() {
            if i % 4 != 3 {
                *x = if case % 3 == 0 {
                    special[(next(&mut state) % special.len() as u32) as usize]
                } else {
                    ((next(&mut state) >> 8) as f32 / 65536.0 - 128.0) / 31.0
                };
            }
        }
        if case % 7 == 0 {
            m[3] = 0.001;
        }
        if case % 11 == 0 {
            m[15] = 0.0;
        }
        for vertex in &mut v {
            for x in vertex
                .pos
                .iter_mut()
                .chain(vertex.uv.iter_mut())
                .chain(vertex.color.iter_mut())
                .chain(vertex.tex_matrix_scale.iter_mut())
            {
                *x = if case % 3 == 0 {
                    special[(next(&mut state) % special.len() as u32) as usize]
                } else {
                    ((next(&mut state) >> 8) as f32 / 65536.0 - 128.0) / 13.0
                };
            }
        }
        let rect = if case % 5 == 0 {
            [-0.0, 0.0, -0.0, 0.0]
        } else if case % 13 == 0 {
            [
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
            ]
        } else {
            [-8.0, 8.0, -8.0, 8.0]
        };
        compare(&v, Matrix4::from_cols_array(&m), rect);
        cases += 1;
    }
    println!(
        "{cases} cases match all non-NaN output vertex bits on runtime and portable paths; {} NaN-bit differences",
        NAN_DIFFS.with(Cell::get)
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
fn masked_mesh_frames_match_original_clipping() {
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

#[test]
#[ignore = "release CPU benchmark for archived before/after binaries"]
fn benchmark_mesh_clipping() {
    bench::pin_thread();
    let rotation = std::env::var("MESH_CLIP_BENCH_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let mut cases = Vec::new();
    for n in [0, 3, 6, 96, 1536] {
        for (label, rect) in [
            ("contained", [-1000.0, 1000.0, -1000.0, 1000.0]),
            ("partial", [-4.0, 4.0, -4.0, 4.0]),
            ("rejected", [2000.0, 3000.0, 2000.0, 3000.0]),
        ] {
            cases.push((format!("{label}-{n}"), vertices(n), transform(false), rect));
        }
    }
    for n in [6, 96, 1536] {
        cases.push((
            format!("perspective-partial-{n}"),
            vertices(n),
            transform(true),
            [-4.0, 4.0, -4.0, 4.0],
        ));
    }
    for j in 0..cases.len() {
        let (name, v, m, r) = &cases[(j + rotation) % cases.len()];
        compare(v, *m, *r);
        let r = WorldRect {
            left: r[0],
            right: r[1],
            bottom: r[2],
            top: r[3],
        };
        let n = v.len();
        let iterations = (100_000 / n.max(1)).max(40);
        let mut pool = vec![Vec::with_capacity(n * 7 + 3)];
        for round in 0..10 {
            let start_cycles = bench::cycles();
            let start = Instant::now();
            for _ in 0..iterations {
                if let Some(obj) = black_box(production(
                    black_box(v),
                    black_box(*m),
                    black_box(r),
                    black_box(SETTINGS),
                    Some(black_box(&mut pool)),
                )) {
                    let EditablePayload::TexturedMesh {
                        vertices: renderer::TexturedMeshVertices::Transient(mut out),
                        ..
                    } = obj.object_type
                    else {
                        panic!("expected transient mesh")
                    };
                    out.clear();
                    pool.push(out);
                }
            }
            let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
            let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
            println!("mesh-clip,{name},{n},{round},{iterations},{cycles:.4},{ns:.4}");
        }
    }
}
