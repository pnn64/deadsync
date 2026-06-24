mod commands;
mod mine;
mod model_cache;
mod texture;

use self::commands::itg_apply_state_properties_from_commands;
#[cfg(test)]
use self::commands::itg_apply_state_properties_from_script;
use self::mine::mine_fill_slots;
pub use self::model_cache::ModelMeshCacheStats;
pub(crate) use self::model_cache::{ModelMeshCache, build_model_geometry};
use self::texture::{
    itg_apply_frame_override, itg_find_texture_with_prefix, itg_model_slot_from_texture_path,
    itg_slot_from_path, itg_slot_from_path_all_frames, itg_slot_from_path_animated,
    itg_slot_from_path_with_frame,
};
#[cfg(test)]
use self::texture::{itg_register_texture_dims_for_path, itg_texture_key};
use crate::assets;
use deadlib_platform::dirs;
use deadlib_present::actors::TextureKeyHandle;
use deadsync_noteskin::lua::{itg_extract_quoted_strings, itg_parse_lua_quoted};
use deadsync_noteskin::model::{
    itg_parse_milkshape_model, itg_parse_milkshape_model_auto_rot,
    itg_parse_milkshape_model_layers, itg_resolve_model_texture_path,
};
use deadsync_noteskin::parse_explosion_animation;
pub use deadsync_noteskin::{
    AnimationRate, ExplosionAnimation, ExplosionSegment, ExplosionState, ExplosionVisualState,
    GlowEffect, ModelAutoRotKey, ModelDrawState, ModelEffectClock, ModelEffectMode,
    ModelEffectState, ModelMesh, ModelTweenSegment, ModelVertex, NOTE_ANIM_PART_COUNT,
    NUM_QUANTIZATIONS, NoteAnimPart, NoteColorType, NoteDisplayMetrics, NotePartAnimation,
    NotePartTextureTranslate, Quantization, ReceptorGlowBehavior, ReceptorPulse,
    ReceptorReverseBehavior, ReceptorReverseState, ReceptorStepBehavior, ReceptorStepBehaviors,
    SpriteDefinition, Style, TweenType,
};
use deadsync_noteskin::{
    actor as noteskin_actor, compiled as noteskin_compiled, compiler as noteskin_compiler,
    duration_frame_index, frame_duration_total, itg as noteskin_itg, model_draw_at, model_glow_at,
    model_glow_with_draw, neg_rot_sin_cos, script as noteskin_script,
};
use log::warn;
use noteskin_script::{
    ScriptActorMod, ScriptEffectMod, itg_parse_command_effect, normalized_script_command,
    parse_script_actor_mod, parse_script_bool, parse_script_effect_mod,
    parse_script_effectclock_from_commands, parse_script_number, parse_script_vertalign,
    split_script_token,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

const ITG_ARG0_TOKEN: &str = "__ITG_ARG0__";

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
    base_rot_sin_cos: [f32; 2],
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

    #[inline(always)]
    fn uv_scroll_clock(&self, elapsed: f32) -> f32 {
        self.uv_cycle_seconds
            .filter(|total| *total > f32::EPSILON && total.is_finite())
            .map_or(elapsed, |total| elapsed.rem_euclid(total) / total)
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
        let frames = self.source.frame_count();
        if frames <= 1 {
            return 0;
        }
        match self.source.as_ref() {
            SpriteSource::Atlas { .. } => 0,
            SpriteSource::Animated {
                rate,
                frame_durations,
                ..
            } => {
                if let Some(durations) = frame_durations.as_ref() {
                    let clock = match rate {
                        AnimationRate::FramesPerSecond(_) => time,
                        AnimationRate::FramesPerBeat(_) => beat,
                    };
                    if let Some(total) = frame_duration_total(durations, frames)
                        && let Some(idx) =
                            duration_frame_index(durations, frames, clock.rem_euclid(total))
                    {
                        return idx;
                    }
                }
                let frame = match rate {
                    AnimationRate::FramesPerSecond(fps) if *fps > 0.0 => {
                        (time * fps).floor() as isize
                    }
                    AnimationRate::FramesPerBeat(frames_per_beat) if *frames_per_beat > 0.0 => {
                        (beat * frames_per_beat).floor() as isize
                    }
                    _ => return 0,
                };
                ((frame % frames as isize) + frames as isize) as usize % frames
            }
        }
    }

    pub fn frame_index_from_phase(&self, phase: f32) -> usize {
        let frames = self.source.frame_count();
        if frames <= 1 {
            return 0;
        }
        let p = phase.rem_euclid(1.0);
        match self.source.as_ref() {
            SpriteSource::Atlas { .. } => 0,
            SpriteSource::Animated {
                frame_durations, ..
            } => {
                if let Some(durations) = frame_durations.as_ref()
                    && let Some(total) = frame_duration_total(durations, frames)
                    && let Some(idx) = duration_frame_index(durations, frames, p * total)
                {
                    return idx;
                }
                ((p * frames as f32).floor() as usize).min(frames - 1)
            }
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
        let mut uv = match self.source.as_ref() {
            SpriteSource::Atlas { tex_dims, .. } => {
                let tw = tex_dims.0.max(1) as f32;
                let th = tex_dims.1.max(1) as f32;
                let src = self.def.src;
                let size = self.def.size;

                let mut u0 = src[0] as f32;
                let mut v0 = src[1] as f32;
                let mut u1 = (src[0] + size[0]) as f32;
                let mut v1 = (src[1] + size[1]) as f32;

                if self.model.is_none() {
                    if size[0] > 0 {
                        u0 += 0.5;
                        u1 -= 0.5;
                    }
                    if size[1] > 0 {
                        v0 += 0.5;
                        v1 -= 0.5;
                    }
                }

                [u0 / tw, v0 / th, u1 / tw, v1 / th]
            }
            SpriteSource::Animated {
                tex_dims,
                frame_size,
                grid,
                frame_count,
                frame_indices,
                ..
            } => {
                let frames = (*frame_count).max(1);
                let idx = if frames > 0 { frame_index % frames } else { 0 };
                let cols = grid.0.max(1);
                let available = cols.saturating_mul(grid.1.max(1)).max(1);
                let source_idx = frame_indices
                    .as_ref()
                    .and_then(|indices| indices.get(idx).copied())
                    .map_or(idx, |idx| idx % available);
                let row = source_idx / cols;
                let col = source_idx % cols;
                let (src_x, src_y) = if frame_indices.is_some() {
                    (col as i32 * frame_size[0], row as i32 * frame_size[1])
                } else {
                    (
                        self.def.src[0] + (col as i32 * frame_size[0]),
                        self.def.src[1] + (row as i32 * frame_size[1]),
                    )
                };
                let tw = tex_dims.0.max(1) as f32;
                let th = tex_dims.1.max(1) as f32;

                let mut u0 = src_x as f32;
                let mut v0 = src_y as f32;
                let mut u1 = (src_x + frame_size[0]) as f32;
                let mut v1 = (src_y + frame_size[1]) as f32;

                if self.model.is_none() {
                    if frame_size[0] > 0 {
                        u0 += 0.5;
                        u1 -= 0.5;
                    }
                    if frame_size[1] > 0 {
                        v0 += 0.5;
                        v1 -= 0.5;
                    }
                }

                [u0 / tw, v0 / th, u1 / tw, v1 / th]
            }
        };

        // ITG model textures can scroll via AnimatedTexture TexVelocity/TexOffset.
        // ITGmania applies TexVelocity over the animation cycle percentage, not
        // raw seconds (see AnimatedTexture::GetTextureTranslate), so keep model
        // UVs on that clock while preserving the full [0..1] span.
        if self.uv_velocity != [0.0, 0.0] || self.uv_offset != [0.0, 0.0] {
            let w = (uv[2] - uv[0]).abs();
            let h = (uv[3] - uv[1]).abs();
            if self.model.is_some() {
                let clock = self.uv_scroll_clock(elapsed);
                let shift_u = self.uv_offset[0] + self.uv_velocity[0] * clock;
                let shift_v = self.uv_offset[1] + self.uv_velocity[1] * clock;
                uv[0] += shift_u;
                uv[2] += shift_u;
                uv[1] += shift_v;
                uv[3] += shift_v;
            } else {
                let shift_u = self.uv_offset[0] + self.uv_velocity[0] * elapsed;
                let shift_v = self.uv_offset[1] + self.uv_velocity[1] * elapsed;
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
        }
        uv
    }
}

pub type TapExplosionLayer = deadsync_noteskin::TapExplosionLayer<SpriteSlot>;
pub type TapExplosion = deadsync_noteskin::TapExplosion<SpriteSlot>;
pub type HoldVisuals = deadsync_noteskin::HoldVisuals<SpriteSlot>;
pub type Noteskin = deadsync_noteskin::NoteskinRuntime<SpriteSlot>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ItgSkinCacheKey {
    num_cols: usize,
    num_players: usize,
    skin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ItgDataCacheKey {
    root: String,
    game: String,
    skin: String,
}

static ITG_SKIN_CACHE: OnceLock<Mutex<HashMap<ItgSkinCacheKey, Arc<Noteskin>>>> = OnceLock::new();
static ITG_DATA_CACHE: OnceLock<Mutex<HashMap<ItgDataCacheKey, Arc<noteskin_itg::NoteskinData>>>> =
    OnceLock::new();

#[inline(always)]
fn itg_skin_cache_key(style: &Style, skin: &str) -> ItgSkinCacheKey {
    let trimmed = skin.trim();
    let normalized = if trimmed.is_empty() {
        "default"
    } else {
        trimmed
    };
    ItgSkinCacheKey {
        num_cols: style.num_cols,
        num_players: style.num_players,
        skin: normalized.to_ascii_lowercase(),
    }
}

#[inline(always)]
fn itg_data_cache_key(root: &Path, game: &str, skin: &str) -> ItgDataCacheKey {
    let trimmed = skin.trim();
    let normalized_skin = if trimmed.is_empty() {
        "default"
    } else {
        trimmed
    };
    ItgDataCacheKey {
        root: root.to_string_lossy().to_ascii_lowercase(),
        game: game.trim().to_ascii_lowercase(),
        skin: normalized_skin.to_ascii_lowercase(),
    }
}

fn load_itg_data_cached(
    root: &Path,
    game: &str,
    skin: &str,
) -> Result<Arc<noteskin_itg::NoteskinData>, String> {
    let key = itg_data_cache_key(root, game, skin);
    let cache = ITG_DATA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }
    let loaded = Arc::new(noteskin_itg::load_noteskin_data(root, game, skin)?);
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = guard.entry(key).or_insert_with(|| loaded.clone());
    Ok(entry.clone())
}

pub fn clear_itg_runtime_caches() {
    if let Some(cache) = ITG_SKIN_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    if let Some(cache) = ITG_DATA_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    noteskin_itg::clear_lookup_caches();
}

fn song_lua_itg_data(skin: &str) -> Option<Arc<noteskin_itg::NoteskinData>> {
    let requested = skin.trim();
    let skin = if requested.is_empty() {
        "default"
    } else {
        requested
    };
    for root in &dirs::app_dirs().noteskin_roots() {
        if let Ok(data) = load_itg_data_cached(root, "dance", skin) {
            return Some(data);
        }
    }
    None
}

pub(crate) fn song_lua_noteskin_resolve_path(
    skin: &str,
    button: &str,
    element: &str,
) -> Option<PathBuf> {
    song_lua_itg_data(skin)?.resolve_path(button, element)
}

pub(crate) fn song_lua_noteskin_metric(skin: &str, element: &str, value: &str) -> Option<String> {
    song_lua_itg_data(skin)?
        .get_metric(element, value)
        .map(str::to_string)
}

pub(crate) fn song_lua_noteskin_metric_f(skin: &str, element: &str, value: &str) -> Option<f32> {
    parse_script_number(song_lua_noteskin_metric(skin, element, value)?.as_str())
}

pub(crate) fn song_lua_noteskin_metric_b(skin: &str, element: &str, value: &str) -> Option<bool> {
    Some(parse_script_bool(
        song_lua_noteskin_metric(skin, element, value)?.as_str(),
    ))
}

pub(crate) fn song_lua_noteskin_exists(skin: &str) -> bool {
    song_lua_itg_data(skin).is_some()
}

fn noteskin_cache_dir() -> PathBuf {
    dirs::app_dirs().noteskin_cache_dir()
}

fn ensure_itg_compiled(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<noteskin_compiler::CompileOutcome, String> {
    let cache_dir = noteskin_cache_dir();
    noteskin_compiler::ensure_compiled(&cache_dir, game, data)
}

#[allow(dead_code)]
fn load_itg_compiled(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Option<noteskin_compiled::CompiledNoteskinBundle> {
    let cache_dir = noteskin_cache_dir();
    noteskin_compiler::load_compiled(&cache_dir, game, data)
}

#[cfg(test)]
fn compiled_bundle_path(game: &str, skin: &str, source_hash: &str) -> PathBuf {
    let cache_dir = noteskin_cache_dir();
    noteskin_compiled::compiled_bundle_path(&cache_dir, game, skin, source_hash)
}

pub fn load_itg_skin_cached(style: &Style, skin: &str) -> Result<Arc<Noteskin>, String> {
    let key = itg_skin_cache_key(style, skin);
    let cache = ITG_SKIN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let loaded = Arc::new(load_itg_skin(style, skin)?);
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = guard.entry(key).or_insert_with(|| loaded.clone());
    Ok(entry.clone())
}

pub(crate) fn load_itg_model_slots_from_path(path: &Path) -> Result<Arc<[SpriteSlot]>, String> {
    let model_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        dirs::app_dirs().resolve_asset_path(&path.to_string_lossy())
    };
    if !model_path.is_file() {
        return Err(format!("model '{}' was not found", model_path.display()));
    }

    let Some(search_dir) = model_path.parent() else {
        return Err(format!(
            "model '{}' has no parent directory",
            model_path.display()
        ));
    };
    let data = noteskin_itg::NoteskinData {
        name: "shared-model".to_string(),
        metrics: noteskin_itg::IniData::default(),
        search_dirs: vec![search_dir.to_path_buf()],
    };
    let model_auto_rot = itg_parse_milkshape_model_auto_rot(&model_path);
    let mut slots = Vec::new();

    if let Some(model_layers) = itg_parse_milkshape_model_layers(&data, &model_path) {
        for layer in model_layers {
            let Some(mut slot) = itg_model_slot_from_texture_path(&layer.texture.texture_path)
            else {
                continue;
            };
            slot.model = Some(layer.mesh);
            if let Some(auto_rot) = model_auto_rot.as_ref() {
                slot.model_auto_rot_total_frames = auto_rot.total_frames;
                slot.model_auto_rot_z_keys = Arc::clone(&auto_rot.z_keys);
            }
            slot.note_color_translate = !layer.flags.nomove;
            slot.uv_velocity = if layer.flags.nomove {
                [0.0, 0.0]
            } else {
                layer.texture.tex.uv_velocity
            };
            slot.uv_offset = layer.texture.tex.uv_offset;
            slot.uv_cycle_seconds = layer.texture.tex.uv_cycle_seconds;
            slots.push(slot);
        }
    }

    if slots.is_empty() {
        let Some(model_texture) = itg_resolve_model_texture_path(&data, &model_path) else {
            return Err(format!(
                "model '{}' did not resolve a texture",
                model_path.display()
            ));
        };
        let Some(mut slot) = itg_model_slot_from_texture_path(&model_texture.texture_path) else {
            return Err(format!(
                "model texture '{}' did not load",
                model_texture.texture_path.display()
            ));
        };
        slot.model = itg_parse_milkshape_model(&data, &model_path);
        if slot.model.is_none() {
            return Err(format!(
                "model '{}' did not produce any geometry",
                model_path.display()
            ));
        }
        if let Some(auto_rot) = model_auto_rot.as_ref() {
            slot.model_auto_rot_total_frames = auto_rot.total_frames;
            slot.model_auto_rot_z_keys = Arc::clone(&auto_rot.z_keys);
        }
        slot.uv_velocity = model_texture.tex.uv_velocity;
        slot.uv_offset = model_texture.tex.uv_offset;
        slot.uv_cycle_seconds = model_texture.tex.uv_cycle_seconds;
        slots.push(slot);
    }

    Ok(Arc::from(slots))
}

pub fn prewarm_itg_preview_cache() {
    let _ = compile_all_itg_caches_with_progress(|_, _, _, _| {});
    let skins = discover_itg_skins("dance");
    let styles = [
        Style {
            num_cols: 4,
            num_players: 1,
        },
        Style {
            num_cols: 8,
            num_players: 1,
        },
    ];

    for style in styles {
        for skin in &skins {
            if let Err(err) = load_itg_skin_cached(&style, skin) {
                warn!(
                    "noteskin prewarm failed for '{}' ({} columns): {}",
                    skin, style.num_cols, err
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompileAllItgSummary {
    pub total: usize,
    pub built: usize,
    pub reused: usize,
    pub failed: usize,
}

pub fn compile_all_itg_caches_with_progress<F>(mut on_progress: F) -> CompileAllItgSummary
where
    F: FnMut(usize, usize, &str, &str),
{
    clear_itg_runtime_caches();
    let roots = dirs::app_dirs().noteskin_roots();
    let game = "dance";
    let skins = discover_itg_skins(game);
    let total = skins.len();
    let mut summary = CompileAllItgSummary {
        total,
        ..CompileAllItgSummary::default()
    };
    for (idx, skin) in skins.iter().enumerate() {
        let label = format!("{game}/{skin}");
        let mut last_load_err = None;
        let mut loaded = None;
        for root in &roots {
            match noteskin_itg::load_noteskin_data(root, game, skin) {
                Ok(data) => {
                    loaded = Some(data);
                    break;
                }
                Err(err) => {
                    last_load_err = Some(err);
                }
            }
        }
        let result = loaded
            .ok_or_else(|| {
                last_load_err
                    .unwrap_or_else(|| format!("noteskin '{game}/{skin}' not found in any root"))
            })
            .and_then(|data| ensure_itg_compiled(game, &data).map(|outcome| (data, outcome)));
        match result {
            Ok((_data, noteskin_compiler::CompileOutcome::Built)) => {
                summary.built += 1;
                on_progress(idx + 1, total, &label, "compiled");
            }
            Ok((_data, noteskin_compiler::CompileOutcome::Reused)) => {
                summary.reused += 1;
                on_progress(idx + 1, total, &label, "");
            }
            Err(err) => {
                summary.failed += 1;
                warn!("noteskin cache compile failed for '{}': {}", label, err);
                on_progress(idx + 1, total, &label, "failed");
            }
        }
    }
    summary
}

pub fn load_itg_default(style: &Style) -> Result<Noteskin, String> {
    let roots = dirs::app_dirs().noteskin_roots();
    for root in &roots {
        if let Ok(ns) = load_itg(root, "dance", "default", style) {
            return Ok(ns);
        }
    }
    // Final attempt: try cel fallback on any root
    for root in &roots {
        if let Ok(ns) = load_itg(root, "dance", "cel", style) {
            warn!("ITG default noteskin load failed; using dance/cel fallback");
            return Ok(ns);
        }
    }
    Err("failed to load ITG default noteskin from any root".to_string())
}

pub fn discover_itg_skins(game: &str) -> Vec<String> {
    let roots = dirs::app_dirs().noteskin_roots();
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for root in &roots {
        let game_dir = root.join(game);
        let Ok(entries) = fs::read_dir(&game_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.is_empty() || name == "common" || name.starts_with('.') {
                continue;
            }
            let dir = entry.path();
            let has_itg_files = dir.join("NoteSkin.lua").is_file()
                || dir.join("metrics.ini").is_file()
                || fs::read_dir(&dir)
                    .ok()
                    .and_then(|mut it| it.next())
                    .is_some();
            if has_itg_files && seen.insert(name.clone()) {
                found.push(name);
            }
        }
    }
    found.sort();
    let mut ordered = Vec::with_capacity(found.len().max(2));
    for preferred in ["default", "cel"] {
        if let Some(pos) = found.iter().position(|s| s == preferred) {
            ordered.push(found.remove(pos));
        }
    }
    ordered.extend(found);
    if ordered.is_empty() {
        vec!["default".to_string(), "cel".to_string()]
    } else {
        ordered
    }
}

pub fn load_itg_skin(style: &Style, skin: &str) -> Result<Noteskin, String> {
    let requested = skin.trim();
    if requested.is_empty() || requested.eq_ignore_ascii_case("default") {
        return load_itg_default(style);
    }

    let roots = dirs::app_dirs().noteskin_roots();
    let mut last_err = String::new();
    for root in &roots {
        match load_itg(root, "dance", requested, style) {
            Ok(ns) => return Ok(ns),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

pub fn load_itg(root: &Path, game: &str, skin: &str, style: &Style) -> Result<Noteskin, String> {
    let data = load_itg_data_cached(root, game, skin)?;
    let bundle = if let Some(bundle) = load_itg_compiled(game, &data) {
        bundle
    } else {
        ensure_itg_compiled(game, &data).map_err(|err| {
            format!(
                "failed to compile noteskin cache for '{}/{}': {}",
                game, data.name, err
            )
        })?;
        load_itg_compiled(game, &data).ok_or_else(|| {
            format!(
                "compiled noteskin cache missing for '{}/{}' after successful compilation",
                game, data.name
            )
        })?
    };
    load_itg_sprite_noteskin_compiled(&data, style, &bundle.loader, &bundle.actors).map_err(|err| {
        format!(
            "failed to load compiled noteskin '{}/{}': {}",
            game, data.name, err
        )
    })
}

fn load_itg_sprite_noteskin_compiled(
    data: &noteskin_itg::NoteskinData,
    style: &Style,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
) -> Result<Noteskin, String> {
    let note_display_metrics = noteskin_itg::note_display_metrics(&data.metrics);
    let animation_is_beat_based = noteskin_itg::animation_is_beat_based(data);

    let mut notes = Vec::with_capacity(style.num_cols * NUM_QUANTIZATIONS);
    let mut note_layers = Vec::with_capacity(style.num_cols * NUM_QUANTIZATIONS);
    let mut lift_note_layers: Vec<Arc<[SpriteSlot]>> =
        Vec::with_capacity(style.num_cols * NUM_QUANTIZATIONS);
    let mut receptor_off = Vec::with_capacity(style.num_cols);
    let mut receptor_glow = Vec::with_capacity(style.num_cols);
    let mut receptor_off_reverse = Vec::with_capacity(style.num_cols);
    let mut receptor_glow_reverse = Vec::with_capacity(style.num_cols);
    let mut receptor_step_behaviors = Vec::with_capacity(style.num_cols);
    let mut mines = Vec::with_capacity(style.num_cols);
    let mut mine_frames = Vec::with_capacity(style.num_cols);
    let mut hold_columns = Vec::with_capacity(style.num_cols);
    let mut roll_columns = Vec::with_capacity(style.num_cols);
    let mut receptor_pulse_command: Option<String> = None;
    let resolve_slots = |button: &str, element: &str| {
        itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, button, element)
            .into_iter()
            .map(|mut s| {
                let (draw, timeline, effect) = noteskin_script::model_draw_program(&s.commands);
                s.slot.model_draw = draw;
                s.slot.model_timeline = timeline;
                s.slot.model_effect = effect;
                s.slot
            })
            .collect::<Vec<_>>()
    };
    let resolve_head_slots = |button: &str, element: &str| {
        let slots = resolve_slots(button, element);
        match slots.len() {
            0 => (None, None),
            1 => (slots.into_iter().next(), None),
            _ => (slots.first().cloned(), Some(Arc::from(slots))),
        }
    };
    let resolve_single_slot = |button: &str, element: &str| {
        let request = itg_load_request(compiled, button, element);
        itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, button, element)
            .into_iter()
            .next()
            .map(|s| s.slot)
            .or_else(|| {
                if request.blank {
                    return None;
                }
                data.resolve_path(&request.load_button, &request.load_element)
                    .and_then(|p| itg_slot_from_path(&p))
            })
    };

    for col in 0..style.num_cols {
        let button = itg_button_for_col(col);
        let mut note_sprites =
            itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, button, "Tap Note")
                .into_iter()
                .map(|mut s| {
                    let (draw, timeline, effect) = noteskin_script::model_draw_program(&s.commands);
                    s.slot.model_draw = draw;
                    s.slot.model_timeline = timeline;
                    s.slot.model_effect = effect;
                    s.slot
                })
                .collect::<Vec<_>>();
        if note_sprites.len() > 1 {
            note_sprites.sort_by_key(|slot| {
                if slot.model.is_some() {
                    if slot.uv_velocity[0].abs() > f32::EPSILON
                        || slot.uv_velocity[1].abs() > f32::EPSILON
                    {
                        0u8
                    } else {
                        1u8
                    }
                } else {
                    2u8
                }
            });
        }
        if note_sprites.is_empty()
            && let Some(fallback) =
                itg_find_texture_with_prefix(data, "_arrow").and_then(|p| itg_slot_from_path(&p))
        {
            note_sprites.push(fallback);
        }
        let model_color_texture = note_sprites
            .iter()
            .find(|slot| {
                slot.model.is_some()
                    && (slot.uv_velocity[0].abs() > f32::EPSILON
                        || slot.uv_velocity[1].abs() > f32::EPSILON)
            })
            .map(|slot| slot.texture_key().to_string())
            .or_else(|| {
                note_sprites
                    .iter()
                    .find(|slot| slot.model.is_some())
                    .map(|slot| slot.texture_key().to_string())
            });
        let note_base = model_color_texture
            .as_ref()
            .and_then(|key| note_sprites.iter().find(|slot| slot.texture_key() == key))
            .cloned()
            .or_else(|| note_sprites.first().cloned())
            .ok_or_else(|| format!("failed to resolve Tap Note for button '{button}'"))?;
        for _ in 0..NUM_QUANTIZATIONS {
            let layers = note_sprites.clone();
            let primary = layers.first().cloned().unwrap_or_else(|| note_base.clone());
            notes.push(primary);
            note_layers.push(Arc::from(layers));
        }

        let lift_sprites =
            itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, button, "Tap Lift")
                .into_iter()
                .map(|mut s| {
                    let (draw, timeline, effect) = noteskin_script::model_draw_program(&s.commands);
                    s.slot.model_draw = draw;
                    s.slot.model_timeline = timeline;
                    s.slot.model_effect = effect;
                    s.slot
                })
                .collect::<Vec<_>>();
        let lift_layers_for_col: Arc<[SpriteSlot]> = if lift_sprites.is_empty() {
            Arc::from(note_sprites.clone())
        } else {
            Arc::from(lift_sprites)
        };
        for _ in 0..NUM_QUANTIZATIONS {
            lift_note_layers.push(Arc::clone(&lift_layers_for_col));
        }

        let receptor_sprites =
            itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, button, "Receptor");
        if receptor_pulse_command.is_none() {
            receptor_pulse_command = receptor_sprites
                .first()
                .and_then(|s| s.commands.get("initcommand"))
                .cloned();
        }
        let receptor_commands = receptor_sprites.first().map(|s| &s.commands);
        let mut receptor_slot = receptor_sprites
            .first()
            .map(|s| s.slot.clone())
            .or_else(|| {
                itg_find_texture_with_prefix(data, "_receptor").and_then(|p| itg_slot_from_path(&p))
            })
            .ok_or_else(|| format!("failed to resolve Receptor for button '{button}'"))?;
        if let Some(init_command) =
            receptor_commands.and_then(|commands| commands.get("initcommand"))
        {
            itg_apply_parent_command(&mut receptor_slot, init_command);
        }
        let step_behaviors =
            itg_receptor_step_behaviors(data, receptor_commands, receptor_slot.model_draw.zoom[0]);
        let receptor_reverse = receptor_sprites
            .first()
            .map(|s| itg_receptor_reverse_behavior(&s.commands))
            .unwrap_or_default();
        let glow_slot = receptor_sprites
            .get(1)
            .map(|s| s.slot.clone())
            .or_else(|| {
                if receptor_sprites.is_empty() {
                    itg_find_texture_with_prefix(data, "_rflash")
                        .and_then(|p| itg_slot_from_path(&p))
                } else {
                    None
                }
            })
            .or_else(|| {
                if receptor_sprites.is_empty() {
                    itg_find_texture_with_prefix(data, "_glow").and_then(|p| itg_slot_from_path(&p))
                } else {
                    None
                }
            });
        let glow_reverse = receptor_sprites
            .get(1)
            .map(|s| itg_receptor_reverse_behavior(&s.commands))
            .unwrap_or_default();
        receptor_off.push(receptor_slot);
        receptor_glow.push(glow_slot);
        receptor_off_reverse.push(receptor_reverse);
        receptor_glow_reverse.push(glow_reverse);
        receptor_step_behaviors.push(step_behaviors);

        let mut mine_sprites =
            itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, button, "Tap Mine")
                .into_iter()
                .map(|mut s| {
                    let (draw, timeline, effect) = noteskin_script::model_draw_program(&s.commands);
                    s.slot.model_draw = draw;
                    s.slot.model_timeline = timeline;
                    s.slot.model_effect = effect;
                    s.slot
                })
                .collect::<Vec<_>>();
        let mine_fallback =
            itg_find_texture_with_prefix(data, "_mine").and_then(|p| itg_slot_from_path(&p));
        let mine_fill = mine_sprites
            .first()
            .cloned()
            .or_else(|| mine_sprites.get(1).cloned())
            .or_else(|| mine_fallback.clone());
        let mine_frame = if mine_sprites.len() > 1 {
            mine_sprites.get(1).cloned().or_else(|| mine_sprites.pop())
        } else {
            None
        };
        mines.push(mine_fill);
        mine_frames.push(mine_frame);

        let (hold_head_inactive, hold_head_inactive_layers) = if itg_request_maps_head_to_tap(
            &itg_load_request(compiled, button, "Hold Head Inactive"),
        ) {
            (None, None)
        } else {
            resolve_head_slots(button, "Hold Head Inactive")
        };
        let (hold_head_active, hold_head_active_layers) = if itg_request_maps_head_to_tap(
            &itg_load_request(compiled, button, "Hold Head Active"),
        ) {
            (None, None)
        } else {
            resolve_head_slots(button, "Hold Head Active")
        };
        let hold_body_inactive = resolve_single_slot(button, "Hold Body Inactive");
        let hold_body_active = resolve_single_slot(button, "Hold Body Active");
        let hold_topcap_inactive = resolve_single_slot(button, "Hold TopCap Inactive");
        let hold_topcap_active = resolve_single_slot(button, "Hold TopCap Active");
        let hold_bottomcap_inactive = resolve_single_slot(button, "Hold BottomCap Inactive");
        let hold_bottomcap_active = resolve_single_slot(button, "Hold BottomCap Active");

        let hold_head_active_layers = if hold_head_active.is_some() {
            hold_head_active_layers.clone()
        } else {
            hold_head_active_layers
                .clone()
                .or_else(|| hold_head_inactive_layers.clone())
        };
        let hold_visual = HoldVisuals {
            head_inactive: hold_head_inactive.clone(),
            head_active: hold_head_active.or(hold_head_inactive.clone()),
            head_inactive_layers: hold_head_inactive_layers.clone(),
            head_active_layers: hold_head_active_layers,
            body_inactive: hold_body_inactive.clone(),
            body_active: hold_body_active.or(hold_body_inactive.clone()),
            topcap_inactive: hold_topcap_inactive.clone(),
            topcap_active: hold_topcap_active.or(hold_topcap_inactive.clone()),
            bottomcap_inactive: hold_bottomcap_inactive.clone(),
            bottomcap_active: hold_bottomcap_active.or(hold_bottomcap_inactive.clone()),
            explosion: None,
        };

        let (roll_head_inactive, roll_head_inactive_layers) = if itg_request_maps_head_to_tap(
            &itg_load_request(compiled, button, "Roll Head Inactive"),
        ) {
            (None, None)
        } else {
            resolve_head_slots(button, "Roll Head Inactive")
        };
        let (roll_head_active, roll_head_active_layers) = if itg_request_maps_head_to_tap(
            &itg_load_request(compiled, button, "Roll Head Active"),
        ) {
            (None, None)
        } else {
            resolve_head_slots(button, "Roll Head Active")
        };
        let roll_body_inactive = resolve_single_slot(button, "Roll Body Inactive");
        let roll_body_active = resolve_single_slot(button, "Roll Body Active");
        let roll_topcap_inactive = resolve_single_slot(button, "Roll TopCap Inactive");
        let roll_topcap_active = resolve_single_slot(button, "Roll TopCap Active");
        let roll_bottomcap_inactive = resolve_single_slot(button, "Roll BottomCap Inactive");
        let roll_bottomcap_active = resolve_single_slot(button, "Roll BottomCap Active");

        let roll_head_inactive_has_slot = roll_head_inactive.is_some();
        let roll_head_active_has_slot = roll_head_active.is_some();
        let roll_head_inactive_layers_resolved = if roll_head_inactive_has_slot {
            roll_head_inactive_layers.clone()
        } else {
            roll_head_inactive_layers
                .clone()
                .or_else(|| hold_visual.head_inactive_layers.clone())
        };
        let roll_head_active_layers_resolved = if roll_head_active_has_slot {
            roll_head_active_layers.clone()
        } else if roll_head_inactive_has_slot {
            roll_head_inactive_layers.clone()
        } else {
            roll_head_active_layers
                .or(roll_head_inactive_layers)
                .or_else(|| hold_visual.head_active_layers.clone())
                .or_else(|| hold_visual.head_inactive_layers.clone())
        };
        let roll_visual = HoldVisuals {
            head_inactive: roll_head_inactive
                .clone()
                .or(hold_visual.head_inactive.clone()),
            head_active: roll_head_active
                .or(roll_head_inactive)
                .or(hold_visual.head_active.clone())
                .or(hold_visual.head_inactive.clone()),
            head_inactive_layers: roll_head_inactive_layers_resolved,
            head_active_layers: roll_head_active_layers_resolved,
            body_inactive: roll_body_inactive
                .clone()
                .or(hold_visual.body_inactive.clone()),
            body_active: roll_body_active
                .or(roll_body_inactive)
                .or(hold_visual.body_active.clone())
                .or(hold_visual.body_inactive.clone()),
            topcap_inactive: roll_topcap_inactive
                .clone()
                .or(hold_visual.topcap_inactive.clone()),
            topcap_active: roll_topcap_active
                .or(roll_topcap_inactive)
                .or(hold_visual.topcap_active.clone())
                .or(hold_visual.topcap_inactive.clone()),
            bottomcap_inactive: roll_bottomcap_inactive
                .clone()
                .or(hold_visual.bottomcap_inactive.clone()),
            bottomcap_active: roll_bottomcap_active
                .or(roll_bottomcap_inactive)
                .or(hold_visual.bottomcap_active.clone())
                .or(hold_visual.bottomcap_inactive.clone()),
            explosion: None,
        };

        hold_columns.push(hold_visual);
        roll_columns.push(roll_visual);
    }
    let down_col = (0..style.num_cols)
        .find(|&col| itg_button_for_col(col).eq_ignore_ascii_case("Down"))
        .unwrap_or(0);
    let mut hold = hold_columns
        .get(down_col)
        .cloned()
        .or_else(|| hold_columns.first().cloned())
        .unwrap_or_default();
    let mut roll = roll_columns
        .get(down_col)
        .cloned()
        .or_else(|| roll_columns.first().cloned())
        .unwrap_or_else(|| HoldVisuals {
            head_inactive: hold.head_inactive.clone(),
            head_active: hold.head_active.clone(),
            head_inactive_layers: hold.head_inactive_layers.clone(),
            head_active_layers: hold.head_active_layers.clone(),
            body_inactive: hold.body_inactive.clone(),
            body_active: hold.body_active.clone(),
            topcap_inactive: hold.topcap_inactive.clone(),
            topcap_active: hold.topcap_active.clone(),
            bottomcap_inactive: hold.bottomcap_inactive.clone(),
            bottomcap_active: hold.bottomcap_active.clone(),
            explosion: None,
        });

    let explosion_sprites =
        itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, "Down", "Explosion");
    let slot_with_active_cmd =
        |slot: &SpriteSlot, commands: &HashMap<String, String>, active_key: &str| {
            let mut with_fx = slot.clone();
            let mut scripted = HashMap::new();
            if let Some(v) = commands.get("initcommand") {
                scripted.insert("initcommand".to_string(), v.clone());
            }
            if let Some(v) = commands.get(active_key) {
                scripted.insert("nonecommand".to_string(), v.clone());
            }
            let (draw, timeline, effect) = noteskin_script::model_draw_program(&scripted);
            with_fx.model_draw = draw;
            with_fx.model_timeline = timeline;
            with_fx.model_effect = effect;
            with_fx
        };

    let find_explosion_wrapper = |active_key: &str, element_hint: &str| {
        explosion_sprites
            .iter()
            .find(|sprite| sprite.commands.contains_key(active_key))
            .or_else(|| {
                explosion_sprites
                    .iter()
                    .find(|sprite| sprite.element.to_ascii_lowercase().contains(element_hint))
            })
    };
    let hold_wrapper = find_explosion_wrapper("holdingoncommand", "hold explosion");
    let roll_wrapper = find_explosion_wrapper("rolloncommand", "roll explosion");

    let hold_explosion_request = itg_load_request(compiled, "Down", "Hold Explosion");
    let roll_explosion_request = itg_load_request(compiled, "Down", "Roll Explosion");
    let hold_explosion_blank = hold_explosion_request.blank;
    let roll_explosion_blank = roll_explosion_request.blank;
    let hold_explosion_sprites = itg_resolve_actor_sprites_compiled(
        data,
        compiled,
        compiled_actors,
        "Down",
        "Hold Explosion",
    );
    let hold_source = hold_explosion_sprites
        .iter()
        .find(|sprite| sprite.commands.contains_key("holdingoncommand"))
        .or_else(|| hold_explosion_sprites.first());
    let hold_wrapper_source =
        hold_wrapper.filter(|sprite| sprite.commands.contains_key("holdingoncommand"));
    hold.explosion = if hold_explosion_blank {
        None
    } else {
        hold_wrapper_source
            .map(|sprite| slot_with_active_cmd(&sprite.slot, &sprite.commands, "holdingoncommand"))
            .or_else(|| {
                hold_source.map(|sprite| {
                    let cmd = hold_wrapper.map_or(&sprite.commands, |wrapped| &wrapped.commands);
                    slot_with_active_cmd(&sprite.slot, cmd, "holdingoncommand")
                })
            })
            .or_else(|| {
                hold_wrapper.map(|s| slot_with_active_cmd(&s.slot, &s.commands, "holdingoncommand"))
            })
            .or_else(|| {
                data.resolve_path("Down", "Hold Explosion")
                    .and_then(|p| itg_slot_from_path(&p))
            })
            .or_else(|| {
                data.resolve_path("Down", "Hold Explosion")
                    .and_then(|p| {
                        itg_slot_from_actor_path_first_sprite_compiled(data, compiled_actors, &p)
                    })
                    .map(|slot| {
                        hold_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "holdingoncommand")
                        })
                    })
            })
            .or_else(|| {
                itg_find_texture_with_prefix(data, "_down hold explosion")
                    .and_then(|p| {
                        itg_slot_from_path_all_frames(
                            &p,
                            Some(0.01),
                            noteskin_itg::animation_is_beat_based(data),
                        )
                    })
                    .map(|slot| {
                        hold_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "holdingoncommand")
                        })
                    })
            })
    };
    let roll_explosion_sprites = itg_resolve_actor_sprites_compiled(
        data,
        compiled,
        compiled_actors,
        "Down",
        "Roll Explosion",
    );
    let roll_source = roll_explosion_sprites
        .iter()
        .find(|sprite| sprite.commands.contains_key("rolloncommand"))
        .or_else(|| roll_explosion_sprites.first());
    let roll_wrapper_source =
        roll_wrapper.filter(|sprite| sprite.commands.contains_key("rolloncommand"));
    let roll_explosion = if roll_explosion_blank {
        None
    } else {
        roll_wrapper_source
            .map(|sprite| slot_with_active_cmd(&sprite.slot, &sprite.commands, "rolloncommand"))
            .or_else(|| {
                roll_source.map(|sprite| {
                    let cmd = roll_wrapper.map_or(&sprite.commands, |wrapped| &wrapped.commands);
                    slot_with_active_cmd(&sprite.slot, cmd, "rolloncommand")
                })
            })
            .or_else(|| {
                roll_wrapper.map(|s| slot_with_active_cmd(&s.slot, &s.commands, "rolloncommand"))
            })
            .or_else(|| {
                data.resolve_path("Down", "Roll Explosion")
                    .and_then(|p| itg_slot_from_path(&p))
            })
            .or_else(|| {
                data.resolve_path("Down", "Roll Explosion")
                    .and_then(|p| {
                        itg_slot_from_actor_path_first_sprite_compiled(data, compiled_actors, &p)
                    })
                    .map(|slot| {
                        roll_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "rolloncommand")
                        })
                    })
            })
            .or_else(|| {
                itg_find_texture_with_prefix(data, "_down hold explosion")
                    .and_then(|p| {
                        itg_slot_from_path_all_frames(
                            &p,
                            Some(0.01),
                            noteskin_itg::animation_is_beat_based(data),
                        )
                    })
                    .map(|slot| {
                        roll_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "rolloncommand")
                        })
                    })
            })
    };
    roll.explosion = if roll_explosion_blank {
        None
    } else {
        roll_explosion.or(hold.explosion.clone())
    };
    if let (Some(roll_slot), Some(hold_slot)) = (roll.explosion.clone(), hold.explosion.clone()) {
        let roll_key = roll_slot.texture_key().to_ascii_lowercase();
        let hold_key = hold_slot.texture_key().to_ascii_lowercase();
        let roll_is_common_fallback_hold =
            roll_key.contains("noteskins/common/common/fallback hold explosion");
        let hold_is_skin_specific = !hold_key.contains("noteskins/common/common/");
        if roll_is_common_fallback_hold && hold_is_skin_specific {
            let roll_commands = roll_wrapper
                .filter(|sprite| sprite.commands.contains_key("rolloncommand"))
                .map(|sprite| sprite.commands.clone())
                .or_else(|| {
                    let mut metrics_commands = HashMap::new();
                    if let Some(v) = data.metrics.get("HoldGhostArrow", "RollOnCommand") {
                        metrics_commands.insert("rolloncommand".to_string(), v.to_string());
                    }
                    if let Some(v) = data.metrics.get("HoldGhostArrow", "RollOffCommand") {
                        metrics_commands.insert("rolloffcommand".to_string(), v.to_string());
                    }
                    if metrics_commands.is_empty() {
                        None
                    } else {
                        Some(metrics_commands)
                    }
                });
            if let Some(commands) = roll_commands {
                roll.explosion = Some(slot_with_active_cmd(&hold_slot, &commands, "rolloncommand"));
            } else {
                roll.explosion = Some(hold_slot);
            }
        }
    }
    let resolve_hold_explosion_for_button =
        |button: &str,
         active_key: &str,
         element_hint: &str,
         request_element: &str,
         fallback: Option<&SpriteSlot>| {
            let column_explosion_sprites = if button.eq_ignore_ascii_case("Down") {
                explosion_sprites.clone()
            } else {
                itg_resolve_actor_sprites_compiled(
                    data,
                    compiled,
                    compiled_actors,
                    button,
                    "Explosion",
                )
            };
            let wrapper = column_explosion_sprites
                .iter()
                .find(|sprite| sprite.commands.contains_key(active_key))
                .or_else(|| {
                    column_explosion_sprites
                        .iter()
                        .find(|sprite| sprite.element.to_ascii_lowercase().contains(element_hint))
                });
            if let Some(sprite) = wrapper.filter(|sprite| sprite.commands.contains_key(active_key))
            {
                return Some(slot_with_active_cmd(
                    &sprite.slot,
                    &sprite.commands,
                    active_key,
                ));
            }

            let request = itg_load_request(compiled, button, request_element);
            if request.blank {
                return None;
            }
            let source_sprites = itg_resolve_actor_sprites_compiled(
                data,
                compiled,
                compiled_actors,
                button,
                request_element,
            );
            let source = source_sprites
                .iter()
                .find(|sprite| sprite.commands.contains_key(active_key))
                .or_else(|| source_sprites.first());
            source
                .map(|sprite| {
                    let cmd = wrapper.map_or(&sprite.commands, |wrapped| &wrapped.commands);
                    slot_with_active_cmd(&sprite.slot, cmd, active_key)
                })
                .or_else(|| {
                    wrapper.map(|sprite| {
                        slot_with_active_cmd(&sprite.slot, &sprite.commands, active_key)
                    })
                })
                .or_else(|| fallback.cloned())
        };
    for (col, visuals) in hold_columns.iter_mut().enumerate() {
        let button = itg_button_for_col(col);
        visuals.explosion = resolve_hold_explosion_for_button(
            button,
            "holdingoncommand",
            "hold explosion",
            "Hold Explosion",
            hold.explosion.as_ref(),
        );
    }
    for (col, visuals) in roll_columns.iter_mut().enumerate() {
        let button = itg_button_for_col(col);
        visuals.explosion = resolve_hold_explosion_for_button(
            button,
            "rolloncommand",
            "roll explosion",
            "Roll Explosion",
            roll.explosion.as_ref(),
        );
    }
    let mut tap_explosions_by_col = Vec::with_capacity(style.num_cols);
    for col in 0..style.num_cols {
        let button = itg_button_for_col(col);
        let column_explosion_sprites = if button.eq_ignore_ascii_case("Down") {
            explosion_sprites.clone()
        } else {
            itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, button, "Explosion")
        };
        tap_explosions_by_col.push(itg_tap_explosion_map_compiled(
            data,
            compiled,
            compiled_actors,
            button,
            &column_explosion_sprites,
        ));
    }
    let mine_source = explosion_sprites
        .iter()
        .find(|sprite| sprite.commands.contains_key("hitminecommand"))
        .or_else(|| {
            explosion_sprites.iter().find(|sprite| {
                sprite
                    .element
                    .to_ascii_lowercase()
                    .contains("hitmine explosion")
            })
        });
    let mine_slot = mine_source
        .map(|sprite| sprite.slot.clone())
        .or_else(|| {
            data.resolve_path("Down", "HitMine Explosion")
                .and_then(|p| itg_slot_from_path(&p))
        })
        .or_else(|| {
            data.resolve_path("Down", "HitMine Explosion")
                .and_then(|p| {
                    itg_slot_from_actor_path_first_sprite_compiled(data, compiled_actors, &p)
                })
        });
    let mine_hit_explosion = mine_source
        .and_then(itg_mine_explosion_from_actor_source)
        .or_else(|| {
            mine_slot.map(|slot| {
                let mine_command = mine_source
                    .and_then(|sprite| sprite.commands.get("hitminecommand"))
                    .cloned()
                    .or_else(|| {
                        data.metrics
                            .get("GhostArrowBright", "HitMineCommand")
                            .map(str::to_string)
                    });
                TapExplosion::from_single(
                    slot,
                    mine_command
                        .as_deref()
                        .and_then(|cmd| itg_command_with_init(mine_source, cmd))
                        .as_deref()
                        .map(parse_explosion_animation)
                        .unwrap_or_default(),
                )
            })
        });

    let tap_explosions = tap_explosions_by_col
        .get(down_col)
        .filter(|by_window| !by_window.is_empty())
        .cloned()
        .or_else(|| {
            tap_explosions_by_col
                .iter()
                .find(|by_window| !by_window.is_empty())
                .cloned()
        })
        .unwrap_or_default();

    let hold_let_go_gray_percent = note_display_metrics
        .hold_let_go_gray_percent
        .clamp(0.0, 1.0);

    let receptor_glow_behavior =
        itg_receptor_glow_behavior_compiled(data, compiled, compiled_actors);
    let receptor_pulse = receptor_pulse_command
        .as_deref()
        .map(itg_receptor_pulse_from_script)
        .unwrap_or_default();
    let mine_fill_slots = mine_fill_slots(&mines);
    let column_xs = if style.num_cols == 0 {
        Vec::new()
    } else {
        const ITG_DANCE_COL_SPACING: i32 = 64;
        const ITG_DANCE_COL_HALF_SPACING: i32 = ITG_DANCE_COL_SPACING / 2;
        (0..style.num_cols)
            .map(|i| {
                (i as i32 * ITG_DANCE_COL_SPACING)
                    - ((style.num_cols - 1) as i32 * ITG_DANCE_COL_HALF_SPACING)
            })
            .collect()
    };
    Ok(Noteskin {
        notes,
        note_layers,
        lift_note_layers,
        receptor_off,
        receptor_glow,
        receptor_off_reverse,
        receptor_glow_reverse,
        receptor_step_behaviors,
        tap_explosions,
        tap_explosions_by_col,
        mine_hit_explosion,
        hold,
        roll,
        mine_fill_slots,
        mines,
        mine_frames,
        hold_columns,
        roll_columns,
        receptor_glow_behavior,
        receptor_pulse,
        column_xs,
        note_display_metrics,
        animation_is_beat_based,
        hold_let_go_gray_percent,
    })
}

fn itg_mine_explosion_from_actor_source(source: &ItgLuaResolvedSprite) -> Option<TapExplosion> {
    let mut layers = Vec::new();
    for key in ["ecommand", "e2command"] {
        let Some(command) = source.commands.get(key) else {
            continue;
        };
        let Some(command_with_init) = itg_command_with_init(Some(source), command) else {
            continue;
        };
        layers.push(TapExplosionLayer {
            slot: source.slot.clone(),
            animation: parse_explosion_animation(&command_with_init),
        });
    }
    TapExplosion::from_layers(layers)
}

fn itg_command_with_init(source: Option<&ItgLuaResolvedSprite>, command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let mut sequence = Vec::with_capacity(2);
    if let Some(init) = source
        .and_then(|sprite| sprite.commands.get("initcommand"))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        sequence.push(init.to_string());
    }
    sequence.push(command.to_string());
    Some(sequence.join(";"))
}

fn itg_tap_explosion_map_compiled(
    data: &noteskin_itg::NoteskinData,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
    button: &str,
    explosion_sprites: &[ItgLuaResolvedSprite],
) -> HashMap<String, TapExplosion> {
    let actor_tap_sprites = explosion_sprites
        .iter()
        .filter(|sprite| itg_sprite_has_tap_explosion_command(sprite))
        .collect::<Vec<_>>();

    let mut dim_sprites = Vec::<ItgTapExplosionSource>::new();
    let mut bright_sprites = Vec::<ItgTapExplosionSource>::new();
    let (direct_dim_sprites, direct_bright_sprites) = if actor_tap_sprites.is_empty() {
        let dim_request = itg_load_request(compiled, button, "Tap Explosion Dim");
        let bright_request = itg_load_request(compiled, button, "Tap Explosion Bright");
        (
            itg_direct_tap_explosion_sprites(
                data,
                compiled,
                compiled_actors,
                button,
                "Tap Explosion Dim",
                dim_request.blank,
            ),
            itg_direct_tap_explosion_sprites(
                data,
                compiled,
                compiled_actors,
                button,
                "Tap Explosion Bright",
                bright_request.blank,
            ),
        )
    } else {
        (Vec::new(), Vec::new())
    };

    for sprite in actor_tap_sprites
        .iter()
        .copied()
        .chain(direct_dim_sprites.iter())
        .chain(direct_bright_sprites.iter())
    {
        let source = ItgTapExplosionSource::from_sprite(sprite);
        match source.mode {
            ItgTapExplosionMode::Dim => dim_sprites.push(source),
            ItgTapExplosionMode::Bright => bright_sprites.push(source),
        }
    }

    if dim_sprites.is_empty() && bright_sprites.is_empty() {
        return HashMap::new();
    }

    let mut tap_explosions = HashMap::new();
    for window in ["W1", "W2", "W3", "W4", "W5", "Miss", "Held"] {
        let key = format!("{}command", window.to_ascii_lowercase());
        for mode in [ItgTapExplosionMode::Dim, ItgTapExplosionMode::Bright] {
            if mode == ItgTapExplosionMode::Bright && bright_sprites.is_empty() {
                continue;
            }
            let sources = itg_tap_explosion_sources_for_window(
                &dim_sprites,
                &bright_sprites,
                window,
                &key,
                mode,
            );
            if sources.is_empty() {
                continue;
            }
            let metric_key = format!("{window}Command");
            let mut layers = Vec::with_capacity(sources.len());
            for source in sources {
                let command =
                    source.commands.get(&key).cloned().or_else(|| {
                        itg_tap_explosion_metric_command(data, source.mode, &metric_key)
                    });
                if window == "Held" && command.as_deref().map_or(true, |c| c.trim().is_empty()) {
                    continue;
                }
                let Some(command) = command.filter(|cmd| !cmd.trim().is_empty()) else {
                    continue;
                };
                let Some(command_with_init) =
                    itg_tap_explosion_command_with_init(source, mode, &command)
                else {
                    continue;
                };
                layers.push(TapExplosionLayer {
                    slot: source.slot.clone(),
                    animation: parse_explosion_animation(&command_with_init),
                });
            }
            if let Some(explosion) = TapExplosion::from_layers(layers) {
                tap_explosions.insert(itg_tap_explosion_key(window, mode).to_string(), explosion);
            }
        }
    }
    tap_explosions
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItgTapExplosionMode {
    Dim,
    Bright,
}

#[derive(Clone)]
struct ItgTapExplosionSource {
    element: String,
    slot: SpriteSlot,
    commands: HashMap<String, String>,
    mode: ItgTapExplosionMode,
}

impl ItgTapExplosionSource {
    fn from_sprite(sprite: &ItgLuaResolvedSprite) -> Self {
        Self {
            element: sprite.element.clone(),
            slot: sprite.slot.clone(),
            commands: sprite.commands.clone(),
            mode: itg_tap_explosion_mode(&sprite.element)
                .or_else(|| itg_tap_explosion_mode_from_commands(&sprite.commands))
                .unwrap_or(ItgTapExplosionMode::Dim),
        }
    }

    fn matches_window(&self, window: &str) -> bool {
        itg_tap_explosion_element_window(&self.element)
            .is_some_and(|value| value.eq_ignore_ascii_case(window))
    }

    fn applies_to_window(&self, window: &str, command_key: &str) -> bool {
        self.commands.contains_key(command_key)
            || self.matches_window(window)
            || self.is_generic_tap_explosion()
    }

    fn is_generic_tap_explosion(&self) -> bool {
        let element = self.element.trim().to_ascii_lowercase();
        (element == "tap explosion dim" || element == "tap explosion bright")
            && itg_tap_explosion_element_window(&self.element).is_none()
    }
}

fn itg_tap_explosion_sources_for_window<'a>(
    dim_sprites: &'a [ItgTapExplosionSource],
    bright_sprites: &'a [ItgTapExplosionSource],
    window: &str,
    command_key: &str,
    mode: ItgTapExplosionMode,
) -> Vec<&'a ItgTapExplosionSource> {
    let mut out = Vec::new();
    let (preferred, fallback) = match mode {
        ItgTapExplosionMode::Dim => (dim_sprites, bright_sprites),
        ItgTapExplosionMode::Bright => (bright_sprites, dim_sprites),
    };
    out.extend(
        preferred
            .iter()
            .filter(|sprite| sprite.applies_to_window(window, command_key)),
    );
    let has_preferred = !out.is_empty();
    if mode == ItgTapExplosionMode::Bright && !has_preferred {
        return out;
    }
    if !has_preferred {
        out.extend(
            fallback
                .iter()
                .filter(|sprite| sprite.applies_to_window(window, command_key)),
        );
    }
    if out.is_empty() {
        if let Some(first) = preferred.first() {
            out.push(first);
        } else if let Some(first) = fallback.first() {
            out.push(first);
        }
    } else if has_preferred {
        out.extend(
            fallback
                .iter()
                .filter(|sprite| sprite.applies_to_window(window, command_key)),
        );
    }
    out
}

fn itg_tap_explosion_command_with_init(
    source: &ItgTapExplosionSource,
    mode: ItgTapExplosionMode,
    command: &str,
) -> Option<String> {
    let mut sequence = Vec::with_capacity(4);
    let mut push_command = |raw: Option<&String>| {
        if let Some(value) = raw {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                sequence.push(trimmed.to_string());
            }
        }
    };
    push_command(source.commands.get("initcommand"));
    push_command(source.commands.get("judgmentcommand"));
    push_command(
        source
            .commands
            .get(itg_tap_explosion_mode_command_key(mode)),
    );
    sequence.push(command.trim().to_string());
    (!sequence.is_empty()).then(|| sequence.join(";"))
}

fn itg_tap_explosion_mode(element: &str) -> Option<ItgTapExplosionMode> {
    let element = element.to_ascii_lowercase();
    if element.starts_with("tap explosion bright") {
        Some(ItgTapExplosionMode::Bright)
    } else if element.starts_with("tap explosion dim") {
        Some(ItgTapExplosionMode::Dim)
    } else {
        None
    }
}

fn itg_tap_explosion_mode_command_key(mode: ItgTapExplosionMode) -> &'static str {
    match mode {
        ItgTapExplosionMode::Dim => "dimcommand",
        ItgTapExplosionMode::Bright => "brightcommand",
    }
}

fn itg_tap_explosion_metric_command(
    data: &noteskin_itg::NoteskinData,
    mode: ItgTapExplosionMode,
    metric_key: &str,
) -> Option<String> {
    let section = match mode {
        ItgTapExplosionMode::Dim => "GhostArrowDim",
        ItgTapExplosionMode::Bright => "GhostArrowBright",
    };
    data.metrics.get(section, metric_key).map(str::to_string)
}

fn itg_tap_explosion_key(window: &str, mode: ItgTapExplosionMode) -> &str {
    if mode == ItgTapExplosionMode::Bright
        && let Some(key) = deadsync_noteskin::bright_tap_explosion_key(window)
    {
        key
    } else {
        window
    }
}

fn itg_tap_explosion_mode_from_commands(
    commands: &HashMap<String, String>,
) -> Option<ItgTapExplosionMode> {
    let bright_visible = commands
        .get("brightcommand")
        .and_then(|cmd| itg_script_visible_command(cmd));
    let dim_visible = commands
        .get("dimcommand")
        .and_then(|cmd| itg_script_visible_command(cmd));
    match (bright_visible, dim_visible) {
        (Some(true), Some(false)) => Some(ItgTapExplosionMode::Bright),
        (Some(false), Some(true)) => Some(ItgTapExplosionMode::Dim),
        (None, Some(true)) => Some(ItgTapExplosionMode::Dim),
        (Some(true), None) => Some(ItgTapExplosionMode::Bright),
        _ => None,
    }
}

fn itg_script_visible_command(script: &str) -> Option<bool> {
    let script = normalized_script_command(script);
    script.split(';').find_map(|token| {
        let (command, args) = split_script_token(token)?;
        (command == "visible")
            .then(|| args.first().map(|arg| parse_script_bool(arg)))
            .flatten()
    })
}

fn itg_sprite_has_tap_explosion_command(sprite: &ItgLuaResolvedSprite) -> bool {
    [
        "w1command",
        "w2command",
        "w3command",
        "w4command",
        "w5command",
        "heldcommand",
    ]
    .iter()
    .any(|key| sprite.commands.contains_key(*key))
}

fn itg_tap_explosion_element_window(element: &str) -> Option<&str> {
    let element = element.trim();
    element
        .strip_prefix("Tap Explosion Dim ")
        .or_else(|| element.strip_prefix("Tap Explosion Bright "))
        .map(str::trim)
        .filter(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "w1" | "w2" | "w3" | "w4" | "w5"
            )
        })
}

fn itg_direct_tap_explosion_sprites(
    data: &noteskin_itg::NoteskinData,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
    button: &str,
    base_element: &str,
    base_blank: bool,
) -> Vec<ItgLuaResolvedSprite> {
    let mut out = Vec::new();
    if !base_blank {
        out.extend(itg_resolve_actor_sprites_compiled(
            data,
            compiled,
            compiled_actors,
            button,
            base_element,
        ));
    }
    for window in ["W1", "W2", "W3", "W4", "W5"] {
        let element = format!("{base_element} {window}");
        if itg_load_request(compiled, button, &element).blank {
            continue;
        }
        out.extend(itg_resolve_actor_sprites_compiled(
            data,
            compiled,
            compiled_actors,
            button,
            &element,
        ));
    }
    out
}

fn itg_receptor_pulse_from_script(command: &str) -> ReceptorPulse {
    let mut pulse = ReceptorPulse::default();
    let command = normalized_script_command(command);
    for raw_token in command.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = split_script_token(token) else {
            continue;
        };
        if let Some(effect_mod) = parse_script_effect_mod(cmd.as_str(), &args) {
            match effect_mod {
                ScriptEffectMod::EffectColor1(color) => pulse.effect_color1 = color,
                ScriptEffectMod::EffectColor2(color) => pulse.effect_color2 = color,
                ScriptEffectMod::EffectPeriod(v) => {
                    let period = v.max(f32::EPSILON);
                    pulse.effect_period = period;
                    pulse.ramp_to_half = period * 0.5;
                    pulse.hold_at_half = 0.0;
                    pulse.ramp_to_full = period * 0.5;
                    pulse.hold_at_full = 0.0;
                    pulse.hold_at_zero = 0.0;
                }
                ScriptEffectMod::EffectOffset(v) => {
                    pulse.effect_offset = v;
                }
                ScriptEffectMod::EffectTiming(v) => {
                    pulse.ramp_to_half = v[0].max(0.0);
                    pulse.hold_at_half = v[1].max(0.0);
                    pulse.ramp_to_full = v[2].max(0.0);
                    pulse.hold_at_full = v[3].max(0.0);
                    pulse.hold_at_zero = v[4].max(0.0);
                    pulse.effect_period = pulse.total_period().max(f32::EPSILON);
                }
                _ => {}
            }
        }
    }

    pulse
}

fn itg_receptor_arrow_command(
    data: &noteskin_itg::NoteskinData,
    commands: Option<&HashMap<String, String>>,
    actor_key: &str,
    metric_key: &str,
) -> Option<String> {
    commands
        .and_then(|commands| commands.get(actor_key).cloned())
        .or_else(|| {
            data.metrics
                .get("ReceptorArrow", metric_key)
                .map(str::to_string)
        })
}

fn itg_receptor_step_behavior_for_command(
    command: Option<String>,
    base_zoom: f32,
) -> ReceptorStepBehavior {
    let Some(command) = command else {
        return ReceptorStepBehavior::identity();
    };
    let none = itg_parse_command_effect(&command);
    let Some(zoom_start) = none.start_zoom.or(none.target_zoom) else {
        return ReceptorStepBehavior {
            interrupts: none.interrupts,
            ..ReceptorStepBehavior::identity()
        };
    };
    let zoom_end = none.target_zoom.or(none.start_zoom).unwrap_or(zoom_start);
    if (zoom_end - zoom_start).abs() <= f32::EPSILON {
        return ReceptorStepBehavior {
            interrupts: none.interrupts,
            ..ReceptorStepBehavior::identity()
        };
    }
    let base_zoom = if base_zoom.abs() > f32::EPSILON {
        base_zoom
    } else {
        1.0
    };

    ReceptorStepBehavior {
        duration: none.duration.max(0.0),
        zoom_start: (zoom_start / base_zoom).max(0.0),
        zoom_end: (zoom_end / base_zoom).max(0.0),
        tween: if none.duration > f32::EPSILON {
            none.tween
        } else {
            TweenType::Linear
        },
        interrupts: none.interrupts,
    }
}

fn itg_receptor_step_behaviors(
    data: &noteskin_itg::NoteskinData,
    commands: Option<&HashMap<String, String>>,
    base_zoom: f32,
) -> ReceptorStepBehaviors {
    let behavior = |actor_key, metric_key| {
        itg_receptor_step_behavior_for_command(
            itg_receptor_arrow_command(data, commands, actor_key, metric_key),
            base_zoom,
        )
    };
    ReceptorStepBehaviors::new(
        behavior("nonecommand", "NoneCommand"),
        behavior("misscommand", "MissCommand"),
        [
            behavior("w1command", "W1Command"),
            behavior("w2command", "W2Command"),
            behavior("w3command", "W3Command"),
            behavior("w4command", "W4Command"),
            behavior("w5command", "W5Command"),
        ],
    )
}

fn itg_receptor_reverse_behavior(commands: &HashMap<String, String>) -> ReceptorReverseBehavior {
    ReceptorReverseBehavior {
        reverse_off: commands
            .get("reverseoffcommand")
            .map(|script| itg_receptor_reverse_state(script))
            .unwrap_or_default(),
        reverse_on: commands
            .get("reverseoncommand")
            .map(|script| itg_receptor_reverse_state(script))
            .unwrap_or_default(),
    }
}

fn itg_receptor_reverse_state(script: &str) -> ReceptorReverseState {
    let mut out = ReceptorReverseState::default();
    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = split_script_token(token) else {
            continue;
        };
        match cmd.as_str() {
            "baserotationz" => {
                out.base_rotation_z = args.first().and_then(|v| parse_script_number(v));
            }
            "vertalign" | "valign" => {
                out.vert_align = args.first().and_then(|v| parse_script_vertalign(v));
            }
            _ => {}
        }
    }
    out
}

fn itg_receptor_glow_behavior_compiled(
    data: &noteskin_itg::NoteskinData,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
) -> ReceptorGlowBehavior {
    let mut out = ReceptorGlowBehavior::default();
    let receptor =
        itg_resolve_actor_sprites_compiled(data, compiled, compiled_actors, "Down", "Receptor");
    let overlay = receptor.get(1);
    let init_cmd = overlay
        .and_then(|s| s.commands.get("initcommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "InitCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();
    let press_cmd = overlay
        .and_then(|s| s.commands.get("presscommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "PressCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();
    let lift_cmd = overlay
        .and_then(|s| s.commands.get("liftcommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "LiftCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();
    let none_cmd = overlay
        .and_then(|s| s.commands.get("nonecommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "NoneCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();

    let init = itg_parse_command_effect(&init_cmd);
    let press = itg_parse_command_effect(&press_cmd);
    let lift = itg_parse_command_effect(&lift_cmd);
    let none = itg_parse_command_effect(&none_cmd);

    out.press_duration = press.duration.max(0.0);
    out.press_alpha_start = press
        .start_alpha
        .or(press.target_alpha)
        .or(init.target_alpha)
        .unwrap_or(out.press_alpha_start);
    out.press_alpha_end = press
        .target_alpha
        .or(press.start_alpha)
        .or(init.target_alpha)
        .unwrap_or(out.press_alpha_end);
    out.press_zoom_start = press
        .start_zoom
        .or(press.target_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.press_zoom_start);
    out.press_zoom_end = press
        .target_zoom
        .or(press.start_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.press_zoom_end);
    out.press_tween = if press.duration > f32::EPSILON {
        press.tween
    } else {
        out.press_tween
    };

    out.duration = if lift.duration > f32::EPSILON {
        lift.duration
    } else if none.duration > f32::EPSILON {
        none.duration
    } else if press.duration > f32::EPSILON {
        press.duration
    } else {
        out.duration
    };
    out.alpha_start = out.press_alpha_end;
    out.alpha_end = lift
        .target_alpha
        .or(none.target_alpha)
        .or(init.target_alpha)
        .unwrap_or(0.0);
    out.zoom_start = out.press_zoom_end;
    out.zoom_end = lift
        .target_zoom
        .or(none.target_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.zoom_end);
    out.tween = if lift.duration > f32::EPSILON {
        lift.tween
    } else if none.duration > f32::EPSILON {
        none.tween
    } else if press.duration > f32::EPSILON {
        press.tween
    } else {
        out.tween
    };
    out.blend_add = press
        .blend_add
        .or(lift.blend_add)
        .or(init.blend_add)
        .unwrap_or(out.blend_add);
    out.press_alpha_start = out.press_alpha_start.clamp(0.0, 1.0);
    out.press_alpha_end = out.press_alpha_end.clamp(0.0, 1.0);
    out.press_zoom_start = out.press_zoom_start.max(0.0);
    out.press_zoom_end = out.press_zoom_end.max(0.0);
    out.press_duration = out.press_duration.max(0.0);
    out.alpha_start = out.alpha_start.clamp(0.0, 1.0);
    out.alpha_end = out.alpha_end.clamp(0.0, 1.0);
    out.zoom_start = out.zoom_start.max(0.0);
    out.zoom_end = out.zoom_end.max(0.0);
    out.duration = out.duration.max(0.0);
    out
}

fn itg_apply_parent_zoom(slot: &mut SpriteSlot, axis: usize, zoom: f32) {
    if zoom < 0.0 {
        match axis {
            0 => slot.def.mirror_h = !slot.def.mirror_h,
            1 => slot.def.mirror_v = !slot.def.mirror_v,
            _ => {}
        }
    }
    slot.model_draw.zoom[axis] *= zoom.abs();
}

fn itg_apply_parent_actor_mod(slot: &mut SpriteSlot, actor_mod: ScriptActorMod) {
    match actor_mod {
        ScriptActorMod::X(v) | ScriptActorMod::AddX(v) => slot.model_draw.pos[0] += v,
        ScriptActorMod::Y(v) | ScriptActorMod::AddY(v) => slot.model_draw.pos[1] += v,
        ScriptActorMod::Z(v) | ScriptActorMod::AddZ(v) => slot.model_draw.pos[2] += v,
        ScriptActorMod::RotationX(v) | ScriptActorMod::AddRotationX(v) => {
            slot.model_draw.rot[0] += v
        }
        ScriptActorMod::RotationY(v) | ScriptActorMod::AddRotationY(v) => {
            slot.model_draw.rot[1] += v
        }
        ScriptActorMod::RotationZ(v) | ScriptActorMod::AddRotationZ(v) => {
            slot.model_draw.rot[2] += v
        }
        ScriptActorMod::Zoom(v) => {
            itg_apply_parent_zoom(slot, 0, v);
            itg_apply_parent_zoom(slot, 1, v);
            itg_apply_parent_zoom(slot, 2, v);
        }
        ScriptActorMod::ZoomX(v) => itg_apply_parent_zoom(slot, 0, v),
        ScriptActorMod::ZoomY(v) => itg_apply_parent_zoom(slot, 1, v),
        ScriptActorMod::ZoomZ(v) => itg_apply_parent_zoom(slot, 2, v),
        ScriptActorMod::Diffuse(color) => {
            for (dst, src) in slot.model_draw.tint.iter_mut().zip(color) {
                *dst *= src;
            }
        }
        ScriptActorMod::DiffuseAlpha(alpha) => slot.model_draw.tint[3] *= alpha,
        ScriptActorMod::Glow(color) => slot.model_draw.glow = color,
        ScriptActorMod::VertAlign(v) => slot.model_draw.vert_align = v,
        ScriptActorMod::BlendAdd(v) => slot.model_draw.blend_add = v,
        ScriptActorMod::Visible(v) => slot.model_draw.visible &= v,
    }
}

fn itg_apply_parent_command(slot: &mut SpriteSlot, script: &str) {
    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((command, args)) = split_script_token(token) else {
            continue;
        };
        if let Some(actor_mod) = parse_script_actor_mod(&command, &args) {
            itg_apply_parent_actor_mod(slot, actor_mod);
        }
    }
}

fn itg_apply_loader_command(sprites: &mut [ItgLuaResolvedSprite], command: Option<&str>) {
    let Some(command) = command.filter(|cmd| !cmd.trim().is_empty()) else {
        return;
    };
    for sprite in sprites {
        itg_apply_parent_command(&mut sprite.slot, command);
    }
}

type ItgLuaActorDecl = noteskin_actor::ItgLuaActorDecl;

#[derive(Debug, Clone)]
struct ItgLuaResolvedSprite {
    element: String,
    slot: SpriteSlot,
    commands: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct ItgLoadRequest {
    blank: bool,
    load_button: String,
    load_element: String,
    rotation_z: Option<i32>,
    init_command: Option<String>,
}

#[inline(always)]
fn itg_button_for_col(col: usize) -> &'static str {
    match col % 4 {
        0 => "Left",
        1 => "Down",
        2 => "Up",
        _ => "Right",
    }
}

fn itg_load_request(
    compiled: &noteskin_compiled::CompiledLoader,
    button: &str,
    element: &str,
) -> ItgLoadRequest {
    if let Some(entry) = compiled.find(button, element) {
        return ItgLoadRequest {
            blank: entry.blank,
            load_button: entry.load_button.clone(),
            load_element: entry.load_element.clone(),
            rotation_z: entry.rotation_z,
            init_command: entry.init_command.clone(),
        };
    }
    warn!("compiled noteskin loader is missing '{button} {element}'");
    ItgLoadRequest {
        blank: false,
        load_button: button.to_string(),
        load_element: element.to_string(),
        rotation_z: None,
        init_command: None,
    }
}

fn itg_request_maps_head_to_tap(request: &ItgLoadRequest) -> bool {
    !request.blank && request.load_element.eq_ignore_ascii_case("Tap Note")
}

fn itg_compiled_actor_decl(
    data: &noteskin_itg::NoteskinData,
    compiled_actors: &noteskin_compiled::CompiledActors,
    path: &Path,
) -> Option<ItgLuaActorDecl> {
    let key = noteskin_compiled::actor_manifest_key(&data.search_dirs, path)?;
    compiled_actors.find(&key).cloned().map(|file| file.decl)
}

fn itg_resolve_actor_sprites_compiled(
    data: &noteskin_itg::NoteskinData,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
    button: &str,
    element: &str,
) -> Vec<ItgLuaResolvedSprite> {
    let mut visiting = HashSet::new();
    itg_resolve_actor_sprites_inner_compiled(
        data,
        compiled,
        compiled_actors,
        button,
        element,
        0,
        &mut visiting,
    )
}

fn itg_resolve_actor_sprites_inner_compiled(
    data: &noteskin_itg::NoteskinData,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
    button: &str,
    element: &str,
    depth: usize,
    visiting: &mut HashSet<String>,
) -> Vec<ItgLuaResolvedSprite> {
    if depth > 24 {
        warn!("noteskin lua actor recursion depth exceeded at '{button} {element}'");
        return Vec::new();
    }

    let visit_key = format!(
        "{}|{}",
        button.to_ascii_lowercase(),
        element.to_ascii_lowercase()
    );
    if !visiting.insert(visit_key.clone()) {
        warn!("noteskin lua actor recursion loop detected at '{button} {element}'");
        return Vec::new();
    }

    let request = itg_load_request(compiled, button, element);
    if request.blank {
        visiting.remove(&visit_key);
        return Vec::new();
    }
    let path = data.resolve_path(&request.load_button, &request.load_element);
    let Some(path) = path else {
        visiting.remove(&visit_key);
        return Vec::new();
    };

    let mut out = itg_resolve_actor_file_compiled(
        data,
        compiled,
        compiled_actors,
        button,
        element,
        &path,
        request.rotation_z,
        depth,
        visiting,
        None,
    );
    itg_apply_loader_command(&mut out, request.init_command.as_deref());

    visiting.remove(&visit_key);
    out
}

fn itg_resolve_actor_file_compiled(
    data: &noteskin_itg::NoteskinData,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
    button: &str,
    element: &str,
    path: &Path,
    rotation_z: Option<i32>,
    depth: usize,
    visiting: &mut HashSet<String>,
    arg0_path: Option<&Path>,
) -> Vec<ItgLuaResolvedSprite> {
    if depth > 48 {
        warn!(
            "noteskin lua file recursion depth exceeded at '{}' for '{button} {element}'",
            path.display()
        );
        return Vec::new();
    }

    let mut out = Vec::new();
    let is_lua = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"));
    if !is_lua {
        if let Some(mut slot) =
            itg_slot_from_path_with_frame(path, 0).or_else(|| itg_slot_from_path(path))
        {
            if let Some(rot) = rotation_z {
                slot.set_rotation_deg(rot);
            }
            out.push(ItgLuaResolvedSprite {
                element: element.to_string(),
                slot,
                commands: HashMap::new(),
            });
        }
        return out;
    }

    let path_key = format!("file:{}", path.display().to_string().to_ascii_lowercase());
    if !visiting.insert(path_key.clone()) {
        warn!(
            "noteskin lua file recursion loop detected at '{}' for '{button} {element}'",
            path.display()
        );
        return Vec::new();
    }

    let Some(decl) = itg_compiled_actor_decl(data, compiled_actors, path) else {
        warn!("compiled noteskin actors are missing '{}'", path.display());
        visiting.remove(&path_key);
        return Vec::new();
    };
    let default_anim_is_beat = noteskin_itg::animation_is_beat_based(data);
    for sprite in decl.sprites {
        let texture_path = itg_resolve_texture_expr(data, &sprite.texture_expr, arg0_path);
        let Some(texture_path) = texture_path else {
            continue;
        };
        let anim_is_beat =
            itg_sprite_animation_is_beat_based(&sprite.commands, default_anim_is_beat);
        let mut slot = if sprite.frame_count > 1 {
            itg_slot_from_path_animated(
                &texture_path,
                sprite.frame0,
                sprite.frame_count,
                sprite.frame_indices.as_deref(),
                sprite.frame_delays.as_deref(),
                anim_is_beat,
            )
            .or_else(|| itg_slot_from_path_with_frame(&texture_path, sprite.frame0))
        } else {
            itg_slot_from_path_with_frame(&texture_path, sprite.frame0)
        }
        .or_else(|| itg_slot_from_path(&texture_path));
        let Some(mut slot) = slot.take() else {
            continue;
        };
        if let Some(rot) = rotation_z {
            slot.set_rotation_deg(rot);
        }
        itg_apply_state_properties_from_commands(&mut slot, &sprite.commands);
        out.push(ItgLuaResolvedSprite {
            element: element.to_string(),
            slot,
            commands: sprite.commands,
        });
    }
    for model in decl.models {
        let model_path = model
            .materials_expr
            .as_deref()
            .or(model.meshes_expr.as_deref())
            .or(model.texture_expr.as_deref())
            .and_then(|expr| itg_resolve_texture_expr(data, expr, arg0_path));
        let Some(model_path) = model_path else {
            continue;
        };
        let (draw, timeline, effect) = noteskin_script::model_draw_program(&model.commands);
        let model_auto_rot = itg_parse_milkshape_model_auto_rot(&model_path);
        if let Some(model_layers) = itg_parse_milkshape_model_layers(data, &model_path) {
            let mut pushed = false;
            for layer in model_layers {
                let mut slot =
                    itg_slot_from_path_with_frame(&layer.texture.texture_path, model.frame0)
                        .or_else(|| itg_slot_from_path(&layer.texture.texture_path));
                let Some(mut slot) = slot.take() else {
                    continue;
                };
                slot.model = Some(layer.mesh);
                slot.model_draw = draw;
                slot.model_timeline = Arc::clone(&timeline);
                slot.model_effect = effect;
                if let Some(auto_rot) = model_auto_rot.as_ref() {
                    slot.model_auto_rot_total_frames = auto_rot.total_frames;
                    slot.model_auto_rot_z_keys = Arc::clone(&auto_rot.z_keys);
                }
                slot.note_color_translate = !layer.flags.nomove;
                slot.uv_velocity = if layer.flags.nomove {
                    [0.0, 0.0]
                } else {
                    layer.texture.tex.uv_velocity
                };
                slot.uv_offset = layer.texture.tex.uv_offset;
                slot.uv_cycle_seconds = layer.texture.tex.uv_cycle_seconds;
                if let Some(rot) = rotation_z {
                    slot.set_rotation_deg(rot);
                }
                out.push(ItgLuaResolvedSprite {
                    element: element.to_string(),
                    slot,
                    commands: model.commands.clone(),
                });
                pushed = true;
            }
            if pushed {
                continue;
            }
        }

        let Some(model_texture) = itg_resolve_model_texture_path(data, &model_path) else {
            warn!(
                "noteskin model '{}' for '{button} {element}' did not resolve a texture fallback",
                model_path.display()
            );
            continue;
        };
        let mut slot = itg_slot_from_path_with_frame(&model_texture.texture_path, model.frame0)
            .or_else(|| itg_slot_from_path(&model_texture.texture_path));
        let Some(mut slot) = slot.take() else {
            continue;
        };
        slot.model = itg_parse_milkshape_model(data, &model_path);
        slot.model_draw = draw;
        slot.model_timeline = timeline;
        slot.model_effect = effect;
        if let Some(auto_rot) = model_auto_rot.as_ref() {
            slot.model_auto_rot_total_frames = auto_rot.total_frames;
            slot.model_auto_rot_z_keys = Arc::clone(&auto_rot.z_keys);
        }
        slot.uv_velocity = model_texture.tex.uv_velocity;
        slot.uv_offset = model_texture.tex.uv_offset;
        slot.uv_cycle_seconds = model_texture.tex.uv_cycle_seconds;
        if let Some(rot) = rotation_z {
            slot.set_rotation_deg(rot);
        }
        out.push(ItgLuaResolvedSprite {
            element: element.to_string(),
            slot,
            commands: model.commands,
        });
    }
    for path_ref in decl.path_refs {
        let Some(path) = itg_resolve_texture_expr(data, &path_ref.path_expr, arg0_path) else {
            continue;
        };
        let path_ref_arg = path_ref
            .arg_expr
            .as_deref()
            .and_then(|expr| itg_resolve_texture_expr(data, expr, arg0_path));
        let mut child = itg_resolve_actor_file_compiled(
            data,
            compiled,
            compiled_actors,
            button,
            element,
            &path,
            rotation_z,
            depth + 1,
            visiting,
            path_ref_arg.as_deref(),
        );
        for sprite in &mut child {
            if let Some(frame) = path_ref.frame_override {
                itg_apply_frame_override(&mut sprite.slot, frame);
            }
            for (k, v) in &path_ref.commands {
                sprite.commands.insert(k.clone(), v.clone());
            }
            itg_apply_state_properties_from_commands(&mut sprite.slot, &sprite.commands);
        }
        out.extend(child);
    }
    for reference in decl.refs {
        let child_button = reference.button_override.as_deref().unwrap_or(button);
        let wrapper_commands = reference
            .wrapper_expr
            .as_deref()
            .and_then(|expr| itg_resolve_texture_expr(data, expr, arg0_path))
            .and_then(|path| noteskin_actor::parse_wrapper_commands_from_file(&path, &data.metrics))
            .unwrap_or_default();
        let mut child = itg_resolve_actor_sprites_inner_compiled(
            data,
            compiled,
            compiled_actors,
            child_button,
            &reference.element,
            depth + 1,
            visiting,
        );
        for sprite in &mut child {
            if let Some(frame) = reference.frame_override {
                itg_apply_frame_override(&mut sprite.slot, frame);
            }
            for (k, v) in &wrapper_commands {
                sprite.commands.insert(k.clone(), v.clone());
            }
            for (k, v) in &reference.commands {
                sprite.commands.insert(k.clone(), v.clone());
            }
            itg_apply_state_properties_from_commands(&mut sprite.slot, &sprite.commands);
        }
        out.extend(child);
    }

    visiting.remove(&path_key);
    out
}

fn itg_resolve_texture_expr(
    data: &noteskin_itg::NoteskinData,
    expr: &str,
    arg0_path: Option<&Path>,
) -> Option<PathBuf> {
    let value = expr.trim();
    if value == ITG_ARG0_TOKEN {
        return arg0_path.map(Path::to_path_buf);
    }
    if value.starts_with("NOTESKIN:GetPath(") {
        let args = itg_extract_quoted_strings(value);
        if args.len() >= 2 {
            return data
                .resolve_path(&args[0], &args[1])
                .or_else(|| data.resolve_path("", &args[1]));
        }
        if args.len() == 1 {
            return data
                .resolve_path(&args[0], "")
                .or_else(|| data.resolve_path("", &args[0]));
        }
    }
    let name = itg_parse_lua_quoted(value).unwrap_or_else(|| value.to_string());
    data.resolve_path(&name, "")
        .or_else(|| data.resolve_path("", &name))
        .or_else(|| {
            if value == "..." {
                arg0_path.map(Path::to_path_buf)
            } else {
                None
            }
        })
}

fn itg_sprite_animation_is_beat_based(
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
) -> bool {
    let mut clock = None;
    let preferred = ["initcommand", "nonecommand", "oncommand", "offcommand"];
    for key in preferred {
        if let Some(script) = commands.get(key)
            && let Some(is_beat) = itg_parse_effectclock_from_commands(script)
        {
            clock = Some(is_beat);
        }
    }
    let mut extras = commands
        .iter()
        .filter(|(key, _)| !preferred.contains(&key.as_str()))
        .map(|(key, script)| (key.as_str(), script.as_str()))
        .collect::<Vec<_>>();
    extras.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (_, script) in extras {
        if let Some(is_beat) = itg_parse_effectclock_from_commands(script) {
            clock = Some(is_beat);
        }
    }
    clock.unwrap_or(default_is_beat_based)
}

fn itg_parse_effectclock_from_commands(script: &str) -> Option<bool> {
    parse_script_effectclock_from_commands(script)
}

fn itg_slot_from_actor_path_first_sprite_compiled(
    data: &noteskin_itg::NoteskinData,
    compiled_actors: &noteskin_compiled::CompiledActors,
    path: &Path,
) -> Option<SpriteSlot> {
    let is_lua = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"));
    if !is_lua {
        return itg_slot_from_path(path);
    }

    let decl = itg_compiled_actor_decl(data, compiled_actors, path)?;
    let default_anim_is_beat = noteskin_itg::animation_is_beat_based(data);
    for sprite in decl.sprites {
        let texture_path = itg_resolve_texture_expr(data, &sprite.texture_expr, None)?;
        let anim_is_beat =
            itg_sprite_animation_is_beat_based(&sprite.commands, default_anim_is_beat);
        let slot = if sprite.frame_count > 1 {
            itg_slot_from_path_animated(
                &texture_path,
                sprite.frame0,
                sprite.frame_count,
                sprite.frame_indices.as_deref(),
                sprite.frame_delays.as_deref(),
                anim_is_beat,
            )
            .or_else(|| itg_slot_from_path_with_frame(&texture_path, sprite.frame0))
        } else {
            itg_slot_from_path_with_frame(&texture_path, sprite.frame0)
        }
        .or_else(|| itg_slot_from_path(&texture_path));
        if let Some(slot) = slot {
            return Some(slot);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationRate, ModelAutoRotKey, ModelDrawState, ModelEffectClock, ModelEffectMode,
        ModelTweenSegment, NUM_QUANTIZATIONS, NoteAnimPart, NoteColorType, Quantization,
        SpriteDefinition, SpriteSlot, SpriteSource, Style, clear_itg_runtime_caches,
        compiled_bundle_path, itg_apply_state_properties_from_script, itg_parse_command_effect,
        itg_receptor_pulse_from_script, itg_register_texture_dims_for_path, itg_texture_key,
        load_itg, load_itg_data_cached, load_itg_model_slots_from_path, load_itg_skin,
        noteskin_compiled, parse_explosion_animation,
    };
    use deadsync_noteskin::script::{model_draw_program, parse_script_control};
    use std::collections::{HashMap, HashSet};
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_auto_rot_slot(total_frames: f32, keys: Vec<ModelAutoRotKey>) -> SpriteSlot {
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
            model: None,
            model_draw: ModelDrawState::default(),
            model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
            model_effect: Default::default(),
            model_auto_rot_total_frames: total_frames,
            model_auto_rot_z_keys: Arc::from(keys),
        }
    }

    fn temp_noteskin_root(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-noteskin-mod-{name}-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_noteskin_png(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]))
            .save(path)
            .unwrap();
        itg_register_texture_dims_for_path(path);
    }

    #[test]
    fn compiled_bundle_path_omits_version_dir() {
        let path = compiled_bundle_path(" Dance ", " Default ", "hash123");
        let suffix = Path::new("noteskins")
            .join("dance")
            .join("default")
            .join("hash123.bin");
        let version_dir = format!("v{}", noteskin_compiled::CACHE_SCHEMA_VERSION);
        assert!(path.ends_with(&suffix));
        assert!(
            path.components()
                .all(|component| component.as_os_str() != OsStr::new(&version_dir))
        );
    }

    #[test]
    fn itg_texture_key_preserves_absolute_external_paths() {
        let root = temp_noteskin_root("absolute-texture-key");
        let texture = root.join("Tap Note parts (mipmaps).png");
        write_noteskin_png(&texture);

        let key = itg_texture_key(&texture).unwrap();
        assert!(
            Path::new(&key).is_absolute(),
            "external model texture keys must stay absolute; got {key}"
        );
        assert!(
            Path::new(&key).is_file(),
            "absolute texture key should still resolve to the source file"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clear_itg_runtime_caches_reloads_data_cache() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("data-cache");
        let skin_dir = root.join("dance/hot");
        fs::create_dir_all(&skin_dir).unwrap();
        let metrics = skin_dir.join("metrics.ini");
        fs::write(
            &metrics,
            "[Global]\nFallbackNoteSkin=hot\n[Down]\nFoo=old\n",
        )
        .unwrap();

        let loaded = load_itg_data_cached(&root, "dance", "hot").unwrap();
        assert_eq!(loaded.get_metric("Down", "Foo"), Some("old"));

        fs::write(
            &metrics,
            "[Global]\nFallbackNoteSkin=hot\n[Down]\nFoo=new\n",
        )
        .unwrap();
        let stale = load_itg_data_cached(&root, "dance", "hot").unwrap();
        assert_eq!(
            stale.get_metric("Down", "Foo"),
            Some("old"),
            "loaded noteskin data should stay cached until refresh"
        );

        clear_itg_runtime_caches();
        let refreshed = load_itg_data_cached(&root, "dance", "hot").unwrap();
        assert_eq!(refreshed.get_metric("Down", "Foo"), Some("new"));
        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn actor_mod_parser_supports_vertalign_and_glow() {
        let mut commands = HashMap::new();
        commands.insert(
            "initcommand".to_string(),
            "SetTextureFiltering,false;vertalign,bottom;glow,0.1,0.2,0.3,0.4".to_string(),
        );
        let (draw, timeline, effect) = model_draw_program(&commands);
        assert!(parse_script_control("settexturefiltering").is_some());
        assert!(timeline.is_empty(), "expected no tween timeline");
        assert!(
            (draw.vert_align - 1.0).abs() <= f32::EPSILON,
            "vertalign,bottom should map to 1.0"
        );
        assert!(
            (draw.glow[0] - 0.1).abs() <= 1e-6
                && (draw.glow[1] - 0.2).abs() <= 1e-6
                && (draw.glow[2] - 0.3).abs() <= 1e-6
                && (draw.glow[3] - 0.4).abs() <= 1e-6,
            "glow command should populate base glow color; got {:?}",
            draw.glow
        );
        assert!(
            matches!(effect.mode, ModelEffectMode::None),
            "plain actor mods should not set an effect mode"
        );
    }

    #[test]
    fn loads_default_and_cel_itg_noteskins() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        assert!(load_itg_skin(&style, "default").is_ok());
        assert!(load_itg_skin(&style, "cel").is_ok());
    }

    #[test]
    fn cel_exposes_model_and_uv_motion() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(!ns.notes.is_empty());
        assert!(ns.notes.iter().any(|slot| slot.model.is_some()));
        assert!(ns.notes.iter().any(|slot| {
            slot.uv_velocity[0].abs() > f32::EPSILON || slot.uv_velocity[1].abs() > f32::EPSILON
        }));
    }

    #[test]
    fn shared_background_arrow_model_loads_with_texture_scroll() {
        let slots = load_itg_model_slots_from_path(Path::new(
            "assets/graphics/menu_bg_technique/arrow_model.txt",
        ))
        .expect("technique arrow model should load");
        assert_eq!(slots.len(), 1, "expected one arrow model layer");
        let slot = &slots[0];
        assert!(
            slot.model.is_some(),
            "shared model slot should contain geometry"
        );
        assert_eq!(
            slot.texture_key(),
            "graphics/menu_bg_technique/arrow_tex.png"
        );
        assert!(
            slot.uv_velocity[1] < -0.9 && slot.uv_velocity[1] > -1.1,
            "expected AnimatedTexture TexVelocityY to carry through, got {:?}",
            slot.uv_velocity
        );
        assert_eq!(slot.uv_cycle_seconds, Some(10.0));
    }

    #[test]
    fn shared_background_arrow_model_uv_scroll_uses_animation_cycle() {
        let slots = load_itg_model_slots_from_path(Path::new(
            "assets/graphics/menu_bg_technique/arrow_model.txt",
        ))
        .expect("technique arrow model should load");
        let slot = &slots[0];
        let uv_0 = slot.uv_for_frame_at(0, 0.0);
        let uv_5 = slot.uv_for_frame_at(0, 5.0);
        let uv_10 = slot.uv_for_frame_at(0, 10.0);
        assert!(
            (uv_5[1] - (uv_0[1] - 0.5)).abs() <= 1e-6 && (uv_5[3] - (uv_0[3] - 0.5)).abs() <= 1e-6,
            "expected half-cycle UV shift after 5 seconds, got {uv_0:?} -> {uv_5:?}"
        );
        assert!(
            (uv_10[1] - uv_0[1]).abs() <= 1e-6 && (uv_10[3] - uv_0[3]).abs() <= 1e-6,
            "expected UVs to wrap after one 10-second cycle, got {uv_0:?} -> {uv_10:?}"
        );
    }

    #[test]
    fn model_material_paths_accept_windows_separators() {
        let root = temp_noteskin_root("model-paths");
        let texture_dir = root.join("textures");
        fs::create_dir_all(&texture_dir).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .save(texture_dir.join("Tap Note parts.png"))
            .unwrap();
        fs::write(
            texture_dir.join("Tap Note parts.ini"),
            "[AnimatedTexture]\nTexVelocityY=-1\nFrame0000=Tap Note parts.png\nDelay0000=1.0\n",
        )
        .unwrap();

        let model_path = root.join("_down tap note model.txt");
        fs::write(
            &model_path,
            r#"MilkShape 3D ASCII
Meshes: 1
"mesh" 0 0
3
0 -1.0 -1.0 0.0 0.0 0.0 -1
0 1.0 -1.0 0.0 1.0 0.0 -1
0 0.0 1.0 0.0 0.0 1.0 -1
0
1
0 0 1 2 0 0 0 1
Materials: 1
"mat"
0.0 0.0 0.0 1.0
1.0 1.0 1.0 1.0
0.0 0.0 0.0 1.0
0.0 0.0 0.0 1.0
0.0
1.0
"textures\Tap Note parts.ini"
""
"#,
        )
        .unwrap();

        let slots = load_itg_model_slots_from_path(&model_path)
            .expect("model should resolve backslash material texture path");
        let slot = slots.first().expect("expected one model-backed slot");
        assert!(slot.model.is_some());
        assert!(
            slot.texture_key()
                .replace('\\', "/")
                .ends_with("textures/Tap Note parts.png")
        );
        assert_eq!(slot.uv_velocity, [0.0, -1.0]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cel_model_tap_note_uses_multiple_material_layers() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("cel should expose at least one tap note layer set");
        let model_layers = layers
            .iter()
            .filter(|slot| slot.model.is_some())
            .collect::<Vec<_>>();
        assert!(
            model_layers.len() >= 2,
            "expected cel tap note model to expose multiple material layers; got {}",
            model_layers.len()
        );
        let textures = model_layers
            .iter()
            .map(|slot| slot.texture_key().to_string())
            .collect::<HashSet<_>>();
        assert!(
            textures.contains("noteskins/dance/cel/textures/Tap Note parts (mipmaps).png"),
            "expected cel model tap note layers to resolve Tap Note parts texture; got {:?}",
            textures
        );
    }

    #[test]
    fn cel_model_tap_note_honors_nomove_material() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("cel should expose at least one tap note layer set");
        let mut saw_model = false;
        let mut saw_moving = false;
        let mut saw_nomove = false;
        for layer in layers.iter().filter(|slot| slot.model.is_some()) {
            saw_model = true;
            let moving = layer.uv_velocity[0].abs() > f32::EPSILON
                || layer.uv_velocity[1].abs() > f32::EPSILON;
            if moving {
                saw_moving = true;
            } else {
                saw_nomove = true;
            }
        }
        assert!(
            saw_model,
            "expected at least one model-backed tap-note layer"
        );
        assert!(
            saw_moving,
            "expected at least one scrolling model material in cel tap note"
        );
        assert!(
            saw_nomove,
            "expected at least one nomove model material in cel tap note"
        );
    }

    #[test]
    fn default_exposes_multi_layer_tap_note() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert_eq!(ns.notes.len(), ns.note_layers.len());
        assert!(ns.note_layers.iter().any(|layers| layers.len() > 1));
        let q4_layers = ns
            .note_layers
            .first()
            .expect("default should expose 4th-note tap layers");
        assert_eq!(
            q4_layers.len(),
            5,
            "default tap note should have arrow + four circles"
        );
        let circle_layers = q4_layers
            .iter()
            .filter(|slot| slot.texture_key().to_ascii_lowercase().contains("_circle"))
            .count();
        assert_eq!(
            circle_layers, 4,
            "default tap note should keep four circle layers"
        );
    }

    #[test]
    fn default_exposes_lift_layers_for_each_quantization() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert_eq!(ns.lift_note_layers.len(), ns.note_layers.len());
        assert!(ns.lift_note_layers.iter().all(|layers| !layers.is_empty()));
    }

    #[test]
    fn lambda_tap_note_uses_source_size_hints() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "lambda")
            .expect("dance/lambda should load from assets/noteskins");
        let q4_layers = ns
            .note_layers
            .first()
            .expect("lambda should expose 4th-note tap layers");
        assert_eq!(
            q4_layers.len(),
            5,
            "lambda tap note should have arrow + four circles"
        );
        let arrow = q4_layers
            .first()
            .expect("lambda should expose primary arrow layer");
        assert_eq!(
            arrow.logical_size(),
            [64.0, 64.0],
            "lambda arrow logical size should use '(res 64x512)' source frame dimensions"
        );
        let circle = q4_layers
            .iter()
            .find(|slot| slot.texture_key().to_ascii_lowercase().contains("_circle"))
            .expect("lambda should expose circle layers");
        assert_eq!(
            circle.logical_size(),
            [16.0, 16.0],
            "lambda circle logical size should honor '(doubleres)' source dimensions"
        );
    }

    #[test]
    fn default_receptor_overlay_press_and_lift_behavior_is_parsed() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let behavior = ns.receptor_glow_behavior;

        assert!(
            (behavior.press_duration - 0.2).abs() <= 1e-6,
            "default press command duration should be 0.2s"
        );
        assert!(
            (behavior.press_alpha_start - 0.8).abs() <= 1e-6,
            "default press command should start overlay alpha at 0.8"
        );
        assert!(
            (behavior.press_alpha_end - 0.4).abs() <= 1e-6,
            "default press command should settle overlay alpha at 0.4 while held"
        );
        assert!(
            (behavior.duration - 0.2).abs() <= 1e-6,
            "default lift command duration should be 0.2s"
        );
        assert!(
            behavior.alpha_end.abs() <= 1e-6,
            "default lift command should fade overlay alpha to 0"
        );

        let (press_start_alpha, press_start_zoom) = behavior.sample_press(behavior.press_duration);
        let (press_end_alpha, press_end_zoom) = behavior.sample_press(0.0);
        assert!((press_start_alpha - behavior.press_alpha_start).abs() <= 1e-6);
        assert!((press_end_alpha - behavior.press_alpha_end).abs() <= 1e-6);
        assert!((press_start_zoom - behavior.press_zoom_start).abs() <= 1e-6);
        assert!((press_end_zoom - behavior.press_zoom_end).abs() <= 1e-6);

        let (lift_start_alpha, lift_start_zoom) = behavior.sample_lift(
            behavior.duration,
            behavior.press_alpha_end,
            behavior.press_zoom_end,
        );
        let (lift_end_alpha, lift_end_zoom) =
            behavior.sample_lift(0.0, behavior.press_alpha_end, behavior.press_zoom_end);
        assert!((lift_start_alpha - behavior.press_alpha_end).abs() <= 1e-6);
        assert!((lift_start_zoom - behavior.press_zoom_end).abs() <= 1e-6);
        assert!((lift_end_alpha - behavior.alpha_end).abs() <= 1e-6);
        assert!((lift_end_zoom - behavior.zoom_end).abs() <= 1e-6);
    }

    #[test]
    fn default_receptor_overlay_keeps_source_size_ratio() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");

        let receptor = ns
            .receptor_off
            .first()
            .expect("dance/default should resolve receptor sprite");
        let overlay = ns
            .receptor_glow
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("dance/default should resolve receptor overlay sprite");

        assert_eq!(
            receptor.logical_size(),
            [64.0, 64.0],
            "default receptor should use logical source-frame size"
        );
        assert_eq!(
            overlay.logical_size(),
            [74.0, 74.0],
            "default overlay should preserve larger source-frame size than receptor"
        );
    }

    #[test]
    fn howdy_receptor_none_command_keeps_init_zoom_static() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "howdy").expect("dance/howdy should load from assets/noteskins");
        let receptor = ns
            .receptor_off
            .first()
            .expect("dance/howdy should resolve receptor sprite");
        assert!(
            (receptor.model_draw.zoom[0] - 0.8).abs() <= 1e-6,
            "howdy receptor InitCommand should set base zoom to 0.8"
        );

        let behavior = ns.receptor_step_behavior_for_col(0, None);
        assert_eq!(behavior.duration, 0.0);
        assert!(
            (behavior.sample_zoom(0.8) - 1.0).abs() <= 1e-6,
            "howdy constant-size NoneCommand should not start a shrink/return pulse"
        );
    }

    #[test]
    fn lua_function_receptor_none_command_parses_zoom_pulse() {
        let effect = itg_parse_command_effect(
            "function(self) self:finishtweening():zoom(0.75):linear(0.11):zoom(1.0)end",
        );

        assert!((effect.duration - 0.11).abs() <= 1e-6);
        assert!((effect.start_zoom.unwrap_or_default() - 0.75).abs() <= 1e-6);
        assert!((effect.target_zoom.unwrap_or_default() - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn lua_function_explosion_command_parses_expression_duration_and_blend() {
        let anim = parse_explosion_animation(
            "function (self) self:finishtweening():diffusealpha(1.0):blend(Blend.Add):linear(12/60):diffusealpha(0.0) end",
        );

        assert!(anim.blend_add);
        assert!((anim.initial.color[3] - 1.0).abs() <= 1e-6);
        assert_eq!(anim.segments.len(), 1);
        assert!((anim.segments[0].duration - 0.2).abs() <= 1e-6);
        assert_eq!(anim.segments[0].end_color.map(|c| c[3]), Some(0.0));
    }

    #[test]
    fn explosion_finishtweening_cancels_same_frame_preflash() {
        let anim = parse_explosion_animation(
            "diffusealpha,1;linear,0.2;diffusealpha,0;finishtweening;diffusealpha,1;linear,0.1;diffusealpha,0",
        );

        assert_eq!(anim.segments.len(), 1);
        assert!((anim.duration() - 0.1).abs() <= 1e-6);
        assert!((anim.state_at(0.05).diffuse[3] - 0.5).abs() <= 1e-6);

        let canceled =
            parse_explosion_animation("diffusealpha,1;linear,0.2;diffusealpha,0;finishtweening");
        assert_eq!(canceled.duration(), 0.0);
        assert_eq!(canceled.state_at(0.0).diffuse[3], 0.0);
    }

    #[test]
    fn cf_chrome_family_receptor_none_command_drives_empty_press_pulse() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "CF_VIBRANTALLOY")
            .expect("CF_VIBRANTALLOY should load from assets/noteskins");
        let behavior = ns.receptor_step_behavior_for_col(0, None);

        assert!((behavior.duration - 0.11).abs() <= 1e-6);
        assert!((behavior.sample_zoom(behavior.duration) - 0.75).abs() <= 1e-6);
        assert!((behavior.sample_zoom(0.0) - 1.0).abs() <= 1e-6);

        clear_itg_runtime_caches();
    }

    #[test]
    fn devcel_receptor_hit_commands_do_not_use_none_zoom_pulse() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "devcel-2024")
            .expect("dance/devcel-2024 should load from assets/noteskins");
        let none = ns.receptor_step_behavior_for_col(0, None);
        let w1 = ns.receptor_step_behavior_for_col(0, Some("W1"));
        let w3 = ns.receptor_step_behavior_for_col(0, Some("W3"));

        assert!(none.duration > 0.0);
        assert!((none.sample_zoom(none.duration) - 0.75).abs() <= 1e-6);
        assert_eq!(w1.duration, 0.0);
        assert_eq!(w3.duration, 0.0);
        assert!(w1.interrupts);
        assert!(w3.interrupts);
        assert!((w1.sample_zoom(0.0) - 1.0).abs() <= 1e-6);
        assert!((w3.sample_zoom(0.0) - 1.0).abs() <= 1e-6);

        clear_itg_runtime_caches();
    }

    fn assert_devcel_roll_active_sequence(slot: &SpriteSlot, label: &str) {
        let SpriteSource::Animated {
            frame_count,
            frame_indices,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("{label} should be animated");
        };

        assert_eq!(*frame_count, 6, "{label} should keep every ITG state");
        assert_eq!(
            frame_indices.as_deref(),
            Some([0, 1, 2, 3, 2, 1].as_slice()),
            "{label} should preserve repeated texture frames"
        );
        let delays = frame_durations
            .as_deref()
            .unwrap_or_else(|| panic!("{label} should preserve state delays"));
        assert_eq!(delays, [0.44, 0.03, 0.03, 0.44, 0.03, 0.03]);
        assert!((delays.iter().sum::<f32>() - 1.0).abs() <= 1e-6);
        assert_eq!(slot.frame_index_from_phase(0.955), 4);
        assert_eq!(slot.uv_for_frame_at(2, 0.0), slot.uv_for_frame_at(4, 0.0));
        assert_eq!(slot.uv_for_frame_at(1, 0.0), slot.uv_for_frame_at(5, 0.0));
    }

    #[test]
    fn devcel_roll_active_preserves_repeated_frame_states() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "devcel-2024")
            .expect("dance/devcel-2024 should load from assets/noteskins");
        let body = ns
            .roll
            .body_active
            .as_ref()
            .expect("devcel roll body active should resolve");
        let bottom = ns
            .roll
            .bottomcap_active
            .as_ref()
            .expect("devcel roll bottomcap active should resolve");

        assert_devcel_roll_active_sequence(body, "devcel roll body active");
        assert_devcel_roll_active_sequence(bottom, "devcel roll bottomcap active");

        clear_itg_runtime_caches();
    }

    #[test]
    fn receptor_pulse_uses_actor_init_command_not_fallback_metric() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("receptor-init-command");
        let skin_dir = root.join("dance/steady");
        let common_dir = root.join("common/common");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::create_dir_all(&common_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n[ReceptorArrow]\nNoneCommand=\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }

function skin.Load()
    local button = skin.ButtonRedir[Var "Button"] or Var "Button"
    return LoadActor(NOTESKIN:GetPath(button, Var "Element"))
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Receptor.lua"),
            r#"local t = Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath("_down", "go receptor");
        Frame0000=0;
        Delay0000=0;
        NoneCommand=NOTESKIN:GetMetricA("ReceptorArrow", "NoneCommand");
    };
};
return t
"#,
        )
        .unwrap();
        fs::write(
            common_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n[ReceptorArrow]\nInitCommand=effectclock,'beat';diffuseramp;effectcolor1,color(\"0,0,0,1\");effectcolor2,color(\"1,1,1,1\");effecttiming,.5,0,.5,0\n",
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("_down go receptor.png"));

        let data = load_itg_data_cached(&root, "dance", "steady")
            .expect("steady test noteskin data should load");
        assert!(
            data.metrics
                .get("ReceptorArrow", "InitCommand")
                .is_some_and(|cmd| cmd.contains("diffuseramp")),
            "test skin should inherit a pulsing fallback metric"
        );

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg(&root, "dance", "steady", &style).expect("steady test noteskin should load");
        let receptor = ns
            .receptor_off
            .first()
            .expect("steady test noteskin should resolve a receptor");
        assert_eq!(
            receptor.source.frame_count(),
            1,
            "Frame0000-only receptor actor should stay on a single frame"
        );
        for beat in [0.0, 0.25, 0.5, 0.75] {
            let color = ns.receptor_pulse.color_for_beat(beat);
            assert!(
                color.iter().all(|channel| (*channel - 1.0).abs() <= 1e-6),
                "receptor pulse should ignore fallback InitCommand at beat {beat}, got {color:?}"
            );
        }

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn receptor_pulse_effecttiming_recalculates_period() {
        let pulse = itg_receptor_pulse_from_script(
            "effectclock,'beat';diffuseramp;effectcolor1,0.1,0.1,0.1,1;\
             effectcolor2,1,1,1,1;effectperiod,0.5;\
             effecttiming,0.25,0.50,0,0.25;effectoffset,-0.25",
        );

        assert!(
            (pulse.effect_period - 1.0).abs() <= 1e-6,
            "ITG SetEffectTiming should replace the prior effectperiod"
        );
        let beat_0 = pulse.color_for_beat(0.0);
        let beat_half = pulse.color_for_beat(0.5);
        let beat_1 = pulse.color_for_beat(1.0);
        assert!(
            (beat_0[0] - beat_1[0]).abs() <= 1e-6,
            "one full cycle should take one beat"
        );
        assert!(
            (beat_0[0] - beat_half[0]).abs() > 0.2,
            "half a beat should not complete the cycle; got {beat_0:?} and {beat_half:?}"
        );
    }

    #[test]
    fn loader_init_command_applies_to_resolved_receptor() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("loader-init-command");
        let skin_dir = root.join("dance/mirror");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=mirror\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    if element == "Receptor" and button == "Right" then
        local t = LoadActor(NOTESKIN:GetPath("Left", "Receptor"))
        t.InitCommand=function(self) self:SetTextureFiltering(false); self:y(1); self:zoomx(-1); end
        return t
    end
    if element == "Receptor" and button == "Left" then
        return LoadActor(NOTESKIN:GetPath("Left", "Receptor"))
    end
    return LoadActor(NOTESKIN:GetPath("Down", element))
end

return skin
"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));
        write_noteskin_png(&skin_dir.join("Left Receptor.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg(&root, "dance", "mirror", &style).expect("mirror test noteskin should load");
        let left = ns
            .receptor_off
            .first()
            .expect("left receptor should resolve");
        let right = ns
            .receptor_off
            .get(3)
            .expect("right receptor should resolve");

        assert!(!left.def.mirror_h);
        assert!(right.def.mirror_h);
        assert!((right.model_draw.pos[1] - 1.0).abs() <= f32::EPSILON);
        let uv = right.uv_for_frame_at(0, 0.0);
        assert!(
            uv[0] < uv[2],
            "mirroring stays as actor scale, not a reversed UV rect; got {uv:?}"
        );

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn explosion_children_keep_per_button_rotation() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("explosion-child-rotation");
        let skin_dir = root.join("dance/ghostrot");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=ghostrot\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }
skin.PartsToRotate = { ["Tap Explosion Dim W1"] = true, ["Hold Explosion"] = true }
skin.Rotate = { Up = 180, Down = 0, Left = 90, Right = -90 }

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local load_button = skin.ButtonRedir[button] or button
    local path = element == "Explosion" and NOTESKIN:GetPath("", "Fallback Explosion") or NOTESKIN:GetPath(load_button, element)
    local t = LoadActor(path)
    if skin.PartsToRotate[element] then
        t.BaseRotationZ = skin.Rotate[button]
    end
    return t
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Fallback Explosion.lua"),
            r#"return Def.ActorFrame {
    NOTESKIN:LoadActor(Var "Button", "Tap Explosion Dim W1") .. {
        InitCommand=cmd(diffusealpha,0);
        W1Command=cmd(diffusealpha,1);
        JudgmentCommand=cmd(finishtweening);
        DimCommand=cmd(visible,true);
    };
    NOTESKIN:LoadActor(Var "Button", "Hold Explosion") .. {
        InitCommand=cmd(diffusealpha,0);
        HoldingOnCommand=cmd(diffusealpha,1);
    };
    NOTESKIN:LoadActor(Var "Button", "Hold Explosion") .. {
        InitCommand=cmd(diffusealpha,0);
        RollOnCommand=cmd(diffusealpha,1);
    };
}"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));
        write_noteskin_png(&skin_dir.join("Down Tap Explosion Dim W1.png"));
        write_noteskin_png(&skin_dir.join("Down Hold Explosion.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg(&root, "dance", "ghostrot", &style).expect("test noteskin should load");
        let rotations = (0..4)
            .map(|col| {
                ns.tap_explosion_for_col(col, "W1")
                    .expect("W1 explosion should resolve for each column")
                    .slot
                    .def
                    .rotation_deg
            })
            .collect::<Vec<_>>();
        assert_eq!(rotations, vec![90, 0, 180, -90]);
        let hold_rotations = (0..4)
            .map(|col| {
                ns.hold_visuals_for_col(col, false)
                    .explosion
                    .as_ref()
                    .expect("hold explosion should resolve for each column")
                    .def
                    .rotation_deg
            })
            .collect::<Vec<_>>();
        assert_eq!(hold_rotations, vec![90, 0, 180, -90]);
        let roll_rotations = (0..4)
            .map(|col| {
                ns.hold_visuals_for_col(col, true)
                    .explosion
                    .as_ref()
                    .expect("roll explosion should resolve for each column")
                    .def
                    .rotation_deg
            })
            .collect::<Vec<_>>();
        assert_eq!(roll_rotations, vec![90, 0, 180, -90]);

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn receptor_reverse_commands_are_kept_per_layer() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("receptor-reverse-command");
        let skin_dir = root.join("dance/revbar");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=revbar\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }

function skin.Load()
    local button = skin.ButtonRedir[Var "Button"] or Var "Button"
    return LoadActor(NOTESKIN:GetPath(button, Var "Element"))
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Receptor.lua"),
            r#"local t = Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath("_down", "go receptor");
        Frame0000=0;
        Delay0000=0;
        ReverseOnCommand=function(self)
            self:baserotationz(180)
        end;
        ReverseOffCommand=function(self)
            self:baserotationz(0)
        end;
    };
    Def.Sprite {
        Texture=NOTESKIN:GetPath("_down", "tap flash");
        Frame0000=0;
        Delay0000=1;
        ReverseOnCommand=function(self)
            self:baserotationz(180):vertalign("bottom")
        end;
        ReverseOffCommand=function(self)
            self:baserotationz(0):vertalign("top")
        end;
    };
};
return t
"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("_down go receptor.png"));
        write_noteskin_png(&skin_dir.join("_down tap flash.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg(&root, "dance", "revbar", &style).expect("revbar test noteskin should load");
        let off = ns
            .receptor_off_reverse
            .first()
            .copied()
            .expect("revbar should keep receptor reverse commands");
        assert_eq!(off.state(false).base_rotation_z, Some(0.0));
        assert_eq!(off.state(true).base_rotation_z, Some(180.0));

        let glow = ns
            .receptor_glow_reverse
            .first()
            .copied()
            .expect("revbar should keep receptor glow reverse commands");
        assert_eq!(glow.state(false).base_rotation_z, Some(0.0));
        assert_eq!(glow.state(false).vert_align, Some(0.0));
        assert_eq!(glow.state(true).base_rotation_z, Some(180.0));
        assert_eq!(glow.state(true).vert_align, Some(1.0));

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn default_and_cel_parse_notedisplay_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert!(
            default_ns
                .note_display_metrics
                .draw_hold_head_for_taps_on_same_row
        );
        assert!(
            default_ns
                .note_display_metrics
                .draw_roll_head_for_taps_on_same_row
        );
        assert!(
            default_ns
                .note_display_metrics
                .tap_hold_roll_on_row_means_hold
        );
        assert!(
            default_ns
                .note_display_metrics
                .flip_head_and_tail_when_reverse
        );
        assert!(default_ns.note_display_metrics.flip_hold_body_when_reverse);

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(
            !cel_ns
                .note_display_metrics
                .draw_hold_head_for_taps_on_same_row
        );
        assert!(
            !cel_ns
                .note_display_metrics
                .draw_roll_head_for_taps_on_same_row
        );
        assert!(cel_ns.note_display_metrics.flip_head_and_tail_when_reverse);
        assert!(cel_ns.note_display_metrics.flip_hold_body_when_reverse);
        assert!(cel_ns.note_display_metrics.top_hold_anchor_when_reverse);
    }

    #[test]
    fn ddr_note_and_cel_keep_distinct_reverse_hold_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ddr_note_ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        assert!(
            !ddr_note_ns
                .note_display_metrics
                .flip_head_and_tail_when_reverse
        );
        assert!(!ddr_note_ns.note_display_metrics.flip_hold_body_when_reverse);
        assert!(
            !ddr_note_ns
                .note_display_metrics
                .top_hold_anchor_when_reverse
        );

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(cel_ns.note_display_metrics.flip_head_and_tail_when_reverse);
        assert!(cel_ns.note_display_metrics.flip_hold_body_when_reverse);
        assert!(cel_ns.note_display_metrics.top_hold_anchor_when_reverse);
    }

    #[test]
    fn default_and_cel_parse_note_color_translation_metrics() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let default_tap =
            default_ns.note_display_metrics.part_texture_translate[NoteAnimPart::Tap as usize];
        assert_eq!(default_tap.note_color_count, 8);
        assert_eq!(default_tap.note_color_type, NoteColorType::Denominator);
        assert!((default_tap.note_color_spacing[1] - 0.125).abs() <= 1e-6);
        let default_tap_8th = default_ns.part_uv_translation(NoteAnimPart::Tap, 0.5, false);
        assert!(default_tap_8th[0].abs() <= f32::EPSILON);
        assert!((default_tap_8th[1] - 0.125).abs() <= 1e-6);

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let cel_roll_head =
            cel_ns.note_display_metrics.part_texture_translate[NoteAnimPart::RollHead as usize];
        assert_eq!(cel_roll_head.note_color_count, 8);
        assert_eq!(cel_roll_head.note_color_type, NoteColorType::Denominator);
        assert!((cel_roll_head.note_color_spacing[0] - 0.03125).abs() <= 1e-6);
        let cel_roll_head_8th = cel_ns.part_uv_translation(NoteAnimPart::RollHead, 0.5, false);
        assert!((cel_roll_head_8th[0] - 0.03125).abs() <= 1e-6);
        assert!(cel_roll_head_8th[1].abs() <= f32::EPSILON);
    }

    #[test]
    fn default_and_cel_resolve_hold_topcaps() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let default_visuals = default_ns.hold_visuals_for_col(0, false);
        assert!(
            default_visuals.topcap_inactive.is_none() && default_visuals.topcap_active.is_none(),
            "dance/default should honor ret.Blank and keep hold topcap visuals unresolved"
        );
        assert!(
            default_visuals.bottomcap_inactive.is_some()
                || default_visuals.bottomcap_active.is_some(),
            "dance/default should resolve hold bottomcap visuals"
        );

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let cel_visuals = cel_ns.hold_visuals_for_col(0, false);
        assert!(
            cel_visuals.topcap_inactive.is_none() && cel_visuals.topcap_active.is_none(),
            "dance/cel should honor ret.Blank and keep hold topcap visuals unresolved"
        );
        assert!(
            cel_visuals.bottomcap_inactive.is_some() || cel_visuals.bottomcap_active.is_some(),
            "dance/cel should still resolve hold bottomcap visuals"
        );
    }

    #[test]
    fn default_does_not_bake_quantization_uv_shift_into_slots() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let q4 = ns
            .note_layers
            .first()
            .and_then(|layers| layers.first())
            .expect("default should expose first 4th-note layer");
        let q8 = ns
            .note_layers
            .get(1)
            .and_then(|layers| layers.first())
            .expect("default should expose first 8th-note layer");
        assert_eq!(q4.def.src, q8.def.src);
        assert!(
            (q4.uv_offset[0] - q8.uv_offset[0]).abs() <= f32::EPSILON
                && (q4.uv_offset[1] - q8.uv_offset[1]).abs() <= f32::EPSILON
        );
    }

    #[test]
    fn ddr_vivid_parses_hold_body_offsets() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-vivid")
            .expect("dance/ddr-vivid should load from assets/noteskins");
        assert!(
            (ns.note_display_metrics
                .start_drawing_hold_body_offset_from_head
                - 0.0)
                .abs()
                <= f32::EPSILON
        );
        assert!(
            (ns.note_display_metrics
                .stop_drawing_hold_body_offset_from_tail
                + 32.0)
                .abs()
                <= 1e-6
        );
        assert!((ns.note_display_metrics.hold_let_go_gray_percent - 0.33).abs() <= 1e-6);
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::HoldBody as usize].length - 4.0)
                .abs()
                <= 1e-6
        );
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::RollBody as usize].length - 2.0)
                .abs()
                <= 1e-6
        );
        assert!(
            !ns.note_display_metrics.part_animation[NoteAnimPart::HoldBody as usize].vivid
                && !ns.note_display_metrics.part_animation[NoteAnimPart::RollBody as usize].vivid
        );
    }

    #[test]
    fn vivid_zero_spacing_keeps_model_uv_offsets_across_quants() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "vivid").expect("dance/vivid should load from assets/noteskins");
        let q4 = ns
            .note_layers
            .first()
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("vivid should expose model-backed tap note layer for 4th notes");
        let q8 = ns
            .note_layers
            .get(1)
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("vivid should expose model-backed tap note layer for 8th notes");
        assert!(
            (q4.uv_offset[0] - q8.uv_offset[0]).abs() <= f32::EPSILON,
            "vivid should not force note-color X offset when spacing metrics are zero"
        );
        assert!(
            (q4.uv_offset[1] - q8.uv_offset[1]).abs() <= f32::EPSILON,
            "vivid should not force note-color Y offset when spacing metrics are zero"
        );
    }

    #[test]
    fn vivid_tap_note_honors_vertex_tex_matrix_scale_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "vivid").expect("dance/vivid should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("vivid should expose at least one tap note layer set");
        let mut saw_static_uv_vertex = false;
        let mut saw_scrolling_uv_vertex = false;
        for layer in layers.iter().filter_map(|slot| slot.model.as_ref()) {
            for vertex in layer.vertices.iter() {
                let sx = vertex.tex_matrix_scale[0];
                let sy = vertex.tex_matrix_scale[1];
                if sx < 0.5 || sy < 0.5 {
                    saw_static_uv_vertex = true;
                } else {
                    saw_scrolling_uv_vertex = true;
                }
            }
        }
        assert!(
            saw_static_uv_vertex,
            "vivid tap note should include vertices that ignore texture-matrix scroll"
        );
        assert!(
            saw_scrolling_uv_vertex,
            "vivid tap note should include vertices that follow texture-matrix scroll"
        );
    }

    #[test]
    fn ddr_note_receptor_uses_beat_clock_with_mixed_delays() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let slot = ns
            .receptor_off
            .first()
            .expect("ddr-note should define receptor_off for first column");
        let SpriteSource::Animated {
            rate,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("ddr-note receptor should resolve to animated sprite");
        };
        assert!(
            matches!(rate, AnimationRate::FramesPerBeat(_)),
            "ddr-note receptor expected beat clock animation, got {rate:?}"
        );
        let delays = frame_durations
            .as_ref()
            .expect("ddr-note receptor should preserve per-frame delays");
        assert!(
            delays.len() >= 2,
            "expected at least 2 receptor delays, got {:?}",
            delays
        );
        assert!(
            (delays[0] - 0.2).abs() < 0.01,
            "expected first frame delay near 0.2 beat, got {}",
            delays[0]
        );
        assert!(
            (delays[1] - 0.8).abs() < 0.01,
            "expected second frame delay near 0.8 beat, got {}",
            delays[1]
        );
        assert_eq!(slot.frame_index(0.0, 0.00), 0);
        assert_eq!(slot.frame_index(0.0, 0.19), 0);
        assert_eq!(slot.frame_index(0.0, 0.25), 1);
        assert_eq!(slot.frame_index(0.0, 0.95), 1);
        assert_eq!(slot.frame_index(0.0, 1.05), 0);
    }

    #[test]
    fn ddr_note_receptor_phase_index_uses_weighted_delays() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let slot = ns
            .receptor_off
            .first()
            .expect("ddr-note should define receptor_off for first column");
        assert_eq!(slot.frame_index_from_phase(0.00), 0);
        assert_eq!(slot.frame_index_from_phase(0.19), 0);
        assert_eq!(slot.frame_index_from_phase(0.20), 1);
        assert_eq!(slot.frame_index_from_phase(0.95), 1);
        assert_eq!(slot.frame_index_from_phase(1.05), 0);
        assert_eq!(slot.frame_index_from_phase(-0.05), 1);
    }

    #[test]
    fn ddr_note_hold_body_and_cap_use_per_column_assets() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");

        let expected = [
            ("left hold body inactive", "left hold bottomcap inactive"),
            ("down hold body inactive", "down hold bottomcap inactive"),
            ("up hold body inactive", "up hold bottomcap inactive"),
            ("right hold body inactive", "right hold bottomcap inactive"),
        ];

        for (col, (want_body, want_cap)) in expected.into_iter().enumerate() {
            let visuals = ns.hold_visuals_for_col(col, false);
            let body = visuals
                .body_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold body inactive per column");
            let cap = visuals
                .bottomcap_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold bottomcap inactive per column");
            assert!(
                body.contains(want_body),
                "column {col} expected body containing '{want_body}', got '{body}'"
            );
            assert!(
                cap.contains(want_cap),
                "column {col} expected cap containing '{want_cap}', got '{cap}'"
            );
        }
    }

    #[test]
    fn ddr_note_hold_head_uses_down_hold_head_sheet() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");

        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            let inactive = visuals
                .head_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold head inactive");
            let active = visuals
                .head_active
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold head active");
            assert!(
                inactive.contains("down hold head inactive"),
                "column {col} expected Down hold head inactive sheet, got '{inactive}'"
            );
            assert!(
                active.contains("down hold head active"),
                "column {col} expected Down hold head active sheet, got '{active}'"
            );
        }
    }

    #[test]
    fn multi_layer_hold_heads_keep_model_layers() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("multi-layer-hold-head");
        let skin_dir = root.join("dance/multilayer");
        fs::create_dir_all(skin_dir.join("textures")).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=multilayer\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local ret = ... or {}
ret.Redir = function(sButton, sElement)
    return "Down", sElement
end
ret.Load = function()
    local button, element = ret.Redir(Var "Button", Var "Element")
    return LoadActor(NOTESKIN:GetPath(button, element))
end
return ret
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Tap Note.lua"),
            r#"return Def.Model {
    Meshes=NOTESKIN:GetPath('_down','tap note model');
    Materials=NOTESKIN:GetPath('_down','tap note model');
    Bones=NOTESKIN:GetPath('_down','tap note model');
};
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Hold Head Inactive.lua"),
            r#"return Def.Model {
    Meshes=NOTESKIN:GetPath('_down','tap note model');
    Materials=NOTESKIN:GetPath('_down','tap note model');
    Bones=NOTESKIN:GetPath('_down','tap note model');
};
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("_down tap note model.txt"),
            r#"MilkShape 3D ASCII
Meshes: 2
"fill" 0 0
3
0 -1.0 -1.0 0.0 0.0 0.0 -1
0 1.0 -1.0 0.0 1.0 0.0 -1
0 0.0 1.0 0.0 0.0 1.0 -1
0
1
0 0 1 2 0 0 0 1
"frame" 0 1
3
0 -1.0 -1.0 0.0 0.0 0.0 -1
0 1.0 -1.0 0.0 1.0 0.0 -1
0 0.0 1.0 0.0 0.0 1.0 -1
0
1
0 0 1 2 0 0 0 1
Materials: 2
"fill_mat"
0.0 0.0 0.0 1.0
1.0 1.0 1.0 1.0
0.0 0.0 0.0 1.0
0.0 0.0 0.0 1.0
0.0
1.0
"textures/fill.png"
""
"frame_mat"
0.0 0.0 0.0 1.0
1.0 1.0 1.0 1.0
0.0 0.0 0.0 1.0
0.0 0.0 0.0 1.0
0.0
1.0
"textures/frame.png"
""
"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("textures/fill.png"));
        write_noteskin_png(&skin_dir.join("textures/frame.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg(&root, "dance", "multilayer", &style)
            .expect("temp multilayer noteskin should load");

        for col in 0..style.num_cols {
            let note_idx = col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
            let tap_keys = ns.note_layers[note_idx]
                .iter()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .collect::<Vec<_>>();
            let visuals = ns.hold_visuals_for_col(col, false);
            let head_layers = visuals
                .head_inactive_layers
                .as_deref()
                .expect("hold heads should keep all model layers");
            let head_keys = head_layers
                .iter()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .collect::<Vec<_>>();

            assert_eq!(
                head_keys, tap_keys,
                "column {col} hold head should use the full tap-note model layer stack"
            );
            assert!(
                head_keys.iter().any(|key| key.contains("fill.png")),
                "column {col} hold head is missing the fill layer: {head_keys:?}"
            );
            assert!(
                head_keys.iter().any(|key| key.contains("frame.png")),
                "column {col} hold head is missing the frame layer: {head_keys:?}"
            );
        }

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn default_skin_blanks_hold_and_roll_explosion() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert!(
            ns.hold.explosion.is_none(),
            "default hold explosion should stay blank per NoteSkin.lua"
        );
        assert!(
            ns.roll.explosion.is_none(),
            "default roll explosion should stay blank per NoteSkin.lua"
        );
        for col in 0..style.num_cols {
            let hold_visuals = ns.hold_visuals_for_col(col, false);
            let roll_visuals = ns.hold_visuals_for_col(col, true);
            assert!(
                hold_visuals.explosion.is_none(),
                "default hold visuals should not resolve explosion for col {col}"
            );
            assert!(
                roll_visuals.explosion.is_none(),
                "default roll visuals should not resolve explosion for col {col}"
            );
        }
    }

    #[test]
    fn default_mine_hit_explosion_comes_from_noteskin_actor_and_commands() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let mine = ns
            .mine_hit_explosion
            .as_ref()
            .expect("default should resolve HitMine Explosion actor");
        let key = mine.slot.texture_key().to_ascii_lowercase();
        assert!(
            key.contains("noteskins/common/common/fallback hitmine explosion"),
            "default mine hit explosion should resolve to common fallback actor texture, got '{key}'"
        );
        assert!(
            (mine.animation.duration() - 0.6).abs() <= 1e-6,
            "default mine hit explosion should follow HitMineCommand duration (0.6s), got {}",
            mine.animation.duration()
        );
    }

    #[test]
    fn blank_tap_explosions_do_not_fall_back_to_common() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("blank-tap-explosion");
        let skin_dir = root.join("dance/blanktap");
        let common_dir = root.join("common/common");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::create_dir_all(&common_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }
skin.ElementRedir = { ["Tap Explosion Dim"] = "Tap Explosion Bright" }
skin.Blank = { ["Tap Explosion Bright"] = true, ["Tap Explosion Dim"] = true }

function skin.Load()
    local button = skin.ButtonRedir[Var "Button"] or Var "Button"
    local element = skin.ElementRedir[Var "Element"] or Var "Element"
    local t = LoadActor(NOTESKIN:GetPath(button, element))
    if skin.Blank[Var "Element"] then
        t = Def.Actor {}
        if Var "SpriteOnly" then
            t = LoadActor(NOTESKIN:GetPath("", "_blank"))
        end
    end
    return t
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            common_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n",
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));
        write_noteskin_png(&common_dir.join("Fallback Tap Explosion Dim.png"));
        write_noteskin_png(&common_dir.join("Fallback Tap Explosion Bright.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg(&root, "dance", "blanktap", &style)
            .expect("blanktap test noteskin should load");
        assert!(
            ns.tap_explosions.is_empty(),
            "blank tap explosions should not leak common fallback sprites: {:?}",
            ns.tap_explosions.keys().collect::<Vec<_>>()
        );

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn cel_hold_heads_remap_to_tap_layers() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            assert!(
                visuals.head_inactive.is_none() && visuals.head_active.is_none(),
                "cel hold heads should use tap-note fallback layers, got inactive={:?} active={:?}",
                visuals
                    .head_inactive
                    .as_ref()
                    .map(|slot| slot.texture_key().to_string()),
                visuals
                    .head_active
                    .as_ref()
                    .map(|slot| slot.texture_key().to_string())
            );
        }
    }

    #[test]
    fn cel_hold_body_resolves_for_all_columns() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            let body = visuals
                .body_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("cel should provide hold body inactive for each column");
            assert!(
                body.contains("down hold body inactive"),
                "column {col} expected down hold body inactive, got '{body}'"
            );
        }
    }

    #[test]
    fn enchantment_tap_note_uses_linear_frames_animation() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "enchantment")
            .expect("dance/enchantment should load from assets/noteskins");
        let idx = 2 * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
        let slot = ns
            .note_layers
            .get(idx)
            .and_then(|layers| layers.first())
            .expect("enchantment should expose first tap note layer for 4th quant");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("enchantment tap note should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 16,
            "enchantment tap note should use 16 linear frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("enchantment tap note should preserve linear frame delays");
        assert_eq!(delays.len(), 16, "expected one delay per linear frame");
        assert!(
            (delays[0] - 0.0625).abs() < 1e-4,
            "expected linear frame delay 1/16 beat, got {}",
            delays[0]
        );
        assert_eq!(slot.frame_index(0.0, 0.00), 0);
        assert_eq!(slot.frame_index(0.0, 0.06), 0);
        assert_eq!(slot.frame_index(0.0, 0.07), 1);
        assert_eq!(slot.frame_index(0.0, 1.01), 0);
    }

    #[test]
    fn enchantment_tap_mine_uses_linear_frames_animation() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "enchantment")
            .expect("dance/enchantment should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("enchantment should define first-column mine slot");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            ..
        } = mine.source.as_ref()
        else {
            panic!("enchantment mine should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 8,
            "enchantment mine should use 8 linear frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("enchantment mine should preserve linear frame delays");
        assert_eq!(delays.len(), 8, "expected one delay per mine frame");
        assert!(
            (delays[0] - 0.125).abs() < 1e-4,
            "expected linear frame delay 1/8 beat, got {}",
            delays[0]
        );
        assert_eq!(mine.frame_index(0.0, 0.00), 0);
        assert_eq!(mine.frame_index(0.0, 0.12), 0);
        assert_eq!(mine.frame_index(0.0, 0.13), 1);
        assert_eq!(mine.frame_index(0.0, 1.01), 0);
    }

    #[test]
    fn ddr_vivid_hold_explosion_uses_four_animated_frames() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-vivid")
            .expect("dance/ddr-vivid should load from assets/noteskins");
        let hold = ns
            .hold
            .explosion
            .as_ref()
            .expect("ddr-vivid should define hold explosion slot");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            rate,
            ..
        } = hold.source.as_ref()
        else {
            panic!("ddr-vivid hold explosion should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 4,
            "ddr-vivid hold explosion should use 4 frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("ddr-vivid hold explosion should preserve frame delays");
        assert_eq!(
            delays.len(),
            4,
            "expected one delay per hold explosion frame"
        );
        assert!(
            delays.iter().all(|delay| (*delay - 0.01).abs() < 1e-4),
            "expected all hold explosion frame delays to be 0.01, got {delays:?}"
        );
        assert_eq!(hold.frame_index(0.0, 0.0), 0);
        let advanced = match rate {
            AnimationRate::FramesPerSecond(_) => hold.frame_index(0.011, 0.0),
            AnimationRate::FramesPerBeat(_) => hold.frame_index(0.0, 0.011),
        };
        assert_eq!(
            advanced, 1,
            "ddr-vivid hold explosion should advance to frame 1 after one delay"
        );
    }

    #[test]
    fn setstateproperties_linear_frames_applies_to_synthetic_8x8_slot() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let mut slot = ns
            .mine_hit_explosion
            .as_ref()
            .expect("default should define mine hit explosion")
            .slot
            .clone();
        let key = "tests/fake/Fallback HitMine Explosion 8x8 (res 1536x1536).png".to_string();
        slot.source = Arc::new(SpriteSource::Atlas {
            texture_key: Arc::<str>::from(key.as_str()),
            tex_dims: (2048, 2048),
            cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        });
        slot.model = None;
        let (cols, rows) = crate::assets::sprite_sheet_dims(&key);
        let available = (cols.max(1) as usize).saturating_mul(rows.max(1) as usize);
        assert!(
            available > 1,
            "expected synthetic mine explosion texture to have multiple frames, got {available} for '{key}'"
        );

        itg_apply_state_properties_from_script(
            &mut slot,
            "setstateproperties,Sprite.LinearFrames(64,(64/60))",
            false,
        );

        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            rate,
            ..
        } = slot.source.as_ref()
        else {
            panic!("setstateproperties should convert mine slot source to animated");
        };
        let expected_frames = available.min(64);
        assert_eq!(
            *frame_count, expected_frames,
            "setstateproperties should clamp frame count by available frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("setstateproperties should preserve linear frame delays");
        assert_eq!(
            delays.len(),
            expected_frames,
            "expected one delay per mine animation frame"
        );
        assert!(
            delays
                .iter()
                .all(|delay| (*delay - (1.0 / 60.0)).abs() < 1e-4),
            "expected setstateproperties delays to be 1/60s, got {delays:?}"
        );
        match rate {
            AnimationRate::FramesPerSecond(fps) => {
                assert!(
                    (fps - 60.0).abs() < 1e-3,
                    "expected setstateproperties mine animation to run at 60fps, got {fps}"
                );
            }
            AnimationRate::FramesPerBeat(v) => panic!("expected time-based animation, got {v} fpb"),
        }
        assert_eq!(slot.frame_index(0.0, 0.0), 0);
        assert_eq!(
            slot.frame_index((1.0 / 60.0) + 0.001, 0.0),
            1,
            "mine animation should advance after one frame delay"
        );
    }

    #[test]
    fn explosion_animation_honors_visible_commands() {
        let anim = parse_explosion_animation("visible,false;sleep,0.1;visible,true");
        let at_start = anim.state_at(0.0);
        let mid_sleep = anim.state_at(0.05);
        let after = anim.state_at(0.11);
        assert!(!at_start.visible, "expected animation to start hidden");
        assert!(
            !mid_sleep.visible,
            "expected sleep segment to keep actor hidden"
        );
        assert!(
            after.visible,
            "expected actor to become visible after final command"
        );
    }

    #[test]
    fn explosion_animation_parses_judgment_line_to_color_diffuse() {
        let anim = parse_explosion_animation(
            r#"finishtweening;diffuse,JudgmentLineToColor("JudgmentLine_W5");diffusealpha,1;sleep,.1;decelerate,.2;diffusealpha,0"#,
        );
        let color = anim.initial.color;
        assert!(
            (color[0] - (228.0 / 255.0)).abs() <= f32::EPSILON,
            "unexpected W5 red component: {:?}",
            color
        );
        assert!(
            (color[1] - (77.0 / 255.0)).abs() <= f32::EPSILON,
            "unexpected W5 green component: {:?}",
            color
        );
        assert!(
            (color[2] - 1.0).abs() <= f32::EPSILON,
            "unexpected W5 blue component: {:?}",
            color
        );
        assert!(
            (color[3] - 1.0).abs() <= f32::EPSILON,
            "unexpected W5 alpha component: {:?}",
            color
        );
    }

    #[test]
    fn explosion_animation_clamps_overbright_color_to_itg_vertex_range() {
        let anim = parse_explosion_animation(
            "diffuse,1.5,1.25,1.75,1.2;glowshift;effectperiod,0.05;effectcolor1,1,1,1,1;effectcolor2,1,1,1,1",
        );
        let state = anim.state_at(0.0);
        assert_eq!(
            state.diffuse,
            [1.0, 1.0, 1.0, 1.0],
            "ITG converts Sprite vertex colors to 8-bit and clamps >1.0 channels"
        );
        assert!(
            state.glow.iter().all(|c| *c >= 0.0 && *c <= 1.0),
            "glow channels should also be clamped to [0,1], got {:?}",
            state.glow
        );
    }

    #[test]
    fn explosion_animation_tracks_blend_command_for_render_parity() {
        let add = parse_explosion_animation("blend,'BlendMode_Add';diffusealpha,1");
        assert!(
            add.blend_add,
            "blend,BlendMode_Add should mark explosion as additive"
        );

        let normal = parse_explosion_animation("blend,'BlendMode_Normal';diffusealpha,1");
        assert!(
            !normal.blend_add,
            "non-add blend commands should keep explosion on normal blend"
        );
    }

    #[test]
    fn cel_roll_glowshift_keeps_diffuse_and_uses_glow_channel() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let roll = ns
            .roll
            .explosion
            .as_ref()
            .expect("cel should define roll explosion");
        assert!(
            roll.texture_key()
                .to_ascii_lowercase()
                .contains("down hold explosion"),
            "cel roll explosion should resolve to down hold explosion texture"
        );

        let draw_0 = roll.model_draw_at(0.0, 0.0);
        let draw_1 = roll.model_draw_at(0.0125, 0.0);
        assert!(
            draw_0.visible && draw_1.visible,
            "roll explosion should be visible while active"
        );
        assert!(
            (draw_0.tint[3] - draw_1.tint[3]).abs() <= 1e-6,
            "glowshift should not modulate diffuse alpha"
        );

        let glow_alphas = [0.0f32, 0.0125, 0.025, 0.0375]
            .iter()
            .filter_map(|&t| {
                roll.model_glow_at(t, 0.0, draw_0.tint[3])
                    .map(|glow| glow[3])
            })
            .collect::<Vec<_>>();
        assert!(
            glow_alphas.len() >= 2,
            "glowshift should emit visible glow for at least part of its cycle"
        );
        let min_alpha = glow_alphas.iter().copied().fold(f32::INFINITY, f32::min);
        let max_alpha = glow_alphas
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max_alpha - min_alpha) > 0.05,
            "glow alpha should animate over time for glowshift; got {:?}",
            glow_alphas
        );
    }

    #[test]
    fn cel_w1_tap_explosion_resolves_dim_and_bright_paths() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let w1 = ns
            .tap_explosions
            .get("W1")
            .expect("cel should define W1 tap explosion");
        assert!(
            w1.animation.initial.visible,
            "cel W1 tap explosion should start visible"
        );
        assert!(
            w1.animation.initial.color[3] > 0.9,
            "cel W1 tap explosion should start from the dim W1 alpha path"
        );
        assert!(
            !w1.animation.blend_add,
            "cel W1 tap explosion should render with normal blend like ITG GhostArrow sprites"
        );
        assert!(
            w1.slot
                .texture_key()
                .to_ascii_lowercase()
                .contains("tap explosion dim w1"),
            "cel dim W1 tap explosion should use the dim W1 actor first"
        );

        let w1_bright = ns
            .tap_explosion_for_col_with_bright(0, "W1", true)
            .expect("cel should define bright W1 tap explosion");
        assert!(
            w1_bright
                .slot
                .texture_key()
                .to_ascii_lowercase()
                .contains("tap explosion bright w1"),
            "cel bright W1 tap explosion should use the bright W1 actor first"
        );
        assert!(
            w1_bright.animation.initial.color[3] > 0.9,
            "cel bright W1 tap explosion should start from the bright W1 alpha path"
        );

        let mine = ns
            .mine_hit_explosion
            .as_ref()
            .expect("cel should define hit-mine explosion");
        assert!(
            mine.animation.blend_add,
            "cel hit-mine explosion should keep additive blend from noteskin commands"
        );
    }

    #[test]
    fn cf_chrome_family_tap_explosions_keep_button_rotation() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "CF_VIBRANTALLOY")
            .expect("CF_VIBRANTALLOY should load from assets/noteskins");

        for window in ["W1", "W2", "W3", "W4", "W5"] {
            for (col, expected_rotation) in [90, 0, 180, -90].into_iter().enumerate() {
                let explosion = ns.tap_explosion_for_col(col, window).unwrap_or_else(|| {
                    panic!("{window} tap explosion should resolve for column {col}")
                });
                let mut rotated_child_count = 0usize;
                for layer in explosion.layers.iter() {
                    let key = layer.slot.texture_key().to_ascii_lowercase();
                    if key.contains("flash") || key.contains("glow") {
                        rotated_child_count += 1;
                        assert_eq!(
                            layer.slot.def.rotation_deg, expected_rotation,
                            "{window} column {col} should keep per-button rotation for {key}"
                        );
                    } else if key.contains("spark") {
                        assert_eq!(
                            layer.slot.def.rotation_deg, 0,
                            "{window} column {col} Spark should remain unrotated per PartsToRotate"
                        );
                    }
                    assert!(
                        !key.contains("tap explosion dim"),
                        "{window} column {col} should not replace the actor stack with direct Tap Explosion art"
                    );
                }
                assert!(
                    rotated_child_count > 0,
                    "{window} column {col} should keep at least one rotated Flash/Glow child"
                );
            }
        }

        clear_itg_runtime_caches();
    }

    #[test]
    fn cf_chrome_family_mine_explosion_uses_emitter_commands_without_spin() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "CF_VIBRANTALLOY")
            .expect("CF_VIBRANTALLOY should load from assets/noteskins");
        let mine = ns
            .mine_hit_explosion
            .as_ref()
            .expect("CF_VIBRANTALLOY should define a hit-mine explosion");

        assert!(
            mine.layers.iter().any(|layer| !layer.animation.blend_add),
            "CF mine explosion should keep the normal ECommand layer"
        );
        assert!(
            mine.layers.iter().any(|layer| layer.animation.blend_add),
            "CF mine explosion should keep the additive E2Command layer"
        );
        assert!(
            (mine.duration() - 64.0 / 60.0).abs() <= 1e-6,
            "CF mine explosion should use the emitter E/E2 duration, got {}",
            mine.duration()
        );
        for (idx, layer) in mine.layers.iter().enumerate() {
            assert!(
                layer.animation.initial.color[3] > 0.99,
                "mine layer {idx} should become visible when E/E2Command fires"
            );
            assert_eq!(
                layer.animation.initial.rotation_z, 0.0,
                "mine layer {idx} should not inherit the common rotating HitMineCommand"
            );
            assert!(
                layer
                    .animation
                    .segments
                    .iter()
                    .all(|segment| segment.end_rotation_z.is_none()),
                "mine layer {idx} should not animate rotation"
            );
        }

        clear_itg_runtime_caches();
    }

    #[test]
    fn cel_tap_mine_prefers_model_actor_over_texture_fallback() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            mine.model.is_some(),
            "cel mine should come from Tap Mine model actor, not _mine texture fallback"
        );
        assert!(
            ns.mine_frames.first().is_some_and(Option::is_none),
            "cel mine uses a single model actor and should not duplicate it as a frame layer"
        );
    }

    #[test]
    fn cel_tap_mine_uv_phase_uses_beat_clock_from_metrics() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(
            ns.animation_is_beat_based,
            "cel metrics use beat-based noteskin animation"
        );
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::Mine as usize].length - 1.0)
                .abs()
                <= f32::EPSILON,
            "cel tap mine animation length should be 1 beat"
        );
        let phase = ns.tap_mine_uv_phase(0.5, 1.0, 0.0);
        assert!(
            phase <= 1e-6,
            "one beat should wrap tap mine phase to 0 for cel; got {phase}"
        );
    }

    #[test]
    fn cel_tap_mine_does_not_set_model_spin_effect() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            matches!(mine.model_effect.mode, ModelEffectMode::None),
            "cel mine should not set model spin effect via parser commands"
        );
    }

    #[test]
    fn cel_tap_mine_uses_milkshape_bone_rotation_timing() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            (mine.model_auto_rot_total_frames - 120.0).abs() <= f32::EPSILON,
            "cel mine should use milkshape total frame count for auto-rotation"
        );
        assert!(
            mine.model_auto_rot_z_keys.len() >= 2,
            "cel mine should expose at least two auto-rotation keys"
        );
        let rot_0 = mine.model_draw_at(0.0, 0.0).rot[2];
        let rot_1 = mine.model_draw_at(1.0, 0.0).rot[2];
        let delta = (rot_1 - rot_0 + 540.0).rem_euclid(360.0) - 180.0;
        assert!(
            (delta - 87.3).abs() <= 0.5,
            "cel mine should rotate by ~87.3 degrees after one second; got delta={delta}"
        );
    }

    #[test]
    fn lambda_tap_mine_spin_uses_beat_clock_and_magnitude() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "lambda")
            .expect("dance/lambda should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("lambda should define first-column mine slot");
        assert!(
            matches!(mine.model_effect.mode, ModelEffectMode::Spin),
            "lambda mine init command should enable spin effect"
        );
        assert!(
            matches!(mine.model_effect.clock, ModelEffectClock::Beat),
            "lambda mine spin should run on beat clock"
        );
        let rot_0 = mine.model_draw_at(0.0, 0.0).rot[2];
        let rot_1 = mine.model_draw_at(0.0, 1.0).rot[2];
        let delta = (rot_1 - rot_0 + 540.0).rem_euclid(360.0) - 180.0;
        assert!(
            (delta + 33.0).abs() <= 1e-3,
            "one beat should rotate lambda mine by -33 degrees; got delta={delta}"
        );
    }

    #[test]
    fn model_auto_rot_interpolates_and_wraps() {
        let slot = test_auto_rot_slot(
            80.0,
            vec![
                ModelAutoRotKey {
                    frame: 10.0,
                    z_deg: 20.0,
                },
                ModelAutoRotKey {
                    frame: 40.0,
                    z_deg: 80.0,
                },
            ],
        );
        let auto_rot = |time| {
            deadsync_noteskin::model_auto_rot_z_at(80.0, slot.model_auto_rot_z_keys.as_ref(), time)
        };

        assert_eq!(auto_rot(0.0), Some(20.0));
        let interp = auto_rot(25.0 / 30.0).expect("frame 25 should interpolate between keys");
        assert!(
            (interp - 50.0).abs() <= 1e-6,
            "frame 25 should interpolate to 50 degrees; got {interp}"
        );
        assert_eq!(auto_rot(40.0 / 30.0), Some(80.0));
        assert_eq!(auto_rot(70.0 / 30.0), Some(80.0));
        assert_eq!(auto_rot(80.0 / 30.0), Some(20.0));
    }

    #[test]
    fn ddr_note_tap_mine_keeps_second_model_layer_as_frame() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("ddr-note should define first-column mine slot");
        let frame = ns
            .mine_frames
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("ddr-note should preserve second mine layer");
        assert!(
            mine.model.is_some(),
            "ddr-note mine fill should be model-backed"
        );
        assert!(
            frame.model.is_some(),
            "ddr-note mine frame should be model-backed second layer"
        );
    }
}
