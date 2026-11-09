use super::*;
use deadlib_render_core::{SpriteInstanceRaw, SpriteRun};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "sprite_prepare_original.rs"]
mod original;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Plain,
    Rotated,
    Perspective,
    Rejected,
    Overflow,
    ZeroWidth,
}

fn sprite(i: usize, kind: Kind) -> SpriteInstanceRaw {
    let rotated = !matches!(kind, Kind::Plain | Kind::ZeroWidth);
    let (sin, cos) = if rotated {
        0.3_f32.sin_cos()
    } else {
        (0.0, 1.0)
    };
    let mut s = SpriteInstanceRaw {
        center: [
            (i % 17) as f32 / 12.0 - 0.75,
            (i % 23) as f32 / 15.0 - 0.75,
            0.0,
            1.0,
        ],
        size: [0.2, 0.3],
        rot_sin_cos: [sin, cos],
        tint: [0.4, 0.7, 0.9, 0.8],
        uv_scale: [0.8, -0.7],
        uv_offset: [0.1, 0.9],
        local_offset: if rotated { [0.03, -0.02] } else { [0.0; 2] },
        local_offset_rot_sin_cos: [sin, cos],
        edge_fade: [0.0; 4],
        texture_mask: 0.0,
    };
    if matches!(kind, Kind::Overflow) {
        s.center[0] = f32::MAX;
        s.size[0] = f32::MAX;
        s.local_offset = [f32::MAX; 2];
        s.local_offset_rot_sin_cos = [1.0; 2];
    }
    s
}

fn projection(kind: Kind) -> Matrix4 {
    match kind {
        Kind::Rejected => Matrix4::ZERO,
        Kind::Perspective => Matrix4::from_cols(
            Vector4::new(1.0, 0.0, 0.0, 0.125),
            Vector4::new(0.0, 1.0, 0.0, 0.25),
            Vector4::new(0.0, 0.0, 1.0, 0.375),
            Vector4::new(0.0, 0.0, 0.0, 1.0),
        ),
        _ => Matrix4::IDENTITY,
    }
}

#[inline(never)]
fn vertices<const OLD: bool>(
    p: &Matrix4,
    s: &SpriteInstanceRaw,
    width: usize,
    height: usize,
) -> Option<[ScreenVertex; 4]> {
    let prepare = if OLD {
        original::prepare_sprite_vertices
    } else {
        prepare_sprite_vertices
    };
    prepare(
        p,
        s.center,
        s.size,
        s.rot_sin_cos,
        s.uv_scale,
        s.uv_offset,
        s.local_offset,
        s.local_offset_rot_sin_cos,
        width,
        height,
    )
}

fn bits(v: Option<[ScreenVertex; 4]>) -> Option<[[u32; 4]; 4]> {
    v.map(|v| v.map(|v| [v.x, v.y, v.u, v.v].map(f32::to_bits)))
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

#[test]
fn sprite_vertices_preserve_finite_bits_and_rejection() {
    let mut seed = 0x1397_4682;
    let palette = [
        0.0,
        -0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_1234),
        f32::from_bits(0xff80_1234),
        0.5,
        1.0,
    ];
    let mut fields = 0;
    let mut nan_differences = 0;
    let mut rejected = 0;
    for i in 0..100_000 {
        let kind = [
            Kind::Plain,
            Kind::Rotated,
            Kind::Perspective,
            Kind::Rejected,
            Kind::Overflow,
            Kind::ZeroWidth,
        ][i % 6];
        let mut s = sprite(i, kind);
        let mut p = projection(kind);
        if (i / 6) % 4 != 0 {
            let mut value = || match (i / 6) % 4 {
                1 => (next(&mut seed) >> 8) as f32 / 1_048_576.0 - 8.0,
                2 => palette[next(&mut seed) as usize % palette.len()],
                _ => f32::from_bits(next(&mut seed)),
            };
            s.center = std::array::from_fn(|_| value());
            s.size = [value(), value()];
            s.rot_sin_cos = [value(), value()];
            s.uv_scale = [value(), value()];
            s.uv_offset = [value(), value()];
            s.local_offset = [value(), value()];
            s.local_offset_rot_sin_cos = [value(), value()];
            if i % 7 == 0 {
                p = Matrix4::from_cols_array(&std::array::from_fn(|_| value()));
            }
        }
        let (width, height) = if matches!(kind, Kind::ZeroWidth) {
            (0, 17)
        } else {
            ([1, 8, 17, 128][(i / 24) % 4], 17)
        };
        let expected = bits(vertices::<true>(&p, &s, width, height));
        let actual = bits(vertices::<false>(&p, &s, width, height));
        assert_eq!(actual.is_some(), expected.is_some(), "case {i}, {kind:?}");
        match (actual, expected) {
            (Some(a), Some(b)) => {
                for (&x, &y) in a.iter().flatten().zip(b.iter().flatten()) {
                    if x != y {
                        assert!(
                            f32::from_bits(x).is_nan() && f32::from_bits(y).is_nan(),
                            "case {i}: {x:08x}/{y:08x}"
                        );
                        nan_differences += 1;
                    }
                    fields += 1;
                }
            }
            (None, None) => rejected += 1,
            _ => unreachable!(),
        }
    }
    println!(
        "100000 sprite cases; {fields} coordinate fields; {rejected} rejections; {nan_differences} NaN encoding differences"
    );
}

struct Textures(Texture);
impl TextureLookup for Textures {
    fn software_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        (handle == 7).then_some(&self.0)
    }
}
fn textures(filter: SamplerFilter, wrap: SamplerWrap) -> Textures {
    Textures(Texture {
        image: RgbaImage::from_fn(4, 4, |x, y| {
            image::Rgba([
                (x * 61) as u8,
                (y * 71) as u8,
                97,
                (32 + (x + y) * 31) as u8,
            ])
        }),
        sampler: SamplerDesc {
            filter,
            wrap,
            mipmaps: false,
        },
        opaque: false,
        yuv420: false,
        half_pixels: Vec::new(),
    })
}

fn frame(count: usize, kind: Kind) -> RenderFrame {
    RenderFrame {
        clear_color: [0.0; 4],
        render_targets: Vec::new(),
        cameras: vec![projection(kind)],
        sprite_instances: (0..count).map(|i| sprite(i, kind)).collect(),
        mesh_vertices: Vec::new(),
        tmesh_instances: Vec::new(),
        tmesh_geometries: Vec::new(),
        ops: vec![DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: count as u32,
            blend: BlendMode::Alpha,
            texture_handle: 7,
            camera: 0,
        })],
    }
}

struct Scratch {
    objects: Vec<PreparedObject>,
    meshes: Vec<PreparedTriangle<ScreenVertexColor>>,
    tmeshes: Vec<PreparedTriangle<ScreenVertexTexColor>>,
}
impl Scratch {
    fn new(count: usize) -> Self {
        Self {
            objects: Vec::with_capacity(count),
            meshes: Vec::new(),
            tmeshes: Vec::new(),
        }
    }
}

#[inline(never)]
fn prepare<const OLD: bool>(
    frame: &RenderFrame,
    textures: &Textures,
    width: usize,
    height: usize,
    scratch: &mut Scratch,
) -> u32 {
    let prepare = if OLD {
        original::prepare_objects
    } else {
        prepare_objects
    };
    prepare(
        frame.into(),
        Matrix4::IDENTITY,
        textures,
        width,
        height,
        &mut scratch.objects,
        &mut scratch.meshes,
        &mut scratch.tmeshes,
        false,
    )
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    rows: [u32; 2],
    inv: [Option<u32>; 2],
    vertices: [[u32; 4]; 4],
    tint: [u32; 4],
    mask: bool,
    blend: BlendMode,
    handle: TextureHandle,
}
fn snapshots(objects: &[PreparedObject]) -> Vec<Snapshot> {
    objects
        .iter()
        .map(|o| match o {
            PreparedObject::Sprite {
                rows,
                inv_denom,
                vertices,
                tint,
                texture_mask,
                blend,
                texture_handle,
            } => Snapshot {
                rows: [rows.start, rows.end],
                inv: inv_denom.map(|v| v.map(f32::to_bits)),
                vertices: bits(Some(*vertices)).unwrap(),
                tint: tint.map(f32::to_bits),
                mask: *texture_mask,
                blend: *blend,
                handle: *texture_handle,
            },
            _ => panic!("sprite fixture"),
        })
        .collect()
}

#[test]
fn sprite_frames_preserve_metadata_pixels_and_counts() {
    let mut cases = 0;
    let mut pixels = 0;
    for size in [8, 17] {
        for kind in [Kind::Plain, Kind::Rotated, Kind::Perspective] {
            for filter in [SamplerFilter::Nearest, SamplerFilter::Linear] {
                for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
                    let textures = textures(filter, wrap);
                    for blend in [
                        BlendMode::Alpha,
                        BlendMode::Add,
                        BlendMode::Multiply,
                        BlendMode::Subtract,
                        BlendMode::NoEffect,
                    ] {
                        let mut frame = frame(8, kind);
                        if let DrawOp::Sprite(ref mut run) = frame.ops[0] {
                            run.blend = blend;
                        }
                        let mut old = Scratch::new(8);
                        let mut new = Scratch::new(8);
                        let old_count = prepare::<true>(&frame, &textures, size, size, &mut old);
                        let new_count = prepare::<false>(&frame, &textures, size, size, &mut new);
                        assert_eq!(old_count, new_count);
                        assert_eq!(snapshots(&old.objects), snapshots(&new.objects));
                        for (start, end) in
                            [(0, size), (0, size / 2), (size / 2, size), (size, size)]
                        {
                            let len = size * (end - start);
                            macro_rules! draw {
                        ($buf:expr, $writer:ident $(, $alpha:expr)?) => {{
                            let initial = $buf; let mut expected = initial.clone(); let mut actual = initial;
                            let mut old_depth = vec![1.0; len]; let mut new_depth = old_depth.clone();
                            let a = draw_rows((&frame).into(), &old.objects, None, &old.meshes, &old.tmeshes, &textures, size, size, start, end,
                                &mut $writer(&mut expected $(, $alpha)?), old_count, &mut old_depth);
                            let b = draw_rows((&frame).into(), &new.objects, None, &new.meshes, &new.tmeshes, &textures, size, size, start, end,
                                &mut $writer(&mut actual $(, $alpha)?), new_count, &mut new_depth);
                            assert_eq!(a, b); assert_eq!(actual, expected);
                            assert_eq!(old_depth.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), new_depth.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
                            pixels += len;
                        }};
                    }
                            draw!(vec![0x9f3f5f7f; len], byte_writer);
                            draw!(vec![0x3c00_3800_3400_3000_u64; len], half_writer, true);
                            draw!(vec![0x3c00_3800_3400_3000_u64; len], half_writer, false);
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    println!("{cases} complete sprite frame/stripe cases; {pixels} exact attachment pixels");
}

type Vertices = fn(&Matrix4, &SpriteInstanceRaw, usize, usize) -> Option<[ScreenVertex; 4]>;
type Prepare = fn(&RenderFrame, &Textures, usize, usize, &mut Scratch) -> u32;

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_sprite_preparation() {
    bench::pin_thread();
    let rotation = std::env::var("SPRITE_PREPARE_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let textures = textures(SamplerFilter::Nearest, SamplerWrap::Clamp);
    let cases: Vec<_> = [1, 96, 3072]
        .into_iter()
        .flat_map(|count| {
            [
                Kind::Plain,
                Kind::Rotated,
                Kind::Perspective,
                Kind::Rejected,
                Kind::Overflow,
                Kind::ZeroWidth,
            ]
            .map(|kind| (count, kind))
        })
        .collect();
    for i in 0..cases.len() {
        let (count, kind) = cases[(i + rotation) % cases.len()];
        let frame = frame(count, kind);
        let width = if matches!(kind, Kind::ZeroWidth) {
            0
        } else {
            640
        };
        let n = (100_000 / count).max(16);
        for batch in [false, true] {
            let mut scratch = [Scratch::new(count), Scratch::new(count)];
            for round in 0..10 {
                for k in 0..2 {
                    let variant = (round + k + rotation) % 2;
                    let kernel =
                        black_box([vertices::<false> as Vertices, vertices::<true>][variant]);
                    let prepare =
                        black_box([prepare::<false> as Prepare, prepare::<true>][variant]);
                    let output = &mut scratch[variant];
                    let cycles = bench::cycles();
                    let start = Instant::now();
                    for _ in 0..n {
                        if batch {
                            black_box(prepare(
                                black_box(&frame),
                                black_box(&textures),
                                black_box(width),
                                480,
                                black_box(output),
                            ));
                            black_box(&output.objects);
                        } else {
                            for sprite in black_box(&frame.sprite_instances) {
                                black_box(kernel(
                                    black_box(&frame.cameras[0]),
                                    black_box(sprite),
                                    black_box(width),
                                    480,
                                ));
                            }
                        }
                    }
                    let ns = start.elapsed().as_nanos() as f64 / n as f64;
                    let cpu = (bench::cycles() - cycles) as f64 / n as f64;
                    println!(
                        "sprite-prepare,{}-{kind:?}-{count},{variant},{round},{n},{cpu:.4},{ns:.4}",
                        if batch { "frame" } else { "vertices" }
                    );
                }
            }
        }
    }
}
