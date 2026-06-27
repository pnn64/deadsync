use chrono::{Datelike, Local, Timelike};
use image::image_dimensions;
use log::{debug, info};
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use deadlib_present::actors::{TextAlign, TextAttribute};
use deadlib_present::anim::EffectClock;
use deadsync_noteskin::{NUM_QUANTIZATIONS, Style};
use deadsync_song_lua::{
    GRAPH_DISPLAY_VALUE_RESOLUTION, LUA_PLAYERS, MultitapDesc, RuntimeModEaseEntry,
    SONG_LUA_BROADCASTS_KEY, SONG_LUA_CAPTURE_ACTOR_SET_KEY, SONG_LUA_CAPTURE_ACTORS_KEY,
    SONG_LUA_CAPTURE_SNAPSHOTS_KEY, SONG_LUA_DANGER_LIFE, SONG_LUA_INITIAL_LIFE,
    SONG_LUA_NOTE_COLUMNS, SONG_LUA_PROBE_ACTOR_SET_KEY, SONG_LUA_PROBE_ACTORS_KEY,
    SONG_LUA_PROBE_METHODS_KEY, SONG_LUA_SPRITE_STATE_CLEAR, SONG_LUA_STARTUP_MESSAGE,
    SONG_LUA_THEME_PATH_PREFIX, SongLuaActorMultiVertexPoint, SongLuaDateGlobals,
    SongLuaFunctionActionInput, SongLuaFunctionEaseDecision, SongLuaFunctionEaseInput,
    SongLuaFunctionEaseResult, SongLuaHostState, SongLuaReadEasesStats,
    SongLuaTrackedActor as TrackedCompileActor,
    SongLuaTrackedActorTarget as TrackedCompileActorTarget, THEME_RECEPTOR_Y_REV,
    THEME_RECEPTOR_Y_STD, active_perframe_entries, apply_multitap_field_state, call_perframe_entry,
    clone_lua_value, compile_song_runtime_delta_values, compile_song_runtime_values,
    create_chunk_env_proxy, create_life_record_table, current_perframe_player_states,
    display_bpms_text, ease_window_cmp, entry_file_path, file_path_string, graph_display_body_size,
    initial_chunk_environment, install_cmd_helpers, install_core_globals, install_ease_table,
    install_late_globals, install_message_manager_globals, install_screen_manager_globals,
    install_sound_globals, is_song_lua_audio_path, is_song_lua_image_path, is_song_lua_media_path,
    is_song_lua_video_path, lua_format_text, lua_text_value, make_color_table, merge_compile_info,
    message_event_cmp, method_arg, mod_window_cmp, multitap_explosion_command_blocks,
    multitap_explosion_message_events, multitap_explosion_message_name,
    named_overlay_indices_by_name, note_song_lua_side_effect, overlay_delta_intersection,
    overlay_descendants_by_parent, perframe_boundaries, perframe_delta_seconds, perframe_samples,
    player_index_from_value, player_number_name, preprocess_lua_cmd_syntax,
    push_multitap_actor_eases, push_multitap_explosion_eases, push_perframe_static_targets,
    push_sampled_perframe_targets, read_actions_with_function_capture, read_boolish,
    read_color_value, read_eases_with_function_capture, read_f32, read_i32_value, read_mod_windows,
    read_multitap_descs, read_perframe_entries, read_runtime_mod_eases, read_song_lua_broadcasts,
    read_song_lua_sound_paths, read_string, read_u32_value, read_vertex_colors_value,
    read_xero_runtime_mod_eases_with_overlay_capture, record_unsupported_function_action_capture,
    record_unsupported_function_ease_capture, record_unsupported_xero_overlay_function_ease,
    register_loaded_easing_names, resolve_script_path, restore_compile_globals,
    set_compile_song_runtime_beat, set_compile_song_runtime_delta_values,
    set_compile_song_runtime_values, set_string_method, snapshot_compile_globals,
    song_lua_human_player_count, song_lua_side_effect_count, song_lua_style_column_x,
    song_lua_style_info, song_music_rate, theme_metric_number, theme_path, tracked_player_tables,
    truthy, unsupported_perframe_info, update_function_end_beat, update_function_overlay_eases,
    update_function_samples,
};

mod actor_host;
mod compat;
mod managers;
mod overlay;

use self::actor_host::{
    actor_overlay_initial_state, actor_tree_has_update_functions, broadcast_song_lua_message,
    compile_function_action, compile_note_column_pos_function_ease, compile_overlay_function_ease,
    create_dummy_actor, create_top_screen_table, current_song_lua_style_name, execute_script_file,
    install_def, install_file_loaders, probe_function_ease_target,
    read_global_function_nested_tables, read_note_column_zoom_hides, read_overlay_actors,
    read_tracked_compile_actors, read_update_function_actions, read_update_function_nested_tables,
    read_update_function_tables, reset_overlay_capture_tables, reset_tracked_capture_tables,
    run_actor_draw_functions, run_actor_init_commands, run_actor_startup_commands,
    run_actor_update_functions, run_actor_update_functions_with_delta,
};
use self::compat::install_stdlib_compat;
use self::managers::{create_noteskin_table, song_lua_noteskin_resolver};
pub use self::overlay::{
    SongLuaOverlayActor, SongLuaOverlayBlendMode, SongLuaOverlayCommandBlock, SongLuaOverlayEase,
    SongLuaOverlayKind, SongLuaOverlayMeshVertex, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelDraw, SongLuaOverlayModelLayer, SongLuaOverlayState,
    SongLuaOverlayStateDelta, SongLuaProxyTarget, SongLuaTextGlowMode,
};
use self::overlay::{
    overlay_delta_from_blocks, overlay_state_after_blocks, parse_overlay_blend_mode,
    parse_overlay_effect_clock, parse_overlay_effect_mode, parse_overlay_text_align,
    parse_overlay_text_glow_mode,
};
pub use deadsync_song_lua::{
    SongLuaCapturedActor, SongLuaColumnOffsetWindow, SongLuaCompileContext, SongLuaCompileInfo,
    SongLuaDifficulty, SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow,
    SongLuaNoteHideWindow, SongLuaNoteskinResolver, SongLuaPlayerContext, SongLuaSpanMode,
    SongLuaSpeedMod, SongLuaTimeUnit,
};

pub type CompiledSongLua = deadsync_song_lua::CompiledSongLua<SongLuaOverlayActor>;
struct OverlayCompileActor {
    table: Table,
    actor: SongLuaOverlayActor,
}

pub fn compile_song_lua(
    entry_path: &Path,
    context: &SongLuaCompileContext,
) -> Result<CompiledSongLua, String> {
    let compile_started = Instant::now();
    let mut stage_started = compile_started;
    let mut stage_times = Vec::new();
    let entry_path = entry_file_path(entry_path)
        .ok_or_else(|| format!("song lua entry '{}' does not exist", entry_path.display()))?;
    let trace_entry_path = entry_path.clone();
    let lua = Lua::new();
    let mut host = SongLuaHostState::default();
    install_host(&lua, context, &mut host).map_err(|err| err.to_string())?;
    push_song_lua_stage_time(&mut stage_times, "host", &mut stage_started);
    let root = execute_script_file(&lua, &entry_path, context.song_dir.as_path())
        .map_err(|err| format!("failed to execute '{}': {err}", entry_path.display()))?;
    push_song_lua_stage_time(&mut stage_times, "execute", &mut stage_started);
    run_actor_init_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor init commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    push_song_lua_stage_time(&mut stage_times, "init_commands", &mut stage_started);
    run_actor_startup_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor startup commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    push_song_lua_stage_time(&mut stage_times, "startup_commands", &mut stage_started);
    run_actor_update_functions(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor update functions for '{}': {err}",
            entry_path.display()
        )
    })?;
    push_song_lua_stage_time(&mut stage_times, "update_functions", &mut stage_started);
    run_actor_draw_functions(&lua, &root);
    push_song_lua_stage_time(&mut stage_times, "draw_functions", &mut stage_started);
    register_loaded_easing_names(&lua, &mut host).map_err(|err| err.to_string())?;
    push_song_lua_stage_time(&mut stage_times, "easing_names", &mut stage_started);

    let globals = lua.globals();
    let mut out = CompiledSongLua {
        entry_path,
        screen_width: context.screen_width,
        screen_height: context.screen_height,
        ..CompiledSongLua::default()
    };
    // Overlay command capture replays actor commands. Restore the mod globals
    // afterwards so capture-time side effects do not rewrite compile inputs.
    let compile_globals =
        snapshot_compile_globals(&lua, &globals).map_err(|err| err.to_string())?;
    let overlays = read_overlay_actors(&lua, &root, context, &mut out.info);
    restore_compile_globals(&globals, compile_globals).map_err(|err| err.to_string())?;
    let mut overlays = overlays?;
    push_song_lua_stage_time(&mut stage_times, "read_overlays", &mut stage_started);
    let mut tracked_actors = read_tracked_compile_actors(&lua)?;
    let mut overlay_trigger_counter = 0usize;
    let hidden_players = std::array::from_fn(|player| {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        globals
            .get::<Option<Table>>(key)
            .ok()
            .flatten()
            .and_then(|actor| {
                actor
                    .get::<Option<bool>>("__songlua_visible")
                    .ok()
                    .flatten()
            })
            .is_some_and(|visible| !visible)
    });
    let prefix_perframes = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
        .and_then(|table| table.get::<Option<Table>>("perframes").ok().flatten());
    let global_perframes = globals
        .get::<Option<Table>>("mod_perframes")
        .map_err(|err| err.to_string())?;
    push_song_lua_stage_time(&mut stage_times, "read_globals", &mut stage_started);

    if let Some(prefix_globals) = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
    {
        out.beat_mods.extend(read_mod_windows(
            prefix_globals
                .get::<Option<Table>>("mods")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
        )?);
        push_song_lua_stage_time(&mut stage_times, "prefix_mods", &mut stage_started);
        let (eases, overlay_eases, column_offsets, info) = read_eases(
            &lua,
            prefix_globals
                .get::<Option<Table>>("ease")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
            &mut overlays,
        )?;
        out.eases.extend(eases);
        out.overlay_eases.extend(overlay_eases);
        out.column_offsets.extend(column_offsets);
        merge_compile_info(&mut out.info, info);
        push_song_lua_stage_time(&mut stage_times, "prefix_eases", &mut stage_started);
        read_actions(
            &lua,
            prefix_globals
                .get::<Option<Table>>("actions")
                .map_err(|err| err.to_string())?,
            &mut overlays,
            &mut tracked_actors,
            &mut out.messages,
            &mut overlay_trigger_counter,
            &mut out.info,
        )?;
        push_song_lua_stage_time(&mut stage_times, "prefix_actions", &mut stage_started);
    }

    let global_mods = globals
        .get::<Option<Table>>("mods")
        .map_err(|err| err.to_string())?;
    out.beat_mods.extend(read_mod_windows(
        global_mods.clone(),
        SongLuaTimeUnit::Beat,
    )?);
    let (runtime_eases, runtime_overlay_eases) = read_runtime_mod_eases(
        global_mods,
        &host.easing_names,
        runtime_static_overlay_index(&overlays),
        context,
    )?;
    out.eases.extend(runtime_eases);
    out.overlay_eases.extend(runtime_overlay_eases);
    out.time_mods.extend(read_mod_windows(
        globals
            .get::<Option<Table>>("mod_time")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Second,
    )?);
    for table in read_update_function_tables(&lua, &root, &["mod_time"])? {
        out.time_mods
            .extend(read_mod_windows(Some(table), SongLuaTimeUnit::Second)?);
    }
    push_song_lua_stage_time(&mut stage_times, "global_mods", &mut stage_started);
    let (global_eases, global_overlay_eases, global_column_offsets, global_info) = read_eases(
        &lua,
        globals
            .get::<Option<Table>>("mods_ease")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Beat,
        &host.easing_names,
        &mut overlays,
    )?;
    out.eases.extend(global_eases);
    out.overlay_eases.extend(global_overlay_eases);
    out.column_offsets.extend(global_column_offsets);
    merge_compile_info(&mut out.info, global_info);
    push_song_lua_stage_time(&mut stage_times, "global_eases", &mut stage_started);
    let mut xero_node_tables = read_update_function_nested_tables(&lua, &root, &["nodes"])?;
    xero_node_tables.extend(read_global_function_nested_tables(
        &lua,
        "xero",
        &["definemod", "node"],
        &["nodes"],
    )?);
    let (xero_eases, xero_overlay_eases, xero_info) = read_xero_runtime_mod_eases(
        &lua,
        read_update_function_nested_tables(&lua, &root, &["eases"])?,
        xero_node_tables,
        &host.easing_names,
        &mut overlays,
    )?;
    out.eases.extend(xero_eases);
    out.overlay_eases.extend(xero_overlay_eases);
    merge_compile_info(&mut out.info, xero_info);
    push_song_lua_stage_time(&mut stage_times, "xero_eases", &mut stage_started);
    read_actions(
        &lua,
        globals
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    push_song_lua_stage_time(&mut stage_times, "global_actions", &mut stage_started);
    read_update_function_actions(
        &lua,
        &root,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    push_song_lua_stage_time(&mut stage_times, "update_actions", &mut stage_started);
    let (perframe_eases, perframe_overlay_eases, perframe_info) = compile_perframes(
        &lua,
        prefix_perframes,
        global_perframes,
        context,
        &mut overlays,
        &tracked_actors,
    )?;
    out.eases.extend(perframe_eases);
    out.overlay_eases.extend(perframe_overlay_eases);
    merge_compile_info(&mut out.info, perframe_info);
    push_song_lua_stage_time(&mut stage_times, "perframes", &mut stage_started);
    out.note_hides = read_note_column_zoom_hides(&lua)?;
    push_song_lua_stage_time(&mut stage_times, "note_hides", &mut stage_started);
    let update_overlay_eases =
        match compile_multitap_update_overlays(&lua, context, &mut overlays, &mut out.messages)? {
            Some(eases) => eases,
            None => compile_update_function_overlays(
                &lua,
                &root,
                context,
                &mut overlays,
                &tracked_actors,
            )?,
        };
    out.overlay_eases.extend(update_overlay_eases);
    push_song_lua_stage_time(&mut stage_times, "update_overlays", &mut stage_started);
    if overlays.iter().any(|overlay| {
        overlay
            .actor
            .message_commands
            .iter()
            .any(|command| command.message == SONG_LUA_STARTUP_MESSAGE)
    }) {
        out.messages.push(SongLuaMessageEvent {
            beat: 0.0,
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            persists: false,
        });
    }
    out.overlays = overlays.into_iter().map(|overlay| overlay.actor).collect();
    for tracked in tracked_actors {
        match tracked.target {
            TrackedCompileActorTarget::Player(player) => out.player_actors[player] = tracked.actor,
            TrackedCompileActorTarget::SongForeground => out.song_foreground = tracked.actor,
        }
    }
    out.hidden_players = hidden_players;

    out.beat_mods.sort_by(mod_window_cmp);
    out.time_mods.sort_by(mod_window_cmp);
    out.eases.sort_by(ease_window_cmp);
    out.overlay_eases.sort_by(|left, right| {
        left.start
            .total_cmp(&right.start)
            .then_with(|| left.limit.total_cmp(&right.limit))
            .then_with(|| left.overlay_index.cmp(&right.overlay_index))
    });
    out.messages.sort_by(message_event_cmp);
    out.sound_paths = read_song_lua_sound_paths(&lua)?;
    push_song_lua_stage_time(&mut stage_times, "finalize", &mut stage_started);
    log_song_lua_compile_timing(&trace_entry_path, compile_started, &stage_times);
    Ok(out)
}

fn push_song_lua_stage_time(
    stage_times: &mut Vec<(&'static str, f64)>,
    stage: &'static str,
    stage_started: &mut Instant,
) {
    stage_times.push((stage, stage_started.elapsed().as_secs_f64() * 1000.0));
    *stage_started = Instant::now();
}

fn log_song_lua_compile_timing(
    entry_path: &Path,
    compile_started: Instant,
    stage_times: &[(&'static str, f64)],
) {
    let elapsed_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
    if elapsed_ms < 1000.0 {
        return;
    }
    let mut stages = String::new();
    for (stage, ms) in stage_times {
        if !stages.is_empty() {
            stages.push(' ');
        }
        stages.push_str(stage);
        stages.push_str("_ms=");
        stages.push_str(format!("{ms:.3}").as_str());
    }
    info!(
        "Song lua compile timing: entry='{}' elapsed_ms={elapsed_ms:.3} {}",
        entry_path.display(),
        stages
    );
}

fn install_host(
    lua: &Lua,
    context: &SongLuaCompileContext,
    host: &mut SongLuaHostState,
) -> mlua::Result<()> {
    install_stdlib_compat(lua, context.song_dir.as_path())?;
    install_ease_table(lua, host)?;
    install_globals(lua, context)?;
    install_cmd_helpers(lua)?;
    install_def(lua, context)?;
    install_file_loaders(lua, context.song_dir.clone())?;
    Ok(())
}

fn install_globals(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let now = Local::now();
    let year = now.year();
    let month_of_year = now.month0() as i32;
    let day_of_month = now.day() as i32;
    let hour = now.hour() as i32;
    let minute = now.minute() as i32;
    let second = now.second() as i32;
    let game_state_globals = install_core_globals(
        lua,
        context,
        SongLuaDateGlobals {
            year,
            month_of_year,
            day_of_month,
            hour,
            minute,
            second,
        },
        create_noteskin_table(lua, context)?,
        current_song_lua_style_name,
    )?;
    let current_sort_order = game_state_globals.current_sort_order;
    let current_song = game_state_globals.current_song;

    let top_screen = create_top_screen_table(lua, context, current_sort_order, current_song)?;
    install_screen_manager_globals(
        lua,
        top_screen.top_screen.clone(),
        top_screen.players.clone(),
    )?;
    install_sound_globals(lua, context.song_dir.as_path())?;
    install_message_manager_globals(lua, broadcast_song_lua_message)?;
    install_late_globals(lua, context)?;
    Ok(())
}

fn read_xero_runtime_mod_eases(
    lua: &Lua,
    ease_tables: Vec<Table>,
    node_tables: Vec<Table>,
    easing_names: &HashMap<*const c_void, String>,
    overlays: &[OverlayCompileActor],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
> {
    read_xero_runtime_mod_eases_with_overlay_capture(
        ease_tables,
        node_tables,
        easing_names,
        |entry, function, from, to, info| {
            compile_xero_overlay_function_ease(lua, overlays, entry, function, from, to, info)
        },
    )
}

fn compile_xero_overlay_function_ease(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    entry: &RuntimeModEaseEntry,
    function: &Function,
    from: f32,
    to: f32,
    info: &mut SongLuaCompileInfo,
) -> Result<Vec<SongLuaOverlayEase>, String> {
    let (probed_target, probe_methods, probe_actor_ptrs) =
        probe_function_ease_target(lua, function).map_err(|err| err.to_string())?;
    if !xero_node_touches_overlay(overlays, &probe_actor_ptrs)
        || !matches!(probed_target, None | Some(SongLuaEaseTarget::Function))
    {
        return Ok(Vec::new());
    }
    match compile_overlay_function_ease(
        lua,
        overlays,
        function,
        entry.unit,
        entry.start,
        entry.limit,
        SongLuaSpanMode::Len,
        from,
        to,
        Some(entry.easing.clone()),
        None,
        entry.opt1,
        entry.opt2,
        &probe_actor_ptrs,
    ) {
        Ok(compiled) if !compiled.is_empty() => Ok(compiled),
        Ok(_) => {
            let detail = record_unsupported_xero_overlay_function_ease(
                info,
                entry,
                from,
                to,
                &probe_methods,
            );
            debug!("Unsupported xero overlay function ease capture: {detail}");
            Ok(Vec::new())
        }
        Err(err) => {
            let detail = record_unsupported_xero_overlay_function_ease(
                info,
                entry,
                from,
                to,
                &probe_methods,
            );
            debug!("Unsupported xero overlay function ease capture: {detail}");
            debug!(
                "Unsupported xero overlay function ease capture for '{}': {err}",
                entry.target
            );
            Ok(Vec::new())
        }
    }
}

fn xero_node_touches_overlay(overlays: &[OverlayCompileActor], probe_actor_ptrs: &[usize]) -> bool {
    !probe_actor_ptrs.is_empty()
        && overlays.iter().any(|overlay| {
            let ptr = overlay.table.to_pointer() as usize;
            probe_actor_ptrs.contains(&ptr)
        })
}

fn runtime_static_overlay_index(overlays: &[OverlayCompileActor]) -> Option<usize> {
    overlays.iter().position(|overlay| {
        let SongLuaOverlayKind::Sprite { texture_path, .. } = &overlay.actor.kind else {
            return false;
        };
        texture_path.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .eq_ignore_ascii_case("_static 4x1.png")
        })
    })
}

fn read_eases(
    lua: &Lua,
    table: Option<Table>,
    unit: SongLuaTimeUnit,
    easing_names: &HashMap<*const c_void, String>,
    overlays: &mut [OverlayCompileActor],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        Vec<SongLuaColumnOffsetWindow>,
        SongLuaCompileInfo,
    ),
    String,
> {
    let Some(table) = table else {
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SongLuaCompileInfo::default(),
        ));
    };
    let trace_started = Instant::now();
    let result =
        read_eases_with_function_capture(Some(table), unit, easing_names, |input, info| {
            capture_function_ease(lua, overlays, input, info)
        })?;
    let elapsed_ms = trace_started.elapsed().as_secs_f64() * 1000.0;
    if elapsed_ms >= 1000.0 {
        let stats = result.stats;
        info!(
            "Song lua read_eases timing: unit={unit:?} entries={} function_targets={} overlay_capture_attempts={} overlay_capture_outputs={} player_eases={} overlay_eases={} unsupported_function_eases={} probe_ms={probe_ms:.3} overlay_capture_ms={overlay_capture_ms:.3} elapsed_ms={elapsed_ms:.3}",
            stats.entry_count,
            stats.function_targets,
            stats.overlay_capture_attempts,
            stats.overlay_capture_outputs,
            result.eases.len(),
            result.overlay_eases.len(),
            result.info.unsupported_function_eases,
            probe_ms = stats.probe_ms,
            overlay_capture_ms = stats.overlay_capture_ms,
        );
    }
    Ok((
        result.eases,
        result.overlay_eases,
        result.column_offsets,
        result.info,
    ))
}

fn capture_function_ease(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    input: SongLuaFunctionEaseInput,
    info: &mut SongLuaCompileInfo,
) -> Result<SongLuaFunctionEaseResult, String> {
    let mut stats = SongLuaReadEasesStats::default();
    match compile_note_column_pos_function_ease(
        lua,
        &input.function,
        input.unit,
        input.start,
        input.limit,
        input.span_mode,
        input.from,
        input.to,
        input.easing.clone(),
        input.sustain,
        input.opt1,
        input.opt2,
    ) {
        Ok(compiled) if !compiled.is_empty() => {
            return Ok(SongLuaFunctionEaseResult {
                decision: SongLuaFunctionEaseDecision::ColumnOffsets(compiled),
                stats,
            });
        }
        Ok(_) => {}
        Err(err) => {
            debug!("Skipping song lua note-column position function ease capture: {err}");
        }
    }
    let probe_started = Instant::now();
    let (probed_target, probe_methods, probe_actor_ptrs) =
        probe_function_ease_target(lua, &input.function).map_err(|err| err.to_string())?;
    stats.probe_ms += probe_started.elapsed().as_secs_f64() * 1000.0;
    let target = probed_target.unwrap_or(SongLuaEaseTarget::Function);
    if matches!(target, SongLuaEaseTarget::Function) {
        stats.overlay_capture_attempts += 1;
        let capture_started = Instant::now();
        let captured = compile_overlay_function_ease(
            lua,
            overlays,
            &input.function,
            input.unit,
            input.start,
            input.limit,
            input.span_mode,
            input.from,
            input.to,
            input.easing.clone(),
            input.sustain,
            input.opt1,
            input.opt2,
            &probe_actor_ptrs,
        );
        let capture_ms = capture_started.elapsed().as_secs_f64() * 1000.0;
        stats.overlay_capture_ms += capture_ms;
        if capture_ms >= 1000.0 {
            info!(
                "Slow song lua function ease capture: unit={:?} start={:.3} limit={:.3} span={:?} from={:.3} to={:.3} easing={:?} probe_actors={} overlays={} capture_ms={capture_ms:.3}",
                input.unit,
                input.start,
                input.limit,
                input.span_mode,
                input.from,
                input.to,
                input.easing,
                probe_actor_ptrs.len(),
                overlays.len(),
            );
        }
        return match captured {
            Ok(compiled) if !compiled.is_empty() => Ok(SongLuaFunctionEaseResult {
                decision: SongLuaFunctionEaseDecision::OverlayEases(compiled),
                stats,
            }),
            _ => {
                let detail = record_unsupported_function_ease_capture(
                    info,
                    input.unit,
                    input.start,
                    input.limit,
                    input.span_mode,
                    input.from,
                    input.to,
                    &input.easing,
                    &probe_methods,
                );
                debug!("Unsupported song lua function ease capture: {detail}");
                Ok(SongLuaFunctionEaseResult {
                    decision: SongLuaFunctionEaseDecision::Skip,
                    stats,
                })
            }
        };
    }
    Ok(SongLuaFunctionEaseResult {
        decision: SongLuaFunctionEaseDecision::Target(target),
        stats,
    })
}

fn current_overlay_states(
    overlays: &[OverlayCompileActor],
) -> Result<Vec<SongLuaOverlayState>, String> {
    let mut out = Vec::with_capacity(overlays.len());
    for overlay in overlays {
        out.push(actor_overlay_initial_state(&overlay.table)?);
    }
    Ok(out)
}

fn call_update_functions_at(
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

fn compile_multitap_update_overlays(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlays: &mut Vec<OverlayCompileActor>,
    messages: &mut Vec<SongLuaMessageEvent>,
) -> Result<Option<Vec<SongLuaOverlayEase>>, String> {
    let Some(multitaps) = read_multitap_descs(lua, context)? else {
        return Ok(None);
    };
    if multitaps.is_empty() {
        return Ok(None);
    }
    let overlay_indices = named_overlay_indices_by_name(overlays.len(), |index| {
        overlays[index].actor.name.as_deref()
    });
    let noteskin_resolver = song_lua_noteskin_resolver();
    let mut out = Vec::new();
    for player in 0..LUA_PLAYERS {
        if !context.players[player].enabled {
            continue;
        }
        let pn = player + 1;
        let Some(&field_index) = overlay_indices.get(format!("MultitapFrameP{pn}").as_str()) else {
            return Ok(None);
        };
        apply_multitap_field_state(
            &mut overlays[field_index].actor.initial_state,
            context,
            player,
        );
        for (mti, desc) in multitaps.iter().enumerate() {
            let index = mti + 1;
            let Some(&frame_index) = overlay_indices.get(format!("MultitapP{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            let Some(&arrow_index) =
                overlay_indices.get(format!("MultitapArrowP{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            let Some(&deco_index) =
                overlay_indices.get(format!("MultitapDeco{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            overlays[arrow_index]
                .actor
                .initial_state
                .texcoord_offset
                .get_or_insert([0.0, 0.0]);
            let noteskin = multitap_arrow_noteskin(overlays, arrow_index, context, player)?;
            ensure_multitap_arrow_visual(lua, overlays, arrow_index, context, &noteskin)?;
            let deco_children =
                overlay_descendants_by_parent(overlays.len(), deco_index, |index| {
                    overlays
                        .get(index)
                        .and_then(|overlay| overlay.actor.parent_index)
                })
                .into_iter()
                .map(|index| (index, overlays[index].actor.initial_state))
                .collect::<Vec<_>>();
            push_multitap_actor_eases(
                &mut out,
                frame_index,
                overlays[frame_index].actor.initial_state,
                arrow_index,
                overlays[arrow_index].actor.initial_state,
                deco_index,
                overlays[deco_index].actor.initial_state,
                &deco_children,
                context,
                player,
                noteskin_resolver,
                &noteskin,
                desc,
            );
        }
        for lane in 1..=SONG_LUA_NOTE_COLUMNS {
            let Some(&explosion_index) =
                overlay_indices.get(format!("MultitapExplosionP{pn}_{lane}").as_str())
            else {
                continue;
            };
            push_multitap_explosion_eases(
                &mut out,
                explosion_index,
                overlays[explosion_index].actor.initial_state,
                context,
                &multitaps,
                lane,
            );
            install_multitap_explosion_messages(
                overlays,
                messages,
                explosion_index,
                &multitaps,
                lane,
                pn,
            );
        }
    }
    Ok(Some(out))
}

fn ensure_multitap_arrow_visual(
    lua: &Lua,
    overlays: &mut Vec<OverlayCompileActor>,
    arrow_index: usize,
    context: &SongLuaCompileContext,
    noteskin: &str,
) -> Result<(), String> {
    if matches!(
        overlays[arrow_index].actor.kind,
        SongLuaOverlayKind::Sprite { .. }
            | SongLuaOverlayKind::Model { .. }
            | SongLuaOverlayKind::NoteskinActor { .. }
    ) || overlay_descendants_by_parent(overlays.len(), arrow_index, |index| {
        overlays
            .get(index)
            .and_then(|overlay| overlay.actor.parent_index)
    })
    .into_iter()
    .any(|index| {
        matches!(
            overlays[index].actor.kind,
            SongLuaOverlayKind::Sprite { .. }
                | SongLuaOverlayKind::Model { .. }
                | SongLuaOverlayKind::NoteskinActor { .. }
        )
    }) {
        return Ok(());
    }
    let Some((kind, initial_state)) = multitap_arrow_visual_spec(noteskin, context) else {
        return Ok(());
    };
    overlays.push(OverlayCompileActor {
        table: create_dummy_actor(lua, "Model").map_err(|err| err.to_string())?,
        actor: SongLuaOverlayActor {
            kind,
            name: None,
            parent_index: Some(arrow_index),
            initial_state,
            message_commands: Vec::new(),
        },
    });
    Ok(())
}

fn multitap_arrow_noteskin(
    overlays: &[OverlayCompileActor],
    arrow_index: usize,
    context: &SongLuaCompileContext,
    player: usize,
) -> Result<String, String> {
    overlays[arrow_index]
        .table
        .get::<Option<String>>("__songlua_noteskin_name")
        .map_err(|err| err.to_string())
        .map(|noteskin| {
            noteskin
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| context.players[player].noteskin_name.clone())
        })
}

fn multitap_arrow_visual_spec(
    noteskin: &str,
    context: &SongLuaCompileContext,
) -> Option<(SongLuaOverlayKind, SongLuaOverlayState)> {
    let style = Style {
        num_cols: song_lua_style_info(&context.style_name).columns,
        num_players: song_lua_human_player_count(context).max(1),
    };
    let ns = crate::game::parsing::noteskin::load_itg_skin_cached(&style, noteskin).ok()?;
    let down_col = 1.min(style.num_cols.saturating_sub(1));
    let note_idx = down_col * NUM_QUANTIZATIONS;
    let layers = ns.note_layers.get(note_idx)?;
    if let Some(model_layers) = multitap_arrow_model_layers(layers) {
        return Some((
            SongLuaOverlayKind::Model {
                layers: model_layers,
            },
            SongLuaOverlayState::default(),
        ));
    }
    let slot = layers.iter().find(|slot| slot.model.is_none())?;
    let texture_key = slot.texture_key_shared();
    let mut state = SongLuaOverlayState {
        custom_texture_rect: Some(slot.uv_for_frame_at(slot.frame_index_from_phase(0.0), 0.0)),
        size: Some(slot.logical_size()),
        rot_z_deg: -slot.def.rotation_deg as f32,
        ..SongLuaOverlayState::default()
    };
    if state
        .size
        .is_some_and(|size| size[0] <= 0.0 || size[1] <= 0.0)
    {
        state.size = None;
    }
    Some((
        SongLuaOverlayKind::Sprite {
            texture_path: PathBuf::from(texture_key.as_ref()),
            texture_key,
        },
        state,
    ))
}

fn multitap_arrow_model_layers(
    slots: &[crate::game::parsing::noteskin::SpriteSlot],
) -> Option<Arc<[SongLuaOverlayModelLayer]>> {
    let mut out = Vec::new();
    for slot in slots.iter().filter(|slot| slot.model.is_some()) {
        let model = slot.model.as_ref()?;
        if model.vertices.is_empty() {
            continue;
        }
        let uv_rect = slot.uv_for_frame_at(slot.frame_index_from_phase(0.0), 0.0);
        let (uv_scale, uv_offset, uv_tex_shift) = multitap_arrow_model_uv_params(slot, uv_rect);
        let draw = slot.model_draw_at(0.0, 0.0);
        out.push(SongLuaOverlayModelLayer {
            texture_key: slot.texture_key_shared(),
            vertices: crate::game::parsing::noteskin::build_model_geometry(slot),
            model_size: model.size(),
            uv_scale,
            uv_offset,
            uv_tex_shift,
            uv_velocity: slot.uv_velocity,
            uv_cycle_seconds: slot.uv_cycle_seconds,
            draw: SongLuaOverlayModelDraw {
                pos: draw.pos,
                rot: draw.rot,
                zoom: draw.zoom,
                tint: draw.tint,
                vert_align: draw.vert_align,
                blend_add: draw.blend_add,
                visible: draw.visible,
            },
        });
    }
    (!out.is_empty()).then(|| Arc::from(out.into_boxed_slice()))
}

fn multitap_arrow_model_uv_params(
    slot: &crate::game::parsing::noteskin::SpriteSlot,
    uv_rect: [f32; 4],
) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = match slot.source.as_ref() {
        crate::game::parsing::noteskin::SpriteSource::Atlas { tex_dims, .. } => {
            let tw = tex_dims.0.max(1) as f32;
            let th = tex_dims.1.max(1) as f32;
            let base_u0 = slot.def.src[0] as f32 / tw;
            let base_v0 = slot.def.src[1] as f32 / th;
            [uv_offset[0] - base_u0, uv_offset[1] - base_v0]
        }
        crate::game::parsing::noteskin::SpriteSource::Animated { .. } => [0.0, 0.0],
    };
    (uv_scale, uv_offset, uv_tex_shift)
}

fn install_multitap_explosion_messages(
    overlays: &mut [OverlayCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    explosion_index: usize,
    descs: &[MultitapDesc],
    lane: usize,
    pn: usize,
) {
    let message = multitap_explosion_message_name(lane, pn);
    let mut installed = false;
    let mut targets = vec![explosion_index];
    targets.extend(overlay_descendants_by_parent(
        overlays.len(),
        explosion_index,
        |index| {
            overlays
                .get(index)
                .and_then(|overlay| overlay.actor.parent_index)
        },
    ));
    for overlay_index in targets {
        let blocks = multitap_explosion_command_blocks(&overlays[overlay_index].actor);
        if blocks.is_empty() {
            continue;
        }
        overlays[overlay_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                message: message.clone(),
                blocks,
            });
        installed = true;
    }
    if !installed {
        return;
    }
    messages.extend(multitap_explosion_message_events(descs, lane, pn));
}

fn compile_update_function_overlays(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
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

    reset_overlay_capture_tables(lua, overlays)?;
    reset_tracked_capture_tables(lua, tracked_actors)?;
    call_update_functions_at(lua, root, start, 0.0, 0.0)?;
    let baseline_overlays = current_overlay_states(overlays)?;
    let mut sample_beats = vec![start];
    let mut overlay_samples = vec![baseline_overlays.clone()];

    for sample in update_function_samples(start, end) {
        let delta_seconds = perframe_delta_seconds(context, sample.delta_beats);
        reset_overlay_capture_tables(lua, overlays)?;
        reset_tracked_capture_tables(lua, tracked_actors)?;
        call_update_functions_at(
            lua,
            root,
            sample.eval_beat,
            sample.delta_beats,
            delta_seconds,
        )?;
        sample_beats.push(sample.beat);
        overlay_samples.push(current_overlay_states(overlays)?);
    }

    Ok(update_function_overlay_eases(
        end,
        &baseline_overlays,
        &sample_beats,
        &overlay_samples,
    ))
}

fn compile_perframes(
    lua: &Lua,
    prefix_table: Option<Table>,
    global_table: Option<Table>,
    context: &SongLuaCompileContext,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
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
    let baseline_overlays = current_overlay_states(overlays)?;
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
            let current_overlays = current_overlay_states(overlays)?;
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
            reset_overlay_capture_tables(lua, overlays)?;
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
            overlay_samples.push(current_overlay_states(overlays)?);
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

fn read_actions(
    lua: &Lua,
    table: Option<Table>,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    counter: &mut usize,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    read_actions_with_function_capture(table, messages, |input, messages| {
        capture_function_action(
            lua,
            overlays,
            tracked_actors,
            input,
            counter,
            messages,
            info,
        )
    })
}

fn capture_function_action(
    lua: &Lua,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    input: SongLuaFunctionActionInput,
    counter: &mut usize,
    messages: &mut Vec<SongLuaMessageEvent>,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    if !matches!(
        compile_function_action(
            lua,
            overlays,
            tracked_actors,
            &input.function,
            input.beat,
            input.persists,
            counter,
            messages,
        ),
        Ok(true)
    ) {
        let detail = record_unsupported_function_action_capture(info, input.beat, input.persists);
        debug!("Unsupported song lua function action capture: {detail}");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
