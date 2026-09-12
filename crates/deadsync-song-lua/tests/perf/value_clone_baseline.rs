// Frozen from 4b877a764bb48b98856243b3455d4c221deab37e; only test visibility/formatting differs.

use super::*;

pub(super) fn clone_lua_value(lua: &Lua, value: Value) -> mlua::Result<Value> {
    match value {
        Value::Table(table) => {
            let cloned = lua.create_table()?;
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;
                cloned.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)?;
            }
            Ok(Value::Table(cloned))
        }
        other => Ok(other),
    }
}
