//! Paired text-path benchmarks plus behavior and allocation regression checks.
#[allow(dead_code)]
#[path = "../src/i18n_runtime.rs"]
mod i18n_runtime;
mod i18n {
    pub use crate::i18n_runtime::LookupKey;
}
#[path = "option_frame/baseline_text.rs"]
mod baseline;
#[path = "../src/screens/options/choice_text.rs"]
mod choice_text;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
#[path = "../src/screens/player_options/search_frame.rs"]
mod search_frame;

use choice_text::{Choice, choice_texts, string_choice_texts};
use deadlib_present::actors::TextContent;
use i18n_runtime::lookup_key;
use search_frame::{SearchCurrentText, SearchQueryText};
use std::hint::black_box;
use std::sync::{Arc, Mutex};

static LANGUAGE_TEST: Mutex<()> = Mutex::new(());

#[test]
fn current_text_tracks_values_templates_and_snapshot_lifetimes_without_warm_churn() {
    let cache = SearchCurrentText::default();
    for template in [
        "Current: {value}",
        "Valeur : {value}",
        "{value}/{value}",
        "fixed",
        "{unknown} {value}",
        "",
    ] {
        let template: Arc<str> = Arc::from(template);
        for text in [
            "",
            "One",
            "{value}",
            "{other}",
            "\u{65e5}\u{672c}\u{8a9e}",
            &"long".repeat(130),
        ] {
            let value = TextContent::from(Arc::<str>::from(text));
            let expected = baseline::current(&value, &template);
            let output = cache.get(&value, &template);
            assert_eq!(output, expected);
            perf::assert_no_churn(|| {
                black_box(cache.get(&value, &template));
            });
            assert!(Arc::ptr_eq(&output, &cache.get(&value, &template)));
            let cloned = cache.clone();
            assert!(Arc::ptr_eq(&output, &cloned.get(&value, &template)));
            let replacement: Arc<str> = Arc::from("Changed: {value}");
            assert_eq!(
                cache.get(&value, &replacement),
                baseline::current(&value, &replacement)
            );
            assert_eq!(
                output, expected,
                "already-built actors retain their original text"
            );
            assert_eq!(cloned.get(&value, &template), expected);
        }
    }
    let template = Arc::from("Current: {value}");
    let value = TextContent::inline_format(format_args!("100%")).unwrap();
    perf::assert_churn_budget(1, 64, || {
        black_box(SearchCurrentText::default().get(&value, &template));
    });
    let owned = TextContent::from(String::from("Owned choice"));
    assert_eq!(
        cache.get(&owned, &template),
        baseline::current(&owned, &template)
    );
}

#[test]
fn query_caret_preserves_utf8_boundaries_edits_and_actor_lifetimes() {
    let mut cache = SearchQueryText::default();
    let queries: Vec<_> = [0, 1, 11, 12, 14, 15, 16, 253, 254, 256, 512]
        .into_iter()
        .map(|len| "q".repeat(len))
        .chain([
            "\u{65e5}\u{672c}\u{8a9e}".repeat(30),
            "De\u{301}ja\u{300}".into(),
            "line\nnext".into(),
        ])
        .collect();
    for query in &queries {
        cache.invalidate();
        for caret_on in [true, false, true, false] {
            let actual = cache.get(query, caret_on);
            assert_eq!(actual.as_str(), baseline::query(query, caret_on).as_str());
            perf::assert_no_churn(|| {
                black_box(cache.get(query, caret_on));
            });
        }
        let retained = cache.get(query, true);
        let cloned = cache.clone();
        cache.invalidate();
        assert_eq!(cache.get("Edited", true).as_str(), "Edited\u{25ae}");
        assert_eq!(retained.as_str(), baseline::query(query, true).as_str());
        assert_eq!(cloned.get(query, true).as_str(), retained.as_str());
    }
    for query in ["arrows", "12345678901", "\u{65e5}\u{672c}\u{8a9e}"] {
        perf::assert_no_churn(|| {
            black_box(SearchQueryText::default().get(query, true));
        });
    }
}

#[test]
fn menu_labels_keep_translations_literals_and_dynamic_string_ownership() {
    let _guard = LANGUAGE_TEST.lock().unwrap();
    let labels = [
        Choice::Localized(lookup_key("Common", "On")),
        Choice::Localized(lookup_key("Common", "Off")),
        Choice::Literal("16:9"),
        Choice::Literal(""),
        Choice::Literal("\u{65e5}\u{672c}\u{8a9e}"),
    ];
    let old = baseline::choices(&labels);
    let new = Arc::<[Arc<str>]>::from(choice_texts::<Arc<str>>(&labels));
    assert_eq!(new, old);
    for index in [0, 1] {
        assert!(Arc::ptr_eq(&new[index], &labels[index].get()));
    }
    perf::assert_churn_budget(2, 128, || {
        black_box(Arc::<[Arc<str>]>::from(choice_texts::<Arc<str>>(
            &labels[..2],
        )));
    });
    let input: Vec<String> = vec!["".into(), "Skin".repeat(100), "A\u{301}".into()];
    let expected = baseline::strings(&input);
    let output = Arc::<[Arc<str>]>::from(string_choice_texts::<Arc<str>>(&input));
    drop(input);
    assert_eq!(output, expected);
    assert!(choice_texts::<Arc<str>>(&[]).is_empty());
    assert!(string_choice_texts::<Arc<str>>(&[]).is_empty());
}

// All measurements count the returned actor/text's destruction too. The warm
// caches stay alive outside that scope; lifecycle workloads include their drop.
fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
        perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    } else {
        perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
        perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    }
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_option_frame() {
    let _guard = LANGUAGE_TEST.lock().unwrap();
    let template: Arc<str> = Arc::from("Current: {value}");
    for (name, raw) in [
        ("short", "100%".to_owned()),
        ("unicode", "\u{65e5}\u{672c}\u{8a9e} Skin".to_owned()),
        ("long", "Skin".repeat(130)),
    ] {
        let value = TextContent::inline_format(format_args!("{raw}"))
            .unwrap_or_else(|| TextContent::from(Arc::<str>::from(raw.as_str())));
        let cache = SearchCurrentText::default();
        black_box(cache.get(&value, &template));
        pair(
            &format!("current_{name}_warm"),
            20_000,
            1,
            || baseline::current(black_box(&value), black_box(&template)),
            || cache.get(black_box(&value), black_box(&template)),
        );
        pair(
            &format!("current_{name}_cold"),
            10_000,
            1,
            || baseline::current(black_box(&value), black_box(&template)),
            || SearchCurrentText::default().get(black_box(&value), black_box(&template)),
        );
    }
    let values = [
        TextContent::from(Arc::<str>::from("100%")),
        TextContent::from(Arc::<str>::from("150%")),
    ];
    pair(
        "current_60_frames_two_values",
        1_000,
        60,
        || {
            for frame in 0..60 {
                black_box(baseline::current(
                    black_box(&values[frame / 30]),
                    black_box(&template),
                ));
            }
        },
        || {
            let cache = SearchCurrentText::default();
            for frame in 0..60 {
                black_box(cache.get(black_box(&values[frame / 30]), black_box(&template)));
            }
        },
    );
    let actual_template = i18n_runtime::tr("PlayerOptions", "SettingSearchCurrent");
    let translated_cache = SearchCurrentText::default();
    black_box(translated_cache.get(&values[0], &actual_template));
    pair(
        "current_translated_warm",
        20_000,
        1,
        || {
            baseline::current(
                black_box(&values[0]),
                &i18n_runtime::tr("PlayerOptions", "SettingSearchCurrent"),
            )
        },
        || {
            translated_cache.get(
                black_box(&values[0]),
                &i18n_runtime::tr("PlayerOptions", "SettingSearchCurrent"),
            )
        },
    );
    pair(
        "current_60_frames_changing_every_frame",
        1_000,
        60,
        || {
            for frame in 0..60 {
                black_box(baseline::current(
                    black_box(&values[frame % 2]),
                    black_box(&template),
                ));
            }
        },
        || {
            let cache = SearchCurrentText::default();
            for frame in 0..60 {
                black_box(cache.get(black_box(&values[frame % 2]), black_box(&template)));
            }
        },
    );
    let edits: Vec<_> = (1..=30).map(|n| "q".repeat(n)).collect();
    pair(
        "query_30_edits_ten_frames_each",
        500,
        300,
        || {
            for query in &edits {
                for _ in 0..10 {
                    black_box(baseline::query(black_box(query), black_box(true)));
                }
            }
        },
        || {
            let mut cache = SearchQueryText::default();
            for query in &edits {
                cache.invalidate();
                for _ in 0..10 {
                    black_box(cache.get(black_box(query), black_box(true)));
                }
            }
        },
    );
    for (name, query) in [
        ("short", "arrows".to_owned()),
        ("boundary", "123456789012".to_owned()),
        ("long", "no matching setting in these options".to_owned()),
        ("huge", "\u{65e5}\u{672c}\u{8a9e}".repeat(60)),
    ] {
        let cache = SearchQueryText::default();
        black_box(cache.get(&query, true));
        black_box(cache.get(&query, false));
        pair(
            &format!("query_{name}_warm"),
            20_000,
            1,
            || baseline::query(black_box(&query), black_box(true)),
            || cache.get(black_box(&query), black_box(true)),
        );
        pair(
            &format!("query_{name}_cold"),
            10_000,
            1,
            || baseline::query(black_box(&query), black_box(true)),
            || SearchQueryText::default().get(black_box(&query), black_box(true)),
        );
        pair(
            &format!("query_{name}_60_frames"),
            1_000,
            60,
            || {
                for frame in 0..60 {
                    black_box(baseline::query(black_box(&query), black_box(frame < 30)));
                }
            },
            || {
                let cache = SearchQueryText::default();
                for frame in 0..60 {
                    black_box(cache.get(black_box(&query), black_box(frame < 30)));
                }
            },
        );
    }
    for count in [2, 8, 128] {
        for (kind, labels) in [
            (
                "localized",
                vec![Choice::Localized(lookup_key("Common", "On")); count],
            ),
            ("literal", vec![Choice::Literal("1920x1080"); count]),
            (
                "mixed",
                (0..count)
                    .map(|i| {
                        if i % 2 == 0 {
                            Choice::Localized(lookup_key("Common", "Off"))
                        } else {
                            Choice::Literal("16:9")
                        }
                    })
                    .collect(),
            ),
        ] {
            black_box(choice_texts::<Arc<str>>(&labels)); // language initialization is outside timing
            pair(
                &format!("menu_{kind}_{count}"),
                2_000,
                count,
                || baseline::choices(black_box(&labels)),
                || Arc::<[Arc<str>]>::from(choice_texts::<Arc<str>>(black_box(&labels))),
            );
        }
        let labels: Vec<String> = (0..count)
            .map(|i| format!("Custom Skin {i}: {}", "wide label ".repeat(8)))
            .collect();
        pair(
            &format!("menu_strings_{count}"),
            2_000,
            count,
            || baseline::strings(black_box(&labels)),
            || Arc::<[Arc<str>]>::from(string_choice_texts::<Arc<str>>(black_box(&labels))),
        );
    }
}

#[test]
fn current_details_and_option_labels_follow_locale_changes_and_reload() {
    let _guard = LANGUAGE_TEST.lock().unwrap();
    let cache = SearchCurrentText::default();
    let value = TextContent::from(Arc::<str>::from("100%"));
    let labels = [Choice::Localized(lookup_key("Common", "On"))];
    for locale in ["en", "fr", "en"] {
        i18n_runtime::set_locale(deadsync_assets::language::load_for_tests(locale));
        let template = i18n_runtime::tr("PlayerOptions", "SettingSearchCurrent");
        assert_eq!(
            cache.get(&value, &template),
            baseline::current(&value, &template)
        );
        assert_eq!(
            Arc::<[Arc<str>]>::from(choice_texts::<Arc<str>>(&labels)),
            baseline::choices(&labels)
        );
    }
    // Same-locale resource reload replaces the translation Arc too.
    let mut bundle = deadsync_assets::language::load_for_tests("en");
    bundle
        .active
        .entry(Box::from("PlayerOptions"))
        .or_default()
        .insert(
            Box::from("SettingSearchCurrent"),
            Arc::from("Reloaded: {value}"),
        );
    i18n_runtime::init(bundle);
    let template = i18n_runtime::tr("PlayerOptions", "SettingSearchCurrent");
    assert_eq!(cache.get(&value, &template).as_ref(), "Reloaded: 100%");
    i18n_runtime::init(deadsync_assets::language::load_for_tests("en"));
}

#[test]
fn numeric_input_choices_keep_the_original_single_string_allocation() {
    use choice_text::ChoiceText;
    use std::borrow::Cow;
    for value in [0, 1, 12345, u32::MAX] {
        perf::assert_churn_budget(1, 32, || {
            black_box(<Cow<'static, str> as ChoiceText>::owned(
                black_box(value).to_string(),
            ));
        });
        let text = value.to_string();
        let shared = <Arc<str> as ChoiceText>::owned(text.clone());
        assert_eq!(shared.as_ref(), text);
    }
}
