use crate::i18n_runtime::format_translation_template;
use deadlib_present::actors::TextContent;
use smallvec::SmallVec;
use std::cell::{OnceCell, RefCell};
use std::sync::Arc;

/// One detail entry per open overlay. Compare actual text so player changes,
/// replacement choices and locale reloads cannot leave a stale detail line.
#[derive(Clone, Debug, Default)]
pub(super) struct SearchCurrentText {
    entry: RefCell<Option<CurrentEntry>>,
}

#[derive(Clone, Debug)]
struct CurrentEntry {
    value: TextContent,
    template: Arc<str>,
    rendered: Arc<str>,
}

impl SearchCurrentText {
    pub(super) fn get(&self, value: &TextContent, template: &Arc<str>) -> Arc<str> {
        let mut entry = self.entry.borrow_mut();
        if let Some(cached) = entry.as_ref()
            && Arc::ptr_eq(template, &cached.template)
            && value.as_str() == cached.value.as_str()
        {
            return Arc::clone(&cached.rendered);
        }
        let rendered = format_translation_template(template, &[("value", value.as_str())]);
        *entry = Some(CurrentEntry {
            value: value.clone(),
            template: Arc::clone(template),
            rendered: Arc::clone(&rendered),
        });
        rendered
    }
}

/// Short queries stay inside frame actors. Longer query/caret variants are
/// allocated only on first use, then shared until the next query edit.
#[derive(Clone, Debug, Default)]
pub(super) struct SearchQueryText {
    variants: [OnceCell<Arc<str>>; 2],
}

impl SearchQueryText {
    pub(super) fn invalidate(&mut self) {
        self.variants = Default::default();
    }

    pub(super) fn get(&self, query: &str, caret_on: bool) -> TextContent {
        let caret = if caret_on { "\u{25ae}" } else { "" };
        if let Some(text) = TextContent::inline_format(format_args!("{query}{caret}")) {
            return text;
        }
        TextContent::Shared(Arc::clone(
            self.variants[usize::from(caret_on)].get_or_init(|| {
                if !caret_on {
                    return Arc::from(query);
                }
                let len = query.len() + caret.len();
                if len > 256 {
                    // Keep long UTF-8 input in String so conversion to Arc does
                    // not rescan its bytes for validity. Reserve once, exactly.
                    let mut text = String::with_capacity(len);
                    text.push_str(query);
                    text.push_str(caret);
                    return Arc::from(text);
                }
                let mut bytes = SmallVec::<[u8; 256]>::with_capacity(len);
                bytes.extend_from_slice(query.as_bytes());
                bytes.extend_from_slice(caret.as_bytes());
                Arc::from(std::str::from_utf8(&bytes).expect("query and caret are valid UTF-8"))
            }),
        ))
    }
}
