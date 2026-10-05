// Frozen from fddc06faa (0.5.1625); only dispatcher redirected to its frozen baseline.
use super::*;
use crate::perframe::player_snap_baseline::snap_ended_transforms;

pub(super) fn compile_update_functions<Kind>(
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
    restore_started_message_states(lua, overlays, &replay_overlays, started)?;
    let mut update_overlays = replay_overlays.clone();
    let baseline_players = current_perframe_player_states(&player_tables)?;
    let mut mod_scratch = ModSnapshotScratch::default();
    let baseline_mods = mod_scratch.states(&option_tables)?;
    let baseline_columns = read_note_column_transform_samples(lua)?;
    let replay = update_function_replay_beats(context, start, end);
    let sample_count = replay.len();
    let mut sample_beats = frame_buffer(start, sample_count);
    let rate = f64::from(song_music_rate(context));
    let mut sample_seconds = frame_buffer(
        (f64::from(song_elapsed_seconds_at(start, context)) * rate) as f32,
        sample_count,
    );
    let fallback_bpms = [(0.0, song_display_bps(context) * 60.0)];
    let bpms = if context.song_timing_bpms.is_empty() {
        fallback_bpms.as_slice()
    } else {
        &context.song_timing_bpms
    };
    // Pauses, warps and split chart timing need the player's beat conversion.
    // A matching continuous clock can retain the exact sampled frame instead.
    let use_mod_clock = context
        .player_timing
        .iter()
        .flatten()
        .all(|timing| timing.matches_bpm_clock(bpms));
    let mut player_samples = frame_buffer(baseline_players, sample_count);
    let mut mod_samples = frame_buffer(baseline_mods.clone(), sample_count);
    let mut mod_speed_samples = frame_buffer(
        mod_scratch.player_speeds(lua, &option_tables)?,
        sample_count,
    );
    let mut column_samples = frame_buffer(baseline_columns, sample_count);
    let mut overlay_tracks = Vec::new();
    // Keys are compiler-owned actor indices and target enum discriminants.
    let mut overlay_track_indices = FxHashMap::default();
    let mut scheduled_overlay_samples = Vec::new();
    let mut overlay_sample_scratch = OverlaySampleScratch::default();
    let mut current_overlays = replay_overlays.clone();
    capture_update_overlay_samples(
        lua,
        context,
        overlays,
        &baseline_overlays,
        &baseline_overlays,
        &mut update_overlays,
        &mut current_overlays,
        started,
        &mut overlay_tracks,
        &mut overlay_track_indices,
        start,
        start,
        f64::from(song_elapsed_seconds_at(start, context)),
        &mut scheduled_overlay_samples,
        &mut overlay_sample_scratch,
    )?;

    let mut beat = start;
    let mut seconds = f64::from(song_elapsed_seconds_at(start, context));
    let mut scheduled_states = baseline_overlays.clone();
    let mut transform_masks = player_transform_masks(lua, &player_tables)?;
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
        // Overlay count is fixed for this compiler run. Reuse the three state
        // buffers while keeping prior, message-replayed and updated states apart.
        replay_overlays.copy_from_slice(&current_overlays);
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
        let actor_delta = f64::from(seconds as f32 - (seconds - delta_seconds) as f32);
        call_update_functions_at(lua, root, exact_beat, seconds, delta_beats, actor_delta)?;
        update_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        restore_started_message_states(lua, overlays, &replay_overlays, started)?;
        update_overlays.copy_from_slice(&replay_overlays);
        let next_masks = player_transform_masks(lua, &player_tables)?;
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
        sample_seconds.push((seconds * rate) as f32);
        player_samples.push(next_players);
        player_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        mod_samples.push(mod_scratch.states(&option_tables)?);
        mod_speed_samples.push(mod_scratch.player_speeds(lua, &option_tables)?);
        mod_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        column_samples.push(read_note_column_transform_samples(lua)?);
        column_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        capture_update_overlay_samples(
            lua,
            context,
            overlays,
            &baseline_overlays,
            &current_overlays,
            &mut update_overlays,
            &mut replay_overlays,
            started,
            &mut overlay_tracks,
            &mut overlay_track_indices,
            beat,
            next_beat,
            seconds,
            &mut scheduled_overlay_samples,
            &mut overlay_sample_scratch,
        )?;
        overlay_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        std::mem::swap(&mut current_overlays, &mut replay_overlays);
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
    let mut last_mod_lookup_key = (0, String::new());
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        // Preserve the last sampled target even when the song ends between
        // reference frames; the gameplay window builder retains its tail.
        let from_mods = &mod_samples[index];
        let to_mods = mod_samples.get(index + 1).unwrap_or(from_mods);
        let (mod_start, mod_end, mod_unit) = if use_mod_clock {
            (
                sample_seconds[index],
                sample_seconds
                    .get(index + 1)
                    .copied()
                    .unwrap_or_else(|| sample_seconds[index].next_up()),
                SongLuaTimeUnit::BeatClock,
            )
        } else {
            (
                seg_start,
                seg_end.max(seg_start.next_up()),
                SongLuaTimeUnit::Beat,
            )
        };
        // Dropping a speed mode ends its coalescing run. Selecting the same
        // value again later must start a new window after the intervening mode.
        for player in 0..LUA_PLAYERS {
            for key in ["xmod", "cmod", "mmod"] {
                if from_mods[player].get(key).is_none() {
                    last_mod_lookup_key.0 = player;
                    last_mod_lookup_key.1.clear();
                    last_mod_lookup_key.1.push_str(key);
                    last_mod_windows.remove(&last_mod_lookup_key);
                }
            }
        }
        push_update_mod_targets_with_key(
            &mut eases,
            mod_start,
            mod_end,
            from_mods,
            to_mods,
            &baseline_mods,
            &mod_speed_samples[index],
            &mut last_mod_windows,
            &mut last_mod_lookup_key,
            // Keep the update clock: narrowing to a beat and converting back
            // can move a step target past its own frame's timestamp.
            mod_unit,
        );
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
        let from_columns = &column_samples[index];
        let to_columns = column_samples.get(index + 1).unwrap_or(from_columns);
        append_column_transform_windows_from_samples(
            &mut column_transforms,
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
        );
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

fn call_update_functions_at(
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
        crate::lua_util::update_dispatch_perf::baseline::run_actor_compile_update_functions_with_delta(lua, root, delta_seconds)
            .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, previous_delta.0, previous_delta.1)
        .map_err(|err| err.to_string())?;
    result
}
