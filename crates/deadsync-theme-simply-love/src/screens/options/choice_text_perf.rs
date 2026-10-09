use super::choice_text_original as original;
use super::*;
use crate::perf::measure;
use std::hint::black_box;

const LITERALS: [&str; 8] = [
    "4:3",
    "16:9",
    "1920x1080",
    "60Hz",
    "125%",
    "\u{6771}\u{4eac}",
    "{literal}",
    "line\nbreak",
];

fn choices(count: usize, mode: &str) -> Vec<Choice> {
    (0..count)
        .map(|i| {
            if mode == "localized" || (mode == "mixed" && i % 2 == 0) {
                Choice::Localized(crate::i18n::lookup_key(
                    "Common",
                    if i % 3 == 0 { "Default" } else { "Auto" },
                ))
            } else {
                Choice::Literal(LITERALS[i % LITERALS.len()])
            }
        })
        .collect()
}

#[test]
fn literal_choices_preserve_input_and_display_text_and_shared_translations() {
    for count in [0, 1, 8, 65] {
        for mode in ["literal", "localized", "mixed"] {
            let mut values = choices(count, mode);
            values.extend([
                Choice::Literal(""),
                Choice::Literal("\u{e9}\u{1f642}\u{301}"),
                Choice::Localized(crate::i18n::lookup_key("MissingPerfSection", "MissingKey")),
            ]);
            let old: Vec<Cow<'static, str>> = original::choice_texts(&values);
            let new: Vec<Cow<'static, str>> = choice_texts(&values);
            assert_eq!(new, old);
            for (choice, text) in values.iter().zip(&new) {
                if let Choice::Literal(expected) = choice {
                    assert!(matches!(text, Cow::Borrowed(_)));
                    assert!(std::ptr::eq(text.as_ref().as_ptr(), expected.as_ptr()));
                }
            }
            let old: Vec<Arc<str>> = original::choice_texts(&values);
            let new: Vec<Arc<str>> = choice_texts(&values);
            assert_eq!(new, old);
            for ((choice, old), new) in values.iter().zip(&old).zip(&new) {
                if let Choice::Localized(key) = choice
                    && key.section != "MissingPerfSection"
                {
                    assert!(Arc::ptr_eq(old, new));
                }
            }
        }
    }
}

#[test]
fn borrowed_literal_choices_still_support_owned_mutation_and_dynamic_choices() {
    let values = [
        Choice::Literal("16:9"),
        Choice::Literal(""),
        Choice::Literal("\u{6771}\u{4eac}"),
    ];
    let mut new: Vec<Cow<'static, str>> = choice_texts(&values);
    let old: Vec<Cow<'static, str>> = original::choice_texts(&values);
    for value in &mut new {
        value.to_mut().push('!');
    }
    for (new, old) in new.iter().zip(&old) {
        assert_eq!(new.as_ref(), format!("{old}!"));
    }
    let dynamic = vec!["dynamic".to_owned(), "\u{e9}\u{1f642}".to_owned()];
    let owned: Vec<Cow<'static, str>> = string_choice_texts(&dynamic);
    let shared: Vec<Arc<str>> = string_choice_texts(&dynamic);
    drop(dynamic);
    assert_eq!(
        owned.iter().map(AsRef::<str>::as_ref).collect::<Vec<_>>(),
        ["dynamic", "\u{e9}\u{1f642}"]
    );
    assert_eq!(
        shared.iter().map(AsRef::<str>::as_ref).collect::<Vec<_>>(),
        ["dynamic", "\u{e9}\u{1f642}"]
    );
}

#[test]
fn literal_choices_remove_arc_and_string_roundtrips_only_from_input_values() {
    for count in [0, 1, 8, 64] {
        for mode in ["literal", "localized", "mixed"] {
            let values = choices(count, mode);
            let _: Vec<Cow<'static, str>> = choice_texts(&values);
            let (before, old) = measure(|| original::choice_texts::<Cow<'static, str>>(&values));
            let (after, new) = measure(|| choice_texts::<Cow<'static, str>>(&values));
            assert_eq!(after, before);
            let literals = values
                .iter()
                .filter(|value| matches!(value, Choice::Literal(_)))
                .count();
            assert_eq!(old.allocs - new.allocs, 2 * literals);
            assert_eq!(old.reallocs, new.reallocs);
            assert!(new.allocated_bytes <= old.allocated_bytes);
            let (before, old) = measure(|| original::choice_texts::<Arc<str>>(&values));
            let (after, new) = measure(|| choice_texts::<Arc<str>>(&values));
            assert_eq!(after, before);
            assert_eq!(new, old);
        }
    }
}

fn bench<T: ChoiceText + AsRef<str> + PartialEq + std::fmt::Debug>(label: &str, values: &[Choice]) {
    let _: Vec<T> = choice_texts(values);
    let (before, old) = measure(|| original::choice_texts::<T>(values));
    let (after, new) = measure(|| choice_texts::<T>(values));
    assert_eq!(after, before);
    println!("{label} churn: original {old:?}, current {new:?}");
    crate::paired_bench::compare(
        label,
        100,
        || {
            black_box(original::choice_texts::<T>(black_box(values)));
        },
        || {
            black_box(choice_texts::<T>(black_box(values)));
        },
    );
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_literal_choices() {
    for (count, mode) in [
        (0, "literal"),
        (1, "literal"),
        (8, "literal"),
        (64, "literal"),
        (32, "mixed"),
        (32, "localized"),
    ] {
        let values = choices(count, mode);
        bench::<Cow<'static, str>>(&format!("choices-{count}-{mode}-input"), &values);
        bench::<Arc<str>>(&format!("choices-{count}-{mode}-display"), &values);
    }
}
