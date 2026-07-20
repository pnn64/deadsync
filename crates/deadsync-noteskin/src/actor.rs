use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::itg as noteskin_itg;
use crate::script::parse_linear_frames_expr;

pub const ITG_ARG0_TOKEN: &str = "__ITG_ARG0__";

#[derive(Debug, Default)]
struct CommandContext {
    colors: HashMap<String, String>,
    functions: HashMap<String, LocalFunction>,
}

#[derive(Debug)]
struct LocalFunction {
    params: Vec<String>,
    body: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ItgLuaSpriteDecl {
    pub texture_expr: String,
    pub frame0: usize,
    pub frame_count: usize,
    pub frame_indices: Option<Vec<usize>>,
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
    let command_context = command_context(content);

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
        if let Some(sprite) =
            parse_sprite_block(&content[open + 1..close], metrics, &command_context)
        {
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
        if let Some(model) = parse_model_block(&content[open + 1..close], metrics, &command_context)
        {
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
        let (commands, next_cursor) =
            find_post_call_commands(content, close, metrics, &command_context);
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
        let (mut commands, mut next_cursor) =
            find_post_call_commands(content, close, metrics, &command_context);
        let mut frame_override = find_post_call_frame_override(content, close);
        if commands.is_empty()
            && let Some((outer_args, outer_close)) =
                find_enclosing_loadactor_for_noteskin(content, call_start, close)
            && outer_args.len() >= 2
        {
            wrapper_expr = Some(outer_args[0].clone());
            let (outer_commands, outer_next_cursor) =
                find_post_call_commands(content, outer_close, metrics, &command_context);
            let outer_frame_override = find_post_call_frame_override(content, outer_close);
            if outer_commands.is_empty() {
                next_cursor = outer_close + 1;
            } else {
                commands = outer_commands;
                next_cursor = outer_next_cursor;
                frame_override = outer_frame_override;
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

pub fn parse_wrapper_commands(
    content: &str,
    metrics: &noteskin_itg::IniData,
) -> Option<HashMap<String, String>> {
    let marker = ".. {";
    let marker_idx = content.find(marker)?;
    let open = marker_idx + marker.len() - 1;
    let close = find_matching(content, open, '{', '}')?;
    Some(parse_commands_block(
        &content[open + 1..close],
        metrics,
        &CommandContext::default(),
    ))
}

pub fn parse_wrapper_commands_from_file(
    path: &Path,
    metrics: &noteskin_itg::IniData,
) -> Option<HashMap<String, String>> {
    if !is_lua_path(path) {
        return None;
    }
    parse_wrapper_commands(&fs::read_to_string(path).ok()?, metrics)
}

pub fn is_lua_path(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
}

pub fn element_contains_hint(element: &str, hint: &str) -> bool {
    let hint = hint.as_bytes();
    if hint.is_empty() {
        return true;
    }
    element
        .as_bytes()
        .windows(hint.len())
        .any(|candidate| candidate.eq_ignore_ascii_case(hint))
}

#[cfg(any(test, feature = "bench-support"))]
pub fn element_contains_hint_legacy_for_bench(element: &str, hint: &str) -> bool {
    element
        .to_ascii_lowercase()
        .contains(&hint.to_ascii_lowercase())
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

fn command_context(content: &str) -> CommandContext {
    let mut context = CommandContext::default();
    for raw in content.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        if let Some((name, value)) = parse_local_assignment(line)
            && let Some(color) = parse_lua_color_expr(value)
        {
            context.colors.insert(name.to_ascii_lowercase(), color);
        }
    }
    context.functions = parse_local_functions(content);
    context
}

fn parse_local_assignment(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("local ")?;
    let (lhs, rhs) = rest.split_once('=')?;
    let name = lhs.trim();
    if name.is_empty() || !name.as_bytes().iter().all(|b| is_lua_ident(*b)) {
        return None;
    }
    Some((
        name,
        rhs.trim()
            .trim_end_matches(',')
            .trim_end_matches(';')
            .trim(),
    ))
}

fn parse_lua_color_expr(raw: &str) -> Option<String> {
    let value = raw.trim();
    let lower = value.to_ascii_lowercase();
    let inner = if lower.starts_with("color(") && value.ends_with(')') {
        value[6..value.len().saturating_sub(1)]
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
    } else {
        value.trim_matches('"').trim_matches('\'')
    };
    parse_hex_color(inner)
        .or_else(|| parse_color_list(inner))
        .map(format_color)
}

fn parse_hex_color(raw: &str) -> Option<[f32; 4]> {
    let hex = raw.trim().strip_prefix('#')?;
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let byte = |idx: usize| u8::from_str_radix(&hex[idx..idx + 2], 16).ok();
    Some([
        byte(0)? as f32 / 255.0,
        byte(2)? as f32 / 255.0,
        byte(4)? as f32 / 255.0,
        if hex.len() == 8 {
            byte(6)? as f32 / 255.0
        } else {
            1.0
        },
    ])
}

fn parse_color_list(raw: &str) -> Option<[f32; 4]> {
    let mut values = raw.split(',').filter_map(parse_lua_float_expr);
    Some([
        values.next()?,
        values.next()?,
        values.next()?,
        values.next()?,
    ])
}

fn format_color(color: [f32; 4]) -> String {
    format!("{},{},{},{}", color[0], color[1], color[2], color[3])
}

fn parse_local_functions(content: &str) -> HashMap<String, LocalFunction> {
    let mut functions = HashMap::new();
    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("local function ") {
        let start = cursor + rel;
        if start > 0 && is_lua_ident(content.as_bytes()[start - 1]) {
            cursor = start + 1;
            continue;
        }
        let name_start = start + "local function ".len();
        let mut name_end = name_start;
        while content
            .as_bytes()
            .get(name_end)
            .is_some_and(|b| is_lua_ident(*b))
        {
            name_end += 1;
        }
        let name = content[name_start..name_end].trim();
        if name.is_empty() {
            cursor = name_end;
            continue;
        }
        let open = skip_ws(content, name_end);
        if content.as_bytes().get(open).is_none_or(|b| *b != b'(') {
            cursor = name_end;
            continue;
        }
        let Some(close) = find_matching(content, open, '(', ')') else {
            cursor = name_end;
            continue;
        };
        let Some(end_idx) = find_function_end(content, close + 1) else {
            cursor = close + 1;
            continue;
        };
        let params = split_call_args(&content[open + 1..close])
            .into_iter()
            .filter(|param| param.as_bytes().iter().all(|b| is_lua_ident(*b)))
            .collect();
        functions.insert(
            name.to_ascii_lowercase(),
            LocalFunction {
                params,
                body: content[close + 1..end_idx].to_string(),
            },
        );
        cursor = end_idx + "end".len();
    }
    functions
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
    command_context: &CommandContext,
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
    if content.as_bytes().get(after).is_none_or(|ch| *ch != b'{') {
        return (HashMap::new(), call_close + 1);
    }
    let Some(end) = find_matching(content, after, '{', '}') else {
        return (HashMap::new(), call_close + 1);
    };
    (
        parse_commands_block(&content[after + 1..end], metrics, command_context),
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
    if content.as_bytes().get(after).is_none_or(|ch| *ch != b'{') {
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
        .skip_while(char::is_ascii_whitespace)
        .take_while(char::is_ascii_digit)
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

fn parse_sprite_block(
    block: &str,
    metrics: &noteskin_itg::IniData,
    command_context: &CommandContext,
) -> Option<ItgLuaSpriteDecl> {
    let mut texture_expr = None;
    let mut frame0 = 0usize;
    let mut frame_count = 1usize;
    let mut frame_state_max = 0usize;
    let mut frame_seen = false;
    let mut frame_indices = HashMap::<usize, usize>::new();
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
            frame_delays = linear_delays.into_iter().enumerate().collect();
            continue;
        }
        let key_lower = key.to_ascii_lowercase();
        if key_lower.starts_with("frame") && key_lower[5..].chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(idx) = key_lower[5..].parse::<usize>()
                && let Ok(parsed) = value.parse::<usize>()
            {
                frame_seen = true;
                frame_state_max = frame_state_max.max(idx);
                if idx == 0 {
                    frame0 = parsed;
                }
                frame_indices.insert(idx, parsed);
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
            && let Some(cmd) = resolve_command_expr(value, metrics, command_context)
        {
            commands.insert(key_lower, cmd);
        }
    }
    for (k, v) in parse_function_commands(block, command_context) {
        commands.insert(k, v);
    }
    if frame_seen {
        frame_count = frame_state_max.saturating_add(1).max(1);
    }
    let frame_indices = if frame_indices.is_empty() {
        None
    } else {
        let mut indices = vec![0usize; frame_count];
        let mut last_frame = 0usize;
        for (idx, out) in indices.iter_mut().enumerate() {
            if let Some(frame) = frame_indices.get(&idx).copied() {
                last_frame = frame;
            }
            *out = last_frame;
        }
        Some(indices)
    };
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
        frame_indices,
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

fn parse_model_block(
    block: &str,
    metrics: &noteskin_itg::IniData,
    command_context: &CommandContext,
) -> Option<ItgLuaModelDecl> {
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
            && let Some(cmd) = resolve_command_expr(value, metrics, command_context)
        {
            commands.insert(key_lower, cmd);
        }
    }
    for (k, v) in parse_function_commands(block, command_context) {
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

fn parse_commands_block(
    block: &str,
    metrics: &noteskin_itg::IniData,
    command_context: &CommandContext,
) -> HashMap<String, String> {
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
        if let Some(cmd) = resolve_command_expr(v.trim(), metrics, command_context) {
            commands.insert(key, cmd);
        }
    }
    for (k, v) in parse_function_commands(block, command_context) {
        commands.insert(k, v);
    }
    commands
}

fn parse_function_commands(
    block: &str,
    command_context: &CommandContext,
) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    let scope = HashMap::new();
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
        if let Some(cmd) =
            parse_self_chain_commands_scoped(&block[body_start..end_idx], command_context, &scope)
        {
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
        if content[cursor..].starts_with("if") && token_boundary(bytes, cursor, "if".len()) {
            depth += 1;
            cursor += "if".len();
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

fn resolve_command_expr(
    raw: &str,
    metrics: &noteskin_itg::IniData,
    command_context: &CommandContext,
) -> Option<String> {
    let value = raw
        .trim()
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim();
    if value.starts_with("function") {
        return Some(resolve_lua_function_command(value, command_context).unwrap_or_default());
    }
    if value.starts_with("NOTESKIN:GetMetricA(") {
        let args = extract_quoted_strings(value);
        if args.len() >= 2 {
            return metrics.get(&args[0], &args[1]).map(str::to_string);
        }
    }
    if value.starts_with("cmd(") && value.ends_with(')') {
        return Some(value[4..value.len() - 1].trim().to_string());
    }
    if let Some(command) = resolve_helper_command(value, command_context) {
        return Some(command);
    }
    if let Some(q) = parse_lua_quoted(value) {
        return Some(q);
    }
    Some(value.to_string())
}

fn resolve_lua_function_command(value: &str, context: &CommandContext) -> Option<String> {
    let mut rhs = value.find("function")? + "function".len();
    rhs = skip_ws(value, rhs);
    let param_open = *value.as_bytes().get(rhs)?;
    if param_open != b'(' {
        return None;
    }
    let param_close = find_matching(value, rhs, '(', ')')?;
    let body_start = param_close + 1;
    let end_idx = find_function_end(value, body_start)?;
    parse_self_chain_commands_scoped(&value[body_start..end_idx], context, &HashMap::new())
        .or_else(|| Some(String::new()))
}

fn resolve_helper_command(value: &str, context: &CommandContext) -> Option<String> {
    let (name, args) = split_lua_call(value)?;
    let function = context.functions.get(&name.to_ascii_lowercase())?;
    let scope = function
        .params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| {
            (
                param.to_ascii_lowercase(),
                context.resolve_command_arg(arg, &HashMap::new()),
            )
        })
        .collect::<HashMap<_, _>>();
    let Some(body) = return_function_body(&function.body) else {
        return Some(String::new());
    };
    let body = resolve_lua_conditionals(body, &scope);
    Some(parse_self_chain_commands_scoped(&body, context, &scope).unwrap_or_default())
}

impl CommandContext {
    fn resolve_command_arg(&self, raw: &str, scope: &HashMap<String, String>) -> String {
        let key = raw
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_ascii_lowercase();
        scope
            .get(&key)
            .cloned()
            .or_else(|| self.colors.get(&key).cloned())
            .or_else(|| parse_lua_color_expr(raw))
            .unwrap_or_else(|| raw.trim().to_string())
    }
}

fn return_function_body(body: &str) -> Option<&str> {
    let return_idx = body.find("return")?;
    let mut cursor = return_idx + "return".len();
    cursor = skip_ws(body, cursor);
    if !body.get(cursor..).is_some_and(|tail| {
        tail.starts_with("function") && token_boundary(body.as_bytes(), cursor, "function".len())
    }) {
        return None;
    }
    cursor += "function".len();
    cursor = skip_ws(body, cursor);
    if body.as_bytes().get(cursor).is_none_or(|b| *b != b'(') {
        return None;
    }
    let params_close = find_matching(body, cursor, '(', ')')?;
    let body_start = params_close + 1;
    let body_end = find_function_end(body, body_start)?;
    Some(&body[body_start..body_end])
}

fn parse_self_chain_commands_scoped(
    body: &str,
    context: &CommandContext,
    scope: &HashMap<String, String>,
) -> Option<String> {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find("self:") {
        let mut name_start = cursor + rel + 5;
        loop {
            let Some((name, args, next)) = parse_lua_method_call(body, name_start) else {
                cursor = name_start;
                break;
            };
            if !out.is_empty() {
                out.push(';');
            }
            out.push_str(name);
            for arg in args {
                out.push(',');
                out.push_str(&context.resolve_command_arg(&arg, scope));
            }
            cursor = next;

            let chain = skip_ws(body, next);
            if body.as_bytes().get(chain).is_some_and(|b| *b == b':') {
                name_start = chain + 1;
                continue;
            }
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn parse_lua_method_call(body: &str, name_start: usize) -> Option<(&str, Vec<String>, usize)> {
    let bytes = body.as_bytes();
    let mut name_end = name_start;
    while name_end < bytes.len() && is_lua_ident(bytes[name_end]) {
        name_end += 1;
    }
    if name_end == name_start {
        return None;
    }
    let open = skip_ws(body, name_end);
    if bytes.get(open).is_none_or(|b| *b != b'(') {
        return None;
    }
    let close = find_matching(body, open, '(', ')')?;
    Some((
        body[name_start..name_end].trim(),
        split_call_args(&body[open + 1..close]),
        close + 1,
    ))
}

fn resolve_lua_conditionals(body: &str, scope: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(if_idx) = find_lua_keyword(body, cursor, "if") {
        out.push_str(&body[cursor..if_idx]);
        let condition_start = skip_ws(body, if_idx + "if".len());
        let Some(then_idx) = find_lua_keyword(body, condition_start, "then") else {
            out.push_str(&body[if_idx..]);
            return out;
        };
        let Some((else_idx, end_idx)) = find_lua_if_close(body, then_idx + "then".len()) else {
            out.push_str(&body[if_idx..]);
            return out;
        };
        let condition = body[condition_start..then_idx].trim();
        let then_body = &body[then_idx + "then".len()..else_idx.unwrap_or(end_idx)];
        let else_body = else_idx
            .map(|idx| &body[idx + "else".len()..end_idx])
            .unwrap_or("");
        let selected = if eval_lua_condition(condition, scope) {
            then_body
        } else {
            else_body
        };
        out.push_str(&resolve_lua_conditionals(selected, scope));
        cursor = end_idx + "end".len();
    }
    out.push_str(&body[cursor..]);
    out
}

fn find_lua_keyword(content: &str, mut cursor: usize, keyword: &str) -> Option<usize> {
    let bytes = content.as_bytes();
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
        if content[cursor..].starts_with(keyword) && token_boundary(bytes, cursor, keyword.len()) {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

fn find_lua_if_close(content: &str, mut cursor: usize) -> Option<(Option<usize>, usize)> {
    let bytes = content.as_bytes();
    let mut quote = 0u8;
    let mut depth = 1usize;
    let mut else_idx = None;
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
        if content[cursor..].starts_with("if") && token_boundary(bytes, cursor, "if".len()) {
            depth += 1;
            cursor += "if".len();
            continue;
        }
        if content[cursor..].starts_with("else")
            && depth == 1
            && token_boundary(bytes, cursor, "else".len())
        {
            else_idx = Some(cursor);
            cursor += "else".len();
            continue;
        }
        if content[cursor..].starts_with("end") && token_boundary(bytes, cursor, "end".len()) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some((else_idx, cursor));
            }
            cursor += "end".len();
            continue;
        }
        cursor += 1;
    }
    None
}

fn eval_lua_condition(condition: &str, scope: &HashMap<String, String>) -> bool {
    let condition = condition.trim();
    if let Some(rest) = condition.strip_prefix("not ") {
        return !eval_lua_condition(rest, scope);
    }
    let key = condition
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    scope
        .get(&key)
        .map_or_else(|| parse_lua_bool(condition), |value| parse_lua_bool(value))
}

fn split_lua_call(value: &str) -> Option<(&str, Vec<String>)> {
    let open = value.find('(')?;
    let name = value[..open].trim();
    if name.is_empty() || !name.as_bytes().iter().all(|b| is_lua_ident(*b)) {
        return None;
    }
    let close = find_matching(value, open, '(', ')')?;
    if close + 1 != value.len() {
        return None;
    }
    Some((name, split_call_args(&value[open + 1..close])))
}

fn parse_lua_bool(raw: &str) -> bool {
    let value = raw.trim().trim_matches('"').trim_matches('\'');
    value.eq_ignore_ascii_case("true") || value == "1"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_chain_commands_append_in_call_order() {
        let commands = parse_self_chain_commands_scoped(
            "self:zoom(1):xy(2,3):stoptweening()",
            &CommandContext::default(),
            &HashMap::new(),
        );
        assert_eq!(commands.as_deref(), Some("zoom,1;xy,2,3;stoptweening"));
    }

    #[test]
    fn color_list_uses_first_four_valid_components() {
        assert_eq!(
            parse_color_list("bad, 0.75, 0.5, 0.25, 1, 0"),
            Some([0.75, 0.5, 0.25, 1.0])
        );
        assert_eq!(parse_color_list("1, bad, 0.5, 0.25"), None);
    }

    #[test]
    fn wrapper_commands_parse_metric_cmd_and_function_forms() {
        let path = std::env::temp_dir().join(format!(
            "deadsync-noteskin-wrapper-test-{}.ini",
            std::process::id()
        ));
        std::fs::write(&path, "[ReceptorArrow]\nNoneCommand=diffusealpha,0.5\n")
            .expect("write test metrics");
        let metrics = noteskin_itg::IniData::parse_file(&path).expect("parse test metrics");
        let _ = std::fs::remove_file(&path);
        let content = r#"
return Def.ActorFrame .. {
    InitCommand=cmd(diffusealpha,0);
    NoneCommand=NOTESKIN:GetMetricA("ReceptorArrow", "NoneCommand");
    PressCommand=function(self)
        if true then
            self:zoom(1.2)
        end
        self:diffusealpha(1)
    end;
}
"#;

        let commands = parse_wrapper_commands(content, &metrics).expect("wrapper commands");

        assert_eq!(
            commands.get("initcommand").map(String::as_str),
            Some("diffusealpha,0")
        );
        assert_eq!(
            commands.get("nonecommand").map(String::as_str),
            Some("diffusealpha,0.5")
        );
        assert_eq!(
            commands.get("presscommand").map(String::as_str),
            Some("zoom,1.2;diffusealpha,1")
        );
    }

    #[test]
    fn wrapper_commands_ignore_non_wrapper_content() {
        let metrics = noteskin_itg::IniData::default();

        assert!(parse_wrapper_commands("return Def.ActorFrame {}", &metrics).is_none());
    }

    #[test]
    fn lua_path_detection_is_case_insensitive() {
        assert!(is_lua_path(Path::new("Down Receptor.LUA")));
        assert!(!is_lua_path(Path::new("Down Receptor.png")));
        assert!(!is_lua_path(Path::new("Down Receptor")));
    }

    #[test]
    fn element_hint_matching_is_case_insensitive() {
        let cases = [
            ("Down Hold Explosion", "hold explosion", true),
            ("Down Tap Note", "hold explosion", false),
            ("ROLL EXPLOSION", "roll", true),
            ("Down Hold Explosion", "", true),
            ("", "hold", false),
            ("Döwn Hold Explosion", "DÖWN", false),
        ];
        for (element, hint, expected) in cases {
            assert_eq!(element_contains_hint(element, hint), expected);
            assert_eq!(
                element_contains_hint(element, hint),
                element_contains_hint_legacy_for_bench(element, hint)
            );
        }
    }
}
