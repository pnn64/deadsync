use crate::screens::components::shared::fuzzy;
use std::cell::OnceCell;
use std::sync::Arc;

/// Immutable text prepared lazily on focus, then shared with frame actors.
/// One instance belongs to one match in one query's result list. Rebuilding
/// that list replaces both cells, including cached absent completions/help.
#[derive(Clone, Debug, Default)]
pub(super) struct SearchResultText {
    prefix: OnceCell<Option<Arc<str>>>,
    help: OnceCell<Option<Arc<str>>>,
}

impl SearchResultText {
    pub(super) fn completion(&self, query: &str, label: &Arc<str>) -> Option<(Arc<str>, Arc<str>)> {
        if query.is_empty() {
            return None;
        }
        let prefix = self.prefix.get_or_init(|| {
            let consumed = fuzzy::folded_prefix_len(query, label)?;
            // nth returns None when the query consumes the whole label. The
            // byte boundary includes any original combining marks folded away.
            let (end, _) = label.char_indices().nth(consumed)?;
            Some(Arc::from(&label[..end]))
        });
        prefix
            .as_ref()
            .map(|prefix| (Arc::clone(label), Arc::clone(prefix)))
    }

    pub(super) fn help<'a>(
        &self,
        lines: impl Iterator<Item = &'a Arc<str>> + Clone,
    ) -> Option<Arc<str>> {
        self.help
            .get_or_init(|| {
                let mut nonempty = lines
                    .map(|line| (line, line.trim()))
                    .filter(|(_, text)| !text.is_empty());
                let (original, first) = nonempty.next()?;
                // Most rows already own a single actor-ready help line.
                let Some((_, second)) = nonempty.next() else {
                    return Some(if first.len() == original.len() {
                        Arc::clone(original)
                    } else {
                        Arc::from(first)
                    });
                };
                let bytes = first.len()
                    + 1
                    + second.len()
                    + nonempty
                        .clone()
                        .map(|(_, text)| 1 + text.len())
                        .sum::<usize>();
                // Short help needs only the final Arc allocation. Oversized
                // localized text reserves one exact-sized scratch allocation.
                let mut joined = smallvec::SmallVec::<[u8; 256]>::with_capacity(bytes);
                joined.extend_from_slice(first.as_bytes());
                joined.push(b' ');
                joined.extend_from_slice(second.as_bytes());
                for (_, text) in nonempty {
                    joined.push(b' ');
                    joined.extend_from_slice(text.as_bytes());
                }
                Some(Arc::from(std::str::from_utf8(&joined).expect(
                    "joining UTF-8 strings with ASCII spaces preserves UTF-8",
                )))
            })
            .as_ref()
            .map(Arc::clone)
    }
}
