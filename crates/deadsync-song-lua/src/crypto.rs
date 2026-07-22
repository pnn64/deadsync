use mlua::{Lua, MultiValue, Table, Value};

use crate::note_song_lua_side_effect;

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

#[cfg(test)]
mod tests {
    use mlua::{Lua, MultiValue, Value};

    use super::create_cryptman_table;

    #[test]
    fn cryptman_returns_fixed_hash_lengths() {
        let lua = Lua::new();
        let cryptman = create_cryptman_table(&lua).unwrap();
        let sha1 = cryptman
            .get::<mlua::Function>("SHA1String")
            .unwrap()
            .call::<Value>(MultiValue::new())
            .unwrap();
        let sha256 = cryptman
            .get::<mlua::Function>("SHA256String")
            .unwrap()
            .call::<Value>(MultiValue::new())
            .unwrap();

        assert_eq!(
            matches!(sha1, Value::String(ref text) if text.as_bytes().len() == 20),
            true
        );
        assert_eq!(
            matches!(sha256, Value::String(ref text) if text.as_bytes().len() == 32),
            true
        );
    }
}
