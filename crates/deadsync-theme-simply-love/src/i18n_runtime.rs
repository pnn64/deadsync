use deadsync_assets::language::LanguageBundle;
use std::cell::RefCell;
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

static LANG: OnceLock<RwLock<Arc<LanguageBundle>>> = OnceLock::new();
static LANG_REVISION: AtomicU64 = AtomicU64::new(0);

struct LanguageSnapshot {
    revision: u64,
    bundle: Option<Arc<LanguageBundle>>,
}

impl LanguageSnapshot {
    const fn new() -> Self {
        Self {
            revision: u64::MAX,
            bundle: None,
        }
    }

    #[cold]
    fn refresh(&mut self, revision: u64, source: &RwLock<Arc<LanguageBundle>>) {
        self.bundle = Some(source.read().expect("i18n language lock poisoned").clone());
        self.revision = revision;
    }
}

thread_local! {
    static LANG_SNAPSHOT: RefCell<LanguageSnapshot> =
        const { RefCell::new(LanguageSnapshot::new()) };
}

/// Install shell-prepared language resources.
pub fn init(bundle: LanguageBundle) {
    let bundle = Arc::new(bundle);
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

    let source = LANG.get().expect("i18n not initialized");
    let revision = LANG_REVISION.load(Ordering::Acquire);
    LANG_SNAPSHOT.with_borrow_mut(|snapshot| {
        if snapshot.revision != revision || snapshot.bundle.is_none() {
            snapshot.refresh(revision, source);
        }
        lookup(
            snapshot.bundle.as_deref().expect("i18n snapshot missing"),
            section,
            key,
        )
    })
}

fn lookup(lang: &LanguageBundle, section: &str, key: &str) -> Arc<str> {
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
    *lang = Arc::new(bundle);
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_assets::language::LanguageMap;
    use rustc_hash::FxHashMap;

    fn refresh_if_stale(
        snapshot: &mut LanguageSnapshot,
        revision: u64,
        source: &RwLock<Arc<LanguageBundle>>,
    ) {
        if snapshot.revision != revision || snapshot.bundle.is_none() {
            snapshot.refresh(revision, source);
        }
    }

    fn bundle(locale: &str, text: &str) -> LanguageBundle {
        let mut section = FxHashMap::default();
        section.insert(Box::from("Key"), Arc::from(text));
        let mut fallback = LanguageMap::default();
        fallback.insert(Box::from("Section"), section);
        LanguageBundle {
            active: LanguageMap::default(),
            fallback,
            locale: locale.to_string(),
        }
    }

    #[test]
    fn snapshot_refreshes_only_when_revision_changes() {
        let source = RwLock::new(Arc::new(bundle("en", "Before")));
        let mut snapshot = LanguageSnapshot::new();
        refresh_if_stale(&mut snapshot, 1, &source);
        assert_eq!(
            lookup(
                snapshot.bundle.as_deref().expect("snapshot refreshed"),
                "Section",
                "Key",
            )
            .as_ref(),
            "Before"
        );

        *source.write().expect("test language lock poisoned") = Arc::new(bundle("fr", "After"));
        refresh_if_stale(&mut snapshot, 1, &source);
        assert_eq!(
            lookup(
                snapshot.bundle.as_deref().expect("snapshot retained"),
                "Section",
                "Key",
            )
            .as_ref(),
            "Before"
        );

        refresh_if_stale(&mut snapshot, 2, &source);
        assert_eq!(
            lookup(
                snapshot.bundle.as_deref().expect("snapshot refreshed"),
                "Section",
                "Key",
            )
            .as_ref(),
            "After"
        );
    }
}
