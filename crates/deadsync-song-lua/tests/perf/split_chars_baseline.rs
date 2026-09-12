//! Frozen from cc26826a28f9e9da991cd78a4b8cc2b78a2a90a9; only visibility differs.

use super::*;

pub(super) fn create_split_table(lua: &Lua, text: &str, separator: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if separator.is_empty() {
        if text.is_empty() {
            table.raw_set(1, "")?;
        } else {
            for (idx, value) in text.chars().enumerate() {
                table.raw_set(idx + 1, value.to_string())?;
            }
        }
        return Ok(table);
    }
    for (idx, value) in text.split(separator).enumerate() {
        table.raw_set(idx + 1, value)?;
    }
    Ok(table)
}
