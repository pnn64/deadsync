use super::lua::{
    itg_extract_quoted_strings, itg_find_function_end, itg_find_matching, itg_parse_lua_float_expr,
    itg_parse_lua_quoted, itg_parse_self_chain_commands, itg_skip_ws, itg_split_call_args,
};
use super::script::{
    normalized_script_command, parse_script_effectclock_from_commands, split_script_token,
};
use super::{AnimationRate, SpriteSlot, SpriteSource};
use crate::assets;
use deadsync_noteskin::itg as noteskin_itg;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, atomic::AtomicU64};

fn itg_parse_linear_frames_expr(raw: &str) -> Option<(usize, Vec<f32>)> {
    let value = raw.trim().trim_end_matches(';').trim();
    let open = value.find('(')?;
    let head = value[..open].trim();
    if !head.eq_ignore_ascii_case("Sprite.LinearFrames") {
        return None;
    }
    let close = itg_find_matching(value, open, '(', ')')?;
    let args = itg_split_call_args(&value[open + 1..close]);
    if args.len() < 2 {
        return None;
    }
    let frame_count = args[0]
        .trim()
        .parse::<usize>()
        .ok()
        .or_else(|| itg_parse_lua_float_expr(&args[0]).map(|v| v as usize))?
        .max(1);
    let seconds = itg_parse_lua_float_expr(&args[1])?;
    let delay = (seconds / frame_count as f32).max(0.0);
    Some((frame_count, vec![delay; frame_count]))
}

fn parse_script_state_properties(args: &[String]) -> Option<(usize, Vec<f32>)> {
    args.first()
        .and_then(|expr| itg_parse_linear_frames_expr(expr))
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

fn itg_parse_commands_block(
    block: &str,
    metrics: &noteskin_itg::IniData,
) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    for raw in block.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        if !key.ends_with("command") {
            continue;
        }
        if let Some(cmd) = itg_resolve_command_expr(v.trim(), metrics) {
            commands.insert(key, cmd);
        }
    }
    for (k, v) in itg_parse_function_commands(block) {
        commands.insert(k, v);
    }
    commands
}

fn itg_parse_function_commands(block: &str) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    let bytes = block.as_bytes();
    let mut cursor = 0usize;
    while let Some(eq_rel) = block[cursor..].find('=') {
        let eq = cursor + eq_rel;
        let key_start = block[..eq]
            .rfind(['\n', '\r', '{', ';', ','])
            .map_or(0, |idx| idx + 1);
        let key = block[key_start..eq].trim();
        let key_lower = key.to_ascii_lowercase();
        if !key_lower.ends_with("command") {
            cursor = eq + 1;
            continue;
        }
        let mut rhs = itg_skip_ws(block, eq + 1);
        if !block.get(rhs..).is_some_and(|s| s.starts_with("function")) {
            cursor = eq + 1;
            continue;
        }
        rhs += "function".len();
        let Some(param_open_rel) = block[rhs..].find('(') else {
            cursor = eq + 1;
            continue;
        };
        let param_open = rhs + param_open_rel;
        let Some(param_close) = itg_find_matching(block, param_open, '(', ')') else {
            cursor = eq + 1;
            continue;
        };
        let body_start = param_close + 1;
        let Some(end_idx) = itg_find_function_end(block, body_start) else {
            cursor = eq + 1;
            continue;
        };
        let body = &block[body_start..end_idx];
        if let Some(cmd) = itg_parse_self_chain_commands(body) {
            commands.insert(key_lower, cmd);
        }
        cursor = end_idx + 3;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
    }
    commands
}

fn itg_resolve_command_expr(raw: &str, metrics: &noteskin_itg::IniData) -> Option<String> {
    let value = raw
        .trim()
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim();
    if value.starts_with("NOTESKIN:GetMetricA(") {
        let args = itg_extract_quoted_strings(value);
        if args.len() >= 2 {
            return metrics.get(&args[0], &args[1]).map(str::to_string);
        }
    }
    if value.starts_with("cmd(") && value.ends_with(')') {
        return Some(value[4..value.len() - 1].trim().to_string());
    }
    if let Some(q) = itg_parse_lua_quoted(value) {
        return Some(q);
    }
    Some(value.to_string())
}

pub(super) fn itg_parse_wrapper_commands_from_file(
    path: &Path,
    metrics: &noteskin_itg::IniData,
) -> Option<HashMap<String, String>> {
    let is_lua = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"));
    if !is_lua {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let marker = ".. {";
    let marker_idx = content.find(marker)?;
    let open = marker_idx + marker.len() - 1;
    let close = itg_find_matching(&content, open, '{', '}')?;
    Some(itg_parse_commands_block(&content[open + 1..close], metrics))
}
