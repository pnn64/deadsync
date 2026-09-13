//! Frozen from eeb449635856c265a7e337c4cb7c171904065d4a; only visibility and formatting differ.

use super::*;

pub(super) fn lua_format_text(lua: &Lua, args: &MultiValue) -> mlua::Result<String> {
    let offset = method_arg_offset(args);
    if args.get(offset).is_none() {
        return Ok(String::new());
    }
    let mut call_args = MultiValue::new();
    for index in offset..args.len() {
        if let Some(value) = args.get(index) {
            call_args.push_back(value.clone());
        }
    }
    let string_table = lua.globals().get::<Table>("string")?;
    let format = string_table.get::<Function>("format")?;
    lua_text_value(format.call::<Value>(call_args)?)
}

pub(super) fn install_settextf(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "settextf",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                actor.set("Text", lua_format_text(lua, &args)?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}
