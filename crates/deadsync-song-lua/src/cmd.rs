#[inline]
fn lua_special_offset(bytes: &[u8]) -> usize {
    // A scalar prefix handles tightly packed markers. Fixed-size search
    // windows keep the two delimiter sets linear even when one set is absent.
    // Every returned position is an ASCII marker or EOF, hence a UTF-8 boundary.
    let prefix = bytes.len().min(16);
    if let Some(at) = bytes[..prefix]
        .iter()
        .position(|&byte| matches!(byte, b'-' | b'[' | b'c' | b'\'' | b'"'))
    {
        return at;
    }
    for (base, chunk) in bytes[prefix..].chunks(64).enumerate() {
        let a = memchr::memchr3(b'-', b'[', b'c', chunk);
        let b = memchr::memchr2(b'\'', b'"', chunk);
        if let Some(at) = a.into_iter().chain(b).min() {
            return prefix + base * 64 + at;
        }
    }
    bytes.len()
}
/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn preprocess_lua_cmd_syntax(source: &str) -> Result<String, String> {
    preprocess_lua_cmd_syntax_cow(source).map(std::borrow::Cow::into_owned)
}

pub(crate) fn preprocess_lua_cmd_syntax_cow(
    source: &str,
) -> Result<std::borrow::Cow<'_, str>, String> {
    let bytes = source.as_bytes();
    let mut out: Option<String> = None;
    let mut copied_until = 0;
    let mut index = 0;
    while index < bytes.len() {
        if source[index..].starts_with("--") {
            index = lua_comment_end(source, index);
        } else if matches!(bytes[index], b'\'' | b'"') {
            index = lua_quoted_end(source, index)?;
        } else if let Some(open_end) = lua_long_bracket_end(source, index) {
            index = lua_long_string_end(source, index, open_end)?;
        } else if let Some(open) = lua_cmd_open_at(source, index) {
            let out = out.get_or_insert_with(|| String::with_capacity(source.len()));
            out.push_str(&source[copied_until..index]);
            // Native cmd retains the newlines consumed before its parenthesis.
            out.extend(source[index..open].chars().filter(|&ch| ch == '\n'));
            index = parse_lua_cmd_call(source, open, out)?;
            copied_until = index;
        } else if matches!(bytes[index], b'c' | b'-' | b'[') {
            index += 1;
        } else {
            index += lua_special_offset(&bytes[index..]);
        }
    }
    if let Some(mut out) = out {
        out.push_str(&source[copied_until..]);
        Ok(std::borrow::Cow::Owned(out))
    } else {
        Ok(std::borrow::Cow::Borrowed(source))
    }
}

fn lua_cmd_open_at(source: &str, index: usize) -> Option<usize> {
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

fn parse_lua_cmd_call(source: &str, open: usize, out: &mut String) -> Result<usize, String> {
    let close = lua_matching_paren(source, open)?;
    let body = &source[open + 1..close];
    lua_cmd_function(body, out)?;
    Ok(close + 1)
}

fn lua_cmd_function(body: &str, out: &mut String) -> Result<(), String> {
    let body = lua_cmd_without_comments(body)?;
    out.push_str("function(self) ");
    for command in lua_cmd_commands(&body)? {
        let leading = command.len() - command.trim_start().len();
        out.extend(command[..leading].chars().filter(|&ch| ch == '\n'));
        let trailing = command.trim_end().len();
        let trailing_lines = command[trailing..].chars().filter(|&ch| ch == '\n').count();
        let command = command.trim();
        if command.is_empty() {
            continue;
        }
        let (name, rest) = lua_cmd_name(command)?;
        let name_lines = rest[..rest.len() - rest.trim_start().len()]
            .chars()
            .filter(|&ch| ch == '\n')
            .count();
        let rest = rest.trim_start();
        let (args, argument_lines) = if rest.is_empty() {
            ("", 0)
        } else if let Some(args) = rest.strip_prefix(',') {
            let leading = args.len() - args.trim_start().len();
            (
                args.trim(),
                args[..leading].chars().filter(|&ch| ch == '\n').count(),
            )
        } else {
            return Err(format!("invalid cmd command '{command}'"));
        };
        out.push_str("self:");
        out.push_str(name);
        out.push('(');
        out.extend(std::iter::repeat_n('\n', name_lines + argument_lines));
        out.push_str(args);
        out.extend(std::iter::repeat_n('\n', trailing_lines));
        out.push_str("); ");
    }
    out.push_str("return self end");
    Ok(())
}

fn lua_cmd_without_comments(body: &str) -> Result<std::borrow::Cow<'_, str>, String> {
    // The matching-parenthesis scan already validated string boundaries.
    // A body without a comment marker needs no copy or second character scan.
    if !body.contains("--") {
        return Ok(std::borrow::Cow::Borrowed(body));
    }
    let mut out: Option<String> = None;
    let mut copied_until = 0;
    let mut index = 0;
    while index < body.len() {
        if body[index..].starts_with("--") {
            let end = lua_comment_end(body, index);
            let out = out.get_or_insert_with(|| String::with_capacity(body.len()));
            out.push_str(&body[copied_until..index]);
            out.push(' ');
            out.extend(body[index..end].chars().filter(|&ch| ch == '\n'));
            copied_until = end;
            index = end;
            continue;
        }
        index = if matches!(body.as_bytes()[index], b'\'' | b'"') {
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
    }
    if let Some(mut out) = out {
        out.push_str(&body[copied_until..]);
        Ok(std::borrow::Cow::Owned(out))
    } else {
        Ok(std::borrow::Cow::Borrowed(body))
    }
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

fn lua_cmd_commands(body: &str) -> Result<smallvec::SmallVec<[&str; 8]>, String> {
    let bytes = body.as_bytes();
    let mut out = smallvec::SmallVec::new();
    let mut start = 0;
    let mut index = 0;
    loop {
        index = lua_cmd_space(body, index);
        if index == body.len() {
            out.push(&body[start..]);
            break;
        }
        if bytes[index] == b';' {
            out.push(&body[start..index]);
            index += 1;
            start = index;
            continue;
        }
        let (name, _) = lua_cmd_name(&body[index..])?;
        index = lua_cmd_space(body, index + name.len());
        if bytes.get(index) == Some(&b',') {
            loop {
                index = lua_cmd_expression(body, lua_cmd_space(body, index + 1))?;
                index = lua_cmd_space(body, index);
                if bytes.get(index) != Some(&b',') {
                    break;
                }
            }
        } else if index != body.len() && bytes[index] != b';' {
            return Err(format!("invalid cmd command '{}'", body[start..].trim()));
        }
        // Native cmd_body resumes at the next Name after explist1. In
        // particular, `sleep,0 pulse` contains two commands without a `;`.
        out.push(&body[start..index]);
        start = index;
        if bytes.get(index) == Some(&b';') {
            index += 1;
            start = index;
        }
    }
    Ok(out)
}

fn lua_cmd_space(source: &str, mut index: usize) -> usize {
    while source
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    index
}

fn lua_cmd_word_end(source: &str, mut index: usize) -> usize {
    while source
        .as_bytes()
        .get(index)
        .is_some_and(|byte| lua_ident_byte(*byte))
    {
        index += 1;
    }
    index
}

// Find an expression's token boundary rather than evaluating it. Operator
// precedence changes grouping, but not the end of a Lua expression.
fn lua_cmd_expression(source: &str, mut index: usize) -> Result<usize, String> {
    loop {
        index = lua_cmd_operand(source, index)?;
        let next = lua_cmd_space(source, index);
        let rest = &source[next..];
        let operator = [
            "..", "==", "~=", "<=", ">=", "+", "-", "*", "/", "%", "^", "<", ">",
        ]
        .into_iter()
        .find(|operator| rest.starts_with(operator))
        .map(str::len)
        .or_else(|| {
            ["and", "or"]
                .into_iter()
                .find(|word| {
                    rest.starts_with(word)
                        && rest
                            .as_bytes()
                            .get(word.len())
                            .is_none_or(|byte| !lua_ident_byte(*byte))
                })
                .map(str::len)
        });
        let Some(operator) = operator else {
            return Ok(next);
        };
        index = lua_cmd_space(source, next + operator);
    }
}

fn lua_cmd_operand(source: &str, mut index: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    loop {
        index = lua_cmd_space(source, index);
        if matches!(bytes.get(index), Some(b'-' | b'#')) {
            index += 1;
        } else if source[index..].starts_with("not")
            && bytes
                .get(index + 3)
                .is_none_or(|byte| !lua_ident_byte(*byte))
        {
            index += 3;
        } else {
            break;
        }
    }
    let Some(&first) = bytes.get(index) else {
        return Err("missing cmd argument expression".into());
    };
    let mut suffixes = false;
    if matches!(first, b'\'' | b'"') {
        index = lua_quoted_end(source, index)?;
    } else if let Some(open_end) = lua_long_bracket_end(source, index) {
        index = lua_long_string_end(source, index, open_end)?;
    } else if first.is_ascii_digit()
        || first == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
    {
        index = lua_cmd_numeral_end(source, index)?;
    } else if source[index..].starts_with("...") {
        index += 3;
    } else if first == b'(' || first == b'{' {
        suffixes = first == b'(';
        index = lua_cmd_group_end(source, index)?;
    } else if first.is_ascii_alphabetic() || first == b'_' {
        let end = lua_cmd_word_end(source, index);
        if &source[index..end] == "function" {
            index = lua_cmd_function_end(source, end)?;
        } else {
            suffixes = !matches!(&source[index..end], "nil" | "true" | "false");
            index = end;
        }
    } else {
        return Err(format!(
            "invalid cmd argument expression near '{}'",
            &source[index..]
        ));
    }
    while suffixes {
        let next = lua_cmd_space(source, index);
        match bytes.get(next) {
            Some(b'.' | b':') if !source[next..].starts_with("..") => {
                let name = lua_cmd_space(source, next + 1);
                let (word, _) = lua_cmd_name(&source[name..])?;
                index = name + word.len();
            }
            Some(b'[') if lua_long_bracket_end(source, next).is_some() => {
                index =
                    lua_long_string_end(source, next, lua_long_bracket_end(source, next).unwrap())?;
            }
            Some(b'(' | b'{' | b'[') => index = lua_cmd_group_end(source, next)?,
            Some(b'\'' | b'"') => index = lua_quoted_end(source, next)?,
            _ => break,
        }
    }
    Ok(index)
}

fn lua_cmd_numeral_end(source: &str, mut index: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    if source[index..].starts_with("0x") || source[index..].starts_with("0X") {
        index += 2;
        while bytes.get(index).is_some_and(u8::is_ascii_hexdigit) {
            index += 1;
        }
    } else {
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
        {
            index += 1;
        }
        if matches!(bytes.get(index), Some(b'e' | b'E')) {
            index += 1;
            if matches!(bytes.get(index), Some(b'+' | b'-')) {
                index += 1;
            }
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
    }
    if bytes.get(index).is_some_and(|byte| lua_ident_byte(*byte)) {
        return Err("invalid numeral in cmd argument".into());
    }
    Ok(index)
}

fn lua_cmd_group_end(source: &str, open: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let mut closing = smallvec::SmallVec::<[u8; 8]>::new();
    let mut index = open;
    while index < bytes.len() {
        if matches!(bytes[index], b'\'' | b'"') {
            index = lua_quoted_end(source, index)?;
        } else if let Some(open_end) = lua_long_bracket_end(source, index) {
            index = lua_long_string_end(source, index, open_end)?;
        } else {
            match bytes[index] {
                b'(' => closing.push(b')'),
                b'{' => closing.push(b'}'),
                b'[' => closing.push(b']'),
                b')' | b'}' | b']' => {
                    if closing.pop() != Some(bytes[index]) {
                        return Err("unmatched cmd argument delimiter".into());
                    }
                    if closing.is_empty() {
                        return Ok(index + 1);
                    }
                }
                _ => {}
            }
            index += source[index..]
                .chars()
                .next()
                .expect("character boundary")
                .len_utf8();
        }
    }
    Err("unterminated cmd argument delimiter".into())
}

fn lua_cmd_function_end(source: &str, mut index: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let mut blocks = 1;
    while index < bytes.len() {
        if matches!(bytes[index], b'\'' | b'"') {
            index = lua_quoted_end(source, index)?;
        } else if let Some(open_end) = lua_long_bracket_end(source, index) {
            index = lua_long_string_end(source, index, open_end)?;
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let end = lua_cmd_word_end(source, index);
            match &source[index..end] {
                "function" | "if" | "do" | "repeat" => blocks += 1,
                "end" | "until" => blocks -= 1,
                _ => {}
            }
            if blocks == 0 {
                return Ok(end);
            }
            index = end;
        } else {
            index += source[index..]
                .chars()
                .next()
                .expect("character boundary")
                .len_utf8();
        }
    }
    Err("unterminated cmd function argument".into())
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

fn lua_skip_ws(source: &str, mut index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    (index < bytes.len()).then_some(index)
}

const fn lua_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn lua_comment_end(source: &str, index: usize) -> usize {
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

fn lua_quoted_end(source: &str, index: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let quote = bytes[index];
    let mut cursor = index + 1;
    // Preserve the cheap scalar loop for ordinary short literals.
    let prefix_end = cursor.saturating_add(32).min(bytes.len());
    while cursor < prefix_end {
        if bytes[cursor] == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
        } else if bytes[cursor] == quote {
            return Ok(cursor + 1);
        } else {
            cursor += 1;
        }
    }
    let mut dense_escapes = 0;
    while cursor < bytes.len() {
        let Some(offset) = memchr::memchr2(b'\\', quote, &bytes[cursor..]) else {
            return Err("unterminated Lua string".to_string());
        };
        cursor += offset;
        if bytes[cursor] != b'\\' {
            return Ok(cursor + 1);
        }
        cursor = (cursor + 2).min(bytes.len());
        dense_escapes = if offset < 16 { dense_escapes + 1 } else { 0 };
        // Repeated nearby escapes favor the original scalar loop.
        if dense_escapes == 8 {
            break;
        }
    }
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

fn lua_long_string_end(source: &str, index: usize, open_end: usize) -> Result<usize, String> {
    let equals_len = open_end - index - 2;
    let bytes = source.as_bytes();
    let mut cursor = open_end;
    let mut misses = 0;
    while let Some(offset) = memchr::memchr(b']', &bytes[cursor..]) {
        let close = cursor + offset;
        let mut end = close + 1;
        while bytes.get(end) == Some(&b'=') {
            end += 1;
        }
        if end - close - 1 == equals_len && bytes.get(end) == Some(&b']') {
            return Ok(end + 1);
        }
        // Reconsider the closing bracket after a wrong-length equals run.
        // Each run is scanned once, including adversarial near matches.
        cursor = end;
        misses += 1;
        if misses == 8 && equals_len <= 14 {
            // Dense near matches benefit from memmem's substring search.
            // Ordinary strings never construct a terminator, even on the stack.
            let mut terminator = [b'='; 16];
            terminator[0] = b']';
            terminator[equals_len + 1] = b']';
            return memchr::memmem::find(&bytes[cursor..], &terminator[..equals_len + 2])
                .map(|offset| cursor + offset + equals_len + 2)
                .ok_or_else(|| "unterminated Lua long string".to_string());
        }
    }
    Err("unterminated Lua long string".to_string())
}

#[cfg(test)]
mod perf {
    include!("../tests/perf/cmd.rs");
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
    fn preprocess_lua_cmd_accepts_native_adjacent_commands() {
        let source = r#"return cmd(sleep,0
pulse; effectmagnitude,1,1,1;
value, (2 + 3) * 4 nextvalue, false or 7;
value, function() return {3, 4} end nextvalue, ({9, 10})[2];
value, "literal" nextvalue, [=[long literal]=];
value, math.max(2, 3) nextvalue, .5e+1;
value, (function(x) return x end){11} nextvalue, -2^3;
)"#;
        let expanded = preprocess_lua_cmd_syntax(source).unwrap();
        assert_eq!(expanded.matches('\n').count(), source.matches('\n').count());
        let lua = mlua::Lua::new();
        let command = lua
            .load(expanded)
            .eval::<mlua::Function>()
            .expect("native cmd argument expressions end before the next command name");
        let actor = lua
            .load(
                r#"
            local a = {calls={}}
            for _, name in ipairs({'sleep','pulse','effectmagnitude','value','nextvalue'}) do
                a[name] = function(self, ...) self.calls[#self.calls+1] = {name, ...} end
            end
            return a
        "#,
            )
            .eval::<mlua::Table>()
            .unwrap();
        command.call::<()>(actor.clone()).unwrap();
        let calls: mlua::Table = actor.get("calls").unwrap();
        assert_eq!(calls.raw_len(), 13);
        assert_eq!(
            calls.get::<mlua::Table>(1).unwrap().get::<i32>(2).unwrap(),
            0
        );
        assert_eq!(calls.get::<mlua::Table>(2).unwrap().raw_len(), 1);
        assert_eq!(
            calls.get::<mlua::Table>(4).unwrap().get::<i32>(2).unwrap(),
            20
        );
        assert_eq!(
            calls.get::<mlua::Table>(5).unwrap().get::<i32>(2).unwrap(),
            7
        );
        assert_eq!(
            calls.get::<mlua::Table>(7).unwrap().get::<i32>(2).unwrap(),
            10
        );
        assert_eq!(
            calls.get::<mlua::Table>(11).unwrap().get::<i32>(2).unwrap(),
            5
        );
        assert_eq!(
            calls.get::<mlua::Table>(13).unwrap().get::<i32>(2).unwrap(),
            -8
        );
    }

    #[test]
    fn preprocess_lua_cmd_accepts_comments_between_commands_and_arguments() {
        let source = r#"return cmd(
            -- Disabled: x, 99; )
            x, --[=[ ignored ; ) ]=] 12;
            -- another command
            settext, "--literal; )";
            settext, [=[--long literal]=];
            y, 34 -- trailing comment
        )"#;
        let lua = mlua::Lua::new();
        let command = lua
            .load(preprocess_lua_cmd_syntax(source).unwrap())
            .eval::<mlua::Function>()
            .unwrap();
        let actor = lua
            .load(
                r#"return {
            x = function(self, value) self.a = value end,
            y = function(self, value) self.b = value end,
            settext = function(self, value) self.label = (self.label or '') .. value end,
        }"#,
            )
            .eval::<mlua::Table>()
            .unwrap();
        command.call::<mlua::Value>(actor.clone()).unwrap();
        assert_eq!(actor.get::<i32>("a").unwrap(), 12);
        assert_eq!(actor.get::<i32>("b").unwrap(), 34);
        assert_eq!(
            actor.get::<String>("label").unwrap(),
            "--literal; )--long literal"
        );
    }

    #[test]
    fn preprocess_lua_cmd_preserves_function_arguments() {
        let source = r#"return cmd(SetUpdateFunction, function(self)
            local count = 0;
            for i = 1, 3 do
                if i == 1 then count = count + i;
                elseif i == 2 then do count = count + i; end
                else count = count + i; end
            end
            while count < 7 do count = count + 1; end
            repeat count = count + 1; until count == 8
            local nested = function() return "function; do; end", count; end;
            local label, result = nested(); self.label = label; self.count = result;
        end; zoom, 2)"#;
        let lua = mlua::Lua::new();
        let command = lua
            .load(preprocess_lua_cmd_syntax(source).unwrap())
            .eval::<mlua::Function>()
            .unwrap();
        let actor = lua
            .load(
                r#"return {
                SetUpdateFunction = function(self, callback) self.callback = callback end,
                zoom = function(self, value) self.scale = value end,
            }"#,
            )
            .eval::<mlua::Table>()
            .unwrap();
        command.call::<mlua::Value>(actor.clone()).unwrap();
        actor
            .get::<mlua::Function>("callback")
            .unwrap()
            .call::<()>(actor.clone())
            .unwrap();
        assert_eq!(actor.get::<i32>("count").unwrap(), 8);
        assert_eq!(actor.get::<i32>("scale").unwrap(), 2);
        assert_eq!(actor.get::<String>("label").unwrap(), "function; do; end");
    }

    #[test]
    fn preprocess_lua_cmd_ignores_strings_and_comments() {
        let source = "local s = \"cmd(zoom, 2)\" -- cmd(diffuse, 1)\nreturn cmd(linear, 1)";

        assert_eq!(
            preprocess_lua_cmd_syntax(source).unwrap(),
            "local s = \"cmd(zoom, 2)\" -- cmd(diffuse, 1)\nreturn function(self) self:linear(1); return self end"
        );
    }

    #[test]
    fn preprocess_lua_cmd_appends_multiple_expansions_and_preserves_surroundings() {
        let source = "local a = cmd(); local b = cmd(zoom, f(1, 2); diffuse, {1, 0, 0, 1}); return cmd(settext, [=[cmd(x); y]=]) -- cmd(noop)";
        assert_eq!(
            preprocess_lua_cmd_syntax(source).unwrap(),
            "local a = function(self) return self end; local b = function(self) self:zoom(f(1, 2)); self:diffuse({1, 0, 0, 1}); return self end; return function(self) self:settext([=[cmd(x); y]=]); return self end -- cmd(noop)"
        );
    }

    #[test]
    fn preprocess_lua_cmd_keeps_errors_after_an_earlier_expansion() {
        for (command, expected) in [
            ("cmd(zoom, 1", "unterminated cmd expression"),
            ("cmd(1zoom, 1)", "invalid cmd command '1zoom, 1'"),
            ("cmd(zoom 1)", "invalid cmd command 'zoom 1'"),
            ("cmd(settext, 'abc)", "unterminated Lua string"),
            ("cmd(settext, [[abc)", "unterminated Lua long string"),
        ] {
            let source = format!("local a = cmd(zoom, 2); return {command}");
            assert_eq!(preprocess_lua_cmd_syntax(&source).unwrap_err(), expected);
        }
    }

    #[test]
    fn preprocess_lua_cmd_reuses_openings_across_whitespace_and_long_strings() {
        let source = "local s = [==[cmd(noop); )]==]; --[=[cmd(noop)]=]\nreturn cmd \t\r\n (settext, [==[x; cmd(noop); )]==]; zoom, 2)";
        assert_eq!(
            preprocess_lua_cmd_syntax(source).unwrap(),
            "local s = [==[cmd(noop); )]==]; --[=[cmd(noop)]=]\nreturn \nfunction(self) self:settext([==[x; cmd(noop); )]==]); self:zoom(2); return self end"
        );
        for source in ["cmd", "cmd  \t", "cmdx(zoom, 1)", "xcmd(zoom, 1)"] {
            assert_eq!(preprocess_lua_cmd_syntax(source).unwrap(), source);
        }
    }

    #[test]
    fn preprocess_lua_cmd_preserves_native_function_source_lines() {
        let source = "local first = cmd\n(\nzoom, 2;\nlinear, 1\n)\nlocal second = function() return 7 end\nreturn {first, second}\n";
        let expanded = preprocess_lua_cmd_syntax(source).unwrap();
        assert_eq!(expanded.matches('\n').count(), source.matches('\n').count());
        let lua = mlua::Lua::new();
        let functions = lua
            .load(expanded)
            .set_name("@cmd-source-lines.lua")
            .eval::<mlua::Table>()
            .unwrap();
        let first = functions.get::<mlua::Function>(1).unwrap().info();
        assert_eq!(first.line_defined, Some(2));
        assert_eq!(first.last_line_defined, Some(5));
        let second = functions.get::<mlua::Function>(2).unwrap().info();
        assert_eq!(second.line_defined, Some(6));
        assert_eq!(second.last_line_defined, Some(6));
    }

    #[test]
    fn preprocess_lua_cmd_preserves_invalid_comment_openings() {
        for comment in [
            "--[not a long string",
            "--[=not a long string",
            "--[=[unclosed",
        ] {
            let source = format!("{comment}\nreturn cmd(zoom, 2)");
            assert_eq!(
                preprocess_lua_cmd_syntax(&source).unwrap(),
                format!("{comment}\nreturn function(self) self:zoom(2); return self end")
            );
        }
        assert_eq!(
            preprocess_lua_cmd_syntax("return [=[unclosed").unwrap_err(),
            "unterminated Lua long string"
        );
    }
}
