// Helpers frozen from 0908f3cff. Only visibility, live-branch qualification
// and include_str paths adapt their test-module location.
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
            out.push((key.to_string(), clone_actor_state(lua, &key, value)?));
        }
    }
    {
        let pose = live_pose_snapshot(lua, actor)?.into_iter().flatten();
        let flags = live_render_flags(lua, actor)
            .into_iter()
            .flat_map(RenderFlags::snapshot);
        let base = live_base_transform(lua, actor)
            .into_iter()
            .flat_map(BaseTransform::snapshot);
        for (key, value) in pose.chain(flags).chain(base) {
            if !keep_key(key) {
                continue;
            }
            if let Some(entry) = out.iter_mut().find(|(name, _)| name == key) {
                entry.1 = value;
            } else if !value.is_nil() {
                out.push((key.to_owned(), value));
            }
        }
    }
    Ok(out)
}

pub(super) fn snapshot_actor_semantic_state_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if live_destination_value(lua, actor, |_| ()).is_some() {
        let snapshot = lua.create_table()?;
        for (index, (key, value)) in snapshot_actor_state(lua, actor, is_actor_semantic_state_key)?
            .into_iter()
            .enumerate()
        {
            let entry = lua.create_table_with_capacity(2, 0)?;
            entry.raw_set(1, key)?;
            entry.raw_set(2, value)?;
            snapshot.raw_set(index + 1, entry)?;
        }
        return Ok(snapshot);
    }
    let snapshot = lua.create_table()?;
    let mut index = 1usize;
    for pair in actor.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        // Validate every string key, including keys excluded from the snapshot.
        if !is_actor_semantic_state_key(&key.to_str()?) {
            continue;
        }
        let value = clone_actor_state(lua, &key.to_str()?, value)?;
        let entry = lua.create_table_with_capacity(2, 0)?;
        entry.raw_set(1, key)?;
        entry.raw_set(2, value)?;
        snapshot.raw_set(index, entry)?;
        index += 1;
    }
    Ok(snapshot)
}

pub(super) fn restore_scalar_globals(
    lua: &Lua,
    snapshot: Vec<(String, Value)>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let keys = snapshot
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<HashSet<_>>();
    let mut remove = smallvec::SmallVec::<[mlua::LuaString; 16]>::new();
    for pair in globals.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        // Validate every string key before making any changes, including keys
        // whose values are not scalar. Retain Lua handles instead of copying
        // temporary key bytes into Rust strings.
        let should_remove = {
            let text = key.to_str()?;
            is_scalar_lua_value(&value) && !keys.contains(text.as_ref())
        };
        if should_remove {
            remove.push(key);
        }
    }
    for key in remove {
        globals.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        globals.set(key, value)?;
    }
    Ok(())
}

pub(super) fn for_each_actor_message_command(
    actor: &Table,
    mut visit: impl FnMut(&str, Function),
) -> mlua::Result<()> {
    // Filter values before borrowing/allocating names. Keep keys on Lua's stack
    // and copy only selected names into the caller's pre-execution snapshot.
    actor.for_each::<Value, Value>(|key, value| {
        let (Value::String(key), Value::Function(command)) = (key, value) else {
            return Ok(());
        };
        let Ok(name) = key.to_str() else {
            return Ok(());
        };
        if name.ends_with("MessageCommand") {
            visit(&name, command);
        }
        Ok(())
    })
}
