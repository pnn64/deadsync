use super::{
    AnimationRate, ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelTweenSegment,
    SpriteDefinition, SpriteSlot, SpriteSource,
};
use crate::assets;
use deadlib_platform::dirs;
use deadsync_noteskin::itg as noteskin_itg;
use image::image_dimensions;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicU64};

pub(super) fn itg_find_texture_with_prefix(
    data: &noteskin_itg::NoteskinData,
    prefix: &str,
) -> Option<PathBuf> {
    let want = prefix.to_ascii_lowercase();
    for dir in &data.search_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut matches = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|name| {
                        name.to_ascii_lowercase().starts_with(&want)
                            && name.to_ascii_lowercase().ends_with(".png")
                    })
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        matches.sort_by(|a, b| {
            let a_name = a
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let b_name = b
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            a_name.cmp(&b_name)
        });
        return matches.into_iter().next();
    }
    None
}

pub(super) fn itg_texture_key(path: &Path) -> Option<String> {
    let mut key = if let Some(rel) = dirs::app_dirs()
        .strip_asset_prefix(path)
        .or_else(|| path.strip_prefix("assets").ok())
    {
        rel.to_string_lossy().replace('\\', "/")
    } else if path.is_file() {
        path.to_string_lossy().replace('\\', "/")
    } else {
        return None;
    };
    if !path.is_absolute() {
        while key.starts_with('/') {
            key.remove(0);
        }
    }
    Some(key)
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
    let cols = grid_x.max(1) as usize;
    let rows = grid_y.max(1) as usize;
    let count = (cols * rows).max(1);
    let idx = frame % count;
    let col = idx % cols;
    let row = idx / cols;
    let frame_w = (tex_w / cols as u32).max(1);
    let frame_h = (tex_h / rows as u32).max(1);
    slot.def.src = [col as i32 * frame_w as i32, row as i32 * frame_h as i32];
    slot.def.size = [frame_w as i32, frame_h as i32];
}

pub(super) fn itg_slot_from_path_with_frame(path: &Path, frame: usize) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&key);
    let cols = (grid_x.max(1)) as usize;
    let rows = (grid_y.max(1)) as usize;
    let frame_count = (cols * rows).max(1);
    let idx = frame % frame_count;
    let col = idx % cols;
    let row = idx / cols;
    let frame_w = (dims.0 / cols as u32).max(1);
    let frame_h = (dims.1 / rows as u32).max(1);
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
    let source = Arc::new(SpriteSource::Atlas {
        texture_key: key.into(),
        tex_dims: dims,
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });
    Some(SpriteSlot {
        def: SpriteDefinition {
            src: [col as i32 * frame_w as i32, row as i32 * frame_h as i32],
            size: [frame_w as i32, frame_h as i32],
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
    let cols = grid_x.max(1) as usize;
    let rows = grid_y.max(1) as usize;
    let available = (cols * rows).max(1);
    if available <= 1 || frame_count <= 1 {
        return itg_slot_from_path_with_frame(path, frame0);
    }
    let anim_frames = if frame_indices.is_some() {
        frame_count.max(1)
    } else {
        frame_count.min(available).max(1)
    };
    let start = frame0 % available;
    let col = start % cols;
    let row = start / cols;
    let frame_w = (dims.0 / cols as u32).max(1);
    let frame_h = (dims.1 / rows as u32).max(1);
    let source_frame = assets::texture_source_frame_dims_from_real(&key, dims.0, dims.1);
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
            Arc::<[f32]>::from(normalized)
        })
        .filter(|durations| !durations.is_empty());
    let frame_indices = frame_indices
        .map(|indices| {
            let mut normalized = Vec::with_capacity(anim_frames);
            let fallback = indices.first().copied().unwrap_or(start);
            for idx in 0..anim_frames {
                normalized.push(indices.get(idx).copied().unwrap_or(fallback));
            }
            Arc::<[usize]>::from(normalized)
        })
        .filter(|indices| !indices.is_empty());
    let source = Arc::new(SpriteSource::Animated {
        texture_key: key.into(),
        tex_dims: dims,
        frame_size: [frame_w as i32, frame_h as i32],
        grid: (cols, rows),
        frame_count: anim_frames,
        frame_indices,
        rate,
        frame_durations,
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });
    Some(SpriteSlot {
        def: SpriteDefinition {
            src: [col as i32 * frame_w as i32, row as i32 * frame_h as i32],
            size: [frame_w as i32, frame_h as i32],
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

pub(super) fn itg_slot_from_path_all_frames(
    path: &Path,
    frame_delay: Option<f32>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let (cols, rows) = assets::sprite_sheet_dims(&key);
    let frame_count = (cols.max(1) as usize).saturating_mul(rows.max(1) as usize);
    if frame_count <= 1 {
        return itg_slot_from_path(path);
    }
    let delays = frame_delay.map(|delay| {
        let d = delay.max(1e-6);
        vec![d; frame_count]
    });
    itg_slot_from_path_animated(path, 0, frame_count, None, delays.as_deref(), beat_based)
        .or_else(|| itg_slot_from_path(path))
}

pub(super) fn texture_dimensions(key: &str) -> Option<(u32, u32)> {
    if let Some(meta) = assets::texture_dims(key) {
        return Some((meta.w, meta.h));
    }
    let path = PathBuf::from("assets").join(key);
    image_dimensions(&path).ok()
}
