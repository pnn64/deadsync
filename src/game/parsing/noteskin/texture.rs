use crate::assets;
use deadlib_platform::dirs;
use deadlib_present::actors::TextureKeyHandle;
use deadlib_render::{SamplerDesc, TexturedMeshVertex};
#[cfg(test)]
use deadsync_noteskin::ModelVertex;
use deadsync_noteskin::mine::{
    MINE_GRADIENT_SAMPLES, MineGradientSampleWarning, mine_fill_slots as crate_mine_fill_slots,
    mine_gradient_samples_from_slot, mine_gradient_slot_plan, mine_gradient_texture,
};
use deadsync_noteskin::model::ItgModelSlotPlan;
#[cfg(test)]
use deadsync_noteskin::script::apply_sprite_animation_script_plans;
use deadsync_noteskin::script::{SpriteAnimationCommandPlan, apply_sprite_animation_command_plans};
use deadsync_noteskin::{
    AnimationRate, ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelMesh, ModelTweenSegment,
    SpriteDefinition, SpriteSlotPlan, SpriteSourcePlan, all_frames_sprite_slot_plan,
    all_state_delays_source_plan, animation_sprite_slot_plan, atlas_sprite_slot_plan,
    frame_sprite_slot_plan, generated_animation_sprite_slot_plan, model_draw_at, model_glow_at,
    model_glow_with_draw, neg_rot_sin_cos, sprite_animated_uv, sprite_atlas_uv, sprite_frame_index,
    sprite_frame_index_from_phase, sprite_scrolled_uv, sprite_sheet_frame,
    sprite_state_properties_animation, state_properties_source_plan,
};
use image::image_dimensions;
use log::warn;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub enum SpriteSource {
    Atlas {
        texture_key: Arc<str>,
        tex_dims: (u32, u32),
        cached_handle: AtomicU64,
        cached_generation: AtomicU64,
    },
    Animated {
        texture_key: Arc<str>,
        tex_dims: (u32, u32),
        frame_size: [i32; 2],
        grid: (usize, usize),
        frame_count: usize,
        frame_indices: Option<Arc<[usize]>>,
        rate: AnimationRate,
        frame_durations: Option<Arc<[f32]>>,
        cached_handle: AtomicU64,
        cached_generation: AtomicU64,
    },
}

impl SpriteSource {
    pub fn texture_key(&self) -> &str {
        match self {
            Self::Atlas { texture_key, .. } => texture_key,
            Self::Animated { texture_key, .. } => texture_key,
        }
    }

    #[inline(always)]
    pub fn texture_key_shared(&self) -> Arc<str> {
        match self {
            Self::Atlas { texture_key, .. } => texture_key.clone(),
            Self::Animated { texture_key, .. } => texture_key.clone(),
        }
    }

    #[inline(always)]
    pub fn texture_key_handle(&self) -> TextureKeyHandle {
        let (texture_key, cached_handle, cached_generation) = match self {
            Self::Atlas {
                texture_key,
                cached_handle,
                cached_generation,
                ..
            }
            | Self::Animated {
                texture_key,
                cached_handle,
                cached_generation,
                ..
            } => (texture_key, cached_handle, cached_generation),
        };
        let generation = assets::texture_registry_generation();
        let handle = cached_handle.load(Ordering::Relaxed);
        if handle != deadlib_render::INVALID_TEXTURE_HANDLE
            && cached_generation.load(Ordering::Relaxed) == generation
        {
            return TextureKeyHandle {
                key: texture_key.clone(),
                handle,
                generation,
            };
        }

        let handle = assets::texture_handle(texture_key.as_ref());
        cached_handle.store(handle, Ordering::Relaxed);
        cached_generation.store(generation, Ordering::Relaxed);
        TextureKeyHandle {
            key: texture_key.clone(),
            handle,
            generation,
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            Self::Atlas { .. } => 1,
            Self::Animated { frame_count, .. } => (*frame_count).max(1),
        }
    }

    pub const fn frame_size(&self) -> Option<[i32; 2]> {
        match self {
            Self::Atlas { .. } => None,
            Self::Animated { frame_size, .. } => Some(*frame_size),
        }
    }

    pub const fn is_beat_based(&self) -> bool {
        matches!(
            self,
            Self::Animated {
                rate: AnimationRate::FramesPerBeat(_),
                ..
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct SpriteSlot {
    pub def: SpriteDefinition,
    pub(crate) base_rot_sin_cos: [f32; 2],
    pub source_size: [i32; 2],
    pub source: Arc<SpriteSource>,
    pub uv_velocity: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_cycle_seconds: Option<f32>,
    pub note_color_translate: bool,
    pub model: Option<Arc<ModelMesh>>,
    pub model_draw: ModelDrawState,
    pub model_timeline: Arc<[ModelTweenSegment]>,
    pub model_effect: ModelEffectState,
    pub model_auto_rot_total_frames: f32,
    pub model_auto_rot_z_keys: Arc<[ModelAutoRotKey]>,
}

impl SpriteSlot {
    #[inline(always)]
    pub fn set_rotation_deg(&mut self, rotation_deg: i32) {
        self.def.rotation_deg = rotation_deg;
        self.base_rot_sin_cos = neg_rot_sin_cos(rotation_deg);
    }

    #[inline(always)]
    pub const fn base_rot_sin_cos(&self) -> [f32; 2] {
        self.base_rot_sin_cos
    }

    pub fn texture_key(&self) -> &str {
        self.source.texture_key()
    }

    #[inline(always)]
    pub fn texture_key_shared(&self) -> Arc<str> {
        self.source.texture_key_shared()
    }

    #[inline(always)]
    pub fn texture_key_handle(&self) -> TextureKeyHandle {
        self.source.texture_key_handle()
    }

    pub const fn size(&self) -> [i32; 2] {
        self.def.size
    }

    #[inline(always)]
    pub fn logical_size(&self) -> [f32; 2] {
        [
            self.source_size[0].max(0) as f32,
            self.source_size[1].max(0) as f32,
        ]
    }

    pub fn frame_index(&self, time: f32, beat: f32) -> usize {
        match self.source.as_ref() {
            SpriteSource::Atlas { .. } => 0,
            SpriteSource::Animated {
                rate,
                frame_count,
                frame_durations,
                ..
            } => sprite_frame_index(*frame_count, *rate, frame_durations.as_deref(), time, beat),
        }
    }

    pub fn frame_index_from_phase(&self, phase: f32) -> usize {
        match self.source.as_ref() {
            SpriteSource::Atlas { .. } => 0,
            SpriteSource::Animated {
                frame_count,
                frame_durations,
                ..
            } => sprite_frame_index_from_phase(*frame_count, frame_durations.as_deref(), phase),
        }
    }

    pub fn model_draw_at(&self, time: f32, beat: f32) -> ModelDrawState {
        model_draw_at(
            self.model_draw,
            self.model_timeline.as_ref(),
            self.model_effect,
            self.model_auto_rot_total_frames,
            self.model_auto_rot_z_keys.as_ref(),
            time,
            beat,
        )
    }

    #[inline(always)]
    pub fn model_glow_with_draw(
        &self,
        draw: ModelDrawState,
        time: f32,
        beat: f32,
        diffuse_alpha: f32,
    ) -> Option<[f32; 4]> {
        model_glow_with_draw(draw, self.model_effect, time, beat, diffuse_alpha)
    }

    #[inline(always)]
    pub fn model_glow_at(&self, time: f32, beat: f32, diffuse_alpha: f32) -> Option<[f32; 4]> {
        model_glow_at(
            self.model_draw,
            self.model_timeline.as_ref(),
            self.model_effect,
            self.model_auto_rot_total_frames,
            self.model_auto_rot_z_keys.as_ref(),
            time,
            beat,
            diffuse_alpha,
        )
    }

    pub fn uv_for_frame_at(&self, frame_index: usize, elapsed: f32) -> [f32; 4] {
        let uv = match self.source.as_ref() {
            SpriteSource::Atlas { tex_dims, .. } => {
                sprite_atlas_uv([tex_dims.0, tex_dims.1], &self.def, self.model.is_none())
            }
            SpriteSource::Animated {
                tex_dims,
                frame_size,
                grid,
                frame_count,
                frame_indices,
                ..
            } => sprite_animated_uv(
                [tex_dims.0, tex_dims.1],
                &self.def,
                *frame_size,
                [grid.0, grid.1],
                *frame_count,
                frame_indices.as_deref(),
                frame_index,
                self.model.is_none(),
            ),
        };

        // ITG model textures can scroll via AnimatedTexture TexVelocity/TexOffset.
        // ITGmania applies TexVelocity over the animation cycle percentage, not
        // raw seconds (see AnimatedTexture::GetTextureTranslate), so keep model
        // UVs on that clock while preserving the full [0..1] span.
        sprite_scrolled_uv(
            uv,
            self.uv_velocity,
            self.uv_offset,
            elapsed,
            self.model
                .is_some()
                .then_some(self.uv_cycle_seconds)
                .flatten(),
        )
    }

    #[inline(always)]
    pub fn model_uv_params(&self, uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
        let atlas_tex_dims = match self.source.as_ref() {
            SpriteSource::Atlas { tex_dims, .. } => Some(*tex_dims),
            SpriteSource::Animated { .. } => None,
        };
        deadsync_noteskin::model_texture_uv_params(uv_rect, self.def.src, atlas_tex_dims)
    }
}

#[inline(always)]
pub(crate) fn build_model_geometry(slot: &SpriteSlot) -> Arc<[TexturedMeshVertex]> {
    let model = slot
        .model
        .as_ref()
        .expect("model geometry requested for non-model noteskin slot");
    let mut vertices = Vec::with_capacity(model.vertices.len());
    for v in model.vertices.iter() {
        let mut pos = v.pos;
        if slot.def.mirror_h {
            pos[0] = -pos[0];
        }
        if slot.def.mirror_v {
            pos[1] = -pos[1];
        }
        let u = if slot.def.mirror_h {
            1.0 - v.uv[0]
        } else {
            v.uv[0]
        };
        let v_tex = if slot.def.mirror_v {
            1.0 - v.uv[1]
        } else {
            v.uv[1]
        };
        vertices.push(TexturedMeshVertex {
            pos,
            uv: [u, v_tex],
            color: [1.0; 4],
            tex_matrix_scale: v.tex_matrix_scale,
        });
    }
    Arc::from(vertices)
}

#[cfg(test)]
pub(crate) fn test_model_slot() -> SpriteSlot {
    SpriteSlot {
        def: SpriteDefinition::default(),
        base_rot_sin_cos: [0.0, 1.0],
        source_size: [64, 64],
        source: Arc::new(SpriteSource::Atlas {
            texture_key: Arc::from("test"),
            tex_dims: (64, 64),
            cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        }),
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        note_color_translate: false,
        model: Some(Arc::new(ModelMesh {
            vertices: Arc::from([ModelVertex {
                pos: [0.0, 0.0, 0.0],
                uv: [0.0, 0.0],
                tex_matrix_scale: [1.0, 1.0],
            }]),
            bounds: [0.0; 6],
        })),
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::<[ModelTweenSegment]>::from([]),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::<[ModelAutoRotKey]>::from([]),
    }
}

pub(super) fn itg_texture_key(path: &Path) -> Option<String> {
    let asset_relative_path = dirs::app_dirs()
        .strip_asset_prefix(path)
        .or_else(|| path.strip_prefix("assets").ok())
        .map(Path::to_path_buf);
    deadsync_noteskin::itg::texture_key_for_path(
        asset_relative_path.as_deref(),
        path,
        path.is_file(),
    )
}

pub(super) fn itg_register_texture_dims_for_path(path: &Path) {
    let Some(key) = itg_texture_key(path) else {
        return;
    };
    if assets::texture_dims(&key).is_some() {
        return;
    }
    if let Ok((w, h)) = image_dimensions(path) {
        assets::register_texture_dims(&key, w, h);
    }
}

pub(super) fn itg_model_slot_from_texture_path(path: &Path) -> Option<SpriteSlot> {
    itg_register_texture_dims_for_path(path);
    itg_slot_from_path(path)
}

pub(super) fn apply_model_slot_plan(slot: &mut SpriteSlot, plan: ItgModelSlotPlan) {
    slot.model = plan.model;
    slot.model_draw = plan.model_draw;
    slot.model_timeline = plan.model_timeline;
    slot.model_effect = plan.model_effect;
    slot.model_auto_rot_total_frames = plan.model_auto_rot_total_frames;
    slot.model_auto_rot_z_keys = plan.model_auto_rot_z_keys;
    slot.note_color_translate = plan.note_color_translate;
    slot.uv_velocity = plan.uv_velocity;
    slot.uv_offset = plan.uv_offset;
    slot.uv_cycle_seconds = plan.uv_cycle_seconds;
}

pub(crate) fn load_itg_model_slots_from_path(path: &Path) -> Result<Arc<[SpriteSlot]>, String> {
    let model_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        dirs::app_dirs().resolve_asset_path(&path.to_string_lossy())
    };
    deadsync_noteskin::itg_load_model_slots_from_path(
        &model_path,
        itg_model_slot_from_texture_path,
        apply_model_slot_plan,
    )
    .map(Arc::from)
}

fn source_from_plan(plan: SpriteSourcePlan) -> Arc<SpriteSource> {
    match plan {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        } => Arc::new(SpriteSource::Atlas {
            texture_key: texture_key.into(),
            tex_dims,
            cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        }),
        SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            rate,
            frame_durations,
        } => Arc::new(SpriteSource::Animated {
            texture_key: texture_key.into(),
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices: frame_indices.map(Arc::<[usize]>::from),
            rate,
            frame_durations: frame_durations.map(Arc::<[f32]>::from),
            cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        }),
    }
}

fn slot_from_plan(plan: SpriteSlotPlan) -> SpriteSlot {
    SpriteSlot {
        def: plan.def,
        base_rot_sin_cos: [0.0, 1.0],
        source_size: plan.source_size,
        source: source_from_plan(plan.source),
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        note_color_translate: plan.note_color_translate,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    }
}

pub(super) fn itg_slot_from_path(path: &Path) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    Some(slot_from_plan(atlas_sprite_slot_plan(
        key,
        dims,
        source_frame,
        true,
    )))
}

pub(super) fn itg_apply_frame_override(slot: &mut SpriteSlot, frame: usize) {
    let key = slot.texture_key().to_string();
    let Some((tex_w, tex_h)) = texture_dimensions(&key) else {
        return;
    };
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&key);
    let plan = sprite_sheet_frame(
        [tex_w, tex_h],
        [grid_x.max(1) as usize, grid_y.max(1) as usize],
        frame,
    );
    slot.def.src = plan.def.src;
    slot.def.size = plan.def.size;
}

pub(super) fn itg_slot_from_path_with_frame(path: &Path, frame: usize) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&key);
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    Some(slot_from_plan(frame_sprite_slot_plan(
        key,
        dims,
        (grid_x as usize, grid_y as usize),
        frame,
        source_frame,
        true,
    )))
}

pub(super) fn itg_slot_from_path_animated(
    path: &Path,
    frame0: usize,
    frame_count: usize,
    frame_indices: Option<&[usize]>,
    frame_delays: Option<&[f32]>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&key);
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    let Some(plan) = animation_sprite_slot_plan(
        key,
        dims,
        (grid_x as usize, grid_y as usize),
        frame0,
        frame_count,
        frame_indices,
        frame_delays,
        beat_based,
        source_frame,
        true,
    ) else {
        return itg_slot_from_path_with_frame(path, frame0);
    };
    Some(slot_from_plan(plan))
}

pub(super) fn itg_slot_from_path_all_frames(
    path: &Path,
    frame_delay: Option<f32>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let (cols, rows) = assets::sprite_sheet_dims(&key);
    let dims = texture_dimensions(&key)?;
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    let Some(plan) = all_frames_sprite_slot_plan(
        key,
        dims,
        (cols as usize, rows as usize),
        frame_delay,
        beat_based,
        source_frame,
        true,
    ) else {
        return itg_slot_from_path(path);
    };
    Some(slot_from_plan(plan))
}

pub(super) fn texture_dimensions(key: &str) -> Option<(u32, u32)> {
    if let Some(meta) = assets::texture_dims(key) {
        return Some((meta.w, meta.h));
    }
    let path = PathBuf::from("assets").join(key);
    image_dimensions(&path).ok()
}

fn slot_is_beat_based(slot: &SpriteSlot) -> bool {
    matches!(
        slot.source.as_ref(),
        SpriteSource::Animated {
            rate: AnimationRate::FramesPerBeat(_),
            ..
        }
    )
}

fn itg_apply_slot_state_properties(
    slot: &mut SpriteSlot,
    frame_count: usize,
    frame_delays: &[f32],
    beat_based: bool,
) {
    if slot.model.is_some() {
        return;
    }
    let (texture_key, tex_dims) = match slot.source.as_ref() {
        SpriteSource::Atlas {
            texture_key,
            tex_dims,
            ..
        }
        | SpriteSource::Animated {
            texture_key,
            tex_dims,
            ..
        } => (texture_key.clone(), *tex_dims),
    };
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&texture_key);
    let Some(animation) = sprite_state_properties_animation(
        [tex_dims.0, tex_dims.1],
        [grid_x as usize, grid_y as usize],
        slot.def.src,
        frame_count,
        frame_delays,
        beat_based,
    ) else {
        return;
    };

    let start_src = animation.start_src;
    let frame_size = animation.frame_size;
    slot.source = source_from_plan(state_properties_source_plan(
        texture_key.clone().to_string(),
        tex_dims,
        (grid_x as usize, grid_y as usize),
        animation,
    ));
    slot.def.src = start_src;
    slot.def.size = frame_size;
    let source_frame =
        assets::texture_source_frame_dims_from_real(&texture_key, tex_dims.0, tex_dims.1);
    slot.source_size = [source_frame.0 as i32, source_frame.1 as i32];
}

fn itg_apply_slot_all_state_delays(slot: &mut SpriteSlot, delay: f32, beat_based: bool) {
    if slot.model.is_some() {
        return;
    }
    let delay = delay.max(1e-6);
    let SpriteSource::Animated {
        texture_key,
        tex_dims,
        frame_size,
        grid,
        frame_count,
        frame_indices,
        ..
    } = slot.source.as_ref()
    else {
        return;
    };
    slot.source = source_from_plan(all_state_delays_source_plan(
        texture_key.to_string(),
        *tex_dims,
        *frame_size,
        *grid,
        *frame_count,
        frame_indices.as_ref().map(|indices| indices.to_vec()),
        delay,
        beat_based,
    ));
}

fn itg_apply_sprite_animation_plan(
    slot: &mut SpriteSlot,
    plan: SpriteAnimationCommandPlan,
    beat_based: bool,
) {
    match plan {
        SpriteAnimationCommandPlan::StateProperties(plan) => {
            itg_apply_slot_state_properties(slot, plan.frame_count, &plan.frame_delays, beat_based);
        }
        SpriteAnimationCommandPlan::AllStateDelays(delay) => {
            itg_apply_slot_all_state_delays(slot, delay, beat_based);
        }
    }
}

pub(super) fn itg_apply_state_properties_from_commands(
    slot: &mut SpriteSlot,
    commands: &std::collections::HashMap<String, String>,
) {
    let beat_based = slot_is_beat_based(slot);
    apply_sprite_animation_command_plans(slot, commands, beat_based, |slot, plan, beat_based| {
        itg_apply_sprite_animation_plan(slot, plan, beat_based);
    });
}

#[cfg(test)]
pub(super) fn itg_apply_state_properties_from_script(
    slot: &mut SpriteSlot,
    script: &str,
    beat_based: bool,
) {
    apply_sprite_animation_script_plans(slot, script, beat_based, |slot, plan, beat_based| {
        itg_apply_sprite_animation_plan(slot, plan, beat_based);
    });
}

pub(super) fn mine_fill_slots(mines: &[Option<SpriteSlot>]) -> Vec<Option<SpriteSlot>> {
    crate_mine_fill_slots(mines, |mine| {
        let colors = load_mine_gradient_colors(mine)?;
        Some(build_mine_gradient_slot(&colors))
    })
}

fn load_mine_gradient_colors(slot: &SpriteSlot) -> Option<Vec<[f32; 4]>> {
    let texture_key = slot.texture_key();
    let candidate = Path::new("assets").join(texture_key);
    let path = dirs::app_dirs().resolve_asset_path(&candidate.to_string_lossy());
    let image = assets::open_image_fallback(&path).ok()?.to_rgba8();

    mine_gradient_samples_from_slot(
        &image,
        texture_key,
        slot.def.src,
        slot.def.size,
        slot.source.frame_size(),
        MINE_GRADIENT_SAMPLES,
        |warning| match warning {
            MineGradientSampleWarning::InvalidSlotSize => {
                warn!("Mine fill slot has invalid size for gradient sampling");
            }
            MineGradientSampleWarning::RegionOutsideTexture { texture_key, src } => {
                warn!(
                    "Mine fill region ({}, {}) is outside of texture {}",
                    src[0], src[1], texture_key
                );
            }
            MineGradientSampleWarning::ZeroSampleSize { texture_key } => {
                warn!("Mine fill region has zero sample size for texture {texture_key}");
            }
        },
    )
}

fn build_mine_gradient_slot(colors: &[[f32; 4]]) -> SpriteSlot {
    let plan = mine_gradient_slot_plan(colors);
    if assets::texture_dims(&plan.texture_key).is_none() {
        let texture = mine_gradient_texture(colors);
        assets::register_generated_texture(&plan.texture_key, texture, SamplerDesc::default());
    }

    slot_from_plan(generated_animation_sprite_slot_plan(
        plan.texture_key,
        plan.tex_dims,
        plan.frame_size,
        plan.frame_count,
        AnimationRate::FramesPerBeat(1.0),
        false,
    ))
}
