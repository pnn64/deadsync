mod actor {
    use super::super::itg as noteskin_itg;
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
        if content.as_bytes().get(after).is_none_or(|ch| *ch != b'{') {
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
    ) -> Option<ItgLuaSpriteDecl> {
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
                frame_delays = linear_delays.into_iter().enumerate().collect();
                continue;
            }
            let key_lower = key.to_ascii_lowercase();
            if key_lower.starts_with("frame")
                && key_lower[5..].chars().all(|ch| ch.is_ascii_digit())
            {
                if let Ok(parsed) = value.parse::<usize>() {
                    frame_seen = true;
                    frame_max = frame_max.max(parsed);
                    if key_lower == "frame0000" {
                        frame0 = parsed;
                    }
                }
                continue;
            }
            if key_lower.starts_with("delay")
                && key_lower[5..].chars().all(|ch| ch.is_ascii_digit())
            {
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

    fn parse_commands_block(
        block: &str,
        metrics: &noteskin_itg::IniData,
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
}

mod compiled {
    use super::actor as noteskin_actor;
    use crate::config::dirs;
    use bincode::{Decode, Encode};
    use log::warn;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub const CACHE_SCHEMA_VERSION: u32 = 2;
    static CACHE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[derive(Debug, Clone, Encode, Decode, Default, PartialEq, Eq)]
    pub struct CompiledLoader {
        pub version: u32,
        pub game: String,
        pub skin: String,
        pub entries: Vec<CompiledLoaderEntry>,
    }

    #[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
    pub struct CompiledLoaderEntry {
        pub button: String,
        pub element: String,
        pub load_button: String,
        pub load_element: String,
        pub blank: bool,
        pub rotation_z: Option<i32>,
        pub init_command: Option<String>,
    }

    #[derive(Debug, Clone, Encode, Decode, Default)]
    pub struct CompiledActors {
        pub version: u32,
        pub files: Vec<CompiledActorFile>,
    }

    #[derive(Debug, Clone, Encode, Decode)]
    pub struct CompiledActorFile {
        pub key: String,
        pub decl: noteskin_actor::ItgLuaActorDecl,
    }

    #[derive(Debug, Clone, Encode, Decode, Default)]
    pub struct CompiledNoteskinBundle {
        pub version: u32,
        pub game: String,
        pub skin: String,
        pub source_hash: String,
        pub loader: CompiledLoader,
        pub actors: CompiledActors,
    }

    impl CompiledLoader {
        #[allow(dead_code)]
        pub fn find(&self, button: &str, element: &str) -> Option<&CompiledLoaderEntry> {
            self.entries.iter().find(|entry| {
                entry.button.eq_ignore_ascii_case(button)
                    && entry.element.eq_ignore_ascii_case(element)
            })
        }
    }

    impl CompiledActors {
        #[allow(dead_code)]
        pub fn find(&self, key: &str) -> Option<&CompiledActorFile> {
            self.files
                .iter()
                .find(|file| file.key.eq_ignore_ascii_case(key))
        }
    }

    pub fn compiled_bundle_path(game: &str, skin: &str, source_hash: &str) -> PathBuf {
        dirs::app_dirs()
            .noteskin_cache_dir()
            .join(game.trim().to_ascii_lowercase())
            .join(skin.trim().to_ascii_lowercase())
            .join(format!("{source_hash}.bin"))
    }

    pub fn load_compiled_bundle(path: &Path) -> Option<CompiledNoteskinBundle> {
        let bytes = fs::read(path).ok()?;
        match bincode::decode_from_slice::<CompiledNoteskinBundle, _>(
            &bytes,
            bincode::config::standard(),
        ) {
            Ok((bundle, _)) if bundle.version == CACHE_SCHEMA_VERSION => Some(bundle),
            Ok((bundle, _)) => {
                warn!(
                    "unsupported compiled noteskin cache version {} in '{}'",
                    bundle.version,
                    path.display()
                );
                None
            }
            Err(err) => {
                warn!(
                    "failed to decode compiled noteskin cache '{}': {err}",
                    path.display()
                );
                None
            }
        }
    }

    pub fn save_compiled_bundle(
        path: &Path,
        bundle: &CompiledNoteskinBundle,
    ) -> Result<(), String> {
        let bytes = bincode::encode_to_vec(bundle, bincode::config::standard())
            .map_err(|err| format!("failed to encode compiled noteskin cache: {err}"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create '{}': {err}", parent.display()))?;
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid cache filename '{}'", path.display()))?;
        let tmp_path = parent.join(format!(
            "{file_name}.{}.{}.tmp",
            std::process::id(),
            CACHE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&tmp_path, bytes)
            .map_err(|err| format!("failed to write '{}': {err}", tmp_path.display()))?;
        if let Err(err) = fs::rename(&tmp_path, path) {
            if path.is_file() {
                let _ = fs::remove_file(&tmp_path);
                return Ok(());
            }
            let _ = fs::remove_file(&tmp_path);
            return Err(format!("failed to finalize '{}': {err}", path.display()));
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn actor_manifest_key(search_dirs: &[PathBuf], path: &Path) -> Option<String> {
        for dir in search_dirs {
            if !path.starts_with(dir) {
                continue;
            }
            return actor_manifest_key_for_dir(dir, path);
        }
        None
    }

    pub fn actor_manifest_key_for_dir(dir: &Path, path: &Path) -> Option<String> {
        let game = dir.parent()?.file_name()?.to_str()?;
        let skin = dir.file_name()?.to_str()?;
        let file = path.file_name()?.to_str()?;
        Some(format!("{game}/{skin}/{file}").to_ascii_lowercase())
    }
}

pub use self::actor::{
    ItgLuaActorDecl, ItgLuaModelDecl, ItgLuaPathRefDecl, ItgLuaRefDecl, ItgLuaSpriteDecl,
};
use self::compiled::{CompiledActorFile, CompiledLoaderEntry, CompiledNoteskinBundle};
pub use self::compiled::{CompiledActors, CompiledLoader, actor_manifest_key};
use self::{actor as noteskin_actor, compiled as noteskin_compiled};
use super::itg as noteskin_itg;
use log::info;
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use twox_hash::XxHash64;

const COMPILER_VERSION: u32 = 3;
static COMPILED_HASH_CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const DANCE_BUTTONS: [&str; 6] = ["UpLeft", "UpRight", "Left", "Down", "Up", "Right"];
const CORE_ELEMENTS: [&str; 33] = [
    "Explosion",
    "Go Receptor",
    "HitMine Explosion",
    "Hold Body Active",
    "Hold Body Inactive",
    "Hold BottomCap Active",
    "Hold BottomCap Inactive",
    "Hold Explosion",
    "Hold Head Active",
    "Hold Head Inactive",
    "Hold Tail Active",
    "Hold Tail Inactive",
    "Hold TopCap Active",
    "Hold TopCap Inactive",
    "Ready Receptor",
    "Receptor",
    "Roll Body Active",
    "Roll Body Inactive",
    "Roll BottomCap Active",
    "Roll BottomCap Inactive",
    "Roll Explosion",
    "Roll Head Active",
    "Roll Head Inactive",
    "Roll Tail Active",
    "Roll Tail Inactive",
    "Roll TopCap Active",
    "Roll TopCap Inactive",
    "Tap Explosion Bright",
    "Tap Explosion Dim",
    "Tap Fake",
    "Tap Lift",
    "Tap Mine",
    "Tap Note",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileOutcome {
    Reused,
    Built,
}

pub fn ensure_compiled(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<CompileOutcome, String> {
    if let Some(path) = cached_bundle_path(game, &data.name)
        && noteskin_compiled::load_compiled_bundle(&path).is_some()
    {
        return Ok(CompileOutcome::Reused);
    }
    let source_hash = source_hash(game, data)?;
    remember_source_hash(game, &data.name, &source_hash);
    let path = noteskin_compiled::compiled_bundle_path(game, &data.name, &source_hash);
    if noteskin_compiled::load_compiled_bundle(&path).is_some() {
        return Ok(CompileOutcome::Reused);
    }
    info!("compiling noteskin cache for '{game}/{}'", data.name);
    let bundle = compile_data(game, data, &source_hash)?;
    noteskin_compiled::save_compiled_bundle(&path, &bundle)?;
    Ok(CompileOutcome::Built)
}

#[allow(dead_code)]
pub fn load_compiled(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Option<CompiledNoteskinBundle> {
    let path = cached_bundle_path(game, &data.name)?;
    noteskin_compiled::load_compiled_bundle(&path)
}

fn cached_bundle_path(game: &str, skin: &str) -> Option<PathBuf> {
    let key = compiled_hash_cache_key(game, skin);
    let hash = COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()?;
    Some(noteskin_compiled::compiled_bundle_path(game, skin, &hash))
}

fn remember_source_hash(game: &str, skin: &str, source_hash: &str) {
    let key = compiled_hash_cache_key(game, skin);
    COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, source_hash.to_string());
}

fn compiled_hash_cache_key(game: &str, skin: &str) -> String {
    format!(
        "{}/{}",
        game.trim().to_ascii_lowercase(),
        skin.trim().to_ascii_lowercase()
    )
}

fn source_hash(game: &str, data: &noteskin_itg::NoteskinData) -> Result<String, String> {
    let mut paths = source_paths(data);
    paths.sort_by_key(|left| source_label(data, left));
    let mut hasher = XxHash64::default();
    hasher.write_u32(noteskin_compiled::CACHE_SCHEMA_VERSION);
    hasher.write_u32(COMPILER_VERSION);
    hasher.write(game.as_bytes());
    hasher.write(data.name.as_bytes());
    for path in paths {
        let label = source_label(data, &path);
        hasher.write(label.as_bytes());
        let bytes = fs::read(&path)
            .map_err(|err| format!("failed to read '{}' for hashing: {err}", path.display()))?;
        hasher.write(&bytes);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

fn source_paths(data: &noteskin_itg::NoteskinData) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in &data.search_dirs {
        for name in ["metrics.ini", "NoteSkin.lua"] {
            let path = dir.join(name);
            if path.is_file() {
                out.push(path);
            }
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_actor_lua = path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
                && path.file_name().is_none_or(|name| name != "NoteSkin.lua");
            if is_actor_lua {
                out.push(path);
            }
        }
    }
    out
}

fn source_label(data: &noteskin_itg::NoteskinData, path: &Path) -> String {
    for dir in &data.search_dirs {
        if !path.starts_with(dir) {
            continue;
        }
        let game = dir
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let skin = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let rel = path
            .strip_prefix(dir)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));
        return format!(
            "{}/{}/{}",
            game.to_ascii_lowercase(),
            skin.to_ascii_lowercase(),
            rel.to_ascii_lowercase()
        );
    }
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn compile_data(
    game: &str,
    data: &noteskin_itg::NoteskinData,
    source_hash: &str,
) -> Result<CompiledNoteskinBundle, String> {
    let scripts = noteskin_paths(data);
    let lua = Lua::new();
    install_host(&lua).map_err(|err| err.to_string())?;
    let noteskin = load_noteskin_table(&lua, &scripts)?;
    Ok(CompiledNoteskinBundle {
        version: noteskin_compiled::CACHE_SCHEMA_VERSION,
        game: game.to_string(),
        skin: data.name.clone(),
        source_hash: source_hash.to_string(),
        loader: CompiledLoader {
            version: COMPILER_VERSION,
            game: game.to_string(),
            skin: data.name.clone(),
            entries: compile_entries(&lua, &noteskin, data)?,
        },
        actors: CompiledActors {
            version: COMPILER_VERSION,
            files: compile_actor_files(data)?,
        },
    })
}

fn noteskin_paths(data: &noteskin_itg::NoteskinData) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in data.search_dirs.iter().rev() {
        let path = dir.join("NoteSkin.lua");
        if path.is_file() {
            out.push(path);
        }
    }
    out
}

fn compile_actor_files(
    data: &noteskin_itg::NoteskinData,
) -> Result<Vec<CompiledActorFile>, String> {
    let mut out = Vec::new();
    for dir in &data.search_dirs {
        let entries = fs::read_dir(dir)
            .map_err(|err| format!("failed to read '{}': {err}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_lua = path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
                && path.file_name().is_none_or(|name| name != "NoteSkin.lua");
            if !is_lua {
                continue;
            }
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
            let Some(key) = noteskin_compiled::actor_manifest_key_for_dir(dir, &path) else {
                continue;
            };
            out.push(CompiledActorFile {
                key,
                decl: noteskin_actor::parse_actor_decl(&content, &data.metrics),
            });
        }
    }
    out.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(out)
}

fn install_host(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let actor_mt = lua.create_table()?;
    let actor_methods = lua.create_table()?;
    for name in [
        "x",
        "y",
        "z",
        "addx",
        "addy",
        "addz",
        "rotationx",
        "rotationy",
        "rotationz",
        "addrotationx",
        "addrotationy",
        "addrotationz",
        "zoom",
        "zoomx",
        "zoomy",
        "zoomz",
        "diffuse",
        "diffusealpha",
        "glow",
        "vertalign",
        "valign",
        "blend",
        "visible",
    ] {
        let command = name.to_string();
        actor_methods.set(
            name,
            lua.create_function(move |lua, (actor, args): (Table, MultiValue)| {
                append_actor_command(lua, &actor, &command, args)?;
                Ok(actor)
            })?,
        )?;
    }
    actor_mt.set("__index", actor_methods)?;
    actor_mt.set(
        "__concat",
        lua.create_function(|_, (lhs, _rhs): (Table, Value)| Ok(lhs))?,
    )?;
    let make_actor = {
        let actor_mt = actor_mt.clone();
        lua.create_function(
            move |lua, (blank, button, element): (bool, Option<String>, Option<String>)| {
                let actor = lua.create_table()?;
                actor.set("__blank", blank)?;
                if let Some(button) = button {
                    actor.set("__load_button", button)?;
                }
                if let Some(element) = element {
                    actor.set("__load_element", element)?;
                }
                let _ = actor.set_metatable(Some(actor_mt.clone()));
                Ok(actor)
            },
        )?
    };
    let load_actor = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, value: Value| -> mlua::Result<Table> {
            match value {
                Value::String(text) => {
                    let text = text.to_str()?.to_string();
                    make_actor.call((
                        text.eq_ignore_ascii_case("_blank"),
                        None::<String>,
                        Some(text),
                    ))
                }
                Value::Table(path) => {
                    let button = path.get::<Option<String>>("load_button")?;
                    let element = path.get::<Option<String>>("load_element")?;
                    let blank = element
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("_blank"));
                    make_actor.call((blank, button, element))
                }
                _ => make_actor.call((false, None::<String>, None::<String>)),
            }
        })?
    };
    globals.set("LoadActor", load_actor)?;
    let var_fn = lua.create_function(|lua, name: String| {
        let globals = lua.globals();
        match name.as_str() {
            "Button" => Ok(Value::String(
                lua.create_string(&globals.get::<String>("__itg_button")?)?,
            )),
            "Element" => Ok(Value::String(
                lua.create_string(&globals.get::<String>("__itg_element")?)?,
            )),
            "SpriteOnly" => Ok(Value::Boolean(
                globals.get::<bool>("__itg_sprite_only").unwrap_or(false),
            )),
            _ => Ok(Value::Nil),
        }
    })?;
    globals.set("Var", var_fn)?;
    globals.set(
        "cmd",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    let noteskin = lua.create_table()?;
    noteskin.set(
        "GetPath",
        lua.create_function(|lua, (_self, button, element): (Table, String, String)| {
            let path = lua.create_table()?;
            path.set("load_button", button)?;
            path.set("load_element", element)?;
            Ok(path)
        })?,
    )?;
    globals.set("NOTESKIN", noteskin)?;
    let def = lua.create_table()?;
    let actor_fn = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, _value: Value| -> mlua::Result<Table> {
            make_actor.call((true, None::<String>, None::<String>))
        })?
    };
    def.set("Actor", actor_fn)?;
    globals.set("Def", def)?;
    Ok(())
}

fn append_actor_command(
    lua: &Lua,
    actor: &Table,
    command: &str,
    args: MultiValue,
) -> mlua::Result<()> {
    let commands = actor
        .get::<Option<Table>>("__loader_commands")?
        .unwrap_or(lua.create_table()?);
    let mut token = command.to_string();
    for arg in args {
        token.push(',');
        token.push_str(&lua_command_arg(arg)?);
    }
    commands.raw_set(commands.raw_len() + 1, token)?;
    actor.set("__loader_commands", commands)
}

fn lua_command_arg(value: Value) -> mlua::Result<String> {
    Ok(match value {
        Value::Nil => String::new(),
        Value::Boolean(v) => v.to_string(),
        Value::Integer(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.to_str()?.to_string(),
        _ => String::new(),
    })
}

fn load_noteskin_table(lua: &Lua, paths: &[PathBuf]) -> Result<Table, String> {
    let mut current = None;
    for path in paths {
        let content = fs::read_to_string(path)
            .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
        let chunk = lua.load(&content).set_name(path.to_string_lossy().as_ref());
        let function = chunk
            .into_function()
            .map_err(|err| format!("failed to compile '{}': {err}", path.display()))?;
        let next = if let Some(value) = current.take() {
            function
                .call(value)
                .map_err(|err| format!("failed to execute '{}': {err}", path.display()))?
        } else {
            function
                .call(())
                .map_err(|err| format!("failed to execute '{}': {err}", path.display()))?
        };
        current = Some(next);
    }
    current.ok_or_else(|| "no NoteSkin.lua files were found in fallback chain".to_string())
}

fn compile_entries(
    lua: &Lua,
    noteskin: &Table,
    data: &noteskin_itg::NoteskinData,
) -> Result<Vec<CompiledLoaderEntry>, String> {
    let (buttons, elements) = collect_loader_domain(data);
    normalize_noteskin_tables(noteskin, &buttons, &elements)
        .map_err(|err| format!("failed to normalize noteskin loader tables: {err}"))?;
    let load = noteskin
        .get::<Function>("Load")
        .map_err(|err| format!("compiled noteskin is missing Load(): {err}"))?;
    let globals = lua.globals();
    let mut out = Vec::with_capacity(buttons.len() * elements.len());
    for button in &buttons {
        for element in &elements {
            globals
                .set("__itg_button", button.as_str())
                .map_err(|err| err.to_string())?;
            globals
                .set("__itg_element", element.as_str())
                .map_err(|err| err.to_string())?;
            globals
                .set("__itg_sprite_only", true)
                .map_err(|err| err.to_string())?;
            let actor = load
                .call::<Table>(())
                .map_err(|err| format!("Load() failed for '{button} {element}': {err}"))?;
            out.push(read_entry(button, element, &actor)?);
        }
    }
    out.sort_by(|left, right| {
        (
            left.button.to_ascii_lowercase(),
            left.element.to_ascii_lowercase(),
        )
            .cmp(&(
                right.button.to_ascii_lowercase(),
                right.element.to_ascii_lowercase(),
            ))
    });
    Ok(out)
}

fn normalize_noteskin_tables(
    noteskin: &Table,
    buttons: &[String],
    elements: &[String],
) -> mlua::Result<()> {
    for key in ["RedirTable", "ButtonRedir", "ButtonRedirs", "Rotate"] {
        normalize_table_aliases(noteskin, key, buttons)?;
    }
    for key in [
        "ElementRedir",
        "ElementRedirs",
        "PartsToRotate",
        "Blank",
        "bBlanks",
    ] {
        normalize_table_aliases(noteskin, key, elements)?;
    }
    Ok(())
}

fn normalize_table_aliases(
    noteskin: &Table,
    table_key: &str,
    canonical_keys: &[String],
) -> mlua::Result<()> {
    let Some(table) = noteskin.get::<Option<Table>>(table_key)? else {
        return Ok(());
    };
    let mut existing = Vec::new();
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(text) = key else {
            continue;
        };
        let Ok(text) = text.to_str() else {
            continue;
        };
        existing.push((text.to_string(), value));
    }
    for want in canonical_keys {
        if table.contains_key(want.as_str())? {
            continue;
        }
        if let Some((_, value)) = existing
            .iter()
            .find(|(have, _)| have.eq_ignore_ascii_case(want))
        {
            table.set(want.as_str(), value.clone())?;
        }
    }
    Ok(())
}

fn collect_loader_domain(data: &noteskin_itg::NoteskinData) -> (Vec<String>, Vec<String>) {
    let mut buttons = Vec::new();
    let mut button_seen = HashSet::new();
    for button in ["Left", "Down", "Up", "Right"] {
        push_unique(&mut buttons, &mut button_seen, button);
    }
    let mut elements = Vec::new();
    let mut element_seen = HashSet::new();
    for element in CORE_ELEMENTS {
        push_unique(&mut elements, &mut element_seen, element);
    }
    for dir in &data.search_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let stem = trim_variant_suffix(name);
            let Some((button, element)) = split_prefixed_stem(stem) else {
                continue;
            };
            if let Some(button) = button {
                push_unique(&mut buttons, &mut button_seen, button);
            }
            push_unique(&mut elements, &mut element_seen, element);
        }
    }
    (buttons, elements)
}

fn push_unique(out: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let key = trimmed.to_ascii_lowercase();
    if seen.insert(key) {
        out.push(trimmed.to_string());
    }
}

fn trim_variant_suffix(name: &str) -> &str {
    let stem = name.rsplit_once('.').map_or(name, |(head, _)| head).trim();
    let no_paren = stem
        .rsplit_once(" (")
        .map_or(stem, |(head, _)| head)
        .trim_end();
    match no_paren.rsplit_once(' ') {
        Some((head, tail))
            if tail
                .split_once('x')
                .is_some_and(|(w, h)| digits_only(w) && digits_only(h)) =>
        {
            head.trim_end()
        }
        _ => no_paren,
    }
}

fn split_prefixed_stem(stem: &str) -> Option<(Option<&str>, &str)> {
    let trimmed = stem.trim();
    if let Some(rest) = trimmed.strip_prefix("Fallback ") {
        return Some((None, rest.trim()));
    }
    for button in DANCE_BUTTONS {
        let Some(rest) = trimmed.strip_prefix(button) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(' ') else {
            continue;
        };
        return Some((Some(button), rest.trim()));
    }
    None
}

fn digits_only(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn actor_loader_command(actor: &Table) -> Result<Option<String>, String> {
    let value = actor
        .get::<Value>("InitCommand")
        .map_err(|err| err.to_string())?;
    match value {
        Value::Function(f) => {
            f.call::<()>(actor.clone()).map_err(|err| err.to_string())?;
        }
        Value::String(s) => {
            let command = s.to_str().map_err(|err| err.to_string())?.to_string();
            if !command.trim().is_empty() {
                return Ok(Some(command));
            }
        }
        _ => {}
    }
    let Some(commands) = actor
        .get::<Option<Table>>("__loader_commands")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(commands.raw_len());
    for command in commands.sequence_values::<String>() {
        let command = command.map_err(|err| err.to_string())?;
        if !command.trim().is_empty() {
            out.push(command);
        }
    }
    Ok((!out.is_empty()).then(|| out.join(";")))
}

fn read_entry(button: &str, element: &str, actor: &Table) -> Result<CompiledLoaderEntry, String> {
    let blank = actor.get::<bool>("__blank").unwrap_or(false);
    let load_button = actor
        .get::<Option<String>>("__load_button")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| button.to_string());
    let load_element = actor
        .get::<Option<String>>("__load_element")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| element.to_string());
    let rotation_z = actor.get::<Option<i32>>("BaseRotationZ").unwrap_or(None);
    let init_command = actor_loader_command(actor)?;
    Ok(CompiledLoaderEntry {
        button: button.to_string(),
        element: element.to_string(),
        load_button,
        load_element,
        blank,
        rotation_z,
        init_command,
    })
}

#[cfg(test)]
mod tests {
    use super::compiled;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn compiled_bundle_path_omits_version_dir() {
        let path = compiled::compiled_bundle_path(" Dance ", " Default ", "hash123");
        let suffix = Path::new("noteskins")
            .join("dance")
            .join("default")
            .join("hash123.bin");
        let version_dir = format!("v{}", compiled::CACHE_SCHEMA_VERSION);
        assert!(path.ends_with(&suffix));
        assert!(
            path.components()
                .all(|component| component.as_os_str() != OsStr::new(&version_dir))
        );
    }
}
