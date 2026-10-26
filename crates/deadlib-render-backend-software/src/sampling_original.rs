use super::*;

pub(super) fn original_wrap_uv(u: f32, wrap: SamplerWrap) -> f32 {
    match wrap {
        SamplerWrap::Clamp => clamp01(u),
        SamplerWrap::Repeat => {
            let mut f = u.fract();
            if f < 0.0 {
                f += 1.0;
            }
            f
        }
    }
}

pub(super) fn original_sample_tex_linear<const OPAQUE: bool, const CHANNELS: usize>(
    tex_data: Texels<'_>,
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<[f32; CHANNELS]> {
    let x = original_wrap_uv(u, sampler.wrap).mul_add(tex_w as f32, -0.5);
    let y = original_wrap_uv(v, sampler.wrap).mul_add(tex_h as f32, -0.5);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = clamp01(x - x0 as f32);
    let fy = clamp01(y - y0 as f32);
    let ix0 = original_wrap_index(x0, tex_w, sampler.wrap);
    let ix1 = original_wrap_index(x0 + 1, tex_w, sampler.wrap);
    let iy0 = original_wrap_index(y0, tex_h, sampler.wrap);
    let iy1 = original_wrap_index(y0 + 1, tex_h, sampler.wrap);
    let mut colors = [
        tex_data.pixel::<CHANNELS>(iy0 * tex_w + ix0)?,
        tex_data.pixel::<CHANNELS>(iy0 * tex_w + ix1)?,
        tex_data.pixel::<CHANNELS>(iy1 * tex_w + ix0)?,
        tex_data.pixel::<CHANNELS>(iy1 * tex_w + ix1)?,
    ];
    if OPAQUE {
        for c in &mut colors {
            c[CHANNELS - 1] = 1.0;
        }
    } else if colors.iter().all(|c| c[CHANNELS - 1] == 0.0) {
        return None;
    }
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    Some(std::array::from_fn(|i| {
        lerp(
            lerp(colors[0][i], colors[1][i], fx),
            lerp(colors[2][i], colors[3][i], fx),
            fy,
        )
    }))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the baseline signature for differential checks"
)]
pub(super) fn original_raster_bounds(
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
) -> Option<(i32, i32, i32, i32, i32)> {
    let min_x = min_x.max(0.0) as i32;
    let max_x = max_x.ceil().min((width - 1) as f32) as i32;
    let mut min_y = min_y.max(0.0) as i32;
    let mut max_y = max_y.ceil().min((height - 1) as f32) as i32;
    if min_x > max_x || min_y > max_y {
        return None;
    }

    let stripe_start = stripe_y_start as i32;
    let stripe_end = stripe_y_end as i32 - 1;
    if stripe_start > stripe_end || max_y < stripe_start || min_y > stripe_end {
        return None;
    }
    min_y = min_y.max(stripe_start);
    max_y = max_y.min(stripe_end);
    Some((min_x, max_x, min_y, max_y, stripe_start))
}

pub(super) fn original_wrap_index(i: i32, max: usize, wrap: SamplerWrap) -> usize {
    match wrap {
        SamplerWrap::Clamp => i.clamp(0, max.saturating_sub(1) as i32) as usize,
        SamplerWrap::Repeat => {
            if max.is_power_of_two() {
                return i as usize & (max - 1);
            }
            let m = max as i32;
            if m == 0 {
                0
            } else if i >= 0 && i < m {
                // UVs are already wrapped; only edge texels need a remainder.
                i as usize
            } else {
                let mut v = i % m;
                if v < 0 {
                    v += m;
                }
                v as usize
            }
        }
    }
}
