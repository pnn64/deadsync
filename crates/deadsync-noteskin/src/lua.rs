#[must_use]
pub fn itg_parse_lua_quoted(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim();
    if trimmed.len() < 2 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let quote = bytes[0];
    if (quote != b'"' && quote != b'\'') || bytes[trimmed.len() - 1] != quote {
        return None;
    }
    Some(trimmed[1..trimmed.len() - 1].to_string())
}

#[must_use]
pub fn itg_find_matching(content: &str, open_idx: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in content.char_indices().skip_while(|(i, _)| *i < open_idx) {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

#[must_use]
pub const fn itg_skip_ws(content: &str, mut idx: usize) -> usize {
    let bytes = content.as_bytes();
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    idx
}

#[must_use]
pub fn itg_split_call_args(raw: &str) -> Vec<String> {
    itg_call_args(raw).map(str::to_owned).collect()
}

/// Borrowed, top-level arguments from an ITG Lua method call.
#[derive(Clone, Debug)]
pub struct ItgCallArgs<'a> {
    raw: &'a str,
    start: usize,
    cursor: usize,
    depth: usize,
    quote: u8,
    finished: bool,
}

#[must_use]
pub fn itg_call_args(raw: &str) -> ItgCallArgs<'_> {
    ItgCallArgs {
        raw,
        start: 0,
        cursor: 0,
        depth: 0,
        quote: 0,
        finished: false,
    }
}

impl<'a> Iterator for ItgCallArgs<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let bytes = self.raw.as_bytes();
        while self.cursor < bytes.len() {
            let byte = bytes[self.cursor];
            if self.quote != 0 {
                if byte == self.quote {
                    self.quote = 0;
                }
            } else {
                match byte {
                    b'"' | b'\'' => self.quote = byte,
                    b'(' | b'{' | b'[' => self.depth += 1,
                    b')' | b'}' | b']' => self.depth = self.depth.saturating_sub(1),
                    b',' if self.depth == 0 => {
                        let part = self.raw[self.start..self.cursor].trim();
                        self.cursor += 1;
                        self.start = self.cursor;
                        if !part.is_empty() {
                            return Some(part);
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            self.cursor += 1;
        }

        self.finished = true;
        let tail = self.raw[self.start..].trim();
        (!tail.is_empty()).then_some(tail)
    }
}

fn strip_wrapped_parens(raw: &str) -> &str {
    let mut value = raw.trim();
    loop {
        if !(value.starts_with('(') && value.ends_with(')')) {
            break;
        }
        let Some(close) = itg_find_matching(value, 0, '(', ')') else {
            break;
        };
        if close + 1 != value.len() {
            break;
        }
        value = value[1..value.len() - 1].trim();
    }
    value
}

#[must_use]
pub fn itg_parse_lua_float_expr(raw: &str) -> Option<f32> {
    let value = strip_wrapped_parens(raw.trim().trim_end_matches(';'));
    if let Some(v) = itg_parse_lua_float_token(value) {
        return Some(v);
    }
    let bytes = value.as_bytes();
    let mut depth = 0usize;
    for (idx, b) in bytes.iter().enumerate() {
        match *b {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'/' if depth == 0 => {
                let lhs = value[..idx].trim();
                let rhs = value[idx + 1..].trim();
                let denom = itg_parse_lua_float_expr(rhs)?;
                if denom.abs() <= f32::EPSILON {
                    return None;
                }
                return Some(itg_parse_lua_float_expr(lhs)? / denom);
            }
            _ => {}
        }
    }
    None
}

#[must_use]
pub fn itg_find_function_end(content: &str, mut cursor: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut depth = 1usize;
    let mut quote = 0u8;
    while cursor < bytes.len() {
        let b = bytes[cursor];
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            cursor += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            quote = b;
            cursor += 1;
            continue;
        }
        if content[cursor..].starts_with("function")
            && itg_token_boundary(bytes, cursor, "function".len())
        {
            depth += 1;
            cursor += "function".len();
            continue;
        }
        if content[cursor..].starts_with("end") && itg_token_boundary(bytes, cursor, "end".len()) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(cursor);
            }
            cursor += "end".len();
            continue;
        }
        cursor += 1;
    }
    None
}

const fn itg_token_boundary(bytes: &[u8], start: usize, len: usize) -> bool {
    let prev_ok = if start == 0 {
        true
    } else {
        !is_lua_ident(bytes[start - 1])
    };
    let end = start + len;
    let next_ok = if end >= bytes.len() {
        true
    } else {
        !is_lua_ident(bytes[end])
    };
    prev_ok && next_ok
}

const fn is_lua_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[must_use]
pub fn itg_parse_self_chain_commands(body: &str) -> Option<String> {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find("self:") {
        let mut name_start = cursor + rel + 5;
        loop {
            let Some((name, args, next)) = itg_parse_lua_method_call(body, name_start) else {
                cursor = name_start;
                break;
            };
            if out.capacity() == 0 {
                out.reserve(body.len());
            } else {
                out.push(';');
            }
            out.push_str(name);
            if !args.is_empty() {
                out.push(',');
                out.push_str(args);
            }
            cursor = next;

            let chain = itg_skip_ws(body, next);
            if body.as_bytes().get(chain).is_some_and(|b| *b == b':') {
                name_start = chain + 1;
                continue;
            }
            break;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn itg_parse_lua_method_call(body: &str, name_start: usize) -> Option<(&str, &str, usize)> {
    let bytes = body.as_bytes();
    let mut name_end = name_start;
    while name_end < bytes.len() && is_lua_ident(bytes[name_end]) {
        name_end += 1;
    }
    if name_end == name_start {
        return None;
    }
    let open = itg_skip_ws(body, name_end);
    if bytes.get(open).is_none_or(|b| *b != b'(') {
        return None;
    }
    let close = itg_find_matching(body, open, '(', ')')?;
    Some((
        body[name_start..name_end].trim(),
        body[open + 1..close].trim(),
        close + 1,
    ))
}

#[must_use]
pub fn itg_extract_quoted_strings(input: &str) -> Vec<String> {
    itg_quoted_strings(input).map(str::to_owned).collect()
}

/// Borrowed quoted substrings, in source order, using ITG's simple quote rules.
#[derive(Clone, Debug)]
pub struct ItgQuotedStrings<'a> {
    input: &'a str,
    cursor: usize,
}

#[must_use]
pub const fn itg_quoted_strings(input: &str) -> ItgQuotedStrings<'_> {
    ItgQuotedStrings { input, cursor: 0 }
}

impl<'a> Iterator for ItgQuotedStrings<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.input.as_bytes();
        while self.cursor < bytes.len() && !matches!(bytes[self.cursor], b'"' | b'\'') {
            self.cursor += 1;
        }
        let quote = *bytes.get(self.cursor)?;
        self.cursor += 1;
        let start = self.cursor;
        while self.cursor < bytes.len() && bytes[self.cursor] != quote {
            self.cursor += 1;
        }
        let value = &self.input[start..self.cursor];
        self.cursor = self.cursor.saturating_add(1);
        Some(value)
    }
}

fn itg_parse_lua_float_token(raw: &str) -> Option<f32> {
    let value = raw.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    if let Ok(v) = value.parse::<f32>() {
        return Some(v);
    }
    if value.contains(',') && !value.contains('.') {
        let patched = value.replace(',', ".");
        return patched.parse::<f32>().ok();
    }
    None
}
