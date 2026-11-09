//! Existing JSON/BinaryToHex host conversions, including their depth and sparse-array rules.
use super::*;
use serde_json::Value as Json;

#[derive(Clone, Copy)]
enum Operation {
    Decode,
    Encode,
    Hex,
}
pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    for (name, op) in [
        ("JsonDecode", Operation::Decode),
        ("JsonEncode", Operation::Encode),
        ("BinaryToHex", Operation::Hex),
    ] {
        let function = vm.new_host_function_with_data(dispatch, &[], op)?;
        bind(vm, name, function)?;
    }
    Ok(())
}
fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    match *call.function_data::<Operation>().unwrap() {
        Operation::Decode => {
            let value = match arg(call, 0) {
                Value::String(id) => {
                    serde_json::from_slice::<Json>(call.string_bytes(id).unwrap()).ok()
                }
                _ => None,
            };
            let mut stack = call.native_stack()?;
            if let Some(value) = value {
                push_json(&mut stack, value)?;
            } else {
                stack.create_table(0, 0)?;
            }
            stack.finish(1)
        }
        Operation::Encode => {
            let value = encode(call, arg(call, 0), 0)?;
            let text = serde_json::to_string(&value).unwrap_or_default();
            let text = call.try_intern(text.as_bytes())?;
            call.push(text)
        }
        Operation::Hex => {
            let mut text = String::new();
            if let Value::String(id) = arg(call, 0) {
                use std::fmt::Write;
                for byte in call.string_bytes(id).unwrap() {
                    write!(text, "{byte:02X}").unwrap();
                }
            }
            let value = call.try_intern(text.as_bytes())?;
            call.push(value)
        }
    }
}
fn push_json(stack: &mut deadlib_lua::NativeStack<'_, '_>, value: Json) -> Result<(), Error> {
    match value {
        Json::Null => stack.push(Value::Nil),
        Json::Bool(value) => stack.push(Value::Boolean(value)),
        Json::Number(value) => stack.push(value.as_f64().map_or(Value::Nil, Value::Number)),
        Json::String(value) => stack.push_bytes(value.as_bytes()),
        Json::Array(values) => {
            let capacity = if values.iter().all(|v| !v.is_null()) {
                values.len()
            } else {
                0
            };
            stack.create_table(capacity, 0)?;
            let table = stack.top() as isize;
            for (index, value) in values.into_iter().enumerate() {
                stack.push(Value::Number((index + 1) as f64))?;
                push_json(stack, value)?;
                stack.set_table(table, true)?;
            }
            Ok(())
        }
        Json::Object(values) => {
            stack.create_table(0, values.values().filter(|v| !v.is_null()).count())?;
            let table = stack.top() as isize;
            for (key, value) in values {
                stack.push_bytes(key.as_bytes())?;
                push_json(stack, value)?;
                stack.set_table(table, true)?;
            }
            Ok(())
        }
    }
}
fn encode(call: &mut HostCall<'_>, value: Value, depth: usize) -> Result<Json, Error> {
    if depth >= 16 {
        return Ok(Json::Null);
    }
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Boolean(value) => Json::Bool(value),
        Value::Number(value) => native_integer(value).map_or_else(
            || serde_json::Number::from_f64(value).map_or(Json::Null, Json::Number),
            |value| Json::Number(value.into()),
        ),
        Value::String(id) => {
            Json::String(String::from_utf8_lossy(call.string_bytes(id).unwrap()).into_owned())
        }
        Value::Table(table) => encode_table(call, table, depth + 1)?,
        _ => Json::Null,
    })
}
fn encode_table(call: &mut HostCall<'_>, table: TableId, depth: usize) -> Result<Json, Error> {
    let len = call.raw_table_len(table)?;
    let mut array = vec![Json::Null; len];
    let mut object = serde_json::Map::new();
    let mut is_array = len > 0;
    let entries = rows(call, table)?;
    let roots: Vec<_> = entries.iter().flat_map(|&(k, v)| [k, v]).collect();
    call.with_roots(&roots, |call| {
        for (key, value) in entries {
            let value = encode(call, value, depth)?;
            if let Value::Number(key) = key
                && let Some(index) = native_integer(key)
                && is_array
                && index >= 1
                && (index as usize) <= len
            {
                array[index as usize - 1] = value;
            } else if let Some(key) = value_text(call, key, true)? {
                is_array = false;
                object.insert(key, value);
            }
        }
        Ok(if is_array {
            Json::Array(array)
        } else {
            Json::Object(object)
        })
    })
}
