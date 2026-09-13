//! Frozen from eeb449635856c265a7e337c4cb7c171904065d4a; only visibility and formatting differ.

use super::*;

pub(super) fn create_string_array(lua: &Lua, values: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, *value)?;
    }
    Ok(table)
}

pub(super) fn create_owned_string_array(lua: &Lua, values: &[String]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, value.as_str())?;
    }
    Ok(table)
}
