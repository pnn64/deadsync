//! Actor method-key policy, using a rooted safe Lua byte view.
use mlua::Lua;
pub(crate) fn flags(lua: &Lua, name: &mlua::LuaString) -> mlua::Result<(bool, bool)> {
    name.inspect_bytes(lua, |bytes| {
        let text =
            std::str::from_utf8(bytes).map_err(|error| mlua::Error::FromLuaConversionError {
                from: "string",
                to: "&str".into(),
                message: Some(error.to_string()),
            })?;
        Ok((
            !text.starts_with("__songlua_") && !text.ends_with("Command"),
            matches!(
                text,
                "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
            ),
        ))
    })
}
