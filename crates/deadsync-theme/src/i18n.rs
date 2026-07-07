use deadsync_config::ini::{SimpleIni, unescape_ini_value};
use deadsync_config::theme::{LanguageFlag, resolve_language_locale};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

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

struct LangData {
    active: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>>,
    fallback: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>>,
    locale: String,
}

static LANG: OnceLock<RwLock<LangData>> = OnceLock::new();
static LANG_REVISION: AtomicU64 = AtomicU64::new(0);
const OS_LOCALE_ENV_KEYS: [&str; 4] = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"];

fn language_file_path(languages_dir: &Path, locale: &str) -> PathBuf {
    languages_dir.join(format!("{locale}.ini"))
}

pub fn languages_dir_path(exe_dir: &Path) -> PathBuf {
    exe_dir.join("assets").join("languages")
}

pub fn locale_file_exists(languages_dir: &Path, code: &str) -> bool {
    language_file_path(languages_dir, code).exists()
}

pub fn resolve_locale_in_dir(flag: LanguageFlag, languages_dir: &Path) -> String {
    resolve_locale(flag, raw_os_locale().as_deref(), |code| {
        locale_file_exists(languages_dir, code)
    })
}

fn load_ini_to_map(path: &Path) -> HashMap<Box<str>, HashMap<Box<str>, Arc<str>>> {
    let mut ini = SimpleIni::new();
    if let Err(e) = ini.load(path) {
        log::warn!("Failed to load language file {}: {e}", path.display());
        return HashMap::new();
    }
    let mut sections: HashMap<Box<str>, HashMap<Box<str>, Arc<str>>> = HashMap::new();
    for (section, props) in ini.sections() {
        let entries = sections.entry(section.as_str().into()).or_default();
        for (key, value) in props {
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
    let mut ini = SimpleIni::new();
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

/// Initialize the i18n system from `languages_dir`.
///
/// `locale` is the resolved locale code, for example `"en"`, `"es"`, or
/// `"ja"`. English is always loaded as the fallback language.
pub fn init(languages_dir: &Path, locale: &str) {
    let fallback = load_ini_to_map(&language_file_path(languages_dir, "en"));
    let active = if locale != "en" {
        load_ini_to_map(&language_file_path(languages_dir, locale))
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
/// Returns `"Section.Key"` if the key is missing from English too.
pub fn tr(section: &str, key: &str) -> Arc<str> {
    #[cfg(test)]
    ensure_test_init();

    let lang = LANG.get().expect("i18n not initialized").read().unwrap();

    if let Some(section_map) = lang.active.get(section)
        && let Some(val) = section_map.get(key)
    {
        return val.clone();
    }
    if let Some(section_map) = lang.fallback.get(section)
        && let Some(val) = section_map.get(key)
    {
        return val.clone();
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
    INIT.call_once(|| {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir
            .parent()
            .and_then(Path::parent)
            .unwrap_or(manifest_dir);
        init(&root_dir.join("assets").join("languages"), "en");
    });
}

/// Look up a localized string with named placeholder substitution.
pub fn tr_fmt(section: &str, key: &str, args: &[(&str, &str)]) -> Arc<str> {
    let mut s = tr(section, key).to_string();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    Arc::from(s)
}

/// Switch the active language at runtime.
pub fn set_locale(languages_dir: &Path, locale: &str) {
    let Some(lang_lock) = LANG.get() else {
        init(languages_dir, locale);
        return;
    };
    let mut lang = lang_lock.write().unwrap();
    if lang.locale == locale {
        return;
    }
    lang.active = if locale != "en" {
        load_ini_to_map(&language_file_path(languages_dir, locale))
    } else {
        HashMap::new()
    };
    lang.locale = locale.to_string();
    drop(lang);
    LANG_REVISION.fetch_add(1, Ordering::AcqRel);
}

/// Returns the currently active locale code.
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

pub fn resolve_locale(
    flag: LanguageFlag,
    raw_os_locale: Option<&str>,
    locale_file_exists: impl Fn(&str) -> bool,
) -> String {
    resolve_language_locale(flag, raw_os_locale, locale_file_exists)
}

pub fn raw_os_locale_from_values<T: AsRef<str>>(
    values: impl IntoIterator<Item = T>,
) -> Option<String> {
    for value in values {
        let locale = value.as_ref().split(':').next().unwrap_or_default().trim();
        if !locale.is_empty() {
            return Some(locale.to_string());
        }
    }
    None
}

pub fn raw_os_locale() -> Option<String> {
    raw_os_locale_from_values(
        OS_LOCALE_ENV_KEYS
            .into_iter()
            .filter_map(|key| std::env::var(key).ok()),
    )
}

/// Scan `languages_dir/*.ini` and return `(locale_code, native_name)` pairs
/// sorted by locale code.
pub fn available_locales(languages_dir: &Path) -> Vec<(String, String)> {
    let mut locales = Vec::new();

    let entries = match std::fs::read_dir(languages_dir) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_os_locale_uses_first_colon_separated_locale() {
        assert_eq!(
            raw_os_locale_from_values([" fr_FR.UTF-8:en_US.UTF-8 "]),
            Some("fr_FR.UTF-8".to_string())
        );
    }

    #[test]
    fn raw_os_locale_skips_empty_values() {
        assert_eq!(
            raw_os_locale_from_values(["", "  ", "ja_JP.UTF-8"]),
            Some("ja_JP.UTF-8".to_string())
        );
    }

    #[test]
    fn raw_os_locale_returns_none_without_nonempty_values() {
        assert_eq!(raw_os_locale_from_values(["", "  "]), None);
    }

    #[test]
    fn languages_dir_path_uses_exe_assets_languages() {
        assert_eq!(
            languages_dir_path(Path::new("/game")),
            PathBuf::from("/game/assets/languages")
        );
    }

    #[test]
    fn locale_file_exists_checks_language_ini() {
        let dir =
            std::env::temp_dir().join(format!("deadsync-theme-i18n-locale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create locale dir");
        std::fs::write(dir.join("en.ini"), "").expect("write locale file");

        assert!(locale_file_exists(&dir, "en"));
        assert!(!locale_file_exists(&dir, "missing"));

        let _ = std::fs::remove_dir_all(dir);
    }
}
