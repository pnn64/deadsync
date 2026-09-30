// Frozen functions from b27c63730 (0.5.1634).
use super::*;

pub(super) fn set_state_string_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                let value = table.get::<Option<String>>(key)?.unwrap_or_default();
                Ok(Value::String(lua.create_string(&value)?))
            }
        })?,
    )
}

pub(super) fn set_state_string_setter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}
