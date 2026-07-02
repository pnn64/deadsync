use mlua::{Function, Lua, Table, Value};

use crate::{
    LUA_PLAYERS, SongLuaCompileContext, SongLuaCompileInfo, SongLuaEaseTarget, SongLuaEaseWindow,
    SongLuaOverlayCompileActor, SongLuaOverlayEase, SongLuaOverlayState, SongLuaSpanMode,
    SongLuaTimeUnit, SongLuaTrackedActor, SongLuaTrackedActorTarget, actor_overlay_initial_state,
    actor_tree_has_update_functions, compile_song_runtime_delta_values,
    compile_song_runtime_values, overlay_delta_pair_from_states, push_unique_compile_detail,
    read_f32, reset_overlay_compile_actor_capture_tables, reset_tracked_capture_tables,
    run_actor_update_functions_with_delta, set_compile_song_runtime_beat,
    set_compile_song_runtime_delta_values, set_compile_song_runtime_values, song_display_bps,
    song_elapsed_seconds_for_beat, song_lua_side_effect_count, song_music_rate,
};

pub const SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES: usize = 4096;

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

pub fn compile_update_function_overlays<Kind>(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
) -> Result<Vec<SongLuaOverlayEase>, String> {
    if overlays.is_empty()
        || !actor_tree_has_update_functions(lua, root).map_err(|err| err.to_string())?
    {
        return Ok(Vec::new());
    }
    let start = 0.0;
    let end = update_function_end_beat(context);
    if end <= start {
        return Ok(Vec::new());
    }

    reset_overlay_compile_actor_capture_tables(lua, overlays)?;
    reset_tracked_capture_tables(lua, tracked_actors)?;
    call_update_functions_at(lua, root, start, 0.0, 0.0)?;
    let baseline_overlays = current_overlay_compile_actor_states(overlays)?;
    let mut sample_beats = vec![start];
    let mut overlay_samples = vec![baseline_overlays.clone()];

    for sample in update_function_samples(start, end) {
        let delta_seconds = perframe_delta_seconds(context, sample.delta_beats);
        reset_overlay_compile_actor_capture_tables(lua, overlays)?;
        reset_tracked_capture_tables(lua, tracked_actors)?;
        call_update_functions_at(
            lua,
            root,
            sample.eval_beat,
            sample.delta_beats,
            delta_seconds,
        )?;
        sample_beats.push(sample.beat);
        overlay_samples.push(current_overlay_compile_actor_states(overlays)?);
    }

    Ok(update_function_overlay_eases(
        end,
        &baseline_overlays,
        &sample_beats,
        &overlay_samples,
    ))
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
