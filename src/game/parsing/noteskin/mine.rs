use super::{
    AnimationRate, ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelTweenSegment,
    SpriteDefinition, SpriteSlot, SpriteSource,
};
use crate::assets;
use deadlib_platform::dirs;
use deadlib_render::SamplerDesc;
use image::{Rgba, RgbaImage};
use log::warn;
use std::hash::Hasher;
use std::path::Path;
use std::sync::{Arc, atomic::AtomicU64};
use twox_hash::XxHash64;

const MINE_GRADIENT_SAMPLES: usize = 64;
const MINE_FILL_LAYERS: usize = 32;
const MINE_GRADIENT_FRAME_SIZE: u32 = 64;
const MINE_GRADIENT_KEY_PREFIX: &str = "generated/noteskins/mine_fill";

pub(super) fn mine_fill_slots(mines: &[Option<SpriteSlot>]) -> Vec<Option<SpriteSlot>> {
    mines
        .iter()
        .map(|slot| {
            slot.as_ref().and_then(|mine| {
                let colors = load_mine_gradient_colors(mine)?;
                Some(build_mine_gradient_slot(&colors))
            })
        })
        .collect()
}

fn load_mine_gradient_colors(slot: &SpriteSlot) -> Option<Vec<[f32; 4]>> {
    let texture_key = slot.texture_key();
    let candidate = Path::new("assets").join(texture_key);
    let path = dirs::app_dirs().resolve_asset_path(&candidate.to_string_lossy());
    let image = assets::open_image_fallback(&path).ok()?.to_rgba8();

    let mut width = slot.def.size[0];
    let mut height = slot.def.size[1];
    if (width <= 0 || height <= 0)
        && let Some(frame) = slot.source.frame_size()
    {
        width = frame[0];
        height = frame[1];
    }

    if width <= 0 || height <= 0 {
        warn!("Mine fill slot has invalid size for gradient sampling");
        return None;
    }

    let src_x = slot.def.src[0].max(0) as u32;
    let src_y = slot.def.src[1].max(0) as u32;
    let mut sample_width = width as u32;
    let mut sample_height = height as u32;

    if src_x >= image.width() || src_y >= image.height() {
        warn!("Mine fill region ({src_x}, {src_y}) is outside of texture {texture_key}");
        return None;
    }

    if src_x + sample_width > image.width() {
        sample_width = image.width().saturating_sub(src_x);
    }
    if src_y + sample_height > image.height() {
        sample_height = image.height().saturating_sub(src_y);
    }

    if sample_width == 0 || sample_height == 0 {
        warn!("Mine fill region has zero sample size for texture {texture_key}");
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

    if colors.is_empty() {
        return None;
    }

    if colors.len() == 1 {
        let mut color = colors[0];
        color[3] = 1.0;
        return Some(vec![color; MINE_GRADIENT_SAMPLES.max(1)]);
    }

    let max_index = (colors.len() - 1) as f32;
    let mut samples = Vec::with_capacity(MINE_GRADIENT_SAMPLES);
    let divisor = (MINE_GRADIENT_SAMPLES.saturating_sub(1)).max(1) as f32;
    for i in 0..MINE_GRADIENT_SAMPLES {
        let t = i as f32 / divisor;
        let position = t * max_index;
        let base_index = position.floor() as usize;
        let next_index = (base_index + 1).min(colors.len() - 1);
        let frac = (position - base_index as f32).clamp(0.0, 1.0);

        let c0 = colors[base_index];
        let c1 = colors[next_index];
        let mut sampled = [
            (c1[0] - c0[0]).mul_add(frac, c0[0]),
            (c1[1] - c0[1]).mul_add(frac, c0[1]),
            (c1[2] - c0[2]).mul_add(frac, c0[2]),
            1.0,
        ];

        sampled[0] = sampled[0].clamp(0.0, 1.0);
        sampled[1] = sampled[1].clamp(0.0, 1.0);
        sampled[2] = sampled[2].clamp(0.0, 1.0);

        samples.push(sampled);
    }

    Some(samples)
}

#[inline(always)]
fn mine_grad_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn mine_gradient_texture_key(colors: &[[f32; 4]]) -> String {
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

fn mine_gradient_texture(colors: &[[f32; 4]]) -> RgbaImage {
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

fn build_mine_gradient_slot(colors: &[[f32; 4]]) -> SpriteSlot {
    let texture_key = mine_gradient_texture_key(colors);
    if assets::texture_dims(&texture_key).is_none() {
        let texture = mine_gradient_texture(colors);
        assets::register_generated_texture(&texture_key, texture, SamplerDesc::default());
    }

    let frame_count = colors.len().max(1);
    let frame_size = MINE_GRADIENT_FRAME_SIZE as i32;
    let tex_dims = (
        MINE_GRADIENT_FRAME_SIZE * frame_count as u32,
        MINE_GRADIENT_FRAME_SIZE,
    );
    let source = Arc::new(SpriteSource::Animated {
        texture_key: texture_key.into(),
        tex_dims,
        frame_size: [frame_size, frame_size],
        grid: (frame_count, 1),
        frame_count,
        frame_indices: None,
        rate: AnimationRate::FramesPerBeat(1.0),
        frame_durations: None,
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });

    SpriteSlot {
        def: SpriteDefinition {
            src: [0, 0],
            size: [frame_size, frame_size],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        base_rot_sin_cos: [0.0, 1.0],
        source_size: [frame_size, frame_size],
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        note_color_translate: false,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    }
}
