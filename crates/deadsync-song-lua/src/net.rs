use mlua::{Lua, MultiValue, Table, Value};

use crate::{
    create_network_response_table, create_websocket_table, method_arg, note_song_lua_side_effect,
};

pub fn create_network_table(lua: &Lua) -> mlua::Result<Table> {
    let network = lua.create_table()?;
    network.set(
        "IsUrlAllowed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    network.set(
        "HttpRequest",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            create_network_response_table(lua)
        })?,
    )?;
    network.set(
        "WebSocket",
        lua.create_function(|lua, _args: MultiValue| create_websocket_table(lua))?,
    )?;
    network.set(
        "EncodeQueryParameters",
        lua.create_function(|_, args: MultiValue| {
            let Some(Value::Table(query)) = method_arg(&args, 0).cloned() else {
                return Ok(String::new());
            };
            encode_query_params(query)
        })?,
    )?;
    Ok(network)
}

pub fn encode_query_params(query: Table) -> mlua::Result<String> {
    let mut parts = Vec::new();
    for pair in query.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Some(key) = query_value_text(key) else {
            continue;
        };
        let value = query_value_text(value).unwrap_or_default();
        parts.push(format!(
            "{}={}",
            url_encode_component(&key),
            url_encode_component(&value)
        ));
    }
    parts.sort_unstable();
    Ok(parts.join("&"))
}

pub fn query_value_text(value: Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_string_lossy()),
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

pub fn url_encode_component(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for byte in text.as_bytes().iter().copied() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_component_preserves_unreserved_bytes() {
        assert_eq!(url_encode_component("abc-._~"), "abc-._~");
        assert_eq!(url_encode_component("a b+c/%"), "a%20b%2Bc%2F%25");
    }
}
