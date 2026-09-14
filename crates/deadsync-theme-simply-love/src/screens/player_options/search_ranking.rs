use crate::screens::components::shared::fuzzy;
use std::cell::OnceCell;
use std::sync::Arc;

/// Strip multiline and templated names without copying the candidate.
fn clean_label(raw: &str) -> &str {
    let mut end = raw.len();
    for pat in ["\\n", "\n", "{"] {
        if let Some(i) = raw.find(pat) {
            end = end.min(i);
        }
    }
    raw[..end].trim()
}

/// Only successful matches need a label handle. Unchanged labels share the
/// row's existing allocation; trimmed labels allocate their final Arc directly.
pub(super) fn matched_setting_label(
    query: &fuzzy::Query,
    raw: &Arc<str>,
    aliases: &[&str],
) -> Option<(Arc<str>, i32)> {
    let label = clean_label(raw);
    let score = if query.is_empty() {
        0
    } else {
        fuzzy::best_match_score(query, &fuzzy::fold_diacritics(label), aliases)?
    };
    let label = if label.len() == raw.len() {
        Arc::clone(raw)
    } else {
        Arc::from(label)
    };
    Some((label, score))
}

/// Display variants belong to one immutable result label. Prepare a variant
/// when it enters the visible window, then share it with subsequent actors.
#[derive(Clone, Debug, Default)]
pub(super) struct SearchRowText {
    variants: [OnceCell<Arc<str>>; 2],
}

impl SearchRowText {
    #[inline]
    pub(super) fn get(&self, label: &Arc<str>, focused: bool) -> Arc<str> {
        Arc::clone(
            self.variants[usize::from(focused)].get_or_init(|| build_row_text(label, focused)),
        )
    }
}

#[cold]
fn build_row_text(label: &str, focused: bool) -> Arc<str> {
    let prefix = if focused { "\u{25b8} " } else { "  " };
    // Short labels need only the final Arc allocation. Longer translations
    // reserve exact scratch capacity and never grow it while concatenating.
    let mut text = smallvec::SmallVec::<[u8; 128]>::with_capacity(prefix.len() + label.len());
    text.extend_from_slice(prefix.as_bytes());
    text.extend_from_slice(label.as_bytes());
    Arc::from(std::str::from_utf8(&text).expect("concatenating UTF-8 strings preserves UTF-8"))
}
