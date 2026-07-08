#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpriteDefinition {
    pub src: [i32; 2],
    pub size: [i32; 2],
    pub rotation_deg: i32,
    pub mirror_h: bool,
    pub mirror_v: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationRate {
    FramesPerSecond(f32),
    FramesPerBeat(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteStatePropertiesAnimation {
    pub frame_size: [i32; 2],
    pub start_src: [i32; 2],
    pub frame_count: usize,
    pub frame_durations: Vec<f32>,
    pub rate: AnimationRate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteFramePlan {
    pub def: SpriteDefinition,
    pub frame_size: [i32; 2],
    pub grid: [usize; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteAnimationPlan {
    pub def: SpriteDefinition,
    pub frame_size: [i32; 2],
    pub grid: [usize; 2],
    pub frame_count: usize,
    pub frame_indices: Option<Vec<usize>>,
    pub frame_durations: Option<Vec<f32>>,
    pub rate: AnimationRate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpriteSourcePlan {
    Atlas {
        texture_key: String,
        tex_dims: (u32, u32),
    },
    Animated {
        texture_key: String,
        tex_dims: (u32, u32),
        frame_size: [i32; 2],
        grid: (usize, usize),
        frame_count: usize,
        frame_indices: Option<Vec<usize>>,
        rate: AnimationRate,
        frame_durations: Option<Vec<f32>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteSlotPlan {
    pub def: SpriteDefinition,
    pub source_size: [i32; 2],
    pub source: SpriteSourcePlan,
    pub note_color_translate: bool,
}

#[inline(always)]
pub fn neg_rot_sin_cos(rotation_deg: i32) -> [f32; 2] {
    match rotation_deg.rem_euclid(360) {
        0 => [0.0, 1.0],
        90 => [-1.0, 0.0],
        180 => [0.0, -1.0],
        270 => [1.0, 0.0],
        _ => {
            let (sin_r, cos_r) = (-(rotation_deg as f32)).to_radians().sin_cos();
            [sin_r, cos_r]
        }
    }
}

pub fn atlas_sprite_slot_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    source_frame: (u32, u32),
    note_color_translate: bool,
) -> SpriteSlotPlan {
    SpriteSlotPlan {
        def: SpriteDefinition {
            src: [0, 0],
            size: [tex_dims.0 as i32, tex_dims.1 as i32],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source: SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        },
        note_color_translate,
    }
}

pub fn frame_sprite_slot_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    sheet_grid: (usize, usize),
    frame: usize,
    source_frame: (u32, u32),
    note_color_translate: bool,
) -> SpriteSlotPlan {
    let frame = sprite_sheet_frame(
        [tex_dims.0, tex_dims.1],
        [sheet_grid.0.max(1), sheet_grid.1.max(1)],
        frame,
    );
    SpriteSlotPlan {
        def: frame.def,
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source: SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        },
        note_color_translate,
    }
}

pub fn animation_sprite_slot_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    sheet_grid: (usize, usize),
    frame0: usize,
    frame_count: usize,
    frame_indices: Option<&[usize]>,
    frame_delays: Option<&[f32]>,
    beat_based: bool,
    source_frame: (u32, u32),
    note_color_translate: bool,
) -> Option<SpriteSlotPlan> {
    let plan = sprite_animation_plan(
        [tex_dims.0, tex_dims.1],
        [sheet_grid.0.max(1), sheet_grid.1.max(1)],
        frame0,
        frame_count,
        frame_indices,
        frame_delays,
        beat_based,
    )?;
    Some(animation_plan_to_slot_plan(
        texture_key,
        tex_dims,
        source_frame,
        plan,
        note_color_translate,
    ))
}

pub fn all_frames_sprite_slot_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    sheet_grid: (usize, usize),
    frame_delay: Option<f32>,
    beat_based: bool,
    source_frame: (u32, u32),
    note_color_translate: bool,
) -> Option<SpriteSlotPlan> {
    let plan = sprite_all_frames_animation_plan(
        [tex_dims.0, tex_dims.1],
        [sheet_grid.0.max(1), sheet_grid.1.max(1)],
        frame_delay,
        beat_based,
    )?;
    Some(animation_plan_to_slot_plan(
        texture_key,
        tex_dims,
        source_frame,
        plan,
        note_color_translate,
    ))
}

pub fn animation_plan_to_slot_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    source_frame: (u32, u32),
    plan: SpriteAnimationPlan,
    note_color_translate: bool,
) -> SpriteSlotPlan {
    SpriteSlotPlan {
        def: plan.def,
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source: SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size: plan.frame_size,
            grid: (plan.grid[0], plan.grid[1]),
            frame_count: plan.frame_count,
            frame_indices: plan.frame_indices,
            rate: plan.rate,
            frame_durations: plan.frame_durations,
        },
        note_color_translate,
    }
}

pub fn generated_animation_sprite_slot_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    frame_size: [i32; 2],
    frame_count: usize,
    rate: AnimationRate,
    note_color_translate: bool,
) -> SpriteSlotPlan {
    SpriteSlotPlan {
        def: SpriteDefinition {
            src: [0, 0],
            size: frame_size,
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        source_size: frame_size,
        source: SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid: (frame_count, 1),
            frame_count,
            frame_indices: None,
            rate,
            frame_durations: None,
        },
        note_color_translate,
    }
}

pub fn state_properties_source_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    sheet_grid: (usize, usize),
    animation: SpriteStatePropertiesAnimation,
) -> SpriteSourcePlan {
    SpriteSourcePlan::Animated {
        texture_key,
        tex_dims,
        frame_size: animation.frame_size,
        grid: (sheet_grid.0.max(1), sheet_grid.1.max(1)),
        frame_count: animation.frame_count,
        frame_indices: None,
        rate: animation.rate,
        frame_durations: Some(animation.frame_durations),
    }
}

pub fn all_state_delays_source_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    frame_size: [i32; 2],
    grid: (usize, usize),
    frame_count: usize,
    frame_indices: Option<Vec<usize>>,
    delay: f32,
    beat_based: bool,
) -> SpriteSourcePlan {
    let frame_count = frame_count.max(1);
    let delay = delay.max(1e-6);
    SpriteSourcePlan::Animated {
        texture_key,
        tex_dims,
        frame_size,
        grid,
        frame_count,
        frame_indices,
        rate: if beat_based {
            AnimationRate::FramesPerBeat(1.0 / delay)
        } else {
            AnimationRate::FramesPerSecond(1.0 / delay)
        },
        frame_durations: Some(vec![delay; frame_count]),
    }
}

pub fn sprite_sheet_frame(
    tex_dims: [u32; 2],
    sheet_grid: [usize; 2],
    frame: usize,
) -> SpriteFramePlan {
    let cols = sheet_grid[0].max(1);
    let rows = sheet_grid[1].max(1);
    let frame_count = (cols * rows).max(1);
    let idx = frame % frame_count;
    let col = idx % cols;
    let row = idx / cols;
    let frame_w = (tex_dims[0] / cols as u32).max(1) as i32;
    let frame_h = (tex_dims[1] / rows as u32).max(1) as i32;

    SpriteFramePlan {
        def: SpriteDefinition {
            src: [col as i32 * frame_w, row as i32 * frame_h],
            size: [frame_w, frame_h],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        frame_size: [frame_w, frame_h],
        grid: [cols, rows],
    }
}

pub fn sprite_animation_plan(
    tex_dims: [u32; 2],
    sheet_grid: [usize; 2],
    frame0: usize,
    frame_count: usize,
    frame_indices: Option<&[usize]>,
    frame_delays: Option<&[f32]>,
    beat_based: bool,
) -> Option<SpriteAnimationPlan> {
    let cols = sheet_grid[0].max(1);
    let rows = sheet_grid[1].max(1);
    let available = (cols * rows).max(1);
    if available <= 1 || frame_count <= 1 {
        return None;
    }

    let anim_frames = if frame_indices.is_some() {
        frame_count.max(1)
    } else {
        frame_count.min(available).max(1)
    };
    let frame = sprite_sheet_frame(tex_dims, [cols, rows], frame0);
    let start = frame0 % available;
    let default_delay = frame_delays
        .and_then(|delays| delays.first().copied())
        .unwrap_or(1.0)
        .max(1e-6);
    let rate = if beat_based {
        AnimationRate::FramesPerBeat(1.0 / default_delay)
    } else {
        AnimationRate::FramesPerSecond(1.0 / default_delay)
    };
    let frame_durations = frame_delays
        .map(|delays| {
            let mut normalized = Vec::with_capacity(anim_frames);
            let fallback = delays.first().copied().unwrap_or(1.0).max(0.0);
            for idx in 0..anim_frames {
                normalized.push(delays.get(idx).copied().unwrap_or(fallback).max(0.0));
            }
            normalized
        })
        .filter(|durations| !durations.is_empty());
    let frame_indices = frame_indices
        .map(|indices| {
            let mut normalized = Vec::with_capacity(anim_frames);
            let fallback = indices.first().copied().unwrap_or(start);
            for idx in 0..anim_frames {
                normalized.push(indices.get(idx).copied().unwrap_or(fallback));
            }
            normalized
        })
        .filter(|indices| !indices.is_empty());

    Some(SpriteAnimationPlan {
        def: frame.def,
        frame_size: frame.frame_size,
        grid: frame.grid,
        frame_count: anim_frames,
        frame_indices,
        frame_durations,
        rate,
    })
}

pub fn sprite_all_frames_animation_plan(
    tex_dims: [u32; 2],
    sheet_grid: [usize; 2],
    frame_delay: Option<f32>,
    beat_based: bool,
) -> Option<SpriteAnimationPlan> {
    let cols = sheet_grid[0].max(1);
    let rows = sheet_grid[1].max(1);
    let frame_count = cols.saturating_mul(rows).max(1);
    if frame_count <= 1 {
        return None;
    }
    let delays = frame_delay.map(|delay| vec![delay.max(1e-6); frame_count]);
    sprite_animation_plan(
        tex_dims,
        [cols, rows],
        0,
        frame_count,
        None,
        delays.as_deref(),
        beat_based,
    )
}

pub fn sprite_frame_index(
    frame_count: usize,
    rate: AnimationRate,
    frame_durations: Option<&[f32]>,
    time: f32,
    beat: f32,
) -> usize {
    let frames = frame_count.max(1);
    if frames <= 1 {
        return 0;
    }
    if let Some(durations) = frame_durations {
        let clock = match rate {
            AnimationRate::FramesPerSecond(_) => time,
            AnimationRate::FramesPerBeat(_) => beat,
        };
        if let Some(total) = frame_duration_total(durations, frames)
            && let Some(idx) = duration_frame_index(durations, frames, clock.rem_euclid(total))
        {
            return idx;
        }
    }
    let frame = match rate {
        AnimationRate::FramesPerSecond(fps) if fps > 0.0 => (time * fps).floor() as isize,
        AnimationRate::FramesPerBeat(frames_per_beat) if frames_per_beat > 0.0 => {
            (beat * frames_per_beat).floor() as isize
        }
        _ => return 0,
    };
    ((frame % frames as isize) + frames as isize) as usize % frames
}

pub fn sprite_frame_index_from_phase(
    frame_count: usize,
    frame_durations: Option<&[f32]>,
    phase: f32,
) -> usize {
    let frames = frame_count.max(1);
    if frames <= 1 {
        return 0;
    }
    let p = phase.rem_euclid(1.0);
    if let Some(durations) = frame_durations
        && let Some(total) = frame_duration_total(durations, frames)
        && let Some(idx) = duration_frame_index(durations, frames, p * total)
    {
        return idx;
    }
    ((p * frames as f32).floor() as usize).min(frames - 1)
}

pub fn sprite_atlas_uv(tex_dims: [u32; 2], def: &SpriteDefinition, inset_texels: bool) -> [f32; 4] {
    let tw = tex_dims[0].max(1) as f32;
    let th = tex_dims[1].max(1) as f32;
    let mut u0 = def.src[0] as f32;
    let mut v0 = def.src[1] as f32;
    let mut u1 = (def.src[0] + def.size[0]) as f32;
    let mut v1 = (def.src[1] + def.size[1]) as f32;

    if inset_texels {
        if def.size[0] > 0 {
            u0 += 0.5;
            u1 -= 0.5;
        }
        if def.size[1] > 0 {
            v0 += 0.5;
            v1 -= 0.5;
        }
    }

    [u0 / tw, v0 / th, u1 / tw, v1 / th]
}

pub fn sprite_animated_uv(
    tex_dims: [u32; 2],
    def: &SpriteDefinition,
    frame_size: [i32; 2],
    grid: [usize; 2],
    frame_count: usize,
    frame_indices: Option<&[usize]>,
    frame_index: usize,
    inset_texels: bool,
) -> [f32; 4] {
    let frames = frame_count.max(1);
    let idx = frame_index % frames;
    let cols = grid[0].max(1);
    let available = cols.saturating_mul(grid[1].max(1)).max(1);
    let source_idx = frame_indices
        .and_then(|indices| indices.get(idx).copied())
        .map_or(idx, |idx| idx % available);
    let row = source_idx / cols;
    let col = source_idx % cols;
    let (src_x, src_y) = if frame_indices.is_some() {
        (col as i32 * frame_size[0], row as i32 * frame_size[1])
    } else {
        (
            def.src[0] + (col as i32 * frame_size[0]),
            def.src[1] + (row as i32 * frame_size[1]),
        )
    };
    let frame_def = SpriteDefinition {
        src: [src_x, src_y],
        size: frame_size,
        rotation_deg: 0,
        mirror_h: false,
        mirror_v: false,
    };
    sprite_atlas_uv(tex_dims, &frame_def, inset_texels)
}

pub fn sprite_uv_scroll_clock(elapsed: f32, cycle_seconds: Option<f32>) -> f32 {
    cycle_seconds
        .filter(|total| *total > f32::EPSILON && total.is_finite())
        .map_or(elapsed, |total| elapsed.rem_euclid(total) / total)
}

pub fn sprite_scrolled_uv(
    mut uv: [f32; 4],
    uv_velocity: [f32; 2],
    uv_offset: [f32; 2],
    elapsed: f32,
    model_cycle_seconds: Option<f32>,
) -> [f32; 4] {
    if uv_velocity == [0.0, 0.0] && uv_offset == [0.0, 0.0] {
        return uv;
    }

    let w = (uv[2] - uv[0]).abs();
    let h = (uv[3] - uv[1]).abs();
    if let Some(cycle_seconds) = model_cycle_seconds {
        let clock = sprite_uv_scroll_clock(elapsed, Some(cycle_seconds));
        let shift_u = uv_offset[0] + uv_velocity[0] * clock;
        let shift_v = uv_offset[1] + uv_velocity[1] * clock;
        uv[0] += shift_u;
        uv[2] += shift_u;
        uv[1] += shift_v;
        uv[3] += shift_v;
    } else {
        let shift_u = uv_offset[0] + uv_velocity[0] * elapsed;
        let shift_v = uv_offset[1] + uv_velocity[1] * elapsed;
        let u_span = (1.0 - w).max(0.0);
        let v_span = (1.0 - h).max(0.0);
        let u_shift = if u_span > f32::EPSILON {
            shift_u.rem_euclid(u_span)
        } else {
            0.0
        };
        let v_shift = if v_span > f32::EPSILON {
            shift_v.rem_euclid(v_span)
        } else {
            0.0
        };
        uv[0] += u_shift;
        uv[2] += u_shift;
        uv[1] += v_shift;
        uv[3] += v_shift;
    }
    uv
}

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

#[inline(always)]
pub fn frame_duration_total(durations: &[f32], frames: usize) -> Option<f32> {
    let total = durations.iter().take(frames).fold(0.0, |sum, duration| {
        if *duration > f32::EPSILON {
            sum + *duration
        } else {
            sum
        }
    });
    (total > f32::EPSILON && total.is_finite()).then_some(total)
}

#[inline(always)]
pub fn duration_frame_index(durations: &[f32], frames: usize, mut position: f32) -> Option<usize> {
    let mut last = None;
    for (idx, duration) in durations.iter().take(frames).enumerate() {
        let span = (*duration).max(0.0);
        if span <= f32::EPSILON {
            continue;
        }
        last = Some(idx);
        if position < span {
            return Some(idx);
        }
        position -= span;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationRate, SpriteAnimationPlan, SpriteDefinition, SpriteSourcePlan,
        SpriteStatePropertiesAnimation, all_frames_sprite_slot_plan, atlas_sprite_slot_plan,
        duration_frame_index, frame_duration_total, frame_sprite_slot_plan,
        generated_animation_sprite_slot_plan, neg_rot_sin_cos, sprite_all_frames_animation_plan,
        sprite_animated_uv, sprite_animation_plan, sprite_atlas_uv, sprite_frame_index,
        sprite_frame_index_from_phase, sprite_scrolled_uv, sprite_sheet_frame,
        sprite_state_properties_animation,
    };

    #[test]
    fn neg_rotation_uses_exact_cardinal_values() {
        assert_eq!(neg_rot_sin_cos(0), [0.0, 1.0]);
        assert_eq!(neg_rot_sin_cos(90), [-1.0, 0.0]);
        assert_eq!(neg_rot_sin_cos(180), [0.0, -1.0]);
        assert_eq!(neg_rot_sin_cos(270), [1.0, 0.0]);
        assert_eq!(neg_rot_sin_cos(-90), [1.0, 0.0]);
    }

    #[test]
    fn frame_duration_total_skips_non_positive_spans() {
        assert_eq!(frame_duration_total(&[0.1, 0.0, -1.0, 0.2], 4), Some(0.3));
        assert_eq!(frame_duration_total(&[0.0, -1.0], 2), None);
    }

    #[test]
    fn atlas_slot_plan_uses_full_texture() {
        let plan = atlas_sprite_slot_plan("tap.png".to_string(), (128, 64), (64, 32), true);

        assert_eq!(plan.def.src, [0, 0]);
        assert_eq!(plan.def.size, [128, 64]);
        assert_eq!(plan.source_size, [64, 32]);
        assert!(plan.note_color_translate);
        assert_eq!(
            plan.source,
            SpriteSourcePlan::Atlas {
                texture_key: "tap.png".to_string(),
                tex_dims: (128, 64),
            }
        );
    }

    #[test]
    fn frame_slot_plan_uses_sheet_frame() {
        let plan =
            frame_sprite_slot_plan("tap.png".to_string(), (128, 64), (4, 2), 5, (32, 32), true);

        assert_eq!(plan.def.src, [32, 32]);
        assert_eq!(plan.def.size, [32, 32]);
        assert_eq!(plan.source_size, [32, 32]);
    }

    #[test]
    fn generated_animation_slot_plan_builds_single_row_source() {
        let plan = generated_animation_sprite_slot_plan(
            "generated/mine".to_string(),
            (256, 64),
            [64, 64],
            4,
            AnimationRate::FramesPerBeat(1.0),
            false,
        );

        assert_eq!(plan.def.size, [64, 64]);
        assert!(!plan.note_color_translate);
        assert_eq!(
            plan.source,
            SpriteSourcePlan::Animated {
                texture_key: "generated/mine".to_string(),
                tex_dims: (256, 64),
                frame_size: [64, 64],
                grid: (4, 1),
                frame_count: 4,
                frame_indices: None,
                rate: AnimationRate::FramesPerBeat(1.0),
                frame_durations: None,
            }
        );
    }

    #[test]
    fn all_frames_slot_plan_returns_none_for_single_frame_sheet() {
        assert!(
            all_frames_sprite_slot_plan(
                "tap.png".to_string(),
                (64, 64),
                (1, 1),
                Some(0.1),
                false,
                (64, 64),
                true,
            )
            .is_none()
        );
    }

    #[test]
    fn duration_frame_index_uses_last_positive_span_as_fallback() {
        let durations = [0.1, 0.0, 0.2];
        assert_eq!(duration_frame_index(&durations, 3, 0.05), Some(0));
        assert_eq!(duration_frame_index(&durations, 3, 0.15), Some(2));
        assert_eq!(duration_frame_index(&durations, 3, 9.0), Some(2));
        assert_eq!(duration_frame_index(&[0.0], 1, 0.0), None);
    }

    #[test]
    fn sprite_sheet_frame_selects_wrapped_frame_region() {
        let frame = sprite_sheet_frame([256, 128], [4, 2], 6);

        assert_eq!(
            frame.def,
            SpriteDefinition {
                src: [128, 64],
                size: [64, 64],
                rotation_deg: 0,
                mirror_h: false,
                mirror_v: false,
            }
        );
        assert_eq!(frame.frame_size, [64, 64]);
        assert_eq!(frame.grid, [4, 2]);
    }

    #[test]
    fn sprite_animation_plan_normalizes_indices_and_delays() {
        let plan = sprite_animation_plan(
            [256, 128],
            [4, 2],
            1,
            4,
            Some(&[2, 3]),
            Some(&[0.25]),
            false,
        )
        .expect("plan");

        assert_eq!(
            plan,
            SpriteAnimationPlan {
                def: SpriteDefinition {
                    src: [64, 0],
                    size: [64, 64],
                    rotation_deg: 0,
                    mirror_h: false,
                    mirror_v: false,
                },
                frame_size: [64, 64],
                grid: [4, 2],
                frame_count: 4,
                frame_indices: Some(vec![2, 3, 2, 2]),
                frame_durations: Some(vec![0.25; 4]),
                rate: AnimationRate::FramesPerSecond(4.0),
            }
        );
    }

    #[test]
    fn all_frames_animation_plan_uses_full_grid_and_delay() {
        let plan = sprite_all_frames_animation_plan([256, 128], [4, 2], Some(0.25), true)
            .expect("multi-frame sheet should animate");

        assert_eq!(plan.frame_count, 8);
        assert_eq!(plan.grid, [4, 2]);
        assert_eq!(plan.frame_size, [64, 64]);
        assert_eq!(plan.rate, AnimationRate::FramesPerBeat(4.0));
        assert_eq!(plan.frame_durations, Some(vec![0.25; 8]));
    }

    #[test]
    fn all_frames_animation_plan_ignores_single_frame_sheet() {
        assert!(sprite_all_frames_animation_plan([64, 64], [1, 1], Some(0.25), false).is_none());
    }

    #[test]
    fn sprite_frame_index_uses_weighted_durations_and_phase() {
        let durations = [0.2, 0.8];

        assert_eq!(
            sprite_frame_index(
                2,
                AnimationRate::FramesPerBeat(1.0),
                Some(&durations),
                0.0,
                0.19
            ),
            0
        );
        assert_eq!(
            sprite_frame_index(
                2,
                AnimationRate::FramesPerBeat(1.0),
                Some(&durations),
                0.0,
                0.20
            ),
            1
        );
        assert_eq!(sprite_frame_index_from_phase(2, Some(&durations), -0.05), 1);
    }

    #[test]
    fn sprite_uv_helpers_apply_texel_inset_and_scrolling() {
        let def = SpriteDefinition {
            src: [0, 0],
            size: [64, 64],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        };

        assert_eq!(
            sprite_atlas_uv([128, 128], &def, true),
            [0.5 / 128.0, 0.5 / 128.0, 63.5 / 128.0, 63.5 / 128.0]
        );
        assert_eq!(
            sprite_animated_uv(
                [128, 128],
                &def,
                [64, 64],
                [2, 1],
                2,
                Some(&[1, 0]),
                0,
                false
            ),
            [0.5, 0.0, 1.0, 0.5]
        );
        assert_eq!(
            sprite_scrolled_uv([0.0, 0.0, 0.25, 0.25], [1.0, 0.0], [0.0, 0.0], 1.0, None),
            [0.25, 0.0, 0.5, 0.25]
        );
        assert_eq!(
            sprite_scrolled_uv(
                [0.0, 0.0, 1.0, 1.0],
                [0.5, 0.0],
                [0.0, 0.0],
                5.0,
                Some(10.0)
            ),
            [0.25, 0.0, 1.25, 1.0]
        );
    }

    #[test]
    fn state_properties_animation_calculates_frame_grid_and_rate() {
        let anim =
            sprite_state_properties_animation([256, 128], [4, 2], [64, 64], 3, &[0.25, 0.5], true)
                .expect("animation");

        assert_eq!(
            anim,
            SpriteStatePropertiesAnimation {
                frame_size: [64, 64],
                start_src: [64, 64],
                frame_count: 3,
                frame_durations: vec![0.25, 0.5, 0.25],
                rate: AnimationRate::FramesPerBeat(4.0),
            }
        );
    }

    #[test]
    fn state_properties_animation_ignores_single_frame_sheets() {
        assert_eq!(
            sprite_state_properties_animation([64, 64], [1, 1], [0, 0], 8, &[0.1], false),
            None
        );
        assert_eq!(
            sprite_state_properties_animation([64, 64], [2, 1], [0, 0], 1, &[0.1], false),
            None
        );
    }
}
