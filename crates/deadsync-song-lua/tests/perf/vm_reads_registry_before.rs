// Helpers frozen from 4277c1b63 with parent visibility.
use super::*;

pub(super) fn register_song_lua_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if crate::live::active(lua) {
        ensure_tween_replay(lua, actor)?;
    }
    let registry = song_lua_actor_registry(lua)?;
    let actor_ptr = actor.to_pointer() as usize;
    for value in registry.sequence_values::<Value>() {
        let Value::Table(existing) = value? else {
            continue;
        };
        if existing.to_pointer() as usize == actor_ptr {
            return Ok(());
        }
    }
    registry.raw_set(registry.raw_len() + 1, actor.clone())?;
    Ok(())
}
