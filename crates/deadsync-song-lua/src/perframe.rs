use mlua::{Function, Lua, Table, Value};
use std::collections::BTreeMap;
use std::time::Instant;

use crate::{
    LUA_PLAYERS, SONG_LUA_PLAYER_OPTIONS_KEYS, SongLuaColumnOffsetBuildParams,
    SongLuaColumnOffsetWindow, SongLuaCompileContext, SongLuaCompileInfo, SongLuaEaseTarget,
    SongLuaEaseWindow, SongLuaMessageEvent, SongLuaOverlayCompileActor, SongLuaOverlayEase,
    SongLuaOverlayState, SongLuaOverlayUpdateSample, SongLuaOverlayUpdateTarget,
    SongLuaOverlayUpdateTrack, SongLuaOverlayUpdateValue, SongLuaSpanMode,
    SongLuaStatefulMessageCapture, SongLuaTimeUnit, SongLuaTrackedActor, SongLuaTrackedActorTarget,
    actor_overlay_initial_state, actor_tree_has_update_functions,
    column_transform_windows_from_samples, compile_song_runtime_delta_values,
    compile_song_runtime_values, overlay_delta_pair_from_states, overlay_state_after_blocks,
    push_unique_compile_detail, read_f32, read_note_column_transform_samples, reset_actor_capture,
    reset_overlay_compile_actor_capture_tables, reset_tracked_capture_tables,
    runtime_player_option_ease_target, set_actor_overlay_getter_state,
    set_compile_song_runtime_beat, set_compile_song_runtime_delta_values,
    set_compile_song_runtime_values, song_beat_at_elapsed_seconds, song_display_bps,
    song_elapsed_seconds_at, song_elapsed_seconds_for_beat, song_lua_side_effect_count,
    song_music_rate,
};

pub const SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES: usize = 8192;
const SONG_LUA_UPDATE_REFERENCE_FPS: f32 = 60.0;

pub(crate) fn apply_startup_states<Kind>(
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    states: &std::collections::HashMap<usize, SongLuaOverlayState>,
    tracks: &mut [SongLuaOverlayUpdateTrack],
    messages: &mut Vec<SongLuaMessageEvent>,
) {
    const MESSAGE: &str = "__songlua_queued_startup";
    let beat = song_beat_at_elapsed_seconds(1.0 / SONG_LUA_UPDATE_REFERENCE_FPS, context);
    let mut changed = false;
    for (index, overlay) in overlays.iter_mut().enumerate() {
        let Some(&initial) = states.get(&(overlay.table.to_pointer() as usize)) else {
            continue;
        };
        let ready = overlay.actor.initial_state;
        let Some((_, delta)) = overlay_delta_pair_from_states(initial, ready, ready) else {
            continue;
        };
        overlay.actor.initial_state = initial;
        overlay
            .actor
            .message_commands
            .push(crate::SongLuaOverlayMessageCommand {
                message: MESSAGE.to_string(),
                aux: None,
                blocks: vec![crate::SongLuaOverlayCommandBlock {
                    start: 0.0,
                    duration: 0.0,
                    easing: None,
                    opt1: None,
                    opt2: None,
                    delta,
                }],
            });
        for track in tracks
            .iter_mut()
            .filter(|track| track.overlay_index == index)
        {
            // A zero-time update runs after queued setup during compilation.
            // Its samples must not overwrite the state before that setup.
            for sample in track
                .samples
                .iter_mut()
                .take_while(|sample| sample.beat < beat)
            {
                sample.beat = beat;
            }
        }
        changed = true;
    }
    if changed {
        messages.push(SongLuaMessageEvent {
            beat,
            message: MESSAGE.to_string(),
            persists: true,
        });
    }
}
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
    boundaries.sort_by(f32::total_cmp);
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

fn current_update_mod_speeds(
    tables: &[Table; LUA_PLAYERS],
) -> Result<[SongLuaUpdateModState; LUA_PLAYERS], String> {
    let mut speeds = std::array::from_fn(|_| SongLuaUpdateModState::new());
    for (out, table) in speeds.iter_mut().zip(tables) {
        if let Some(table) = table
            .raw_get::<Option<Table>>("__songlua_player_option_speeds")
            .map_err(|err| err.to_string())?
        {
            for pair in table.pairs::<String, f32>() {
                let (key, speed) = pair.map_err(|err| err.to_string())?;
                out.insert(key, speed);
            }
        }
    }
    Ok(speeds)
}

fn current_update_mod_states_with_note_columns(
    lua: &Lua,
    tables: &[Table; LUA_PLAYERS],
) -> Result<[SongLuaUpdateModState; LUA_PLAYERS], String> {
    let mut states = current_update_mod_states(tables)?;
    for (state, reverse) in states
        .iter_mut()
        .zip(crate::read_note_column_position_reverse_percents(lua)?)
    {
        if let Some(reverse) = reverse {
            state.insert("reverse".to_string(), reverse);
        }
    }
    Ok(states)
}

pub fn active_perframe_entries(
    entries: &[SongLuaPerframeEntry],
    start: f32,
    end: f32,
) -> Vec<&SongLuaPerframeEntry> {
    let mid = 0.5f32.mul_add(end - start, start);
    entries
        .iter()
        .filter(|entry| mid > entry.start && mid < entry.end)
        .collect()
}

#[inline(always)]
#[must_use]
pub fn perframe_segment_step(len: f32) -> f32 {
    (len / 96.0).clamp(1.0 / 192.0, 0.125)
}

#[inline(always)]
#[must_use]
pub fn perframe_delta_seconds(context: &SongLuaCompileContext, delta_beats: f32) -> f32 {
    song_elapsed_seconds_for_beat(
        delta_beats,
        song_display_bps(context),
        song_music_rate(context),
    )
}

#[inline(always)]
fn beat_span_seconds(context: &SongLuaCompileContext, start: f32, end: f32) -> f32 {
    (song_elapsed_seconds_at(end, context) - song_elapsed_seconds_at(start, context)).max(0.0)
}

#[inline(always)]
#[must_use]
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

#[must_use]
pub fn update_function_end_beat(context: &SongLuaCompileContext) -> f32 {
    song_beat_at_elapsed_seconds(context.music_length_seconds.max(0.0), context).max(0.0)
}

#[must_use]
pub fn update_function_sample_step(len: f32) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    (len / SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES as f32).max(1.0 / 192.0)
}

fn update_function_replay_beats(
    context: &SongLuaCompileContext,
    start: f32,
    end: f32,
) -> Vec<(f64, f64)> {
    let start_seconds = f64::from(song_elapsed_seconds_at(start, context));
    let end_seconds = f64::from(song_elapsed_seconds_at(end, context));
    let mut out = vec![(f64::from(start), 0.0)];
    let frame_count = ((end_seconds - start_seconds) * f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
        .ceil()
        .max(0.0) as usize;
    let mut previous_seconds = start_seconds;
    for frame in 1..=frame_count {
        let seconds = (start_seconds + frame as f64 / f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
            .min(end_seconds);
        out.push((
            crate::song_beat_at_seconds64(seconds, context),
            seconds - previous_seconds,
        ));
        previous_seconds = seconds;
    }
    out
}

#[must_use]
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

#[must_use]
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
        approach_speed: None,
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
    speeds: &[SongLuaUpdateModState; LUA_PLAYERS],
    last_windows: &mut BTreeMap<(usize, String), usize>,
) {
    for player in 0..LUA_PLAYERS {
        for (key, &from) in &from_players[player] {
            let key = key.as_str();
            let baseline = baseline_players[player].get(key).copied().unwrap_or(0.0);
            let to = to_players[player].get(key).copied().unwrap_or(baseline);
            let Some(target) = runtime_player_option_ease_target(key, key) else {
                continue;
            };
            if let Some(speed) = speeds[player].get(key).copied() {
                if !from.is_finite() || !speed.is_finite() {
                    continue;
                }
                let value = update_mod_runtime_value(key, from);
                let key = (player, key.to_string());
                if let Some(index) = last_windows.get(&key)
                    && let Some(window) = out.get_mut(*index)
                    && window.to == value
                    && window.approach_speed == Some(speed)
                {
                    window.limit = end - window.start;
                    continue;
                }
                last_windows.insert(key, out.len());
                // Song-level writes are step targets; Current approaches them
                // at the authored speed. Never tween toward a future write.
                out.push(SongLuaEaseWindow {
                    approach_speed: Some(speed),
                    unit: SongLuaTimeUnit::Beat,
                    start,
                    limit: end - start,
                    span_mode: SongLuaSpanMode::Len,
                    from: value,
                    to: value,
                    target,
                    easing: None,
                    player: Some(player as u8 + 1),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                });
                continue;
            }
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

fn push_update_overlay_value(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        usize,
    >,
    overlay_index: usize,
    target: crate::SongLuaOverlayUpdateTarget,
    beat: f32,
    current: crate::SongLuaOverlayUpdateValue,
    next_beat: f32,
    next: crate::SongLuaOverlayUpdateValue,
) {
    let key = (overlay_index, target);
    let track_index = match track_indices.get(&key).copied() {
        Some(index) => index,
        None => {
            let index = tracks.len();
            track_indices.insert(key, index);
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index,
                target,
                // A target enters capture only when UpdateCommand writes it.
                // Do not invent a ramp from the actor default before that first
                // write; the runtime applies the first sampled value as a step.
                samples: vec![SongLuaOverlayUpdateSample {
                    beat: next_beat,
                    value: next,
                }],
            });
            return;
        }
    };
    if current == next {
        return;
    }
    let track = &mut tracks[track_index];
    if track
        .samples
        .last()
        .is_some_and(|sample| sample.beat < beat - f32::EPSILON)
    {
        track.samples.push(SongLuaOverlayUpdateSample {
            beat,
            value: current,
        });
    }
    track.samples.push(SongLuaOverlayUpdateSample {
        beat: next_beat,
        value: next,
    });
}

fn overlay_state_update_value(
    state: &SongLuaOverlayState,
    target: crate::SongLuaOverlayUpdateTarget,
) -> crate::SongLuaOverlayUpdateValue {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value};
    macro_rules! value {
        ($variant:ident, $field:ident) => {
            Value::$variant(state.$field)
        };
    }
    macro_rules! option {
        ($variant:ident, $field:ident) => {
            state.$field.map(Value::$variant).unwrap_or(Value::None)
        };
    }
    match target {
        Target::X => value!(F32, x),
        Target::Y => value!(F32, y),
        Target::Z => value!(F32, z),
        Target::ZBias => value!(F32, z_bias),
        Target::DrawOrder => value!(I32, draw_order),
        Target::DrawByZPosition => value!(Bool, draw_by_z_position),
        Target::HAlign => value!(F32, halign),
        Target::VAlign => value!(F32, valign),
        Target::TextAlign => value!(TextAlign, text_align),
        Target::Uppercase => value!(Bool, uppercase),
        Target::ShadowLen => value!(Vec2, shadow_len),
        Target::ShadowColor => value!(Vec4, shadow_color),
        Target::Glow => value!(Vec4, glow),
        Target::Fov => option!(F32, fov),
        Target::Vanishpoint => option!(Vec2, vanishpoint),
        Target::Diffuse => value!(Vec4, diffuse),
        Target::VertexColors => state
            .vertex_colors
            .map(|value| Value::VertexColors(std::sync::Arc::new(value)))
            .unwrap_or(Value::None),
        Target::Visible => value!(Bool, visible),
        Target::CropLeft => value!(F32, cropleft),
        Target::CropRight => value!(F32, cropright),
        Target::CropTop => value!(F32, croptop),
        Target::CropBottom => value!(F32, cropbottom),
        Target::FadeLeft => value!(F32, fadeleft),
        Target::FadeRight => value!(F32, faderight),
        Target::FadeTop => value!(F32, fadetop),
        Target::FadeBottom => value!(F32, fadebottom),
        Target::MaskSource => value!(Bool, mask_source),
        Target::MaskDest => value!(Bool, mask_dest),
        Target::DepthTest => value!(Bool, depth_test),
        Target::Zoom => value!(F32, zoom),
        Target::ZoomX => value!(F32, zoom_x),
        Target::ZoomY => value!(F32, zoom_y),
        Target::ZoomZ => value!(F32, zoom_z),
        Target::BaseZoom => value!(F32, basezoom),
        Target::BaseZoomX => value!(F32, basezoom_x),
        Target::BaseZoomY => value!(F32, basezoom_y),
        Target::BaseZoomZ => value!(F32, basezoom_z),
        Target::RotationX => value!(F32, rot_x_deg),
        Target::RotationY => value!(F32, rot_y_deg),
        Target::RotationZ => value!(F32, rot_z_deg),
        Target::SkewX => value!(F32, skew_x),
        Target::SkewY => value!(F32, skew_y),
        Target::Blend => value!(Blend, blend),
        Target::Vibrate => value!(Bool, vibrate),
        Target::EffectMagnitude => value!(Vec3, effect_magnitude),
        Target::EffectClock => value!(EffectClock, effect_clock),
        Target::EffectMode => value!(EffectMode, effect_mode),
        Target::EffectColor1 => value!(Vec4, effect_color1),
        Target::EffectColor2 => value!(Vec4, effect_color2),
        Target::EffectPeriod => value!(F32, effect_period),
        Target::EffectOffset => value!(F32, effect_offset),
        Target::EffectTiming => option!(Vec5, effect_timing),
        Target::Rainbow => value!(Bool, rainbow),
        Target::RainbowScroll => value!(Bool, rainbow_scroll),
        Target::TextJitter => value!(Bool, text_jitter),
        Target::TextDistortion => value!(F32, text_distortion),
        Target::TextGlowMode => value!(TextGlowMode, text_glow_mode),
        Target::MultAttrsWithDiffuse => value!(Bool, mult_attrs_with_diffuse),
        Target::SpriteAnimate => value!(Bool, sprite_animate),
        Target::SpriteLoop => value!(Bool, sprite_loop),
        Target::SpritePlaybackRate => value!(F32, sprite_playback_rate),
        Target::SpriteStateDelay => value!(F32, sprite_state_delay),
        Target::SpriteStateIndex => option!(U32, sprite_state_index),
        Target::VertSpacing => option!(I32, vert_spacing),
        Target::WrapWidthPixels => option!(I32, wrap_width_pixels),
        Target::MaxWidth => option!(F32, max_width),
        Target::MaxHeight => option!(F32, max_height),
        Target::MaxWPreZoom => value!(Bool, max_w_pre_zoom),
        Target::MaxHPreZoom => value!(Bool, max_h_pre_zoom),
        Target::MaxDimensionUsesZoom => value!(Bool, max_dimension_uses_zoom),
        Target::TextureFiltering => value!(Bool, texture_filtering),
        Target::TextureWrapping => value!(Bool, texture_wrapping),
        Target::TexcoordOffset => option!(Vec2, texcoord_offset),
        Target::CustomTextureRect => option!(Vec4, custom_texture_rect),
        Target::TexcoordVelocity => option!(Vec2, texcoord_velocity),
        Target::Size => option!(Vec2, size),
        Target::StretchRect => option!(Vec4, stretch_rect),
    }
}

fn set_overlay_state_update_value(
    state: &mut SongLuaOverlayState,
    target: crate::SongLuaOverlayUpdateTarget,
    value: &crate::SongLuaOverlayUpdateValue,
) {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value};
    macro_rules! set_value {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                if let Value::$variant(value) = value {
                    state.$field = *value;
                }
                return;
            }
        };
    }
    macro_rules! set_option {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                state.$field = match value {
                    Value::$variant(value) => Some(*value),
                    Value::None => None,
                    _ => state.$field,
                };
                return;
            }
        };
    }
    set_value!(X, F32, x);
    set_value!(Y, F32, y);
    set_value!(Z, F32, z);
    set_value!(ZBias, F32, z_bias);
    set_value!(DrawOrder, I32, draw_order);
    set_value!(DrawByZPosition, Bool, draw_by_z_position);
    set_value!(HAlign, F32, halign);
    set_value!(VAlign, F32, valign);
    set_value!(TextAlign, TextAlign, text_align);
    set_value!(Uppercase, Bool, uppercase);
    set_value!(ShadowLen, Vec2, shadow_len);
    set_value!(ShadowColor, Vec4, shadow_color);
    set_value!(Glow, Vec4, glow);
    set_option!(Fov, F32, fov);
    set_option!(Vanishpoint, Vec2, vanishpoint);
    set_value!(Diffuse, Vec4, diffuse);
    if target == Target::VertexColors {
        state.vertex_colors = match value {
            Value::VertexColors(value) => Some(**value),
            Value::None => None,
            _ => state.vertex_colors,
        };
        return;
    }
    set_value!(Visible, Bool, visible);
    set_value!(CropLeft, F32, cropleft);
    set_value!(CropRight, F32, cropright);
    set_value!(CropTop, F32, croptop);
    set_value!(CropBottom, F32, cropbottom);
    set_value!(FadeLeft, F32, fadeleft);
    set_value!(FadeRight, F32, faderight);
    set_value!(FadeTop, F32, fadetop);
    set_value!(FadeBottom, F32, fadebottom);
    set_value!(MaskSource, Bool, mask_source);
    set_value!(MaskDest, Bool, mask_dest);
    set_value!(DepthTest, Bool, depth_test);
    set_value!(Zoom, F32, zoom);
    set_value!(ZoomX, F32, zoom_x);
    set_value!(ZoomY, F32, zoom_y);
    set_value!(ZoomZ, F32, zoom_z);
    set_value!(BaseZoom, F32, basezoom);
    set_value!(BaseZoomX, F32, basezoom_x);
    set_value!(BaseZoomY, F32, basezoom_y);
    set_value!(BaseZoomZ, F32, basezoom_z);
    set_value!(RotationX, F32, rot_x_deg);
    set_value!(RotationY, F32, rot_y_deg);
    set_value!(RotationZ, F32, rot_z_deg);
    set_value!(SkewX, F32, skew_x);
    set_value!(SkewY, F32, skew_y);
    set_value!(Blend, Blend, blend);
    set_value!(Vibrate, Bool, vibrate);
    set_value!(EffectMagnitude, Vec3, effect_magnitude);
    set_value!(EffectClock, EffectClock, effect_clock);
    set_value!(EffectMode, EffectMode, effect_mode);
    set_value!(EffectColor1, Vec4, effect_color1);
    set_value!(EffectColor2, Vec4, effect_color2);
    set_value!(EffectPeriod, F32, effect_period);
    set_value!(EffectOffset, F32, effect_offset);
    set_option!(EffectTiming, Vec5, effect_timing);
    set_value!(Rainbow, Bool, rainbow);
    set_value!(RainbowScroll, Bool, rainbow_scroll);
    set_value!(TextJitter, Bool, text_jitter);
    set_value!(TextDistortion, F32, text_distortion);
    set_value!(TextGlowMode, TextGlowMode, text_glow_mode);
    set_value!(MultAttrsWithDiffuse, Bool, mult_attrs_with_diffuse);
    set_value!(SpriteAnimate, Bool, sprite_animate);
    set_value!(SpriteLoop, Bool, sprite_loop);
    set_value!(SpritePlaybackRate, F32, sprite_playback_rate);
    set_value!(SpriteStateDelay, F32, sprite_state_delay);
    set_option!(SpriteStateIndex, U32, sprite_state_index);
    set_option!(VertSpacing, I32, vert_spacing);
    set_option!(WrapWidthPixels, I32, wrap_width_pixels);
    set_option!(MaxWidth, F32, max_width);
    set_option!(MaxHeight, F32, max_height);
    set_value!(MaxWPreZoom, Bool, max_w_pre_zoom);
    set_value!(MaxHPreZoom, Bool, max_h_pre_zoom);
    set_value!(MaxDimensionUsesZoom, Bool, max_dimension_uses_zoom);
    set_value!(TextureFiltering, Bool, texture_filtering);
    set_value!(TextureWrapping, Bool, texture_wrapping);
    set_option!(TexcoordOffset, Vec2, texcoord_offset);
    set_option!(CustomTextureRect, Vec4, custom_texture_rect);
    set_option!(TexcoordVelocity, Vec2, texcoord_velocity);
    set_option!(Size, Vec2, size);
    set_option!(StretchRect, Vec4, stretch_rect);
}

fn capture_update_overlay_samples<Kind>(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    baseline: &[SongLuaOverlayState],
    from_states: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    restored_indices: &[usize],
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        usize,
    >,
    beat: f32,
    next_beat: f32,
    next_seconds: f64,
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
) -> Result<(), String> {
    merge_completed_scheduled_overlay_samples(
        tracks,
        track_indices,
        baseline,
        update_states,
        to_states,
        scheduled_samples,
        next_beat,
    );
    let mut reset_indices = Vec::new();
    let mut captured_targets = Vec::new();
    crate::lua_util::drain_overlay_update_capture(
        lua,
        |overlay_index, values, scheduled, final_values| {
            let Some(baseline) = baseline.get(overlay_index) else {
                return Ok(());
            };
            debug_assert!(overlay_index < overlays.len());
            reset_indices.push(overlay_index);
            for (target, value) in final_values {
                if scheduled.iter().any(|update| update.target == *target) {
                    continue;
                }
                if let Some(state) = update_states.get_mut(overlay_index) {
                    set_overlay_state_update_value(state, *target, value);
                }
                if restored_indices.binary_search(&overlay_index).is_err()
                    && let Some(state) = to_states.get_mut(overlay_index)
                {
                    set_overlay_state_update_value(state, *target, value);
                }
            }
            for (target, next) in values {
                captured_targets.push((overlay_index, *target));
                let current = from_states
                    .get(overlay_index)
                    .map(|state| overlay_state_update_value(state, *target))
                    .unwrap_or_else(|| overlay_state_update_value(baseline, *target));
                push_update_overlay_value(
                    tracks,
                    track_indices,
                    overlay_index,
                    *target,
                    beat,
                    current,
                    next_beat,
                    next.clone(),
                );
            }
            let message_seconds = next_seconds;
            let mut scheduled_values = std::collections::HashMap::new();
            for update in scheduled {
                let from = scheduled_values
                    .get(&update.target)
                    .cloned()
                    .or_else(|| {
                        update_states
                            .get(overlay_index)
                            .map(|state| overlay_state_update_value(state, update.target))
                    })
                    .unwrap_or(SongLuaOverlayUpdateValue::None);
                scheduled_samples.push(SongLuaScheduledOverlaySample {
                    overlay_index,
                    target: update.target,
                    start_seconds: message_seconds + f64::from(update.delay_seconds),
                    end_seconds: message_seconds
                        + f64::from(update.delay_seconds)
                        + f64::from(update.duration_seconds),
                    start_beat: song_beat_at_elapsed_seconds(
                        (message_seconds + f64::from(update.delay_seconds)) as f32,
                        context,
                    ),
                    end_beat: song_beat_at_elapsed_seconds(
                        (message_seconds
                            + f64::from(update.delay_seconds)
                            + f64::from(update.duration_seconds)) as f32,
                        context,
                    ),
                    easing: update.easing.clone(),
                    opt1: update.opt1,
                    from,
                    value: update.value.clone(),
                });
                scheduled_values.insert(update.target, update.value.clone());
            }
            Ok(())
        },
    )?;
    let message_targets = track_indices
        .iter()
        .filter(|(target, _)| !captured_targets.contains(target))
        .filter_map(|(&(overlay_index, target), &track_index)| {
            let current = from_states
                .get(overlay_index)
                .map(|state| overlay_state_update_value(state, target))?;
            let message = to_states
                .get(overlay_index)
                .map(|state| overlay_state_update_value(state, target))?;
            let tracked = tracks
                .get(track_index)
                .and_then(|track| track.samples.last())
                .map(|sample| &sample.value)?;
            (*tracked != message).then_some((overlay_index, target, current, message))
        })
        .collect::<Vec<_>>();
    // Runtime update tracks are applied after message commands. Keep an existing
    // track synchronized when a message changes its target, otherwise a stale
    // sampled value can overwrite a later persistent action (notably Player
    // ActorProxy visibility in Step Your Game Up).
    for (overlay_index, target, current, message) in message_targets {
        push_update_overlay_value(
            tracks,
            track_indices,
            overlay_index,
            target,
            beat,
            current,
            next_beat,
            message,
        );
    }
    for overlay_index in reset_indices {
        reset_actor_capture(lua, &overlays[overlay_index].table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

struct SongLuaScheduledOverlaySample {
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    start_seconds: f64,
    end_seconds: f64,
    start_beat: f32,
    end_beat: f32,
    easing: Option<String>,
    opt1: Option<f32>,
    from: SongLuaOverlayUpdateValue,
    value: SongLuaOverlayUpdateValue,
}

fn lerp_scheduled_value(
    from: &SongLuaOverlayUpdateValue,
    to: &SongLuaOverlayUpdateValue,
    factor: f32,
) -> SongLuaOverlayUpdateValue {
    use SongLuaOverlayUpdateValue as Value;
    let lerp = |from: f32, to: f32| (to - from).mul_add(factor, from);
    match (from, to) {
        (Value::F32(from), Value::F32(to)) => Value::F32(lerp(*from, *to)),
        (Value::Vec2(from), Value::Vec2(to)) => {
            Value::Vec2([lerp(from[0], to[0]), lerp(from[1], to[1])])
        }
        (Value::Vec3(from), Value::Vec3(to)) => Value::Vec3([
            lerp(from[0], to[0]),
            lerp(from[1], to[1]),
            lerp(from[2], to[2]),
        ]),
        (Value::Vec4(from), Value::Vec4(to)) => Value::Vec4([
            lerp(from[0], to[0]),
            lerp(from[1], to[1]),
            lerp(from[2], to[2]),
            lerp(from[3], to[3]),
        ]),
        _ => {
            if factor >= 1.0 {
                to.clone()
            } else {
                from.clone()
            }
        }
    }
}

fn apply_scheduled_overlay_states<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    states: &mut [SongLuaOverlayState],
    scheduled: &[SongLuaScheduledOverlaySample],
    seconds: f64,
) -> Result<(), String> {
    for sample in scheduled {
        if seconds + f64::EPSILON < sample.start_seconds {
            continue;
        }
        let linear_factor = if sample.end_seconds <= sample.start_seconds + f64::EPSILON {
            1.0
        } else {
            ((seconds - sample.start_seconds) / (sample.end_seconds - sample.start_seconds))
                .clamp(0.0, 1.0) as f32
        };
        let factor = crate::overlay_command_ease_factor(
            sample.easing.as_deref(),
            linear_factor,
            sample.opt1,
        );
        let Some(state) = states.get_mut(sample.overlay_index) else {
            continue;
        };
        let value = lerp_scheduled_value(&sample.from, &sample.value, factor);
        set_overlay_state_update_value(state, sample.target, &value);
        if let Some(overlay) = overlays.get(sample.overlay_index) {
            crate::lua_util::set_actor_overlay_update_getter_value(
                lua,
                &overlay.table,
                sample.target,
                &value,
            )?;
        }
    }
    Ok(())
}

fn merge_completed_scheduled_overlay_samples(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    baseline: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    beat: f32,
) {
    if scheduled.is_empty() {
        return;
    }
    let (mut completed, pending) = std::mem::take(scheduled)
        .into_iter()
        .partition(|sample| sample.end_beat <= beat + f32::EPSILON);
    *scheduled = pending;
    if !completed.is_empty() {
        completed.sort_by(|left, right| left.end_beat.total_cmp(&right.end_beat));
        for sample in &completed {
            if let Some(state) = update_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
            if let Some(state) = to_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
        }
        merge_scheduled_overlay_samples(tracks, track_indices, baseline, completed);
    }
}

fn sort_overlay_update_samples(samples: &mut Vec<SongLuaOverlayUpdateSample>) {
    samples.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    let mut merged: Vec<SongLuaOverlayUpdateSample> = Vec::with_capacity(samples.len());
    for sample in samples.drain(..) {
        if let Some(last) = merged.last_mut()
            && (last.beat - sample.beat).abs() <= f32::EPSILON
        {
            *last = sample;
        } else {
            merged.push(sample);
        }
    }
    *samples = merged;
}

fn merge_scheduled_overlay_samples(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    baseline: &[SongLuaOverlayState],
    mut scheduled: Vec<SongLuaScheduledOverlaySample>,
) {
    for track in tracks.iter_mut() {
        sort_overlay_update_samples(&mut track.samples);
    }
    scheduled.sort_by(|left, right| left.start_beat.total_cmp(&right.start_beat));
    for sample in scheduled {
        let track_index = *track_indices
            .entry((sample.overlay_index, sample.target))
            .or_insert_with(|| {
                let index = tracks.len();
                tracks.push(SongLuaOverlayUpdateTrack {
                    overlay_index: sample.overlay_index,
                    target: sample.target,
                    samples: vec![SongLuaOverlayUpdateSample {
                        beat: 0.0,
                        value: overlay_state_update_value(
                            &baseline[sample.overlay_index],
                            sample.target,
                        ),
                    }],
                });
                index
            });
        let track = &mut tracks[track_index];
        let current = track
            .samples
            .iter()
            .rev()
            .find(|current| current.beat <= sample.start_beat + f32::EPSILON)
            .map(|current| current.value.clone())
            .unwrap_or_else(|| {
                overlay_state_update_value(&baseline[sample.overlay_index], sample.target)
            });
        if sample.end_beat > sample.start_beat + f32::EPSILON {
            track.samples.push(SongLuaOverlayUpdateSample {
                beat: sample.start_beat,
                value: current,
            });
        }
        track.samples.push(SongLuaOverlayUpdateSample {
            beat: sample.end_beat,
            value: sample.value,
        });
        sort_overlay_update_samples(&mut track.samples);
    }
    for track in tracks {
        sort_overlay_update_samples(&mut track.samples);
    }
}

pub fn call_update_functions_at(
    lua: &Lua,
    root: &Value,
    beat: f64,
    seconds: f64,
    delta_beats: f32,
    delta_seconds: f64,
) -> Result<(), String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let previous_delta = compile_song_runtime_delta_values(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat as f32).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, delta_beats, delta_seconds as f32)
        .map_err(|err| err.to_string())?;
    let runtime = lua
        .globals()
        .get::<Table>(crate::SONG_LUA_RUNTIME_KEY)
        .map_err(|err| err.to_string())?;
    runtime
        .set(crate::SONG_LUA_RUNTIME_BEAT_KEY, beat)
        .map_err(|err| err.to_string())?;
    runtime
        .set(crate::SONG_LUA_RUNTIME_SECONDS_KEY, seconds)
        .map_err(|err| err.to_string())?;
    let result =
        crate::lua_util::run_actor_compile_update_functions_with_delta(lua, root, delta_seconds)
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
    messages: &[SongLuaMessageEvent],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        Vec<SongLuaOverlayUpdateTrack>,
        Vec<SongLuaColumnOffsetWindow>,
        Vec<SongLuaStatefulMessageCapture>,
        Vec<(f32, String, bool)>,
    ),
    String,
> {
    let profile = std::env::var_os("DEADSYNC_SONG_LUA_TIMING_STDERR").is_some();
    let mut reset_ms = 0.0;
    let mut message_ms = 0.0;
    let mut update_ms = 0.0;
    let mut player_ms = 0.0;
    let mut mod_ms = 0.0;
    let mut column_ms = 0.0;
    let mut overlay_ms = 0.0;
    if !actor_tree_has_update_functions(lua, root).map_err(|err| err.to_string())? {
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }
    let start = 0.0;
    let end = update_function_end_beat(context);
    if end <= start {
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }

    let player_tables = tracked_player_tables(tracked_actors);
    let option_tables = update_player_option_tables(lua)?;
    reset_overlay_compile_actor_capture_tables(lua, overlays)?;
    reset_tracked_capture_tables(lua, tracked_actors)?;
    let baseline_overlays = current_overlay_compile_actor_states(overlays)?;
    let overlay_indices_by_pointer = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| (overlay.table.to_pointer() as usize, index))
        .collect::<std::collections::HashMap<_, _>>();
    crate::lua_util::begin_overlay_update_capture(lua, overlay_indices_by_pointer);
    let mut message_replay = SongLuaPerframeMessageReplay::new(messages, overlays.len());
    let mut replay_overlays = baseline_overlays.clone();
    let started = message_replay.advance(lua, context, overlays, &mut replay_overlays, start)?;
    restore_started_message_states(lua, overlays, &replay_overlays, &started)?;
    let mut update_overlays = replay_overlays.clone();
    let baseline_players = current_perframe_player_states(&player_tables)?;
    let baseline_mods = current_update_mod_states_with_note_columns(lua, &option_tables)?;
    let baseline_columns = read_note_column_transform_samples(lua)?;
    let mut sample_beats = vec![start];
    let mut player_samples = vec![baseline_players];
    let mut mod_samples = vec![baseline_mods.clone()];
    let mut mod_speed_samples = vec![current_update_mod_speeds(&option_tables)?];
    let mut column_samples = vec![baseline_columns];
    let mut overlay_tracks = Vec::new();
    let mut overlay_track_indices = std::collections::HashMap::new();
    let mut scheduled_overlay_samples = Vec::new();
    let mut current_overlays = replay_overlays;
    capture_update_overlay_samples(
        lua,
        context,
        overlays,
        &baseline_overlays,
        &baseline_overlays,
        &mut update_overlays,
        &mut current_overlays,
        &started,
        &mut overlay_tracks,
        &mut overlay_track_indices,
        start,
        start,
        f64::from(song_elapsed_seconds_at(start, context)),
        &mut scheduled_overlay_samples,
    )?;

    let replay = update_function_replay_beats(context, start, end);
    let mut beat = start;
    let mut seconds = f64::from(song_elapsed_seconds_at(start, context));
    let mut scheduled_states = baseline_overlays.clone();
    let mut transform_masks = player_transform_masks(&player_tables)?;
    let mut frame_count = 0;
    for (exact_beat, delta_seconds) in replay.into_iter().skip(1) {
        let next_beat = exact_beat as f32;
        frame_count += 1;
        let delta_beats = next_beat - beat;
        seconds += delta_seconds;
        let stage = profile.then(Instant::now);
        reset_tracked_capture_tables(lua, tracked_actors)?;
        reset_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        let mut replay_overlays = current_overlays.clone();
        let started =
            message_replay.advance(lua, context, overlays, &mut replay_overlays, next_beat)?;
        message_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        apply_scheduled_overlay_states(
            lua,
            overlays,
            &mut scheduled_states,
            &scheduled_overlay_samples,
            seconds,
        )?;
        call_update_functions_at(lua, root, exact_beat, seconds, delta_beats, delta_seconds)?;
        update_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        restore_started_message_states(lua, overlays, &replay_overlays, &started)?;
        let mut update_overlays = replay_overlays.clone();
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
        player_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        mod_samples.push(current_update_mod_states_with_note_columns(
            lua,
            &option_tables,
        )?);
        mod_speed_samples.push(current_update_mod_speeds(&option_tables)?);
        mod_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        column_samples.push(read_note_column_transform_samples(lua)?);
        column_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        let mut next_overlays = replay_overlays;
        capture_update_overlay_samples(
            lua,
            context,
            overlays,
            &baseline_overlays,
            &current_overlays,
            &mut update_overlays,
            &mut next_overlays,
            &started,
            &mut overlay_tracks,
            &mut overlay_track_indices,
            beat,
            next_beat,
            seconds,
            &mut scheduled_overlay_samples,
        )?;
        overlay_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        current_overlays = next_overlays;
        beat = next_beat;
    }
    if profile {
        eprintln!(
            "Song lua update timing: frames={frame_count} reset_ms={reset_ms:.3} message_ms={message_ms:.3} update_ms={update_ms:.3} player_ms={player_ms:.3} mod_ms={mod_ms:.3} column_ms={column_ms:.3} overlay_ms={overlay_ms:.3}"
        );
    }

    let mut eases = Vec::new();
    let mut column_transforms = Vec::new();
    let mut last_mod_windows = BTreeMap::new();
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
            &mod_speed_samples[index],
            &mut last_mod_windows,
        );
        let from_columns = &column_samples[index];
        let to_columns = column_samples.get(index + 1).unwrap_or(from_columns);
        column_transforms.extend(column_transform_windows_from_samples(
            from_columns,
            to_columns,
            SongLuaColumnOffsetBuildParams {
                unit: SongLuaTimeUnit::Beat,
                start: seg_start,
                limit: seg_end - seg_start,
                span_mode: SongLuaSpanMode::Len,
                easing: None,
                sustain: None,
                opt1: None,
                opt2: None,
            },
        ));
    }
    merge_scheduled_overlay_samples(
        &mut overlay_tracks,
        &mut overlay_track_indices,
        &baseline_overlays,
        scheduled_overlay_samples,
    );
    let stateful_messages = crate::lua_util::stateful_message_captures(lua);
    let runtime_broadcasts = crate::lua_util::runtime_broadcast_captures(lua);
    crate::lua_util::end_overlay_update_capture(lua);
    Ok((
        eases,
        Vec::new(),
        overlay_tracks,
        column_transforms,
        stateful_messages,
        runtime_broadcasts,
    ))
}

#[derive(Clone, Copy)]
struct SongLuaPerframeActiveMessage {
    command_index: usize,
    start_beat: f32,
    base: SongLuaOverlayState,
}

struct SongLuaPerframeMessageReplay<'a> {
    messages: &'a [SongLuaMessageEvent],
    order: Vec<usize>,
    next: usize,
    active: Vec<Option<SongLuaPerframeActiveMessage>>,
}

impl<'a> SongLuaPerframeMessageReplay<'a> {
    fn new(messages: &'a [SongLuaMessageEvent], overlay_count: usize) -> Self {
        let mut order = (0..messages.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| messages[a].beat.total_cmp(&messages[b].beat));
        Self {
            messages,
            order,
            next: 0,
            active: vec![None; overlay_count],
        }
    }

    fn advance<Kind>(
        &mut self,
        lua: &Lua,
        context: &SongLuaCompileContext,
        overlays: &mut [SongLuaOverlayCompileActor<Kind>],
        states: &mut [SongLuaOverlayState],
        beat: f32,
    ) -> Result<Vec<usize>, String> {
        let mut started = Vec::new();
        while let Some(&event_index) = self.order.get(self.next) {
            let event = &self.messages[event_index];
            let event_beat = event.beat;
            if event_beat > beat + f32::EPSILON {
                break;
            }
            for (overlay_index, overlay) in overlays.iter_mut().enumerate() {
                let was_active = self.active[overlay_index].is_some();
                apply_perframe_active_message(
                    lua,
                    context,
                    overlay,
                    &mut self.active[overlay_index],
                    event_beat,
                )?;
                if was_active && let Some(state) = states.get_mut(overlay_index) {
                    *state = actor_overlay_initial_state(&overlay.table)?;
                }
                let Some(command_index) = overlay
                    .actor
                    .message_commands
                    .iter()
                    .position(|command| command.message == event.message)
                else {
                    continue;
                };
                if let Some(aux) = overlay.actor.message_commands[command_index].aux {
                    overlay
                        .table
                        .set("__songlua_aux", aux)
                        .map_err(|err| err.to_string())?;
                }
                let base = match states.get(overlay_index) {
                    Some(state) => *state,
                    None => actor_overlay_initial_state(&overlay.table)?,
                };
                self.active[overlay_index] = Some(SongLuaPerframeActiveMessage {
                    command_index,
                    start_beat: event_beat,
                    base,
                });
                started.push(overlay_index);
                apply_perframe_active_message(
                    lua,
                    context,
                    overlay,
                    &mut self.active[overlay_index],
                    event_beat,
                )?;
                if let Some(state) = states.get_mut(overlay_index) {
                    *state = actor_overlay_initial_state(&overlay.table)?;
                }
            }
            self.next += 1;
        }
        for (overlay_index, overlay) in overlays.iter_mut().enumerate() {
            let was_active = self.active[overlay_index].is_some();
            apply_perframe_active_message(
                lua,
                context,
                overlay,
                &mut self.active[overlay_index],
                beat,
            )?;
            if was_active && let Some(state) = states.get_mut(overlay_index) {
                *state = actor_overlay_initial_state(&overlay.table)?;
            }
        }
        started.sort_unstable();
        started.dedup();
        Ok(started)
    }
}

fn restore_started_message_states<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    states: &[SongLuaOverlayState],
    started: &[usize],
) -> Result<(), String> {
    for &index in started {
        let (Some(overlay), Some(state)) = (overlays.get(index), states.get(index)) else {
            continue;
        };
        set_actor_overlay_getter_state(lua, &overlay.table, *state)?;
    }
    Ok(())
}

fn apply_perframe_active_message<Kind>(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlay: &SongLuaOverlayCompileActor<Kind>,
    active: &mut Option<SongLuaPerframeActiveMessage>,
    beat: f32,
) -> Result<(), String> {
    let Some(message) = *active else {
        return Ok(());
    };
    let Some(command) = overlay.actor.message_commands.get(message.command_index) else {
        *active = None;
        return Ok(());
    };
    let elapsed = beat_span_seconds(context, message.start_beat, beat);
    let state = overlay_state_after_blocks(message.base, &command.blocks, elapsed);
    set_actor_overlay_getter_state(lua, &overlay.table, state)?;
    let duration = command
        .blocks
        .iter()
        .map(|block| block.start + block.duration.max(0.0))
        .fold(0.0_f32, f32::max);
    if elapsed >= duration {
        *active = None;
    }
    Ok(())
}

pub fn compile_perframes<Kind>(
    lua: &Lua,
    prefix_table: Option<Table>,
    global_table: Option<Table>,
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
    messages: &[SongLuaMessageEvent],
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
    let mut message_replay = SongLuaPerframeMessageReplay::new(messages, overlays.len());
    let mut message_states = baseline_overlays.clone();

    for window in boundaries.windows(2) {
        let [start, end] = [window[0], window[1]];
        if end <= start {
            continue;
        }
        let active = active_perframe_entries(&entries, start, end);
        if active.is_empty() {
            let current_players = current_perframe_player_states(&player_tables)?;
            let current_overlays = current_overlay_compile_actor_states(overlays)?;
            message_states.clone_from(&current_overlays);
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
            let delta_seconds = beat_span_seconds(
                context,
                sample.eval_beat - sample.delta_beats,
                sample.eval_beat,
            );
            let _ = message_replay.advance(
                lua,
                context,
                overlays,
                &mut message_states,
                sample.eval_beat,
            )?;
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
            let current_overlays = current_overlay_compile_actor_states(overlays)?;
            message_states.clone_from(&current_overlays);
            overlay_samples.push(current_overlays);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_update_tweens_persist_into_following_state() {
        let baseline = vec![SongLuaOverlayState::default()];
        let mut update_states = baseline.clone();
        let mut next_states = baseline.clone();
        let mut tracks = Vec::new();
        let mut track_indices = std::collections::HashMap::new();
        let mut scheduled = vec![SongLuaScheduledOverlaySample {
            overlay_index: 0,
            target: SongLuaOverlayUpdateTarget::X,
            start_seconds: 1.0,
            end_seconds: 2.0,
            start_beat: 1.0,
            end_beat: 2.0,
            easing: Some("accelerate".to_owned()),
            opt1: None,
            from: SongLuaOverlayUpdateValue::F32(0.0),
            value: SongLuaOverlayUpdateValue::F32(427.0),
        }];

        merge_completed_scheduled_overlay_samples(
            &mut tracks,
            &mut track_indices,
            &baseline,
            &mut update_states,
            &mut next_states,
            &mut scheduled,
            2.0,
        );

        assert!(scheduled.is_empty());
        assert_eq!(update_states[0].x, 427.0);
        assert_eq!(next_states[0].x, 427.0);
        assert!(tracks[0].samples.last().is_some_and(|sample| {
            sample.beat == 2.0 && sample.value == SongLuaOverlayUpdateValue::F32(427.0)
        }));
    }
}
