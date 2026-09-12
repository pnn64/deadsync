// Frozen from 02df6cae276557eb6ddc7db182c93dd5cb24704f; only test visibility/formatting differs.

use super::*;

pub(super) fn snapshot_scalar_globals(lua: &Lua) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for pair in lua.globals().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        if is_scalar_lua_value(&value) {
            out.push((key.to_str()?.to_string(), value));
        }
    }
    Ok(out)
}

pub(super) fn capture_actor_command_preserving_state(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let snapshot = snapshot_actor_mutable_state(lua, actor).map_err(|err| err.to_string())?;
    let globals_snapshot = snapshot_scalar_globals(lua).map_err(|err| err.to_string())?;
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let captured = capture_actor_command(lua, actor, command_name);
    let touched = capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string());
    let touched_snapshots =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string());
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    reset_actor_capture_tables(lua, &touched?)?;
    restore_actors_semantic_state(touched_snapshots?).map_err(|err| err.to_string())?;
    restore_scalar_globals(lua, globals_snapshot).map_err(|err| err.to_string())?;
    restore_actor_mutable_state(actor, snapshot).map_err(|err| err.to_string())?;
    captured
}
