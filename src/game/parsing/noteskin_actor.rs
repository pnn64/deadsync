use super::noteskin_itg;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const ITG_ARG0_TOKEN: &str = "__ITG_ARG0__";

#[derive(Debug, Default, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ItgLuaSpriteDecl {
    pub texture_expr: String,
    pub frame0: usize,
    pub frame_count: usize,
    pub frame_delays: Option<Vec<f32>>,
    pub commands: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ItgLuaModelDecl {
    pub meshes_expr: Option<String>,
    pub materials_expr: Option<String>,
    pub texture_expr: Option<String>,
    pub frame0: usize,
    pub commands: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ItgLuaRefDecl {
    pub button_override: Option<String>,
    pub element: String,
    pub wrapper_expr: Option<String>,
    pub frame_override: Option<usize>,
    pub commands: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ItgLuaPathRefDecl {
    pub path_expr: String,
    pub arg_expr: Option<String>,
    pub frame_override: Option<usize>,
    pub commands: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ItgLuaActorDecl {
    pub sprites: Vec<ItgLuaSpriteDecl>,
    pub models: Vec<ItgLuaModelDecl>,
    pub refs: Vec<ItgLuaRefDecl>,
    pub path_refs: Vec<ItgLuaPathRefDecl>,
}

pub fn parse_actor_decl(content: &str, metrics: &noteskin_itg::IniData) -> ItgLuaActorDecl {
    let mut decl = ItgLuaActorDecl::default();
    let arg0_aliases = parse_arg0_aliases(content);

    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("Def.Sprite") {
        let start = cursor + rel;
        let Some(open_rel) = content[start..].find('{') else {
            break;
        };
        let open = start + open_rel;
        let Some(close) = find_matching(content, open, '{', '}') else {
            break;
        };
        if let Some(sprite) = parse_sprite_block(&content[open + 1..close], metrics) {
            decl.sprites.push(sprite);
        }
        cursor = close + 1;
    }

    cursor = 0usize;
    while let Some(rel) = content[cursor..].find("Def.Model") {
        let start = cursor + rel;
        let Some(open_rel) = content[start..].find('{') else {
            break;
        };
        let open = start + open_rel;
        let Some(close) = find_matching(content, open, '{', '}') else {
            break;
        };
        if let Some(model) = parse_model_block(&content[open + 1..close], metrics) {
            decl.models.push(model);
        }
        cursor = close + 1;
    }

    cursor = 0usize;
    while let Some(rel) = content[cursor..].find("LoadActor(") {
        let call_start = cursor + rel;
        if content
            .as_bytes()
            .get(call_start.saturating_sub(1))
            .is_some_and(|b| *b == b':')
        {
            cursor = call_start + 1;
            continue;
        }
        let open = call_start + "LoadActor".len();
        let Some(close) = find_matching(content, open, '(', ')') else {
            break;
        };
        let args = split_call_args(&content[open + 1..close]);
        let (commands, next_cursor) = find_post_call_commands(content, close, metrics);
        let frame_override = find_post_call_frame_override(content, close);
        if !args.is_empty() {
            decl.path_refs.push(ItgLuaPathRefDecl {
                path_expr: rewrite_arg0_expr(&args[0], &arg0_aliases),
                arg_expr: args.get(1).map(|arg| rewrite_arg0_expr(arg, &arg0_aliases)),
                frame_override,
                commands,
            });
        }
        cursor = next_cursor;
    }

    cursor = 0usize;
    while let Some(rel) = content[cursor..].find("NOTESKIN:LoadActor(") {
        let call_start = cursor + rel;
        let open = call_start + "NOTESKIN:LoadActor".len();
        let Some(close) = find_matching(content, open, '(', ')') else {
            break;
        };
        let Some((button_override, element)) = parse_loadactor_args(&content[open + 1..close])
        else {
            cursor = close + 1;
            continue;
        };
        let mut wrapper_expr = None;
        let (mut commands, mut next_cursor) = find_post_call_commands(content, close, metrics);
        let mut frame_override = find_post_call_frame_override(content, close);
        if commands.is_empty()
            && let Some((outer_args, outer_close)) =
                find_enclosing_loadactor_for_noteskin(content, call_start, close)
            && outer_args.len() >= 2
        {
            wrapper_expr = Some(outer_args[0].clone());
            let (outer_commands, outer_next_cursor) =
                find_post_call_commands(content, outer_close, metrics);
            let outer_frame_override = find_post_call_frame_override(content, outer_close);
            if !outer_commands.is_empty() {
                commands = outer_commands;
                next_cursor = outer_next_cursor;
                frame_override = outer_frame_override;
            } else {
                next_cursor = outer_close + 1;
            }
        }
        decl.refs.push(ItgLuaRefDecl {
            button_override,
            element,
            wrapper_expr,
            frame_override,
            commands,
        });
        cursor = next_cursor;
    }

    decl
}

fn parse_arg0_aliases(content: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for raw in content.lines() {
        let line = raw.trim().trim_end_matches(';').trim();
        if !line.starts_with("local ") {
            continue;
        }
        let rest = line[6..].trim();
        let Some((lhs, rhs)) = rest.split_once('=') else {
            continue;
        };
        if rhs.trim() != "..." {
            continue;
        }
        let name = lhs.trim();
        if name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            out.insert(name.to_string());
        }
    }
    out
}

fn rewrite_arg0_expr(expr: &str, arg0_aliases: &HashSet<String>) -> String {
    let trimmed = expr.trim();
    if trimmed == "..." || arg0_aliases.contains(trimmed) {
        ITG_ARG0_TOKEN.to_string()
    } else {
        trimmed.to_string()
    }
}

fn split_call_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = 0u8;
    let bytes = raw.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let b = bytes[idx];
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            idx += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => quote = b,
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                let part = raw[start..idx].trim();
                if !part.is_empty() {
                    out.push(part.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn find_post_call_commands(
    content: &str,
    call_close: usize,
    metrics: &noteskin_itg::IniData,
) -> (HashMap<String, String>, usize) {
    let mut after = skip_ws(content, call_close + 1);
    if !content
        .get(after..)
        .is_some_and(|tail| tail.starts_with(".."))
    {
        return (HashMap::new(), call_close + 1);
    }
    after += 2;
    after = skip_ws(content, after);
    if !content.as_bytes().get(after).is_some_and(|ch| *ch == b'{') {
        return (HashMap::new(), call_close + 1);
    }
    let Some(end) = find_matching(content, after, '{', '}') else {
        return (HashMap::new(), call_close + 1);
    };
    (
        parse_commands_block(&content[after + 1..end], metrics),
        end + 1,
    )
}

fn find_post_call_frame_override(content: &str, call_close: usize) -> Option<usize> {
    let mut after = skip_ws(content, call_close + 1);
    if !content
        .get(after..)
        .is_some_and(|tail| tail.starts_with(".."))
    {
        return None;
    }
    after += 2;
    after = skip_ws(content, after);
    if !content.as_bytes().get(after).is_some_and(|ch| *ch == b'{') {
        return None;
    }
    let end = find_matching(content, after, '{', '}')?;
    parse_frame_override_block(&content[after + 1..end])
}

fn parse_frame_override_block(block: &str) -> Option<usize> {
    let marker_idx = block.find("Frames")?;
    let tail = &block[marker_idx + "Frames".len()..];
    let eq_idx = tail.find('=')?;
    let after_eq = marker_idx + "Frames".len() + eq_idx + 1;
    let bytes = block.as_bytes();
    let mut open = after_eq;
    while open < bytes.len() && bytes[open].is_ascii_whitespace() {
        open += 1;
    }
    if bytes.get(open).is_none_or(|b| *b != b'{') {
        return None;
    }
    let close = find_matching(block, open, '{', '}')?;
    let frames = &block[open + 1..close];
    let frame_key_idx = frames.find("Frame")?;
    let frame_tail = &frames[frame_key_idx + "Frame".len()..];
    let frame_eq = frame_tail.find('=')?;
    let digits: String = frame_tail[frame_eq + 1..]
        .trim()
        .chars()
        .skip_while(|ch| ch.is_ascii_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok()
}

fn find_enclosing_loadactor_for_noteskin(
    content: &str,
    call_start: usize,
    call_close: usize,
) -> Option<(Vec<String>, usize)> {
    let mut search_end = call_start;
    while let Some(pos) = content[..search_end].rfind("LoadActor(") {
        if content
            .as_bytes()
            .get(pos.saturating_sub(1))
            .is_some_and(|b| *b == b':')
        {
            search_end = pos;
            continue;
        }
        let open = pos + "LoadActor".len();
        let Some(outer_close) = find_matching(content, open, '(', ')') else {
            search_end = pos;
            continue;
        };
        if pos < call_start && outer_close >= call_close {
            return Some((
                split_call_args(&content[open + 1..outer_close]),
                outer_close,
            ));
        }
        search_end = pos;
    }
    None
}

fn parse_sprite_block(block: &str, metrics: &noteskin_itg::IniData) -> Option<ItgLuaSpriteDecl> {
    let mut texture_expr = None;
    let mut frame0 = 0usize;
    let mut frame_count = 1usize;
    let mut frame_max = 0usize;
    let mut frame_seen = false;
    let mut frame_delays = HashMap::<usize, f32>::new();
    let mut commands = HashMap::new();
    for raw in block.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if key.eq_ignore_ascii_case("Texture") {
            texture_expr = Some(value.to_string());
            continue;
        }
        if key.eq_ignore_ascii_case("Frames")
            && let Some((linear_count, linear_delays)) = parse_linear_frames_expr(value)
        {
            frame_count = linear_count.max(1);
            frame_delays = linear_delays
                .into_iter()
                .enumerate()
                .map(|(idx, delay)| (idx, delay))
                .collect();
            continue;
        }
        let key_lower = key.to_ascii_lowercase();
        if key_lower.starts_with("frame") && key_lower[5..].chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(parsed) = value.parse::<usize>() {
                frame_seen = true;
                frame_max = frame_max.max(parsed);
                if key_lower == "frame0000" {
                    frame0 = parsed;
                }
            }
            continue;
        }
        if key_lower.starts_with("delay") && key_lower[5..].chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(idx) = key_lower[5..].parse::<usize>()
                && let Some(delay) = parse_lua_float_token(value)
            {
                frame_delays.insert(idx, delay.max(0.0));
            }
            continue;
        }
        if key_lower.ends_with("command")
            && let Some(cmd) = resolve_command_expr(value, metrics)
        {
            commands.insert(key_lower, cmd);
        }
    }
    for (k, v) in parse_function_commands(block) {
        commands.insert(k, v);
    }
    if frame_seen {
        frame_count = frame_max.saturating_add(1).max(1);
    }
    let frame_delays = if frame_delays.is_empty() {
        None
    } else {
        let mut delays = vec![frame_delays.get(&0).copied().unwrap_or(1.0); frame_count];
        for (idx, delay) in frame_delays {
            if idx < delays.len() {
                delays[idx] = delay.max(0.0);
            }
        }
        Some(delays)
    };
    Some(ItgLuaSpriteDecl {
        texture_expr: texture_expr?,
        frame0,
        frame_count,
        frame_delays,
        commands,
    })
}

fn strip_wrapped_parens(raw: &str) -> &str {
    let mut value = raw.trim();
    loop {
        if !(value.starts_with('(') && value.ends_with(')')) {
            break;
        }
        let Some(close) = find_matching(value, 0, '(', ')') else {
            break;
        };
        if close + 1 != value.len() {
            break;
        }
        value = value[1..value.len() - 1].trim();
    }
    value
}

fn parse_lua_float_expr(raw: &str) -> Option<f32> {
    let value = strip_wrapped_parens(raw.trim().trim_end_matches(';'));
    if let Some(v) = parse_lua_float_token(value) {
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
                let denom = parse_lua_float_expr(rhs)?;
                if denom.abs() <= f32::EPSILON {
                    return None;
                }
                return Some(parse_lua_float_expr(lhs)? / denom);
            }
            _ => {}
        }
    }
    None
}

fn parse_linear_frames_expr(raw: &str) -> Option<(usize, Vec<f32>)> {
    let value = raw.trim().trim_end_matches(';').trim();
    let open = value.find('(')?;
    if !value[..open]
        .trim()
        .eq_ignore_ascii_case("Sprite.LinearFrames")
    {
        return None;
    }
    let close = find_matching(value, open, '(', ')')?;
    let args = split_call_args(&value[open + 1..close]);
    if args.len() < 2 {
        return None;
    }
    let frame_count = args[0]
        .trim()
        .parse::<usize>()
        .ok()
        .or_else(|| parse_lua_float_expr(&args[0]).map(|v| v as usize))?
        .max(1);
    let seconds = parse_lua_float_expr(&args[1])?;
    let delay = (seconds / frame_count as f32).max(0.0);
    Some((frame_count, vec![delay; frame_count]))
}

fn parse_model_block(block: &str, metrics: &noteskin_itg::IniData) -> Option<ItgLuaModelDecl> {
    let mut meshes_expr = None;
    let mut materials_expr = None;
    let mut texture_expr = None;
    let mut frame0 = 0usize;
    let mut commands = HashMap::new();
    for raw in block.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if key.eq_ignore_ascii_case("Meshes") {
            meshes_expr = Some(value.to_string());
            continue;
        }
        if key.eq_ignore_ascii_case("Materials") {
            materials_expr = Some(value.to_string());
            continue;
        }
        if key.eq_ignore_ascii_case("Texture") {
            texture_expr = Some(value.to_string());
            continue;
        }
        let key_lower = key.to_ascii_lowercase();
        if key_lower.starts_with("frame")
            && key_lower[5..].chars().all(|ch| ch.is_ascii_digit())
            && let Ok(parsed) = value.parse::<usize>()
            && key_lower == "frame0000"
        {
            frame0 = parsed;
            continue;
        }
        if key_lower.ends_with("command")
            && let Some(cmd) = resolve_command_expr(value, metrics)
        {
            commands.insert(key_lower, cmd);
        }
    }
    for (k, v) in parse_function_commands(block) {
        commands.insert(k, v);
    }
    if meshes_expr.is_none() && materials_expr.is_none() && texture_expr.is_none() {
        return None;
    }
    Some(ItgLuaModelDecl {
        meshes_expr,
        materials_expr,
        texture_expr,
        frame0,
        commands,
    })
}

fn parse_loadactor_args(args: &str) -> Option<(Option<String>, String)> {
    let quoted = extract_quoted_strings(args);
    let element = quoted.last()?.to_string();
    let button_override = if args.contains("Var \"Button\"") || args.contains("Var 'Button'") {
        None
    } else if quoted.len() >= 2 {
        Some(quoted[0].clone())
    } else {
        None
    };
    Some((button_override, element))
}

fn parse_commands_block(block: &str, metrics: &noteskin_itg::IniData) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    for raw in block.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        if !key.ends_with("command") {
            continue;
        }
        if let Some(cmd) = resolve_command_expr(v.trim(), metrics) {
            commands.insert(key, cmd);
        }
    }
    for (k, v) in parse_function_commands(block) {
        commands.insert(k, v);
    }
    commands
}

fn parse_function_commands(block: &str) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    let bytes = block.as_bytes();
    let mut cursor = 0usize;
    while let Some(eq_rel) = block[cursor..].find('=') {
        let eq = cursor + eq_rel;
        let key_start = block[..eq]
            .rfind(['\n', '\r', '{', ';', ','])
            .map_or(0, |idx| idx + 1);
        let key_lower = block[key_start..eq].trim().to_ascii_lowercase();
        if !key_lower.ends_with("command") {
            cursor = eq + 1;
            continue;
        }
        let mut rhs = skip_ws(block, eq + 1);
        if !block.get(rhs..).is_some_and(|s| s.starts_with("function")) {
            cursor = eq + 1;
            continue;
        }
        rhs += "function".len();
        let Some(param_open_rel) = block[rhs..].find('(') else {
            cursor = eq + 1;
            continue;
        };
        let param_open = rhs + param_open_rel;
        let Some(param_close) = find_matching(block, param_open, '(', ')') else {
            cursor = eq + 1;
            continue;
        };
        let body_start = param_close + 1;
        let Some(end_idx) = find_function_end(block, body_start) else {
            cursor = eq + 1;
            continue;
        };
        if let Some(cmd) = parse_self_chain_commands(&block[body_start..end_idx]) {
            commands.insert(key_lower, cmd);
        }
        cursor = end_idx + 3;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
    }
    commands
}

fn find_function_end(content: &str, mut cursor: usize) -> Option<usize> {
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
            && token_boundary(bytes, cursor, "function".len())
        {
            depth += 1;
            cursor += "function".len();
            continue;
        }
        if content[cursor..].starts_with("end") && token_boundary(bytes, cursor, "end".len()) {
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

fn token_boundary(bytes: &[u8], start: usize, len: usize) -> bool {
    let prev_ok = start == 0 || !is_lua_ident(bytes[start - 1]);
    let end = start + len;
    let next_ok = end >= bytes.len() || !is_lua_ident(bytes[end]);
    prev_ok && next_ok
}

fn is_lua_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn parse_self_chain_commands(body: &str) -> Option<String> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find("self:") {
        let name_start = cursor + rel + 5;
        let mut name_end = name_start;
        while name_end < bytes.len() && is_lua_ident(bytes[name_end]) {
            name_end += 1;
        }
        if name_end == name_start {
            cursor = name_start;
            continue;
        }
        let name = body[name_start..name_end].trim();
        let mut open = skip_ws(body, name_end);
        if bytes.get(open).is_none_or(|b| *b != b'(') {
            cursor = name_end;
            continue;
        }
        let Some(close) = find_matching(body, open, '(', ')') else {
            cursor = name_end;
            continue;
        };
        let args = body[open + 1..close].trim();
        out.push(if args.is_empty() {
            name.to_string()
        } else {
            format!("{name},{args}")
        });
        open = close + 1;
        cursor = open;
    }
    (!out.is_empty()).then(|| out.join(";"))
}

fn resolve_command_expr(raw: &str, metrics: &noteskin_itg::IniData) -> Option<String> {
    let value = raw
        .trim()
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim();
    if value.starts_with("NOTESKIN:GetMetricA(") {
        let args = extract_quoted_strings(value);
        if args.len() >= 2 {
            return metrics.get(&args[0], &args[1]).map(str::to_string);
        }
    }
    if value.starts_with("cmd(") && value.ends_with(')') {
        return Some(value[4..value.len() - 1].trim().to_string());
    }
    if let Some(q) = parse_lua_quoted(value) {
        return Some(q);
    }
    Some(value.to_string())
}

fn extract_quoted_strings(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let quote = bytes[idx];
        if quote != b'"' && quote != b'\'' {
            idx += 1;
            continue;
        }
        idx += 1;
        let start = idx;
        while idx < bytes.len() && bytes[idx] != quote {
            idx += 1;
        }
        if idx <= bytes.len() {
            out.push(input[start..idx].to_string());
        }
        idx += 1;
    }
    out
}

fn parse_lua_quoted(raw: &str) -> Option<String> {
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
    ((quote == b'"' || quote == b'\'') && bytes[trimmed.len() - 1] == quote)
        .then(|| trimmed[1..trimmed.len() - 1].to_string())
}

fn find_matching(content: &str, open_idx: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in content
        .char_indices()
        .skip_while(|(idx, _)| *idx < open_idx)
    {
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

fn skip_ws(content: &str, mut idx: usize) -> usize {
    let bytes = content.as_bytes();
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    idx
}

fn parse_lua_float_token(raw: &str) -> Option<f32> {
    let value = raw.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    if let Ok(v) = value.parse::<f32>() {
        return Some(v);
    }
    (value.contains(',') && !value.contains('.'))
        .then(|| value.replace(',', "."))
        .and_then(|patched| patched.parse::<f32>().ok())
}
