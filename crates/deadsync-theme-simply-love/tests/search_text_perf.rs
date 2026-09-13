//! Production search text with frozen 0.5.1202 behavior and paired benchmarks.
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[allow(dead_code)]
#[path = "../src/screens/components/shared/fuzzy.rs"]
pub(crate) mod fuzzy;

mod screens {
    pub mod components {
        pub mod shared {
            pub(crate) use crate::fuzzy;
        }
    }
}

#[path = "../src/screens/player_options/search_text.rs"]
mod search_text;

use search_text::SearchResultText;
use std::hint::black_box;
use std::sync::Arc;

// Frozen from 91b7bcfca's search.rs; inputs replace state/row access only.
fn old_completion(query: &str, label: &str) -> Option<(String, String)> {
    if query.is_empty() {
        return None;
    }
    let consumed = fuzzy::folded_prefix_len(query, label)?;
    (label.chars().count() > consumed).then(|| {
        let prefix: String = label.chars().take(consumed).collect();
        (label.to_string(), prefix)
    })
}

fn old_help(lines: &[Arc<str>]) -> Option<String> {
    let text = lines
        .iter()
        .map(|line| line.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() { None } else { Some(text) }
}

fn lines(raw: &[&str]) -> Vec<Arc<str>> {
    raw.iter().map(|line| Arc::from(*line)).collect()
}

#[test]
fn completion_preserves_original_unicode_boundaries_and_absence() {
    let labels = [
        "Speed Mod",
        "Music Rate",
        "D\u{e9}j\u{e0} Vu",
        "De\u{301}ja\u{300} Vu",
        "\u{421}\u{43a}\u{43e}\u{440}\u{43e}\u{441}\u{442}\u{44c}",
        "\u{65e5}\u{672c}\u{8a9e} Skin",
        "\u{130}stanbul",
        "\u{301}abc\u{301}",
        "",
        "\u{301}",
        "a",
        "a\u{301}",
        "  spaced  label ",
    ];
    let queries = [
        "",
        "s",
        "SPE",
        "Speed Mod",
        "Speed Mod Extra",
        "arrows",
        "music",
        "deja",
        "d\u{e9}j\u{e0}",
        "de",
        "\u{421}\u{43a}\u{43e}\u{440}",
        "\u{65e5}",
        "\u{301}",
        "a",
        "a\u{301}",
        "  ",
    ];
    for label in labels {
        let label: Arc<str> = Arc::from(label);
        for query in queries {
            let expected = old_completion(query, &label);
            let text = SearchResultText::default();
            for _ in 0..3 {
                let actual = text.completion(query, &label);
                assert_eq!(
                    actual
                        .as_ref()
                        .map(|(full, prefix)| (full.as_ref(), prefix.as_ref())),
                    expected
                        .as_ref()
                        .map(|(full, prefix)| (full.as_str(), prefix.as_str())),
                    "query {query:?}, label {label:?}"
                );
                if let Some((full, _)) = actual {
                    assert!(Arc::ptr_eq(&full, &label));
                }
            }
            perf::assert_no_churn(|| {
                black_box(text.completion(query, &label));
            });
        }
    }
}

#[test]
fn help_preserves_trim_join_and_none_semantics() {
    let fixtures: &[&[&str]] = &[
        &[],
        &[""],
        &[" ", "\t\n", "\u{2003}"],
        &["A single help line."],
        &["  trimmed line\t"],
        &[" first ", "", "\t second\n", " third"],
        &[
            "\u{2003}\u{65e5}\u{672c}\u{8a9e}  ",
            "  D\u{e9}j\u{e0} vu\u{2003}",
        ],
        &["first\ninside", "literal\\nline"],
    ];
    for raw in fixtures {
        let lines = lines(raw);
        let text = SearchResultText::default();
        let expected = old_help(&lines);
        for _ in 0..3 {
            assert_eq!(text.help(lines.iter()).as_deref(), expected.as_deref());
        }
        perf::assert_no_churn(|| {
            black_box(text.help(lines.iter()));
        });
    }
}

#[test]
fn single_untrimmed_help_shares_the_row_allocation_even_on_first_focus() {
    let lines = lines(&[
        "",
        "A long help line retained by the immutable option row.",
        "\t",
    ]);
    let text = SearchResultText::default();
    perf::assert_no_churn(|| {
        let help = text.help(lines.iter()).unwrap();
        assert!(Arc::ptr_eq(&help, &lines[1]));
    });
}

#[test]
fn long_completion_retains_its_prefix_allocation() {
    let label: Arc<str> = Arc::from("Multilingual \u{65e5}\u{672c}\u{8a9e} ".repeat(64));
    let query = "Multilingual \u{65e5}\u{672c}\u{8a9e} ".repeat(32);
    let text = SearchResultText::default();
    let first = text.completion(&query, &label).unwrap();
    let second = text.completion(&query, &label).unwrap();
    assert!(Arc::ptr_eq(&first.1, &second.1));
    assert_eq!(first.1.as_ref(), query);
    perf::assert_no_churn(|| {
        black_box(text.completion(&query, &label));
    });
}

#[test]
fn help_and_completion_cells_are_independent_and_clones_share_text() {
    let lines = lines(&["One", "Two"]);
    let label: Arc<str> = Arc::from("Speed Mod");
    let text = SearchResultText::default();
    let help = text.help(lines.iter()).unwrap();
    let ghost = text.completion("spe", &label).unwrap();
    perf::assert_no_churn(|| {
        let cloned = text.clone();
        assert!(Arc::ptr_eq(&help, &cloned.help(lines.iter()).unwrap()));
        assert!(Arc::ptr_eq(
            &ghost.1,
            &cloned.completion("spe", &label).unwrap().1
        ));
    });
}

#[test]
fn help_spill_preserves_text_and_reuses_the_final_allocation() {
    for bytes in [245, 246, 247, 255, 256, 257, 1024, 4096] {
        let long = "x".repeat(bytes);
        let lines = lines(&[&long, "\u{65e5}\u{672c}\u{8a9e}"]);
        let text = SearchResultText::default();
        assert_eq!(
            text.help(lines.iter()).as_deref(),
            old_help(&lines).as_deref()
        );
        perf::assert_no_churn(|| {
            black_box(text.help(lines.iter()));
        });
        perf::assert_churn_budget(
            if bytes <= 246 { 1 } else { 2 },
            2 * (bytes + 10) + 24,
            || {
                black_box(SearchResultText::default().help(lines.iter()));
            },
        );
    }
}

#[test]
fn short_multiline_help_only_allocates_the_final_shared_text() {
    let lines = lines(&[" first ", "", " second ", " third "]);
    perf::assert_churn_budget(1, 40, || {
        black_box(SearchResultText::default().help(lines.iter()));
    });
}

#[test]
#[ignore = "manual release benchmark; use --ignored --nocapture --test-threads=1"]
fn benchmark_search_text() {
    eprintln!(
        "SearchResultText: {} inline bytes per result",
        std::mem::size_of::<SearchResultText>()
    );
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let long_label = "Multilingual \u{65e5}\u{672c}\u{8a9e} ".repeat(64);
    let long_query = "Multilingual \u{65e5}\u{672c}\u{8a9e} ".repeat(32);
    for (name, query, raw_label) in [
        ("short", "spe", "Speed Mod"),
        ("unicode", "deja", "De\u{301}ja\u{300} Vu"),
        ("long", long_query.as_str(), long_label.as_str()),
        ("alias", "arrows", "NoteSkin"),
        ("exact", "Speed Mod", "Speed Mod"),
        ("empty", "", "Speed Mod"),
    ] {
        let label: Arc<str> = Arc::from(raw_label);
        for warm in [false, true] {
            if !warm && matches!(name, "alias" | "exact" | "empty") {
                continue;
            }
            let state = SearchResultText::default();
            if warm {
                black_box(state.completion(query, &label));
            }
            let name = format!("ghost_{name}_{}", if warm { "warm" } else { "cold" });
            let iterations = if raw_label.len() > 100 {
                4_000
            } else {
                100_000
            };
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                if old {
                    perf::measure_sampled(&format!("{name}/old"), iterations, 1, || {
                        old_completion(black_box(query), black_box(&label))
                    });
                } else if warm {
                    perf::measure_sampled(&format!("{name}/new"), iterations, 1, || {
                        black_box(&state).completion(black_box(query), black_box(&label))
                    });
                } else {
                    perf::measure_sampled(&format!("{name}/new"), iterations, 1, || {
                        SearchResultText::default().completion(black_box(query), black_box(&label))
                    });
                }
            }
        }
    }
    let long_help = "A longer localized help line. ".repeat(32);
    for (name, raw) in [
        (
            "single",
            vec!["Control how much the background is darkened while playing."],
        ),
        (
            "multiline",
            vec![
                " Choose a speed modifier. ",
                "",
                " CMod uses a constant speed regardless of BPM changes. ",
                " MMod follows the chart's maximum BPM. ",
            ],
        ),
        (
            "unicode",
            vec![
                "\u{2003}\u{65e5}\u{672c}\u{8a9e}\u{2003}",
                "  D\u{e9}j\u{e0} vu ",
                " De\u{301}ja\u{300} Vu  ",
            ],
        ),
        ("empty", vec!["", " \t", "\u{2003}"]),
        (
            "long",
            vec![
                long_help.as_str(),
                " A second line after the scratch buffer spills. ",
            ],
        ),
    ] {
        let lines = lines(&raw);
        for warm in [false, true] {
            let state = SearchResultText::default();
            if warm {
                black_box(state.help(lines.iter()));
            }
            let name = format!("help_{name}_{}", if warm { "warm" } else { "cold" });
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                if old {
                    perf::measure_sampled(&format!("{name}/old"), 50_000, 1, || {
                        old_help(black_box(&lines))
                    });
                } else if warm {
                    perf::measure_sampled(&format!("{name}/new"), 50_000, 1, || {
                        black_box(&state).help(black_box(&lines).iter())
                    });
                } else {
                    perf::measure_sampled(&format!("{name}/new"), 50_000, 1, || {
                        SearchResultText::default().help(black_box(&lines).iter())
                    });
                }
            }
        }
    }
    let label: Arc<str> = Arc::from("Speed Mod");
    let lines = lines(&[
        " Choose a speed modifier. ",
        " CMod uses a constant speed. ",
        " MMod follows the chart's maximum BPM. ",
    ]);
    for old in if reverse {
        [false, true]
    } else {
        [true, false]
    } {
        if old {
            perf::measure_sampled("search_focus_60frames/old", 4_000, 60, || {
                for _ in 0..60 {
                    black_box(old_completion(black_box("spe"), black_box(&label)));
                    black_box(old_help(black_box(&lines)));
                }
            });
        } else {
            perf::measure_sampled("search_focus_60frames/new", 4_000, 60, || {
                let state = SearchResultText::default();
                for _ in 0..60 {
                    black_box(black_box(&state).completion(black_box("spe"), black_box(&label)));
                    black_box(black_box(&state).help(black_box(&lines).iter()));
                }
            });
        }
    }
}
