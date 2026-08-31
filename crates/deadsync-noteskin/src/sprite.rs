use crate::draw::{ModelDrawState, ModelMesh, ModelTweenCursor, ModelVertex};
use crate::script::SpriteAnimationCommandPlan;
use std::path::Path;
use std::sync::Arc;

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

/// Precomputed timing invariants for a weighted sprite animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteFrameTiming {
    total: Option<f32>,
    uniform_duration: Option<f32>,
    duration_count: usize,
}

impl SpriteFrameTiming {
    #[must_use]
    pub fn new(frame_count: usize, durations: &[f32]) -> Self {
        const F32_MANTISSA_MASK: u32 = 0x007f_ffff;

        let duration_count = durations.len().min(frame_count.max(1));
        let durations = &durations[..duration_count];
        let total = frame_duration_total(durations, duration_count);
        let uniform_duration = durations.first().copied().filter(|first| {
            *first > f32::EPSILON
                && first.is_finite()
                // Division by a binary power of two preserves the legacy
                // repeated-subtraction boundary decisions exactly.
                && (first.to_bits() & F32_MANTISSA_MASK) == 0
                && durations
                    .iter()
                    .all(|duration| duration.to_bits() == first.to_bits())
        });
        Self {
            total,
            uniform_duration,
            duration_count,
        }
    }
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

/// Renderer-neutral slot data consumed by canonical noteskin presentation.
///
/// Implementations retain ownership of texture registration and cached render
/// handles; consumers receive only stable keys and noteskin-owned draw data.
pub trait NoteskinSlot: Sized {
    fn sprite_def(&self) -> &SpriteDefinition;
    fn source_size(&self) -> [i32; 2];

    #[inline(always)]
    fn size(&self) -> [i32; 2] {
        self.sprite_def().size
    }

    #[inline(always)]
    fn logical_size(&self) -> [f32; 2] {
        let size = self.source_size();
        [size[0].max(0) as f32, size[1].max(0) as f32]
    }

    fn texture_key_shared(&self) -> Arc<str>;
    fn model(&self) -> Option<&ModelMesh>;
    fn base_rot_sin_cos(&self) -> [f32; 2];

    #[inline(always)]
    fn frame_count(&self) -> usize {
        1
    }

    #[inline(always)]
    fn animation_is_beat_based(&self) -> bool {
        false
    }

    fn frame_index(&self, time: f32, beat: f32) -> usize;
    fn frame_index_from_phase(&self, phase: f32) -> usize;
    fn uv_for_frame_at(&self, frame_index: usize, elapsed: f32) -> [f32; 4];
    fn model_draw_at(&self, time: f32, beat: f32) -> ModelDrawState;

    #[inline(always)]
    fn stable_id(&self) -> u64 {
        (self as *const Self as usize as u64).max(1)
    }

    #[inline(always)]
    fn model_draw_at_cursor(
        &self,
        time: f32,
        beat: f32,
        _cursor: &mut ModelTweenCursor,
    ) -> ModelDrawState {
        self.model_draw_at(time, beat)
    }
    fn model_glow_with_draw(
        &self,
        draw: ModelDrawState,
        time: f32,
        beat: f32,
        diffuse_alpha: f32,
    ) -> Option<[f32; 4]>;
    fn model_uv_params(&self, uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]);
}

#[inline]
#[must_use]
pub fn model_vertex_for_sprite(def: &SpriteDefinition, mut vertex: ModelVertex) -> ModelVertex {
    if def.mirror_h {
        vertex.pos[0] = -vertex.pos[0];
        vertex.uv[0] = 1.0 - vertex.uv[0];
    }
    if def.mirror_v {
        vertex.pos[1] = -vertex.pos[1];
        vertex.uv[1] = 1.0 - vertex.uv[1];
    }
    vertex
}

#[inline(always)]
#[must_use]
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

#[must_use]
pub const fn atlas_sprite_slot_plan(
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

#[must_use]
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

#[must_use]
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

#[must_use]
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

pub fn itg_sprite_slot_plan_from_path(
    path: &Path,
    mut texture_key: impl FnMut(&Path) -> Option<String>,
    mut texture_dimensions: impl FnMut(&str) -> Option<(u32, u32)>,
    mut source_frame_dims: impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let key = texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let source_frame = source_frame_dims(&key, dims.0, dims.1);
    Some(atlas_sprite_slot_plan(key, dims, source_frame, true))
}

pub fn itg_frame_sprite_slot_plan_from_path(
    path: &Path,
    frame: usize,
    mut texture_key: impl FnMut(&Path) -> Option<String>,
    mut texture_dimensions: impl FnMut(&str) -> Option<(u32, u32)>,
    mut sprite_sheet_dims: impl FnMut(&str) -> (u32, u32),
    mut source_frame_dims: impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let key = texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = sprite_sheet_dims(&key);
    let source_frame = source_frame_dims(&key, dims.0, dims.1);
    Some(frame_sprite_slot_plan(
        key,
        dims,
        (grid_x as usize, grid_y as usize),
        frame,
        source_frame,
        true,
    ))
}

pub fn itg_animation_sprite_slot_plan_from_path(
    path: &Path,
    frame0: usize,
    frame_count: usize,
    frame_indices: Option<&[usize]>,
    frame_delays: Option<&[f32]>,
    beat_based: bool,
    mut texture_key: impl FnMut(&Path) -> Option<String>,
    mut texture_dimensions: impl FnMut(&str) -> Option<(u32, u32)>,
    mut sprite_sheet_dims: impl FnMut(&str) -> (u32, u32),
    mut source_frame_dims: impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let key = texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = sprite_sheet_dims(&key);
    let grid = (grid_x as usize, grid_y as usize);
    let source_frame = source_frame_dims(&key, dims.0, dims.1);
    animation_sprite_slot_plan(
        key.clone(),
        dims,
        grid,
        frame0,
        frame_count,
        frame_indices,
        frame_delays,
        beat_based,
        source_frame,
        true,
    )
    .or_else(|| {
        Some(frame_sprite_slot_plan(
            key,
            dims,
            grid,
            frame0,
            source_frame,
            true,
        ))
    })
}

pub fn itg_all_frames_sprite_slot_plan_from_path(
    path: &Path,
    frame_delay: Option<f32>,
    beat_based: bool,
    mut texture_key: impl FnMut(&Path) -> Option<String>,
    mut texture_dimensions: impl FnMut(&str) -> Option<(u32, u32)>,
    mut sprite_sheet_dims: impl FnMut(&str) -> (u32, u32),
    mut source_frame_dims: impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let key = texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (cols, rows) = sprite_sheet_dims(&key);
    let grid = (cols as usize, rows as usize);
    let source_frame = source_frame_dims(&key, dims.0, dims.1);
    all_frames_sprite_slot_plan(
        key.clone(),
        dims,
        grid,
        frame_delay,
        beat_based,
        source_frame,
        true,
    )
    .or_else(|| Some(atlas_sprite_slot_plan(key, dims, source_frame, true)))
}

#[must_use]
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

#[must_use]
pub const fn generated_animation_sprite_slot_plan(
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

#[must_use]
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

#[must_use]
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

pub fn itg_sprite_animation_slot_plan(
    slot: SpriteSlotPlan,
    command: SpriteAnimationCommandPlan,
    beat_based: bool,
    mut sprite_sheet_dims: impl FnMut(&str) -> (u32, u32),
    mut source_frame_dims: impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    match command {
        SpriteAnimationCommandPlan::StateProperties(plan) => itg_state_properties_slot_plan(
            slot,
            plan.frame_count,
            &plan.frame_delays,
            beat_based,
            &mut sprite_sheet_dims,
            &mut source_frame_dims,
        ),
        SpriteAnimationCommandPlan::AllStateDelays(delay) => {
            itg_all_state_delays_slot_plan(slot, delay, beat_based)
        }
    }
}

fn itg_state_properties_slot_plan(
    slot: SpriteSlotPlan,
    frame_count: usize,
    frame_delays: &[f32],
    beat_based: bool,
    sprite_sheet_dims: &mut impl FnMut(&str) -> (u32, u32),
    source_frame_dims: &mut impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let SpriteSlotPlan {
        mut def,
        source,
        note_color_translate,
        ..
    } = slot;
    let (texture_key, tex_dims) = match &source {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        }
        | SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            ..
        } => (texture_key.clone(), *tex_dims),
    };
    let (grid_x, grid_y) = sprite_sheet_dims(&texture_key);
    let animation = sprite_state_properties_animation(
        [tex_dims.0, tex_dims.1],
        [grid_x as usize, grid_y as usize],
        def.src,
        frame_count,
        frame_delays,
        beat_based,
    )?;

    def.src = animation.start_src;
    def.size = animation.frame_size;
    let source_frame = source_frame_dims(&texture_key, tex_dims.0, tex_dims.1);
    Some(SpriteSlotPlan {
        def,
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source: state_properties_source_plan(
            texture_key,
            tex_dims,
            (grid_x as usize, grid_y as usize),
            animation,
        ),
        note_color_translate,
    })
}

fn itg_all_state_delays_slot_plan(
    slot: SpriteSlotPlan,
    delay: f32,
    beat_based: bool,
) -> Option<SpriteSlotPlan> {
    let SpriteSlotPlan {
        def,
        source_size,
        source,
        note_color_translate,
    } = slot;
    let SpriteSourcePlan::Animated {
        texture_key,
        tex_dims,
        frame_size,
        grid,
        frame_count,
        frame_indices,
        ..
    } = source
    else {
        return None;
    };
    Some(SpriteSlotPlan {
        def,
        source_size,
        source: all_state_delays_source_plan(
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            delay,
            beat_based,
        ),
        note_color_translate,
    })
}

#[must_use]
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

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn sprite_frame_index(
    frame_count: usize,
    rate: AnimationRate,
    frame_durations: Option<&[f32]>,
    time: f32,
    beat: f32,
) -> usize {
    sprite_frame_index_with_timing(frame_count, rate, frame_durations, None, time, beat)
}

#[must_use]
pub fn sprite_frame_index_with_timing(
    frame_count: usize,
    rate: AnimationRate,
    frame_durations: Option<&[f32]>,
    timing: Option<SpriteFrameTiming>,
    time: f32,
    beat: f32,
) -> usize {
    let frames = frame_count.max(1);
    if frames <= 1 {
        return 0;
    }
    if let Some(durations) = frame_durations {
        let expected_count = durations.len().min(frames);
        let timing = timing
            .filter(|timing| timing.duration_count == expected_count)
            .unwrap_or_else(|| SpriteFrameTiming::new(frames, durations));
        let clock = match rate {
            AnimationRate::FramesPerSecond(_) => time,
            AnimationRate::FramesPerBeat(_) => beat,
        };
        if let Some(total) = timing.total
            && let Some(idx) =
                duration_frame_index_with_timing(durations, frames, timing, clock.rem_euclid(total))
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
    if frame >= 0 {
        frame as usize % frames
    } else {
        frame.rem_euclid(frames as isize) as usize
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn sprite_frame_index_legacy(
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

#[must_use]
pub fn sprite_frame_index_from_phase(
    frame_count: usize,
    frame_durations: Option<&[f32]>,
    phase: f32,
) -> usize {
    sprite_frame_index_from_phase_with_timing(frame_count, frame_durations, None, phase)
}

#[must_use]
pub fn sprite_frame_index_from_phase_with_timing(
    frame_count: usize,
    frame_durations: Option<&[f32]>,
    timing: Option<SpriteFrameTiming>,
    phase: f32,
) -> usize {
    let frames = frame_count.max(1);
    if frames <= 1 {
        return 0;
    }
    let p = if (0.0..1.0).contains(&phase) {
        phase
    } else {
        phase.rem_euclid(1.0)
    };
    if let Some(durations) = frame_durations {
        let expected_count = durations.len().min(frames);
        let timing = timing
            .filter(|timing| timing.duration_count == expected_count)
            .unwrap_or_else(|| SpriteFrameTiming::new(frames, durations));
        if let Some(total) = timing.total
            && let Some(idx) =
                duration_frame_index_with_timing(durations, frames, timing, p * total)
        {
            return idx;
        }
    }
    ((p * frames as f32).floor() as usize).min(frames - 1)
}

#[cfg(any(test, feature = "bench-support"))]
fn sprite_frame_index_from_phase_legacy(
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

#[must_use]
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

/// Computes atlas coordinates with a texture scale cached by the owning
/// sprite source, avoiding repeated division in the render hot path.
#[must_use]
pub fn sprite_atlas_uv_scaled(
    texel_scale: [f32; 2],
    def: &SpriteDefinition,
    inset_texels: bool,
) -> [f32; 4] {
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

    [
        u0 * texel_scale[0],
        v0 * texel_scale[1],
        u1 * texel_scale[0],
        v1 * texel_scale[1],
    ]
}

/// Allocation-free cache for the two static atlas UV variants used by sprite
/// and model draws. Rebuild it when the texture scale, source, or size changes.
#[derive(Debug, Clone, Copy)]
pub struct SpriteAtlasUvCache {
    uv: [[f32; 4]; 2],
}

/// Cached normalized geometry for an animated sprite atlas.
#[derive(Debug, Clone, Copy)]
pub struct SpriteAnimatedUvCache {
    frame_count: usize,
    cols: usize,
    available: usize,
    frame_mask: usize,
    available_mask: usize,
    col_mask: usize,
    col_shift: u32,
    power_of_two_flags: u8,
    start: [[f32; 2]; 2],
    step: [f32; 2],
    extent: [[f32; 2]; 2],
}

impl SpriteAnimatedUvCache {
    #[must_use]
    pub fn new(
        texel_scale: [f32; 2],
        def: &SpriteDefinition,
        frame_size: [i32; 2],
        grid: [usize; 2],
        frame_count: usize,
        indexed: bool,
    ) -> Self {
        let origin = if indexed { [0, 0] } else { def.src };
        let inset = [
            if frame_size[0] > 0 { 0.5 } else { 0.0 },
            if frame_size[1] > 0 { 0.5 } else { 0.0 },
        ];
        let start = [
            [
                origin[0] as f32 * texel_scale[0],
                origin[1] as f32 * texel_scale[1],
            ],
            [
                (origin[0] as f32 + inset[0]) * texel_scale[0],
                (origin[1] as f32 + inset[1]) * texel_scale[1],
            ],
        ];
        let step = [
            frame_size[0] as f32 * texel_scale[0],
            frame_size[1] as f32 * texel_scale[1],
        ];
        let extent = [
            step,
            [
                (frame_size[0] as f32 - inset[0] * 2.0) * texel_scale[0],
                (frame_size[1] as f32 - inset[1] * 2.0) * texel_scale[1],
            ],
        ];
        const FRAME_COUNT_POWER_OF_TWO: u8 = 1 << 0;
        const AVAILABLE_POWER_OF_TWO: u8 = 1 << 1;
        const COLUMN_COUNT_POWER_OF_TWO: u8 = 1 << 2;

        let frame_count = frame_count.max(1);
        let cols = grid[0].max(1);
        let available = cols.saturating_mul(grid[1].max(1)).max(1);
        let power_of_two_flags = (u8::from(frame_count.is_power_of_two())
            * FRAME_COUNT_POWER_OF_TWO)
            | (u8::from(available.is_power_of_two()) * AVAILABLE_POWER_OF_TWO)
            | (u8::from(cols.is_power_of_two()) * COLUMN_COUNT_POWER_OF_TWO);
        Self {
            frame_count,
            cols,
            available,
            frame_mask: frame_count.saturating_sub(1),
            available_mask: available.saturating_sub(1),
            col_mask: cols.saturating_sub(1),
            col_shift: cols.trailing_zeros(),
            power_of_two_flags,
            start,
            step,
            extent,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn get(
        &self,
        frame_indices: Option<&[usize]>,
        frame_index: usize,
        inset_texels: bool,
    ) -> [f32; 4] {
        const FRAME_COUNT_POWER_OF_TWO: u8 = 1 << 0;
        const AVAILABLE_POWER_OF_TWO: u8 = 1 << 1;
        const COLUMN_COUNT_POWER_OF_TWO: u8 = 1 << 2;

        let idx = if self.power_of_two_flags & FRAME_COUNT_POWER_OF_TWO != 0 {
            frame_index & self.frame_mask
        } else {
            frame_index % self.frame_count
        };
        let source_idx =
            if let Some(source_idx) = frame_indices.and_then(|indices| indices.get(idx).copied()) {
                if self.power_of_two_flags & AVAILABLE_POWER_OF_TWO != 0 {
                    source_idx & self.available_mask
                } else {
                    source_idx % self.available
                }
            } else {
                idx
            };
        let (row, col) = if self.power_of_two_flags & COLUMN_COUNT_POWER_OF_TWO != 0 {
            (source_idx >> self.col_shift, source_idx & self.col_mask)
        } else {
            (source_idx / self.cols, source_idx % self.cols)
        };
        let variant = usize::from(inset_texels);
        let u0 = col as f32 * self.step[0] + self.start[variant][0];
        let v0 = row as f32 * self.step[1] + self.start[variant][1];
        [
            u0,
            v0,
            u0 + self.extent[variant][0],
            v0 + self.extent[variant][1],
        ]
    }
}

impl SpriteAtlasUvCache {
    #[must_use]
    pub fn new(texel_scale: [f32; 2], def: &SpriteDefinition) -> Self {
        Self {
            uv: [
                sprite_atlas_uv_scaled(texel_scale, def, false),
                sprite_atlas_uv_scaled(texel_scale, def, true),
            ],
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn get(&self, inset_texels: bool) -> [f32; 4] {
        self.uv[inset_texels as usize]
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn sprite_atlas_uv_legacy(
    tex_dims: [u32; 2],
    def: &SpriteDefinition,
    inset_texels: bool,
) -> [f32; 4] {
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

#[must_use]
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

#[must_use]
pub fn sprite_animated_uv_scaled(
    texel_scale: [f32; 2],
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
    sprite_atlas_uv_scaled(texel_scale, &frame_def, inset_texels)
}

#[cfg(test)]
fn sprite_animated_uv_legacy(
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
    sprite_atlas_uv_legacy(tex_dims, &frame_def, inset_texels)
}

#[must_use]
pub fn sprite_uv_scroll_clock(elapsed: f32, cycle_seconds: Option<f32>) -> f32 {
    cycle_seconds
        .filter(|total| *total > f32::EPSILON && total.is_finite())
        .map_or(elapsed, |total| elapsed.rem_euclid(total) / total)
}

#[must_use]
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

    let u_active = uv_velocity[0] != 0.0
        || uv_offset[0] != 0.0
        || (uv[0] == 0.0 && uv[0].is_sign_negative())
        || (uv[2] == 0.0 && uv[2].is_sign_negative());
    let v_active = uv_velocity[1] != 0.0
        || uv_offset[1] != 0.0
        || (uv[1] == 0.0 && uv[1].is_sign_negative())
        || (uv[3] == 0.0 && uv[3].is_sign_negative());
    if !elapsed.is_finite() {
        return sprite_scrolled_uv_legacy(uv, uv_velocity, uv_offset, elapsed, model_cycle_seconds);
    }
    if let Some(cycle_seconds) = model_cycle_seconds {
        let clock = sprite_uv_scroll_clock(elapsed, Some(cycle_seconds));
        if u_active {
            let shift = uv_velocity[0].mul_add(clock, uv_offset[0]);
            uv[0] += shift;
            uv[2] += shift;
        }
        if v_active {
            let shift = uv_velocity[1].mul_add(clock, uv_offset[1]);
            uv[1] += shift;
            uv[3] += shift;
        }
    } else {
        if u_active {
            let span = (1.0 - (uv[2] - uv[0]).abs()).max(0.0);
            let shift = uv_velocity[0].mul_add(elapsed, uv_offset[0]);
            let shift = if span > f32::EPSILON {
                shift.rem_euclid(span)
            } else {
                0.0
            };
            uv[0] += shift;
            uv[2] += shift;
        }
        if v_active {
            let span = (1.0 - (uv[3] - uv[1]).abs()).max(0.0);
            let shift = uv_velocity[1].mul_add(elapsed, uv_offset[1]);
            let shift = if span > f32::EPSILON {
                shift.rem_euclid(span)
            } else {
                0.0
            };
            uv[1] += shift;
            uv[3] += shift;
        }
    }
    uv
}

fn sprite_scrolled_uv_legacy(
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
        let shift_u = uv_velocity[0].mul_add(clock, uv_offset[0]);
        let shift_v = uv_velocity[1].mul_add(clock, uv_offset[1]);
        uv[0] += shift_u;
        uv[2] += shift_u;
        uv[1] += shift_v;
        uv[3] += shift_v;
    } else {
        let shift_u = uv_velocity[0].mul_add(elapsed, uv_offset[0]);
        let shift_v = uv_velocity[1].mul_add(elapsed, uv_offset[1]);
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod sprite_math_bench_support {
    use std::hint::black_box;

    use super::*;

    #[inline(always)]
    fn frame_checksum(frame: usize, checksum: u64) -> u64 {
        checksum.wrapping_add(frame as u64).rotate_left(7)
    }

    #[inline(always)]
    fn uv_checksum(uv: [f32; 4], checksum: u64) -> u64 {
        uv.into_iter().fold(checksum, |checksum, value| {
            checksum
                .wrapping_add(u64::from(value.to_bits()))
                .rotate_left(7)
        })
    }

    #[inline(always)]
    fn normalized_uv_checksum(uv: [f32; 4], checksum: u64) -> u64 {
        uv.into_iter().fold(checksum, |checksum, value| {
            checksum
                .wrapping_add((value * 65_536.0).round() as i64 as u64)
                .rotate_left(7)
        })
    }

    fn uniform_frame_index(evaluations: usize, legacy: bool) -> u64 {
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let time = black_box((index & 65_535) as f32 * 0.003_906_25);
            let frame = if legacy {
                sprite_frame_index_legacy(17, AnimationRate::FramesPerSecond(30.0), None, time, 0.0)
            } else {
                sprite_frame_index(17, AnimationRate::FramesPerSecond(30.0), None, time, 0.0)
            };
            checksum = frame_checksum(frame, checksum);
        }
        checksum
    }

    #[must_use]
    pub fn uniform_frame_index_old(evaluations: usize) -> u64 {
        uniform_frame_index(evaluations, true)
    }

    #[must_use]
    pub fn uniform_frame_index_new(evaluations: usize) -> u64 {
        uniform_frame_index(evaluations, false)
    }

    fn atlas_uv(evaluations: usize, legacy: bool) -> u64 {
        let def = black_box(SpriteDefinition {
            src: [64, 96],
            size: [48, 64],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        });
        let texel_scales = black_box([[1.0 / 257.0, 1.0 / 509.0], [1.0 / 258.0, 1.0 / 510.0]]);
        let mut total = [0.0_f32; 4];
        for index in 0..evaluations {
            let tex_dims = black_box([257 + (index & 1) as u32, 509 + (index & 1) as u32]);
            let uv = if legacy {
                sprite_atlas_uv_legacy(tex_dims, &def, true)
            } else {
                sprite_atlas_uv_scaled(texel_scales[index & 1], &def, true)
            };
            for (total, value) in total.iter_mut().zip(uv) {
                *total += value;
            }
        }
        let average = total.map(|value| value / evaluations as f32);
        normalized_uv_checksum(average, 0)
    }

    #[must_use]
    pub fn atlas_uv_old(evaluations: usize) -> u64 {
        atlas_uv(evaluations, true)
    }

    #[must_use]
    pub fn atlas_uv_new(evaluations: usize) -> u64 {
        atlas_uv(evaluations, false)
    }

    fn cached_atlas_uv(evaluations: usize, legacy: bool) -> u64 {
        let def = black_box(SpriteDefinition {
            src: [64, 96],
            size: [48, 64],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        });
        let texel_scale = black_box([1.0 / 512.0, 1.0 / 256.0]);
        let cache = black_box(SpriteAtlasUvCache::new(texel_scale, &def));
        let mut total = [0.0_f32; 4];
        for index in 0..evaluations {
            let inset = black_box(index & 1 != 0);
            let uv = if legacy {
                sprite_atlas_uv_scaled(texel_scale, &def, inset)
            } else {
                cache.get(inset)
            };
            for (total, value) in total.iter_mut().zip(uv) {
                *total += value;
            }
        }
        let average = total.map(|value| value / evaluations as f32);
        normalized_uv_checksum(average, 0)
    }

    #[must_use]
    pub fn cached_atlas_uv_old(evaluations: usize) -> u64 {
        cached_atlas_uv(evaluations, true)
    }

    #[must_use]
    pub fn cached_atlas_uv_new(evaluations: usize) -> u64 {
        cached_atlas_uv(evaluations, false)
    }

    fn cached_weighted_frame_index(evaluations: usize, legacy: bool) -> u64 {
        let durations = black_box([
            0.031_25, 0.062_5, 0.093_75, 0.125, 0.156_25, 0.187_5, 0.218_75, 0.25, 0.281_25,
            0.312_5, 0.343_75, 0.375, 0.406_25, 0.437_5, 0.468_75, 0.5,
        ]);
        let timing = black_box(SpriteFrameTiming::new(durations.len(), &durations));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let time = black_box((index & 65_535) as f32 * 0.003_906_25);
            let frame = if legacy {
                sprite_frame_index_legacy(
                    durations.len(),
                    AnimationRate::FramesPerSecond(30.0),
                    Some(&durations),
                    time,
                    0.0,
                )
            } else {
                sprite_frame_index_with_timing(
                    durations.len(),
                    AnimationRate::FramesPerSecond(30.0),
                    Some(&durations),
                    Some(timing),
                    time,
                    0.0,
                )
            };
            checksum = frame_checksum(frame, checksum);
        }
        checksum
    }

    #[must_use]
    pub fn cached_weighted_frame_index_old(evaluations: usize) -> u64 {
        cached_weighted_frame_index(evaluations, true)
    }

    #[must_use]
    pub fn cached_weighted_frame_index_new(evaluations: usize) -> u64 {
        cached_weighted_frame_index(evaluations, false)
    }

    fn uniform_weighted_frame_index(evaluations: usize, arithmetic: bool) -> u64 {
        let durations = black_box([0.125_f32; 32]);
        let timing = black_box(SpriteFrameTiming::new(durations.len(), &durations));
        let scan_timing = SpriteFrameTiming {
            uniform_duration: None,
            ..timing
        };
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let time = black_box((index & 65_535) as f32 * 0.003_906_25);
            let frame = sprite_frame_index_with_timing(
                durations.len(),
                AnimationRate::FramesPerSecond(8.0),
                Some(&durations),
                Some(if arithmetic { timing } else { scan_timing }),
                time,
                0.0,
            );
            checksum = frame_checksum(frame, checksum);
        }
        checksum
    }

    #[must_use]
    pub fn uniform_weighted_frame_index_old(evaluations: usize) -> u64 {
        uniform_weighted_frame_index(evaluations, false)
    }

    #[must_use]
    pub fn uniform_weighted_frame_index_new(evaluations: usize) -> u64 {
        uniform_weighted_frame_index(evaluations, true)
    }

    fn cached_animated_uv(evaluations: usize, cached: bool) -> u64 {
        let def = black_box(SpriteDefinition {
            src: [64, 32],
            size: [32, 32],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        });
        let frame_indices = black_box([7, 0, 5, 2, 3, 1, 6, 4]);
        let texel_scale = black_box([1.0 / 512.0, 1.0 / 256.0]);
        let frame_size = black_box([32, 32]);
        let grid = black_box([4, 2]);
        let frame_count = black_box(frame_indices.len());
        let cache = black_box(SpriteAnimatedUvCache::new(
            texel_scale,
            &def,
            frame_size,
            grid,
            frame_count,
            true,
        ));
        let mut total = [0.0_f32; 4];
        for index in 0..evaluations {
            let frame_index = black_box(index & 255);
            let inset = black_box(index & 1 != 0);
            let uv = if cached {
                cache.get(Some(&frame_indices), frame_index, inset)
            } else {
                sprite_animated_uv_scaled(
                    texel_scale,
                    &def,
                    frame_size,
                    grid,
                    frame_count,
                    Some(&frame_indices),
                    frame_index,
                    inset,
                )
            };
            for (total, value) in total.iter_mut().zip(uv) {
                *total += value;
            }
        }
        let average = total.map(|value| value / evaluations as f32);
        normalized_uv_checksum(average, 0)
    }

    #[must_use]
    pub fn cached_animated_uv_old(evaluations: usize) -> u64 {
        cached_animated_uv(evaluations, false)
    }

    #[must_use]
    pub fn cached_animated_uv_new(evaluations: usize) -> u64 {
        cached_animated_uv(evaluations, true)
    }

    #[must_use]
    pub fn normalized_phase_old(evaluations: usize) -> u64 {
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let phase = black_box((index & 4_095) as f32 / 4_096.0);
            let frame = sprite_frame_index_from_phase_legacy(8, None, phase);
            checksum = frame_checksum(frame, checksum);
        }
        checksum
    }

    #[must_use]
    pub fn normalized_phase_new(evaluations: usize) -> u64 {
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let phase = black_box((index & 4_095) as f32 / 4_096.0);
            let frame = sprite_frame_index_from_phase(8, None, phase);
            checksum = frame_checksum(frame, checksum);
        }
        checksum
    }

    fn scroll_old(evaluations: usize, velocity: [f32; 2], offset: [f32; 2]) -> u64 {
        let uv = black_box([0.0, 0.0, 0.25, 0.5]);
        let velocity = black_box(velocity);
        let offset = black_box(offset);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let elapsed = black_box(((index & 8_191) as f32 - 4_096.0) * 0.031_25);
            let uv = sprite_scrolled_uv_legacy(uv, velocity, offset, elapsed, None);
            checksum = uv_checksum(uv, checksum);
        }
        checksum
    }

    fn scroll_new(evaluations: usize, velocity: [f32; 2], offset: [f32; 2]) -> u64 {
        let uv = black_box([0.0, 0.0, 0.25, 0.5]);
        let velocity = black_box(velocity);
        let offset = black_box(offset);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let elapsed = black_box(((index & 8_191) as f32 - 4_096.0) * 0.031_25);
            let uv = sprite_scrolled_uv(uv, velocity, offset, elapsed, None);
            checksum = uv_checksum(uv, checksum);
        }
        checksum
    }

    #[must_use]
    pub fn horizontal_scroll_old(evaluations: usize) -> u64 {
        scroll_old(evaluations, [0.125, 0.0], [0.031_25, 0.0])
    }

    #[must_use]
    pub fn horizontal_scroll_new(evaluations: usize) -> u64 {
        scroll_new(evaluations, [0.125, 0.0], [0.031_25, 0.0])
    }

    #[must_use]
    pub fn vertical_scroll_old(evaluations: usize) -> u64 {
        scroll_old(evaluations, [0.0, -0.125], [0.0, 0.031_25])
    }

    #[must_use]
    pub fn vertical_scroll_new(evaluations: usize) -> u64 {
        scroll_new(evaluations, [0.0, -0.125], [0.0, 0.031_25])
    }
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

#[inline(always)]
#[must_use]
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
#[must_use]
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

#[inline(always)]
fn duration_frame_index_with_timing(
    durations: &[f32],
    frames: usize,
    timing: SpriteFrameTiming,
    position: f32,
) -> Option<usize> {
    if let Some(duration) = timing.uniform_duration
        && position.is_finite()
        && timing.duration_count > 0
    {
        return Some(
            ((position / duration).floor() as usize).min(timing.duration_count.saturating_sub(1)),
        );
    }
    duration_frame_index(durations, frames, position)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::draw::ModelVertex;
    use crate::script::{SpriteAnimationCommandPlan, SpriteStatePropertiesPlan};

    use super::{
        AnimationRate, SpriteAnimatedUvCache, SpriteAnimationPlan, SpriteAtlasUvCache,
        SpriteDefinition, SpriteFrameTiming, SpriteSourcePlan, SpriteStatePropertiesAnimation,
        all_frames_sprite_slot_plan, atlas_sprite_slot_plan, duration_frame_index,
        frame_duration_total, frame_sprite_slot_plan, generated_animation_sprite_slot_plan,
        itg_all_frames_sprite_slot_plan_from_path, itg_animation_sprite_slot_plan_from_path,
        itg_frame_sprite_slot_plan_from_path, itg_sprite_animation_slot_plan,
        itg_sprite_slot_plan_from_path, model_vertex_for_sprite, neg_rot_sin_cos,
        sprite_all_frames_animation_plan, sprite_animated_uv, sprite_animated_uv_legacy,
        sprite_animated_uv_scaled, sprite_animation_plan, sprite_atlas_uv, sprite_atlas_uv_legacy,
        sprite_atlas_uv_scaled, sprite_frame_index, sprite_frame_index_from_phase,
        sprite_frame_index_from_phase_legacy, sprite_frame_index_from_phase_with_timing,
        sprite_frame_index_legacy, sprite_frame_index_with_timing, sprite_scrolled_uv,
        sprite_scrolled_uv_legacy, sprite_sheet_frame, sprite_state_properties_animation,
    };

    fn assert_uv_close(old: [f32; 4], new: [f32; 4]) {
        for (old, new) in old.into_iter().zip(new) {
            let tolerance = old.abs().max(1.0) * f32::EPSILON * 2.0;
            assert!(
                (old - new).abs() <= tolerance,
                "legacy {old:?} and optimized {new:?} UV coordinates differ"
            );
        }
    }

    #[test]
    fn neg_rotation_uses_exact_cardinal_values() {
        assert_eq!(neg_rot_sin_cos(0), [0.0, 1.0]);
        assert_eq!(neg_rot_sin_cos(90), [-1.0, 0.0]);
        assert_eq!(neg_rot_sin_cos(180), [0.0, -1.0]);
        assert_eq!(neg_rot_sin_cos(270), [1.0, 0.0]);
        assert_eq!(neg_rot_sin_cos(-90), [1.0, 0.0]);
    }

    #[test]
    fn model_vertex_mirroring_preserves_depth_and_texture_scale() {
        let vertex = ModelVertex {
            pos: [3.0, -5.0, 7.0],
            uv: [0.2, 0.75],
            tex_matrix_scale: [2.0, 4.0],
        };
        let mirrored = model_vertex_for_sprite(
            &SpriteDefinition {
                mirror_h: true,
                mirror_v: true,
                ..SpriteDefinition::default()
            },
            vertex,
        );

        assert_eq!(mirrored.pos, [-3.0, 5.0, 7.0]);
        assert_eq!(mirrored.uv, [0.8, 0.25]);
        assert_eq!(mirrored.tex_matrix_scale, [2.0, 4.0]);
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
    fn itg_animation_slot_plan_applies_state_properties() {
        let slot =
            frame_sprite_slot_plan("tap.png".to_string(), (256, 128), (4, 2), 5, (64, 64), true);
        let plan = itg_sprite_animation_slot_plan(
            slot,
            SpriteAnimationCommandPlan::StateProperties(SpriteStatePropertiesPlan {
                frame_count: 3,
                frame_delays: vec![0.25, 0.5],
            }),
            true,
            |_| (4, 2),
            |_, _, _| (64, 64),
        )
        .expect("state properties should animate a multi-frame sheet");

        assert_eq!(plan.def.src, [64, 64]);
        assert_eq!(plan.def.size, [64, 64]);
        assert_eq!(
            plan.source,
            SpriteSourcePlan::Animated {
                texture_key: "tap.png".to_string(),
                tex_dims: (256, 128),
                frame_size: [64, 64],
                grid: (4, 2),
                frame_count: 3,
                frame_indices: None,
                rate: AnimationRate::FramesPerBeat(4.0),
                frame_durations: Some(vec![0.25, 0.5, 0.25]),
            }
        );
    }

    #[test]
    fn itg_animation_slot_plan_applies_all_state_delays_to_animated_sources() {
        let slot = all_frames_sprite_slot_plan(
            "tap.png".to_string(),
            (128, 64),
            (2, 1),
            Some(0.25),
            false,
            (64, 64),
            true,
        )
        .expect("animated slot");
        let plan = itg_sprite_animation_slot_plan(
            slot,
            SpriteAnimationCommandPlan::AllStateDelays(0.5),
            false,
            |_| (2, 1),
            |_, _, _| (64, 64),
        )
        .expect("all state delays should rewrite animated source");

        assert_eq!(
            plan.source,
            SpriteSourcePlan::Animated {
                texture_key: "tap.png".to_string(),
                tex_dims: (128, 64),
                frame_size: [64, 64],
                grid: (2, 1),
                frame_count: 2,
                frame_indices: None,
                rate: AnimationRate::FramesPerSecond(2.0),
                frame_durations: Some(vec![0.5, 0.5]),
            }
        );
    }

    #[test]
    fn itg_path_slot_plan_uses_texture_metadata_callbacks() {
        let plan = itg_sprite_slot_plan_from_path(
            Path::new("Tap Note.png"),
            |_| Some("noteskin/tap.png".to_string()),
            |_| Some((128, 64)),
            |_, _, _| (64, 32),
        )
        .expect("plan");

        assert_eq!(plan.def.size, [128, 64]);
        assert_eq!(plan.source_size, [64, 32]);
        assert_eq!(
            plan.source,
            SpriteSourcePlan::Atlas {
                texture_key: "noteskin/tap.png".to_string(),
                tex_dims: (128, 64),
            }
        );
    }

    #[test]
    fn itg_path_frame_slot_plan_uses_sheet_metadata_callbacks() {
        let plan = itg_frame_sprite_slot_plan_from_path(
            Path::new("Tap Note.png"),
            5,
            |_| Some("noteskin/tap.png".to_string()),
            |_| Some((128, 64)),
            |_| (4, 2),
            |_, _, _| (32, 32),
        )
        .expect("plan");

        assert_eq!(plan.def.src, [32, 32]);
        assert_eq!(plan.def.size, [32, 32]);
        assert_eq!(plan.source_size, [32, 32]);
    }

    #[test]
    fn itg_path_animation_slot_plan_falls_back_to_frame_slot() {
        let plan = itg_animation_sprite_slot_plan_from_path(
            Path::new("Tap Note.png"),
            1,
            1,
            None,
            None,
            false,
            |_| Some("noteskin/tap.png".to_string()),
            |_| Some((128, 64)),
            |_| (4, 2),
            |_, _, _| (32, 32),
        )
        .expect("plan");

        assert_eq!(plan.def.src, [32, 0]);
        assert_eq!(
            plan.source,
            SpriteSourcePlan::Atlas {
                texture_key: "noteskin/tap.png".to_string(),
                tex_dims: (128, 64),
            }
        );
    }

    #[test]
    fn itg_path_all_frames_slot_plan_falls_back_to_atlas_slot() {
        let plan = itg_all_frames_sprite_slot_plan_from_path(
            Path::new("Tap Note.png"),
            Some(0.25),
            false,
            |_| Some("noteskin/tap.png".to_string()),
            |_| Some((64, 64)),
            |_| (1, 1),
            |_, _, _| (64, 64),
        )
        .expect("plan");

        assert_eq!(plan.def.size, [64, 64]);
        assert_eq!(
            plan.source,
            SpriteSourcePlan::Atlas {
                texture_key: "noteskin/tap.png".to_string(),
                tex_dims: (64, 64),
            }
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
    fn single_remainder_frame_wrapping_matches_legacy_selection() {
        let durations = [0.125, 0.375, 0.25, 0.25];
        for frame_count in [0, 1, 2, 4, 8, 17] {
            for rate in [
                AnimationRate::FramesPerSecond(30.0),
                AnimationRate::FramesPerBeat(4.0),
                AnimationRate::FramesPerSecond(0.0),
            ] {
                for frame_durations in [None, Some(durations.as_slice())] {
                    for tick in -8_192..=8_192 {
                        let time = tick as f32 / 256.0;
                        let beat = tick as f32 / 1_024.0;
                        assert_eq!(
                            sprite_frame_index(frame_count, rate, frame_durations, time, beat),
                            sprite_frame_index_legacy(
                                frame_count,
                                rate,
                                frame_durations,
                                time,
                                beat,
                            )
                        );
                    }
                    for clock in [
                        -f32::MAX,
                        -0.0,
                        0.0,
                        f32::MAX,
                        f32::NEG_INFINITY,
                        f32::INFINITY,
                        f32::NAN,
                    ] {
                        assert_eq!(
                            sprite_frame_index(frame_count, rate, frame_durations, clock, clock),
                            sprite_frame_index_legacy(
                                frame_count,
                                rate,
                                frame_durations,
                                clock,
                                clock,
                            )
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cached_weighted_timing_matches_legacy_frame_selection() {
        let nonuniform = [
            0.031_25, 0.062_5, 0.093_75, 0.125, 0.156_25, 0.187_5, 0.218_75, 0.25,
        ];
        let uniform = [0.125; 8];
        for durations in [nonuniform.as_slice(), uniform.as_slice()] {
            for frame_count in [2, 7, 8, 13] {
                let timing = SpriteFrameTiming::new(frame_count, durations);
                for rate in [
                    AnimationRate::FramesPerSecond(30.0),
                    AnimationRate::FramesPerBeat(4.0),
                ] {
                    for tick in -8_192..=8_192 {
                        let time = tick as f32 / 1_024.0;
                        let beat = tick as f32 / 4_096.0;
                        assert_eq!(
                            sprite_frame_index_with_timing(
                                frame_count,
                                rate,
                                Some(durations),
                                Some(timing),
                                time,
                                beat,
                            ),
                            sprite_frame_index_legacy(
                                frame_count,
                                rate,
                                Some(durations),
                                time,
                                beat,
                            )
                        );
                        let phase = tick as f32 / 4_096.0;
                        assert_eq!(
                            sprite_frame_index_from_phase_with_timing(
                                frame_count,
                                Some(durations),
                                Some(timing),
                                phase,
                            ),
                            sprite_frame_index_from_phase_legacy(
                                frame_count,
                                Some(durations),
                                phase,
                            )
                        );
                    }
                }
                for value in [
                    -f32::MAX,
                    -0.0,
                    0.0,
                    f32::MAX,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    assert_eq!(
                        sprite_frame_index_with_timing(
                            frame_count,
                            AnimationRate::FramesPerSecond(30.0),
                            Some(durations),
                            Some(timing),
                            value,
                            value,
                        ),
                        sprite_frame_index_legacy(
                            frame_count,
                            AnimationRate::FramesPerSecond(30.0),
                            Some(durations),
                            value,
                            value,
                        )
                    );
                }
            }
        }
    }

    #[test]
    fn reciprocal_atlas_uv_normalization_matches_legacy_coordinates() {
        let definitions = [
            SpriteDefinition::default(),
            SpriteDefinition {
                src: [17, 31],
                size: [47, 63],
                ..SpriteDefinition::default()
            },
            SpriteDefinition {
                src: [-19, -7],
                size: [-3, 0],
                ..SpriteDefinition::default()
            },
        ];
        for tex_dims in [[0, 0], [1, 1], [257, 509], [4_096, 2_047]] {
            for def in &definitions {
                for inset in [false, true] {
                    assert_uv_close(
                        sprite_atlas_uv_legacy(tex_dims, def, inset),
                        sprite_atlas_uv_scaled(
                            [
                                1.0 / tex_dims[0].max(1) as f32,
                                1.0 / tex_dims[1].max(1) as f32,
                            ],
                            def,
                            inset,
                        ),
                    );
                }
            }
        }
    }

    #[test]
    fn scaled_animated_uv_matches_legacy_addressing() {
        let def = SpriteDefinition {
            src: [64, 32],
            size: [32, 32],
            ..SpriteDefinition::default()
        };
        let indices = [7, 0, 5, 2, 19, 1, 6, 3];
        for grid in [[8, 1], [1, 8], [4, 2], [3, 3]] {
            for frame_indices in [None, Some(indices.as_slice())] {
                for frame_index in 0..32 {
                    for inset in [false, true] {
                        let old = sprite_animated_uv_legacy(
                            [512, 256],
                            &def,
                            [32, 32],
                            grid,
                            8,
                            frame_indices,
                            frame_index,
                            inset,
                        );
                        let texel_scale = [1.0 / 512.0, 1.0 / 256.0];
                        let new = sprite_animated_uv_scaled(
                            texel_scale,
                            &def,
                            [32, 32],
                            grid,
                            8,
                            frame_indices,
                            frame_index,
                            inset,
                        );
                        assert_uv_close(old, new);
                    }
                }
            }
        }
    }

    #[test]
    fn cached_animated_uv_matches_uncached_addressing() {
        let def = SpriteDefinition {
            src: [64, 32],
            size: [32, 32],
            ..SpriteDefinition::default()
        };
        let indices = [7, 0, 5, 2, 19, 1, 6, 3];
        let texel_scale = [1.0 / 512.0, 1.0 / 256.0];
        for grid in [[8, 1], [1, 8], [4, 2], [3, 3]] {
            for frame_indices in [None, Some(indices.as_slice())] {
                let cache = SpriteAnimatedUvCache::new(
                    texel_scale,
                    &def,
                    [32, 32],
                    grid,
                    8,
                    frame_indices.is_some(),
                );
                for frame_index in 0..32 {
                    for inset in [false, true] {
                        assert_uv_close(
                            sprite_animated_uv_scaled(
                                texel_scale,
                                &def,
                                [32, 32],
                                grid,
                                8,
                                frame_indices,
                                frame_index,
                                inset,
                            ),
                            cache.get(frame_indices, frame_index, inset),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn static_atlas_uv_cache_preserves_both_inset_variants() {
        let scale = [1.0 / 257.0, 1.0 / 509.0];
        let def = SpriteDefinition {
            src: [17, 31],
            size: [47, 63],
            ..SpriteDefinition::default()
        };
        let cache = SpriteAtlasUvCache::new(scale, &def);

        for inset in [false, true] {
            assert_eq!(
                cache.get(inset).map(f32::to_bits),
                sprite_atlas_uv_scaled(scale, &def, inset).map(f32::to_bits)
            );
        }
    }

    #[test]
    fn normalized_phase_fast_path_matches_legacy_frame_selection() {
        let durations = [0.125, 0.375, 0.25, 0.25];
        for frame_count in [0, 1, 2, 4, 8, 17] {
            for frame_durations in [None, Some(durations.as_slice())] {
                for tick in -8_192..=8_192 {
                    let phase = tick as f32 / 4_096.0;
                    assert_eq!(
                        sprite_frame_index_from_phase(frame_count, frame_durations, phase,),
                        sprite_frame_index_from_phase_legacy(frame_count, frame_durations, phase,),
                    );
                }
                for phase in [
                    -f32::MAX,
                    -0.0,
                    0.0,
                    1.0,
                    f32::MAX,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    assert_eq!(
                        sprite_frame_index_from_phase(frame_count, frame_durations, phase,),
                        sprite_frame_index_from_phase_legacy(frame_count, frame_durations, phase,),
                    );
                }
            }
        }
    }

    #[test]
    fn axis_specialized_uv_scrolling_matches_legacy_math() {
        let uvs = [
            [0.0, 0.0, 0.25, 0.5],
            [0.25, 0.5, 0.0, 0.0],
            [-0.0, -0.0, 1.0, 1.0],
            [f32::NEG_INFINITY, 0.0, f32::INFINITY, 1.0],
        ];
        let motions = [
            ([0.0, 0.0], [0.0, 0.0]),
            ([0.125, 0.0], [0.031_25, 0.0]),
            ([0.0, -0.125], [0.0, 0.031_25]),
            ([0.125, -0.125], [0.031_25, -0.031_25]),
        ];
        for uv in uvs {
            for (velocity, offset) in motions {
                for elapsed in [
                    -128.0,
                    -0.0,
                    0.0,
                    0.125,
                    128.0,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    for cycle in [None, Some(10.0), Some(0.0), Some(f32::NAN)] {
                        let old = sprite_scrolled_uv_legacy(uv, velocity, offset, elapsed, cycle);
                        let new = sprite_scrolled_uv(uv, velocity, offset, elapsed, cycle);
                        assert_eq!(old.map(f32::to_bits), new.map(f32::to_bits));
                    }
                }
            }
        }
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
