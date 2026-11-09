#[cfg(any(test, feature = "legacy-oracle"))]
use mlua::{Lua, MultiValue, Table, Value};

#[cfg(any(test, feature = "legacy-oracle"))]
use crate::note_song_lua_side_effect;

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn create_cryptman_table(lua: &Lua) -> mlua::Result<Table> {
    let cryptman = lua.create_table()?;
    for name in ["SHA1File", "SHA1String"] {
        cryptman.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string([0_u8; 20])?))
            })?,
        )?;
    }
    for name in ["SHA256File", "SHA256String"] {
        cryptman.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string([0_u8; 32])?))
            })?,
        )?;
    }
    cryptman.set(
        "GenerateRandomUUID",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(
                lua.create_string("00000000-0000-4000-8000-000000000000")?,
            ))
        })?,
    )?;
    cryptman.set(
        "SignFileToFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    Ok(cryptman)
}


