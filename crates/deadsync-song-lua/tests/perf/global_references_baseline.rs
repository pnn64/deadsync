// Frozen from 4b877a764bb48b98856243b3455d4c221deab37e; only test visibility/formatting differs.

use super::*;

pub(super) fn global_actor_references(lua: &Lua) -> Result<HashSet<usize>, String> {
    let registry = song_lua_actor_registry(lua).map_err(|err| err.to_string())?;
    let actor_pointers = registry
        .sequence_values::<Table>()
        .map(|actor| actor.map(|actor| actor.to_pointer() as usize))
        .collect::<mlua::Result<HashSet<_>>>()
        .map_err(|err| err.to_string())?;
    let globals = lua.globals();
    let globals_pointer = globals.to_pointer() as usize;
    let registry_pointer = registry.to_pointer() as usize;
    let mut referenced = HashSet::new();
    for pair in globals.clone().pairs::<Value, Value>() {
        let (_, Value::Table(table)) = pair.map_err(|err| err.to_string())? else {
            continue;
        };
        let pointer = table.to_pointer() as usize;
        if actor_pointers.contains(&pointer) {
            referenced.insert(pointer);
            continue;
        }
        if pointer == globals_pointer || pointer == registry_pointer {
            continue;
        }
        for pair in table.pairs::<Value, Value>() {
            let (_, Value::Table(value)) = pair.map_err(|err| err.to_string())? else {
                continue;
            };
            let pointer = value.to_pointer() as usize;
            if actor_pointers.contains(&pointer) {
                referenced.insert(pointer);
            }
        }
    }
    Ok(referenced)
}
