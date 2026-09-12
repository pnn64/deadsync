//! Frozen from cbc8023529ef1a076816d3d7dc0d83047aaaab38; only function visibility and formatting differ.

use super::*;

pub(super) fn restore_actor_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
    clear_key: fn(&str) -> bool,
) -> mlua::Result<()> {
    let mut keys = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        // Keep Lua string handles alive until iteration finishes; only retained
        // snapshot entries need an owned Rust key. Validate all string keys.
        if clear_key(&key.to_str()?) {
            keys.push(key);
        }
    }
    for key in keys {
        actor.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        actor.set(key, value)?;
    }
    Ok(())
}

pub(super) fn restore_actor_mutable_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_mutable_state_key)
}

pub(super) fn restore_actor_semantic_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_semantic_state_key)
}

pub(super) fn restore_actors_semantic_state(
    snapshots: Vec<(Table, Vec<(String, Value)>)>,
) -> mlua::Result<()> {
    for (actor, snapshot) in snapshots {
        restore_actor_semantic_state(&actor, snapshot)?;
    }
    Ok(())
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
