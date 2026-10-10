use super::*;
use deadlib_render_core::{CullMode, TexturedMeshVertex};
use glam::Vec3 as Vector3;
use std::hint::black_box;

#[path = "environment_original.rs"]
mod original;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

fn texture(filter: SamplerFilter) -> Texture {
    Texture {
        image: RgbaImage::from_fn(16, 16, |x, y| {
            image::Rgba([(x * 17) as u8, (y * 17) as u8, 93, (37 + (x + y) * 7) as u8])
        }),
        sampler: SamplerDesc {
            filter,
            ..Default::default()
        },
        opaque: false,
        yuv420: false,
        half_pixels: Vec::new(),
    }
}

fn instance(mask: f32) -> TexturedMeshInstanceRaw {
    let mut instance = TexturedMeshInstanceRaw::new(
        Matrix4::IDENTITY,
        [0.7, 0.8, 0.9, 0.6],
        [-0.5, 2.0],
        [0.25, -0.75],
        [0.2, 0.1],
        false,
    );
    instance.sphere_rows = [
        [1.2, 0.3, 0.1, 5.0],
        [0.0, -0.75, 0.4, -3.0],
        [0.3, 0.1, 2.0, -100.0],
    ];
    instance.additive_uv = [0.75, -0.25, 0.1, 0.3];
    instance.texture_mask = mask;
    instance
}

fn vertex(pos: [f32; 3], mode: f32) -> TexturedMeshVertex {
    TexturedMeshVertex {
        pos,
        normal: [pos[0], pos[1] * 0.3, 1.0, mode],
        uv: [pos[0] * 0.5, pos[1] * 0.5],
        color: [0.8, 0.6, 1.0, 0.75],
        tex_matrix_scale: [-0.5, 1.5],
    }
}

#[allow(clippy::too_many_arguments)]
fn render(
    variant: usize,
    mvp: &Matrix4,
    vertices: &[TexturedMeshVertex],
    instance: TexturedMeshInstanceRaw,
    primary: &Texture,
    sampler: Option<MeshSampler>,
    additive: Option<&Texture>,
    width: usize,
    stripe: (usize, usize),
    pixels: &mut [u32],
    depth: &mut DepthRows<'_>,
) -> u32 {
    let mut writer = byte_writer(pixels);
    let primary_sampler = texture_sampler_desc(primary.sampler, 0, true, sampler);
    match variant {
        0 => original::original(
            mvp,
            vertices,
            instance,
            BlendMode::Alpha,
            primary,
            primary_sampler,
            additive,
            width,
            width,
            stripe.0,
            stripe.1,
            &mut writer,
            depth,
        ),
        1 => original::hoisted_only(
            mvp,
            vertices,
            instance,
            BlendMode::Alpha,
            primary,
            primary_sampler,
            additive,
            width,
            width,
            stripe.0,
            stripe.1,
            &mut writer,
            depth,
        ),
        _ => rasterize_environment(
            mvp,
            vertices,
            instance,
            BlendMode::Alpha,
            primary,
            primary_sampler,
            additive,
            width,
            width,
            stripe.0,
            stripe.1,
            &mut writer,
            depth,
        ),
    }
}

#[test]
fn environment_pixels_depth_and_counts_match_original() {
    // Visible, clipped, degenerate, reversed-winding, and incomplete triangles.
    let positions = [
        [-0.8, -0.8, 0.3],
        [0.8, -0.8, 0.3],
        [0.8, 0.8, 0.3],
        [-1.5, 0.0, -1.5],
        [0.3, -0.8, 0.7],
        [0.3, 0.8, 0.7],
        [0.0, 0.0, 0.5],
        [0.0, 0.0, 0.5],
        [0.0, 0.0, 0.5],
        [-0.8, -0.8, 0.2],
        [-0.8, 0.8, 0.2],
        [0.8, 0.8, 0.2],
        [0.0, 0.0, 0.0],
    ];
    for filter in [SamplerFilter::Nearest, SamplerFilter::Linear] {
        let primary = texture(filter);
        let reflection = texture(SamplerFilter::Linear);
        for mode in 0..9 {
            let vertices: Vec<_> = positions
                .into_iter()
                .enumerate()
                .map(|(i, p)| {
                    vertex(
                        p,
                        if mode == 8 {
                            (i % 8) as f32
                        } else {
                            mode as f32
                        },
                    )
                })
                .collect();
            for mask in [0.0, 0.5, 0.75, 1.0, f32::NAN] {
                for mvp in [
                    Matrix4::IDENTITY,
                    Matrix4::from_scale(Vector3::new(-1.0, 0.8, 1.0)),
                    Matrix4::from_rotation_y(0.4),
                    glam::camera::rh::proj::opengl::perspective(1.2, 1.0, 0.1, 100.0)
                        * Matrix4::from_translation(Vector3::new(0.0, 0.0, -1.0)),
                ] {
                    for (cull, stripe, depth_mode, additive, sampler) in [
                        (CullMode::None, (0, 32), 0, None, None),
                        (CullMode::None, (7, 23), 1, Some(&reflection), None),
                        (CullMode::Back, (0, 32), 2, Some(&reflection), None),
                        (CullMode::Back, (7, 23), 0, None, None),
                        (
                            CullMode::Front,
                            (0, 32),
                            2,
                            Some(&reflection),
                            Some(MeshSampler {
                                filter: SamplerFilter::Nearest,
                                wrap: SamplerWrap::Clamp,
                            }),
                        ),
                        (
                            CullMode::Front,
                            (7, 23),
                            0,
                            None,
                            Some(MeshSampler {
                                filter: SamplerFilter::Linear,
                                wrap: SamplerWrap::Repeat,
                            }),
                        ),
                        (
                            CullMode::None,
                            (7, 23),
                            1,
                            Some(&reflection),
                            Some(MeshSampler {
                                filter: SamplerFilter::Nearest,
                                wrap: SamplerWrap::Repeat,
                            }),
                        ),
                        (
                            CullMode::Back,
                            (0, 32),
                            2,
                            Some(&reflection),
                            Some(MeshSampler {
                                filter: SamplerFilter::Linear,
                                wrap: SamplerWrap::Clamp,
                            }),
                        ),
                    ] {
                        let mut inst = instance(mask);
                        inst.cull_mode = cull as u8 as f32;
                        let mut results = Vec::new();
                        for variant in 0..3 {
                            let mut pixels = vec![0xff14_283c; 32 * (stripe.1 - stripe.0)];
                            let mut depth =
                                vec![1.0; if depth_mode == 0 { 0 } else { pixels.len() }];
                            let count = render(
                                variant,
                                &mvp,
                                &vertices,
                                inst,
                                &primary,
                                sampler,
                                additive,
                                32,
                                stripe,
                                &mut pixels,
                                &mut DepthRows {
                                    pixels: &mut depth,
                                    unorm16: depth_mode == 2,
                                },
                            );
                            results.push((
                                pixels,
                                depth.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                count,
                            ));
                        }
                        assert_eq!(
                            results[0], results[1],
                            "hoist: mode={mode} mask={mask} cull={cull:?} sampler={sampler:?}"
                        );
                        assert_eq!(
                            results[0], results[2],
                            "projection: mode={mode} mask={mask} cull={cull:?} sampler={sampler:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "paired release renderer throughput benchmark"]
fn benchmark_environment_geometry() {
    let primary = texture(SamplerFilter::Linear);
    let reflection = texture(SamplerFilter::Linear);
    for (name, mode, mask, before, after) in [
        ("sphere preparation, reflection", 7.0, 0.0, 0, 1),
        ("skip unused projection, sphere", 1.0, 0.0, 1, 2),
        ("skip unused projection, mask", 6.0, 1.0, 1, 2),
        ("reflection projection control", 7.0, 0.0, 1, 2),
        ("combined sphere rendering", 1.0, 0.0, 0, 2),
    ] {
        let vertices: Vec<_> = (0..512)
            .flat_map(|i| {
                let x = (i % 24) as f32 / 12.0 - 0.95;
                let y = (i / 24) as f32 / 12.0 - 0.95;
                [[x, y, 0.5], [x + 0.05, y, 0.5], [x, y + 0.05, 0.5]].map(|p| vertex(p, mode))
            })
            .collect();
        let mut pixels = vec![0; 64 * 64];
        paired_bench::compare(&format!("{name}, 512 triangles"), 300, |current| {
            pixels.fill(0xff14_283c);
            let count = render(
                if current { after } else { before },
                &black_box(Matrix4::IDENTITY),
                black_box(&vertices),
                black_box(instance(mask)),
                &primary,
                None,
                Some(&reflection),
                64,
                (0, 64),
                &mut pixels,
                &mut DepthRows {
                    pixels: &mut [],
                    unorm16: false,
                },
            );
            black_box((count, &pixels));
        });
    }
}
