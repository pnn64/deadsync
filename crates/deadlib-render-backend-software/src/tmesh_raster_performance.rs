use super::*;
use deadlib_render_core::{DepthMode, TexturedMeshVertex};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "tmesh_raster_original.rs"]
mod original;

const MODES: [BlendMode; 5] = [
    BlendMode::Alpha,
    BlendMode::Add,
    BlendMode::Multiply,
    BlendMode::Subtract,
    BlendMode::NoEffect,
];

#[derive(Clone, Copy, Debug)]
enum Path {
    Prepared,
    Direct,
    Control,
}

#[derive(Clone, Copy, Debug)]
struct Case {
    label: &'static str,
    path: Path,
    size: usize,
    blend: BlendMode,
    filter: SamplerFilter,
    wrap: SamplerWrap,
    mask: bool,
    opaque: bool,
    depth: bool,
    mode: DepthMode,
    unorm16: bool,
    clear: f32,
    start: usize,
    end: usize,
}

fn case(size: usize) -> Case {
    Case {
        label: "prepared-nearest",
        path: Path::Prepared,
        size,
        blend: BlendMode::Alpha,
        filter: SamplerFilter::Nearest,
        wrap: SamplerWrap::Repeat,
        mask: false,
        opaque: false,
        depth: false,
        mode: DepthMode::TestWrite,
        unorm16: false,
        clear: 1.0,
        start: 0,
        end: size,
    }
}

struct Fixture {
    mesh: Vec<TexturedMeshVertex>,
    triangles: Vec<PreparedTriangle<ScreenVertexTexColor>>,
    mvp: Matrix4,
}

fn fixture(size: usize) -> Fixture {
    let mesh: Vec<_> = [
        [-0.9, -0.9],
        [0.9, -0.9],
        [-0.9, 0.9],
        [0.9, -0.9],
        [0.9, 0.9],
        [-0.9, 0.9],
    ]
    .into_iter()
    .enumerate()
    .map(|(i, p)| TexturedMeshVertex {
        pos: [p[0], p[1], (i % 3) as f32 * 0.25 - 0.25],
        uv: [p[0] * 0.7 + 0.5, p[1] * -0.7 + 0.5],
        color: [
            [0.2, 0.6, 0.9, 0.7],
            [0.8, 0.3, 0.1, 0.9],
            [0.4, 0.9, 0.6, 0.8],
        ][i % 3],
        ..Default::default()
    })
    .collect();
    let mvp = Matrix4::IDENTITY;
    let mut triangles = Vec::new();
    prepare_tmesh_triangles(
        &mut triangles,
        0,
        &mvp,
        [1.0; 4],
        [1.0; 2],
        [0.0; 2],
        [0.0; 2],
        &mesh,
        size,
        size,
        0.0,
    )
    .unwrap();
    Fixture {
        mesh,
        triangles,
        mvp,
    }
}

fn image() -> RgbaImage {
    RgbaImage::from_fn(8, 8, |x, y| {
        image::Rgba([
            (x * 37 + y * 13) as u8,
            (x * 5 + y * 41) as u8,
            (x * 23 + y * 3) as u8,
            if (x + y) % 4 == 0 {
                0
            } else {
                (32 + x * 17 + y * 11) as u8
            },
        ])
    })
}

#[inline(always)]
fn draw<const OLD: bool>(
    c: &Case,
    f: &Fixture,
    image: Texels<'_>,
    writer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    depth: &mut DepthRows<'_>,
) -> u32 {
    let sampler = SamplerDesc {
        filter: c.filter,
        wrap: c.wrap,
        mipmaps: false,
    };
    match c.path {
        Path::Prepared => {
            let raster = if OLD {
                original::rasterize_prepared_tmesh
            } else {
                rasterize_prepared_tmesh
            };
            raster(
                &f.triangles,
                c.mask,
                c.blend,
                image,
                sampler,
                c.opaque,
                c.start,
                c.end,
                writer,
                c.size,
                depth,
            );
            0
        }
        Path::Direct => {
            let raster = if OLD {
                original::rasterize_textured_mesh_triangles
            } else {
                rasterize_textured_mesh_triangles
            };
            raster(
                &f.mvp, &f.mesh, [1.0; 4], [1.0; 2], [0.0; 2], [0.0; 2], c.mask, c.blend, image,
                sampler, c.opaque, c.size, c.size, c.start, c.end, writer, 0.0, depth,
            )
        }
        Path::Control => {
            let v = [
                ScreenVertex {
                    x: 0.2,
                    y: 0.3,
                    u: -0.2,
                    v: 0.1,
                },
                ScreenVertex {
                    x: c.size as f32 - 0.1,
                    y: 1.1,
                    u: 1.2,
                    v: -0.3,
                },
                ScreenVertex {
                    x: 0.1,
                    y: c.size as f32 - 0.2,
                    u: 0.1,
                    v: 1.3,
                },
            ];
            let inv = triangle_inv_denom(&v[0], &v[1], &v[2]).unwrap();
            rasterize_triangle_with_inv(
                &v[0],
                &v[1],
                &v[2],
                inv,
                [0.3, 0.6, 0.9, 0.7],
                c.mask,
                c.blend,
                image,
                sampler,
                c.opaque,
                c.size,
                c.size,
                c.start,
                c.end,
                writer,
            );
            0
        }
    }
}

type Draw = fn(&Case, &Fixture, Texels<'_>, &mut [u32], &mut [f32]) -> u32;

#[inline(never)]
fn production(c: &Case, f: &Fixture, t: Texels<'_>, p: &mut [u32], d: &mut [f32]) -> u32 {
    draw::<false>(
        c,
        f,
        t,
        &mut byte_writer(p),
        &mut DepthRows {
            pixels: d,
            unorm16: c.unorm16,
            mode: c.mode,
        },
    )
}
#[inline(never)]
fn reference(c: &Case, f: &Fixture, t: Texels<'_>, p: &mut [u32], d: &mut [f32]) -> u32 {
    draw::<true>(
        c,
        f,
        t,
        &mut byte_writer(p),
        &mut DepthRows {
            pixels: d,
            unorm16: c.unorm16,
            mode: c.mode,
        },
    )
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

fn trace_writer(
    trace: &mut Vec<(usize, [u32; 4], BlendMode)>,
) -> impl FnMut(usize, [f32; 4], BlendMode) + '_ {
    #[inline(always)]
    move |i, c: [f32; 4], b| trace.push((i, c.map(f32::to_bits), b))
}

fn exact<T: PartialEq + std::fmt::Debug>(actual: &[T], expected: &[T], _: bool) -> usize {
    assert_eq!(actual, expected);
    0
}

fn half_pixels(actual: &[u64], expected: &[u64], strict: bool) -> usize {
    assert_eq!(actual.len(), expected.len());
    let mut differences = 0;
    for (&a, &b) in actual.iter().zip(expected) {
        for lane in 0..4 {
            let x = (a >> (lane * 16)) as u16;
            let y = (b >> (lane * 16)) as u16;
            if x != y {
                // Even two portable baseline copies can choose different NaN signs.
                assert!(!strict && x ^ y == 0x8000 && decode_half(x).is_nan());
                differences += 1;
            }
        }
    }
    differences
}

fn callback_fields(
    actual: &[(usize, [u32; 4], BlendMode)],
    expected: &[(usize, [u32; 4], BlendMode)],
    strict: bool,
) -> usize {
    assert_eq!(actual.len(), expected.len());
    let mut differences = 0;
    for ((i, a, blend), (j, b, old_blend)) in actual.iter().zip(expected) {
        assert_eq!((i, blend), (j, old_blend));
        for (&x, &y) in a.iter().zip(b) {
            if x != y {
                assert!(!strict && f32::from_bits(x).is_nan() && f32::from_bits(y).is_nan());
                differences += 1;
            }
        }
    }
    differences
}

fn verify(c: &Case, f: &Fixture, t: Texels<'_>, seed: &mut u32, totals: &mut [usize; 8]) {
    let len = c.size * (c.end - c.start);
    // Require every bit, including NaN signs, in the finite shading domain.
    // Track baseline NaN-encoding variation separately for exceptional inputs.
    let strict = t.half.is_empty()
        && (matches!(c.path, Path::Direct)
            || f.triangles.iter().all(|triangle| {
                triangle.vertices.iter().all(|v| {
                    v.color
                        .into_iter()
                        .chain([v.u, v.v, v.z])
                        .all(|x| x.abs() <= f32::MAX / 4.0)
                })
            }));
    let initial_depth: Vec<_> = if c.depth {
        (0..len)
            .map(|i| match i % 5 {
                0 => f32::from_bits(next(seed)),
                1 => 0.0,
                2 => -0.0,
                3 => 0.5,
                _ => 1.0,
            })
            .collect()
    } else {
        Vec::new()
    };
    macro_rules! compare {
        ($pixels:expr, $writer:ident, $equal:ident $(, $alpha:expr)?) => {{
            let initial = $pixels;
            let mut expected = initial.clone();
            let mut actual = initial;
            let mut old_depth = initial_depth.clone();
            let mut new_depth = initial_depth.clone();
            let old = draw::<true>(c, f, t, &mut $writer(&mut expected $(, $alpha)?),
                &mut DepthRows { pixels: &mut old_depth, unorm16: c.unorm16, mode: c.mode });
            let new = draw::<false>(c, f, t, &mut $writer(&mut actual $(, $alpha)?),
                &mut DepthRows { pixels: &mut new_depth, unorm16: c.unorm16, mode: c.mode });
            assert_eq!(new, old, "draw count: {c:?}");
            let differences = $equal(&actual, &expected, strict);
            if stringify!($writer) == "half_writer" { totals[5] += differences; }
            else { totals[6] += differences; }
            assert_eq!(new_depth.iter().map(|z| z.to_bits()).collect::<Vec<_>>(),
                old_depth.iter().map(|z| z.to_bits()).collect::<Vec<_>>(), "depth bits: {c:?}");
            totals[3] += old_depth.len();
            actual.len()
        }};
    }
    let bytes: Vec<_> = (0..len).map(|_| next(seed)).collect();
    let halves: Vec<_> = (0..len)
        .map(|_| u64::from(next(seed)) | (u64::from(next(seed)) << 32))
        .collect();
    totals[0] += compare!(bytes, byte_writer, exact);
    totals[1] += compare!(halves.clone(), half_writer, half_pixels, true);
    totals[1] += compare!(halves, half_writer, half_pixels, false);
    totals[2] += compare!(Vec::new(), trace_writer, callback_fields) * 4;
    totals[4] += 1;
    if strict {
        totals[7] += 2 * len;
    }
}

#[test]
fn textured_meshes_preserve_pixels_depth_and_callback_bits() {
    let image = image();
    let mut seed = 0x1397_4682;
    let half: Vec<_> = (0..64)
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
        (1.0_f32 / 256.0).next_down(),
        1.0 / 256.0,
        (1.0_f32 / 256.0).next_up(),
        0.5,
        1.0,
    ];
    let mut totals = [0; 8];
    for i in 0..4096 {
        let size = [2, 8, 17][i % 3];
        let mut c = case(size);
        c.path = if (i / 3) % 3 == 0 {
            Path::Direct
        } else {
            Path::Prepared
        };
        c.blend = MODES[i % MODES.len()];
        c.filter = if i & 1 == 0 {
            SamplerFilter::Nearest
        } else {
            SamplerFilter::Linear
        };
        c.wrap = if i & 2 == 0 {
            SamplerWrap::Clamp
        } else {
            SamplerWrap::Repeat
        };
        c.mask = i & 4 != 0;
        c.opaque = i & 8 != 0;
        c.mode = DepthMode::ALL[(i / 16) % 6];
        c.unorm16 = i & 128 != 0;
        c.depth = i % 7 != 0;
        (c.start, c.end) =
            [(0, size), (0, size / 2), (size / 2, size), (size, size)][(i / 256) % 4];
        let mut f = fixture(size);
        for triangle in &mut f.triangles {
            if i % 13 == 0 {
                triangle.vertices.swap(0, 2);
                triangle.setup.inv_denom = -triangle.setup.inv_denom;
            }
            for v in &mut triangle.vertices {
                let mut value = || match (i / 32) % 4 {
                    0 => (next(&mut seed) >> 8) as f32 / 16_777_216.0,
                    1 => special[next(&mut seed) as usize % special.len()],
                    2 => f32::from_bits(next(&mut seed)),
                    _ => 0.5,
                };
                v.color = std::array::from_fn(|_| value());
                if i % 5 == 0 {
                    v.u = value();
                    v.v = value();
                    v.z = value();
                }
                if i % 17 == 0 {
                    v.x = value();
                }
            }
        }
        if i % 11 == 0 {
            for v in &mut f.mesh {
                v.pos[2] = special[next(&mut seed) as usize % special.len()];
            }
        }
        if i % 19 == 0 {
            f.mvp = Matrix4::ZERO;
        }
        for half_texture in [false, true] {
            let texels = Texels {
                half: if half_texture { &half } else { &[] },
                ..(&image).into()
            };
            verify(&c, &f, texels, &mut seed, &mut totals);
        }
    }
    println!(
        "{} draw cases; {} byte pixels; {} half pixels; {} callback fields; {} depth fields match original",
        totals[4], totals[0], totals[1], totals[2], totals[3]
    );
    println!(
        "{} strict-domain half pixels; fallback NaN encoding differences: {} half lanes, {} callback fields",
        totals[7], totals[5], totals[6]
    );
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_tmesh_raster() {
    bench::pin_thread();
    let rotation = std::env::var("TMESH_RASTER_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let image = image();
    let mut cases = Vec::new();
    for size in [2, 8, 32, 128] {
        for blend in [BlendMode::Alpha, BlendMode::Add, BlendMode::Multiply] {
            let c = Case {
                blend,
                ..case(size)
            };
            cases.extend([
                c,
                Case {
                    label: "prepared-linear",
                    filter: SamplerFilter::Linear,
                    ..c
                },
                Case {
                    label: "mask-unorm16",
                    filter: SamplerFilter::Linear,
                    mask: true,
                    depth: true,
                    unorm16: true,
                    ..c
                },
                Case {
                    label: "depth-reject",
                    filter: SamplerFilter::Linear,
                    depth: true,
                    clear: 0.0,
                    ..c
                },
                Case {
                    label: "direct-linear",
                    path: Path::Direct,
                    filter: SamplerFilter::Linear,
                    depth: true,
                    ..c
                },
                Case {
                    label: "sprite-control",
                    path: Path::Control,
                    filter: SamplerFilter::Linear,
                    ..c
                },
            ]);
        }
    }
    cases.push(Case {
        label: "empty-stripe",
        end: 0,
        ..cases[0]
    });
    for i in 0..cases.len() {
        let c = cases[(i + rotation) % cases.len()];
        let f = fixture(c.size);
        let n = if c.start == c.end {
            100_000
        } else {
            (100_000 / (c.size * c.size)).max(16)
        };
        let mut pixels = [vec![0; c.size * c.size], vec![0; c.size * c.size]];
        let mut depths = [
            vec![1.0; if c.depth { c.size * c.size } else { 0 }],
            vec![1.0; if c.depth { c.size * c.size } else { 0 }],
        ];
        for round in 0..10 {
            for k in 0..2 {
                let variant = (round + k + rotation) % 2;
                let run = black_box([production as Draw, reference][variant]);
                let p = &mut pixels[variant];
                let d = &mut depths[variant];
                let cycles = bench::cycles();
                let start = Instant::now();
                for _ in 0..n {
                    p.fill(0x9f3f5f7f);
                    d.fill(c.clear);
                    black_box(run(
                        black_box(&c),
                        black_box(&f),
                        (&image).into(),
                        black_box(p),
                        black_box(d),
                    ));
                    black_box(&*p);
                    black_box(&*d);
                }
                let ns = start.elapsed().as_nanos() as f64 / n as f64;
                let cpu = (bench::cycles() - cycles) as f64 / n as f64;
                println!(
                    "tmesh-raster,{}-{}-{:?},{variant},{round},{n},{cpu:.4},{ns:.4}",
                    c.label, c.size, c.blend
                );
            }
        }
    }
}
