use super::*;
use deadlib_render_core::{DepthMode, TexturedMeshVertex};

#[path = "environment_raster_original.rs"]
mod original;

#[derive(Clone, Copy, Debug)]
struct Case {
    size: usize,
    texgen: u8,
    mask: bool,
    reflection: bool,
    control: bool,
    perspective: bool,
    filter: SamplerFilter,
    wrap: SamplerWrap,
    blend: BlendMode,
    mode: DepthMode,
    unorm16: bool,
    depth: bool,
    start: usize,
    end: usize,
}

fn case(size: usize) -> Case {
    Case {
        size,
        texgen: 0,
        mask: false,
        reflection: false,
        control: false,
        perspective: false,
        filter: SamplerFilter::Nearest,
        wrap: SamplerWrap::Repeat,
        blend: BlendMode::Alpha,
        mode: DepthMode::TestWrite,
        unorm16: false,
        depth: true,
        start: 0,
        end: size,
    }
}

struct Fixture {
    vertices: Vec<TexturedMeshVertex>,
    instance: TexturedMeshInstanceRaw,
    mvp: Matrix4,
    primary: Texture,
    additive: Texture,
}

fn texture(filter: SamplerFilter) -> Texture {
    Texture {
        image: RgbaImage::from_fn(8, 8, |x, y| {
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
        }),
        sampler: SamplerDesc {
            filter,
            wrap: SamplerWrap::Repeat,
            mipmaps: false,
        },
        opaque: false,
        yuv420: false,
        half_pixels: Vec::new(),
    }
}

fn fixture(c: &Case) -> Fixture {
    let vertices = [
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
        normal: [p[0], p[1] * 0.3, 1.0, f32::from(c.texgen)],
        color: [
            [0.2, 0.6, 0.9, 0.7],
            [0.8, 0.3, 0.1, 0.9],
            [0.4, 0.9, 0.6, 0.8],
        ][i % 3],
        ..Default::default()
    })
    .collect();
    let mut instance = TexturedMeshInstanceRaw::new(
        Matrix4::IDENTITY,
        [0.7, 0.8, 0.9, 0.6],
        [-0.5, 2.0],
        [0.25, -0.75],
        [0.2, 0.1],
        c.mask,
    );
    instance.sphere_rows = [
        [1.2, 0.3, 0.1, 5.0],
        [0.0, -0.75, 0.4, -3.0],
        [0.3, 0.1, 2.0, -100.0],
    ];
    instance.additive_uv = [0.75, -0.25, 0.1, 0.3];
    let mvp = if c.perspective {
        Matrix4::from_cols(
            Vector4::new(1.0, 0.0, 0.0, 0.125),
            Vector4::new(0.0, 1.0, 0.0, 0.25),
            Vector4::new(0.0, 0.0, 1.0, 0.375),
            Vector4::new(0.0, 0.0, 0.0, 1.0),
        )
    } else {
        Matrix4::IDENTITY
    };
    Fixture {
        vertices,
        instance,
        mvp,
        primary: texture(c.filter),
        additive: texture(c.filter),
    }
}

#[inline(always)]
fn draw<const OLD: bool>(
    c: &Case,
    f: &Fixture,
    writer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    depth: &mut DepthRows<'_>,
) -> u32 {
    let sampler = SamplerDesc {
        filter: c.filter,
        wrap: c.wrap,
        mipmaps: false,
    };
    if c.control {
        return rasterize_textured_mesh_triangles(
            &f.mvp,
            &f.vertices,
            f.instance.tint,
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            c.mask,
            c.blend,
            f.primary.texels(),
            sampler,
            false,
            c.size,
            c.size,
            c.start,
            c.end,
            writer,
            f.instance.cull_mode,
            depth,
        );
    }
    let raster = if OLD {
        original::rasterize_environment
    } else {
        rasterize_environment
    };
    raster(
        &f.mvp,
        &f.vertices,
        f.instance,
        c.blend,
        &f.primary,
        sampler,
        if c.reflection {
            Some(&f.additive)
        } else {
            None
        },
        c.size,
        c.size,
        c.start,
        c.end,
        writer,
        depth,
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

fn exact<T: PartialEq + std::fmt::Debug>(a: &[T], b: &[T], _: bool) -> usize {
    assert_eq!(a, b);
    0
}
fn half_pixels(a: &[u64], b: &[u64], strict: bool) -> usize {
    assert_eq!(a.len(), b.len());
    let mut differences = 0;
    for (&x, &y) in a.iter().zip(b) {
        for lane in 0..4 {
            let u = (x >> (lane * 16)) as u16;
            let v = (y >> (lane * 16)) as u16;
            if u != v {
                assert!(!strict && u ^ v == 0x8000 && decode_half(u).is_nan());
                differences += 1;
            }
        }
    }
    differences
}
fn callback_fields(
    a: &[(usize, [u32; 4], BlendMode)],
    b: &[(usize, [u32; 4], BlendMode)],
    strict: bool,
) -> usize {
    assert_eq!(a.len(), b.len());
    let mut differences = 0;
    for ((i, x, blend), (j, y, old_blend)) in a.iter().zip(b) {
        assert_eq!((i, blend), (j, old_blend));
        for (&u, &v) in x.iter().zip(y) {
            if u != v {
                assert!(!strict && f32::from_bits(u).is_nan() && f32::from_bits(v).is_nan());
                differences += 1;
            }
        }
    }
    differences
}

#[test]
fn environment_raster_preserves_pixels_depth_and_callbacks() {
    let mut seed = 0x7193_4682;
    let values = [
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
    let mut totals = [0_usize; 7];
    for i in 0..4096 {
        let size = [2, 8, 17][i % 3];
        let mut c = case(size);
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
        c.texgen = ((i / 4) % 8) as u8;
        c.mask = i & 32 != 0;
        c.reflection = i & 64 != 0;
        c.perspective = i & 128 != 0;
        c.mode = DepthMode::ALL[(i / 256) % 6];
        c.unorm16 = i & 1024 != 0;
        c.depth = i % 7 != 0;
        (c.start, c.end) = [(0, size), (0, size / 2), (size / 2, size), (size, size)][(i / 16) % 4];
        let mut f = fixture(&c);
        let exceptional = i % 5 == 0;
        if exceptional {
            for v in &mut f.vertices {
                let mut value = || {
                    if i % 2 == 0 {
                        values[next(&mut seed) as usize % values.len()]
                    } else {
                        f32::from_bits(next(&mut seed))
                    }
                };
                v.color = std::array::from_fn(|_| value());
                v.uv = [value(), value()];
                if i % 3 == 0 {
                    v.pos[2] = value();
                    v.normal[0] = value();
                }
            }
        }
        if i % 11 == 0 {
            f.vertices.swap(0, 2);
        }
        if i % 19 == 0 {
            f.instance.cull_mode = 1.0;
        }
        if i % 23 == 0 {
            f.mvp = Matrix4::ZERO;
        }
        for half_texture in [false, true] {
            if half_texture {
                f.primary.half_pixels = (0..64)
                    .map(|_| u64::from(next(&mut seed)) | (u64::from(next(&mut seed)) << 32))
                    .collect();
                f.additive.half_pixels = f.primary.half_pixels.clone();
            }
            let strict = !exceptional && !half_texture;
            let len = size * (c.end - c.start);
            let initial_depth: Vec<_> = if c.depth {
                (0..len)
                    .map(|j| match j % 4 {
                        0 => f32::from_bits(next(&mut seed)),
                        1 => 0.0,
                        2 => 0.5,
                        _ => 1.0,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            macro_rules! compare {
                ($pixels:expr, $writer:ident, $equal:ident $(, $alpha:expr)?) => {{
                    let initial = $pixels; let mut expected = initial.clone(); let mut actual = initial;
                    let mut old_depth = initial_depth.clone(); let mut new_depth = initial_depth.clone();
                    let old = draw::<true>(&c, &f, &mut $writer(&mut expected $(, $alpha)?), &mut DepthRows { pixels: &mut old_depth, mode: c.mode, unorm16: c.unorm16 });
                    let new = draw::<false>(&c, &f, &mut $writer(&mut actual $(, $alpha)?), &mut DepthRows { pixels: &mut new_depth, mode: c.mode, unorm16: c.unorm16 });
                    assert_eq!(new, old, "count: {c:?}");
                    let differences = $equal(&actual, &expected, strict);
                    if stringify!($writer) == "half_writer" { totals[5] += differences; } else { totals[6] += differences; }
                    assert_eq!(new_depth.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), old_depth.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), "depth: {c:?}");
                    totals[3] += new_depth.len(); actual.len()
                }};
            }
            let bytes: Vec<_> = (0..len).map(|_| next(&mut seed)).collect();
            let half: Vec<_> = (0..len)
                .map(|_| u64::from(next(&mut seed)) | (u64::from(next(&mut seed)) << 32))
                .collect();
            totals[0] += compare!(bytes, byte_writer, exact);
            totals[1] += compare!(half.clone(), half_writer, half_pixels, true);
            totals[1] += compare!(half, half_writer, half_pixels, false);
            totals[2] += compare!(Vec::new(), trace_writer, callback_fields) * 4;
            totals[4] += 1;
        }
    }
    println!(
        "{} cases; {} byte pixels; {} half pixels; {} callback fields; {} depth fields",
        totals[4], totals[0], totals[1], totals[2], totals[3]
    );
    println!(
        "Exceptional NaN encoding differences: {} half lanes, {} callback fields",
        totals[5], totals[6]
    );
}
