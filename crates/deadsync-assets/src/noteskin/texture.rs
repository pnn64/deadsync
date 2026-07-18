use crate as assets;
use deadlib_platform::dirs;
use deadlib_present::actors::{
    ActorResourceArena, SpriteSource as ActorSpriteSource, TextureKeyHandle,
};
use deadlib_render::{SamplerDesc, TexturedMeshVertex};
use deadsync_noteskin::ModelVertex;
use deadsync_noteskin::mine::{
    MINE_GRADIENT_SAMPLES, MineGradientSampleWarning, mine_fill_slots as crate_mine_fill_slots,
    mine_gradient_samples_from_slot, mine_gradient_slot_plan, mine_gradient_texture,
};
use deadsync_noteskin::model::ItgModelSlotPlan;
use deadsync_noteskin::script::apply_sprite_animation_command_plans;
#[cfg(test)]
use deadsync_noteskin::script::apply_sprite_animation_script_plans;
use deadsync_noteskin::{
    AnimationRate, ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelMesh, ModelTweenCursor,
    ModelTweenSegment, NoteskinSlot, SpriteDefinition, SpriteSlotPlan, SpriteSourcePlan,
    generated_animation_sprite_slot_plan, itg_all_frames_sprite_slot_plan_from_path,
    itg_animation_sprite_slot_plan_from_path, itg_frame_sprite_slot_plan_from_path,
    itg_sprite_animation_slot_plan, itg_sprite_slot_plan_from_path, model_draw_at,
    model_draw_at_cursor, model_glow_at, model_glow_with_draw, model_vertex_for_sprite,
    neg_rot_sin_cos, sprite_animated_uv, sprite_atlas_uv, sprite_frame_index,
    sprite_frame_index_from_phase, sprite_scrolled_uv, sprite_sheet_frame,
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
        cached_actor_texture: AtomicU64,
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
        cached_actor_texture: AtomicU64,
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

    #[inline(always)]
    pub fn actor_texture_source(&self, arena: &ActorResourceArena) -> ActorSpriteSource {
        let (texture_key, cached_handle, cached_generation, cached_actor_texture) = match self {
            Self::Atlas {
                texture_key,
                cached_handle,
                cached_generation,
                cached_actor_texture,
                ..
            }
            | Self::Animated {
                texture_key,
                cached_handle,
                cached_generation,
                cached_actor_texture,
                ..
            } => (
                texture_key,
                cached_handle,
                cached_generation,
                cached_actor_texture,
            ),
        };
        let generation = assets::texture_registry_generation();
        let mut handle = cached_handle.load(Ordering::Relaxed);
        if handle == deadlib_render::INVALID_TEXTURE_HANDLE
            || cached_generation.load(Ordering::Relaxed) != generation
        {
            handle = assets::texture_handle(texture_key.as_ref());
            cached_handle.store(handle, Ordering::Relaxed);
            cached_generation.store(generation, Ordering::Relaxed);
        }
        arena.texture_source(texture_key, handle, generation, cached_actor_texture)
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

static NEXT_SLOT_ID: AtomicU64 = AtomicU64::new(1);

#[inline(always)]
fn next_slot_id() -> u64 {
    let id = NEXT_SLOT_ID.fetch_add(1, Ordering::Relaxed);
    assert_ne!(id, 0, "noteskin slot ID space exhausted");
    id
}

#[derive(Debug)]
pub struct SpriteSlot {
    stable_id: u64,
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

impl Clone for SpriteSlot {
    fn clone(&self) -> Self {
        Self {
            stable_id: next_slot_id(),
            def: self.def.clone(),
            base_rot_sin_cos: self.base_rot_sin_cos,
            source_size: self.source_size,
            source: self.source.clone(),
            uv_velocity: self.uv_velocity,
            uv_offset: self.uv_offset,
            uv_cycle_seconds: self.uv_cycle_seconds,
            note_color_translate: self.note_color_translate,
            model: self.model.clone(),
            model_draw: self.model_draw,
            model_timeline: self.model_timeline.clone(),
            model_effect: self.model_effect,
            model_auto_rot_total_frames: self.model_auto_rot_total_frames,
            model_auto_rot_z_keys: self.model_auto_rot_z_keys.clone(),
        }
    }
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

    #[inline(always)]
    pub fn actor_texture_source(&self, arena: &ActorResourceArena) -> ActorSpriteSource {
        self.source.actor_texture_source(arena)
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
    pub fn model_draw_at_cursor(
        &self,
        time: f32,
        beat: f32,
        cursor: &mut ModelTweenCursor,
    ) -> ModelDrawState {
        model_draw_at_cursor(
            self.model_draw,
            self.model_timeline.as_ref(),
            self.model_effect,
            self.model_auto_rot_total_frames,
            self.model_auto_rot_z_keys.as_ref(),
            [time, beat],
            cursor,
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

impl NoteskinSlot for SpriteSlot {
    #[inline(always)]
    fn sprite_def(&self) -> &SpriteDefinition {
        &self.def
    }

    #[inline(always)]
    fn source_size(&self) -> [i32; 2] {
        self.source_size
    }

    #[inline(always)]
    fn texture_key_shared(&self) -> Arc<str> {
        SpriteSlot::texture_key_shared(self)
    }

    #[inline(always)]
    fn model(&self) -> Option<&ModelMesh> {
        self.model.as_deref()
    }

    #[inline(always)]
    fn base_rot_sin_cos(&self) -> [f32; 2] {
        SpriteSlot::base_rot_sin_cos(self)
    }

    #[inline(always)]
    fn frame_count(&self) -> usize {
        self.source.frame_count()
    }

    #[inline(always)]
    fn animation_is_beat_based(&self) -> bool {
        self.source.is_beat_based()
    }

    #[inline(always)]
    fn frame_index(&self, time: f32, beat: f32) -> usize {
        SpriteSlot::frame_index(self, time, beat)
    }

    #[inline(always)]
    fn frame_index_from_phase(&self, phase: f32) -> usize {
        SpriteSlot::frame_index_from_phase(self, phase)
    }

    #[inline(always)]
    fn uv_for_frame_at(&self, frame_index: usize, elapsed: f32) -> [f32; 4] {
        SpriteSlot::uv_for_frame_at(self, frame_index, elapsed)
    }

    #[inline(always)]
    fn model_draw_at(&self, time: f32, beat: f32) -> ModelDrawState {
        SpriteSlot::model_draw_at(self, time, beat)
    }

    #[inline(always)]
    fn stable_id(&self) -> u64 {
        self.stable_id
    }

    #[inline(always)]
    fn model_draw_at_cursor(
        &self,
        time: f32,
        beat: f32,
        cursor: &mut ModelTweenCursor,
    ) -> ModelDrawState {
        SpriteSlot::model_draw_at_cursor(self, time, beat, cursor)
    }

    #[inline(always)]
    fn model_glow_with_draw(
        &self,
        draw: ModelDrawState,
        time: f32,
        beat: f32,
        diffuse_alpha: f32,
    ) -> Option<[f32; 4]> {
        SpriteSlot::model_glow_with_draw(self, draw, time, beat, diffuse_alpha)
    }

    #[inline(always)]
    fn model_uv_params(&self, uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
        SpriteSlot::model_uv_params(self, uv_rect)
    }
}

#[inline(always)]
pub fn build_model_geometry(slot: &SpriteSlot) -> Arc<[TexturedMeshVertex]> {
    let model = slot
        .model
        .as_ref()
        .expect("model geometry requested for non-model noteskin slot");
    let mut vertices = Vec::with_capacity(model.vertices.len());
    for vertex in model.vertices.iter().copied() {
        let vertex = model_vertex_for_sprite(&slot.def, vertex);
        vertices.push(TexturedMeshVertex {
            pos: vertex.pos,
            uv: vertex.uv,
            color: [1.0; 4],
            tex_matrix_scale: vertex.tex_matrix_scale,
        });
    }
    Arc::from(vertices)
}

pub fn test_model_slot() -> SpriteSlot {
    SpriteSlot {
        stable_id: next_slot_id(),
        def: SpriteDefinition::default(),
        base_rot_sin_cos: [0.0, 1.0],
        source_size: [64, 64],
        source: Arc::new(SpriteSource::Atlas {
            texture_key: Arc::from("test"),
            tex_dims: (64, 64),
            cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
            cached_actor_texture: AtomicU64::new(0),
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

pub fn itg_texture_key(path: &Path) -> Option<String> {
    let dirs = dirs::app_dirs();
    let asset_relative_path = dirs
        .strip_asset_prefix(path)
        .map(Path::to_path_buf)
        .or_else(|| path.strip_prefix("assets").ok().map(Path::to_path_buf))
        .or_else(|| workspace_asset_relative_path(path));
    deadsync_noteskin::itg::texture_key_for_path(
        asset_relative_path.as_deref(),
        path,
        path.is_file(),
    )
}

fn workspace_asset_relative_path(path: &Path) -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir()
        && let Some(relative) = workspace_asset_relative_path_from_base(path, &cwd)
    {
        return Some(relative);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        if let Some(relative) = workspace_asset_relative_path_from_base(path, exe_dir) {
            return Some(relative);
        }
    }
    workspace_asset_relative_path_from_base(path, Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn workspace_asset_relative_path_from_base(path: &Path, base: &Path) -> Option<PathBuf> {
    for ancestor in base.ancestors() {
        for asset_root in [
            ancestor.join("assets"),
            ancestor.join("deadsync").join("assets"),
        ] {
            if let Ok(relative) = path.strip_prefix(asset_root) {
                return Some(relative.to_path_buf());
            }
        }
    }
    None
}

pub fn itg_register_texture_dims_for_path(path: &Path) {
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

pub fn itg_model_slot_from_texture_path(path: &Path) -> Option<SpriteSlot> {
    itg_register_texture_dims_for_path(path);
    itg_slot_from_path(path)
}

pub fn apply_model_slot_plan(slot: &mut SpriteSlot, plan: ItgModelSlotPlan) {
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

pub fn load_itg_model_slots_from_path(path: &Path) -> Result<Arc<[SpriteSlot]>, String> {
    let model_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        resolve_asset_path(path)
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
            cached_actor_texture: AtomicU64::new(0),
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
            cached_actor_texture: AtomicU64::new(0),
        }),
    }
}

fn slot_from_plan(plan: SpriteSlotPlan) -> SpriteSlot {
    SpriteSlot {
        stable_id: next_slot_id(),
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

fn source_plan_from_slot(slot: &SpriteSlot) -> SpriteSourcePlan {
    match slot.source.as_ref() {
        SpriteSource::Atlas {
            texture_key,
            tex_dims,
            ..
        } => SpriteSourcePlan::Atlas {
            texture_key: texture_key.to_string(),
            tex_dims: *tex_dims,
        },
        SpriteSource::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            rate,
            frame_durations,
            ..
        } => SpriteSourcePlan::Animated {
            texture_key: texture_key.to_string(),
            tex_dims: *tex_dims,
            frame_size: *frame_size,
            grid: *grid,
            frame_count: *frame_count,
            frame_indices: frame_indices.as_ref().map(|indices| indices.to_vec()),
            rate: *rate,
            frame_durations: frame_durations.as_ref().map(|durations| durations.to_vec()),
        },
    }
}

fn plan_from_slot(slot: &SpriteSlot) -> SpriteSlotPlan {
    SpriteSlotPlan {
        def: slot.def.clone(),
        source_size: slot.source_size,
        source: source_plan_from_slot(slot),
        note_color_translate: slot.note_color_translate,
    }
}

fn apply_slot_plan(slot: &mut SpriteSlot, plan: SpriteSlotPlan) {
    slot.def = plan.def;
    slot.source_size = plan.source_size;
    slot.source = source_from_plan(plan.source);
    slot.note_color_translate = plan.note_color_translate;
}

pub fn itg_slot_from_path(path: &Path) -> Option<SpriteSlot> {
    itg_sprite_slot_plan_from_path(
        path,
        itg_texture_key,
        texture_dimensions,
        assets::texture_source_frame_dims_from_real,
    )
    .map(slot_from_plan)
}

pub fn itg_apply_frame_override(slot: &mut SpriteSlot, frame: usize) {
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

pub fn itg_slot_from_path_with_frame(path: &Path, frame: usize) -> Option<SpriteSlot> {
    itg_frame_sprite_slot_plan_from_path(
        path,
        frame,
        itg_texture_key,
        texture_dimensions,
        assets::sprite_sheet_dims,
        assets::texture_source_frame_dims_from_real,
    )
    .map(slot_from_plan)
}

pub fn itg_slot_from_path_animated(
    path: &Path,
    frame0: usize,
    frame_count: usize,
    frame_indices: Option<&[usize]>,
    frame_delays: Option<&[f32]>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    itg_animation_sprite_slot_plan_from_path(
        path,
        frame0,
        frame_count,
        frame_indices,
        frame_delays,
        beat_based,
        itg_texture_key,
        texture_dimensions,
        assets::sprite_sheet_dims,
        assets::texture_source_frame_dims_from_real,
    )
    .map(slot_from_plan)
}

pub fn itg_slot_from_path_all_frames(
    path: &Path,
    frame_delay: Option<f32>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    itg_all_frames_sprite_slot_plan_from_path(
        path,
        frame_delay,
        beat_based,
        itg_texture_key,
        texture_dimensions,
        assets::sprite_sheet_dims,
        assets::texture_source_frame_dims_from_real,
    )
    .map(slot_from_plan)
}

pub fn texture_dimensions(key: &str) -> Option<(u32, u32)> {
    if let Some(meta) = assets::texture_dims(key) {
        return Some((meta.w, meta.h));
    }
    let path = resolve_asset_path(&PathBuf::from("assets").join(key));
    image_dimensions(&path).ok()
}

fn slot_is_beat_based(slot: &SpriteSlot) -> bool {
    slot.source.is_beat_based()
}

fn itg_apply_sprite_animation_plan(
    slot: &mut SpriteSlot,
    plan: deadsync_noteskin::script::SpriteAnimationCommandPlan,
    beat_based: bool,
) {
    if slot.model.is_some() {
        return;
    }
    if let Some(plan) = itg_sprite_animation_slot_plan(
        plan_from_slot(slot),
        plan,
        beat_based,
        assets::sprite_sheet_dims,
        assets::texture_source_frame_dims_from_real,
    ) {
        apply_slot_plan(slot, plan);
    }
}

pub fn itg_apply_state_properties_from_commands(
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

pub fn mine_fill_slots(mines: &[Option<SpriteSlot>]) -> Vec<Option<SpriteSlot>> {
    crate_mine_fill_slots(mines, |mine| {
        let colors = load_mine_gradient_colors(mine)?;
        Some(build_mine_gradient_slot(&colors))
    })
}

fn load_mine_gradient_colors(slot: &SpriteSlot) -> Option<Vec<[f32; 4]>> {
    let texture_key = slot.texture_key();
    let candidate = Path::new("assets").join(texture_key);
    let path = resolve_asset_path(&candidate);
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

fn resolve_asset_path(path: &Path) -> PathBuf {
    let resolved = dirs::app_dirs().resolve_asset_path(&path.to_string_lossy());
    if resolved.exists() {
        return resolved;
    }
    workspace_asset_path(path).unwrap_or(resolved)
}

fn workspace_asset_path(path: &Path) -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir()
        && let Some(candidate) = workspace_asset_path_from_base(path, &cwd)
    {
        return Some(candidate);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
        && let Some(candidate) = workspace_asset_path_from_base(path, exe_dir)
    {
        return Some(candidate);
    }
    workspace_asset_path_from_base(path, Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn workspace_asset_path_from_base(path: &Path, base: &Path) -> Option<PathBuf> {
    for ancestor in base.ancestors() {
        for candidate in [ancestor.join(path), ancestor.join("deadsync").join(path)] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
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

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn noteskin_actor_source_uses_arena_ownership() {
        let slot = test_model_slot();
        let arena = ActorResourceArena::new(1);
        arena.begin_hit_stats(true);

        let first = slot.actor_texture_source(&arena);
        let second = slot.actor_texture_source(&arena);

        assert!(matches!(
            first,
            ActorSpriteSource::ArenaTextureHandle { .. }
        ));
        assert!(matches!(
            second,
            ActorSpriteSource::ArenaTextureHandle { .. }
        ));
        let texture_key = match slot.source.as_ref() {
            SpriteSource::Atlas { texture_key, .. }
            | SpriteSource::Animated { texture_key, .. } => texture_key,
        };
        assert_eq!(Arc::strong_count(texture_key), 2);
        assert_eq!(arena.stats().texture_misses, 1);
        assert_eq!(arena.stats().texture_hits, 1);
    }

    #[test]
    fn noteskin_slot_contract_preserves_data_and_identity() {
        let slot = test_model_slot();
        let cloned_slot = slot.clone();
        let contract_def = <SpriteSlot as NoteskinSlot>::sprite_def(&slot);
        let contract_key = <SpriteSlot as NoteskinSlot>::texture_key_shared(&slot);
        let inherent_key = slot.texture_key_shared();
        let contract_model =
            <SpriteSlot as NoteskinSlot>::model(&slot).expect("test slot should retain its model");

        assert!(std::ptr::eq(contract_def, &slot.def));
        assert!(Arc::ptr_eq(&contract_key, &inherent_key));
        assert!(std::ptr::eq(
            contract_model,
            slot.model.as_deref().expect("test model")
        ));
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::stable_id(&slot),
            slot.stable_id
        );
        assert_ne!(
            <SpriteSlot as NoteskinSlot>::stable_id(&slot),
            <SpriteSlot as NoteskinSlot>::stable_id(&cloned_slot)
        );
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::source_size(&slot),
            slot.source_size
        );
        assert_eq!(<SpriteSlot as NoteskinSlot>::size(&slot), slot.size());
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::logical_size(&slot),
            slot.logical_size()
        );
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::base_rot_sin_cos(&slot),
            slot.base_rot_sin_cos()
        );
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::animation_is_beat_based(&slot),
            slot.source.is_beat_based()
        );
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::frame_index(&slot, 0.25, 1.5),
            slot.frame_index(0.25, 1.5)
        );
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::frame_index_from_phase(&slot, 0.75),
            slot.frame_index_from_phase(0.75)
        );
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::uv_for_frame_at(&slot, 0, 0.5),
            slot.uv_for_frame_at(0, 0.5)
        );

        let contract_draw = <SpriteSlot as NoteskinSlot>::model_draw_at(&slot, 0.25, 1.5);
        let inherent_draw = slot.model_draw_at(0.25, 1.5);
        assert_eq!(contract_draw.pos, inherent_draw.pos);
        assert_eq!(contract_draw.rot, inherent_draw.rot);
        assert_eq!(contract_draw.zoom, inherent_draw.zoom);
        assert_eq!(contract_draw.tint, inherent_draw.tint);
        assert_eq!(contract_draw.glow, inherent_draw.glow);
        assert_eq!(contract_draw.vert_align, inherent_draw.vert_align);
        assert_eq!(contract_draw.blend_add, inherent_draw.blend_add);
        assert_eq!(contract_draw.visible, inherent_draw.visible);
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::model_glow_with_draw(
                &slot,
                contract_draw,
                0.25,
                1.5,
                0.8,
            ),
            slot.model_glow_with_draw(inherent_draw, 0.25, 1.5, 0.8)
        );
        assert_eq!(
            <SpriteSlot as NoteskinSlot>::model_uv_params(&slot, [0.1, 0.2, 0.8, 0.9]),
            slot.model_uv_params([0.1, 0.2, 0.8, 0.9])
        );
    }

    #[test]
    fn noteskin_slot_contract_reports_animation_clock_and_frame_count() {
        let beat_slot = slot_from_plan(generated_animation_sprite_slot_plan(
            "beat-test".to_string(),
            (4, 1),
            [1, 1],
            4,
            AnimationRate::FramesPerBeat(1.0),
            false,
        ));
        assert!(<SpriteSlot as NoteskinSlot>::animation_is_beat_based(
            &beat_slot
        ));
        assert_eq!(<SpriteSlot as NoteskinSlot>::frame_count(&beat_slot), 4);

        let time_slot = slot_from_plan(generated_animation_sprite_slot_plan(
            "time-test".to_string(),
            (4, 1),
            [1, 1],
            4,
            AnimationRate::FramesPerSecond(30.0),
            false,
        ));
        assert!(!<SpriteSlot as NoteskinSlot>::animation_is_beat_based(
            &time_slot
        ));
    }
}
