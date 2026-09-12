//! Frozen from cbc8023529ef1a076816d3d7dc0d83047aaaab38; only function visibility and formatting differ.

use super::*;

pub(super) fn stringify_lua_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(Value::Table(table)) = args.front() else {
        return Ok(Value::Nil);
    };
    let form = args.get(1).cloned().and_then(read_string);
    let format = lua
        .globals()
        .get::<Table>("string")?
        .get::<Function>("format")?;
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        let value = value?;
        let text = if let Some(form) = form.as_deref()
            && matches!(value, Value::Integer(_) | Value::Number(_))
        {
            lua_text_value(format.call::<Value>((form, value))?)?
        } else {
            lua_text_value(value)?
        };
        out.raw_set(idx + 1, text)?;
    }
    Ok(Value::Table(out))
}
