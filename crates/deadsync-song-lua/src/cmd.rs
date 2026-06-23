pub fn preprocess_lua_cmd_syntax(source: &str) -> Result<String, String> {
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
        } else if lua_long_bracket_end(source, index).is_some() {
            let end = lua_long_string_end(source, index)?;
            out.push_str(&source[index..end]);
            index = end;
        } else if lua_cmd_token_at(source, index) {
            let (replacement, end) = parse_lua_cmd_call(source, index)?;
            out.push_str(&replacement);
            index = end;
        } else {
            let ch = source[index..].chars().next().unwrap();
            out.push(ch);
            index += ch.len_utf8();
        }
    }
    Ok(out)
}

fn lua_cmd_token_at(source: &str, index: usize) -> bool {
    let bytes = source.as_bytes();
    if !source[index..].starts_with("cmd") {
        return false;
    }
    let before_is_ident = index > 0 && lua_ident_byte(bytes[index - 1]);
    let after = index + 3;
    if before_is_ident || after >= bytes.len() || lua_ident_byte(bytes[after]) {
        return false;
    }
    lua_skip_ws(source, after).is_some_and(|next| source.as_bytes().get(next) == Some(&b'('))
}

fn parse_lua_cmd_call(source: &str, index: usize) -> Result<(String, usize), String> {
    let Some(open) = lua_skip_ws(source, index + 3) else {
        return Err("unterminated cmd expression".to_string());
    };
    let close = lua_matching_paren(source, open)?;
    let body = &source[open + 1..close];
    let replacement = lua_cmd_function(body)?;
    Ok((replacement, close + 1))
}

fn lua_cmd_function(body: &str) -> Result<String, String> {
    let mut out = String::from("function(self) ");
    for command in lua_cmd_commands(body)? {
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
    Ok(out)
}

fn lua_cmd_name(command: &str) -> Result<(&str, &str), String> {
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

fn lua_cmd_commands(body: &str) -> Result<Vec<&str>, String> {
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
        } else if lua_long_bracket_end(body, index).is_some() {
            index = lua_long_string_end(body, index)?;
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

fn lua_matching_paren(source: &str, open: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let mut index = open + 1;
    let mut depth = 1_i32;
    while index < bytes.len() {
        if source[index..].starts_with("--") {
            index = lua_comment_end(source, index);
        } else if matches!(bytes[index], b'\'' | b'"') {
            index = lua_quoted_end(source, index)?;
        } else if lua_long_bracket_end(source, index).is_some() {
            index = lua_long_string_end(source, index)?;
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

fn lua_skip_ws(source: &str, mut index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    (index < bytes.len()).then_some(index)
}

fn lua_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn lua_comment_end(source: &str, index: usize) -> usize {
    if source[index + 2..].starts_with('[')
        && let Ok(end) = lua_long_string_end(source, index + 2)
    {
        return end;
    }
    source[index..]
        .find('\n')
        .map(|offset| index + offset)
        .unwrap_or(source.len())
}

fn lua_quoted_end(source: &str, index: usize) -> Result<usize, String> {
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

fn lua_long_bracket_end(source: &str, index: usize) -> Option<usize> {
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

fn lua_long_string_end(source: &str, index: usize) -> Result<usize, String> {
    let Some(open_end) = lua_long_bracket_end(source, index) else {
        return Err("invalid Lua long string".to_string());
    };
    let equals = &source[index + 1..open_end - 1];
    let close = format!("]{equals}]");
    source[open_end..]
        .find(&close)
        .map(|offset| open_end + offset + close.len())
        .ok_or_else(|| "unterminated Lua long string".to_string())
}

#[cfg(test)]
mod tests {
    use super::preprocess_lua_cmd_syntax;

    #[test]
    fn preprocess_lua_cmd_turns_commands_into_function() {
        assert_eq!(
            preprocess_lua_cmd_syntax("return cmd(diffuse, 1, 0, 0, 1; zoom, 2)").unwrap(),
            "return function(self) self:diffuse(1, 0, 0, 1); self:zoom(2); return self end"
        );
    }

    #[test]
    fn preprocess_lua_cmd_ignores_strings_and_comments() {
        let source = "local s = \"cmd(zoom, 2)\" -- cmd(diffuse, 1)\nreturn cmd(linear, 1)";

        assert_eq!(
            preprocess_lua_cmd_syntax(source).unwrap(),
            "local s = \"cmd(zoom, 2)\" -- cmd(diffuse, 1)\nreturn function(self) self:linear(1); return self end"
        );
    }
}
