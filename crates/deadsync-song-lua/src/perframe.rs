use mlua::{Function, Lua, Table, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    LUA_PLAYERS, SONG_LUA_PLAYER_OPTIONS_KEYS, SongLuaCompileContext, SongLuaCompileInfo,
    SongLuaEaseTarget, SongLuaEaseWindow, SongLuaOverlayCompileActor, SongLuaOverlayEase,
    SongLuaOverlayState, SongLuaSpanMode, SongLuaTimeUnit, SongLuaTrackedActor,
    SongLuaTrackedActorTarget, actor_overlay_initial_state, actor_tree_has_update_functions,
    compile_song_runtime_delta_values, compile_song_runtime_values, overlay_delta_pair_from_states,
    push_unique_compile_detail, read_f32, reset_overlay_compile_actor_capture_tables,
    reset_tracked_capture_tables, run_actor_update_functions_with_delta,
    runtime_player_option_ease_target, set_compile_song_runtime_beat,
    set_compile_song_runtime_delta_values, set_compile_song_runtime_values, song_display_bps,
    song_elapsed_seconds_for_beat, song_lua_side_effect_count, song_music_rate,
};

pub const SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES: usize = 4096;
const SONG_LUA_PLAYER_TRANSFORM_SAMPLE_DIVISOR: f32 = 4.0;
const PLAYER_TRANSFORM_CAPTURE_KEYS: [&str; 11] = [
    "x",
    "y",
    "z",
    "rot_x_deg",
    "rot_z_deg",
    "rot_y_deg",
    "zoom_x",
    "zoom_y",
    "zoom_z",
    "skew_x",
    "skew_y",
];

pub struct SongLuaPerframeEntry {
    pub start: f32,
    pub end: f32,
    pub function: Function,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaPerframeSample {
    pub beat: f32,
    pub eval_beat: f32,
    pub delta_beats: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaPerframePlayerState {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub rotation_x: Option<f32>,
    pub rotation_z: Option<f32>,
    pub rotation_y: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
}

pub type SongLuaUpdateModState = BTreeMap<String, f32>;

pub fn read_perframe_entries(table: Option<Table>) -> Result<Vec<SongLuaPerframeEntry>, String> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(end) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?) else {
            continue;
        };
        let Value::Function(function) = entry.raw_get::<Value>(3).map_err(|err| err.to_string())?
        else {
            continue;
        };
        if !start.is_finite() || !end.is_finite() || end <= start {
            continue;
        }
        out.push(SongLuaPerframeEntry {
            start,
            end,
            function,
        });
    }
    Ok(out)
}

pub fn perframe_boundaries(entries: &[SongLuaPerframeEntry]) -> Vec<f32> {
    let mut boundaries = entries
        .iter()
        .flat_map(|entry| [entry.start, entry.end])
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    boundaries.sort_by(|left, right| left.total_cmp(right));
    boundaries.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);
    boundaries
}

pub fn actor_perframe_player_state(actor: &Table) -> Result<SongLuaPerframePlayerState, String> {
    let zoom = actor
        .get::<Option<f32>>("__songlua_state_zoom")
        .map_err(|err| err.to_string())?;
    Ok(SongLuaPerframePlayerState {
        x: actor
            .get::<Option<f32>>("__songlua_state_x")
            .map_err(|err| err.to_string())?,
        y: actor
            .get::<Option<f32>>("__songlua_state_y")
            .map_err(|err| err.to_string())?,
        z: actor
            .get::<Option<f32>>("__songlua_state_z")
            .map_err(|err| err.to_string())?,
        rotation_x: actor
            .get::<Option<f32>>("__songlua_state_rot_x_deg")
            .map_err(|err| err.to_string())?,
        rotation_z: actor
            .get::<Option<f32>>("__songlua_state_rot_z_deg")
            .map_err(|err| err.to_string())?,
        rotation_y: actor
            .get::<Option<f32>>("__songlua_state_rot_y_deg")
            .map_err(|err| err.to_string())?,
        zoom_x: actor
            .get::<Option<f32>>("__songlua_state_zoom_x")
            .map_err(|err| err.to_string())?
            .or(zoom),
        zoom_y: actor
            .get::<Option<f32>>("__songlua_state_zoom_y")
            .map_err(|err| err.to_string())?
            .or(zoom),
        zoom_z: actor
            .get::<Option<f32>>("__songlua_state_zoom_z")
            .map_err(|err| err.to_string())?
            .or(zoom),
        skew_x: actor
            .get::<Option<f32>>("__songlua_state_skew_x")
            .map_err(|err| err.to_string())?,
        skew_y: actor
            .get::<Option<f32>>("__songlua_state_skew_y")
            .map_err(|err| err.to_string())?,
    })
}

pub fn current_perframe_player_states(
    player_tables: &[Option<Table>; LUA_PLAYERS],
) -> Result<[SongLuaPerframePlayerState; LUA_PLAYERS], String> {
    let mut out = [SongLuaPerframePlayerState::default(); LUA_PLAYERS];
    for player in 0..LUA_PLAYERS {
        let Some(actor) = player_tables[player].as_ref() else {
            continue;
        };
        out[player] = actor_perframe_player_state(actor)?;
    }
    Ok(out)
}

fn capture_transform_mask(block: &Table) -> Result<u16, String> {
    let mut mask = 0;
    for (index, key) in PLAYER_TRANSFORM_CAPTURE_KEYS.iter().enumerate() {
        if !matches!(
            block
                .raw_get::<Value>(*key)
                .map_err(|err| err.to_string())?,
            Value::Nil
        ) {
            mask |= 1 << index;
        }
    }
    Ok(mask)
}

fn actor_transform_mask(actor: &Table) -> Result<u16, String> {
    let mut mask = 0;
    if let Some(block) = actor
        .get::<Option<Table>>("__songlua_capture_block")
        .map_err(|err| err.to_string())?
    {
        mask |= capture_transform_mask(&block)?;
    }
    if let Some(blocks) = actor
        .get::<Option<Table>>("__songlua_capture_blocks")
        .map_err(|err| err.to_string())?
    {
        for block in blocks.sequence_values::<Table>() {
            mask |= capture_transform_mask(&block.map_err(|err| err.to_string())?)?;
        }
    }
    Ok(mask)
}

fn player_transform_masks(
    player_tables: &[Option<Table>; LUA_PLAYERS],
) -> Result<[u16; LUA_PLAYERS], String> {
    let mut masks = [0; LUA_PLAYERS];
    for (player, actor) in player_tables.iter().enumerate() {
        if let Some(actor) = actor {
            masks[player] = actor_transform_mask(actor)?;
        }
    }
    Ok(masks)
}

fn transform_distance(left: f32, right: f32, cyclic: bool) -> f32 {
    let distance = (left - right).abs();
    if cyclic {
        let wrapped = distance.rem_euclid(360.0);
        wrapped.min(360.0 - wrapped)
    } else {
        distance
    }
}

fn transform_tail_closes(
    current: Option<f32>,
    prior: Option<f32>,
    baseline: Option<f32>,
    default: f32,
    cyclic: bool,
) -> bool {
    let current = current.unwrap_or(default);
    let prior = prior.unwrap_or(default);
    let baseline = baseline.unwrap_or(default);
    let remaining = transform_distance(current, baseline, cyclic);
    let prior_remaining = transform_distance(prior, baseline, cyclic);
    let last_step = transform_distance(current, prior, cyclic);
    remaining < prior_remaining && remaining <= last_step + f32::EPSILON
}

fn snap_ended_transforms(
    actor: &Table,
    current: &mut SongLuaPerframePlayerState,
    prior: SongLuaPerframePlayerState,
    baseline: SongLuaPerframePlayerState,
    ended: u16,
) -> Result<(), String> {
    macro_rules! snap {
        ($index:expr, $field:ident, $state_key:literal, $default:expr, $cyclic:expr) => {
            if ended & (1 << $index) != 0
                && transform_tail_closes(
                    current.$field,
                    prior.$field,
                    baseline.$field,
                    $default,
                    $cyclic,
                )
            {
                current.$field = baseline.$field;
                actor
                    .set($state_key, baseline.$field)
                    .map_err(|err| err.to_string())?;
            }
        };
    }

    snap!(0, x, "__songlua_state_x", 0.0, false);
    snap!(1, y, "__songlua_state_y", 0.0, false);
    snap!(2, z, "__songlua_state_z", 0.0, false);
    snap!(3, rotation_x, "__songlua_state_rot_x_deg", 0.0, true);
    snap!(4, rotation_z, "__songlua_state_rot_z_deg", 0.0, true);
    snap!(5, rotation_y, "__songlua_state_rot_y_deg", 0.0, true);
    snap!(6, zoom_x, "__songlua_state_zoom_x", 1.0, false);
    snap!(7, zoom_y, "__songlua_state_zoom_y", 1.0, false);
    snap!(8, zoom_z, "__songlua_state_zoom_z", 1.0, false);
    snap!(9, skew_x, "__songlua_state_skew_x", 0.0, false);
    snap!(10, skew_y, "__songlua_state_skew_y", 0.0, false);
    Ok(())
}

pub fn tracked_player_tables(
    tracked_actors: &[SongLuaTrackedActor],
) -> [Option<Table>; LUA_PLAYERS] {
    let mut out = std::array::from_fn(|_| None);
    for tracked in tracked_actors {
        if let SongLuaTrackedActorTarget::Player(player) = tracked.target {
            out[player] = Some(tracked.table.clone());
        }
    }
    out
}

pub fn update_player_option_tables(lua: &Lua) -> Result<[Table; LUA_PLAYERS], String> {
    let globals = lua.globals();
    Ok([
        globals
            .get::<Table>(SONG_LUA_PLAYER_OPTIONS_KEYS[0])
            .map_err(|err| err.to_string())?,
        globals
            .get::<Table>(SONG_LUA_PLAYER_OPTIONS_KEYS[1])
            .map_err(|err| err.to_string())?,
    ])
}

pub fn player_option_sample(table: &Table) -> Result<SongLuaUpdateModState, String> {
    let mut out = SongLuaUpdateModState::new();
    if let Some(state) = table
        .raw_get::<Option<Table>>("__songlua_player_option_state")
        .map_err(|err| err.to_string())?
    {
        for pair in state.pairs::<String, Value>() {
            let (key, value) = pair.map_err(|err| err.to_string())?;
            let value = match value {
                Value::Boolean(value) => f32::from(value),
                value => match read_f32(value) {
                    Some(value) => value,
                    None => continue,
                },
            };
            out.insert(key, value);
        }
    }
    for key in ["xmod", "cmod", "mmod"] {
        if let Some(value) = table
            .get::<Option<f32>>(format!("__songlua_speedmod_{key}"))
            .map_err(|err| err.to_string())?
        {
            out.insert(key.to_string(), value);
        }
    }
    Ok(out)
}

pub fn current_update_mod_states(
    tables: &[Table; LUA_PLAYERS],
) -> Result<[SongLuaUpdateModState; LUA_PLAYERS], String> {
    Ok([
        player_option_sample(&tables[0])?,
        player_option_sample(&tables[1])?,
    ])
}

pub fn active_perframe_entries(
    entries: &[SongLuaPerframeEntry],
    start: f32,
    end: f32,
) -> Vec<&SongLuaPerframeEntry> {
    let mid = start + 0.5 * (end - start);
    entries
        .iter()
        .filter(|entry| mid > entry.start && mid < entry.end)
        .collect()
}

#[inline(always)]
pub fn perframe_segment_step(len: f32) -> f32 {
    (len / 96.0).clamp(1.0 / 192.0, 0.125)
}

#[inline(always)]
pub fn perframe_delta_seconds(context: &SongLuaCompileContext, delta_beats: f32) -> f32 {
    song_elapsed_seconds_for_beat(
        delta_beats,
        song_display_bps(context),
        song_music_rate(context),
    )
}

#[inline(always)]
pub fn relative_player_target(value: Option<f32>, baseline: Option<f32>) -> Option<f32> {
    value.map(|value| value - baseline.unwrap_or(0.0))
}

pub fn call_perframe_entry(
    lua: &Lua,
    entry: &SongLuaPerframeEntry,
    beat: f32,
    delta_beats: f32,
    delta_seconds: f32,
) -> Result<bool, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let previous_delta = compile_song_runtime_delta_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, delta_beats, delta_seconds)
        .map_err(|err| err.to_string())?;
    let result = entry
        .function
        .call::<Value>((beat, delta_seconds))
        .map(|_| ())
        .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, previous_delta.0, previous_delta.1)
        .map_err(|err| err.to_string())?;
    let saw_side_effect =
        song_lua_side_effect_count(lua).map_err(|err| err.to_string())? > side_effect_before;
    result?;
    Ok(saw_side_effect)
}

pub fn update_function_end_beat(context: &SongLuaCompileContext) -> f32 {
    let seconds = context.music_length_seconds.max(0.0);
    let beats = seconds * song_display_bps(context) * song_music_rate(context);
    beats.max(0.0)
}

pub fn update_function_sample_step(len: f32) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    let capped = len / SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES as f32;
    perframe_segment_step(len).max(capped)
}

pub fn update_function_samples(start: f32, end: f32) -> Vec<SongLuaPerframeSample> {
    let step = update_function_sample_step(end - start);
    let mut out = Vec::new();
    let mut beat = (start + step).min(end);
    let mut prev_eval = Some(start);

    loop {
        let eval_beat = beat;
        let delta_beats = prev_eval
            .map(|prev| (eval_beat - prev).abs())
            .unwrap_or(0.0);
        out.push(SongLuaPerframeSample {
            beat,
            eval_beat,
            delta_beats,
        });
        prev_eval = Some(eval_beat);
        if beat >= end - f32::EPSILON {
            break;
        }
        beat = (beat + step).min(end);
    }
    out
}

pub fn perframe_samples(start: f32, end: f32) -> Vec<SongLuaPerframeSample> {
    let step = perframe_segment_step(end - start);
    let eps = (0.5 * step).min(0.25 * (end - start)).max(1.0e-4_f32);
    let mut out = Vec::new();
    let mut beat = start;
    let mut prev_eval = None::<f32>;
    loop {
        let eval_beat = if beat <= start + f32::EPSILON {
            (start + eps).min(end - eps)
        } else if beat >= end - f32::EPSILON {
            (end - eps).max(start + eps)
        } else {
            beat
        };
        let delta_beats = prev_eval
            .map(|prev| (eval_beat - prev).abs())
            .unwrap_or(0.0);
        out.push(SongLuaPerframeSample {
            beat,
            eval_beat,
            delta_beats,
        });
        prev_eval = Some(eval_beat);
        if beat >= end - f32::EPSILON {
            break;
        }
        beat = (beat + step).min(end);
        if beat > end {
            beat = end;
        }
    }
    out
}

pub fn unsupported_perframe_info(entries: &[SongLuaPerframeEntry]) -> SongLuaCompileInfo {
    let mut info = SongLuaCompileInfo {
        unsupported_perframes: entries.len(),
        ..SongLuaCompileInfo::default()
    };
    for entry in entries {
        push_unique_compile_detail(
            &mut info.unsupported_perframe_captures,
            format!("perframe start={:.3} end={:.3}", entry.start, entry.end),
        );
    }
    info
}

pub fn push_perframe_overlay_targets(
    out: &mut Vec<SongLuaOverlayEase>,
    start: f32,
    end: f32,
    from_overlays: &[SongLuaOverlayState],
    to_overlays: &[SongLuaOverlayState],
    baseline_overlays: &[SongLuaOverlayState],
    skip_unchanged: bool,
) {
    for overlay_index in 0..from_overlays.len().min(to_overlays.len()) {
        if skip_unchanged && from_overlays[overlay_index] == to_overlays[overlay_index] {
            continue;
        }
        let Some((from, to)) = overlay_delta_pair_from_states(
            baseline_overlays[overlay_index],
            from_overlays[overlay_index],
            to_overlays[overlay_index],
        ) else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit: SongLuaTimeUnit::Beat,
            start,
            limit: end - start,
            span_mode: SongLuaSpanMode::Len,
            from,
            to,
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        });
    }
}

pub fn update_function_overlay_eases(
    end: f32,
    baseline_overlays: &[SongLuaOverlayState],
    sample_beats: &[f32],
    overlay_samples: &[Vec<SongLuaOverlayState>],
) -> Vec<SongLuaOverlayEase> {
    let mut out = Vec::new();
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        if seg_end <= seg_start {
            continue;
        }
        let from_overlays = &overlay_samples[index];
        let to_overlays = overlay_samples.get(index + 1).unwrap_or(from_overlays);
        push_perframe_overlay_targets(
            &mut out,
            seg_start,
            seg_end,
            from_overlays,
            to_overlays,
            baseline_overlays,
            true,
        );
    }
    out
}

pub fn push_perframe_player_target(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from: Option<f32>,
    to: Option<f32>,
    baseline: Option<f32>,
    neutral: f32,
    target: SongLuaEaseTarget,
    player: usize,
) {
    if end <= start {
        return;
    }
    let baseline = baseline.unwrap_or(neutral);
    let from = from.unwrap_or(baseline);
    let to = to.unwrap_or(baseline);
    if !from.is_finite() || !to.is_finite() {
        return;
    }
    if (from - baseline).abs() <= f32::EPSILON && (to - baseline).abs() <= f32::EPSILON {
        return;
    }
    out.push(SongLuaEaseWindow {
        unit: SongLuaTimeUnit::Beat,
        start,
        limit: end - start,
        span_mode: SongLuaSpanMode::Len,
        from,
        to,
        target,
        easing: Some("linear".to_string()),
        player: Some((player + 1) as u8),
        sustain: None,
        opt1: None,
        opt2: None,
    });
}

pub fn push_perframe_player_targets(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    to_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
) {
    for player in 0..LUA_PLAYERS {
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].x,
            to_players[player].x,
            baseline_players[player].x,
            0.0,
            SongLuaEaseTarget::PlayerX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].y,
            to_players[player].y,
            baseline_players[player].y,
            0.0,
            SongLuaEaseTarget::PlayerY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            relative_player_target(from_players[player].z, baseline_players[player].z),
            relative_player_target(to_players[player].z, baseline_players[player].z),
            Some(0.0),
            0.0,
            SongLuaEaseTarget::PlayerZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_x,
            to_players[player].rotation_x,
            baseline_players[player].rotation_x,
            0.0,
            SongLuaEaseTarget::PlayerRotationX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_z,
            to_players[player].rotation_z,
            baseline_players[player].rotation_z,
            0.0,
            SongLuaEaseTarget::PlayerRotationZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_y,
            to_players[player].rotation_y,
            baseline_players[player].rotation_y,
            0.0,
            SongLuaEaseTarget::PlayerRotationY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_x,
            to_players[player].zoom_x,
            baseline_players[player].zoom_x,
            1.0,
            SongLuaEaseTarget::PlayerZoomX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_y,
            to_players[player].zoom_y,
            baseline_players[player].zoom_y,
            1.0,
            SongLuaEaseTarget::PlayerZoomY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_z,
            to_players[player].zoom_z,
            baseline_players[player].zoom_z,
            1.0,
            SongLuaEaseTarget::PlayerZoomZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].skew_x,
            to_players[player].skew_x,
            baseline_players[player].skew_x,
            0.0,
            SongLuaEaseTarget::PlayerSkewX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].skew_y,
            to_players[player].skew_y,
            baseline_players[player].skew_y,
            0.0,
            SongLuaEaseTarget::PlayerSkewY,
            player,
        );
    }
}

#[inline(always)]
fn update_mod_runtime_value(key: &str, value: f32) -> f32 {
    if matches!(key, "xmod" | "cmod" | "mmod") {
        value
    } else {
        value * 100.0
    }
}

pub fn push_update_mod_targets(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    to_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    baseline_players: &[SongLuaUpdateModState; LUA_PLAYERS],
) {
    for player in 0..LUA_PLAYERS {
        let keys = from_players[player]
            .keys()
            .chain(to_players[player].keys())
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for key in keys {
            let baseline = baseline_players[player].get(key).copied().unwrap_or(0.0);
            let from = from_players[player].get(key).copied().unwrap_or(baseline);
            let to = to_players[player].get(key).copied().unwrap_or(baseline);
            let Some(target) = runtime_player_option_ease_target(key, key) else {
                continue;
            };
            push_perframe_player_target(
                out,
                start,
                end,
                Some(update_mod_runtime_value(key, from)),
                Some(update_mod_runtime_value(key, to)),
                Some(update_mod_runtime_value(key, baseline)),
                0.0,
                target,
                player,
            );
        }
    }
}

pub fn push_perframe_static_targets(
    out_eases: &mut Vec<SongLuaEaseWindow>,
    out_overlay_eases: &mut Vec<SongLuaOverlayEase>,
    start: f32,
    end: f32,
    current_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    current_overlays: &[SongLuaOverlayState],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_overlays: &[SongLuaOverlayState],
) {
    push_perframe_player_targets(
        out_eases,
        start,
        end,
        current_players,
        current_players,
        baseline_players,
    );
    push_perframe_overlay_targets(
        out_overlay_eases,
        start,
        end,
        current_overlays,
        current_overlays,
        baseline_overlays,
        false,
    );
}

pub fn push_sampled_perframe_targets(
    out_eases: &mut Vec<SongLuaEaseWindow>,
    out_overlay_eases: &mut Vec<SongLuaOverlayEase>,
    end: f32,
    sample_beats: &[f32],
    player_samples: &[[SongLuaPerframePlayerState; LUA_PLAYERS]],
    overlay_samples: &[Vec<SongLuaOverlayState>],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_overlays: &[SongLuaOverlayState],
) {
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        if seg_end <= seg_start {
            continue;
        }
        let from_players = player_samples[index];
        let to_players = player_samples
            .get(index + 1)
            .copied()
            .unwrap_or(from_players);
        push_perframe_player_targets(
            out_eases,
            seg_start,
            seg_end,
            &from_players,
            &to_players,
            baseline_players,
        );
        let from_overlays = &overlay_samples[index];
        let to_overlays = overlay_samples.get(index + 1).unwrap_or(from_overlays);
        push_perframe_overlay_targets(
            out_overlay_eases,
            seg_start,
            seg_end,
            from_overlays,
            to_overlays,
            baseline_overlays,
            false,
        );
    }
}

pub fn current_overlay_compile_actor_states<Kind>(
    overlays: &[SongLuaOverlayCompileActor<Kind>],
) -> Result<Vec<SongLuaOverlayState>, String> {
    let mut out = Vec::with_capacity(overlays.len());
    for overlay in overlays {
        out.push(actor_overlay_initial_state(&overlay.table)?);
    }
    Ok(out)
}

pub fn call_update_functions_at(
    lua: &Lua,
    root: &Value,
    beat: f32,
    delta_beats: f32,
    delta_seconds: f32,
) -> Result<(), String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let previous_delta = compile_song_runtime_delta_values(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, delta_beats, delta_seconds)
        .map_err(|err| err.to_string())?;
    let result = run_actor_update_functions_with_delta(lua, root, delta_seconds as f64)
        .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, previous_delta.0, previous_delta.1)
        .map_err(|err| err.to_string())?;
    result
}

pub fn compile_update_functions<Kind>(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
) -> Result<(Vec<SongLuaEaseWindow>, Vec<SongLuaOverlayEase>), String> {
    if !actor_tree_has_update_functions(lua, root).map_err(|err| err.to_string())? {
        return Ok((Vec::new(), Vec::new()));
    }
    let start = 0.0;
    let end = update_function_end_beat(context);
    if end <= start {
        return Ok((Vec::new(), Vec::new()));
    }

    let player_tables = tracked_player_tables(tracked_actors);
    let option_tables = update_player_option_tables(lua)?;
    reset_overlay_compile_actor_capture_tables(lua, overlays)?;
    reset_tracked_capture_tables(lua, tracked_actors)?;
    call_update_functions_at(lua, root, start, 0.0, 0.0)?;
    let baseline_players = current_perframe_player_states(&player_tables)?;
    let baseline_mods = current_update_mod_states(&option_tables)?;
    let baseline_overlays = current_overlay_compile_actor_states(overlays)?;
    let mut sample_beats = vec![start];
    let mut player_samples = vec![baseline_players];
    let mut mod_samples = vec![baseline_mods.clone()];
    let mut overlay_samples = vec![baseline_overlays.clone()];

    let coarse_step = update_function_sample_step(end - start);
    let fine_step = coarse_step / SONG_LUA_PLAYER_TRANSFORM_SAMPLE_DIVISOR;
    let mut beat = start;
    let mut transform_masks = player_transform_masks(&player_tables)?;
    while beat < end - f32::EPSILON {
        let next_beat = (beat
            + if transform_masks.iter().any(|mask| *mask != 0) {
                fine_step
            } else {
                coarse_step
            })
        .min(end);
        let delta_beats = next_beat - beat;
        let delta_seconds = perframe_delta_seconds(context, delta_beats);
        reset_overlay_compile_actor_capture_tables(lua, overlays)?;
        reset_tracked_capture_tables(lua, tracked_actors)?;
        call_update_functions_at(lua, root, next_beat, delta_beats, delta_seconds)?;
        let next_masks = player_transform_masks(&player_tables)?;
        let prior_active = if player_samples.len() >= 2 {
            player_samples[player_samples.len() - 2]
        } else {
            baseline_players
        };
        let mut next_players = current_perframe_player_states(&player_tables)?;
        for player in 0..LUA_PLAYERS {
            let ended = transform_masks[player] & !next_masks[player];
            if ended != 0 {
                if let Some(actor) = player_tables[player].as_ref() {
                    snap_ended_transforms(
                        actor,
                        &mut next_players[player],
                        prior_active[player],
                        baseline_players[player],
                        ended,
                    )?;
                }
            }
        }
        transform_masks = next_masks;
        sample_beats.push(next_beat);
        player_samples.push(next_players);
        mod_samples.push(current_update_mod_states(&option_tables)?);
        overlay_samples.push(current_overlay_compile_actor_states(overlays)?);
        beat = next_beat;
    }

    let mut eases = Vec::new();
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        if seg_end <= seg_start {
            continue;
        }
        let from_players = player_samples[index];
        let to_players = player_samples
            .get(index + 1)
            .copied()
            .unwrap_or(from_players);
        push_perframe_player_targets(
            &mut eases,
            seg_start,
            seg_end,
            &from_players,
            &to_players,
            &baseline_players,
        );
        let from_mods = &mod_samples[index];
        let to_mods = mod_samples.get(index + 1).unwrap_or(from_mods);
        push_update_mod_targets(
            &mut eases,
            seg_start,
            seg_end,
            from_mods,
            to_mods,
            &baseline_mods,
        );
    }
    let overlay_eases =
        update_function_overlay_eases(end, &baseline_overlays, &sample_beats, &overlay_samples);
    Ok((eases, overlay_eases))
}

pub fn compile_perframes<Kind>(
    lua: &Lua,
    prefix_table: Option<Table>,
    global_table: Option<Table>,
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
> {
    let mut entries = read_perframe_entries(prefix_table)?;
    entries.extend(read_perframe_entries(global_table)?);
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let boundaries = perframe_boundaries(&entries);
    if boundaries.len() < 2 {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let player_tables = tracked_player_tables(tracked_actors);
    let baseline_players = current_perframe_player_states(&player_tables)?;
    let baseline_overlays = current_overlay_compile_actor_states(overlays)?;
    let mut out_eases = Vec::new();
    let mut out_overlay_eases = Vec::new();
    let mut saw_recognized_side_effect = false;

    for window in boundaries.windows(2) {
        let [start, end] = [window[0], window[1]];
        if end <= start {
            continue;
        }
        let active = active_perframe_entries(&entries, start, end);
        if active.is_empty() {
            let current_players = current_perframe_player_states(&player_tables)?;
            let current_overlays = current_overlay_compile_actor_states(overlays)?;
            push_perframe_static_targets(
                &mut out_eases,
                &mut out_overlay_eases,
                start,
                end,
                &current_players,
                &current_overlays,
                &baseline_players,
                &baseline_overlays,
            );
            continue;
        }

        let mut sample_beats = Vec::new();
        let mut player_samples = Vec::new();
        let mut overlay_samples = Vec::new();
        for sample in perframe_samples(start, end) {
            let delta_seconds = perframe_delta_seconds(context, sample.delta_beats);
            reset_overlay_compile_actor_capture_tables(lua, overlays)?;
            reset_tracked_capture_tables(lua, tracked_actors)?;
            for entry in &active {
                saw_recognized_side_effect |= call_perframe_entry(
                    lua,
                    entry,
                    sample.eval_beat,
                    sample.delta_beats,
                    delta_seconds,
                )?;
            }
            sample_beats.push(sample.beat);
            player_samples.push(current_perframe_player_states(&player_tables)?);
            overlay_samples.push(current_overlay_compile_actor_states(overlays)?);
        }

        push_sampled_perframe_targets(
            &mut out_eases,
            &mut out_overlay_eases,
            end,
            &sample_beats,
            &player_samples,
            &overlay_samples,
            &baseline_players,
            &baseline_overlays,
        );
    }

    let mut info = SongLuaCompileInfo::default();
    if out_eases.is_empty() && out_overlay_eases.is_empty() && !saw_recognized_side_effect {
        info = unsupported_perframe_info(&entries);
    }
    Ok((out_eases, out_overlay_eases, info))
}
