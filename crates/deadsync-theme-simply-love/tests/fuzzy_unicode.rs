//! Frozen old implementations versus the production public fuzzy-search API.
use deadsync_theme_simply_love::screens::components::shared::fuzzy;
use std::borrow::Cow;
use std::hint::black_box;

#[path = "fuzzy_unicode/baseline.rs"]
#[allow(dead_code)]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn generated_texts() -> Vec<String> {
    let parts = [
        "a",
        "Z",
        "Speed",
        " ",
        "\t",
        "\n",
        "\0",
        "\u{a0}",
        "\u{e9}",
        "e\u{301}",
        "\u{130}",
        "\u{212a}",
        "\u{212b}",
        "\u{65e5}",
        "\u{30ac}",
        "\u{30ab}\u{3099}",
        "\u{d55c}",
        "\u{3a3}",
        "\u{421}",
        "\u{1f3b5}",
        "\u{300}",
        "\u{36f}",
        "\u{2003}",
        "[12]",
        "-",
        "\u{1e69}",
    ];
    let mut state = 196754321u64;
    (0..512)
        .map(|n| {
            let mut out = String::new();
            for _ in 0..n % 49 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                out.push_str(parts[(state >> 32) as usize % parts.len()]);
            }
            out
        })
        .collect()
}

fn assert_fold_and_query(input: &str) {
    let old = baseline::fold_diacritics(input);
    let new = fuzzy::fold_diacritics(input);
    assert_eq!(new, old, "fold {input:?}");
    assert_eq!(
        matches!(new, Cow::Borrowed(_)),
        matches!(old, Cow::Borrowed(_))
    );
    let old = baseline::prepare_query(input);
    let new = fuzzy::prepare_query(input);
    assert_eq!(new.chars(), old.chars(), "query {input:?}");
    assert_eq!(new.is_empty(), old.is_empty());
    assert_eq!(fuzzy::query_chars(input), baseline::query_chars(input));
}

#[test]
fn folding_and_query_preparation_match_for_every_unicode_scalar() {
    let mut buffer = [0u8; 4];
    for code in 0..=0x10ffff {
        if let Some(ch) = char::from_u32(code) {
            assert_fold_and_query(ch.encode_utf8(&mut buffer));
        }
    }
}

#[test]
fn mixed_queries_preserve_whitespace_combining_marks_and_spills() {
    for input in generated_texts() {
        assert_fold_and_query(&input);
    }
    for len in [0, 1, 31, 32, 33, 79, 80, 81, 95, 96, 97, 256] {
        for unit in ["a", "\u{e9}", "\u{65e5}", "e\u{301} "] {
            assert_fold_and_query(&unit.repeat(len));
        }
    }
    for input in [
        "\u{300}\u{36f}",
        "Cafe\u{301}",
        "\u{130}STANBUL",
        "x\u{2003}y",
    ] {
        assert_fold_and_query(input);
    }
}

#[test]
fn search_scores_aliases_and_prefix_splits_match_old_behavior() {
    let mut labels = generated_texts();
    labels.extend([
        "Perspective".into(),
        "Speed Mod".into(),
        "\u{421}\u{43a}\u{43e}\u{440}".into(),
        "\u{65e5}".repeat(300),
    ]);
    for input in [
        "",
        "xyz",
        "prespective",
        "spdm",
        "\u{43a}\u{43e}",
        "\u{e9}",
        "e\u{301}",
        "\u{65e5}",
        "\u{1f3b5}",
        " ",
        "\u{300}",
    ] {
        let old = baseline::prepare_query(input);
        let new = fuzzy::prepare_query(input);
        for label in &labels {
            let folded = baseline::fold_diacritics(label);
            for aliases in [&[][..], &["arrows", "speed", "\u{65e5}"][..]] {
                assert_eq!(
                    fuzzy::best_match_score(&new, &folded, aliases),
                    baseline::best_match_score(&old, &folded, aliases),
                    "{input:?} / {label:?}"
                );
            }
            assert_eq!(
                fuzzy::folded_prefix_len(input, label),
                baseline::folded_prefix_len(input, label),
                "prefix {input:?} / {label:?}"
            );
        }
    }
}

#[test]
fn unicode_typo_length_guard_preserves_threshold_edges_and_long_queries() {
    for qlen in [1usize, 2, 3, 8, 32, 71, 72, 80, 95, 96, 97, 160] {
        let query = "\u{65e5}".repeat(qlen);
        let old = baseline::prepare_query(&query);
        let new = fuzzy::prepare_query(&query);
        let limit = (qlen / 3).max(1);
        for len in [
            qlen.saturating_sub(limit + 1),
            qlen.saturating_sub(limit),
            qlen,
            qlen + limit,
            qlen + limit + 1,
            300,
        ] {
            let mut word = "\u{65e5}".repeat(len);
            // Force the subsequence miss so the typo branch is exercised.
            if len > 0 {
                word.replace_range(..3, "\u{6708}");
            }
            for label in [
                word.clone(),
                format!("{word} tail"),
                format!("prefix {word}"),
                format!("  {word}  "),
            ] {
                assert_eq!(
                    fuzzy::best_match_score(&new, &label, &[]),
                    baseline::best_match_score(&old, &label, &[]),
                    "qlen={qlen}, len={len}"
                );
            }
        }
    }
}

#[test]
fn unicode_search_allocation_budgets() {
    for input in [
        "ASCII",
        "\u{65e5}\u{672c}\u{8a9e}",
        "\u{d55c}\u{ad6d}",
        "Music \u{1f3b5}",
    ] {
        perf::assert_no_churn(|| {
            black_box(fuzzy::fold_diacritics(input));
        });
    }
    for input in ["Caf\u{e9}", "\u{300}tail", "De\u{301}ja\u{300} Vu"] {
        perf::assert_churn_budget(1, input.len(), || {
            black_box(fuzzy::fold_diacritics(input));
        });
    }
    for input in [
        "ASCII",
        "\u{65e5}\u{672c}\u{8a9e}",
        "Caf\u{e9}",
        "e\u{301} \u{e9}",
    ] {
        perf::assert_no_churn(|| {
            black_box(fuzzy::prepare_query(input));
        });
    }
    let query = fuzzy::prepare_query("zzzz");
    let label = "\u{65e5}".repeat(300);
    perf::assert_no_churn(|| {
        black_box(fuzzy::best_match_score(&query, &label, &[]));
    });
}

#[test]
#[ignore = "release CPU/allocation benchmark; --ignored --nocapture --test-threads=1"]
fn fuzzy_unicode_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let order = if reverse {
        [false, true]
    } else {
        [true, false]
    };
    for (case, input) in [
        ("ascii", "Plain ASCII Title".to_string()),
        (
            "japanese",
            "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6b4c}\u{66f2}".repeat(4),
        ),
        (
            "mixed",
            format!("{}\u{1f3b5}", "Plain ASCII Title ".repeat(8)),
        ),
        (
            "accents",
            "D\u{e9}j\u{e0} Vu Caf\u{e9} Se\u{f1}orita".to_string(),
        ),
        ("late", format!("{}Caf\u{e9}", "A plain title ".repeat(8))),
    ] {
        for old in order {
            let run = black_box(if old {
                baseline::fold_diacritics
            } else {
                fuzzy::fold_diacritics
            });
            perf::measure_sampled(
                &format!("fold_{case}_{}", if old { "old" } else { "new" }),
                20000,
                input.chars().count(),
                || run(black_box(&input)),
            );
        }
    }
    for (case, input) in [
        ("ascii", "Speed Mod".to_string()),
        ("accents", "D\u{e9}j\u{e0} Caf\u{e9}".to_string()),
        ("nfd", "De\u{301}ja\u{300} Cafe\u{301}".to_string()),
        (
            "japanese",
            "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6b4c}".to_string(),
        ),
        ("spill", "Caf\u{e9} ".repeat(20)),
    ] {
        for old in order {
            let name = format!("query_{case}_{}", if old { "old" } else { "new" });
            if old {
                let run = black_box(baseline::prepare_query as fn(&str) -> baseline::Query);
                perf::measure_sampled(&name, 20000, 1, || run(black_box(&input)));
            } else {
                let run = black_box(fuzzy::prepare_query as fn(&str) -> fuzzy::Query);
                perf::measure_sampled(&name, 20000, 1, || run(black_box(&input)));
            }
        }
    }
    for (case, query, candidate) in [
        ("ascii_typo", "prespective", "Perspective".to_string()),
        (
            "unicode_typo",
            "\u{441}\u{43a}\u{43e}\u{43b}",
            "\u{421}\u{43a}\u{43e}\u{440}".to_string(),
        ),
        (
            "unicode_reject",
            "zzzz",
            "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6b4c}\u{66f2}".repeat(4),
        ),
        (
            "unicode_long",
            "zzzz",
            "\u{65e5}\u{672c}\u{8a9e}".repeat(100),
        ),
        (
            "unicode_words",
            "zzzz",
            "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6b4c}\u{66f2} ".repeat(40),
        ),
    ] {
        let old_query = baseline::prepare_query(query);
        let new_query = fuzzy::prepare_query(query);
        for old in order {
            let name = format!("match_{case}_{}", if old { "old" } else { "new" });
            if old {
                let run = black_box(
                    baseline::best_match_score
                        as fn(&baseline::Query, &str, &[&str]) -> Option<i32>,
                );
                perf::measure_sampled(&name, 5000, 1, || {
                    run(black_box(&old_query), black_box(&candidate), black_box(&[]))
                });
            } else {
                let run = black_box(
                    fuzzy::best_match_score as fn(&fuzzy::Query, &str, &[&str]) -> Option<i32>,
                );
                perf::measure_sampled(&name, 5000, 1, || {
                    run(black_box(&new_query), black_box(&candidate), black_box(&[]))
                });
            }
        }
    }
}
