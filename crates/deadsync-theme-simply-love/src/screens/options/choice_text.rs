use crate::i18n::LookupKey;
use std::borrow::Cow;
use std::sync::Arc;

/// Choice values — some are localizable, some are format-specific literals.
#[derive(Clone, Copy)]
pub enum Choice {
    /// Translatable text (e.g., "Windowed", "On", "Off").
    Localized(LookupKey),
    /// Format-specific literal that should never be translated (e.g., "16:9", "1920x1080").
    Literal(&'static str),
}

impl Choice {
    pub fn get(&self) -> Arc<str> {
        match self {
            Self::Localized(lkey) => lkey.get(),
            Self::Literal(s) => Arc::from(*s),
        }
    }
}

/// Layouts preserve shared translations. Input handling keeps owned strings so
/// numeric choices do not acquire a temporary Arc solely to inspect/cycle them.
pub(super) trait ChoiceText: Sized {
    fn shared(text: Arc<str>) -> Self;
    fn owned(text: String) -> Self;
    fn borrowed(text: &str) -> Self;
}

impl ChoiceText for Arc<str> {
    fn shared(text: Arc<str>) -> Self {
        text
    }
    fn owned(text: String) -> Self {
        Arc::from(text)
    }
    fn borrowed(text: &str) -> Self {
        Arc::from(text)
    }
}

impl ChoiceText for Cow<'static, str> {
    fn shared(text: Arc<str>) -> Self {
        Cow::Owned(text.to_string())
    }
    fn owned(text: String) -> Self {
        Cow::Owned(text)
    }
    fn borrowed(text: &str) -> Self {
        Cow::Owned(text.to_owned())
    }
}

pub(super) fn choice_texts<T: ChoiceText>(choices: &[Choice]) -> Vec<T> {
    choices
        .iter()
        .map(|choice| T::shared(choice.get()))
        .collect()
}

pub(super) fn string_choice_texts<T: ChoiceText>(choices: &[String]) -> Vec<T> {
    choices.iter().map(|text| T::borrowed(text)).collect()
}
