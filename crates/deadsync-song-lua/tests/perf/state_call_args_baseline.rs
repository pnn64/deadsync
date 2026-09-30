// Frozen callback functions from a960e1c6f (0.5.1633).
use super::*;

pub(super) fn set_state_number_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |_, _args: MultiValue| Ok(table.get::<Option<f32>>(key)?.unwrap_or(0.0))
        })?,
    )
}

pub(super) fn set_state_number_setter(
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
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}

pub(super) fn set_state_bool_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |_, _args: MultiValue| Ok(table.get::<Option<bool>>(key)?.unwrap_or(false))
        })?,
    )
}

pub(super) fn set_state_bool_setter(
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
                    .and_then(read_boolish)
                    .unwrap_or(false);
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}
