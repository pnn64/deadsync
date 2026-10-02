// Frozen from parent 05256d0214d6c7ece0524999cd49e2831b069656 (0.5.1688).
// Shared scalar parsing and selector handlers remain unchanged.

#[inline]
fn old_find_attack_time(raw: &[u8], start: usize) -> Option<usize> {
    const TIME: &[u8; 5] = b"TIME=";
    raw.get(start..)?
        .windows(TIME.len())
        .position(|window| window.eq_ignore_ascii_case(TIME))
        .map(|offset| start + offset)
}

#[inline(always)]
fn old_parse_attack_scroll_override(token: &str) -> Option<ScrollSpeedSetting> {
    let trimmed = token.trim();
    let value = trimmed
        .strip_suffix('x')
        .or_else(|| trimmed.strip_suffix('X'))
        .and_then(|v| v.trim().parse::<f32>().ok());
    if let Some(v) = value.filter(|v| v.is_finite() && *v > 0.0) {
        return Some(ScrollSpeedSetting::XMod(v));
    }
    let (&kind, value) = trimmed.as_bytes().split_first()?;
    let value = std::str::from_utf8(value)
        .ok()?
        .trim()
        .parse::<f32>()
        .ok()?;
    if value <= 0.0 {
        return None;
    }
    match kind.to_ascii_lowercase() {
        b'c' => Some(ScrollSpeedSetting::CMod(value)),
        b'x' => Some(ScrollSpeedSetting::XMod(value)),
        b'm' => Some(ScrollSpeedSetting::MMod(value)),
        _ => None,
    }
}

fn old_parse_attack_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    let mut key_buffer = [0u8; ATTACK_KEY_STACK_BYTES];
    for token in mods.split(',') {
        let (approach_speed, token) = parse_attack_approach_prefix(token);
        if token.is_empty() {
            continue;
        }
        if let Some(scroll_speed) = old_parse_attack_scroll_override(token) {
            out.scroll_speed = Some(scroll_speed);
            continue;
        }
        let (percent_value, token_key) = parse_attack_level_token(token);
        let key = buffered_attack_token_key(token_key, &mut key_buffer);
        let key = key.as_str();
        if key.is_empty() {
            continue;
        }
        match key {
            "clearall" => {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
            }
            _ => apply_runtime_mod(&mut out, key, percent_value, approach_speed),
        }
    }
    out
}

fn old_parse_song_lua_runtime_mods_core<const BUFFERED: bool>(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    let mut key_buffer = [0u8; ATTACK_KEY_STACK_BYTES];
    for token in mods.split(',') {
        let mut parts = token.trim().split_ascii_whitespace();
        let Some(first) = parts.next() else {
            continue;
        };
        let Some(second) = parts.next() else {
            if let Some(scroll_speed) = old_parse_attack_scroll_override(first) {
                out.scroll_speed = Some(scroll_speed);
                continue;
            }
            let key = song_lua_runtime_attack_key::<BUFFERED>(first, &mut key_buffer);
            let key = key.as_str();
            if key.is_empty() {
                continue;
            }
            if key == "clearall" {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
                continue;
            }
            apply_runtime_mod(&mut out, key, Some(100.0), 1.0);
            continue;
        };

        if first.starts_with('*') {
            let approach_speed = parse_attack_approach_prefix(first).0;
            let Some(third) = parts.next() else {
                if let Some(scroll_speed) = old_parse_attack_scroll_override(second) {
                    out.scroll_speed = Some(scroll_speed);
                    continue;
                }
                let key = song_lua_runtime_attack_key::<BUFFERED>(second, &mut key_buffer);
                let key = key.as_str();
                if !key.is_empty() {
                    apply_runtime_mod(&mut out, key, Some(100.0), approach_speed);
                }
                continue;
            };
            let key = song_lua_runtime_attack_key::<BUFFERED>(third, &mut key_buffer);
            let key = key.as_str();
            if key.is_empty() {
                continue;
            }
            let amount = parse_song_lua_mod_amount(second).unwrap_or(0.0);
            apply_runtime_mod(&mut out, key, Some(amount), approach_speed);
            continue;
        }

        let key = song_lua_runtime_attack_key::<BUFFERED>(second, &mut key_buffer);
        let key = key.as_str();
        if key.is_empty() {
            continue;
        }
        let amount = parse_song_lua_mod_amount(first).unwrap_or(0.0);
        apply_runtime_mod(&mut out, key, Some(amount), 1.0);
    }
    out
}
