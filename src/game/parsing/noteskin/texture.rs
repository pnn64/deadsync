use super::{
    ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelTweenSegment, SpriteDefinition,
    SpriteSlot, SpriteSource,
};
use crate::assets;
use deadlib_platform::dirs;
use deadsync_noteskin::{
    SpriteAnimationPlan, sprite_all_frames_animation_plan, sprite_animation_plan,
    sprite_sheet_frame,
};
use image::image_dimensions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicU64};

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

pub(super) fn itg_slot_from_path(path: &Path) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    let source = Arc::new(SpriteSource::Atlas {
        texture_key: key.into(),
        tex_dims: dims,
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });
    Some(SpriteSlot {
        def: SpriteDefinition {
            src: [0, 0],
            size: [dims.0 as i32, dims.1 as i32],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        base_rot_sin_cos: [0.0, 1.0],
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        note_color_translate: true,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    })
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
    let frame = sprite_sheet_frame(
        [dims.0, dims.1],
        [grid_x.max(1) as usize, grid_y.max(1) as usize],
        frame,
    );
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    let source = Arc::new(SpriteSource::Atlas {
        texture_key: key.into(),
        tex_dims: dims,
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });
    Some(SpriteSlot {
        def: frame.def,
        base_rot_sin_cos: [0.0, 1.0],
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        note_color_translate: true,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    })
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
    let Some(plan) = sprite_animation_plan(
        [dims.0, dims.1],
        [grid_x.max(1) as usize, grid_y.max(1) as usize],
        frame0,
        frame_count,
        frame_indices,
        frame_delays,
        beat_based,
    ) else {
        return itg_slot_from_path_with_frame(path, frame0);
    };
    Some(slot_from_animation_plan(key, dims, plan))
}

fn slot_from_animation_plan(
    key: String,
    dims: (u32, u32),
    plan: SpriteAnimationPlan,
) -> SpriteSlot {
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    let source = Arc::new(SpriteSource::Animated {
        texture_key: key.into(),
        tex_dims: dims,
        frame_size: plan.frame_size,
        grid: (plan.grid[0], plan.grid[1]),
        frame_count: plan.frame_count,
        frame_indices: plan.frame_indices.map(Arc::<[usize]>::from),
        rate: plan.rate,
        frame_durations: plan.frame_durations.map(Arc::<[f32]>::from),
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });
    SpriteSlot {
        def: plan.def,
        base_rot_sin_cos: [0.0, 1.0],
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        note_color_translate: true,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    }
}

pub(super) fn itg_slot_from_path_all_frames(
    path: &Path,
    frame_delay: Option<f32>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let (cols, rows) = assets::sprite_sheet_dims(&key);
    let dims = texture_dimensions(&key)?;
    let Some(plan) = sprite_all_frames_animation_plan(
        [dims.0, dims.1],
        [cols.max(1) as usize, rows.max(1) as usize],
        frame_delay,
        beat_based,
    ) else {
        return itg_slot_from_path(path);
    };
    Some(slot_from_animation_plan(key, dims, plan))
}

pub(super) fn texture_dimensions(key: &str) -> Option<(u32, u32)> {
    if let Some(meta) = assets::texture_dims(key) {
        return Some((meta.w, meta.h));
    }
    let path = PathBuf::from("assets").join(key);
    image_dimensions(&path).ok()
}
