//! Shared font identity and bundled Simply Love font aliases. No Lua handles.
use crate::SONG_LUA_THEME_PATH_PREFIX;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

pub(crate) fn song_lua_builtin_theme_font(raw: &str) -> Option<(&'static str, &'static str)> {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.replace('\\', "/");
    let name = if let Some(path) = normalized.strip_prefix(SONG_LUA_THEME_PATH_PREFIX) {
        path.strip_prefix("F/").unwrap_or(path)
    } else if !normalized.contains('/') && Path::new(&normalized).extension().is_none() {
        normalized.as_str()
    } else {
        return None;
    };
    // BitmapText resolves named fonts through ThemeManager. Simply Love's
    // Common Header.redir selects Wendy/_wendy small; Common Normal.redir
    // selects Miso/_miso light. GetPathF's group and element use a slash in
    // our virtual paths, while the native theme filenames use a space.
    if name.eq_ignore_ascii_case("Common Header")
        || name.eq_ignore_ascii_case("Common/Header")
        || name.eq_ignore_ascii_case("Wendy/_wendy small")
    {
        Some(("wendy", "assets/fonts/wendy/_wendy small.ini"))
    } else {
        Some(("miso", "assets/fonts/miso/_miso light.ini"))
    }
}

pub(crate) fn song_lua_font_name(font_path: &Path) -> &'static str {
    static SONG_LUA_FONT_NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let canonical = font_path.to_string_lossy().replace('\\', "/");
    let cache = SONG_LUA_FONT_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&name) = guard.get(&canonical) {
        return name;
    }
    let leaked = Box::leak(format!("songlua_font:{canonical}").into_boxed_str());
    guard.insert(canonical, leaked);
    leaked
}
