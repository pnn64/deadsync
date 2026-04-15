use crate::config;
use ini::Ini;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

// ---------------------------------------------------------------------------
// LKey — lazy localization key (const-compatible)
// ---------------------------------------------------------------------------

/// A reference to a localized string that resolves at render time via `tr()`.
///
/// `LookupKey` is `Copy` and can live in `const` static arrays. Call `.get()` to
/// resolve to the current language's text. If the key is missing, falls back
/// to English, then to `"Section.Key"`.
#[derive(Clone, Copy)]
pub struct LookupKey {
    pub section: &'static str,
    pub key: &'static str,
}

impl LookupKey {
    /// Resolve this key to the localized string for the current language.
    pub fn get(&self) -> Arc<str> {
        tr(self.section, self.key)
    }
}

/// Shorthand for constructing a `LookupKey` in const contexts.
pub const fn lookup_key(section: &'static str, key: &'static str) -> LookupKey {
    LookupKey { section, key }
}

// ---------------------------------------------------------------------------
// LangData
// ---------------------------------------------------------------------------

/// Loaded language data: active locale strings + English fallback.
struct LangData {
    /// Active (non-English) language strings: section → (key → value).
    /// Empty when the active language is English.
    active: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>>,
    /// English fallback strings (always loaded).
    fallback: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>>,
    /// Current locale code (e.g. "en", "es", "ja").
    locale: String,
}

static LANG: OnceLock<RwLock<LangData>> = OnceLock::new();

// ---------------------------------------------------------------------------
// INI loading
// ---------------------------------------------------------------------------

fn load_ini_to_map(path: &Path) -> HashMap<Box<str>, HashMap<Box<str>, Arc<str>>> {
    let ini = match Ini::load_from_file(path) {
        Ok(ini) => ini,
        Err(e) => {
            log::warn!("Failed to load language file {}: {e}", path.display());
            return HashMap::new();
        }
    };
    let mut sections: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>> = HashMap::new();
    for (section, props) in &ini {
        let section_name: Box<str> = section.unwrap_or("").into();
        let entries = sections.entry(section_name).or_default();
        for (key, value) in props.iter() {
            entries.insert(key.into(), Arc::from(value));
        }
    }
    sections
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// Initialize the i18n system. Call once at startup after config is loaded.
///
/// `locale` is the resolved locale code (e.g. `"en"`, `"es"`). If the locale
/// is `"en"`, only the fallback map is populated and the active map stays empty.
pub fn init(locale: &str) {
    let dirs = config::dirs::app_dirs();
    let fallback = load_ini_to_map(&dirs.resolve_asset_path("assets/languages/en.ini"));
    let active = if locale != "en" {
        let path = dirs.resolve_asset_path(&format!("assets/languages/{locale}.ini"));
        load_ini_to_map(&path)
    } else {
        HashMap::new()
    };
    let data = LangData {
        active,
        fallback,
        locale: locale.to_string(),
    };
    let _ = LANG.set(RwLock::new(data));
}

// ---------------------------------------------------------------------------
// Lookup API
// ---------------------------------------------------------------------------

/// Look up a localized string by section and key.
///
/// Falls back to English if the key is missing from the active language.
/// Returns `"Section.Key"` if the key is missing from English too (makes
/// untranslated strings visible during development).
pub fn tr(section: &str, key: &str) -> Arc<str> {
    let lang = LANG.get().expect("i18n not initialized").read().unwrap();

    // Try active language first.
    if let Some(section_map) = lang.active.get(section) {
        if let Some(val) = section_map.get(key) {
            return val.clone();
        }
    }
    // Fall back to English.
    if let Some(section_map) = lang.fallback.get(section) {
        if let Some(val) = section_map.get(key) {
            return val.clone();
        }
    }
    // Missing everywhere — return "Section.Key" so it's visible.
    Arc::from(format!("{section}.{key}"))
}

/// Look up a localized string with named placeholder substitution.
///
/// Each `{name}` in the translated string is replaced with the corresponding
/// value from `args`. Named placeholders allow translators to reorder
/// arguments freely (e.g. Japanese word order differs from English).
pub fn tr_fmt(section: &str, key: &str, args: &[(&str, &str)]) -> Arc<str> {
    let mut s = tr(section, key).to_string();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    Arc::from(s)
}

// ---------------------------------------------------------------------------
// Language switching (runtime)
// ---------------------------------------------------------------------------

/// Switch the active language at runtime. Called when the user changes the
/// Language option. Re-reads the new `.ini` file into the active map.
///
/// All subsequent `tr()` calls will return strings from the new language.
pub fn set_locale(locale: &str) {
    let mut lang = LANG.get().expect("i18n not initialized").write().unwrap();
    let dirs = config::dirs::app_dirs();
    lang.active = if locale != "en" {
        let path = dirs.resolve_asset_path(&format!("assets/languages/{locale}.ini"));
        load_ini_to_map(&path)
    } else {
        HashMap::new()
    };
    lang.locale = locale.to_string();
}

/// Returns the currently active locale code (e.g. `"en"`, `"es"`).
pub fn current_locale() -> String {
    LANG.get()
        .expect("i18n not initialized")
        .read()
        .unwrap()
        .locale
        .clone()
}

// ---------------------------------------------------------------------------
// OS language detection
// ---------------------------------------------------------------------------

/// Detect the best locale from the OS settings, falling back to `"en"` if
/// no matching language file is found.
pub fn detect_os_locale() -> String {
    let raw = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
    let code = normalize_locale(&raw);

    if locale_file_exists(&code) {
        return code;
    }
    // Try base language without region (e.g. "ja-JP" → "ja").
    if let Some(base) = code.split('-').next() {
        if base != code && locale_file_exists(base) {
            return base.to_string();
        }
    }
    "en".to_string()
}

/// Normalize an OS locale string to our file-naming convention.
///
/// - `"ja-JP"` → `"ja"`
/// - `"fr-FR"` → `"fr"`
/// - `"zh-TW"` / `"zh-HK"` → `"zh-Hant"`
/// - `"zh-CN"` / `"zh-SG"` → `"zh-Hans"`
fn normalize_locale(raw: &str) -> String {
    let lower = raw.replace('_', "-").to_ascii_lowercase();

    // Handle Chinese variants (match ITGmania convention).
    if lower.starts_with("zh") {
        if lower.contains("hant") || lower.contains("tw") || lower.contains("hk") {
            return "zh-Hant".to_string();
        }
        if lower.contains("hans") || lower.contains("cn") || lower.contains("sg") {
            return "zh-Hans".to_string();
        }
        return "zh-Hans".to_string();
    }

    // For most locales, just use the primary language subtag.
    lower.split('-').next().unwrap_or("en").to_string()
}

fn locale_file_exists(code: &str) -> bool {
    let dirs = config::dirs::app_dirs();
    let path = dirs.resolve_asset_path(&format!("assets/languages/{code}.ini"));
    path.exists()
}

// ---------------------------------------------------------------------------
// Available locales (for the Language option)
// ---------------------------------------------------------------------------

/// Scan `assets/languages/*.ini` and return `(locale_code, native_name)` pairs
/// sorted by locale code. The native name comes from each file's `[Meta]
/// NativeName` value.
pub fn available_locales() -> Vec<(String, String)> {
    let dirs = config::dirs::app_dirs();
    let languages_dir = dirs.resolve_asset_path("assets/languages");
    let mut locales = Vec::new();

    let entries = match std::fs::read_dir(&languages_dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!(
                "Failed to read languages directory {}: {e}",
                languages_dir.display()
            );
            return locales;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ini") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // Skip pseudo-localization file.
        if stem == "pseudo" {
            continue;
        }
        let locale_code = stem.to_string();
        let native_name = match Ini::load_from_file(&path) {
            Ok(ini) => ini
                .get_from(Some("Meta"), "NativeName")
                .unwrap_or(&locale_code)
                .to_string(),
            Err(_) => locale_code.clone(),
        };
        locales.push((locale_code, native_name));
    }

    locales.sort_by(|a, b| a.0.cmp(&b.0));
    locales
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_locale_strips_region() {
        assert_eq!(normalize_locale("en-US"), "en");
        assert_eq!(normalize_locale("ja-JP"), "ja");
        assert_eq!(normalize_locale("fr_FR"), "fr");
    }

    #[test]
    fn normalize_locale_handles_chinese_variants() {
        assert_eq!(normalize_locale("zh-TW"), "zh-Hant");
        assert_eq!(normalize_locale("zh-HK"), "zh-Hant");
        assert_eq!(normalize_locale("zh-Hant-TW"), "zh-Hant");
        assert_eq!(normalize_locale("zh-CN"), "zh-Hans");
        assert_eq!(normalize_locale("zh-SG"), "zh-Hans");
        assert_eq!(normalize_locale("zh-Hans-CN"), "zh-Hans");
        assert_eq!(normalize_locale("zh"), "zh-Hans");
    }
}
