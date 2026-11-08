// Helpers frozen from 1c744eebf; parent visibility only.
use super::*;

pub(super) fn create_style_table(lua: &Lua, style_name: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set("__songlua_style_key", style_name)?;
    let style = crate::song_lua_style_info(style_name);
    let style_name_for_get = style.name.to_string();
    table.set(
        "GetName",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&style_name_for_get)?))
        })?,
    )?;
    set_string_method(lua, &table, "GetStepsType", style.steps_type)?;
    set_string_method(lua, &table, "GetStyleType", style.style_type)?;
    table.set(
        "ColumnsPerPlayer",
        lua.create_function(move |_, _args: MultiValue| Ok(style.columns as i64))?,
    )?;
    table.set(
        "GetWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(style.width))?,
    )?;
    table.set(
        "GetColumnInfo",
        lua.create_function(move |lua, args: MultiValue| {
            let index = method_arg(&args, 1)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(1)
                .clamp(1, style.columns as i32) as usize
                - 1;
            let info = lua.create_table()?;
            info.set("Name", crate::song_lua_style_column_name(index))?;
            info.set("Track", index as i64)?;
            info.set("XOffset", crate::song_lua_style_column_x(style.name, index))?;
            Ok(info)
        })?,
    )?;
    Ok(table)
}

pub(super) fn set_string_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    value: &str,
) -> mlua::Result<()> {
    // Native LuaReference retains values in the registry. Holding an mlua
    // string inside every getter consumes its limited auxiliary stack instead.
    let value = lua.create_registry_value(lua.create_string(value)?)?;
    table.set(
        name,
        lua.create_function(move |lua, (): ()| lua.registry_value::<Value>(&value))?,
    )
}
