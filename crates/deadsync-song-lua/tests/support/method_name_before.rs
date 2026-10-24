use mlua::Lua;

pub(crate) fn flags(lua: &Lua, name: &mlua::LuaString) -> mlua::Result<(bool, bool)> {
    // SAFETY: the checked string stays rooted at base+1. Only validated flags
    // leave the read scope; neither borrowed bytes nor a pointer escape.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 1, |raw, base| {
            raw.push(name)?;
            let mut len = 0;
            let data = mlua::ffi::lua_tolstring(raw.state(), base + 1, &mut len);
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
            let text = std::str::from_utf8(bytes).map_err(|error| mlua::Error::FromLuaConversionError {
                from: "string", to: "&str".into(), message: Some(error.to_string()),
            })?;
            Ok((!text.starts_with("__songlua_") && !text.ends_with("Command"),
                matches!(text, "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction")))
        })
    }
}
