// Frozen from ff42a7005d492d6bf2b732ea8a80bcbfa40654b4; only test visibility/formatting differs.

use super::*;

pub(super) fn remove_all_actor_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    for index in 1..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    let children = actor_children(lua, actor)?;
    let mut keys = Vec::new();
    for pair in children.pairs::<Value, Value>() {
        let (key, _) = pair?;
        keys.push(key);
    }
    for key in keys {
        children.set(key, Value::Nil)?;
    }
    Ok(())
}
