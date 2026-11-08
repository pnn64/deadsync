// Unchanged capture bodies; generated before the callback pass.
use super::*;

pub fn snapshot_actors_semantic_state(
    lua: &Lua,
    actors: &[Table],
) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    actors
        .iter()
        .map(|actor| Ok((actor.clone(), snapshot_actor_semantic_state(lua, actor)?)))
        .collect()
}

pub fn capture_indexed_actor_function_blocks(
    lua: &Lua,
    actors: &[(usize, Table)],
    function: &Function,
    arg: Option<f32>,
    song_beat: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    if let Some(song_beat) = song_beat {
        set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    }
    let snapshot_tables: Vec<_> = actors.iter().map(|(_, actor)| actor.clone()).collect();
    let state_snapshot =
        snapshot_actors_semantic_state(lua, &snapshot_tables).map_err(|err| err.to_string())?;
    reset_indexed_actor_capture_tables(lua, actors)?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let blocks = collect_indexed_actor_capture_blocks(actors);
    reset_indexed_actor_capture_tables(lua, actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let blocks = blocks?;
    result.map_err(|err| err.to_string())?;
    Ok(blocks)
}

pub(super) fn capture_touched_blocks(
    lua: &Lua,
    actors: &[(usize, Table)],
    function: &Function,
    arg: Option<f32>,
    song_beat: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    if let Some(song_beat) = song_beat {
        set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    }
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let touched_actors =
        capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string())?;
    let actor_ptrs =
        capture_scope_actor_pointers(&capture_scope.actors).map_err(|err| err.to_string())?;
    let state_snapshot =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string())?;
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    let touched = actors
        .iter()
        .filter(|(_, actor)| actor_ptrs.contains(&(actor.to_pointer() as usize)))
        .cloned()
        .collect::<Vec<_>>();
    let blocks = collect_indexed_actor_capture_blocks(&touched);
    reset_actor_capture_tables(lua, &touched_actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let blocks = blocks?;
    result.map_err(|err| err.to_string())?;
    Ok(blocks)
}

pub fn capture_function_action_blocks(
    lua: &Lua,
    overlays: &[(usize, Table)],
    tracked_actors: &[SongLuaTrackedActor],
    function: &Function,
    beat: f32,
) -> Result<SongLuaFunctionActionCapture, String> {
    capture_function_action_blocks_inner(lua, overlays, tracked_actors, function, beat, true)
}

pub(super) fn capture_function_action_blocks_inner(
    lua: &Lua,
    overlays: &[(usize, Table)],
    tracked_actors: &[SongLuaTrackedActor],
    function: &Function,
    beat: f32,
    restore_function_tables: bool,
) -> Result<SongLuaFunctionActionCapture, String> {
    let table_snapshots = restore_function_tables
        .then(|| {
            let functions = lua.create_sequence_from([function.clone()])?;
            snapshot_function_action_tables(lua, &functions)
        })
        .transpose()
        .map_err(|err| err.to_string())?;
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    let globals = lua.globals();
    let column_snapshot = snapshot_note_field_columns(lua).map_err(|err| err.to_string())?;
    let previous_broadcasts = globals
        .get::<Value>(SONG_LUA_BROADCASTS_KEY)
        .map_err(|err| err.to_string())?;
    let broadcast_table = lua.create_table().map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, broadcast_table.clone())
        .map_err(|err| err.to_string())?;
    let previous_sound_calls = globals
        .get::<Value>(SONG_LUA_SOUND_CALLS_KEY)
        .map_err(|err| err.to_string())?;
    let sound_calls = lua.create_table().map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_SOUND_CALLS_KEY, sound_calls.clone())
        .map_err(|err| err.to_string())?;
    let previous_suppress_broadcasts = globals
        .get::<Value>(SONG_LUA_SUPPRESS_BROADCAST_KEY)
        .map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_SUPPRESS_BROADCAST_KEY, true)
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let result = function.call::<Value>(());
    let touched_actors =
        capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string())?;
    let actor_ptrs =
        capture_scope_actor_pointers(&capture_scope.actors).map_err(|err| err.to_string())?;
    let state_snapshot =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string())?;
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    let overlay_tables: Vec<_> = overlays
        .iter()
        .filter(|(_, actor)| actor_ptrs.contains(&(actor.to_pointer() as usize)))
        .map(|(index, actor)| (*index, actor.clone()))
        .collect();
    let tracked_indices = tracked_indices_for_actor_pointers(tracked_actors, &actor_ptrs);
    let overlay_blocks = collect_indexed_actor_capture_blocks(&overlay_tables);
    let tracked_blocks =
        collect_tracked_capture_blocks_for_indices(tracked_actors, &tracked_indices);
    let broadcasts = read_song_lua_broadcasts(&broadcast_table).map_err(|err| err.to_string());
    let sound_paths = read_path_table(&sound_calls);
    restore_note_field_columns(lua, column_snapshot).map_err(|err| err.to_string())?;
    let side_effects = song_lua_side_effect_count(lua)
        .map_err(|err| err.to_string())?
        .saturating_sub(side_effect_before);
    reset_actor_capture_tables(lua, &touched_actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, previous_broadcasts)
        .map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_SOUND_CALLS_KEY, previous_sound_calls)
        .map_err(|err| err.to_string())?;
    globals
        .set(
            SONG_LUA_SUPPRESS_BROADCAST_KEY,
            previous_suppress_broadcasts,
        )
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    if let Some(table_snapshots) = table_snapshots {
        restore_function_action_tables(lua, table_snapshots).map_err(|err| err.to_string())?;
    }
    let overlay_blocks = overlay_blocks?;
    let tracked_blocks = tracked_blocks?;
    let broadcasts = broadcasts?;
    let sound_paths = sound_paths?;
    result.map_err(|err| err.to_string())?;
    Ok(SongLuaFunctionActionCapture {
        overlay_blocks,
        tracked_blocks,
        broadcasts,
        sound_paths,
        side_effects,
    })
}

pub fn capture_overlay_compile_actor_function_action_blocks<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
    function: &Function,
    beat: f32,
) -> Result<SongLuaFunctionActionCapture, String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let overlay_tables = overlay_compile_actor_tables_for_indices(overlays, &indices);
    capture_function_action_blocks(lua, &overlay_tables, tracked_actors, function, beat)
}

pub fn compile_overlay_compile_actor_function_action<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &mut [SongLuaTrackedActor],
    function: &Function,
    beat: f32,
    persists: bool,
    counter: &mut usize,
    messages: &mut Vec<SongLuaMessageEvent>,
    sound_events: &mut Vec<SongLuaSoundEvent>,
) -> Result<bool, String> {
    let capture = capture_overlay_compile_actor_function_action_blocks(
        lua,
        overlays,
        tracked_actors,
        function,
        beat,
    )?;
    sound_events.extend(
        capture
            .sound_paths
            .iter()
            .cloned()
            .map(|path| SongLuaSoundEvent {
                beat,
                path,
                source_message: None,
                queued_startup: false,
            }),
    );
    let mut handled_broadcast = false;
    if !capture.broadcasts.is_empty()
        && capture
            .broadcasts
            .iter()
            .all(|(_, has_params)| !*has_params)
    {
        for (message, _) in &capture.broadcasts {
            if overlay_compile_actors_have_message_listener(overlays, tracked_actors, &message) {
                messages.push(SongLuaMessageEvent {
                    beat,
                    message: message.clone(),
                    persists,
                });
                handled_broadcast = true;
            }
        }
    }

    let has_direct_effects =
        !capture.overlay_blocks.is_empty() || !capture.tracked_blocks.is_empty();
    if handled_broadcast && !has_direct_effects {
        return Ok(true);
    }
    if !has_direct_effects {
        return Ok(capture.side_effects > 0);
    }

    let message = format!("__songlua_overlay_fn_action_{counter}");
    *counter += 1;
    for (overlay_index, blocks) in capture.overlay_blocks {
        overlays[overlay_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: message.clone(),
                blocks,
            });
    }
    for (tracked_index, blocks) in capture.tracked_blocks {
        tracked_actors[tracked_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: message.clone(),
                blocks,
            });
    }
    messages.push(SongLuaMessageEvent {
        beat,
        message,
        persists,
    });
    Ok(true)
}
