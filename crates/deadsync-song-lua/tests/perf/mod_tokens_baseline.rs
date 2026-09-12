// Frozen from f2d8fea821cefb4aa7655302b8e4da2430ca16ff; only test visibility differs.

use super::*;

pub fn normalize_player_option_key(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

pub(super) fn apply_player_options_string(
    lua: &Lua,
    owner: &Table,
    text: &str,
) -> mlua::Result<()> {
    for option in text.split(',') {
        apply_player_option_token(lua, owner, option)?;
    }
    Ok(())
}

pub(super) fn apply_player_option_token(lua: &Lua, owner: &Table, raw: &str) -> mlua::Result<()> {
    let text = strip_player_option_prefix(raw);
    let speed = raw
        .trim_start()
        .strip_prefix('*')
        .and_then(|prefix| split_first_word(prefix).0.parse::<f32>().ok())
        .unwrap_or(1.0)
        .max(0.0);
    if text.is_empty() || apply_player_speed_option(owner, text)? {
        return Ok(());
    }

    let (head, tail) = split_first_word(text);
    let (amount, name) = if head.eq_ignore_ascii_case("inf") && !tail.is_empty() {
        // PlayerOptions::FromOneModString only recognizes levels beginning
        // with a digit or '-'; positive Lua infinity leaves the default 1.
        (Some(1.0), tail)
    } else if !tail.is_empty() {
        parse_player_option_amount(head).map_or((None, text), |amount| (Some(amount), tail))
    } else {
        (None, text)
    };
    let key = normalize_player_option_key(name);
    if key.is_empty() {
        return Ok(());
    }

    let state = player_option_state(lua, owner)?;
    let value = if player_option_uses_bool(key.as_str()) {
        Value::Boolean(amount != Some(0.0))
    } else {
        Value::Number(f64::from(amount.unwrap_or(1.0)))
    };
    state.set(key.as_str(), value)?;
    player_option_speeds(lua, owner)?.set(key.as_str(), speed)
}
