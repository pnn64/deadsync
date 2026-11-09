use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "color_raster_original.rs"]
mod original;

const MODES: [BlendMode; 5] = [
    BlendMode::Alpha,
    BlendMode::Add,
    BlendMode::Multiply,
    BlendMode::Subtract,
    BlendMode::NoEffect,
];
type Raster = fn(&[ScreenVertexColor; 3], RasterSetup, BlendMode, usize, usize, usize, &mut [u32]);
type Mesh = fn(&Matrix4, &[deadlib_render_core::MeshVertex], BlendMode, usize, &mut [u32]) -> u32;

#[inline(never)]
fn production(
    v: &[ScreenVertexColor; 3],
    setup: RasterSetup,
    mode: BlendMode,
    width: usize,
    start: usize,
    end: usize,
    pixels: &mut [u32],
) {
    rasterize_triangle_color_prepared(v, setup, mode, start, end, &mut byte_writer(pixels), width);
}
#[inline(never)]
fn reference(
    v: &[ScreenVertexColor; 3],
    setup: RasterSetup,
    mode: BlendMode,
    width: usize,
    start: usize,
    end: usize,
    pixels: &mut [u32],
) {
    original::rasterize_triangle_color_prepared(
        v,
        setup,
        mode,
        start,
        end,
        &mut byte_writer(pixels),
        width,
    );
}
fn portable(
    v: &[ScreenVertexColor; 3],
    setup: RasterSetup,
    mode: BlendMode,
    width: usize,
    start: usize,
    end: usize,
    pixels: &mut [u32],
) {
    if let Some(bounds) = setup.stripe_bounds(start, end) {
        rasterize_triangle_color_impl(
            v,
            setup.inv_denom,
            bounds,
            width,
            mode,
            &mut byte_writer(pixels),
        );
    }
}
#[inline(never)]
fn mesh_production(
    m: &Matrix4,
    v: &[deadlib_render_core::MeshVertex],
    mode: BlendMode,
    size: usize,
    pixels: &mut [u32],
) -> u32 {
    rasterize_mesh_triangles(m, v, mode, size, size, 0, size, &mut byte_writer(pixels))
}
#[inline(never)]
fn mesh_reference(
    m: &Matrix4,
    v: &[deadlib_render_core::MeshVertex],
    mode: BlendMode,
    size: usize,
    pixels: &mut [u32],
) -> u32 {
    original::rasterize_mesh_triangles(m, v, mode, size, size, 0, size, &mut byte_writer(pixels))
}

fn vertices(size: usize) -> [ScreenVertexColor; 3] {
    [
        ScreenVertexColor {
            x: 0.2,
            y: 0.3,
            color: [0.2, 0.6, 0.9, 0.7],
        },
        ScreenVertexColor {
            x: size as f32 - 0.1,
            y: 1.1,
            color: [0.8, 0.3, 0.1, 0.9],
        },
        ScreenVertexColor {
            x: 0.1,
            y: size as f32 - 0.2,
            color: [0.4, 0.9, 0.6, 0.8],
        },
    ]
}
fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}
fn setup(v: &[ScreenVertexColor; 3], size: usize) -> Option<RasterSetup> {
    triangle_setup(
        [v[0].x, v[1].x, v[2].x],
        [v[0].y, v[1].y, v[2].y],
        size,
        size,
    )
}

#[test]
fn colored_triangles_preserve_byte_and_half_pixels() {
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
    let mut seed = 0x2631_4793;
    let mut comparisons = 0;
    for case in 0..2048 {
        let size = [2, 8, 17][case % 3];
        let mut v = vertices(size);
        for (i, vertex) in v.iter_mut().enumerate() {
            for (j, color) in vertex.color.iter_mut().enumerate() {
                *color = if case < values.len() * 12 {
                    values[(case + i * 3 + j) % values.len()]
                } else if case < 512 {
                    let finite = [
                        0.0,
                        -0.0,
                        f32::MAX,
                        -f32::MAX,
                        f32::MIN_POSITIVE,
                        f32::from_bits(1),
                        1.0,
                        -1.0,
                        0.5,
                        -0.5,
                    ];
                    finite[(case + i * 3 + j) % finite.len()]
                } else if case.is_multiple_of(3) {
                    next(&mut seed) as f32 / u32::MAX as f32
                } else {
                    f32::from_bits(next(&mut seed))
                };
            }
        }
        if case.is_multiple_of(7) {
            v.swap(1, 2);
        }
        if case.is_multiple_of(11) {
            v[0].x = -2.5;
            v[1].y = -0.5;
        }
        let Some(s) = setup(&v, size) else { continue };
        let (start, end) = match case % 4 {
            0 => (0, size),
            1 => (0, size / 2),
            2 => (size / 2, size),
            _ => (size, size),
        };
        for mode in MODES {
            let initial: Vec<_> = (0..size * (end - start)).map(|_| next(&mut seed)).collect();
            let mut expected = initial.clone();
            reference(&v, s, mode, size, start, end, &mut expected);
            for apply in [production as Raster, portable] {
                let mut actual = initial.clone();
                apply(&v, s, mode, size, start, end, &mut actual);
                for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
                    assert_eq!(a, b, "byte case={case}, mode={mode:?}, pixel={index}");
                }
                comparisons += actual.len();
            }
            for alpha in [false, true] {
                let initial: Vec<_> = (0..size * (end - start))
                    .map(|_| u64::from(next(&mut seed)) | (u64::from(next(&mut seed)) << 32))
                    .collect();
                let mut expected = initial.clone();
                original::rasterize_triangle_color_prepared(
                    &v,
                    s,
                    mode,
                    start,
                    end,
                    &mut half_writer(&mut expected, alpha),
                    size,
                );
                let mut actual = initial;
                rasterize_triangle_color_prepared(
                    &v,
                    s,
                    mode,
                    start,
                    end,
                    &mut half_writer(&mut actual, alpha),
                    size,
                );
                for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
                    assert_eq!(
                        a, b,
                        "half case={case}, mode={mode:?}, alpha={alpha}, pixel={index}"
                    );
                }
                comparisons += actual.len();
            }
        }
    }
    println!("{comparisons} exact byte/half attachment pixel comparisons");
}

#[test]
fn colored_triangles_preserve_callback_order_and_float_results() {
    let mut seed = 0x1348_7926;
    let mut nan_differences = 0;
    let mut fields = 0;
    for case in 0_usize..512 {
        let mut v = vertices(8);
        for vertex in &mut v {
            for c in &mut vertex.color {
                *c = f32::from_bits(next(&mut seed));
            }
        }
        if case.is_multiple_of(7) {
            v[0].x = f32::from_bits(next(&mut seed));
        }
        let Some(s) = setup(&v, 8) else { continue };
        for mode in MODES {
            let (mut expected, mut actual) = (Vec::new(), Vec::new());
            original::rasterize_triangle_color_prepared(
                &v,
                s,
                mode,
                0,
                8,
                &mut |i, c, b| expected.push((i, c, b)),
                8,
            );
            rasterize_triangle_color_prepared(
                &v,
                s,
                mode,
                0,
                8,
                &mut |i, c, b| actual.push((i, c, b)),
                8,
            );
            assert_eq!(expected.len(), actual.len());
            for ((ai, ac, ab), (bi, bc, bb)) in actual.into_iter().zip(expected) {
                assert_eq!((ai, ab), (bi, bb));
                for (a, b) in ac.into_iter().zip(bc) {
                    if a.is_nan() && b.is_nan() {
                        nan_differences += usize::from(a.to_bits() != b.to_bits());
                    } else {
                        assert_eq!(a.to_bits(), b.to_bits());
                    }
                    fields += 1;
                }
            }
        }
    }
    println!("{fields} callback float fields match; NaN payload differences: {nan_differences}");
}

fn mesh() -> Vec<deadlib_render_core::MeshVertex> {
    let color = [
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
        color: color[i % 3],
    })
    .collect()
}

#[test]
fn colored_mesh_draws_preserve_projection_stripes_and_pixels() {
    let mut seed = 0x7193_6428;
    let mut cases = 0;
    for index in 0..128 {
        let mut v = mesh();
        let size = [1, 8, 17][index % 3];
        let transform = match index % 4 {
            0 => Matrix4::IDENTITY,
            1 => Matrix4::from_scale(Vector4::new(0.7, 1.2, 0.8, 1.0).truncate()),
            2 => Matrix4::ZERO,
            _ => {
                Matrix4::from_cols_array(&std::array::from_fn(|_| f32::from_bits(next(&mut seed))))
            }
        };
        for vertex in &mut v {
            for c in &mut vertex.color {
                *c = f32::from_bits(next(&mut seed));
            }
        }
        for mode in MODES {
            for (start, end) in [(0, size), (0, size / 2), (size / 2, size), (size, size)] {
                let mut expected = vec![0x9f3f5f7f; size * (end - start)];
                let mut actual = expected.clone();
                let old = original::rasterize_mesh_triangles(
                    &transform,
                    &v,
                    mode,
                    size,
                    size,
                    start,
                    end,
                    &mut byte_writer(&mut expected),
                );
                let new = rasterize_mesh_triangles(
                    &transform,
                    &v,
                    mode,
                    size,
                    size,
                    start,
                    end,
                    &mut byte_writer(&mut actual),
                );
                assert_eq!(new, old);
                assert_eq!(actual, expected);
                cases += 1;
            }
        }
    }
    println!("{cases} complete mesh draw/stripe cases match original");
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_colored_triangles() {
    bench::pin_thread();
    let rotation = std::env::var("COLOR_RASTER_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let cases: Vec<_> = [2, 8, 32, 128]
        .into_iter()
        .flat_map(|size| MODES.map(|mode| (size, mode)))
        .collect();
    for i in 0..cases.len() {
        let (size, mode) = cases[(i + rotation) % cases.len()];
        let v = vertices(size);
        let s = setup(&v, size).unwrap();
        let n = (100_000 / (size * size)).max(16);
        let mut buffers = [vec![0; size * size], vec![0; size * size]];
        for round in 0..10 {
            for k in 0..2 {
                let variant = (round + k + rotation) % 2;
                let draw = black_box([production as Raster, reference][variant]);
                let pixels = &mut buffers[variant];
                let cycles = bench::cycles();
                let start = Instant::now();
                for _ in 0..n {
                    pixels.fill(0x9f3f5f7f);
                    draw(
                        black_box(&v),
                        black_box(s),
                        black_box(mode),
                        size,
                        0,
                        size,
                        black_box(pixels),
                    );
                    black_box(&*pixels);
                }
                let ns = start.elapsed().as_nanos() as f64 / n as f64;
                let cpu = (bench::cycles() - cycles) as f64 / n as f64;
                println!(
                    "color-raster,triangle-{size}-{mode:?},{variant},{round},{n},{cpu:.4},{ns:.4}"
                );
            }
        }
    }
    for size in [8, 32, 128] {
        for mode in [BlendMode::Alpha, BlendMode::Add, BlendMode::Multiply] {
            let v = mesh();
            let m = Matrix4::IDENTITY;
            let n = (100_000 / (size * size)).max(16);
            let mut buffers = [vec![0; size * size], vec![0; size * size]];
            for round in 0..10 {
                for k in 0..2 {
                    let variant = (round + k + rotation) % 2;
                    let draw = black_box([mesh_production as Mesh, mesh_reference][variant]);
                    let pixels = &mut buffers[variant];
                    let cycles = bench::cycles();
                    let start = Instant::now();
                    for _ in 0..n {
                        pixels.fill(0x9f3f5f7f);
                        black_box(draw(
                            black_box(&m),
                            black_box(&v),
                            black_box(mode),
                            size,
                            black_box(pixels),
                        ));
                        black_box(&*pixels);
                    }
                    let ns = start.elapsed().as_nanos() as f64 / n as f64;
                    let cpu = (bench::cycles() - cycles) as f64 / n as f64;
                    println!(
                        "color-raster,mesh-{size}-{mode:?},{variant},{round},{n},{cpu:.4},{ns:.4}"
                    );
                }
            }
        }
    }
    let v = vertices(8);
    let s = setup(&v, 8).unwrap();
    for round in 0..10 {
        for k in 0..2 {
            let variant = (round + k + rotation) % 2;
            let draw = black_box([production as Raster, reference][variant]);
            let mut pixels = vec![0; 64];
            let n = 100_000;
            let cycles = bench::cycles();
            let start = Instant::now();
            for _ in 0..n {
                draw(
                    black_box(&v),
                    black_box(s),
                    BlendMode::Alpha,
                    8,
                    8,
                    8,
                    black_box(&mut pixels),
                );
            }
            let ns = start.elapsed().as_nanos() as f64 / n as f64;
            let cpu = (bench::cycles() - cycles) as f64 / n as f64;
            println!("color-raster,empty-stripe,{variant},{round},{n},{cpu:.4},{ns:.4}");
        }
    }
}
