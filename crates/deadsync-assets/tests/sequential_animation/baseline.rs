// Frozen from 1a2129e19 (0.5.1652).
use super::*;

fn source_from_plan(plan: SpriteSourcePlan, def: &SpriteDefinition) -> Arc<SpriteSource> {
    match plan {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        } => {
            let texel_scale = texture_texel_scale(tex_dims);
            Arc::new(SpriteSource::Atlas {
                texture_key: texture_key.into(),
                tex_dims,
                texel_scale,
                uv_cache: SpriteAtlasUvCache::new(texel_scale, def),
                cached_handle: AtomicU64::new(deadlib_render_core::INVALID_TEXTURE_HANDLE),
                cached_generation: AtomicU64::new(u64::MAX),
                cached_actor_texture: AtomicU64::new(0),
            })
        }
        SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            rate,
            frame_durations,
        } => {
            let texel_scale = texture_texel_scale(tex_dims);
            let frame_timing = frame_durations
                .as_deref()
                .map(|durations| SpriteFrameTiming::new(frame_count, durations));
            let uv_cache = SpriteAnimatedUvCache::new(
                texel_scale,
                def,
                frame_size,
                [grid.0, grid.1],
                frame_count,
                frame_indices.is_some(),
            );
            Arc::new(SpriteSource::Animated {
                texture_key: texture_key.into(),
                tex_dims,
                texel_scale,
                frame_size,
                grid,
                frame_count,
                frame_indices: frame_indices.map(Arc::<[usize]>::from),
                rate,
                frame_durations: frame_durations.map(Arc::<[f32]>::from),
                frame_timing,
                uv_cache,
                cached_handle: AtomicU64::new(deadlib_render_core::INVALID_TEXTURE_HANDLE),
                cached_generation: AtomicU64::new(u64::MAX),
                cached_actor_texture: AtomicU64::new(0),
            })
        }
    }
}

pub(super) fn itg_note_animation_source(
    slot: &SpriteSlot,
    animation: NotePartAnimation,
    translate: NotePartTextureTranslate,
    beat_based: bool,
) -> Option<Arc<SpriteSource>> {
    if slot.model.is_some() || matches!(slot.source.as_ref(), SpriteSource::Animated { .. }) {
        return None;
    }
    let key = slot.texture_key();
    let (grid_x, grid_y) = assets::sprite_sheet_dims(key);
    let (grid_x, grid_y) = (grid_x.max(1) as usize, grid_y.max(1) as usize);
    if grid_x.saturating_mul(grid_y) <= 1 {
        return None;
    }

    let color_x = translate.note_color_spacing[0].abs() > f32::EPSILON;
    let color_y = translate.note_color_spacing[1].abs() > f32::EPSILON;
    if color_x && color_y {
        return None;
    }
    let frame_w = slot.def.size[0].abs().max(1);
    let frame_h = slot.def.size[1].abs().max(1);
    let base_col = (slot.def.src[0].max(0) / frame_w) as usize % grid_x;
    let base_row = (slot.def.src[1].max(0) / frame_h) as usize % grid_y;
    let frame_indices = match (color_x, color_y) {
        (false, false) => (0..grid_x * grid_y).collect::<Vec<_>>(),
        (true, false) => (0..grid_y).map(|row| row * grid_x + base_col).collect(),
        (false, true) => (0..grid_x).map(|col| base_row * grid_x + col).collect(),
        (true, true) => unreachable!(),
    };
    if frame_indices.len() <= 1 {
        return None;
    }

    let tex_dims = match slot.source.as_ref() {
        SpriteSource::Atlas { tex_dims, .. } => *tex_dims,
        SpriteSource::Animated { .. } => return None,
    };
    let frames_per_cycle = frame_indices.len() as f32 / animation.length.max(1e-6);
    Some(source_from_plan(
        SpriteSourcePlan::Animated {
            texture_key: key.to_string(),
            tex_dims,
            frame_size: [frame_w, frame_h],
            grid: (grid_x, grid_y),
            frame_count: frame_indices.len(),
            frame_indices: Some(frame_indices),
            rate: if beat_based {
                AnimationRate::FramesPerBeat(frames_per_cycle)
            } else {
                AnimationRate::FramesPerSecond(frames_per_cycle)
            },
            frame_durations: None,
        },
        &slot.def,
    ))
}
