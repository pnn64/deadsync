use deadsync_assets::language::LanguageBundle;
use smallvec::SmallVec;
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
    #[must_use]
    pub fn get(&self) -> Arc<str> {
        tr(self.section, self.key)
    }
}

/// Shorthand for constructing a `LookupKey` in const contexts.
#[must_use]
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
/// # Panics
///
/// Panics if an internal state invariant is violated.
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
#[must_use]
pub fn tr_fmt(section: &str, key: &str, args: &[(&str, &str)]) -> Arc<str> {
    format_translation_template(tr(section, key).as_ref(), args)
}

/// Append a localized string with named placeholder substitution.
///
/// Callers that retain and clear an output buffer can use this path without
/// creating a temporary `String` and `Arc<str>` for every formatted value.
pub fn tr_fmt_into(out: &mut String, section: &str, key: &str, args: &[(&str, &str)]) {
    let template = tr(section, key);
    append_translation_template(out, template.as_ref(), args);
}

fn format_translation_template(template: &str, args: &[(&str, &str)]) -> Arc<str> {
    // This covers normal UI translations while keeping enough stack space for
    // multi-line status text without increasing persistent actor size.
    const INLINE_BYTES: usize = 256;
    if let Some(probe) = probe_translation::<INLINE_BYTES>(template, args) {
        if !probe.overflow {
            return Arc::from(probe.as_str());
        }

        let mut text = String::with_capacity(probe.rendered_len);
        write_translation_template(&mut text, template, args);
        return Arc::from(text);
    }

    let mut text = String::new();
    append_translation_template_sequential(&mut text, template, args);
    Arc::from(text)
}

fn append_translation_template(out: &mut String, template: &str, args: &[(&str, &str)]) {
    if let Some(rendered_len) = fast_translation_len(template, args) {
        out.reserve(rendered_len);
        write_translation_template(out, template, args);
        return;
    }

    append_translation_template_sequential(out, template, args);
}

fn append_translation_template_sequential(out: &mut String, template: &str, args: &[(&str, &str)]) {
    let extra_capacity = args.iter().fold(0usize, |capacity, (name, value)| {
        capacity.saturating_add(value.len().saturating_sub(name.len().saturating_add(2)))
    });
    out.reserve(template.len().saturating_add(extra_capacity));
    let start = out.len();
    out.push_str(template);
    for (name, value) in args {
        replace_named_placeholder(out, start, name, value);
    }
}

struct TranslationProbe<const N: usize> {
    bytes: SmallVec<[u8; N]>,
    rendered_len: usize,
    overflow: bool,
}

impl<const N: usize> TranslationProbe<N> {
    #[inline(always)]
    fn new() -> Self {
        Self {
            bytes: SmallVec::new(),
            rendered_len: 0,
            overflow: false,
        }
    }

    #[inline]
    fn push_str(&mut self, value: &str) -> Option<()> {
        let end = self.rendered_len.checked_add(value.len())?;
        if !self.overflow && end <= N {
            self.bytes.extend_from_slice(value.as_bytes());
        } else {
            self.overflow = true;
        }
        self.rendered_len = end;
        Some(())
    }

    #[inline(always)]
    fn as_str(&self) -> &str {
        debug_assert!(!self.overflow);
        std::str::from_utf8(self.bytes.as_slice())
            .expect("translation formatting only copies valid UTF-8")
    }
}

fn placeholder_value<'a>(name: &str, args: &'a [(&str, &str)]) -> Option<&'a str> {
    args.iter()
        .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
}

fn fast_translation_len(template: &str, args: &[(&str, &str)]) -> Option<usize> {
    probe_translation::<0>(template, args).map(|probe| probe.rendered_len)
}

fn probe_translation<const N: usize>(
    template: &str,
    args: &[(&str, &str)],
) -> Option<TranslationProbe<N>> {
    if args.iter().any(|(name, value)| {
        name.bytes().any(|byte| matches!(byte, b'{' | b'}'))
            || value.bytes().any(|byte| matches!(byte, b'{' | b'}'))
    }) {
        return None;
    }

    let mut probe = TranslationProbe::new();
    let mut cursor = 0usize;
    while cursor < template.len() {
        let remaining = &template[cursor..];
        let special = remaining
            .bytes()
            .position(|byte| matches!(byte, b'{' | b'}'));
        let Some(relative_open) = special else {
            probe.push_str(remaining)?;
            return Some(probe);
        };
        let open = cursor + relative_open;
        if template.as_bytes()[open] == b'}' {
            return None;
        }
        probe.push_str(&template[cursor..open])?;

        let name_start = open + 1;
        let relative_close = template[name_start..].find('}')?;
        let close = name_start + relative_close;
        let name = &template[name_start..close];
        if name.contains('{') {
            return None;
        }
        if let Some(value) = placeholder_value(name, args) {
            probe.push_str(value)?;
        } else {
            probe.push_str(&template[open..=close])?;
        }
        cursor = close + 1;
    }
    Some(probe)
}

fn write_translation_template(out: &mut String, template: &str, args: &[(&str, &str)]) {
    let mut cursor = 0usize;
    while let Some(relative_open) = template[cursor..].find('{') {
        let open = cursor + relative_open;
        out.push_str(&template[cursor..open]);
        let name_start = open + 1;
        let relative_close = template[name_start..]
            .find('}')
            .expect("preflighted translation placeholder must close");
        let close = name_start + relative_close;
        if let Some(value) = placeholder_value(&template[name_start..close], args) {
            out.push_str(value);
        } else {
            out.push_str(&template[open..=close]);
        }
        cursor = close + 1;
    }
    out.push_str(&template[cursor..]);
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn benchmark_format_translation_reference(template: &str, args: &[(&str, &str)]) -> Arc<str> {
    let mut text = String::new();
    append_translation_template_sequential(&mut text, template, args);
    Arc::from(text)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn benchmark_format_translation_current(template: &str, args: &[(&str, &str)]) -> Arc<str> {
    format_translation_template(template, args)
}

fn replace_named_placeholder(text: &mut String, start: usize, name: &str, value: &str) {
    let mut search_from = start;
    while let Some(relative_open) = text[search_from..].find('{') {
        let open = search_from + relative_open;
        let name_start = open + 1;
        let name_end = name_start.saturating_add(name.len());
        let close = name_end.saturating_add(1);
        let matches = text
            .get(name_start..name_end)
            .is_some_and(|candidate| candidate == name)
            && text.as_bytes().get(name_end) == Some(&b'}');
        if matches {
            text.replace_range(open..close, value);
            search_from = open.saturating_add(value.len());
        } else {
            search_from = name_start;
        }
    }
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
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
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

    #[test]
    fn template_append_preserves_prefix_and_placeholder_semantics() {
        let mut out = String::from("prefix: ");
        append_translation_template(
            &mut out,
            "{player} has {count} steps; {player} is ready; {missing}",
            &[("player", "P1"), ("count", "123")],
        );

        assert_eq!(out, "prefix: P1 has 123 steps; P1 is ready; {missing}");
    }

    #[test]
    fn template_append_matches_sequential_replacement_behavior() {
        let mut out = String::new();
        append_translation_template(
            &mut out,
            "{first}/{second}",
            &[("first", "{second}"), ("second", "done")],
        );

        assert_eq!(out, "done/done");
    }

    #[test]
    fn template_format_handles_repeated_missing_unicode_and_duplicate_arguments() {
        let text = format_translation_template(
            "{player} scored {score}/{score} — {missing}",
            &[("player", "Åsa"), ("score", "12345"), ("score", "ignored")],
        );

        assert_eq!(text.as_ref(), "Åsa scored 12345/12345 — {missing}");
    }

    #[test]
    fn template_format_preserves_cross_boundary_sequential_replacements() {
        let text =
            format_translation_template("{{remove}name}", &[("remove", ""), ("name", "ready")]);

        assert_eq!(text.as_ref(), "ready");
        assert_eq!(
            text,
            benchmark_format_translation_reference(
                "{{remove}name}",
                &[("remove", ""), ("name", "ready")],
            )
        );
    }

    #[test]
    fn template_format_long_output_matches_ground_truth_and_reference() {
        let template = "{value}|".repeat(64);
        let value = "expanded-placeholder-value";
        let expected = format!("{}|", value).repeat(64);
        let args = [("value", value)];
        let current = format_translation_template(&template, &args);

        assert_eq!(current.as_ref(), expected);
        assert_eq!(
            current,
            benchmark_format_translation_reference(&template, &args)
        );
    }

    #[test]
    fn template_format_matches_reference_matrix() {
        let fixtures: [(&str, &[(&str, &str)]); 6] = [
            ("plain text", &[]),
            ("{a}/{b}/{a}", &[("a", "1"), ("b", "two")]),
            ("{known} {missing}", &[("known", "yes")]),
            ("Δ {name} ✓", &[("name", "Miyuki")]),
            (
                "{first}/{second}",
                &[("first", "{second}"), ("second", "done")],
            ),
            ("{{remove}name}", &[("remove", ""), ("name", "ready")]),
        ];

        for (template, args) in fixtures {
            assert_eq!(
                format_translation_template(template, args),
                benchmark_format_translation_reference(template, args),
                "template={template:?}"
            );
        }
    }
}
