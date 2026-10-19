use crate::{create_split_table, path_basename};
use mlua::{BorrowedStr, Lua, MultiValue, Table, Value};

fn string_value(value: &Value) -> Option<BorrowedStr> {
    // These callbacks accept UTF-8 strings; other values fall back to "".
    let Value::String(text) = value else {
        return None;
    };
    text.to_str().ok()
}

pub(crate) fn split_args(
    lua: &Lua,
    args: &MultiValue,
    text_index: usize,
    separator_index: usize,
) -> mlua::Result<Table> {
    let text = args.get(text_index).and_then(string_value);
    let separator = args.get(separator_index).and_then(string_value);
    create_split_table(
        lua,
        text.as_deref().unwrap_or_default(),
        separator.as_deref().unwrap_or_default(),
    )
}

pub(crate) fn basename(lua: &Lua, value: Value) -> mlua::Result<Value> {
    let text = string_value(&value);
    Ok(Value::String(lua.create_string(path_basename(
        text.as_deref().unwrap_or_default(),
    ))?))
}

#[cfg(test)]
#[path = "../tests/perf/compat_args.rs"]
mod perf;
