#[cfg(any(test, feature = "legacy-oracle"))]
use mlua::{Lua, Table, Value};

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn json_to_lua_value(lua: &Lua, value: serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Boolean(value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .and_then(|value| mlua::Integer::try_from(value).ok())
            .map(Value::Integer)
            .or_else(|| value.as_f64().map(Value::Number))
            .unwrap_or(Value::Nil),
        serde_json::Value::String(value) => Value::String(lua.create_string(value)?),
        serde_json::Value::Array(values) => {
            // Sparse arrays keep their original growth and Lua length behavior.
            let capacity = if values.iter().all(|value| !value.is_null()) {
                values.len()
            } else {
                0
            };
            let table = lua.create_table_with_capacity(capacity, 0)?;
            for (index, value) in values.into_iter().enumerate() {
                table.raw_set(index + 1, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(values) => {
            let capacity = values.values().filter(|value| !value.is_null()).count();
            let table = lua.create_table_with_capacity(0, capacity)?;
            for (key, value) in values {
                table.set(key, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
    })
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn lua_to_json_value(value: Value) -> serde_json::Value {
    lua_to_json_value_inner(value, 0)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn lua_to_json_value_inner(value: Value, depth: usize) -> serde_json::Value {
    if depth >= 16 {
        return serde_json::Value::Null;
    }
    match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(value) => serde_json::Value::Bool(value),
        Value::Integer(value) => serde_json::Value::Number(value.into()),
        Value::Number(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(value) => serde_json::Value::String(value.to_string_lossy()),
        Value::Table(table) => lua_table_to_json_value(table, depth + 1),
        _ => serde_json::Value::Null,
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn lua_table_to_json_value(table: Table, depth: usize) -> serde_json::Value {
    let len = table.raw_len();
    let mut array = vec![serde_json::Value::Null; len];
    let mut object = serde_json::Map::new();
    let mut is_array = len > 0;
    for pair in table.pairs::<Value, Value>() {
        let Ok((key, value)) = pair else {
            continue;
        };
        let json_value = lua_to_json_value_inner(value, depth);
        match key {
            Value::Integer(index) if is_array && index >= 1 && index as usize <= len => {
                array[index as usize - 1] = json_value;
            }
            Value::String(key) => {
                is_array = false;
                object.insert(key.to_string_lossy(), json_value);
            }
            Value::Integer(index) => {
                is_array = false;
                object.insert(index.to_string(), json_value);
            }
            Value::Number(index) => {
                is_array = false;
                object.insert(index.to_string(), json_value);
            }
            Value::Boolean(key) => {
                is_array = false;
                object.insert(key.to_string(), json_value);
            }
            _ => {}
        }
    }
    if is_array {
        serde_json::Value::Array(array)
    } else {
        serde_json::Value::Object(object)
    }
}


