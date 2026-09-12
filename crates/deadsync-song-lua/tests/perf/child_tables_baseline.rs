// Frozen from 4b877a764bb48b98856243b3455d4c221deab37e; only test visibility/formatting differs.

use super::*;

pub(super) fn actor_named_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let children = lua.create_table()?;
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (key, value) = pair?;
        children.set(key, value)?;
    }
    merge_actor_sequence_children(lua, actor, &children)?;
    Ok(children)
}

pub(super) fn actor_direct_children(lua: &Lua, actor: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::new();
    let mut seen = ActorChildPointers::new();
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        seen.push(&mut out, child);
    }
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (_, value) = pair?;
        let Value::Table(child) = value else {
            continue;
        };
        if actor_is_child_group(&child)? {
            for group_value in child.sequence_values::<Value>() {
                if let Value::Table(group_child) = group_value? {
                    seen.push(&mut out, group_child);
                }
            }
        } else {
            seen.push(&mut out, child);
        }
    }
    Ok(out)
}
