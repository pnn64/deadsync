// Frozen from 4c7312013a6143518f0ea5088c046e56afb7c8a4; only test visibility/formatting differs.

use super::*;

pub(super) fn snapshot_actor_state(
    lua: &Lua,
    actor: &Table,
    keep_key: fn(&str) -> bool,
) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?;
        if keep_key(&key) {
            out.push((key.to_string(), clone_lua_value(lua, value)?));
        }
    }
    Ok(out)
}

pub(super) fn snapshot_actor_semantic_state(
    lua: &Lua,
    actor: &Table,
) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_semantic_state_key)
}

pub(super) fn snapshot_actor_semantic_state_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let snapshot = lua.create_table()?;
    for (index, (key, value)) in snapshot_actor_semantic_state(lua, actor)?
        .into_iter()
        .enumerate()
    {
        let entry = lua.create_table()?;
        entry.raw_set(1, key)?;
        entry.raw_set(2, value)?;
        snapshot.raw_set(index + 1, entry)?;
    }
    Ok(snapshot)
}
