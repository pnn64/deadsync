use super::*;
use renderer::{SpriteInstanceRaw as Sprite, TexturedMeshVertex as Vertex};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
#[allow(
    unsafe_code,
    reason = "benchmark-only Windows affinity and cycle-count FFI"
)]
mod bench;
#[path = "compose_sprite_clip_original.rs"]
mod original;
type Pool = Vec<Vec<Vertex>>;
type Clip = fn(Sprite, WorldRect, Option<&mut Pool>) -> Option<ClippedSpriteObject>;

#[inline(never)]
fn production(s: Sprite, r: WorldRect, p: Option<&mut Pool>) -> Option<ClippedSpriteObject> {
    clipped_sprite_instance_to_world_rect(17, s, r, p)
}
#[inline(never)]
fn portable(s: Sprite, r: WorldRect, p: Option<&mut Pool>) -> Option<ClippedSpriteObject> {
    portable_instance(17, s, r, p)
}
fn reference(s: Sprite, r: WorldRect, p: Option<&mut Pool>) -> Option<ClippedSpriteObject> {
    original::clipped_sprite_instance_to_world_rect(17, s, r, p)
}

fn portable_instance(
    index: u32,
    sprite: renderer::SpriteInstanceRaw,
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    let eps = 1e-6;
    let offset_world = [
        sprite.local_offset_rot_sin_cos[1].mul_add(
            sprite.local_offset[0],
            -(sprite.local_offset_rot_sin_cos[0] * sprite.local_offset[1]),
        ),
        sprite.local_offset_rot_sin_cos[0].mul_add(
            sprite.local_offset[0],
            sprite.local_offset_rot_sin_cos[1] * sprite.local_offset[1],
        ),
    ];
    let world_center = [
        sprite.center[0] + offset_world[0],
        sprite.center[1] + offset_world[1],
    ];
    if sprite.rot_sin_cos[0].abs() > eps || sprite.rot_sin_cos[1] < 1.0 - eps {
        return portable_rotated(
            sprite.tint,
            sprite.center,
            sprite.size,
            sprite.rot_sin_cos,
            sprite.uv_scale,
            sprite.uv_offset,
            offset_world,
            clip,
            sprite.texture_mask != 0.0,
            recycled_vertices,
        );
    }

    let w = sprite.size[0];
    let h = sprite.size[1];
    if w <= eps || h <= eps {
        return None;
    }

    let half_w = w * 0.5;
    let half_h = h * 0.5;

    let left = world_center[0] - half_w;
    let right = world_center[0] + half_w;
    let bottom = world_center[1] - half_h;
    let top = world_center[1] + half_h;

    if left >= clip.left && right <= clip.right && bottom >= clip.bottom && top <= clip.top {
        return Some(ClippedSpriteObject {
            object_type: EditablePayload::Sprite(index),
            sprite: None,
        });
    }

    let inter_left = left.max(clip.left);
    let inter_right = right.min(clip.right);
    let inter_bottom = bottom.max(clip.bottom);
    let inter_top = top.min(clip.top);
    if inter_left >= inter_right || inter_bottom >= inter_top {
        return None;
    }

    let inv_w = 1.0 / w;
    let inv_h = 1.0 / h;

    let cl = ((inter_left - left) * inv_w).clamp(0.0, 1.0);
    let cr = ((right - inter_right) * inv_w).clamp(0.0, 1.0);
    let cb = ((inter_bottom - bottom) * inv_h).clamp(0.0, 1.0);
    let ct = ((top - inter_top) * inv_h).clamp(0.0, 1.0);

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= eps || sy_crop <= eps {
        return None;
    }

    let uv_offset = [
        sprite.uv_scale[0].mul_add(cl, sprite.uv_offset[0]),
        sprite.uv_scale[1].mul_add(ct, sprite.uv_offset[1]),
    ];
    let uv_scale = [sprite.uv_scale[0] * sx_crop, sprite.uv_scale[1] * sy_crop];

    let center_x = ((cl - cr) * w).mul_add(0.5, world_center[0]) - offset_world[0];
    let center_y = ((cb - ct) * h).mul_add(0.5, world_center[1]) - offset_world[1];
    let new_w = w * sx_crop;
    let new_h = h * sy_crop;

    Some(ClippedSpriteObject {
        object_type: EditablePayload::Sprite(index),
        sprite: Some(renderer::SpriteInstanceRaw {
            center: [center_x, center_y, sprite.center[2], sprite.center[3]],
            size: [new_w, new_h],
            rot_sin_cos: sprite.rot_sin_cos,
            tint: sprite.tint,
            uv_scale,
            uv_offset,
            local_offset: sprite.local_offset,
            local_offset_rot_sin_cos: sprite.local_offset_rot_sin_cos,
            edge_fade: sprite.edge_fade,
            texture_mask: sprite.texture_mask,
        }),
    })
}
#[allow(clippy::too_many_arguments)]
fn portable_rotated(
    tint: [f32; 4],
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    offset_world: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    clip_rotated_sprite_to_world_rect_inner::<false>(
        tint,
        center,
        size,
        rot_sin_cos,
        uv_scale,
        uv_offset,
        offset_world,
        clip,
        texture_mask,
        recycled_vertices,
    )
}
fn sprite(i: usize, angle: f32) -> Sprite {
    Sprite {
        center: [(i % 8) as f32 - 3.5, (i / 8) as f32 - 3.5, 0.125, 1.0],
        size: [18.0 + (i % 3) as f32, 12.0 + (i % 7) as f32],
        rot_sin_cos: [angle.sin(), angle.cos()],
        tint: [0.25, 0.5, 0.75, 0.8],
        uv_scale: [0.75, 1.25],
        uv_offset: [0.125, -0.25],
        local_offset: [1.25, -0.75],
        local_offset_rot_sin_cos: [0.6, 0.8],
        edge_fade: [0.1, 0.2, 0.3, 0.4],
        texture_mask: (i % 2) as f32,
    }
}

fn rect(r: [f32; 4]) -> WorldRect {
    WorldRect {
        left: r[0],
        right: r[1],
        bottom: r[2],
        top: r[3],
    }
}

fn fields(s: &mut Sprite) -> impl Iterator<Item = &mut f32> {
    s.center
        .iter_mut()
        .chain(&mut s.size)
        .chain(&mut s.rot_sin_cos)
        .chain(&mut s.tint)
        .chain(&mut s.uv_scale)
        .chain(&mut s.uv_offset)
        .chain(&mut s.local_offset)
        .chain(&mut s.local_offset_rot_sin_cos)
        .chain(&mut s.edge_fade)
        .chain(std::iter::once(&mut s.texture_mask))
}

fn bits(out: Option<ClippedSpriteObject>) -> (Vec<u64>, Vec<u32>) {
    let mut metadata = Vec::new();
    let mut floats = Vec::new();
    let Some(mut out) = out else {
        return (vec![0], floats);
    };
    metadata.push(1);
    if let Some(ref mut s) = out.sprite {
        metadata.push(1);
        floats.extend(fields(s).map(|f| f.to_bits()));
    } else {
        metadata.push(0);
    }
    match out.object_type {
        EditablePayload::Sprite(index) => {
            metadata.extend([0, index as u64]);
        }
        EditablePayload::TexturedMesh {
            instance: i,
            vertices: renderer::TexturedMeshVertices::Transient(v),
            geom_cache_key,
            depth_test,
            depth_write,
            depth_test_on_fail,
            clear_depth,
            clear_depth_after,
            sampler,
        } => {
            metadata.extend([1, v.len() as u64, i.additive_texture]);
            assert_eq!(geom_cache_key, renderer::INVALID_TMESH_CACHE_KEY);
            assert!(
                !depth_test
                    && !depth_write
                    && !depth_test_on_fail
                    && !clear_depth
                    && !clear_depth_after
                    && sampler.is_none()
            );
            floats.extend(
                i.model_col0
                    .into_iter()
                    .chain(i.model_col1)
                    .chain(i.model_col2)
                    .chain(i.model_col3)
                    .chain(i.tint)
                    .chain(i.uv_scale)
                    .chain(i.uv_offset)
                    .chain(i.uv_tex_shift)
                    .chain([i.texture_mask, i.cull_mode])
                    .chain(i.sphere_rows.into_iter().flatten())
                    .chain(i.additive_uv)
                    .map(f32::to_bits),
            );
            floats.extend(
                v.iter()
                    .flat_map(|v| {
                        v.pos
                            .into_iter()
                            .chain(v.normal)
                            .chain(v.uv)
                            .chain(v.tex_matrix_scale)
                            .chain(v.color)
                    })
                    .map(f32::to_bits),
            );
        }
        _ => unreachable!(),
    }
    (metadata, floats)
}

fn compare(s: Sprite, r: WorldRect, case: usize, nan_diffs: &mut [usize; 2], compared: &mut usize) {
    let expected = bits(reference(s, r, None));
    for (index, f) in [production as Clip, portable].into_iter().enumerate() {
        // Exercise both the empty allocation path and a retained buffer.
        let mut pool = vec![vec![Vertex::default()], Vec::with_capacity(256)];
        let saved_ptr = pool[1].as_ptr();
        let saved_capacity = pool[1].capacity();
        let result = f(
            s,
            r,
            if case.is_multiple_of(2) {
                Some(&mut pool)
            } else {
                None
            },
        );
        let mesh = match &result {
            Some(ClippedSpriteObject {
                object_type:
                    EditablePayload::TexturedMesh {
                        vertices: renderer::TexturedMeshVertices::Transient(out),
                        ..
                    },
                ..
            }) => Some(out),
            _ => None,
        };
        if case.is_multiple_of(2) {
            assert_eq!(
                pool.len(),
                if mesh.is_some() { 1 } else { 2 },
                "pool use case={case}"
            );
            if let Some(out) = mesh {
                assert_eq!(out.as_ptr(), saved_ptr);
                assert_eq!(out.capacity(), saved_capacity);
            } else {
                assert_eq!(pool[1].as_ptr(), saved_ptr);
                assert!(pool[1].is_empty());
            }
        } else {
            assert_eq!(pool.len(), 2);
        }
        assert_eq!(pool[0], vec![Vertex::default()]);
        let actual = bits(result);
        assert_eq!(expected.0, actual.0, "metadata case={case},variant={index}");
        assert_eq!(
            expected.1.len(),
            actual.1.len(),
            "length case={case},variant={index}"
        );
        for (field, (a, b)) in expected.1.iter().zip(&actual.1).enumerate() {
            if a != b && f32::from_bits(*a).is_nan() && f32::from_bits(*b).is_nan() {
                nan_diffs[index] += 1;
            } else {
                assert_eq!(
                    a, b,
                    "field={field},case={case},variant={index},sprite={s:?}"
                );
            }
        }
        *compared += expected.1.len();
    }
}

fn next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    *state
}

#[test]
fn rotated_sprite_clipping_preserves_geometry_and_storage() {
    let mut cases = 0;
    let mut nan_diffs = [0; 2];
    let mut compared = 0;
    let mut state = 7;
    let rectangles = [
        [-1000.0, 1000.0, -1000.0, 1000.0],
        [-4.0, 4.0, -4.0, 4.0],
        [2000.0, 3000.0, 2000.0, 3000.0],
        [-0.0, 0.0, -0.0, 0.0],
        [
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
        ],
    ];
    for angle in [
        0.0,
        0.0000009,
        0.0000011,
        0.03,
        0.5,
        1.0,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        std::f32::consts::TAU,
    ] {
        for r in rectangles {
            for i in 0..64 {
                compare(
                    sprite(i, angle),
                    rect(r),
                    cases,
                    &mut nan_diffs,
                    &mut compared,
                );
                cases += 1;
            }
        }
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
    for i in 0..100_000 {
        let angle = (next(&mut state) >> 8) as f32 / 16777216.0 * 6.3;
        let mut s = sprite(i % 64, angle);
        for (field, v) in fields(&mut s).enumerate() {
            if i % 4 == 0 {
                *v = f32::from_bits(next(&mut state));
            } else if i % 4 == 1 {
                *v = special[(next(&mut state) % special.len() as u32) as usize];
            } else if !(6..8).contains(&field) {
                *v = ((next(&mut state) >> 8) as f32 / 16777216.0 - 0.5) * 32.0;
            }
        }
        let r = if i % 7 == 0 {
            let mut r = [0.0; 4];
            for x in &mut r {
                *x = special[(next(&mut state) % special.len() as u32) as usize];
            }
            r
        } else {
            rectangles[i % 5]
        };
        compare(s, rect(r), cases, &mut nan_diffs, &mut compared);
        cases += 1;
    }
    println!(
        "{cases} sprites / {compared} compared float fields: all non-NaN bits and metadata match. NaN payload/sign differences runtime={}, portable={}",
        nan_diffs[0], nan_diffs[1]
    );
}

#[allow(
    clippy::collapsible_if,
    reason = "keep the timed recycling helper identical to the archived baseline"
)]
fn recycle(out: Option<ClippedSpriteObject>, pool: &mut Pool, reuse: bool) {
    if reuse {
        if let Some(ClippedSpriteObject {
            object_type:
                EditablePayload::TexturedMesh {
                    vertices: renderer::TexturedMeshVertices::Transient(mut v),
                    ..
                },
            ..
        }) = out
        {
            v.clear();
            pool.push(v);
        }
    }
}
#[test]
#[ignore = "release CPU benchmark for archived before/after binaries"]
fn benchmark_sprite_clipping() {
    bench::pin_thread();
    let rotation = std::env::var("SPRITE_CLIP_BENCH_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let mut cases = Vec::new();
    for (shape, angle) in [("rotated", 0.47), ("axis", 0.0)] {
        for (label, r) in [
            ("contained", [-1000.0, 1000.0, -1000.0, 1000.0]),
            ("partial", [-4.0, 4.0, -4.0, 4.0]),
            ("rejected", [2000.0, 3000.0, 2000.0, 3000.0]),
        ] {
            cases.push((
                format!("{shape}-{label}"),
                (0..64).map(|i| sprite(i, angle)).collect::<Vec<_>>(),
                rect(r),
                true,
            ));
        }
    }
    for (label, angle, r, reuse) in [
        ("rotated-edge", 0.47, [-9.0, 9.0, -9.0, 9.0], true),
        ("rotated-thin", 0.03, [-0.01, 0.01, -20.0, 20.0], true),
        ("rotated-partial-fresh", 0.47, [-4.0, 4.0, -4.0, 4.0], false),
        (
            "rotated-contained-fresh",
            0.47,
            [-1000.0, 1000.0, -1000.0, 1000.0],
            false,
        ),
    ] {
        cases.push((
            label.into(),
            (0..64)
                .map(|i| sprite(i, angle + (i % 7) as f32 * 0.001))
                .collect(),
            rect(r),
            reuse,
        ));
    }
    let mixed = (0..64)
        .map(|i| sprite(i, if i % 8 == 0 { 0.47 } else { 0.0 }))
        .collect();
    cases.push((
        "mixed-partial".into(),
        mixed,
        rect([-4.0, 4.0, -4.0, 4.0]),
        true,
    ));
    for (label, angle) in [
        ("quarter-turn-partial", std::f32::consts::FRAC_PI_2),
        ("half-turn-partial", std::f32::consts::PI),
    ] {
        cases.push((
            label.into(),
            (0..64).map(|i| sprite(i, angle)).collect(),
            rect([-4.0, 4.0, -4.0, 4.0]),
            true,
        ));
    }

    for j in 0..cases.len() {
        let (name, sprites, r, reuse) = &cases[(j + rotation) % cases.len()];
        let iterations = 1024;
        let operations = iterations * sprites.len();
        let mut pools: [Pool; 2] = std::array::from_fn(|_| vec![Vec::with_capacity(18)]);
        for round in 0..10 {
            for k in 0..2 {
                let mode = (round + k + rotation) % 2;
                let f = black_box([production as Clip, portable][mode]);
                let p = &mut pools[mode];
                let start_cycles = bench::cycles();
                let start = Instant::now();
                for _ in 0..iterations {
                    for s in sprites {
                        let out = black_box(f(
                            black_box(*s),
                            black_box(*r),
                            if *reuse {
                                Some(black_box(&mut *p))
                            } else {
                                None
                            },
                        ));
                        black_box(&out);
                        recycle(out, p, *reuse);
                    }
                }
                let ns = start.elapsed().as_nanos() as f64 / operations as f64;
                let cycles = (bench::cycles() - start_cycles) as f64 / operations as f64;
                println!("sprite-clip,{name},{mode},{round},{operations},{cycles:.4},{ns:.4}");
            }
        }
    }
}
