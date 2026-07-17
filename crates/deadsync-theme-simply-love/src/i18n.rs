pub use crate::i18n_runtime::{LookupKey, current_locale, lookup_key, revision, tr, tr_fmt};
pub use deadsync_assets::language::LanguageBundle;

/// Initialize render-time localization from a prepared asset bundle.
pub fn init(bundle: LanguageBundle) {
    crate::i18n_runtime::init(bundle);
}

/// Switch render-time localization to a prepared asset bundle.
pub fn set_locale(bundle: LanguageBundle) {
    crate::i18n_runtime::set_locale(bundle);
}

/// Test initializer that loads `en.ini` through the asset boundary.
#[doc(hidden)]
pub fn init_for_tests() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| init(deadsync_assets::language::load_for_tests("en")));
}
