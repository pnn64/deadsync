use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::itg as noteskin_itg;
use crate::lua::{ItgCallArgs, itg_call_args, itg_quoted_strings};
use crate::script::parse_linear_frames_expr;

pub const ITG_ARG0_TOKEN: &str = "__ITG_ARG0__";
pub const ITG_ACTOR_UPDATE_COMMAND: &str = "__deadsync_actor_update";
pub const ITG_BEAT_FADE_GLOW_UPDATE: &str = "beat_fade_glow";

const STACK_LOWERCASE_KEY_CAPACITY: usize = 128;
type Arg0Aliases<'a> = SmallVec<[&'a str; 4]>;
const BEAT_UPDATE_MARKER: &[u8] = b"setupdatefunction,";
const BEAT_FADE_GLOW_SIGNATURES: [&[u8]; 4] = [
    b"part=beat%1",
    b"part=clamp(part,0,0.5)",
    b"eff=scale(part,0,0.5,1,0)",
    b".glow:diffusealpha(eff)",
];

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
    pub condition_expr: Option<String>,
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

#[must_use]
pub fn parse_actor_decl(content: &str, metrics: &noteskin_itg::IniData) -> ItgLuaActorDecl {
    let content = strip_lua_comments(content);
    let content = content.as_str();
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
        let args = parse_path_loadactor_args(&content[open + 1..close], &arg0_aliases);
        let (commands, next_cursor) =
            find_post_call_commands(content, close, metrics, &command_context);
        let frame_override = find_post_call_frame_override(content, close);
        if let Some((path_expr, arg_expr)) = args {
            decl.path_refs.push(ItgLuaPathRefDecl {
                path_expr,
                arg_expr,
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
        let mut condition_expr = find_post_call_property(content, close, "Condition");
        if commands.is_empty()
            && let Some((wrapper, outer_close)) =
                find_enclosing_loadactor_for_noteskin(content, call_start, close)
        {
            wrapper_expr = Some(wrapper.to_string());
            let (outer_commands, outer_next_cursor) =
                find_post_call_commands(content, outer_close, metrics, &command_context);
            let outer_frame_override = find_post_call_frame_override(content, outer_close);
            if outer_commands.is_empty() {
                next_cursor = outer_close + 1;
            } else {
                commands = outer_commands;
                next_cursor = outer_next_cursor;
                frame_override = outer_frame_override;
                condition_expr = find_post_call_property(content, outer_close, "Condition");
            }
        }
        decl.refs.push(ItgLuaRefDecl {
            button_override,
            element,
            wrapper_expr,
            frame_override,
            condition_expr,
            commands,
        });
        cursor = next_cursor;
    }

    if has_beat_fade_glow_update(content, &command_context) {
        let mark = |commands: &mut HashMap<String, String>| {
            commands.insert(
                ITG_ACTOR_UPDATE_COMMAND.to_string(),
                ITG_BEAT_FADE_GLOW_UPDATE.to_string(),
            );
        };
        decl.sprites
            .iter_mut()
            .for_each(|sprite| mark(&mut sprite.commands));
        decl.models
            .iter_mut()
            .for_each(|model| mark(&mut model.commands));
        decl.refs
            .iter_mut()
            .for_each(|reference| mark(&mut reference.commands));
        decl.path_refs
            .iter_mut()
            .for_each(|reference| mark(&mut reference.commands));
    }

    decl
}

fn has_beat_fade_glow_update(content: &str, context: &CommandContext) -> bool {
    let Some(update_start) = find_compact_ascii_case_insensitive(content, BEAT_UPDATE_MARKER)
    else {
        return false;
    };
    let Some(function) =
        with_compact_ascii_identifier(&content[update_start..], |name| context.functions.get(name))
            .flatten()
    else {
        return false;
    };
    has_beat_fade_glow_signature(&function.body)
}

fn find_compact_ascii_case_insensitive(content: &str, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    content.char_indices().find_map(|(start, ch)| {
        (!ch.is_whitespace() && ch.is_ascii() && (ch as u8).eq_ignore_ascii_case(&needle[0]))
            .then(|| compact_ascii_match_end(content, start, needle))
            .flatten()
    })
}

fn compact_ascii_match_end(content: &str, start: usize, needle: &[u8]) -> Option<usize> {
    let mut chars = content[start..].char_indices();
    let mut end = start;
    for expected in needle {
        let (offset, ch) = chars.find(|(_, ch)| !ch.is_whitespace())?;
        if !ch.is_ascii() || !(ch as u8).eq_ignore_ascii_case(expected) {
            return None;
        }
        end = start + offset + ch.len_utf8();
    }
    Some(end)
}

fn with_compact_ascii_identifier<T>(content: &str, use_key: impl FnOnce(&str) -> T) -> Option<T> {
    let mut start = None;
    let mut end = 0usize;
    let mut gap = false;
    let mut internal_gap = false;
    for (idx, ch) in content.char_indices() {
        if ch.is_whitespace() {
            if start.is_some() {
                gap = true;
            }
            continue;
        }
        if !ch.is_ascii() || !is_lua_ident(ch as u8) {
            break;
        }
        start.get_or_insert(idx);
        internal_gap |= gap;
        gap = false;
        end = idx + ch.len_utf8();
    }
    let start = start?;
    if !internal_gap {
        return Some(with_ascii_lowercase_key(&content[start..end], use_key));
    }

    let mut normalized = [0_u8; STACK_LOWERCASE_KEY_CAPACITY];
    let mut len = 0usize;
    for ch in content[start..end].chars().filter(|ch| !ch.is_whitespace()) {
        if len == normalized.len() {
            let key = content[start..end]
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .map(|ch| ch.to_ascii_lowercase())
                .collect::<String>();
            return Some(use_key(&key));
        }
        normalized[len] = ch.to_ascii_lowercase() as u8;
        len += 1;
    }
    let key =
        std::str::from_utf8(&normalized[..len]).expect("Lua identifiers contain only ASCII bytes");
    Some(use_key(key))
}

fn has_beat_fade_glow_signature(body: &str) -> bool {
    BEAT_FADE_GLOW_SIGNATURES
        .iter()
        .all(|signature| find_compact_ascii_case_insensitive(body, signature).is_some())
}

#[cfg(test)]
fn has_beat_fade_glow_update_reference(content: &str, context: &CommandContext) -> bool {
    let compact = content
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let Some(update) = compact.split("setupdatefunction,").nth(1) else {
        return false;
    };
    let function_name = update
        .bytes()
        .take_while(|byte| is_lua_ident(*byte))
        .map(char::from)
        .collect::<String>();
    let Some(function) = get_ascii_lowercase(&context.functions, &function_name) else {
        return false;
    };
    has_beat_fade_glow_signature_reference(&function.body)
}

#[cfg(any(test, feature = "bench-support"))]
fn has_beat_fade_glow_signature_reference(body: &str) -> bool {
    let body = body
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    body.contains("part=beat%1")
        && body.contains("part=clamp(part,0,0.5)")
        && body.contains("eff=scale(part,0,0.5,1,0)")
        && body.contains(".glow:diffusealpha(eff)")
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn itg_has_beat_update_marker_for_bench(content: &str) -> bool {
    find_compact_ascii_case_insensitive(content, BEAT_UPDATE_MARKER).is_some()
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn itg_has_beat_update_marker_reference_for_bench(content: &str) -> bool {
    content
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
        .contains("setupdatefunction,")
}

#[cfg(any(test, feature = "bench-support"))]
fn identifier_checksum(name: &str) -> u64 {
    name.bytes().fold(name.len() as u64, |checksum, byte| {
        checksum
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(u64::from(byte))
    })
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn itg_update_function_name_for_bench(content: &str) -> Option<u64> {
    with_compact_ascii_identifier(content, identifier_checksum)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn itg_update_function_name_reference_for_bench(content: &str) -> Option<u64> {
    let name = content
        .bytes()
        .take_while(|byte| is_lua_ident(*byte))
        .map(char::from)
        .collect::<String>();
    (!name.is_empty()).then(|| identifier_checksum(&name))
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn itg_has_beat_fade_glow_signature_for_bench(body: &str) -> bool {
    has_beat_fade_glow_signature(body)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn itg_has_beat_fade_glow_signature_reference_for_bench(body: &str) -> bool {
    has_beat_fade_glow_signature_reference(body)
}

fn strip_lua_comments(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut out = Vec::with_capacity(content.len());
    let mut idx = 0usize;
    let mut quote = 0u8;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if quote != 0 {
            out.push(byte);
            if byte == b'\\' && idx + 1 < bytes.len() {
                idx += 1;
                out.push(bytes[idx]);
            } else if byte == quote {
                quote = 0;
            }
            idx += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = byte;
            out.push(byte);
            idx += 1;
            continue;
        }
        if byte != b'-' || bytes.get(idx + 1) != Some(&b'-') {
            out.push(byte);
            idx += 1;
            continue;
        }

        let mut body = idx + 2;
        let mut equals = 0usize;
        if bytes.get(body) == Some(&b'[') {
            body += 1;
            while bytes.get(body) == Some(&b'=') {
                equals += 1;
                body += 1;
            }
        }
        let long_comment = bytes.get(body) == Some(&b'[');
        if long_comment {
            idx = body + 1;
            while idx < bytes.len() {
                let equals_end = idx + 1 + equals;
                let closing = bytes[idx] == b']'
                    && equals_end < bytes.len()
                    && bytes[idx + 1..equals_end].iter().all(|byte| *byte == b'=')
                    && bytes[equals_end] == b']';
                if closing {
                    idx += equals + 2;
                    break;
                }
                out.push(if bytes[idx] == b'\n' { b'\n' } else { b' ' });
                idx += 1;
            }
        } else {
            idx += 2;
            while idx < bytes.len() && bytes[idx] != b'\n' {
                out.push(b' ');
                idx += 1;
            }
        }
    }
    String::from_utf8(out).expect("comment removal preserves valid UTF-8")
}

#[must_use]
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

#[must_use]
pub fn parse_wrapper_commands_from_file(
    path: &Path,
    metrics: &noteskin_itg::IniData,
) -> Option<HashMap<String, String>> {
    if !is_lua_path(path) {
        return None;
    }
    parse_wrapper_commands(&fs::read_to_string(path).ok()?, metrics)
}

#[must_use]
pub fn is_lua_path(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
}

#[must_use]
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

fn parse_arg0_aliases(content: &str) -> Arg0Aliases<'_> {
    let mut out = SmallVec::new();
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
            && !out.contains(&name)
        {
            out.push(name);
        }
    }
    out
}

#[cfg(any(test, feature = "bench-support"))]
fn parse_arg0_aliases_reference(content: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
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

fn rewrite_arg0_expr(expr: &str, arg0_aliases: &[&str]) -> String {
    let trimmed = expr.trim();
    if trimmed == "..." || arg0_aliases.contains(&trimmed) {
        ITG_ARG0_TOKEN.to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_path_loadactor_args(raw: &str, arg0_aliases: &[&str]) -> Option<(String, Option<String>)> {
    let mut args = itg_call_args(raw);
    let path_expr = rewrite_arg0_expr(args.next()?, arg0_aliases);
    let arg_expr = args.next().map(|arg| rewrite_arg0_expr(arg, arg0_aliases));
    Some((path_expr, arg_expr))
}

#[cfg(any(test, feature = "bench-support"))]
fn parse_path_loadactor_args_reference(
    raw: &str,
    arg0_aliases: &[&str],
) -> Option<(String, Option<String>)> {
    let args = split_call_args(raw);
    let path_expr = rewrite_arg0_expr(args.first()?, arg0_aliases);
    let arg_expr = args.get(1).map(|arg| rewrite_arg0_expr(arg, arg0_aliases));
    Some((path_expr, arg_expr))
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
    let inner = if value
        .as_bytes()
        .get(.."color(".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"color("))
        && value.ends_with(')')
    {
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

#[cfg(any(test, feature = "bench-support"))]
fn parse_lua_color_expr_reference(raw: &str) -> Option<String> {
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_parse_actor_color_for_bench(raw: &str) -> Option<String> {
    parse_lua_color_expr(raw)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_parse_actor_color_reference_for_bench(raw: &str) -> Option<String> {
    parse_lua_color_expr_reference(raw)
}

fn parse_hex_color(raw: &str) -> Option<[f32; 4]> {
    let hex = raw.trim().strip_prefix('#')?;
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let byte = |idx: usize| u8::from_str_radix(&hex[idx..idx + 2], 16).ok();
    Some([
        f32::from(byte(0)?) / 255.0,
        f32::from(byte(2)?) / 255.0,
        f32::from(byte(4)?) / 255.0,
        if hex.len() == 8 {
            f32::from(byte(6)?) / 255.0
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
        let params = parse_local_function_params(&content[open + 1..close]);
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

fn parse_local_function_params(raw: &str) -> Vec<String> {
    itg_call_args(raw)
        .filter(|param| param.as_bytes().iter().all(|b| is_lua_ident(*b)))
        .map(str::to_owned)
        .collect()
}

#[cfg(any(test, feature = "bench-support"))]
fn parse_local_function_params_reference(raw: &str) -> Vec<String> {
    split_call_args(raw)
        .into_iter()
        .filter(|param| param.as_bytes().iter().all(|b| is_lua_ident(*b)))
        .collect()
}

#[cfg(any(test, feature = "bench-support"))]
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

fn find_post_call_property(content: &str, call_close: usize, property: &str) -> Option<String> {
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
    for raw in content[after + 1..end].lines() {
        let line = raw
            .split_once("--")
            .map_or(raw, |(prefix, _)| prefix)
            .trim()
            .trim_end_matches(',')
            .trim_end_matches(';')
            .trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(property) {
            return Some(value.trim().to_string());
        }
    }
    None
}

#[must_use]
pub fn actor_condition_matches(condition: &str, button: &str, steps_type: &str) -> bool {
    condition
        .split(" and ")
        .all(|term| actor_condition_term(term, button, steps_type).unwrap_or(true))
}

fn actor_condition_term(term: &str, button: &str, steps_type: &str) -> Option<bool> {
    let term = term.trim();
    let (lhs, rhs, equal) = if let Some((lhs, rhs)) = term.split_once("~=") {
        (lhs, rhs, false)
    } else if let Some((lhs, rhs)) = term.split_once("==") {
        (lhs, rhs, true)
    } else {
        return match term {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        };
    };
    let lhs = lhs.trim();
    let actual = if lhs.contains("Var \"Button\"") || lhs.contains("Var 'Button'") {
        button
    } else if lhs.contains("GetCurrentStyle") && lhs.contains("GetStepsType") {
        steps_type
    } else {
        return None;
    };
    let expected = parse_lua_quoted(rhs.trim())?;
    Some(actual.eq_ignore_ascii_case(&expected) == equal)
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
    let digits = frame_tail[frame_eq + 1..].trim_start();
    let end = digits
        .bytes()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    digits[..end].parse::<usize>().ok()
}

#[cfg(any(test, feature = "bench-support"))]
fn parse_frame_override_block_reference(block: &str) -> Option<usize> {
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
) -> Option<(&str, usize)> {
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
            let mut args = itg_call_args(&content[open + 1..outer_close]);
            let wrapper = args.next()?;
            args.next()?;
            return Some((wrapper, outer_close));
        }
        search_end = pos;
    }
    None
}

#[cfg(any(test, feature = "bench-support"))]
fn find_enclosing_loadactor_for_noteskin_reference(
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
        if let Some(idx) = parse_numbered_actor_key(key, b"frame") {
            if let Ok(parsed) = value.parse::<usize>() {
                frame_seen = true;
                frame_state_max = frame_state_max.max(idx);
                if idx == 0 {
                    frame0 = parsed;
                }
                frame_indices.insert(idx, parsed);
            }
            continue;
        }
        if let Some(idx) = parse_numbered_actor_key(key, b"delay") {
            if let Some(delay) = parse_lua_float_token(value) {
                frame_delays.insert(idx, delay.max(0.0));
            }
            continue;
        }
        if has_command_suffix(key)
            && let Some(cmd) = resolve_command_expr(value, metrics, command_context)
        {
            commands.insert(key.to_ascii_lowercase(), cmd);
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
        if key.eq_ignore_ascii_case("frame0000") {
            if let Ok(parsed) = value.parse::<usize>() {
                frame0 = parsed;
            }
            continue;
        }
        if has_command_suffix(key)
            && let Some(cmd) = resolve_command_expr(value, metrics, command_context)
        {
            commands.insert(key.to_ascii_lowercase(), cmd);
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
    let mut quoted = itg_quoted_strings(args);
    let first = quoted.next()?;
    let mut last = first;
    let mut count = 1usize;
    for value in quoted {
        last = value;
        count += 1;
    }
    let element = last.to_string();
    let button_override = if args.contains("Var \"Button\"") || args.contains("Var 'Button'") {
        None
    } else if count >= 2 {
        Some(first.to_string())
    } else {
        None
    };
    Some((button_override, element))
}

#[cfg(any(test, feature = "bench-support"))]
fn parse_loadactor_args_reference(args: &str) -> Option<(Option<String>, String)> {
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
        let key = k.trim();
        if !has_command_suffix(key) {
            continue;
        }
        if let Some(cmd) = resolve_command_expr(v.trim(), metrics, command_context) {
            commands.insert(key.to_ascii_lowercase(), cmd);
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
        let key = block[key_start..eq].trim();
        if !has_command_suffix(key) {
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
            commands.insert(key.to_ascii_lowercase(), cmd);
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

const fn token_boundary(bytes: &[u8], start: usize, len: usize) -> bool {
    let prev_ok = start == 0 || !is_lua_ident(bytes[start - 1]);
    let end = start + len;
    let next_ok = end >= bytes.len() || !is_lua_ident(bytes[end]);
    prev_ok && next_ok
}

const fn is_lua_ident(b: u8) -> bool {
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
    if let Some(metric) = resolve_metric_command_expr(value, metrics) {
        return metric;
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

fn resolve_metric_command_expr(
    value: &str,
    metrics: &noteskin_itg::IniData,
) -> Option<Option<String>> {
    if !value.starts_with("NOTESKIN:GetMetricA(") {
        return None;
    }
    let mut args = itg_quoted_strings(value);
    let section = args.next()?;
    let key = args.next()?;
    Some(metrics.get(section, key).map(str::to_string))
}

#[cfg(any(test, feature = "bench-support"))]
fn resolve_metric_command_expr_reference(
    value: &str,
    metrics: &noteskin_itg::IniData,
) -> Option<Option<String>> {
    if !value.starts_with("NOTESKIN:GetMetricA(") {
        return None;
    }
    let args = extract_quoted_strings(value);
    if args.len() < 2 {
        return None;
    }
    Some(metrics.get(&args[0], &args[1]).map(str::to_string))
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
    let function = get_ascii_lowercase(&context.functions, name)?;
    let scope = function
        .params
        .iter()
        .zip(args)
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
        let key = raw.trim().trim_matches('"').trim_matches('\'');
        get_ascii_lowercase_from_two(scope, &self.colors, key)
            .cloned()
            .or_else(|| parse_lua_color_expr(raw))
            .unwrap_or_else(|| raw.trim().to_string())
    }
}

fn get_ascii_lowercase<'a, V>(map: &'a HashMap<String, V>, key: &str) -> Option<&'a V> {
    with_ascii_lowercase_key(key, |normalized| map.get(normalized))
}

fn get_ascii_lowercase_from_two<'a, V>(
    first: &'a HashMap<String, V>,
    second: &'a HashMap<String, V>,
    key: &str,
) -> Option<&'a V> {
    with_ascii_lowercase_key(key, |normalized| {
        first.get(normalized).or_else(|| second.get(normalized))
    })
}

fn with_ascii_lowercase_key<T>(key: &str, use_key: impl FnOnce(&str) -> T) -> T {
    if !key.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return use_key(key);
    }
    if key.len() <= STACK_LOWERCASE_KEY_CAPACITY {
        let mut normalized = [0_u8; STACK_LOWERCASE_KEY_CAPACITY];
        normalized[..key.len()].copy_from_slice(key.as_bytes());
        normalized[..key.len()].make_ascii_lowercase();
        // SAFETY: `normalized` starts as valid UTF-8 copied from `key`, and
        // ASCII case conversion cannot change UTF-8 byte structure.
        let normalized = unsafe { std::str::from_utf8_unchecked(&normalized[..key.len()]) };
        return use_key(normalized);
    }
    let normalized = key.to_ascii_lowercase();
    use_key(&normalized)
}

fn parse_numbered_actor_key(key: &str, prefix: &[u8]) -> Option<usize> {
    let (key_prefix, digits) = key.as_bytes().split_at_checked(prefix.len())?;
    if !key_prefix.eq_ignore_ascii_case(prefix)
        || digits.is_empty()
        || !digits.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

fn has_command_suffix(key: &str) -> bool {
    key.as_bytes()
        .get(key.len().saturating_sub("command".len())..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b"command"))
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
                out.push_str(&context.resolve_command_arg(arg, scope));
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

fn parse_lua_method_call(body: &str, name_start: usize) -> Option<(&str, ItgCallArgs<'_>, usize)> {
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
        itg_call_args(&body[open + 1..close]),
        close + 1,
    ))
}

#[cfg(any(test, feature = "bench-support"))]
fn parse_lua_method_call_reference(
    body: &str,
    name_start: usize,
) -> Option<(&str, Vec<String>, usize)> {
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

#[cfg(any(test, feature = "bench-support"))]
fn parse_self_chain_commands_scoped_reference(
    body: &str,
    context: &CommandContext,
    scope: &HashMap<String, String>,
) -> Option<String> {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find("self:") {
        let mut name_start = cursor + rel + 5;
        loop {
            let Some((name, args, next)) = parse_lua_method_call_reference(body, name_start) else {
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
    let key = condition.trim_matches('"').trim_matches('\'');
    get_ascii_lowercase(scope, key)
        .map_or_else(|| parse_lua_bool(condition), |value| parse_lua_bool(value))
}

fn split_lua_call(value: &str) -> Option<(&str, ItgCallArgs<'_>)> {
    let open = value.find('(')?;
    let name = value[..open].trim();
    if name.is_empty() || !name.as_bytes().iter().all(|b| is_lua_ident(*b)) {
        return None;
    }
    let close = find_matching(value, open, '(', ')')?;
    if close + 1 != value.len() {
        return None;
    }
    Some((name, itg_call_args(&value[open + 1..close])))
}

#[cfg(any(test, feature = "bench-support"))]
fn split_lua_call_reference(value: &str) -> Option<(&str, Vec<String>)> {
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

#[cfg(any(test, feature = "bench-support"))]
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

const fn skip_ws(content: &str, mut idx: usize) -> usize {
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

#[cfg(feature = "bench-support")]
fn alias_checksum<'a>(aliases: impl IntoIterator<Item = &'a str>, len: usize) -> u64 {
    aliases.into_iter().fold(len as u64, |checksum, alias| {
        checksum.wrapping_add(identifier_checksum(alias))
    })
}

#[cfg(feature = "bench-support")]
fn loadactor_args_checksum(args: Option<(Option<String>, String)>) -> u64 {
    let Some((button, element)) = args else {
        return 1;
    };
    let checksum = button.as_deref().map_or(2, identifier_checksum);
    checksum
        .wrapping_mul(1_099_511_628_211)
        .wrapping_add(identifier_checksum(&element))
}

#[cfg(feature = "bench-support")]
fn frame_override_checksum(frame: Option<usize>) -> u64 {
    frame.map_or(1, |frame| frame as u64 + 2)
}

#[cfg(feature = "bench-support")]
fn metric_command_checksum(metric: Option<Option<String>>) -> u64 {
    match metric {
        None => 1,
        Some(None) => 2,
        Some(Some(value)) => identifier_checksum(&value).wrapping_add(3),
    }
}

#[cfg(feature = "bench-support")]
fn lua_call_checksum<'a>(name: &str, args: impl IntoIterator<Item = &'a str>) -> u64 {
    args.into_iter().fold(
        identifier_checksum(name).wrapping_add(1),
        |checksum, arg| {
            checksum
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(identifier_checksum(arg))
        },
    )
}

#[cfg(feature = "bench-support")]
fn path_loadactor_checksum(args: Option<(String, Option<String>)>) -> u64 {
    let Some((path, arg)) = args else {
        return 1;
    };
    identifier_checksum(&path)
        .wrapping_mul(1_099_511_628_211)
        .wrapping_add(arg.as_deref().map_or(2, identifier_checksum))
}

#[cfg(feature = "bench-support")]
fn enclosing_wrapper_checksum(wrapper: Option<(String, usize)>) -> u64 {
    let Some((wrapper, close)) = wrapper else {
        return 1;
    };
    identifier_checksum(&wrapper)
        .wrapping_mul(1_099_511_628_211)
        .wrapping_add(close as u64)
}

#[cfg(feature = "bench-support")]
fn local_params_checksum(params: &[String]) -> u64 {
    params.iter().fold(params.len() as u64, |checksum, param| {
        checksum
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(identifier_checksum(param))
    })
}

#[cfg(feature = "bench-support")]
fn noteskin_loadactor_bounds(content: &str) -> Option<(usize, usize)> {
    let call_start = content.find("NOTESKIN:LoadActor(")?;
    let open = call_start + "NOTESKIN:LoadActor".len();
    Some((call_start, find_matching(content, open, '(', ')')?))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_arg0_aliases_for_bench(content: &str) -> u64 {
    let aliases = parse_arg0_aliases(content);
    alias_checksum(aliases.iter().copied(), aliases.len())
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_arg0_aliases_reference_for_bench(content: &str) -> u64 {
    let aliases = parse_arg0_aliases_reference(content);
    alias_checksum(aliases.iter().map(String::as_str), aliases.len())
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_loadactor_args_for_bench(args: &str) -> u64 {
    loadactor_args_checksum(parse_loadactor_args(args))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_loadactor_args_reference_for_bench(args: &str) -> u64 {
    loadactor_args_checksum(parse_loadactor_args_reference(args))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_parse_actor_self_chain_for_bench(body: &str) -> Option<String> {
    parse_self_chain_commands_scoped(body, &CommandContext::default(), &HashMap::new())
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_parse_actor_self_chain_reference_for_bench(body: &str) -> Option<String> {
    parse_self_chain_commands_scoped_reference(body, &CommandContext::default(), &HashMap::new())
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_frame_override_for_bench(block: &str) -> u64 {
    frame_override_checksum(parse_frame_override_block(block))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_frame_override_reference_for_bench(block: &str) -> u64 {
    frame_override_checksum(parse_frame_override_block_reference(block))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_metric_command_for_bench(value: &str, metrics: &noteskin_itg::IniData) -> u64 {
    metric_command_checksum(resolve_metric_command_expr(value, metrics))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_metric_command_reference_for_bench(value: &str, metrics: &noteskin_itg::IniData) -> u64 {
    metric_command_checksum(resolve_metric_command_expr_reference(value, metrics))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_helper_call_for_bench(value: &str) -> u64 {
    let Some((name, args)) = split_lua_call(value) else {
        return 1;
    };
    lua_call_checksum(name, args).wrapping_add(2)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_helper_call_reference_for_bench(value: &str) -> u64 {
    let Some((name, args)) = split_lua_call_reference(value) else {
        return 1;
    };
    lua_call_checksum(name, args.iter().map(String::as_str)).wrapping_add(2)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_path_loadactor_args_for_bench(raw: &str) -> u64 {
    path_loadactor_checksum(parse_path_loadactor_args(raw, &["path", "alias_name"]))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_path_loadactor_args_reference_for_bench(raw: &str) -> u64 {
    path_loadactor_checksum(parse_path_loadactor_args_reference(
        raw,
        &["path", "alias_name"],
    ))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_enclosing_loadactor_wrapper_for_bench(content: &str) -> u64 {
    let wrapper = noteskin_loadactor_bounds(content).and_then(|(start, close)| {
        find_enclosing_loadactor_for_noteskin(content, start, close)
            .map(|(wrapper, outer_close)| (wrapper.to_string(), outer_close))
    });
    enclosing_wrapper_checksum(wrapper)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_enclosing_loadactor_wrapper_reference_for_bench(content: &str) -> u64 {
    let wrapper = noteskin_loadactor_bounds(content).and_then(|(start, close)| {
        let (args, outer_close) =
            find_enclosing_loadactor_for_noteskin_reference(content, start, close)?;
        (args.len() >= 2).then(|| (args[0].clone(), outer_close))
    });
    enclosing_wrapper_checksum(wrapper)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_local_function_params_for_bench(raw: &str) -> u64 {
    local_params_checksum(&parse_local_function_params(raw))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn itg_local_function_params_reference_for_bench(raw: &str) -> u64 {
    local_params_checksum(&parse_local_function_params_reference(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_inline_arg0_aliases_preserve_rewrite_behavior() {
        let content =
            "local path = ...;\nlocal alias_name = ...\nlocal path = ...\nlocal other = 3";
        let current = parse_arg0_aliases(content);
        let reference = parse_arg0_aliases_reference(content);

        assert_eq!(current.as_slice(), ["path", "alias_name"]);
        assert_eq!(current.len(), reference.len());
        assert!(current.iter().all(|alias| reference.contains(*alias)));
        assert_eq!(rewrite_arg0_expr(" path ", &current), ITG_ARG0_TOKEN);
        assert_eq!(rewrite_arg0_expr("...", &current), ITG_ARG0_TOKEN);
        assert_eq!(rewrite_arg0_expr("other", &current), "other");
    }

    #[test]
    fn borrowed_loadactor_quotes_preserve_first_and_last_arguments() {
        let cases = [
            "\"Left\", \"Tap Note\"",
            "Var \"Button\", 'Receptor'",
            "'Tap Mine'",
            "'Left', helper('ignored'), \"Hold Body Active\"",
            "",
        ];
        for args in cases {
            assert_eq!(
                parse_loadactor_args(args),
                parse_loadactor_args_reference(args),
                "arguments {args:?}"
            );
        }
        assert_eq!(
            parse_loadactor_args("\"Left\", \"Tap Note\""),
            Some((Some("Left".to_string()), "Tap Note".to_string()))
        );
        assert_eq!(
            parse_loadactor_args("Var \"Button\", 'Receptor'"),
            Some((None, "Receptor".to_string()))
        );
    }

    #[test]
    fn borrowed_self_chain_arguments_preserve_compiled_commands() {
        let body = r#"
self:zoom(1.25):xy(3, 4)
self:queuecommand("Ready"):visible(true)
self:diffuse(scale(beat, 0, 1))
"#;
        let context = CommandContext::default();
        let scope = HashMap::new();
        let current = parse_self_chain_commands_scoped(body, &context, &scope);
        let reference = parse_self_chain_commands_scoped_reference(body, &context, &scope);

        assert_eq!(current, reference);
        assert_eq!(
            current.as_deref(),
            Some("zoom,1.25;xy,3,4;queuecommand,\"Ready\";visible,true;diffuse,scale(beat, 0, 1)")
        );
    }

    #[test]
    fn borrowed_frame_digits_preserve_frame_overrides() {
        let cases = [
            ("Frames = { Frame = 12, Delay = 0.1 }", Some(12)),
            ("Frames={ Delay=0.1, Frame = 007 }", Some(7)),
            ("Frames = { Frame = 3frames }", Some(3)),
            ("Frames = { Frame = -1 }", None),
            ("Frames = { Frame = }", None),
            ("Other = { Frame = 2 }", None),
            ("Frames = nope", None),
            ("Frames = { Nested = {}; Frame = 42 }", Some(42)),
        ];

        for (block, expected) in cases {
            assert_eq!(
                parse_frame_override_block(block),
                expected,
                "block {block:?}"
            );
            assert_eq!(
                parse_frame_override_block(block),
                parse_frame_override_block_reference(block),
                "block {block:?}"
            );
        }
    }

    #[test]
    fn borrowed_metric_arguments_preserve_command_resolution() {
        let path = std::env::temp_dir().join(format!(
            "deadsync-noteskin-metric-command-test-{}.ini",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[ReceptorArrow]\nNoneCommand=diffusealpha,0.5\nPressCommand=zoom,1.2\n",
        )
        .expect("write test metrics");
        let metrics = noteskin_itg::IniData::parse_file(&path).expect("parse test metrics");
        let _ = std::fs::remove_file(&path);
        let cases = [
            "NOTESKIN:GetMetricA(\"ReceptorArrow\", \"NoneCommand\")",
            "NOTESKIN:GetMetricA('receptorarrow', 'presscommand')",
            "NOTESKIN:GetMetricA(\"ReceptorArrow\", \"MissingCommand\")",
            "NOTESKIN:GetMetricA(\"ReceptorArrow\")",
            "cmd(diffusealpha,0)",
        ];

        for value in cases {
            assert_eq!(
                resolve_metric_command_expr(value, &metrics),
                resolve_metric_command_expr_reference(value, &metrics),
                "expression {value:?}"
            );
        }
        assert_eq!(
            resolve_metric_command_expr(cases[0], &metrics),
            Some(Some("diffusealpha,0.5".to_string()))
        );
        assert_eq!(resolve_metric_command_expr(cases[2], &metrics), Some(None));
        assert_eq!(resolve_metric_command_expr(cases[3], &metrics), None);
    }

    #[test]
    fn borrowed_helper_arguments_preserve_top_level_splitting() {
        let cases = [
            "pulse(1.25, 0.5)",
            "colorize(Color(1, 0.5, 0.25, 1), 'Ready,Set')",
            "nested(scale(beat, 0, 1), {1, 2}, values[3])",
            "empty()",
            "spaced ( first , second )",
            "invalid-name(1)",
            "helper(1) trailing",
            "not a call",
        ];

        for value in cases {
            match (split_lua_call(value), split_lua_call_reference(value)) {
                (Some((current_name, current_args)), Some((reference_name, reference_args))) => {
                    assert_eq!(current_name, reference_name, "call {value:?}");
                    assert_eq!(
                        current_args.collect::<Vec<_>>(),
                        reference_args
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>(),
                        "call {value:?}"
                    );
                }
                (None, None) => {}
                _ => panic!("call behavior diverged for {value:?}"),
            }
        }

        let (_, args) = split_lua_call(cases[1]).expect("valid helper call");
        assert_eq!(
            args.collect::<Vec<_>>(),
            ["Color(1, 0.5, 0.25, 1)", "'Ready,Set'"]
        );
    }

    #[test]
    fn borrowed_path_loadactor_arguments_preserve_rewrites() {
        let aliases = ["path", "alias_name"];
        let cases = [
            "\"actor.lua\"",
            "\"actor.lua\", \"Left\"",
            "..., Var \"Button\"",
            "path, alias_name",
            "helper(1, 2), { Color = 'red,blue' }",
            ", \"fallback.lua\"",
            "\"actor.lua\", \"Left\", \"ignored\"",
            "",
        ];

        for raw in cases {
            assert_eq!(
                parse_path_loadactor_args(raw, &aliases),
                parse_path_loadactor_args_reference(raw, &aliases),
                "arguments {raw:?}"
            );
        }
        assert_eq!(
            parse_path_loadactor_args("path, \"Left\"", &aliases),
            Some((ITG_ARG0_TOKEN.to_string(), Some("\"Left\"".to_string())))
        );
    }

    #[test]
    fn borrowed_enclosing_loadactor_arguments_preserve_wrappers() {
        let cases = [
            "LoadActor(\"wrapper.png\", NOTESKIN:LoadActor(\"Left\", \"Tap Note\"))",
            "LoadActor(path, NOTESKIN:LoadActor('Down', 'Receptor')) .. {}",
            "LoadActor(helper(1, 2), NOTESKIN:LoadActor(Var \"Button\", \"Tap Mine\"))",
            "LoadActor(NOTESKIN:LoadActor(\"Left\", \"Tap Note\"))",
            "Actor:LoadActor(\"wrapper.png\", NOTESKIN:LoadActor(\"Left\", \"Tap Note\"))",
            "NOTESKIN:LoadActor(\"Left\", \"Tap Note\")",
        ];

        for content in cases {
            let call_start = content
                .find("NOTESKIN:LoadActor(")
                .expect("case contains NOTESKIN call");
            let open = call_start + "NOTESKIN:LoadActor".len();
            let close = find_matching(content, open, '(', ')').expect("balanced NOTESKIN call");
            let current = find_enclosing_loadactor_for_noteskin(content, call_start, close);
            let reference =
                find_enclosing_loadactor_for_noteskin_reference(content, call_start, close)
                    .and_then(|(args, outer_close)| {
                        (args.len() >= 2).then(|| (args[0].clone(), outer_close))
                    });

            assert_eq!(
                current.map(|(wrapper, outer_close)| (wrapper.to_string(), outer_close)),
                reference,
                "content {content:?}"
            );
        }

        let content = cases[0];
        let call_start = content.find("NOTESKIN:LoadActor(").unwrap();
        let open = call_start + "NOTESKIN:LoadActor".len();
        let close = find_matching(content, open, '(', ')').unwrap();
        assert_eq!(
            find_enclosing_loadactor_for_noteskin(content, call_start, close)
                .map(|(wrapper, _)| wrapper),
            Some("\"wrapper.png\"")
        );
    }

    #[test]
    fn borrowed_local_function_params_preserve_identifiers() {
        let cases = [
            "self",
            "self, color, amount",
            " first , second_2 , third3 ",
            "self, invalid-name, valid",
            "self, helper(1, 2), tail",
            "self, 'quoted', tail",
            ", self, , value,",
            "",
        ];

        for raw in cases {
            assert_eq!(
                parse_local_function_params(raw),
                parse_local_function_params_reference(raw),
                "parameters {raw:?}"
            );
        }
        assert_eq!(
            parse_local_function_params("self, color, amount"),
            ["self", "color", "amount"]
        );
    }

    #[test]
    fn pump_receptor_conditions_follow_button_and_steps_type() {
        let center_condition = "Var \"Button\" == \"Center\" and \
            GAMESTATE:GetCurrentStyle():GetStepsType() ~= 'StepsType_Pump_Halfdouble'";
        let halfdouble_condition = "Var \"Button\" == \"DownLeft\" and \
            GAMESTATE:GetCurrentStyle():GetStepsType() == 'StepsType_Pump_Halfdouble'";

        assert!(actor_condition_matches(
            center_condition,
            "Center",
            "StepsType_Pump_Single"
        ));
        assert!(!actor_condition_matches(
            center_condition,
            "UpLeft",
            "StepsType_Pump_Single"
        ));
        assert!(!actor_condition_matches(
            halfdouble_condition,
            "DownLeft",
            "StepsType_Pump_Single"
        ));
    }

    #[test]
    fn actor_refs_preserve_condition_expression() {
        let content = r#"
return Def.ActorFrame {
    NOTESKIN:LoadActor("Center", "Outline Receptor")..{
        Condition=Var "Button" == "Center" and GAMESTATE:GetCurrentStyle():GetStepsType() ~= 'StepsType_Pump_Halfdouble';
    };
}
"#;

        let decl = parse_actor_decl(content, &noteskin_itg::IniData::default());

        assert_eq!(decl.refs.len(), 1);
        assert_eq!(
            decl.refs[0].condition_expr.as_deref(),
            Some(
                "Var \"Button\" == \"Center\" and GAMESTATE:GetCurrentStyle():GetStepsType() ~= 'StepsType_Pump_Halfdouble'"
            )
        );
    }

    #[test]
    fn actor_parser_ignores_commented_receptor_layers() {
        let content = r#"
return Def.ActorFrame {
    NOTESKIN:LoadActor(Var "Button", "Ready Receptor")..{
        Name="Base";
        Frames={{ Frame = 0 }};
    };
    --[[
    NOTESKIN:LoadActor(Var "Button", "Ready Receptor")..{
        Name="Tap";
        Frames={{ Frame = 2 }};
    };
    ]]
    -- NOTESKIN:LoadActor(Var "Button", "Ready Receptor")..{ Frames={{ Frame = 2 }}; };
    NOTESKIN:LoadActor(Var "Button", "Ready Receptor")..{
        Name="Glow";
        Frames={{ Frame = 1 }};
    };
}
"#;

        let decl = parse_actor_decl(content, &noteskin_itg::IniData::default());

        assert_eq!(
            decl.refs
                .iter()
                .map(|reference| reference.frame_override)
                .collect::<Vec<_>>(),
            [Some(0), Some(1)]
        );
    }

    #[test]
    fn allocation_free_update_marker_scan_matches_committed_behavior() {
        let cases = [
            ("InitCommand=cmd(SetUpdateFunction,Beat)", true),
            ("cmd(SET\u{2003}UPDATE\nFUNCTION , Pulse)", true),
            ("prefix setupdatefunction, suffix", true),
            ("SetUpdateFunction", false),
            ("SetUpdaterFunction,Beat", false),
            ("", false),
        ];

        for (content, expected) in cases {
            assert_eq!(
                itg_has_beat_update_marker_for_bench(content),
                expected,
                "case: {content:?}"
            );
            assert_eq!(
                itg_has_beat_update_marker_for_bench(content),
                itg_has_beat_update_marker_reference_for_bench(content),
                "case: {content:?}"
            );
        }
    }

    #[test]
    fn allocation_free_update_function_keys_match_committed_behavior() {
        let cases = [
            ("  Beat);", Some("beat")),
            ("\u{2003}Pu lSe_2,", Some("pulse_2")),
            ("9TICK ", Some("9tick")),
            ("_Update\n)", Some("_update")),
            ("\u{00c9}lan", None),
            (" --", None),
        ];

        for (content, expected) in cases {
            let expected = expected.map(identifier_checksum);
            assert_eq!(
                itg_update_function_name_for_bench(content),
                expected,
                "case: {content:?}"
            );
            let committed_input = content
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
                .to_ascii_lowercase();
            assert_eq!(
                itg_update_function_name_for_bench(&committed_input),
                itg_update_function_name_reference_for_bench(&committed_input),
                "case: {content:?}"
            );
        }
    }

    #[test]
    fn allocation_free_beat_fade_signature_matches_committed_behavior() {
        let cases = [
            (
                "part=beat%1; part=clamp(part,0,0.5); \
                 eff=scale(part,0,0.5,1,0); this.Glow:diffusealpha(eff)",
                true,
            ),
            (
                "PART = BEAT % 1\u{2003}PART = CLAMP(PART, 0, 0.5)\n\
                 EFF = SCALE(PART, 0, 0.5, 1, 0)\n\
                 THIS.GLOW : DIFFUSEALPHA(EFF)",
                true,
            ),
            (
                "part=beat%1; part=clamp(part,0,0.5); \
                 eff=scale(part,0,0.5,1,0)",
                false,
            ),
            (
                "part=beat%1; part=clamp(part,0,1); \
                 eff=scale(part,0,0.5,1,0); this.Glow:diffusealpha(eff)",
                false,
            ),
            ("", false),
        ];

        for (body, expected) in cases {
            assert_eq!(
                itg_has_beat_fade_glow_signature_for_bench(body),
                expected,
                "case: {body:?}"
            );
            assert_eq!(
                itg_has_beat_fade_glow_signature_for_bench(body),
                itg_has_beat_fade_glow_signature_reference_for_bench(body),
                "case: {body:?}"
            );
        }
    }

    #[test]
    fn actor_parser_marks_pump_beat_fade_glow_update() {
        let content = r#"
local function BeAt(self)
    local this = self:GetChildren()
    local beat = GAMESTATE:GetSongPosition():GetSongBeat()
    local part = beat%1
    part = clamp(part,0,0.5)
    local eff = scale(part,0,0.5,1,0)
    this.Glow:diffusealpha(eff)
end

return Def.ActorFrame {
    InitCommand=cmd(SetUpdateFunction , BeAt);
    NOTESKIN:LoadActor(Var "Button", "Ready Receptor")..{
        Name="Base";
        Frames={{ Frame = 0 }};
    };
    NOTESKIN:LoadActor(Var "Button", "Ready Receptor")..{
        Name="Glow";
        Frames={{ Frame = 1 }};
        InitCommand=cmd(blend,'BlendMode_Add');
    };
}
"#;

        let decl = parse_actor_decl(content, &noteskin_itg::IniData::default());
        let stripped = strip_lua_comments(content);
        let context = command_context(&stripped);

        assert!(has_beat_fade_glow_update(&stripped, &context));
        assert_eq!(
            has_beat_fade_glow_update(&stripped, &context),
            has_beat_fade_glow_update_reference(&stripped, &context)
        );

        assert_eq!(decl.refs.len(), 2);
        for reference in &decl.refs {
            assert_eq!(
                reference
                    .commands
                    .get(ITG_ACTOR_UPDATE_COMMAND)
                    .map(String::as_str),
                Some(ITG_BEAT_FADE_GLOW_UPDATE)
            );
        }
    }

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
    fn borrowed_actor_color_wrapper_scan_matches_lowercase_behavior() {
        for value in [
            "Color(\"#ff0080\")",
            " cOlOr( '#00ff0080' ) ",
            "\"#102030\"",
            "0.75, 0.5, 0.25, 1",
            "Color(1, 0.5, 0.25, 0.75)",
            "Colorful(#ffffff)",
            "nÃ¸color(#ffffff)",
            "",
        ] {
            assert_eq!(
                parse_lua_color_expr(value),
                parse_lua_color_expr_reference(value),
                "value {value:?}"
            );
        }
    }

    #[test]
    fn mixed_case_actor_keys_helpers_and_arguments_retain_behavior() {
        let content = r##"
local Accent = color("#ff0000")
local function Pulse(Tint, Scale)
    return function(self)
        self:diffuse(Tint):zoom(Scale)
    end
end

return Def.Sprite {
    Texture = "arrow.png",
    FrAmE0 = 3,
    fRaMe1 = 5,
    DeLaY0 = 0.25,
    dElAy1 = 0.75,
    InItCoMmAnD = PuLsE(ACCENT, 1.25),
    PrEsScOmMaNd = function(self)
        self:diffuse(Accent):zoom(0.9)
    end,
    UnrelatedValue = "ignored",
}

Def.Model {
    Meshes = "arrow.txt",
    FrAmE0000 = 7,
    OnCommand = PuLsE("ACCENT", 1.5),
}
"##;

        let decl = parse_actor_decl(content, &noteskin_itg::IniData::default());

        assert_eq!(decl.sprites.len(), 1);
        let sprite = &decl.sprites[0];
        assert_eq!(sprite.texture_expr, "\"arrow.png\"");
        assert_eq!(sprite.frame0, 3);
        assert_eq!(sprite.frame_count, 2);
        assert_eq!(sprite.frame_indices.as_deref(), Some([3, 5].as_slice()));
        assert_eq!(
            sprite.frame_delays.as_deref(),
            Some([0.25, 0.75].as_slice())
        );
        assert_eq!(
            sprite.commands.get("initcommand").map(String::as_str),
            Some("diffuse,1,0,0,1;zoom,1.25")
        );
        assert_eq!(
            sprite.commands.get("presscommand").map(String::as_str),
            Some("diffuse,1,0,0,1;zoom,0.9")
        );

        assert_eq!(decl.models.len(), 1);
        let model = &decl.models[0];
        assert_eq!(model.frame0, 7);
        assert_eq!(
            model.commands.get("oncommand").map(String::as_str),
            Some("diffuse,1,0,0,1;zoom,1.5")
        );
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
}
