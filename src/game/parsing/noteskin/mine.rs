use super::{
    AnimationRate, ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelTweenSegment,
    SpriteDefinition, SpriteSlot, SpriteSource,
};
use crate::assets;
use deadlib_platform::dirs;
use deadlib_render::SamplerDesc;
use deadsync_noteskin::mine::{
    MINE_GRADIENT_FRAME_SIZE, MINE_GRADIENT_SAMPLES, mine_gradient_texture,
    mine_gradient_texture_key,
};
use log::warn;
use std::path::Path;
use std::sync::{Arc, atomic::AtomicU64};

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
