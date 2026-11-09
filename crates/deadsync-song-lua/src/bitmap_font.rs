//! Shared font identity and existing theme fallback policy. No Lua handles.
use crate::SONG_LUA_THEME_PATH_PREFIX;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

pub(crate) fn song_lua_uses_builtin_theme_font(raw: &str) -> bool {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.replace('\\', "/");
    normalized.starts_with(SONG_LUA_THEME_PATH_PREFIX)
        || (!normalized.contains('/') && Path::new(&normalized).extension().is_none())
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
