// Frozen command preprocessor from 7508e0b1b (0.5.1630).
/// # Panics
///
/// Panics if an internal state invariant is violated.
pub(super) fn preprocess_lua_cmd_syntax(source: &str) -> Result<String, String> {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        if source[index..].starts_with("--") {
            let end = lua_comment_end(source, index);
            out.push_str(&source[index..end]);
            index = end;
        } else if matches!(bytes[index], b'\'' | b'"') {
            let end = lua_quoted_end(source, index)?;
            out.push_str(&source[index..end]);
            index = end;
        } else if let Some(open_end) = lua_long_bracket_end(source, index) {
            let end = lua_long_string_end(source, index, open_end)?;
            out.push_str(&source[index..end]);
            index = end;
        } else if let Some(open) = lua_cmd_open_at(source, index) {
            index = parse_lua_cmd_call(source, open, &mut out)?;
        } else {
            let ch = source[index..].chars().next().unwrap();
            out.push(ch);
            index += ch.len_utf8();
        }
    }
    Ok(out)
}

pub(super) fn lua_cmd_open_at(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if !source[index..].starts_with("cmd") {
        return None;
    }
    let before_is_ident = index > 0 && lua_ident_byte(bytes[index - 1]);
    let after = index + 3;
    if before_is_ident || after >= bytes.len() || lua_ident_byte(bytes[after]) {
        return None;
    }
    lua_skip_ws(source, after).filter(|&next| bytes.get(next) == Some(&b'('))
}

pub(super) fn parse_lua_cmd_call(
    source: &str,
    open: usize,
    out: &mut String,
) -> Result<usize, String> {
    let close = lua_matching_paren(source, open)?;
    let body = &source[open + 1..close];
    lua_cmd_function(body, out)?;
    Ok(close + 1)
}

pub(super) fn lua_cmd_function(body: &str, out: &mut String) -> Result<(), String> {
    let body = lua_cmd_without_comments(body)?;
    out.push_str("function(self) ");
    for command in lua_cmd_commands(&body)? {
        let command = command.trim();
        if command.is_empty() {
            continue;
        }
        let (name, rest) = lua_cmd_name(command)?;
        let rest = rest.trim_start();
        let args = if rest.is_empty() {
            ""
        } else if let Some(args) = rest.strip_prefix(',') {
            args.trim()
        } else {
            return Err(format!("invalid cmd command '{command}'"));
        };
        out.push_str("self:");
        out.push_str(name);
        out.push('(');
        out.push_str(args);
        out.push_str("); ");
    }
    out.push_str("return self end");
    Ok(())
}

pub(super) fn lua_cmd_without_comments(body: &str) -> Result<String, String> {
    let mut out = String::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        if body[index..].starts_with("--") {
            let end = lua_comment_end(body, index);
            out.push(' ');
            out.extend(body[index..end].chars().filter(|&ch| ch == '\n'));
            index = end;
            continue;
        }
        let end = if matches!(body.as_bytes()[index], b'\'' | b'"') {
            lua_quoted_end(body, index)?
        } else if let Some(open_end) = lua_long_bracket_end(body, index) {
            lua_long_string_end(body, index, open_end)?
        } else {
            index
                + body[index..]
                    .chars()
                    .next()
                    .expect("character boundary")
                    .len_utf8()
        };
        out.push_str(&body[index..end]);
        index = end;
    }
    Ok(out)
}

pub(super) fn lua_cmd_name(command: &str) -> Result<(&str, &str), String> {
    let bytes = command.as_bytes();
    if bytes
        .first()
        .is_none_or(|byte| !byte.is_ascii_alphabetic() && *byte != b'_')
    {
        return Err(format!("invalid cmd command '{command}'"));
    }
    let mut end = 1;
    while end < bytes.len() && lua_ident_byte(bytes[end]) {
        end += 1;
    }
    Ok((&command[..end], &command[end..]))
}

pub(super) fn lua_cmd_commands(body: &str) -> Result<Vec<&str>, String> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut paren = 0_i32;
    let mut brace = 0_i32;
    let mut bracket = 0_i32;
    while index < bytes.len() {
        if body[index..].starts_with("--") {
            index = lua_comment_end(body, index);
        } else if matches!(bytes[index], b'\'' | b'"') {
            index = lua_quoted_end(body, index)?;
        } else if let Some(open_end) = lua_long_bracket_end(body, index) {
            index = lua_long_string_end(body, index, open_end)?;
        } else {
            match bytes[index] {
                b'(' => paren += 1,
                b')' => paren -= 1,
                b'{' => brace += 1,
                b'}' => brace -= 1,
                b'[' => bracket += 1,
                b']' => bracket -= 1,
                b';' if paren == 0 && brace == 0 && bracket == 0 => {
                    out.push(&body[start..index]);
                    start = index + 1;
                }
                _ => {}
            }
            index += 1;
        }
    }
    out.push(&body[start..]);
    Ok(out)
}

pub(super) fn lua_matching_paren(source: &str, open: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let mut index = open + 1;
    let mut depth = 1_i32;
    while index < bytes.len() {
        if source[index..].starts_with("--") {
            index = lua_comment_end(source, index);
        } else if matches!(bytes[index], b'\'' | b'"') {
            index = lua_quoted_end(source, index)?;
        } else if let Some(open_end) = lua_long_bracket_end(source, index) {
            index = lua_long_string_end(source, index, open_end)?;
        } else {
            match bytes[index] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(index);
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
    Err("unterminated cmd expression".to_string())
}

pub(super) fn lua_skip_ws(source: &str, mut index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    (index < bytes.len()).then_some(index)
}

const fn lua_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(super) fn lua_comment_end(source: &str, index: usize) -> usize {
    if let Some(open_end) = lua_long_bracket_end(source, index + 2)
        && let Ok(end) = lua_long_string_end(source, index + 2, open_end)
    {
        return end;
    }
    source[index..]
        .find('\n')
        .map(|offset| index + offset)
        .unwrap_or(source.len())
}

pub(super) fn lua_quoted_end(source: &str, index: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let quote = bytes[index];
    let mut cursor = index + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
        } else if bytes[cursor] == quote {
            return Ok(cursor + 1);
        } else {
            cursor += 1;
        }
    }
    Err("unterminated Lua string".to_string())
}

pub(super) fn lua_long_bracket_end(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(index) != Some(&b'[') {
        return None;
    }
    let mut cursor = index + 1;
    while bytes.get(cursor) == Some(&b'=') {
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'[')).then_some(cursor + 1)
}

pub(super) fn lua_long_string_end(
    source: &str,
    index: usize,
    open_end: usize,
) -> Result<usize, String> {
    let equals = &source[index + 1..open_end - 1];
    let close = format!("]{equals}]");
    source[open_end..]
        .find(&close)
        .map(|offset| open_end + offset + close.len())
        .ok_or_else(|| "unterminated Lua long string".to_string())
}
