//! Production search helpers and frozen parent scoring, with paired benchmarks.
#[allow(dead_code)]
#[path = "search_ranking/baseline_fuzzy.rs"]
mod baseline_fuzzy;
#[path = "search_ranking/baseline_labels.rs"]
mod baseline_labels;
#[allow(dead_code)]
#[path = "../src/screens/components/shared/fuzzy.rs"]
pub(crate) mod fuzzy;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
mod screens {
    pub mod components {
        pub mod shared {
            pub(crate) use crate::fuzzy;
        }
    }
}
#[path = "../src/screens/player_options/search_ranking.rs"]
mod search_ranking;

use search_ranking::{SearchRowText, matched_setting_label};
use std::hint::black_box;
use std::sync::Arc;

#[test]
fn labels_keep_cleaning_scores_aliases_and_shared_ownership() {
    let raw_labels = [
        "Speed Mod",
        " Music Rate\\nbpm: {bpm}",
        "\tMusic Rate\nbpm: {bpm}",
        "Name{value}\\nsuffix",
        "{only_template}",
        "",
        " \u{2003}\t",
        "D\u{e9}j\u{e0} Vu",
        "De\u{301}ja\u{300} Vu",
        "\u{65e5}\u{672c}\u{8a9e} Skin",
        "Line\r\nDetail",
        "Back\\slash",
        "Back\\nName\nFirst{value}",
    ];
    for raw in raw_labels {
        let label: Arc<str> = Arc::from(raw);
        for text in [
            "", "spe", "music", "deja", "skin", "arrows", "missing", "\u{2003}",
        ] {
            let query = fuzzy::prepare_query(text);
            for aliases in [&[][..], &["arrows", "speed", "cmod"][..]] {
                let old = baseline_labels::matched_setting_label(&query, &label, aliases);
                let new = matched_setting_label(&query, &label, aliases);
                assert_eq!(new, old, "{raw:?} / {text:?}");
                if let Some((matched, _)) = new {
                    if matched.as_ref() == raw {
                        assert!(Arc::ptr_eq(&matched, &label));
                    }
                }
            }
        }
    }
    for (text, raw, aliases) in [
        ("speed", "Speed Mod", &[][..]),
        ("missing", "Speed Mod", &[][..]),
        ("arrows", "NoteSkin", &["arrows"][..]),
        ("", "Speed Mod", &[][..]),
    ] {
        let query = fuzzy::prepare_query(text);
        let label = Arc::from(raw);
        perf::assert_no_churn(|| {
            black_box(matched_setting_label(&query, &label, aliases));
        });
    }
    let query = fuzzy::prepare_query("music");
    let label = Arc::from("  Music Rate\\nbpm: {bpm}");
    perf::assert_churn_budget(1, 64, || {
        black_box(matched_setting_label(&query, &label, &[]));
    });
}

#[test]
fn display_variants_stay_lazy_and_preserve_exact_text_when_scrolling() {
    for label in [
        "",
        "ASCII",
        "De\u{301}ja\u{300} Vu",
        "\u{65e5}\u{672c}\u{8a9e}",
        "line\nnext",
    ] {
        let label: Arc<str> = Arc::from(label);
        let expected = baseline_labels::row_text(&label);
        let row = SearchRowText::default();
        perf::assert_no_churn(|| {
            black_box(SearchRowText::default());
        });
        let focused = row.get(&label, true);
        assert_eq!(focused, expected[1]);
        let cloned = row.clone();
        assert!(Arc::ptr_eq(&focused, &cloned.get(&label, true)));
        let normal = row.get(&label, false);
        assert_eq!(normal, expected[0]);
        for state in [true, false, false, true] {
            perf::assert_no_churn(|| {
                black_box(row.get(&label, state));
            });
            assert!(Arc::ptr_eq(
                &row.get(&label, state),
                if state { &focused } else { &normal }
            ));
        }
    }
    let labels: Vec<_> = (0..128)
        .map(|i| Arc::<str>::from(format!("Choice {i:03}")))
        .collect();
    let old: Vec<_> = labels
        .iter()
        .map(|s| baseline_labels::row_text(s))
        .collect();
    let rows: Vec<_> = labels.iter().map(|_| SearchRowText::default()).collect();
    for selected in [0usize, 1, 7, 64, 127, 0] {
        let first = selected.saturating_sub(4).min(labels.len() - 8);
        for index in first..first + 8 {
            let focused = index == selected;
            assert_eq!(
                rows[index].get(&labels[index], focused),
                old[index][usize::from(focused)]
            );
        }
    }
}

fn compare_scores(query: &str, label: &str, aliases: &[&str]) {
    let old_query = baseline_fuzzy::prepare_query(query);
    let new_query = fuzzy::prepare_query(query);
    let old_label = baseline_fuzzy::fold_diacritics(label);
    let new_label = fuzzy::fold_diacritics(label);
    assert_eq!(
        fuzzy::best_match_score(&new_query, &new_label, aliases),
        baseline_fuzzy::best_match_score(&old_query, &old_label, aliases),
        "{query:?} / {label:?} / {aliases:?}",
    );
}

#[test]
fn scores_match_parent_for_exhaustive_short_words_and_unicode_edits() {
    let mut words = vec![String::new()];
    for len in 1..=7 {
        for bits in 0..1usize << len {
            words.push(
                (0..len)
                    .map(|i| if bits & (1 << i) == 0 { 'a' } else { 'b' })
                    .collect(),
            );
        }
    }
    for query in &words {
        for label in &words {
            compare_scores(query, label, &[]);
        }
    }
    for original in [
        "Perspective",
        "backgroundFilter",
        "De\u{301}ja\u{300} Vu",
        "\u{421}\u{43a}\u{43e}\u{440}\u{43e}\u{441}\u{442}\u{44c}",
        "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{66f2}",
        "\u{130}stanbul",
        "\u{301}a\u{301}b",
        "white\u{2003}space\tword",
    ] {
        let chars: Vec<_> = original.chars().collect();
        for index in 0..=chars.len() {
            for replacement in ['x', ' ', '\u{e9}', '\u{65e5}', '\u{301}'] {
                let mut inserted = chars.clone();
                inserted.insert(index, replacement);
                let inserted: String = inserted.into_iter().collect();
                compare_scores(&inserted, original, &[]);
                compare_scores(original, &inserted, &["alias", "arrows"]);
                if index < chars.len() {
                    let mut replaced = chars.clone();
                    replaced[index] = replacement;
                    compare_scores(&replaced.into_iter().collect::<String>(), original, &[]);
                    let mut removed = chars.clone();
                    removed.remove(index);
                    compare_scores(&removed.into_iter().collect::<String>(), original, &[]);
                }
            }
        }
    }
}

#[test]
fn display_text_allocations_cover_inline_and_spill_boundaries() {
    for label in [
        "a".repeat(124),
        "b".repeat(125),
        "c".repeat(127),
        "\u{65e5}\u{672c}".repeat(256),
    ] {
        let label: Arc<str> = Arc::from(label);
        let expected = baseline_labels::row_text(&label);
        for focused in [false, true] {
            let row = SearchRowText::default();
            let bytes = label.len() + if focused { 4 } else { 2 };
            let allocations = if bytes <= 128 { 1 } else { 2 };
            perf::assert_churn_budget(allocations, bytes * allocations + 32, || {
                black_box(row.get(&label, focused));
            });
            assert_eq!(row.get(&label, focused), expected[usize::from(focused)]);
            perf::assert_no_churn(|| {
                black_box(row.get(&label, focused));
            });
        }
    }
}

#[test]
fn ranked_catalog_keeps_scores_and_stable_ties() {
    let labels: Vec<_> = [
        "Perspective",
        "perspective",
        "Adjust Perspective",
        "Speed Mod",
        "NoteSkin",
        "D\u{e9}j\u{e0} Vu",
        "De\u{301}ja\u{300} Vu",
        "\u{65e5}\u{672c}\u{8a9e}",
        "duplicate",
        "duplicate",
    ]
    .into_iter()
    .map(fuzzy::fold_diacritics)
    .collect();
    for text in [
        "",
        "p",
        "perspextive",
        "duplciate",
        "speed",
        "deja",
        "\u{65e5}",
        "unmatched",
    ] {
        let old_query = baseline_fuzzy::prepare_query(text);
        let new_query = fuzzy::prepare_query(text);
        let rank = |old| {
            let mut matches: Vec<_> = labels
                .iter()
                .enumerate()
                .filter_map(|(index, label)| {
                    let score = if old {
                        baseline_fuzzy::best_match_score(&old_query, label, &[])
                    } else {
                        fuzzy::best_match_score(&new_query, label, &[])
                    }?;
                    Some((index, score))
                })
                .collect();
            if !new_query.is_empty() {
                matches.sort_by(|(a, sa), (b, sb)| {
                    sb.cmp(sa)
                        .then_with(|| labels[*a].len().cmp(&labels[*b].len()))
                        .then_with(|| labels[*a].cmp(&labels[*b]))
                });
            }
            matches
        };
        assert_eq!(rank(false), rank(true), "{text:?}");
    }
}

#[test]
fn long_typo_boundaries_keep_scores_and_avoid_dp_row_allocations() {
    for len in [31, 32, 33, 80, 95, 96, 97, 128, 256] {
        let query = "a".repeat(len);
        for index in [0, len / 2, len - 1] {
            let mut candidate = query.clone();
            candidate.replace_range(index..index + 1, "b");
            compare_scores(&query, &candidate, &[]);
            let prepared = fuzzy::prepare_query(&query);
            perf::assert_no_churn(|| {
                black_box(fuzzy::best_match_score(&prepared, &candidate, &[]));
            });
        }
        for edits in [len / 3, len / 3 + 1, len] {
            let candidate = "b".repeat(edits) + &"a".repeat(len - edits);
            compare_scores(&query, &candidate, &[]);
        }
    }
}

#[test]
#[ignore = "manual release benchmark; filter benchmark_search_ranking"]
fn benchmark_search_ranking() {
    eprintln!(
        "row storage: old {}, new {} bytes",
        std::mem::size_of::<[Arc<str>; 2]>(),
        std::mem::size_of::<SearchRowText>()
    );
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let order = if reverse {
        [false, true]
    } else {
        [true, false]
    };
    for (name, text, raw, aliases) in [
        ("label_hit", "speed", "Speed Mod", &[][..]),
        ("label_miss", "missing", "Speed Mod", &[][..]),
        (
            "label_trimmed",
            "music",
            " Music Rate\\nbpm: {bpm}",
            &[][..],
        ),
        ("label_alias", "arrows", "NoteSkin", &["arrows"][..]),
        ("label_unicode", "deja", "D\u{e9}j\u{e0} Vu", &[][..]),
        ("label_empty_query", "", "Speed Mod", &[][..]),
    ] {
        let query = fuzzy::prepare_query(text);
        let label = Arc::from(raw);
        for old in order {
            let name = format!("{name}/{}", if old { "old" } else { "new" });
            if old {
                perf::measure_sampled(&name, 50_000, 1, || {
                    baseline_labels::matched_setting_label(
                        black_box(&query),
                        black_box(&label),
                        black_box(aliases),
                    )
                });
            } else {
                perf::measure_sampled(&name, 50_000, 1, || {
                    matched_setting_label(black_box(&query), black_box(&label), black_box(aliases))
                });
            }
        }
    }
    for (name, count, visible, both) in [
        ("rows_8", 8, 8, false),
        ("rows_128", 128, 8, false),
        ("rows_1024", 1024, 8, false),
        ("rows_all", 128, 128, false),
        ("rows_both", 128, 128, true),
        ("rows_long", 128, 8, false),
        ("rows_long_both", 128, 128, true),
        ("rows_60_frames", 128, 8, false),
    ] {
        let labels: Vec<_> =
            (0..count)
                .map(|i| {
                    Arc::<str>::from(
                        format!("Noteskin choice {i:04}")
                            .repeat(if name.starts_with("rows_long") { 12 } else { 1 }),
                    )
                })
                .collect();
        let frames = if name == "rows_60_frames" { 60 } else { 1 };
        let units = if frames == 60 { frames } else { count };
        for old in order {
            let name = format!("{name}/{}", if old { "old" } else { "new" });
            if old {
                perf::measure_sampled(&name, 256, units, || {
                    let rows: Vec<_> = black_box(&labels)
                        .iter()
                        .map(|label| baseline_labels::row_text(label))
                        .collect();
                    for frame in 0..frames {
                        for (index, row) in rows[..visible].iter().enumerate() {
                            black_box(Arc::clone(
                                &row[usize::from(!both && index == frame % visible)],
                            ));
                            if both {
                                black_box(Arc::clone(&row[1]));
                            }
                        }
                    }
                    black_box(&rows);
                });
            } else {
                perf::measure_sampled(&name, 256, units, || {
                    let rows: Vec<_> = black_box(&labels)
                        .iter()
                        .map(|_| SearchRowText::default())
                        .collect();
                    for frame in 0..frames {
                        for (index, (row, label)) in rows[..visible].iter().zip(&labels).enumerate()
                        {
                            black_box(row.get(label, !both && index == frame % visible));
                            if both {
                                black_box(row.get(label, true));
                            }
                        }
                    }
                    black_box(&rows);
                });
            }
        }
    }
    let labels: Vec<_> = (0..8)
        .map(|i| Arc::<str>::from(format!("Noteskin choice {i:04}")))
        .collect();
    let old_rows: Vec<_> = labels
        .iter()
        .map(|s| (Arc::clone(s), baseline_labels::row_text(s)))
        .collect();
    let rows: Vec<_> = labels
        .iter()
        .map(|label| {
            let row = SearchRowText::default();
            black_box(row.get(label, false));
            (Arc::clone(label), row)
        })
        .collect();
    for old in order {
        let name = format!("rows_warm/{}", if old { "old" } else { "new" });
        if old {
            perf::measure_sampled(&name, 50_000, 8, || {
                for (_, row) in black_box(&old_rows) {
                    black_box(Arc::clone(&row[0]));
                }
            });
        } else {
            perf::measure_sampled(&name, 50_000, 8, || {
                for (label, row) in black_box(&rows) {
                    black_box(row.get(label, false));
                }
            });
        }
    }
    let long = "a".repeat(128);
    let long_typo = "a".repeat(64) + "b" + &"a".repeat(63);
    for (name, query, label, aliases) in [
        ("typo_front", "xerspective", "Perspective", &[][..]),
        ("typo_middle", "perspextive", "Perspective", &[][..]),
        ("typo_end", "perspectivx", "Perspective", &[][..]),
        ("typo_transposed", "prespective", "Perspective", &[][..]),
        ("typo_unrelated", "abcdefghi", "zyxwvutsr", &[][..]),
        (
            "typo_unicode",
            "\u{421}\u{43a}\u{43e}\u{43b}\u{43e}\u{441}\u{442}\u{44c}",
            "\u{421}\u{43a}\u{43e}\u{440}\u{43e}\u{441}\u{442}\u{44c}",
            &[][..],
        ),
        (
            "typo_multiword",
            "perspextive",
            "Adjust Perspective Mode",
            &[][..],
        ),
        ("typo_long", long.as_str(), long_typo.as_str(), &[][..]),
        ("match_prefix", "speed", "Speed Mod", &[][..]),
        ("match_alias", "arrows", "NoteSkin", &["arrows"][..]),
    ] {
        let old_query = baseline_fuzzy::prepare_query(query);
        let new_query = fuzzy::prepare_query(query);
        for old in order {
            let name = format!("{name}/{}", if old { "old" } else { "new" });
            if old {
                perf::measure_sampled(&name, 10_000, 1, || {
                    baseline_fuzzy::best_match_score(
                        black_box(&old_query),
                        black_box(label),
                        black_box(aliases),
                    )
                });
            } else {
                perf::measure_sampled(&name, 10_000, 1, || {
                    fuzzy::best_match_score(
                        black_box(&new_query),
                        black_box(label),
                        black_box(aliases),
                    )
                });
            }
        }
    }
}
