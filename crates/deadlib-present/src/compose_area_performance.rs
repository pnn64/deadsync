use super::*;
use renderer::{
    SpriteInstanceRaw as Sprite, TexturedMeshVertex as Vertex, TexturedMeshVertices as Vertices,
};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
#[allow(
    unsafe_code,
    reason = "benchmark-only Windows affinity and cycle-count FFI"
)]
mod bench;
#[path = "compose_area_original.rs"]
mod original;

type Area = fn(&EditablePayload, &[Sprite]) -> f32;
type Pool = Vec<Vec<Vertex>>;
type Clip = fn(&mut EditableDraw, &mut [Sprite], &[WorldRect], &mut Pool) -> bool;

fn sprite(size: [f32; 2]) -> Sprite {
    Sprite {
        center: [0.0, 0.0, 0.0, 1.0],
        size,
        rot_sin_cos: [0.0, 1.0],
        tint: [1.0; 4],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        local_offset: [0.0; 2],
        local_offset_rot_sin_cos: [0.0, 1.0],
        edge_fade: [0.0; 4],
        texture_mask: 0.0,
    }
}

#[inline(never)]
fn production(object: &EditablePayload, sprites: &[Sprite]) -> f32 {
    object_world_area(object, sprites)
}
#[inline(never)]
fn reference(object: &EditablePayload, sprites: &[Sprite]) -> f32 {
    original::object_world_area(object, sprites)
}
fn portable(object: &EditablePayload, sprites: &[Sprite]) -> f32 {
    match object {
        EditablePayload::TexturedMesh {
            instance, vertices, ..
        } => textured_mesh_world_area_portable(vertices.as_ref(), &instance.transform()),
        _ => original::object_world_area(object, sprites),
    }
}
#[inline(never)]
fn production_clip(
    object: &mut EditableDraw,
    sprites: &mut [Sprite],
    masks: &[WorldRect],
    pool: &mut Pool,
) -> bool {
    clip_object_to_world_masks(object, sprites, masks, pool)
}
#[inline(never)]
fn reference_clip(
    object: &mut EditableDraw,
    sprites: &mut [Sprite],
    masks: &[WorldRect],
    pool: &mut Pool,
) -> bool {
    original::clip_object_to_world_masks(object, sprites, masks, pool)
}

fn matrix(perspective: bool) -> Matrix4 {
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
        -4.0,
        2.0,
        1.0,
    ])
}

fn vertices(length: usize) -> Vec<Vertex> {
    (0..length)
        .map(|i| {
            let triangle = i / 3;
            let corner = [[-0.25, -0.5], [0.5, 0.4], [-0.6, 0.65]][i % 3];
            Vertex {
                pos: [
                    (triangle % 17) as f32 * 1.25 - 10.0 + corner[0],
                    (triangle % 11) as f32 * 0.31 - 2.0 + corner[1],
                    (i % 19) as f32 * 0.12,
                ],
                uv: [(i % 3) as f32 * 0.5, (i % 2) as f32],
                color: [0.25, 0.5, 0.75, 0.8],
                ..Default::default()
            }
        })
        .collect()
}

fn mesh(input: &[Vertex], transform: Matrix4, storage: usize) -> EditableDraw {
    EditableDraw {
        texture_handle: 17,
        order: 23,
        z: -2,
        camera: 0,
        blend: BlendMode::Alpha,
        object_type: EditablePayload::TexturedMesh {
            instance: renderer::TexturedMeshInstanceRaw::new(
                transform,
                [0.25, 0.5, 0.75, 0.8],
                [-0.5, 0.25],
                [0.75, 0.1],
                [0.2, -0.1],
                true,
            ),
            vertices: match storage {
                0 => Vertices::Shared(Arc::from(input)),
                1 => Vertices::Reusable(Arc::new(input.to_vec())),
                _ => Vertices::Transient(input.to_vec()),
            },
            geom_cache_key: 41,
            depth_test: true,
            depth_write: false,
            depth_test_on_fail: true,
            clear_depth: false,
            clear_depth_after: true,
            sampler: Some(renderer::MeshSampler {
                filter: renderer::SamplerFilter::Nearest,
                wrap: renderer::SamplerWrap::Clamp,
            }),
        },
    }
}

fn compare_area(
    object: &EditablePayload,
    sprites: &[Sprite],
    case: usize,
    nan_differences: &mut [usize; 2],
) {
    let expected = reference(object, sprites);
    for (index, apply) in [production as Area, portable].into_iter().enumerate() {
        let actual = apply(black_box(object), black_box(sprites));
        if actual.is_nan() && expected.is_nan() {
            nan_differences[index] += usize::from(actual.to_bits() != expected.to_bits());
        } else {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "case={case}, variant={index}"
            );
        }
    }
}

#[test]
fn mesh_areas_match_original_for_storage_tails_and_ieee_inputs() {
    let transforms = [
        Matrix4::IDENTITY,
        matrix(false),
        matrix(true),
        Matrix4::from_scale(Vector3::new(-1.0, 0.5, 1.0)),
        Matrix4::ZERO,
        Matrix4::from_cols_array(&[
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            -0.0,
            0.0,
            0.0,
            f32::EPSILON,
        ]),
        Matrix4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -0.0, 0.0, 0.0, -1.0,
        ]),
    ];
    let special = [
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_1234),
        f32::from_bits(0xff80_1234),
    ];
    let mut cases = 0;
    let mut nan_differences = [0; 2];
    for length in (0..=40).chain([63, 64, 65, 95, 96, 97, 191, 192, 193, 767, 768, 769]) {
        let input = vertices(length);
        for transform in transforms {
            for storage in 0..3 {
                compare_area(
                    &mesh(&input, transform, storage).object_type,
                    &[],
                    cases,
                    &mut nan_differences,
                );
                cases += 1;
            }
        }
    }
    for value in special {
        for transform in transforms {
            for storage in 0..3 {
                let mut input = vertices(19);
                for (i, vertex) in input.iter_mut().enumerate() {
                    vertex.pos[i % 3] = value;
                }
                compare_area(
                    &mesh(&input, transform, storage).object_type,
                    &[],
                    cases,
                    &mut nan_differences,
                );
                cases += 1;
            }
        }
        let sprites = [sprite([value, -value])];
        compare_area(
            &EditablePayload::Sprite(0),
            &sprites,
            cases,
            &mut nan_differences,
        );
        cases += 1;
    }
    let mut seed = 0x42a7_913d_u32;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for index in 0_usize..4096 {
        let length = (random() % 97) as usize;
        let mut input = vertices(length);
        let transform = if index.is_multiple_of(2) {
            transforms[index % transforms.len()]
        } else {
            Matrix4::from_cols_array(&std::array::from_fn(|_| f32::from_bits(random())))
        };
        for vertex in &mut input {
            for coordinate in &mut vertex.pos {
                *coordinate = f32::from_bits(random());
            }
        }
        compare_area(
            &mesh(&input, transform, index % 3).object_type,
            &[],
            cases,
            &mut nan_differences,
        );
        cases += 1;
    }
    println!(
        "{cases} area cases match original; NaN payload differences runtime/portable: {nan_differences:?}"
    );
}

fn compare_objects(actual: &EditableDraw, expected: &EditableDraw) {
    assert_eq!(
        (
            actual.texture_handle,
            actual.order,
            actual.z,
            actual.camera,
            actual.blend
        ),
        (
            expected.texture_handle,
            expected.order,
            expected.z,
            expected.camera,
            expected.blend
        )
    );
    let (
        EditablePayload::TexturedMesh {
            instance: a,
            vertices: av,
            geom_cache_key: ak,
            depth_test: at,
            depth_write: aw,
            depth_test_on_fail: af,
            clear_depth: ac,
            clear_depth_after: aa,
            sampler: asm,
        },
        EditablePayload::TexturedMesh {
            instance: b,
            vertices: bv,
            geom_cache_key: bk,
            depth_test: bt,
            depth_write: bw,
            depth_test_on_fail: bf,
            clear_depth: bc,
            clear_depth_after: ba,
            sampler: bsm,
        },
    ) = (&actual.object_type, &expected.object_type)
    else {
        panic!("expected textured mesh")
    };
    assert_eq!(a, b);
    assert_eq!(av.as_ref(), bv.as_ref());
    assert_eq!(std::mem::discriminant(av), std::mem::discriminant(bv));
    assert_eq!((ak, at, aw, af, ac, aa, asm), (bk, bt, bw, bf, bc, ba, bsm));
}

fn rect(left: f32, right: f32, bottom: f32, top: f32) -> WorldRect {
    WorldRect {
        left,
        right,
        bottom,
        top,
    }
}

#[test]
fn mesh_area_mask_selection_preserves_payloads_ties_and_pools() {
    let masks = [
        vec![],
        vec![rect(-1000.0, 1000.0, -1000.0, 1000.0); 2],
        vec![rect(-4.0, 4.0, -4.0, 4.0), rect(-6.0, 2.0, -8.0, 3.0)],
        vec![
            rect(-1000.0, 1000.0, -1000.0, 1000.0),
            rect(-4.0, 4.0, -4.0, 4.0),
        ],
        vec![
            rect(0.0, 0.0, -4.0, 4.0),
            rect(2000.0, 3000.0, 2000.0, 3000.0),
        ],
        vec![rect(-4.0, 0.0, -4.0, 4.0), rect(0.0, 4.0, -4.0, 4.0)],
    ];
    let mut cases = 0;
    for length in [0, 1, 2, 3, 5, 6, 18, 96, 193] {
        for transform in [Matrix4::IDENTITY, matrix(false), matrix(true)] {
            for storage in 0..3 {
                let source = mesh(&vertices(length), transform, storage);
                for masks in &masks {
                    for reverse in [false, true] {
                        let mut masks = masks.clone();
                        if reverse {
                            masks.reverse();
                        }
                        let mut old = source.clone();
                        let mut new = source.clone();
                        let (mut old_pool, mut new_pool) =
                            (vec![Vec::with_capacity(48)], vec![Vec::with_capacity(48)]);
                        assert_eq!(
                            reference_clip(&mut old, &mut [], &masks, &mut old_pool),
                            production_clip(&mut new, &mut [], &masks, &mut new_pool)
                        );
                        compare_objects(&new, &old);
                        assert_eq!(
                            old_pool.iter().map(Vec::capacity).collect::<Vec<_>>(),
                            new_pool.iter().map(Vec::capacity).collect::<Vec<_>>()
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    println!("{cases} complete mask selections match original");
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_mesh_areas() {
    bench::pin_thread();
    let rotation = std::env::var("AREA_BENCH_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let mut cases = Vec::new();
    for length in [0, 2, 3, 6, 18, 96, 768] {
        for perspective in [false, true] {
            cases.push((
                format!(
                    "shared-{}-{length}",
                    if perspective { "perspective" } else { "affine" }
                ),
                mesh(&vertices(length), matrix(perspective), 0),
                Vec::new(),
            ));
        }
    }
    for storage in [1, 2] {
        cases.push((
            format!("storage-{storage}-96"),
            mesh(&vertices(96), matrix(false), storage),
            Vec::new(),
        ));
    }
    cases.push((
        "sprite-control".into(),
        EditableDraw {
            texture_handle: 17,
            order: 0,
            z: 0,
            camera: 0,
            blend: BlendMode::Alpha,
            object_type: EditablePayload::Sprite(0),
        },
        vec![sprite([14.0, 20.0])],
    ));
    for index in 0..cases.len() {
        let (name, object, sprites) = &cases[(index + rotation) % cases.len()];
        let length = match &object.object_type {
            EditablePayload::TexturedMesh { vertices, .. } => vertices.len(),
            _ => 1,
        };
        let iterations = (300_000 / length.max(3)).max(512);
        for round in 0..10 {
            for k in 0..2 {
                let variant = (round + k + rotation) % 2;
                let apply = black_box([production as Area, reference][variant]);
                let start_cycles = bench::cycles();
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(apply(black_box(&object.object_type), black_box(sprites)));
                }
                let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
                let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
                println!("area,{name},{variant},{round},{iterations},{cycles:.4},{ns:.4}");
            }
        }
    }
    for length in [6, 96, 768] {
        for contained in [false, true] {
            let source = mesh(&vertices(length), matrix(false), 0);
            let masks = if contained {
                vec![rect(-1000.0, 1000.0, -1000.0, 1000.0); 2]
            } else {
                vec![rect(-4.0, 4.0, -4.0, 4.0), rect(-6.0, 2.0, -8.0, 3.0)]
            };
            let name = format!(
                "{}-{length}",
                if contained { "contained" } else { "partial" }
            );
            let iterations = (100_000 / length).max(128);
            let mut pools: [Pool; 2] =
                std::array::from_fn(|_| vec![Vec::with_capacity(length * 6)]);
            for round in 0..10 {
                for k in 0..2 {
                    let variant = (round + k + rotation) % 2;
                    let apply = black_box([production_clip as Clip, reference_clip][variant]);
                    let pool = &mut pools[variant];
                    let start_cycles = bench::cycles();
                    let start = Instant::now();
                    for _ in 0..iterations {
                        let mut object = source.clone();
                        black_box(apply(
                            black_box(&mut object),
                            &mut [],
                            black_box(&masks),
                            black_box(&mut *pool),
                        ));
                        black_box(&object);
                        recycle_transient_object_vertices(object.object_type, pool);
                    }
                    let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
                    let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
                    println!("mask-area,{name},{variant},{round},{iterations},{cycles:.4},{ns:.4}");
                }
            }
        }
    }
}
