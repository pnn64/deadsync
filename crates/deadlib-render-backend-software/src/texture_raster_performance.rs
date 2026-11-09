use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "texture_raster_original.rs"]
mod original;

const MODES: [BlendMode; 5] = [
    BlendMode::Alpha,
    BlendMode::Add,
    BlendMode::Multiply,
    BlendMode::Subtract,
    BlendMode::NoEffect,
];

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Mesh,
    Texture(SamplerFilter, bool, bool),
}

#[derive(Clone, Copy, Debug)]
struct Case {
    kind: Kind,
    size: usize,
    mode: BlendMode,
    tint: [f32; 4],
    wrap: SamplerWrap,
    start: usize,
    end: usize,
}

type Draw = fn(&Case, Texels<'_>, &[deadlib_render_core::MeshVertex], &mut [u32]) -> u32;

#[inline(always)]
fn draw<const OLD: bool>(
    case: &Case,
    image: Texels<'_>,
    mesh: &[deadlib_render_core::MeshVertex],
    writer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) -> u32 {
    let &Case {
        kind,
        size,
        mode,
        tint,
        wrap,
        start,
        end,
    } = case;
    match kind {
        Kind::Mesh => rasterize_mesh_triangles(
            &Matrix4::IDENTITY,
            mesh,
            mode,
            size,
            size,
            start,
            end,
            writer,
        ),
        Kind::Texture(filter, mask, opaque) => {
            let v = [
                ScreenVertex {
                    x: 0.2,
                    y: 0.3,
                    u: -0.2,
                    v: 0.1,
                },
                ScreenVertex {
                    x: size as f32 - 0.1,
                    y: 1.1,
                    u: 1.2,
                    v: -0.3,
                },
                ScreenVertex {
                    x: 0.1,
                    y: size as f32 - 0.2,
                    u: 0.1,
                    v: 1.3,
                },
            ];
            let inv = triangle_inv_denom(&v[0], &v[1], &v[2]).unwrap();
            let raster = if OLD {
                original::rasterize_triangle_with_inv
            } else {
                rasterize_triangle_with_inv
            };
            raster(
                &v[0],
                &v[1],
                &v[2],
                inv,
                tint,
                mask,
                mode,
                image,
                SamplerDesc {
                    filter,
                    wrap,
                    mipmaps: false,
                },
                opaque,
                size,
                size,
                start,
                end,
                writer,
            );
            3
        }
    }
}

#[inline(never)]
fn production(
    c: &Case,
    t: Texels<'_>,
    m: &[deadlib_render_core::MeshVertex],
    p: &mut [u32],
) -> u32 {
    draw::<false>(c, t, m, &mut byte_writer(p))
}

#[inline(never)]
fn reference(c: &Case, t: Texels<'_>, m: &[deadlib_render_core::MeshVertex], p: &mut [u32]) -> u32 {
    draw::<true>(c, t, m, &mut byte_writer(p))
}

fn mesh() -> Vec<deadlib_render_core::MeshVertex> {
    let colors = [
        [0.2, 0.6, 0.9, 0.7],
        [0.8, 0.3, 0.1, 0.9],
        [0.4, 0.9, 0.6, 0.8],
    ];
    [
        [-0.9, -0.9],
        [0.9, -0.9],
        [-0.9, 0.9],
        [0.9, -0.9],
        [0.9, 0.9],
        [-0.9, 0.9],
    ]
    .into_iter()
    .enumerate()
    .map(|(i, pos)| deadlib_render_core::MeshVertex {
        pos,
        color: colors[i % 3],
    })
    .collect()
}

fn image() -> RgbaImage {
    RgbaImage::from_fn(8, 8, |x, y| {
        image::Rgba([
            (x * 37 + y * 13) as u8,
            (x * 5 + y * 41) as u8,
            (x * 23 + y * 3) as u8,
            (32 + x * 17 + y * 11) as u8,
        ])
    })
}

#[test]
fn textured_draws_preserve_pixels_and_stripes() {
    let image = image();
    let mesh = mesh();
    let mut seed = 0x7186_4309;
    let half: Vec<u64> = (0..64)
        .map(|_| u64::from(next(&mut seed)) | (u64::from(next(&mut seed)) << 32))
        .collect();
    let mut kinds = vec![Kind::Mesh];
    for filter in [SamplerFilter::Nearest, SamplerFilter::Linear] {
        for mask in [false, true] {
            for opaque in [false, true] {
                kinds.push(Kind::Texture(filter, mask, opaque));
            }
        }
    }
    let tints = [
        [0.3, 0.6, 0.9, 0.7],
        [1.0; 4],
        [f32::INFINITY, -f32::MAX, f32::NAN, 0.5],
    ];
    let mut cases = 0;
    for size in [2, 8, 17] {
        for kind in &kinds {
            for mode in MODES {
                for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
                    for tint in tints {
                        for (start, end) in
                            [(0, size), (0, size / 2), (size / 2, size), (size, size)]
                        {
                            let c = Case {
                                kind: *kind,
                                size,
                                mode,
                                tint,
                                wrap,
                                start,
                                end,
                            };
                            for half_texels in [false, true] {
                                let texels = Texels {
                                    half: if half_texels { &half } else { &[] },
                                    ..(&image).into()
                                };
                                let mut expected: Vec<_> =
                                    (0..size * (end - start)).map(|_| next(&mut seed)).collect();
                                let mut actual = expected.clone();
                                assert_eq!(
                                    production(&c, texels, &mesh, &mut actual),
                                    reference(&c, texels, &mesh, &mut expected)
                                );
                                assert_eq!(actual, expected, "{c:?}, half texture: {half_texels}");
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("{cases} mesh/textured draw/stripe cases match original byte writer");
}

#[test]
fn textured_triangles_preserve_half_pixels_and_callback_bits() {
    let image = image();
    let mut seed = 0x1397_4682;
    let half: Vec<u64> = (0..64)
        .map(|_| u64::from(next(&mut seed)) | (u64::from(next(&mut seed)) << 32))
        .collect();
    let special = [
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
        1.0,
        0.5,
    ];
    let mut fields = 0;
    let mut pixels = 0;
    for case in 0..4096 {
        let size = [2, 8, 17][case % 3];
        let mut v = [
            ScreenVertex {
                x: -0.2,
                y: -0.3,
                u: -0.2,
                v: 0.1,
            },
            ScreenVertex {
                x: size as f32 - 0.1,
                y: 1.1,
                u: 1.2,
                v: -0.3,
            },
            ScreenVertex {
                x: 0.1,
                y: size as f32 - 0.2,
                u: 0.1,
                v: 1.3,
            },
        ];
        let mut tint = [0.3, 0.6, 0.9, 0.7];
        if (case / 16) % 4 != 0 {
            for vertex in &mut v {
                for coord in [&mut vertex.u, &mut vertex.v] {
                    *coord = match (case / 16) % 4 {
                        1 => (next(&mut seed) >> 8) as f32 / 1_048_576.0 - 8.0,
                        2 => special[next(&mut seed) as usize % special.len()],
                        _ => f32::from_bits(next(&mut seed)),
                    };
                }
            }
        }
        match case % 5 {
            1 => tint = [1.0; 4],
            2 => tint = std::array::from_fn(|_| special[next(&mut seed) as usize % special.len()]),
            3 => tint = std::array::from_fn(|_| f32::from_bits(next(&mut seed))),
            _ => (),
        }
        if case % 7 == 0 {
            v.swap(0, 2);
        }
        if case % 13 == 0 {
            v[0].x = f32::from_bits(next(&mut seed));
        }
        let inv = triangle_inv_denom(&v[0], &v[1], &v[2]).unwrap_or(0.0);
        let mode = MODES[case % MODES.len()];
        let sampler = SamplerDesc {
            filter: if case & 1 == 0 {
                SamplerFilter::Nearest
            } else {
                SamplerFilter::Linear
            },
            wrap: if case & 2 == 0 {
                SamplerWrap::Clamp
            } else {
                SamplerWrap::Repeat
            },
            mipmaps: false,
        };
        let mask = case & 4 != 0;
        let opaque = case & 8 != 0;
        let (start, end) =
            [(0, size), (0, size / 2), (size / 2, size), (size, size)][(case / 64) % 4];
        for half_texels in [false, true] {
            let texels = Texels {
                half: if half_texels { &half } else { &[] },
                ..(&image).into()
            };
            let mut traces = [Vec::new(), Vec::new()];
            for (old, trace) in traces.iter_mut().enumerate() {
                let mut writer = |index, color: [f32; 4], blend| {
                    trace.push((index, color.map(f32::to_bits), blend))
                };
                let raster = if old == 1 {
                    original::rasterize_triangle_with_inv
                } else {
                    rasterize_triangle_with_inv
                };
                raster(
                    &v[0],
                    &v[1],
                    &v[2],
                    inv,
                    tint,
                    mask,
                    mode,
                    texels,
                    sampler,
                    opaque,
                    size,
                    size,
                    start,
                    end,
                    &mut writer,
                );
            }
            assert_eq!(
                traces[0], traces[1],
                "callback case {case}, half texture: {half_texels}"
            );
            fields += traces[0].len() * 4;
            for alpha in [false, true] {
                let initial: Vec<u64> = (0..size * (end - start))
                    .map(|_| u64::from(next(&mut seed)) | (u64::from(next(&mut seed)) << 32))
                    .collect();
                let mut actual = initial.clone();
                let mut expected = initial;
                rasterize_triangle_with_inv(
                    &v[0],
                    &v[1],
                    &v[2],
                    inv,
                    tint,
                    mask,
                    mode,
                    texels,
                    sampler,
                    opaque,
                    size,
                    size,
                    start,
                    end,
                    &mut half_writer(&mut actual, alpha),
                );
                original::rasterize_triangle_with_inv(
                    &v[0],
                    &v[1],
                    &v[2],
                    inv,
                    tint,
                    mask,
                    mode,
                    texels,
                    sampler,
                    opaque,
                    size,
                    size,
                    start,
                    end,
                    &mut half_writer(&mut expected, alpha),
                );
                assert_eq!(
                    actual, expected,
                    "half attachment case {case}, half texture: {half_texels}, alpha: {alpha}"
                );
                pixels += actual.len();
            }
        }
    }
    println!("{pixels} exact half pixels and {fields} callback float fields match original");
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_texture_raster() {
    bench::pin_thread();
    let rotation = std::env::var("TEXTURE_RASTER_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let image = image();
    let mesh = mesh();
    let mut cases = Vec::new();
    for size in [2, 8, 32, 128] {
        for kind in [
            Kind::Mesh,
            Kind::Texture(SamplerFilter::Nearest, false, false),
            Kind::Texture(SamplerFilter::Linear, false, false),
            Kind::Texture(SamplerFilter::Linear, true, false),
            Kind::Texture(SamplerFilter::Nearest, false, true),
            Kind::Texture(SamplerFilter::Linear, true, true),
        ] {
            for mode in [BlendMode::Alpha, BlendMode::Add, BlendMode::Multiply] {
                cases.push(Case {
                    kind,
                    size,
                    mode,
                    tint: [0.3, 0.6, 0.9, 0.7],
                    wrap: SamplerWrap::Repeat,
                    start: 0,
                    end: size,
                });
            }
        }
    }
    cases.push(Case { end: 0, ..cases[0] });
    cases.push(Case { end: 0, ..cases[3] });
    for i in 0..cases.len() {
        let c = cases[(i + rotation) % cases.len()];
        let label = if c.start == c.end {
            match c.kind {
                Kind::Mesh => "empty-mesh",
                Kind::Texture(..) => "empty-texture",
            }
        } else {
            match c.kind {
                Kind::Mesh => "mesh",
                Kind::Texture(SamplerFilter::Nearest, _, true) => "nearest-opaque",
                Kind::Texture(SamplerFilter::Nearest, _, false) => "nearest",
                Kind::Texture(SamplerFilter::Linear, false, _) => "linear",
                Kind::Texture(SamplerFilter::Linear, true, true) => "mask-opaque",
                Kind::Texture(SamplerFilter::Linear, true, false) => "mask",
            }
        };
        let n = if c.start == c.end {
            100_000
        } else {
            (100_000 / (c.size * c.size)).max(16)
        };
        let mut buffers = [vec![0; c.size * c.size], vec![0; c.size * c.size]];
        for round in 0..10 {
            for k in 0..2 {
                let variant = (round + k + rotation) % 2;
                let run = black_box([production as Draw, reference][variant]);
                let pixels = &mut buffers[variant];
                let cycles = bench::cycles();
                let start = Instant::now();
                for _ in 0..n {
                    pixels.fill(0x9f3f5f7f);
                    black_box(run(
                        black_box(&c),
                        (&image).into(),
                        black_box(&mesh),
                        black_box(pixels),
                    ));
                    black_box(&*pixels);
                }
                let ns = start.elapsed().as_nanos() as f64 / n as f64;
                let cpu = (bench::cycles() - cycles) as f64 / n as f64;
                println!(
                    "texture-raster,{label}-{}-{:?},{variant},{round},{n},{cpu:.4},{ns:.4}",
                    c.size, c.mode
                );
            }
        }
    }
}
