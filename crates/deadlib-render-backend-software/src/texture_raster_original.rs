//! Frozen textured rasterization from 4c803ac4f.
use super::*;

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn rasterize_triangle_with_inv(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    inv_denom: f32,
    tint: [f32; 4],
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
) {
    match (texture_mask, opaque) {
        (false, false) => rasterize_triangle_mode::<false, false>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (false, true) => rasterize_triangle_mode::<false, true>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (true, false) => rasterize_triangle_mode::<true, false>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (true, true) => rasterize_triangle_mode::<true, true>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn rasterize_triangle_mode<const MASK: bool, const OPAQUE: bool>(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    inv_denom: f32,
    tint: [f32; 4],
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    match sampler.filter {
        SamplerFilter::Nearest => rasterize_triangle_impl::<false, MASK, OPAQUE>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            blend,
            buffer,
        ),
        SamplerFilter::Linear => rasterize_triangle_impl::<true, MASK, OPAQUE>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            blend,
            buffer,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn rasterize_triangle_impl<const LINEAR: bool, const MASK: bool, const OPAQUE: bool>(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    inv_denom: f32,
    tint: [f32; 4],
    image: Texels<'_>,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    blend: BlendMode,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    let Some((min_x, max_x, min_y, max_y, stripe_start)) = raster_bounds(
        v0.x.min(v1.x).min(v2.x),
        v0.x.max(v1.x).max(v2.x),
        v0.y.min(v1.y).min(v2.y),
        v0.y.max(v1.y).max(v2.y),
        width,
        height,
        stripe_y_start,
        stripe_y_end,
    ) else {
        return;
    };

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

            let sr = if MASK { tint[0] } else { sampled[0] * tint[0] };
            let sg = if MASK { tint[1] } else { sampled[1] * tint[1] };
            let sb = if MASK { tint[2] } else { sampled[2] * tint[2] };
            let sa = sampled[3] * tint[3];
            if sa == 0.0 && blend != BlendMode::Multiply {
                continue;
            }

            let dst_idx = row * width + x as usize;
            buffer(dst_idx, [sr, sg, sb, sa], blend);
        }
    }
}
