// Frozen callback bodies from 7a4bdbb55, wrapped as helpers with borrowed MultiValue input.
use mlua::{Lua,MultiValue,Table,Value};
use crate::{create_split_table,path_basename,read_string};
pub fn string_split(lua: &Lua, args: &MultiValue) -> mlua::Result<Table> {
let text = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let separator = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            create_split_table(lua, &text, &separator)
}
pub fn global_split(lua: &Lua, args: &MultiValue) -> mlua::Result<Table> {
let separator = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let text = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            create_split_table(lua, &text, &separator)
}
pub fn basename(lua: &Lua, value: Value) -> mlua::Result<Value> {
Ok(Value::String(lua.create_string(path_basename(
                &read_string(value).unwrap_or_default(),
            ))?))
}
