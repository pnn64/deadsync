// Unoptimized and mapper-only paths, retaining current-main culling and sampler behavior.
use super::*;

#[path = "../../deadlib-render-core/tests/support/uv_mapper_original.rs"]
mod uv_original;

pub(super) fn original(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    instance: deadlib_render_core::TexturedMeshInstanceRaw,
    blend: BlendMode,
    primary: &Texture,
    primary_sampler: SamplerDesc,
    additive: Option<&Texture>,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    depth: &mut DepthRows<'_>,
) -> u32 {
    let mut count = 0;
    for triangle in vertices.as_chunks::<3>().0 {
        let mut first = *triangle;
        let mut second = *triangle;
        for i in 0..3 {
            let uv = uv_original::textured_mesh_uvs(triangle[i], instance);
            first[i].uv = uv[0];
            second[i].uv = uv[1];
        }
        let Some((p, len)) = project_tmesh_polygon(
            mvp,
            instance.tint,
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &first,
            width,
            height,
            instance.cull_mode,
        ) else {
            continue;
        };
        let Some((q, _)) = project_tmesh_polygon(
            mvp,
            instance.tint,
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &second,
            width,
            height,
            instance.cull_mode,
        ) else {
            continue;
        };
        count += 3;
        for i in 1..len.saturating_sub(1) {
            let p = [p[0], p[i], p[i + 1]];
            let q = [q[0], q[i], q[i + 1]];
            let Some(setup) = triangle_setup(p.map(|v| v.x), p.map(|v| v.y), width, height) else {
                continue;
            };
            let Some((min_x, max_x, min_y, max_y, start)) =
                setup.stripe_bounds(stripe_y_start, stripe_y_end)
            else {
                continue;
            };
            let sample = |texture: &Texture, sampler: SamplerDesc, uv: [f32; 2]| {
                let image = texture.texels();
                if sampler.filter == SamplerFilter::Linear {
                    sample_tex_linear::<false>(
                        image,
                        image.width,
                        image.height,
                        uv[0],
                        uv[1],
                        sampler,
                    )
                } else {
                    sample_tex_nearest::<false>(
                        image,
                        image.width,
                        image.height,
                        uv[0],
                        uv[1],
                        sampler,
                    )
                }
                .unwrap_or([0.0; 4])
            };
            let edges = [
                owns_edge(p[1].x, p[1].y, p[2].x, p[2].y, setup.inv_denom),
                owns_edge(p[2].x, p[2].y, p[0].x, p[0].y, setup.inv_denom),
                owns_edge(p[0].x, p[0].y, p[1].x, p[1].y, setup.inv_denom),
            ];
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let px = x as f32 + 0.5;
                    let py = y as f32 + 0.5;
                    let a = edge_function(p[1].x, p[1].y, p[2].x, p[2].y, px, py) * setup.inv_denom;
                    let b = edge_function(p[2].x, p[2].y, p[0].x, p[0].y, px, py) * setup.inv_denom;
                    let c = 1.0 - a - b;
                    if !covered([a, b, c], edges) {
                        continue;
                    }
                    let z = a * p[0].z + b * p[1].z + c * p[2].z;
                    let sum = a * p[0].inv_w + b * p[1].inv_w + c * p[2].inv_w;
                    let w = [
                        a * p[0].inv_w / sum,
                        b * p[1].inv_w / sum,
                        c * p[2].inv_w / sum,
                    ];
                    let uv = |v: &[ScreenVertexTexColor; 3]| {
                        [
                            w[0] * v[0].u + w[1] * v[1].u + w[2] * v[2].u,
                            w[0] * v[0].v + w[1] * v[1].v + w[2] * v[2].v,
                        ]
                    };
                    let texel = sample(primary, primary_sampler, uv(&p));
                    let tint: [f32; 4] = std::array::from_fn(|i| {
                        w[0] * p[0].color[i] + w[1] * p[1].color[i] + w[2] * p[2].color[i]
                    });
                    let mut color: [f32; 4] = std::array::from_fn(|i| texel[i] * tint[i]);
                    if instance.texture_mask > 0.5 {
                        color[..3].copy_from_slice(&tint[..3]);
                    } else if triangle[0].normal[3] as u8 & 4 != 0 {
                        let reflection = sample(
                            additive.unwrap_or(primary),
                            SamplerDesc {
                                wrap: SamplerWrap::Repeat,
                                ..additive.unwrap_or(primary).sampler
                            },
                            uv(&q),
                        );
                        for i in 0..3 {
                            color[i] = (color[i] + reflection[i]).min(1.0);
                        }
                        color[3] *= reflection[3];
                    }
                    if color[3] <= 1.0 / 256.0 {
                        continue;
                    }
                    let index = (y - start) as usize * width + x as usize;
                    if !depth.test(index, z) {
                        continue;
                    }
                    buffer(index, color, blend);
                }
            }
        }
    }
    count
}

pub(super) fn hoisted_only(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    instance: deadlib_render_core::TexturedMeshInstanceRaw,
    blend: BlendMode,
    primary: &Texture,
    primary_sampler: SamplerDesc,
    additive: Option<&Texture>,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    depth: &mut DepthRows<'_>,
) -> u32 {
    let texture_uvs = deadlib_render_core::textured_mesh_uv_mapper(instance);
    let mut count = 0;
    for triangle in vertices.as_chunks::<3>().0 {
        let mut first = *triangle;
        let mut second = *triangle;
        for i in 0..3 {
            let uv = texture_uvs(triangle[i]);
            first[i].uv = uv[0];
            second[i].uv = uv[1];
        }
        let Some((p, len)) = project_tmesh_polygon(
            mvp,
            instance.tint,
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &first,
            width,
            height,
            instance.cull_mode,
        ) else {
            continue;
        };
        let Some((q, _)) = project_tmesh_polygon(
            mvp,
            instance.tint,
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &second,
            width,
            height,
            instance.cull_mode,
        ) else {
            continue;
        };
        count += 3;
        for i in 1..len.saturating_sub(1) {
            let p = [p[0], p[i], p[i + 1]];
            let q = [q[0], q[i], q[i + 1]];
            let Some(setup) = triangle_setup(p.map(|v| v.x), p.map(|v| v.y), width, height) else {
                continue;
            };
            let Some((min_x, max_x, min_y, max_y, start)) =
                setup.stripe_bounds(stripe_y_start, stripe_y_end)
            else {
                continue;
            };
            let sample = |texture: &Texture, sampler: SamplerDesc, uv: [f32; 2]| {
                let image = texture.texels();
                if sampler.filter == SamplerFilter::Linear {
                    sample_tex_linear::<false>(
                        image,
                        image.width,
                        image.height,
                        uv[0],
                        uv[1],
                        sampler,
                    )
                } else {
                    sample_tex_nearest::<false>(
                        image,
                        image.width,
                        image.height,
                        uv[0],
                        uv[1],
                        sampler,
                    )
                }
                .unwrap_or([0.0; 4])
            };
            let edges = [
                owns_edge(p[1].x, p[1].y, p[2].x, p[2].y, setup.inv_denom),
                owns_edge(p[2].x, p[2].y, p[0].x, p[0].y, setup.inv_denom),
                owns_edge(p[0].x, p[0].y, p[1].x, p[1].y, setup.inv_denom),
            ];
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let px = x as f32 + 0.5;
                    let py = y as f32 + 0.5;
                    let a = edge_function(p[1].x, p[1].y, p[2].x, p[2].y, px, py) * setup.inv_denom;
                    let b = edge_function(p[2].x, p[2].y, p[0].x, p[0].y, px, py) * setup.inv_denom;
                    let c = 1.0 - a - b;
                    if !covered([a, b, c], edges) {
                        continue;
                    }
                    let z = a * p[0].z + b * p[1].z + c * p[2].z;
                    let sum = a * p[0].inv_w + b * p[1].inv_w + c * p[2].inv_w;
                    let w = [
                        a * p[0].inv_w / sum,
                        b * p[1].inv_w / sum,
                        c * p[2].inv_w / sum,
                    ];
                    let uv = |v: &[ScreenVertexTexColor; 3]| {
                        [
                            w[0] * v[0].u + w[1] * v[1].u + w[2] * v[2].u,
                            w[0] * v[0].v + w[1] * v[1].v + w[2] * v[2].v,
                        ]
                    };
                    let texel = sample(primary, primary_sampler, uv(&p));
                    let tint: [f32; 4] = std::array::from_fn(|i| {
                        w[0] * p[0].color[i] + w[1] * p[1].color[i] + w[2] * p[2].color[i]
                    });
                    let mut color: [f32; 4] = std::array::from_fn(|i| texel[i] * tint[i]);
                    if instance.texture_mask > 0.5 {
                        color[..3].copy_from_slice(&tint[..3]);
                    } else if triangle[0].normal[3] as u8 & 4 != 0 {
                        let reflection = sample(
                            additive.unwrap_or(primary),
                            SamplerDesc {
                                wrap: SamplerWrap::Repeat,
                                ..additive.unwrap_or(primary).sampler
                            },
                            uv(&q),
                        );
                        for i in 0..3 {
                            color[i] = (color[i] + reflection[i]).min(1.0);
                        }
                        color[3] *= reflection[3];
                    }
                    if color[3] <= 1.0 / 256.0 {
                        continue;
                    }
                    let index = (y - start) as usize * width + x as usize;
                    if !depth.test(index, z) {
                        continue;
                    }
                    buffer(index, color, blend);
                }
            }
        }
    }
    count
}
