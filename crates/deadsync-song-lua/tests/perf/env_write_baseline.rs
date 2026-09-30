// Frozen callback functions from a960e1c6f (0.5.1633).
use super::*;

pub(super) fn create_chunk_env_proxy(lua: &Lua, target: Table) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    proxy.set("__songlua_env_target", target)?;
    let globals = lua.globals();
    let mt = lua.create_table()?;
    let proxy_for_index = proxy.clone();
    let globals_for_index = globals.clone();
    mt.set(
        "__index",
        lua.create_function(move |_, (_self, key): (Table, Value)| {
            let target: Table = proxy_for_index.get("__songlua_env_target")?;
            let value = target.get::<Value>(key.clone())?;
            if !matches!(value, Value::Nil) {
                return Ok(value);
            }
            globals_for_index.get::<Value>(key)
        })?,
    )?;
    let proxy_for_newindex = proxy.clone();
    let globals_for_newindex = globals;
    mt.set(
        "__newindex",
        lua.create_function(move |_, (_self, key, value): (Table, Value, Value)| {
            let target: Table = proxy_for_newindex.get("__songlua_env_target")?;
            target.set(key.clone(), value.clone())?;
            if let Some(name) = read_string(key)
                && is_compile_global_name(name.as_str())
            {
                globals_for_newindex.set(name, value)?;
            }
            Ok(())
        })?,
    )?;
    let _ = proxy.set_metatable(Some(mt));
    Ok(proxy)
}
