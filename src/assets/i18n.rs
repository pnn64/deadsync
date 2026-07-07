use crate::config;
use deadsync_config::theme::LanguageFlag;
use std::path::PathBuf;

pub use deadsync_theme::i18n::{LookupKey, current_locale, lookup_key, revision, tr, tr_fmt};

fn language_file_path(locale: &str) -> PathBuf {
    languages_dir_path().join(format!("{locale}.ini"))
}

fn languages_dir_path() -> PathBuf {
    deadlib_platform::dirs::app_dirs()
        .exe_dir
        .join("assets")
        .join("languages")
}

/// Initialize the i18n system. Call once at startup after config is loaded.
pub fn init(locale: &str) {
    deadsync_theme::i18n::init(&languages_dir_path(), locale);
}

/// Switch the active language at runtime.
pub fn set_locale(locale: &str) {
    deadsync_theme::i18n::set_locale(&languages_dir_path(), locale);
}

pub fn resolve_locale(flag: LanguageFlag) -> String {
    deadsync_theme::i18n::resolve_locale(
        flag,
        deadsync_theme::i18n::raw_os_locale().as_deref(),
        locale_file_exists,
    )
}

/// Detect the best locale from the OS settings, falling back to `"en"` if
/// no matching language file is found.
pub fn detect_os_locale() -> String {
    deadsync_theme::i18n::resolve_locale(
        config::LanguageFlag::Auto,
        deadsync_theme::i18n::raw_os_locale().as_deref(),
        locale_file_exists,
    )
}

fn locale_file_exists(code: &str) -> bool {
    language_file_path(code).exists()
}

/// Scan `assets/languages/*.ini` and return `(locale_code, native_name)` pairs
/// sorted by locale code.
pub fn available_locales() -> Vec<(String, String)> {
    deadsync_theme::i18n::available_locales(&languages_dir_path())
}

/// Test-only initializer that loads `en.ini` from the root crate's
/// `assets/languages/` directory using `CARGO_MANIFEST_DIR`.
#[cfg(test)]
pub(crate) fn init_for_tests() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let languages_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("languages");
        deadsync_theme::i18n::init(&languages_dir, "en");
    });
}
