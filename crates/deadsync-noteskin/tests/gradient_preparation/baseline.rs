// Frozen from 57ece088f (0.5.1655); unchanged color/profile helpers are shared.
use super::*;

#[must_use]
pub(super) fn mine_gradient_texture(colors: &[[f32; 4]]) -> RgbaImage {
    assert!(
        !colors.is_empty(),
        "mine gradient requires at least one color"
    );
    let frame_count = colors.len();
    let frame_size = MINE_GRADIENT_FRAME_SIZE.max(2);
    let mut image = RgbaImage::new(frame_size * frame_count as u32, frame_size);
    let colors = mine_gradient_colors(colors);
    let profile = &*MINE_GRADIENT_PROFILE;
    for frame in 0..frame_count {
        let x_offset = frame as u32 * frame_size;
        for y in 0..frame_size {
            for x in 0..frame_size {
                let profile_index = y as usize * frame_size as usize + x as usize;
                let layer = profile.layers[profile_index];
                if layer == MINE_GRADIENT_OUTSIDE_LAYER {
                    continue;
                }
                let color_index =
                    (frame + colors.len() - (usize::from(layer) % colors.len())) % colors.len();
                image.put_pixel(
                    x_offset + x,
                    y,
                    Rgba(mine_gradient_pixel(
                        colors[color_index],
                        profile.edge_alpha[profile_index],
                    )),
                );
            }
        }
    }

    image
}

#[must_use]
pub(super) fn mine_gradient_samples(
    image: &RgbaImage,
    src: [u32; 2],
    size: [u32; 2],
    sample_count: usize,
) -> Option<Vec<[f32; 4]>> {
    let [src_x, src_y] = src;
    let [sample_width, sample_height] = size;
    if sample_width == 0 || sample_height == 0 {
        return None;
    }

    let mut colors = Vec::with_capacity(sample_width as usize);
    for dx in 0..sample_width {
        let mut r = 0.0_f32;
        let mut g = 0.0_f32;
        let mut b = 0.0_f32;
        let mut alpha_weight = 0.0_f32;

        for dy in 0..sample_height {
            let pixel = image.get_pixel(src_x + dx, src_y + dy);
            let a = f32::from(pixel[3]) / 255.0;
            if a <= f32::EPSILON {
                continue;
            }
            r = f32::from(pixel[0]).mul_add(a, r);
            g = f32::from(pixel[1]).mul_add(a, g);
            b = f32::from(pixel[2]).mul_add(a, b);
            alpha_weight += a;
        }

        if alpha_weight <= f32::EPSILON {
            colors.push([0.0, 0.0, 0.0, 0.0]);
        } else {
            let inv = 1.0 / alpha_weight;
            colors.push([
                (r * inv) / 255.0,
                (g * inv) / 255.0,
                (b * inv) / 255.0,
                (alpha_weight / sample_height as f32).clamp(0.0, 1.0),
            ]);
        }
    }

    mine_gradient_resample(&colors, sample_count)
}

#[must_use]
pub(super) fn mine_gradient_resample(
    colors: &[[f32; 4]],
    sample_count: usize,
) -> Option<Vec<[f32; 4]>> {
    if colors.is_empty() {
        return None;
    }

    let sample_count = sample_count.max(1);
    if colors.len() == 1 {
        let mut color = colors[0];
        color[3] = 1.0;
        return Some(vec![color; sample_count]);
    }

    let max_index = (colors.len() - 1) as f32;
    let mut samples = Vec::with_capacity(sample_count);
    let divisor = (sample_count.saturating_sub(1)).max(1) as f32;
    for i in 0..sample_count {
        let t = i as f32 / divisor;
        let position = t * max_index;
        let base_index = position as usize;
        let next_index = (base_index + 1).min(colors.len() - 1);
        let frac = (position - base_index as f32).clamp(0.0, 1.0);

        let c0 = colors[base_index];
        let c1 = colors[next_index];
        samples.push([
            (c1[0] - c0[0]).mul_add(frac, c0[0]).clamp(0.0, 1.0),
            (c1[1] - c0[1]).mul_add(frac, c0[1]).clamp(0.0, 1.0),
            (c1[2] - c0[2]).mul_add(frac, c0[2]).clamp(0.0, 1.0),
            1.0,
        ]);
    }

    Some(samples)
}
