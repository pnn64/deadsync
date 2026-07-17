use deadsync_config::ini::{SimpleIni, unescape_ini_value};
use deadsync_config::theme::{LanguageFlag, resolve_language_locale};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type LanguageMap = HashMap<Box<str>, HashMap<Box<str>, Arc<str>>>;

/// Language resources prepared at the asset boundary for a concrete theme.
pub struct LanguageBundle {
    pub active: LanguageMap,
    pub fallback: LanguageMap,
    pub locale: String,
}

const OS_LOCALE_ENV_KEYS: [&str; 4] = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"];

fn languages_dir_path() -> PathBuf {
    crate::resolve_asset_path("assets/languages")
}

fn language_file_path(languages_dir: &Path, locale: &str) -> PathBuf {
    languages_dir.join(format!("{locale}.ini"))
}

fn locale_file_exists(languages_dir: &Path, code: &str) -> bool {
    language_file_path(languages_dir, code).exists()
}

fn load_ini_to_map(path: &Path) -> LanguageMap {
    let mut ini = SimpleIni::new();
    if let Err(e) = ini.load(path) {
        log::warn!("Failed to load language file {}: {e}", path.display());
        return HashMap::new();
    }
    let mut sections: LanguageMap = HashMap::new();
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

fn load_from_dir(languages_dir: &Path, locale: &str) -> LanguageBundle {
    LanguageBundle {
        fallback: load_ini_to_map(&language_file_path(languages_dir, "en")),
        active: if locale == "en" {
            HashMap::new()
        } else {
            load_ini_to_map(&language_file_path(languages_dir, locale))
        },
        locale: locale.to_string(),
    }
}

/// Load the selected language and English fallback from bundled assets.
pub fn load(locale: &str) -> LanguageBundle {
    load_from_dir(&languages_dir_path(), locale)
}

/// Resolve a configured language choice against bundled assets and the host
/// locale environment.
pub fn resolve_locale(flag: LanguageFlag) -> String {
    let languages_dir = languages_dir_path();
    resolve_language_locale(flag, raw_os_locale().as_deref(), |code| {
        locale_file_exists(&languages_dir, code)
    })
}

fn raw_os_locale_from_values<T: AsRef<str>>(values: impl IntoIterator<Item = T>) -> Option<String> {
    for value in values {
        let locale = value.as_ref().split(':').next().unwrap_or_default().trim();
        if !locale.is_empty() {
            return Some(locale.to_string());
        }
    }
    None
}

fn raw_os_locale() -> Option<String> {
    raw_os_locale_from_values(
        OS_LOCALE_ENV_KEYS
            .into_iter()
            .filter_map(|key| std::env::var(key).ok()),
    )
}

/// Return bundled `(locale_code, native_name)` pairs sorted by locale code.
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

/// Load root-repository language assets for theme unit tests.
#[doc(hidden)]
pub fn load_for_tests(locale: &str) -> LanguageBundle {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest_dir);
    load_from_dir(&root_dir.join("assets").join("languages"), locale)
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
    fn locale_file_exists_checks_language_ini() {
        let dir = std::env::temp_dir().join(format!(
            "deadsync-assets-language-locale-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create locale dir");
        std::fs::write(dir.join("en.ini"), "").expect("write locale file");

        assert!(locale_file_exists(&dir, "en"));
        assert!(!locale_file_exists(&dir, "missing"));

        let _ = std::fs::remove_dir_all(dir);
    }
}
