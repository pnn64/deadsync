// Frozen from 5c0e0263b's search.rs. Inputs replace row/state access; the
// unchanged scorer is shared to isolate label ownership from typo pruning.
use crate::fuzzy;
use std::sync::Arc;

fn clean_label(raw: &str) -> String {
    let mut end = raw.len();
    for pat in ["\\n", "\n", "{"] {
        if let Some(i) = raw.find(pat) {
            end = end.min(i);
        }
    }
    raw[..end].trim().to_string()
}

pub(super) fn matched_setting_label(
    query: &fuzzy::Query,
    raw: &Arc<str>,
    aliases: &[&str],
) -> Option<(Arc<str>, i32)> {
    let label = clean_label(raw);
    if query.is_empty() {
        Some((label.into(), 0))
    } else if let Some(score) =
        fuzzy::best_match_score(query, &fuzzy::fold_diacritics(&label), aliases)
    {
        Some((label.into(), score))
    } else {
        None
    }
}

pub(super) fn row_text(label: &str) -> [Arc<str>; 2] {
    [
        Arc::from(format!("  {label}")),
        Arc::from(format!("\u{25b8} {label}")),
    ]
}
