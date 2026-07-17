use deadsync_assets::language::LanguageBundle;
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

static LANG: OnceLock<RwLock<LanguageBundle>> = OnceLock::new();
static LANG_REVISION: AtomicU64 = AtomicU64::new(0);

/// Install shell-prepared language resources.
pub fn init(bundle: LanguageBundle) {
    if let Some(lang) = LANG.get() {
        *lang.write().unwrap() = bundle;
    } else {
        let _ = LANG.set(RwLock::new(bundle));
    }
    LANG_REVISION.fetch_add(1, Ordering::AcqRel);
}

/// Look up a localized string by section and key.
///
/// Falls back to English if the key is missing from the active language.
/// Returns `"Section.Key"` if the key is missing from English too.
pub fn tr(section: &str, key: &str) -> Arc<str> {
    #[cfg(any(test, feature = "test-support"))]
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

#[cfg(any(test, feature = "test-support"))]
fn ensure_test_init() {
    use std::sync::Once;

    static INIT: Once = Once::new();
    if LANG.get().is_some() {
        return;
    }
    INIT.call_once(|| init(deadsync_assets::language::load_for_tests("en")));
}

/// Look up a localized string with named placeholder substitution.
pub fn tr_fmt(section: &str, key: &str, args: &[(&str, &str)]) -> Arc<str> {
    let mut s = tr(section, key).to_string();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    Arc::from(s)
}

/// Switch to shell-prepared language resources.
pub fn set_locale(bundle: LanguageBundle) {
    let Some(lang_lock) = LANG.get() else {
        init(bundle);
        return;
    };
    let mut lang = lang_lock.write().unwrap();
    if lang.locale == bundle.locale {
        return;
    }
    *lang = bundle;
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
