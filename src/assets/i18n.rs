use crate::config;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Loaded language data: active locale strings + English fallback.
struct LangData {
    /// Active (non-English) language strings: section -> (key -> value).
    /// Empty when the active language is English.
    active: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>>,
    /// English fallback strings (always loaded).
    fallback: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>>,
    /// Current locale code (e.g. "en", "es", "ja").
    locale: String,
}

static LANG: OnceLock<RwLock<LangData>> = OnceLock::new();
static LANG_REVISION: AtomicU64 = AtomicU64::new(0);

fn language_file_path(locale: &str) -> std::path::PathBuf {
    config::dirs::app_dirs()
        .exe_dir
        .join("assets")
        .join("languages")
        .join(format!("{locale}.ini"))
}

fn languages_dir_path() -> std::path::PathBuf {
    config::dirs::app_dirs()
        .exe_dir
        .join("assets")
        .join("languages")
}

/// Unescape INI string escape sequences (`\n`, `\t`, `\\`).
fn unescape_ini_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn load_ini_to_map(path: &Path) -> HashMap<Box<str>, HashMap<Box<str>, Arc<str>>> {
    let mut ini = config::SimpleIni::new();
    if let Err(e) = ini.load(path) {
        log::warn!("Failed to load language file {}: {e}", path.display());
        return HashMap::new();
    }
    let mut sections: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>> = HashMap::new();
    for (section, props) in ini.sections() {
        let entries = sections.entry(section.as_str().into()).or_default();
        for (key, value) in props {
            // @skip means "intentionally use English fallback for this key"
            if value.trim() == "@skip" {
                continue;
            }
            entries.insert(
                key.as_str().into(),
                Arc::from(unescape_ini_value(value.as_str()).as_str()),
            );
        }
    }
    sections
}

fn native_name(path: &Path, locale_code: &str) -> String {
    let mut ini = config::SimpleIni::new();
    match ini.load(path) {
        Ok(()) => ini
            .get("Meta", "NativeName")
            .unwrap_or_else(|| locale_code.to_string()),
        Err(e) => {
            log::warn!("Failed to load language file {}: {e}", path.display());
            locale_code.to_string()
        }
    }
}

/// Initialize the i18n system. Call once at startup after config is loaded.
///
/// `locale` is the resolved locale code (e.g. `"en"`, `"es"`). If the locale
/// is `"en"`, only the fallback map is populated and the active map stays empty.
pub fn init(locale: &str) {
    let fallback = load_ini_to_map(&language_file_path("en"));
    let active = if locale != "en" {
        load_ini_to_map(&language_file_path(locale))
    } else {
        HashMap::new()
    };
    let data = LangData {
        active,
        fallback,
        locale: locale.to_string(),
    };
    if let Some(lang) = LANG.get() {
        *lang.write().unwrap() = data;
    } else {
        let _ = LANG.set(RwLock::new(data));
    }
    LANG_REVISION.fetch_add(1, Ordering::AcqRel);
}

/// Look up a localized string by section and key.
///
/// Falls back to English if the key is missing from the active language.
/// Returns `"Section.Key"` if the key is missing from English too (makes
/// untranslated strings visible during development).
pub fn tr(section: &str, key: &str) -> Arc<str> {
    #[cfg(test)]
    ensure_test_init();

    let lang = LANG.get().expect("i18n not initialized").read().unwrap();

    if let Some(section_map) = lang.active.get(section) {
        if let Some(val) = section_map.get(key) {
            return val.clone();
        }
    }
    if let Some(section_map) = lang.fallback.get(section) {
        if let Some(val) = section_map.get(key) {
            return val.clone();
        }
    }
    Arc::from(format!("{section}.{key}"))
}

#[cfg(test)]
fn ensure_test_init() {
    use std::sync::Once;

    static INIT: Once = Once::new();
    if LANG.get().is_some() {
        return;
    }
    INIT.call_once(|| init("en"));
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

/// Switch the active language at runtime. Called when the user changes the
/// Language option. Re-reads the new `.ini` file into the active map.
///
/// All subsequent `tr()` calls will return strings from the new language.
pub fn set_locale(locale: &str) {
    let Some(lang_lock) = LANG.get() else {
        init(locale);
        return;
    };
    let mut lang = lang_lock.write().unwrap();
    if lang.locale == locale {
        return;
    }
    lang.active = if locale != "en" {
        load_ini_to_map(&language_file_path(locale))
    } else {
        HashMap::new()
    };
    lang.locale = locale.to_string();
    drop(lang);
    LANG_REVISION.fetch_add(1, Ordering::AcqRel);
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

pub fn revision() -> u64 {
    LANG_REVISION.load(Ordering::Acquire)
}

pub fn resolve_locale(flag: config::LanguageFlag) -> String {
    match flag {
        config::LanguageFlag::Auto => detect_os_locale(),
        flag => flag.locale_code().to_string(),
    }
}

/// Detect the best locale from the OS settings, falling back to `"en"` if
/// no matching language file is found.
pub fn detect_os_locale() -> String {
    let raw = raw_os_locale().unwrap_or_else(|| "en".to_string());
    let code = normalize_locale(&raw);

    if locale_file_exists(&code) {
        return code;
    }
    if let Some(base) = code.split('-').next() {
        if base != code && locale_file_exists(base) {
            return base.to_string();
        }
    }
    "en".to_string()
}

fn raw_os_locale() -> Option<String> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        let Some(value) = std::env::var(key).ok() else {
            continue;
        };
        let locale = value.split(':').next().unwrap_or_default().trim();
        if !locale.is_empty() {
            return Some(locale.to_string());
        }
    }
    None
}

/// Normalize an OS locale string to our file-naming convention.
///
/// - `"ja-JP"` -> `"ja-jp"`
/// - `"fr-FR.UTF-8"` -> `"fr-fr"`
/// - `"pt_BR"` -> `"pt-br"`
/// - `"zh-TW"` / `"zh-HK"` -> `"zh-Hant"`
/// - `"zh-CN"` / `"zh-SG"` -> `"zh-Hans"`
fn normalize_locale(raw: &str) -> String {
    let lower = raw
        .trim()
        .split('.')
        .next()
        .unwrap_or(raw)
        .split('@')
        .next()
        .unwrap_or(raw)
        .replace('_', "-")
        .to_ascii_lowercase();

    if lower.starts_with("zh") {
        if lower.contains("hant") || lower.contains("tw") || lower.contains("hk") {
            return "zh-Hant".to_string();
        }
        if lower.contains("hans") || lower.contains("cn") || lower.contains("sg") {
            return "zh-Hans".to_string();
        }
        return "zh-Hans".to_string();
    }

    lower
}

fn locale_file_exists(code: &str) -> bool {
    language_file_path(code).exists()
}

/// Scan `assets/languages/*.ini` and return `(locale_code, native_name)` pairs
/// sorted by locale code. The native name comes from each file's `[Meta]`
/// `NativeName` value.
pub fn available_locales() -> Vec<(String, String)> {
    let languages_dir = languages_dir_path();
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
        if stem == "pseudo" {
            continue;
        }
        let locale_code = stem.to_string();
        locales.push((locale_code.clone(), native_name(&path, &locale_code)));
    }

    locales.sort_by(|a, b| a.0.cmp(&b.0));
    locales
}

/// Test-only initializer that loads `en.ini` from the crate's
/// `assets/languages/` directory using `CARGO_MANIFEST_DIR`. Production code
/// uses [`init`], which resolves paths relative to the executable; that path
/// is unavailable under `cargo test`. Idempotent across calls.
#[cfg(test)]
pub(crate) fn init_for_tests() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let en_path = std::path::Path::new(manifest_dir)
            .join("assets")
            .join("languages")
            .join("en.ini");
        let fallback = load_ini_to_map(&en_path);
        let data = LangData {
            active: HashMap::new(),
            fallback,
            locale: "en".to_string(),
        };
        if let Some(lang) = LANG.get() {
            *lang.write().unwrap() = data;
        } else {
            let _ = LANG.set(RwLock::new(data));
        }
        LANG_REVISION.fetch_add(1, Ordering::AcqRel);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_locale_keeps_exact_region() {
        assert_eq!(normalize_locale("en-US"), "en-us");
        assert_eq!(normalize_locale("ja-JP"), "ja-jp");
        assert_eq!(normalize_locale("fr_FR.UTF-8"), "fr-fr");
        assert_eq!(normalize_locale("pt_BR"), "pt-br");
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
