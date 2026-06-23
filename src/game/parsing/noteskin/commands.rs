use super::{AnimationRate, SpriteSlot, SpriteSource};
use crate::assets;
use deadsync_noteskin::script::{
    normalized_script_command, parse_script_effectclock_from_commands,
    parse_script_state_properties, split_script_token,
};
use std::collections::HashMap;
use std::sync::{Arc, atomic::AtomicU64};

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
    let cols = grid_x.max(1) as usize;
    let rows = grid_y.max(1) as usize;
    let available = (cols * rows).max(1);
    if available <= 1 {
        return;
    }
    let anim_frames = frame_count.min(available).max(1);
    if anim_frames <= 1 {
        return;
    }

    let frame_w = (tex_dims.0 / cols as u32).max(1) as i32;
    let frame_h = (tex_dims.1 / rows as u32).max(1) as i32;
    let src_x = slot.def.src[0].max(0) as usize;
    let src_y = slot.def.src[1].max(0) as usize;
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

    slot.source = Arc::new(SpriteSource::Animated {
        texture_key: texture_key.clone(),
        tex_dims,
        frame_size: [frame_w, frame_h],
        grid: (cols, rows),
        frame_count: anim_frames,
        frame_indices: None,
        rate,
        frame_durations: Some(Arc::<[f32]>::from(durations)),
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });
    let start_col = start_idx % cols;
    let start_row = start_idx / cols;
    slot.def.src = [start_col as i32 * frame_w, start_row as i32 * frame_h];
    slot.def.size = [frame_w, frame_h];
    let source_frame =
        assets::texture_source_frame_dims_from_real(&texture_key, tex_dims.0, tex_dims.1);
    slot.source_size = [source_frame.0 as i32, source_frame.1 as i32];
}

pub(super) fn itg_apply_state_properties_from_script(
    slot: &mut SpriteSlot,
    script: &str,
    beat_based: bool,
) {
    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((command, args)) = split_script_token(token) else {
            continue;
        };
        if command != "setstateproperties" {
            continue;
        }
        if let Some((frame_count, delays)) = parse_script_state_properties(&args) {
            itg_apply_slot_state_properties(slot, frame_count, &delays, beat_based);
        }
    }
}

pub(super) fn itg_apply_state_properties_from_commands(
    slot: &mut SpriteSlot,
    commands: &HashMap<String, String>,
) {
    if commands.is_empty() {
        return;
    }
    let mut sorted = commands.iter().collect::<Vec<_>>();
    sorted.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut beat_based = slot_is_beat_based(slot);
    for (_, script) in sorted.iter().copied() {
        if let Some(script_clock) = parse_script_effectclock_from_commands(script) {
            beat_based = script_clock;
        }
    }
    for (_, script) in sorted {
        itg_apply_state_properties_from_script(slot, script, beat_based);
    }
}
