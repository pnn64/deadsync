//! Frozen textured-mesh rasterization from b8f046c77.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn rasterize_prepared_tmesh(
    triangles: &[PreparedTriangle<ScreenVertexTexColor>],
    texture_mask: bool,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,

    depth: &mut DepthRows<'_>,
) {
    for triangle in triangles {
        rasterize_triangle_tex_color_prepared(
            &triangle.vertices,
            triangle.setup,
            blend,
            texture_mask,
            image,
            sampler,
            opaque,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn rasterize_textured_mesh_triangles(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: bool,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    cull_mode: f32,

    depth: &mut DepthRows<'_>,
) -> u32 {
    if vertices.len() < 3 || width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return 0;
    }

    let mut verts_drawn = 0u32;
    for chunk in vertices.as_chunks::<3>().0 {
        let Some((polygon, polygon_len)) = project_tmesh_polygon(
            mvp,
            tint,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            chunk,
            width,
            height,
            cull_mode,
        ) else {
            continue;
        };
        verts_drawn = verts_drawn.saturating_add(3);
        for index in 1..polygon_len.saturating_sub(1) {
            rasterize_triangle_tex_color(
                &polygon[0],
                &polygon[index],
                &polygon[index + 1],
                blend,
                texture_mask,
                image,
                sampler,
                opaque,
                width,
                height,
                stripe_y_start,
                stripe_y_end,
                buffer,
                depth,
            );
        }
    }

    verts_drawn
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn rasterize_triangle_tex_color(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    blend: BlendMode,
    texture_mask: bool,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),

    depth: &mut DepthRows<'_>,
) {
    let Some(setup) = triangle_setup_in_rows(
        [v0.x, v1.x, v2.x],
        [v0.y, v1.y, v2.y],
        width,
        height,
        stripe_y_start,
        stripe_y_end,
    ) else {
        return;
    };
    rasterize_triangle_tex_color_prepared(
        &[*v0, *v1, *v2],
        setup,
        blend,
        texture_mask,
        image,
        sampler,
        opaque,
        stripe_y_start,
        stripe_y_end,
        buffer,
        width,
        depth,
    );
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn rasterize_triangle_tex_color_prepared(
    vertices: &[ScreenVertexTexColor; 3],
    setup: RasterSetup,
    blend: BlendMode,
    texture_mask: bool,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,

    depth: &mut DepthRows<'_>,
) {
    let [v0, v1, v2] = vertices;
    match (texture_mask, opaque) {
        (false, false) => rasterize_triangle_tex_color_mode::<false, false>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
        (false, true) => rasterize_triangle_tex_color_mode::<false, true>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
        (true, false) => rasterize_triangle_tex_color_mode::<true, false>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
        (true, true) => rasterize_triangle_tex_color_mode::<true, true>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn rasterize_triangle_tex_color_mode<const MASK: bool, const OPAQUE: bool>(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    setup: RasterSetup,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,

    depth: &mut DepthRows<'_>,
) {
    match sampler.filter {
        SamplerFilter::Nearest => {
            rasterize_triangle_tex_color_impl::<false, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                blend,
                buffer,
                depth,
            );
        }
        SamplerFilter::Linear => {
            rasterize_triangle_tex_color_impl::<true, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                blend,
                buffer,
                depth,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn rasterize_triangle_tex_color_impl<
    const LINEAR: bool,
    const MASK: bool,
    const OPAQUE: bool,
>(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    setup: RasterSetup,
    image: Texels<'_>,
    sampler: SamplerDesc,
    width: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    blend: BlendMode,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),

    depth: &mut DepthRows<'_>,
) {
    let Some((min_x, max_x, min_y, max_y, stripe_start)) =
        setup.stripe_bounds(stripe_y_start, stripe_y_end)
    else {
        return;
    };
    let inv_denom = setup.inv_denom;
    let tex_w = image.width.max(1);
    let tex_h = image.height.max(1);
    let tex_data = image;

    let edges = [
        owns_edge(v1.x, v1.y, v2.x, v2.y, inv_denom),
        owns_edge(v2.x, v2.y, v0.x, v0.y, inv_denom),
        owns_edge(v0.x, v0.y, v1.x, v1.y, inv_denom),
    ];
    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let row = (y - stripe_start) as usize;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;
            if !covered([w0, w1, w2], edges) {
                continue;
            }

            let sampled = if MASK && OPAQUE {
                Some([0.0, 0.0, 0.0, 1.0])
            } else {
                let u = v2.u.mul_add(w2, v0.u.mul_add(w0, v1.u * w1));
                let v = v2.v.mul_add(w2, v0.v.mul_add(w0, v1.v * w1));
                if MASK {
                    let alpha = if LINEAR {
                        sample_alpha_linear(tex_data, tex_w, tex_h, u, v, sampler)
                    } else {
                        sample_alpha_nearest(tex_data, tex_w, tex_h, u, v, sampler)
                    };
                    alpha.map(|alpha| [0.0, 0.0, 0.0, alpha])
                } else if LINEAR {
                    sample_tex_linear::<OPAQUE, 4>(tex_data, tex_w, tex_h, u, v, sampler)
                } else {
                    sample_tex_nearest::<OPAQUE, 4>(tex_data, tex_w, tex_h, u, v, sampler)
                }
            };
            let Some(sampled) = sampled else {
                continue;
            };
            if sampled[3] == 0.0 && blend != BlendMode::Multiply {
                continue;
            }

            let ca = v2.color[3].mul_add(w2, v0.color[3].mul_add(w0, v1.color[3] * w1));

            let sa = sampled[3] * ca;
            if sa <= 1.0 / 256.0 {
                continue;
            }

            let dst_idx = row * width + x as usize;
            let z = v2.z.mul_add(w2, v0.z.mul_add(w0, v1.z * w1));
            if !depth.test(dst_idx, z) {
                continue;
            }
            let cr = v2.color[0].mul_add(w2, v0.color[0].mul_add(w0, v1.color[0] * w1));
            let cg = v2.color[1].mul_add(w2, v0.color[1].mul_add(w0, v1.color[1] * w1));
            let cb = v2.color[2].mul_add(w2, v0.color[2].mul_add(w0, v1.color[2] * w1));
            let sr = if MASK { cr } else { sampled[0] * cr };
            let sg = if MASK { cg } else { sampled[1] * cg };
            let sb = if MASK { cb } else { sampled[2] * cb };
            buffer(dst_idx, [sr, sg, sb, sa], blend);
        }
    }
}
