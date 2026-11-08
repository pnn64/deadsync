use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "triangle_pipeline_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn next(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

#[test]
fn coverage_retains_edges_zeros_and_nan_rejection() {
    let values = [
        -1.0,
        -0.0,
        0.0,
        1.0,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0xff80_0001),
    ];
    for a in values {
        for b in values {
            for c in values {
                for mask in 0..8 {
                    let edges = std::array::from_fn(|i| mask & (1 << i) != 0);
                    assert_eq!(
                        covered([a, b, c], edges),
                        original::covered([a, b, c], edges)
                    );
                }
            }
        }
    }
    let mut state = 0x1234_5678;
    for _ in 0..100_000 {
        let weights = std::array::from_fn(|_| f32::from_bits(next(&mut state)));
        let edges = std::array::from_fn(|_| next(&mut state) & 1 != 0);
        assert_eq!(covered(weights, edges), original::covered(weights, edges));
    }
}

fn projection_triangle(state: &mut u32) -> [deadlib_render_core::TexturedMeshVertex; 3] {
    std::array::from_fn(|_| deadlib_render_core::TexturedMeshVertex {
        pos: std::array::from_fn(|_| (next(state) % 1024) as f32 / 512.0 - 1.0),
        uv: [0.25, 0.75],
        color: [0.1, 0.2, 0.3, 0.4],
        ..Default::default()
    })
}

fn projection_matrix() -> Matrix4 {
    Matrix4::from_cols(
        Vector4::new(1.0, 0.0, 0.0, 0.125),
        Vector4::new(0.0, 1.0, 0.0, 0.25),
        Vector4::new(0.0, 0.0, 1.0, 0.375),
        Vector4::new(0.0, 0.0, 0.0, 1.0),
    )
}

fn clip_vertex(z: f32, w: f32) -> ClipVertexTexColor {
    ClipVertexTexColor {
        clip: Vector4::new(0.25, -0.75, z, w),
        u: 0.25,
        v: 0.75,
        color: [0.1, 0.2, 0.3, 0.4],
    }
}

#[test]
fn clipping_retains_all_vertex_bits_and_lengths() {
    let mut state = 0x1234_5678;
    let values = [
        -2.0,
        -1.0,
        -0.0,
        0.0,
        1.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    for index in 0..100_000 {
        let triangle = std::array::from_fn(|i| {
            if index < 4096 {
                clip_vertex(values[(index >> (i * 3)) & 7], values[(index >> 9) & 7])
            } else {
                clip_vertex(
                    f32::from_bits(next(&mut state)),
                    f32::from_bits(next(&mut state)),
                )
            }
        });
        let bits = |(vertices, len): ([ClipVertexTexColor; 4], usize)| {
            (
                vertices.map(|v| {
                    [
                        v.clip.x, v.clip.y, v.clip.z, v.clip.w, v.u, v.v, v.color[0], v.color[1],
                        v.color[2], v.color[3],
                    ]
                    .map(f32::to_bits)
                }),
                len,
            )
        };
        assert_eq!(
            bits(clip_tmesh_near(triangle)),
            bits(original::clip_tmesh_near(triangle))
        );
    }
}

fn project<const OLD: bool>(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::TexturedMeshVertex; 3],
    cull: f32,
) -> Option<([ScreenVertexTexColor; 4], usize)> {
    if OLD {
        original::project_tmesh_polygon(
            mvp,
            [0.5, 0.75, 1.0, 0.25],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            vertices,
            640,
            480,
            cull,
        )
    } else {
        project_tmesh_polygon(
            mvp,
            [0.5, 0.75, 1.0, 0.25],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            vertices,
            640,
            480,
            cull,
        )
    }
}

#[test]
fn projection_retains_vertex_bits_clipping_and_culling() {
    let mut state = 0x1234_5678;
    let special = [
        0.0,
        -0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::MAX,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
    ];
    for index in 0..20_000 {
        let mut vertices = projection_triangle(&mut state);
        let mvp = if index < 10_000 {
            if index < special.len() * 3 {
                vertices[index % 3].pos[index % 3] = special[index / 3];
            }
            projection_matrix()
        } else {
            Matrix4::from_cols_array(&std::array::from_fn(|_| f32::from_bits(next(&mut state))))
        };
        for cull in [0.0, 1.0, 2.0] {
            let bits = |result: Option<([ScreenVertexTexColor; 4], usize)>| {
                result.map(|(vertices, len)| {
                    (
                        vertices.map(|v| {
                            [
                                v.x, v.y, v.z, v.inv_w, v.u, v.v, v.color[0], v.color[1],
                                v.color[2], v.color[3],
                            ]
                            .map(f32::to_bits)
                        }),
                        len,
                    )
                })
            };
            assert_eq!(
                bits(project::<false>(&mvp, &vertices, cull)),
                bits(project::<true>(&mvp, &vertices, cull))
            );
        }
    }
}

fn vertices(alpha: f32) -> [ScreenVertexTexColor; 3] {
    std::array::from_fn(|i| ScreenVertexTexColor {
        inv_w: 1.0,
        z: [0.25, 0.5, 0.75][i],
        x: [0.0, 32.0, 0.0][i],
        y: [0.0, 0.0, 32.0][i],
        u: [0.0, 1.0, 0.0][i],
        v: [0.0, 0.0, 1.0][i],
        color: [0.1 + i as f32 * 0.2, 0.5, 0.9 - i as f32 * 0.1, alpha],
    })
}

fn raster<const OLD: bool, const LINEAR: bool, const MASK: bool, const OPAQUE: bool>(
    vertices: &[ScreenVertexTexColor; 3],
    image: &RgbaImage,
    depth: &mut DepthRows<'_>,
    write: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    let setup = triangle_setup(vertices.map(|v| v.x), vertices.map(|v| v.y), 32, 32).unwrap();
    let sampler = SamplerDesc {
        filter: SamplerFilter::Nearest,
        wrap: SamplerWrap::Clamp,
        mipmaps: false,
    };
    if OLD {
        original::rasterize_triangle_tex_color_impl::<LINEAR, MASK, OPAQUE>(
            &vertices[0],
            &vertices[1],
            &vertices[2],
            setup,
            image.into(),
            sampler,
            32,
            0,
            32,
            BlendMode::Alpha,
            write,
            depth,
        );
    } else {
        rasterize_triangle_tex_color_impl::<LINEAR, MASK, OPAQUE>(
            &vertices[0],
            &vertices[1],
            &vertices[2],
            setup,
            image.into(),
            sampler,
            32,
            0,
            32,
            BlendMode::Alpha,
            write,
            depth,
        );
    }
}

fn check_raster<const LINEAR: bool, const MASK: bool, const OPAQUE: bool>() {
    let image = RgbaImage::from_fn(4, 4, |x, y| {
        image::Rgba([x as u8 * 51, y as u8 * 71, 97, (x + y) as u8 * 31])
    });
    for alpha in [0.0, -0.0, 0.001, 0.005, 0.5, 1.0, 2.0, f32::NAN] {
        for mode in deadlib_render_core::DepthMode::ALL {
            for unorm16 in [false, true] {
                let mut old_depth = vec![0.4; 1024];
                let mut new_depth = old_depth.clone();
                let mut old_pixels = Vec::new();
                let mut new_pixels = Vec::new();
                let vertices = vertices(alpha);
                raster::<true, LINEAR, MASK, OPAQUE>(
                    &vertices,
                    &image,
                    &mut DepthRows {
                        pixels: &mut old_depth,
                        unorm16,
                        mode,
                    },
                    &mut |index, color, _| old_pixels.push((index, color.map(f32::to_bits))),
                );
                raster::<false, LINEAR, MASK, OPAQUE>(
                    &vertices,
                    &image,
                    &mut DepthRows {
                        pixels: &mut new_depth,
                        unorm16,
                        mode,
                    },
                    &mut |index, color, _| new_pixels.push((index, color.map(f32::to_bits))),
                );
                assert_eq!(new_pixels, old_pixels);
                assert_eq!(
                    new_depth.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    old_depth.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn fragment_rejection_retains_writes_and_depth_bits() {
    check_raster::<false, false, false>();
    check_raster::<false, false, true>();
    check_raster::<false, true, false>();
    check_raster::<false, true, true>();
    check_raster::<true, false, false>();
    check_raster::<true, false, true>();
    check_raster::<true, true, false>();
    check_raster::<true, true, true>();
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_triangle_pipeline() {
    let mut state = 0x1234_5678;
    let weights: Vec<_> = (0..4096)
        .map(|_| {
            let a = (next(&mut state) % 1024) as f32 / 1024.0;
            let b = (next(&mut state) % 1024) as f32 / 1024.0;
            [a, b, 1.0 - a - b]
        })
        .collect();
    let work = || {
        for &weights in black_box(&weights) {
            black_box(covered(black_box(weights), black_box([true, false, true])));
        }
    };
    let (_, counts) = perf::measure(work);
    println!("coverage: allocation {counts:?}");
    bench::measure("coverage", 200, work);
    for (label, range) in [("clip_inside", 1.0), ("clip_mixed", 4.0)] {
        let triangles: Vec<_> = (0..1024)
            .map(|_| {
                std::array::from_fn(|_| {
                    clip_vertex(
                        (next(&mut state) % 1024) as f32 / 1024.0 * range - range * 0.5,
                        1.0,
                    )
                })
            })
            .collect();
        let work = || {
            for &triangle in black_box(&triangles) {
                black_box(clip_tmesh_near(black_box(triangle)));
            }
        };
        let (_, counts) = perf::measure(work);
        println!("{label}: allocation {counts:?}");
        bench::measure(label, 100, work);
    }
    let triangles: Vec<_> = (0..1024).map(|_| projection_triangle(&mut state)).collect();
    let mvp = projection_matrix();
    let work = || {
        for triangle in black_box(&triangles) {
            black_box(project::<false>(
                black_box(&mvp),
                black_box(triangle),
                black_box(0.0),
            ));
        }
    };
    let (_, counts) = perf::measure(work);
    println!("projection: allocation {counts:?}");
    bench::measure("projection", 100, work);
    let image = RgbaImage::from_pixel(4, 4, image::Rgba([71, 97, 23, 195]));
    for (label, alpha, depth_value) in [
        ("raster_visible", 1.0, 1.0),
        ("raster_alpha", 0.001, 1.0),
        ("raster_depth", 1.0, 0.0),
    ] {
        let vertices = vertices(alpha);
        let mut values = vec![depth_value; 1024];
        let mut work = || {
            raster::<false, false, false, false>(
                black_box(&vertices),
                black_box(&image),
                &mut DepthRows {
                    pixels: black_box(&mut values),
                    unorm16: false,
                    mode: deadlib_render_core::DepthMode::Test,
                },
                &mut |index, color, mode| {
                    black_box((index, color, mode));
                },
            );
        };
        let (_, counts) = perf::measure(&mut work);
        println!("{label}: allocation {counts:?}");
        bench::measure(label, 200, work);
    }
}
