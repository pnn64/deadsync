use deadsync_config::theme::LanguageFlag;
use std::path::PathBuf;

pub use crate::i18n_runtime::{LookupKey, current_locale, lookup_key, revision, tr, tr_fmt};

fn languages_dir_path() -> PathBuf {
    crate::i18n_runtime::languages_dir_path(&deadlib_platform::dirs::app_dirs().exe_dir)
}

/// Initialize the i18n system. Call once at startup after config is loaded.
pub fn init(locale: &str) {
    crate::i18n_runtime::init(&languages_dir_path(), locale);
}

/// Switch the active language at runtime.
pub fn set_locale(locale: &str) {
    crate::i18n_runtime::set_locale(&languages_dir_path(), locale);
}

pub fn resolve_locale(flag: LanguageFlag) -> String {
    crate::i18n_runtime::resolve_locale_in_dir(flag, &languages_dir_path())
}

/// Detect the best locale from the OS settings, falling back to `"en"` if
/// no matching language file is found.
pub fn detect_os_locale() -> String {
    crate::i18n_runtime::resolve_locale_in_dir(LanguageFlag::Auto, &languages_dir_path())
}

/// Scan `assets/languages/*.ini` and return `(locale_code, native_name)` pairs
/// sorted by locale code.
pub fn available_locales() -> Vec<(String, String)> {
    crate::i18n_runtime::available_locales(&languages_dir_path())
}

/// Test initializer that loads `en.ini` from the root crate's assets.
#[doc(hidden)]
pub fn init_for_tests() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap_or(manifest_dir);
        crate::i18n_runtime::init(&root_dir.join("assets").join("languages"), "en");
    });
}
