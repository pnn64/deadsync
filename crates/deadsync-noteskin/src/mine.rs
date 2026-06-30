use image::{Rgba, RgbaImage};
use std::hash::Hasher;
use twox_hash::XxHash64;

pub const MINE_GRADIENT_SAMPLES: usize = 64;
pub const MINE_GRADIENT_FRAME_SIZE: u32 = 64;

const MINE_FILL_LAYERS: usize = 32;
const MINE_GRADIENT_KEY_PREFIX: &str = "generated/noteskins/mine_fill";

pub fn mine_fill_slots<T, U>(
    mines: &[Option<T>],
    mut fill_slot: impl FnMut(&T) -> Option<U>,
) -> Vec<Option<U>> {
    mines
        .iter()
        .map(|slot| slot.as_ref().and_then(&mut fill_slot))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MineGradientSlotPlan {
    pub texture_key: String,
    pub tex_dims: (u32, u32),
    pub frame_size: [i32; 2],
    pub frame_count: usize,
}

pub fn mine_gradient_slot_plan(colors: &[[f32; 4]]) -> MineGradientSlotPlan {
    let frame_count = colors.len().max(1);
    let frame_size = MINE_GRADIENT_FRAME_SIZE as i32;
    MineGradientSlotPlan {
        texture_key: mine_gradient_texture_key(colors),
        tex_dims: (
            MINE_GRADIENT_FRAME_SIZE * frame_count as u32,
            MINE_GRADIENT_FRAME_SIZE,
        ),
        frame_size: [frame_size, frame_size],
        frame_count,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MineGradientSampleRegion {
    pub src: [u32; 2],
    pub size: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MineGradientSampleRegionError {
    InvalidSlotSize,
    RegionOutsideTexture,
    ZeroSampleSize,
}

#[inline(always)]
fn mine_grad_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn mine_gradient_texture_key(colors: &[[f32; 4]]) -> String {
    let mut hasher = XxHash64::default();
    hasher.write_u32(MINE_FILL_LAYERS as u32);
    hasher.write_u32(MINE_GRADIENT_FRAME_SIZE);
    hasher.write_u32(colors.len() as u32);
    for color in colors {
        for channel in color {
            hasher.write_u32(channel.to_bits());
        }
    }
    format!("{MINE_GRADIENT_KEY_PREFIX}/{:016x}.png", hasher.finish())
}

pub fn mine_gradient_texture(colors: &[[f32; 4]]) -> RgbaImage {
    let frame_count = colors.len().max(1);
    let frame_size = MINE_GRADIENT_FRAME_SIZE.max(2);
    let mut image = RgbaImage::new(frame_size * frame_count as u32, frame_size);
    let center = (frame_size as f32 - 1.0) * 0.5;
    let inv_radius = if center > f32::EPSILON {
        1.0 / center
    } else {
        0.0
    };

    for frame in 0..frame_count {
        let x_offset = frame as u32 * frame_size;
        for y in 0..frame_size {
            let dy = y as f32 - center;
            for x in 0..frame_size {
                let dx = x as f32 - center;
                let radius = (dx.mul_add(dx, dy * dy)).sqrt() * inv_radius;
                if radius >= 1.0 {
                    image.put_pixel(x_offset + x, y, Rgba([0, 0, 0, 0]));
                    continue;
                }
                let layer = ((radius * MINE_FILL_LAYERS as f32).ceil() as usize)
                    .saturating_sub(1)
                    .min(MINE_FILL_LAYERS - 1);
                let idx = (frame + colors.len() - (layer % colors.len())) % colors.len();
                let color = colors[idx];
                let edge_alpha = ((1.0 - radius) * center).clamp(0.0, 1.0);
                image.put_pixel(
                    x_offset + x,
                    y,
                    Rgba([
                        mine_grad_byte(color[0]),
                        mine_grad_byte(color[1]),
                        mine_grad_byte(color[2]),
                        mine_grad_byte(color[3] * edge_alpha),
                    ]),
                );
            }
        }
    }

    image
}

pub fn mine_gradient_sample_region(
    texture_size: [u32; 2],
    src: [i32; 2],
    size: [i32; 2],
    frame_size: Option<[i32; 2]>,
) -> Result<MineGradientSampleRegion, MineGradientSampleRegionError> {
    let mut width = size[0];
    let mut height = size[1];
    if (width <= 0 || height <= 0)
        && let Some(frame) = frame_size
    {
        width = frame[0];
        height = frame[1];
    }

    if width <= 0 || height <= 0 {
        return Err(MineGradientSampleRegionError::InvalidSlotSize);
    }

    let src_x = src[0].max(0) as u32;
    let src_y = src[1].max(0) as u32;
    if src_x >= texture_size[0] || src_y >= texture_size[1] {
        return Err(MineGradientSampleRegionError::RegionOutsideTexture);
    }

    let mut sample_width = width as u32;
    let mut sample_height = height as u32;
    if src_x + sample_width > texture_size[0] {
        sample_width = texture_size[0].saturating_sub(src_x);
    }
    if src_y + sample_height > texture_size[1] {
        sample_height = texture_size[1].saturating_sub(src_y);
    }

    if sample_width == 0 || sample_height == 0 {
        return Err(MineGradientSampleRegionError::ZeroSampleSize);
    }

    Ok(MineGradientSampleRegion {
        src: [src_x, src_y],
        size: [sample_width, sample_height],
    })
}

pub fn mine_gradient_samples(
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
            r += f32::from(pixel[0]) * a;
            g += f32::from(pixel[1]) * a;
            b += f32::from(pixel[2]) * a;
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

pub fn mine_gradient_resample(colors: &[[f32; 4]], sample_count: usize) -> Option<Vec<[f32; 4]>> {
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
        let base_index = position.floor() as usize;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mine_fill_slots_maps_present_mines_only() {
        let fills = mine_fill_slots(&[Some(2), None, Some(4)], |mine| Some(mine * 10));
        assert_eq!(fills, vec![Some(20), None, Some(40)]);
    }

    #[test]
    fn mine_fill_slots_keeps_failed_fills_empty() {
        let fills = mine_fill_slots(&[Some(2), Some(3)], |mine| {
            (mine % 2 == 0).then_some(mine * 10)
        });
        assert_eq!(fills, vec![Some(20), None]);
    }

    #[test]
    fn mine_gradient_slot_plan_describes_generated_sheet() {
        let colors = [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]];
        let plan = mine_gradient_slot_plan(&colors);

        assert_eq!(
            plan.tex_dims,
            (MINE_GRADIENT_FRAME_SIZE * 2, MINE_GRADIENT_FRAME_SIZE)
        );
        assert_eq!(
            plan.frame_size,
            [
                MINE_GRADIENT_FRAME_SIZE as i32,
                MINE_GRADIENT_FRAME_SIZE as i32
            ]
        );
        assert_eq!(plan.frame_count, 2);
        assert_eq!(plan.texture_key, mine_gradient_texture_key(&colors));
    }

    #[test]
    fn mine_gradient_key_changes_with_colors() {
        let a = mine_gradient_texture_key(&[[1.0, 0.0, 0.0, 1.0]]);
        let b = mine_gradient_texture_key(&[[0.0, 1.0, 0.0, 1.0]]);

        assert_ne!(a, b);
        assert!(a.starts_with(MINE_GRADIENT_KEY_PREFIX));
        assert!(a.ends_with(".png"));
    }

    #[test]
    fn mine_gradient_texture_uses_one_frame_per_color() {
        let colors = [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]];
        let image = mine_gradient_texture(&colors);

        assert_eq!(
            image.width(),
            MINE_GRADIENT_FRAME_SIZE * colors.len() as u32
        );
        assert_eq!(image.height(), MINE_GRADIENT_FRAME_SIZE);
        assert_eq!(image.get_pixel(0, 0), &Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn mine_gradient_sample_region_uses_frame_size_and_clamps() {
        let region =
            mine_gradient_sample_region([10, 8], [7, 6], [0, 0], Some([5, 5])).expect("region");

        assert_eq!(
            region,
            MineGradientSampleRegion {
                src: [7, 6],
                size: [3, 2],
            }
        );
    }

    #[test]
    fn mine_gradient_sample_region_rejects_invalid_or_outside_regions() {
        assert_eq!(
            mine_gradient_sample_region([10, 8], [0, 0], [0, 2], None),
            Err(MineGradientSampleRegionError::InvalidSlotSize)
        );
        assert_eq!(
            mine_gradient_sample_region([10, 8], [10, 0], [1, 1], None),
            Err(MineGradientSampleRegionError::RegionOutsideTexture)
        );
    }

    #[test]
    fn mine_gradient_samples_average_alpha_weighted_columns() {
        let mut image = RgbaImage::new(2, 2);
        image.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        image.put_pixel(0, 1, Rgba([0, 0, 255, 0]));
        image.put_pixel(1, 0, Rgba([0, 255, 0, 128]));
        image.put_pixel(1, 1, Rgba([0, 0, 255, 128]));

        let samples = mine_gradient_samples(&image, [0, 0], [2, 2], 2).expect("samples");

        assert_eq!(samples[0], [1.0, 0.0, 0.0, 1.0]);
        assert!((samples[1][1] - 0.5).abs() < 0.01);
        assert!((samples[1][2] - 0.5).abs() < 0.01);
        assert_eq!(samples[1][3], 1.0);
    }

    #[test]
    fn mine_gradient_resample_interpolates_and_expands_single_color() {
        let gradient = mine_gradient_resample(&[[1.0, 0.0, 0.0, 0.5], [0.0, 0.0, 1.0, 0.5]], 3)
            .expect("gradient");
        assert_eq!(gradient[1], [0.5, 0.0, 0.5, 1.0]);

        let solid = mine_gradient_resample(&[[0.25, 0.5, 0.75, 0.1]], 2).expect("solid");
        assert_eq!(solid, vec![[0.25, 0.5, 0.75, 1.0]; 2]);
    }
}
