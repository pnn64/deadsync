//! Frozen from cc26826a28f9e9da991cd78a4b8cc2b78a2a90a9; only visibility differs.

use super::*;

pub(super) fn stringify_lua_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(Value::Table(table)) = args.front().cloned() else {
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
            let mut call_args = MultiValue::new();
            call_args.push_back(Value::String(lua.create_string(form)?));
            call_args.push_back(value);
            lua_text_value(format.call::<Value>(call_args)?)?
        } else {
            lua_text_value(value)?
        };
        out.raw_set(idx + 1, text)?;
    }
    Ok(Value::Table(out))
}
