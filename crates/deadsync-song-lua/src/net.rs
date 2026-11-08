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
        with_query_value_text(key, |key| {
            let Some(key) = key else {
                return;
            };
            with_query_value_text(value, |value| {
                let value = value.unwrap_or_default();
                let key_len = encoded_component_len(key);
                let value_len = encoded_component_len(value);
                let mut part = String::with_capacity(key_len + 1 + value_len);
                append_encoded_component(&mut part, key, key_len);
                part.push('=');
                append_encoded_component(&mut part, value, value_len);
                parts.push(part);
            });
        });
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

#[must_use]
pub fn url_encode_component(text: &str) -> String {
    let size = encoded_component_len(text);
    if size == text.len() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(size);
    append_encoded_component(&mut out, text, size);
    out
}

#[inline]
const fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn encoded_component_len(text: &str) -> usize {
    // A byte lookup keeps exact sizing inexpensive even for long escaped inputs.
    const EXTRA: [u8; 256] = {
        let mut extra = [2; 256];
        let mut i = 0;
        while i < 256 {
            let byte = i as u8;
            if unreserved(byte) {
                extra[i] = 0;
            }
            i += 1;
        }
        extra
    };
    text.len()
        + text
            .bytes()
            .map(|byte| usize::from(EXTRA[usize::from(byte)]))
            .sum::<usize>()
}

fn append_encoded_component(out: &mut String, text: &str, encoded_len: usize) {
    if encoded_len == text.len() {
        out.push_str(text);
        return;
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in text.bytes() {
        if unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

fn with_query_value_text<R>(value: Value, use_text: impl FnOnce(Option<&str>) -> R) -> R {
    match value {
        Value::String(value) => {
            let bytes = value.as_bytes();
            let text = String::from_utf8_lossy(&bytes);
            use_text(Some(&text))
        }
        Value::Integer(value) => {
            let mut buffer = itoa::Buffer::new();
            use_text(Some(buffer.format(value)))
        }
        Value::Number(value) => {
            let text = value.to_string();
            use_text(Some(&text))
        }
        Value::Boolean(value) => use_text(Some(if value { "true" } else { "false" })),
        _ => use_text(None),
    }
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

#[cfg(test)]
#[path = "../tests/perf/query_encode.rs"]
mod perf;
