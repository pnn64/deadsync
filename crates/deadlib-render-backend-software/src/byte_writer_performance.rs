use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "byte_writer_original.rs"]
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

#[inline(always)]
fn write_colors(colors: &[[f32; 4]], mode: BlendMode, pixels: &mut [u32], old: bool) {
    #[inline(always)]
    fn run(
        colors: &[[f32; 4]],
        mode: BlendMode,
        mut writer: impl FnMut(usize, [f32; 4], BlendMode),
    ) {
        for (i, &color) in colors.iter().enumerate() {
            writer(i, color, mode);
        }
    }
    if old {
        run(colors, mode, original::byte_writer(pixels));
    } else {
        run(colors, mode, byte_writer(pixels));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "fma")]
unsafe fn write_colors_fma(colors: &[[f32; 4]], mode: BlendMode, pixels: &mut [u32], old: bool) {
    write_colors(colors, mode, pixels, old);
}

#[test]
fn byte_writer_preserves_exact_pixels_in_portable_and_fma_contexts() {
    let mut seed = 0xa193_2468;
    let values = [
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
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
    let mut colors: Vec<[f32; 4]> = (0..16_384)
        .map(|i| {
            std::array::from_fn(|lane| match i % 3 {
                0 => f32::from_bits(next(&mut seed)),
                1 => (next(&mut seed) >> 8) as f32 * (1.0 / 16_777_216.0),
                _ => values[(i + lane) % values.len()],
            })
        })
        .collect();
    // Include both sides of every byte-quantization boundary.
    for i in 0..255 {
        let boundary = (i as f32 + 0.5) / 255.0;
        for value in [boundary.next_down(), boundary, boundary.next_up()] {
            colors.push([value; 4]);
            colors.push([value, 1.0 - value, value, 1.0]);
        }
    }
    let initial: Vec<_> = colors.iter().map(|_| next(&mut seed)).collect();
    let mut contexts = 0;
    for fma in [false, true] {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if fma && !std::arch::is_x86_feature_detected!("fma") {
            continue;
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        if fma {
            continue;
        }
        for mode in MODES {
            let mut expected = initial.clone();
            let mut actual = initial.clone();
            if fma {
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                // SAFETY: this context is reached only after detecting FMA support.
                unsafe {
                    write_colors_fma(&colors, mode, &mut expected, true);
                    write_colors_fma(&colors, mode, &mut actual, false);
                }
            } else {
                write_colors(&colors, mode, &mut expected, true);
                write_colors(&colors, mode, &mut actual, false);
            }
            assert_eq!(actual, expected, "{mode:?}, FMA context: {fma}");
        }
        contexts += 1;
    }
    println!(
        "{} exact writer pixels match across {contexts} contexts",
        colors.len() * MODES.len() * contexts
    );
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
fn draw(
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
            rasterize_triangle_with_inv(
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
    draw(c, t, m, &mut byte_writer(p))
}

#[inline(never)]
fn reference(c: &Case, t: Texels<'_>, m: &[deadlib_render_core::MeshVertex], p: &mut [u32]) -> u32 {
    draw(c, t, m, &mut original::byte_writer(p))
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
fn byte_writer_preserves_textured_draws_and_meshes() {
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
#[ignore = "release CPU benchmark"]
fn benchmark_byte_writer_callers() {
    bench::pin_thread();
    let rotation = std::env::var("BYTE_WRITER_ROTATION")
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
    for i in 0..cases.len() {
        let c = cases[(i + rotation) % cases.len()];
        let label = if c.start == c.end {
            "empty-mesh"
        } else {
            match c.kind {
                Kind::Mesh => "mesh",
                Kind::Texture(SamplerFilter::Nearest, _, _) => "nearest",
                Kind::Texture(SamplerFilter::Linear, false, _) => "linear",
                Kind::Texture(SamplerFilter::Linear, true, _) => "mask",
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
                    "byte-writer,{label}-{}-{:?},{variant},{round},{n},{cpu:.4},{ns:.4}",
                    c.size, c.mode
                );
            }
        }
    }
}
