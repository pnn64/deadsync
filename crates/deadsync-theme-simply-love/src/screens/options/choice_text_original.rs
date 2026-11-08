// Frozen function from the starting main commit.
use super::*;

pub(super) fn choice_texts<T: ChoiceText>(choices: &[Choice]) -> Vec<T> {
    choices
        .iter()
        .map(|choice| T::shared(choice.get()))
        .collect()
}
