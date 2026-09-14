//! Text-producing expressions extracted from fc570795b (0.5.1206).
//! `baseline_search.rs` and `baseline_layout.rs` also retain the complete callers.
use crate::choice_text::Choice;
use crate::i18n_runtime::format_translation_template;
use deadlib_present::actors::TextContent;
use std::borrow::Cow;
use std::sync::Arc;

pub fn current(value: &TextContent, template: &Arc<str>) -> Arc<str> {
    let value = value.to_string();
    format_translation_template(template, &[("value", &value)])
}

pub fn query(query: &str, caret_on: bool) -> TextContent {
    let caret = if caret_on { "\u{25ae}" } else { "" };
    TextContent::from(format!("{query}{caret}"))
}

pub fn choices(choices: &[Choice]) -> Arc<[Arc<str>]> {
    let texts: Vec<Cow<'static, str>> = choices
        .iter()
        .map(|choice| Cow::Owned(choice.get().to_string()))
        .collect();
    finish(texts)
}

pub fn strings(choices: &[String]) -> Arc<[Arc<str>]> {
    let texts: Vec<Cow<'static, str>> = choices.iter().cloned().map(Cow::Owned).collect();
    finish(texts)
}

fn finish(choice_texts: Vec<Cow<'static, str>>) -> Arc<[Arc<str>]> {
    let texts: Vec<Arc<str>> = choice_texts
        .iter()
        .map(|text| Arc::from(text.as_ref()))
        .collect();
    Arc::from(texts)
}
