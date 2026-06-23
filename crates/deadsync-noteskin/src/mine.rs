use image::{Rgba, RgbaImage};
use std::hash::Hasher;
use twox_hash::XxHash64;

pub const MINE_GRADIENT_SAMPLES: usize = 64;
pub const MINE_GRADIENT_FRAME_SIZE: u32 = 64;

const MINE_FILL_LAYERS: usize = 32;
const MINE_GRADIENT_KEY_PREFIX: &str = "generated/noteskins/mine_fill";

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
