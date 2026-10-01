// Frozen from d8b34ebb9 (0.5.1658). Unchanged source builders and metadata lookups are shared.
use super::*;
use deadsync_noteskin::SpriteStatePropertiesAnimation;

pub(super) fn source_from_plan(
    plan: SpriteSourcePlan,
    def: &SpriteDefinition,
) -> Arc<SpriteSource> {
    match plan {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        } => Arc::new(atlas_source(texture_key.into(), tex_dims, def)),
        SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            rate,
            frame_durations,
        } => Arc::new(animated_source(
            texture_key.into(),
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices.map(|indices| {
                if indices.is_empty() {
                    Arc::clone(&SEQUENTIAL_FRAME_INDICES)
                } else {
                    Arc::from(indices)
                }
            }),
            rate,
            frame_durations.map(Arc::<[f32]>::from),
            def,
        )),
    }
}

pub(super) fn itg_apply_frame_override(slot: &mut SpriteSlot, frame: usize) {
    if let SpriteSource::Animated {
        frame_count,
        frame_durations,
        rate,
        ..
    } = slot.source.as_ref()
    {
        // setstate selects an animation state, not a new atlas origin.
        slot.animation_start_frame = frame.min(frame_count.saturating_sub(1));
        slot.animation_start_time = frame_durations.as_ref().map_or_else(
            || match rate {
                AnimationRate::FramesPerSecond(rate) | AnimationRate::FramesPerBeat(rate)
                    if *rate > 0.0 =>
                {
                    slot.animation_start_frame as f32 / rate
                }
                _ => 0.0,
            },
            |durations| durations.iter().take(slot.animation_start_frame).sum(),
        );
        return;
    }
    let (tex_w, tex_h) = match slot.source.as_ref() {
        SpriteSource::Atlas { tex_dims, .. } | SpriteSource::Animated { tex_dims, .. } => *tex_dims,
    };
    let (grid_x, grid_y) = assets::sprite_sheet_dims(slot.texture_key());
    let plan = sprite_sheet_frame(
        [tex_w, tex_h],
        [grid_x.max(1) as usize, grid_y.max(1) as usize],
        frame,
    );
    slot.def.src = plan.def.src;
    slot.def.size = plan.def.size;
    let source_plan = source_plan_from_slot(slot);
    slot.source = source_from_plan(source_plan, &slot.def);
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
    let frame_count = match (color_x, color_y) {
        (false, false) => grid_x * grid_y,
        (true, false) => grid_y,
        (false, true) => grid_x,
        (true, true) => unreachable!(),
    };
    if frame_count <= 1 {
        return None;
    }
    let frame_indices = match (color_x, color_y) {
        // Empty explicit indices retain the sheet origin and use the existing
        // identity fallback for every frame without storing the whole range.
        (false, false) => Some(Vec::new()),
        (true, false) => Some((0..grid_y).map(|row| row * grid_x + base_col).collect()),
        (false, true) => Some((0..grid_x).map(|col| base_row * grid_x + col).collect()),
        (true, true) => unreachable!(),
    };

    let tex_dims = match slot.source.as_ref() {
        SpriteSource::Atlas { tex_dims, .. } => *tex_dims,
        SpriteSource::Animated { .. } => return None,
    };
    let frames_per_cycle = frame_count as f32 / animation.length.max(1e-6);
    Some(source_from_plan(
        SpriteSourcePlan::Animated {
            texture_key: key.to_string(),
            tex_dims,
            frame_size: [frame_w, frame_h],
            grid: (grid_x, grid_y),
            frame_count,
            frame_indices,
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

#[inline(always)]
pub(super) fn itg_apply_sprite_animation_plan(
    slot: &mut SpriteSlot,
    plan: deadsync_noteskin::script::SpriteAnimationCommandPlan,
    beat_based: bool,
) {
    if slot.model.is_some() {
        return;
    }
    match plan {
        deadsync_noteskin::script::SpriteAnimationCommandPlan::StateProperties(plan) => {
            apply_state_properties(slot, plan.frame_count, &plan.frame_delays, beat_based);
        }
        deadsync_noteskin::script::SpriteAnimationCommandPlan::AllStateDelays(delay) => {
            apply_all_state_delays(slot, delay, beat_based);
        }
    }
}

pub(super) fn apply_state_properties(
    slot: &mut SpriteSlot,
    frame_count: usize,
    frame_delays: &[f32],
    beat_based: bool,
) {
    let tex_dims = match slot.source.as_ref() {
        SpriteSource::Atlas { tex_dims, .. } | SpriteSource::Animated { tex_dims, .. } => *tex_dims,
    };
    let key = slot.texture_key();
    let (columns, rows) = assets::sprite_sheet_dims(key);
    let Some(animation) = sprite_state_properties_animation(
        [tex_dims.0, tex_dims.1],
        [columns as usize, rows as usize],
        slot.def.src,
        frame_count,
        frame_delays,
        beat_based,
    ) else {
        return;
    };
    let source_frame = assets::texture_source_frame_dims_from_real(key, tex_dims.0, tex_dims.1);
    let key = slot.texture_key_shared();
    slot.def.src = animation.start_src;
    slot.def.size = animation.frame_size;
    slot.source_size = [source_frame.0 as i32, source_frame.1 as i32];
    let source = animated_source(
        key,
        tex_dims,
        animation.frame_size,
        (columns.max(1) as usize, rows.max(1) as usize),
        animation.frame_count,
        None,
        animation.rate,
        Some(Arc::from(animation.frame_durations)),
        &slot.def,
    );
    replace_sprite_source(&mut slot.source, source);
}

#[must_use]
pub fn sprite_state_properties_animation(
    tex_dims: [u32; 2],
    sheet_grid: [usize; 2],
    src: [i32; 2],
    frame_count: usize,
    frame_delays: &[f32],
    beat_based: bool,
) -> Option<SpriteStatePropertiesAnimation> {
    let cols = sheet_grid[0].max(1);
    let rows = sheet_grid[1].max(1);
    let available = (cols * rows).max(1);
    if available <= 1 {
        return None;
    }

    let anim_frames = frame_count.min(available).max(1);
    if anim_frames <= 1 {
        return None;
    }

    let frame_w = (tex_dims[0] / cols as u32).max(1) as i32;
    let frame_h = (tex_dims[1] / rows as u32).max(1) as i32;
    let src_x = src[0].max(0) as usize;
    let src_y = src[1].max(0) as usize;
    let col = (src_x / frame_w.max(1) as usize).min(cols.saturating_sub(1));
    let row = (src_y / frame_h.max(1) as usize).min(rows.saturating_sub(1));
    let start_idx = row
        .saturating_mul(cols)
        .saturating_add(col)
        .min(available - 1);

    let fallback = frame_delays.first().copied().unwrap_or(1.0).max(0.0);
    let mut durations = Vec::with_capacity(anim_frames);
    for idx in 0..anim_frames {
        durations.push(frame_delays.get(idx).copied().unwrap_or(fallback).max(0.0));
    }
    let default_delay = durations.first().copied().unwrap_or(1.0).max(1e-6);
    let rate = if beat_based {
        AnimationRate::FramesPerBeat(1.0 / default_delay)
    } else {
        AnimationRate::FramesPerSecond(1.0 / default_delay)
    };

    let start_col = start_idx % cols;
    let start_row = start_idx / cols;
    Some(SpriteStatePropertiesAnimation {
        frame_size: [frame_w, frame_h],
        start_src: [start_col as i32 * frame_w, start_row as i32 * frame_h],
        frame_count: anim_frames,
        frame_durations: durations,
        rate,
    })
}
