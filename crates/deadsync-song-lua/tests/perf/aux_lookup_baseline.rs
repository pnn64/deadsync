// Frozen from 02df6cae276557eb6ddc7db182c93dd5cb24704f; only test visibility/formatting differs.

use super::*;

pub(super) fn captured_actor_aux_change(
    actor: &Table,
    snapshots: &[(Table, Vec<(String, Value)>)],
) -> Option<f32> {
    let current = actor
        .get::<Option<f32>>("__songlua_aux")
        .ok()
        .flatten()
        .unwrap_or(0.0);
    let previous = snapshots
        .iter()
        .find(|(snapshot_actor, _)| snapshot_actor.to_pointer() == actor.to_pointer())
        .and_then(|(_, state)| state.iter().find(|(key, _)| key == "__songlua_aux"))
        .and_then(|(_, value)| read_f32(value.clone()))
        .unwrap_or(0.0);
    (current != previous).then_some(current)
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
        .then(|| snapshot_function_action_tables(lua, function))
        .transpose()
        .map_err(|err| err.to_string())?;
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    let globals = lua.globals();
    let column_writes_before = globals
        .raw_get::<Option<u64>>("__songlua_column_writes")
        .map_err(|err| err.to_string())?
        .unwrap_or(0);
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
    let overlay_aux = overlay_tables
        .iter()
        .filter_map(|(index, actor)| {
            captured_actor_aux_change(actor, &state_snapshot).map(|aux| (*index, aux))
        })
        .collect();
    let tracked_aux = tracked_indices
        .iter()
        .filter_map(|&index| {
            tracked_actors.get(index).and_then(|tracked| {
                captured_actor_aux_change(&tracked.table, &state_snapshot).map(|aux| (index, aux))
            })
        })
        .collect();
    let overlay_blocks = collect_indexed_actor_capture_blocks(&overlay_tables);
    let tracked_blocks =
        collect_tracked_capture_blocks_for_indices(tracked_actors, &tracked_indices);
    let broadcasts = read_song_lua_broadcasts(&broadcast_table).map_err(|err| err.to_string());
    let sound_paths = read_path_table(&sound_calls);
    let column_writes = globals
        .raw_get::<Option<u64>>("__songlua_column_writes")
        .map_err(|err| err.to_string())?
        .unwrap_or(0)
        > column_writes_before;
    restore_note_field_columns(lua, column_snapshot).map_err(|err| err.to_string())?;
    let saw_side_effect =
        song_lua_side_effect_count(lua).map_err(|err| err.to_string())? > side_effect_before;
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
        restore_function_action_tables(table_snapshots).map_err(|err| err.to_string())?;
    }
    let overlay_blocks = overlay_blocks?;
    let tracked_blocks = tracked_blocks?;
    let broadcasts = broadcasts?;
    let sound_paths = sound_paths?;
    result.map_err(|err| err.to_string())?;
    Ok(SongLuaFunctionActionCapture {
        overlay_blocks,
        tracked_blocks,
        overlay_aux,
        tracked_aux,
        broadcasts,
        sound_paths,
        saw_side_effect,
        column_writes,
    })
}
