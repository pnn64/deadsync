// Frozen from parent 05256d0214d6c7ece0524999cd49e2831b069656 (0.5.1688).
// Shared scalar parsing and selector handlers remain unchanged.

#[inline]
fn old_with_ascii_lowercase_overlay_key<T>(raw: &str, use_key: impl FnOnce(&str) -> T) -> T {
    if raw.len() <= OVERLAY_PARSE_KEY_STACK_BYTES {
        let mut key = [0u8; OVERLAY_PARSE_KEY_STACK_BYTES];
        key[..raw.len()].copy_from_slice(raw.as_bytes());
        key[..raw.len()].make_ascii_lowercase();
        let key = std::str::from_utf8(&key[..raw.len()])
            .expect("ASCII case folding preserves valid UTF-8");
        use_key(key)
    } else {
        let key = raw.to_ascii_lowercase();
        use_key(&key)
    }
}

fn old_parse_overlay_effect_mode(raw: &str) -> Option<EffectMode> {
    old_with_ascii_lowercase_overlay_key(raw.trim(), parse_overlay_effect_mode_normalized)
}

fn old_parse_overlay_effect_clock(raw: &str) -> Option<EffectClock> {
    let raw = raw.trim().trim_matches('"').trim_matches('\'');
    old_with_ascii_lowercase_overlay_key(raw, parse_overlay_effect_clock_normalized)
}

fn old_parse_overlay_text_align(raw: &str) -> Option<TextAlign> {
    let raw = raw.trim().trim_matches('"').trim_matches('\'');
    old_with_ascii_lowercase_overlay_key(raw, parse_overlay_text_align_normalized)
}

fn old_parse_overlay_text_glow_mode(raw: &str) -> Option<SongLuaTextGlowMode> {
    let raw = raw.trim().trim_matches('"').trim_matches('\'');
    old_with_ascii_lowercase_overlay_key(raw, parse_overlay_text_glow_mode_normalized)
}

fn old_theme_pref_default(lua: &mlua::Lua, name: &str) -> mlua::Result<mlua::Value> {
    old_with_ascii_lowercase_overlay_key(name, |key| theme_pref_default_normalized(lua, key))
}

fn old_theme_screen_fallback(group: &str) -> Option<&'static str> {
    old_with_ascii_lowercase_overlay_key(group, theme_screen_fallback_normalized)
}
