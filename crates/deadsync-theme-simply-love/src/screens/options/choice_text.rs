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

/// Layouts preserve shared translations. Input handling borrows static literals
/// and owns dynamic values without temporary Arc conversions.
pub(super) trait ChoiceText: Sized {
    fn shared(text: Arc<str>) -> Self;
    fn owned(text: String) -> Self;
    fn borrowed(text: &str) -> Self;
    #[inline]
    fn choices(choices: &[Choice]) -> Vec<Self> {
        choices
            .iter()
            .map(|choice| Self::shared(choice.get()))
            .collect()
    }
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
    #[inline]
    fn choices(choices: &[Choice]) -> Vec<Self> {
        choices
            .iter()
            .map(|choice| match choice {
                Choice::Localized(key) => Self::shared(key.get()),
                Choice::Literal(text) => Cow::Borrowed(*text),
            })
            .collect()
    }
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
    T::choices(choices)
}

pub(super) fn string_choice_texts<T: ChoiceText>(choices: &[String]) -> Vec<T> {
    choices.iter().map(|text| T::borrowed(text)).collect()
}

#[cfg(test)]
#[path = "choice_text_original.rs"]
mod choice_text_original;

#[cfg(test)]
#[path = "choice_text_perf.rs"]
mod choice_text_perf_tests;
