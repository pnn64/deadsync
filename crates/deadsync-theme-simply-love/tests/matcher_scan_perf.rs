//! Production matcher versus the frozen 0.5.1207 implementation.
#[allow(dead_code)]
#[path = "matcher_scan/baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "../src/screens/components/shared/fuzzy.rs"]
mod fuzzy;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

use std::hint::black_box;

fn compare(query: &str, label: &str, aliases: &[&str]) {
    let old = baseline::prepare_query(query);
    let new = fuzzy::prepare_query(query);
    assert_eq!(
        fuzzy::best_match_score(&new, label, aliases),
        baseline::best_match_score(&old, label, aliases),
        "{query:?} / {label:?} / {aliases:?}"
    );
    assert_eq!(
        fuzzy::subsequence_score(new.chars(), label),
        baseline::subsequence_score(old.chars(), label),
        "direct {query:?} / {label:?}"
    );
}

#[test]
fn ascii_search_preserves_every_byte_boundary_and_short_long_crossover() {
    for previous in 0..=127u8 {
        for matched in 0..=127u8 {
            let query = char::from(matched).to_string();
            let mut bytes = vec![b'~'; 40];
            bytes.extend_from_slice(&[previous, matched, b'!', matched.to_ascii_uppercase()]);
            let candidate = String::from_utf8(bytes).unwrap();
            compare(&query, &candidate, &[]);
        }
    }
    for len in [0, 1, 15, 16, 30, 31, 32, 33, 63, 64, 65, 127, 128, 257] {
        for (query, fragment) in [
            ("aB9", "aB9"),
            ("b9", "AB9"),
            ("!_", "!_"),
            ("z", "abc"),
            ("a", "a"),
            ("aa", "a"),
        ] {
            for pos in 0..=len {
                let mut candidate = "x".repeat(pos);
                candidate.push_str(fragment);
                candidate.push_str(&"x".repeat(len - pos));
                compare(query, &candidate, &[]);
            }
        }
    }
}

#[test]
fn unicode_suffix_count_preserves_scores_and_exact_character_penalties() {
    let fragments = [
        "\u{65e5}",
        "\u{30ac}",
        "\u{d55c}",
        "\u{1f3b5}",
        "\u{421}",
        "\u{3a3}",
        "\u{130}",
        "e\u{301}",
        "\u{2003}",
        "ASCII",
        "\0",
    ];
    for fragment in fragments {
        for count in [0, 1, 7, 8, 9, 31, 32, 80, 256, 4096] {
            let suffix = fragment.repeat(count);
            for prefix in [
                "Speed",
                "\u{421}\u{43a}\u{43e}\u{440}",
                "\u{65e5}\u{672c}",
                "\u{130}",
            ] {
                let label = format!("{prefix} {suffix}");
                compare(prefix, &label, &[]);
                compare("absent", &label, &[]);
            }
        }
    }
}

#[test]
fn typo_budget_keeps_later_better_words_alias_precedence_and_unicode() {
    for (query, label, expected) in [
        ("abcxefgy", "abcuefgw abcdefgw abcxefgw", Some(39)),
        ("abcxefgy", "abcuefgw abcdefgw", Some(38)),
        (
            "perspextive",
            "Perspective retrospective prospective",
            Some(39),
        ),
    ] {
        compare(query, label, &[]);
        assert_eq!(
            fuzzy::best_match_score(&fuzzy::prepare_query(query), label, &[]),
            expected
        );
    }
    let parts = [
        "abcxefgw", "abcuefgw", "abcdefgw", "abx", "", "abcxefgy", "ABCDEFXW",
    ];
    for a in parts {
        for b in parts {
            for c in parts {
                let label = format!("{a} {b}\t{c}");
                for aliases in [&[][..], &["abcxefgy"][..], &["unrelated", "abcxefgw"][..]] {
                    compare("abcxefgy", &label, aliases);
                }
            }
        }
    }
    for count in [8, 32, 95, 96, 97, 128] {
        let query = format!("{}\u{44f}", "\u{430}".repeat(count));
        let close = format!("{}\u{431}", "\u{430}".repeat(count));
        let shorter = "\u{430}".repeat(count);
        for separator in [" ", "\t", "\u{2003}"] {
            let label = format!("{close}{separator}{}{separator}{close}", shorter.repeat(2));
            compare(&query, &label, &[]);
        }
    }
}

#[test]
fn generated_catalog_keeps_candidate_scores_and_stable_ranking() {
    let parts = [
        "Speed",
        "Music",
        "ABC",
        "09",
        " ",
        "-",
        "_",
        "\u{65e5}",
        "\u{30ac}",
        "\u{421}",
        "\u{1f3b5}",
        "e\u{301}",
        "\u{130}",
        "\u{2003}",
    ];
    let mut seed = 382713925u64;
    let mut labels = Vec::new();
    for n in 0..768 {
        let mut label = String::new();
        for _ in 0..n % 31 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            label.push_str(parts[(seed >> 32) as usize % parts.len()]);
        }
        labels.push(fuzzy::fold_diacritics(&label).into_owned());
    }
    for query in [
        "",
        "s",
        "speed",
        "spmd",
        "musix",
        "abcxefgy",
        "\u{65e5}",
        "\u{30ac}",
        "\u{441}",
        "\u{130}",
        "\u{1f3b5}",
        "\u{301}",
        "09",
        " ",
    ] {
        let old = baseline::prepare_query(query);
        let new = fuzzy::prepare_query(query);
        let mut old_ranked = Vec::new();
        let mut new_ranked = Vec::new();
        for (index, label) in labels.iter().enumerate() {
            let a = baseline::best_match_score(&old, label, &["catalog", "alias"]);
            let b = fuzzy::best_match_score(&new, label, &["catalog", "alias"]);
            assert_eq!(b, a, "{query:?} / {label:?}");
            if let Some(score) = a {
                old_ranked.push((score, index));
            }
            if let Some(score) = b {
                new_ranked.push((score, index));
            }
        }
        old_ranked.sort_by(|a, b| b.0.cmp(&a.0));
        new_ranked.sort_by(|a, b| b.0.cmp(&a.0));
        assert_eq!(new_ranked, old_ranked);
    }
}

#[test]
fn hot_matchers_keep_zero_heap_churn_and_skip_redundant_unicode_spills() {
    for (query, label) in [
        (
            "spr",
            "some remarkably long distant prefix before Speed Rate",
        ),
        ("absent", "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"),
        (
            "\u{65e5}",
            "\u{65e5}\u{672c}\u{8a9e} with a long trailing suffix",
        ),
        ("perspextive", "Perspective retrospective prospective"),
    ] {
        let query = fuzzy::prepare_query(query);
        perf::assert_no_churn(|| {
            black_box(fuzzy::best_match_score(&query, label, &[]));
        });
    }
    let query = fuzzy::prepare_query(&("\u{430}".repeat(96) + "\u{44f}"));
    let close = "\u{430}".repeat(96) + "\u{431}";
    let label = std::iter::repeat_n(close.as_str(), 16)
        .collect::<Vec<_>>()
        .join(" ");
    // The first near-match spills once; the remaining 15 words are never folded.
    perf::assert_churn_budget(1, 1024, || {
        black_box(fuzzy::best_match_score(&query, &label, &[]));
    });
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut measure_old =
        || perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    let mut measure_new =
        || perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        measure_new();
        measure_old();
    } else {
        measure_old();
        measure_new();
    }
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_matcher_scan() {
    let mut cases: Vec<(String, String, String)> = Vec::new();
    for len in [16usize, 31, 32, 64, 128, 512] {
        for (kind, query, label) in [
            (
                "prefix",
                "speed",
                format!("Speed {}", "a".repeat(len.saturating_sub(6))),
            ),
            (
                "sparse",
                "spr",
                format!("S{}p{}R", "a".repeat(len / 2), "b".repeat(len / 2)),
            ),
            ("missing", "xyz", "a".repeat(len)),
            ("dense", "aaaaaa", "a".repeat(len)),
        ] {
            cases.push((format!("ascii_{kind}_{len}"), query.into(), label));
        }
    }
    for count in [8, 32, 128, 4096] {
        cases.push((
            format!("unicode_prefix_{count}"),
            "\u{65e5}".into(),
            "\u{65e5}".repeat(count),
        ));
        cases.push((
            format!("unicode_late_{count}"),
            "\u{65e5}".into(),
            "\u{672c}".repeat(count) + "\u{65e5}",
        ));
    }
    for count in [2, 8, 32] {
        cases.push((
            format!("typo_words_{count}"),
            "perspextive".into(),
            std::iter::repeat_n("Perspective", count)
                .collect::<Vec<_>>()
                .join(" "),
        ));
        cases.push((
            format!("typo_improving_{count}"),
            "abcxefgy".into(),
            format!(
                "{} abcxefgw",
                std::iter::repeat_n("abcuefgw", count)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ));
    }
    cases.push((
        "typo_single_control".into(),
        "prespective".into(),
        "Perspective".into(),
    ));
    cases.push(("alias_control".into(), "arrows".into(), "NoteSkin".into()));
    let long = "\u{430}".repeat(96) + "\u{431}";
    cases.push((
        "typo_unicode_spill".into(),
        "\u{430}".repeat(96) + "\u{44f}",
        std::iter::repeat_n(long.as_str(), 16)
            .collect::<Vec<_>>()
            .join(" "),
    ));
    for (name, query, label) in &cases {
        let old = baseline::prepare_query(query);
        let new = fuzzy::prepare_query(query);
        let aliases = if name == "alias_control" {
            &["arrows"][..]
        } else {
            &[][..]
        };
        assert_eq!(
            fuzzy::best_match_score(&new, label, aliases),
            baseline::best_match_score(&old, label, aliases)
        );
        pair(
            name,
            if label.len() > 1000 { 2_000 } else { 10_000 },
            1,
            || baseline::best_match_score(black_box(&old), black_box(label), black_box(aliases)),
            || fuzzy::best_match_score(black_box(&new), black_box(label), black_box(aliases)),
        );
    }
    let catalog: Vec<_> = (0..4096)
        .map(|i| format!("Electronic Music Volume {i:04} - An Extended Remix"))
        .collect();
    for (name, query) in [
        ("catalog_sparse", "emr"),
        ("catalog_prefix", "electronic"),
        ("catalog_missing", "zzzz"),
    ] {
        let old = baseline::prepare_query(query);
        let new = fuzzy::prepare_query(query);
        pair(
            name,
            32,
            catalog.len(),
            || {
                let mut sum = 0i64;
                for title in black_box(&catalog) {
                    sum += i64::from(
                        baseline::best_match_score(black_box(&old), black_box(title), &[])
                            .unwrap_or(-1),
                    );
                }
                sum
            },
            || {
                let mut sum = 0i64;
                for title in black_box(&catalog) {
                    sum += i64::from(
                        fuzzy::best_match_score(black_box(&new), black_box(title), &[])
                            .unwrap_or(-1),
                    );
                }
                sum
            },
        );
    }
}
