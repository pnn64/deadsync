use deadsync_score::*;
use std::{collections::HashSet, hint::black_box};

#[path = "score_preparation/baseline.rs"]
mod baseline;
#[path = "score_preparation/fixtures.rs"]
mod fixtures;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn leaderboard(count: usize, order: &str) -> Vec<LeaderboardEntry> {
    let mut entries: Vec<_> = (0..count)
        .map(|i| LeaderboardEntry {
            rank: i as u32 + 1,
            name: format!("Player {i:05}"),
            machine_tag: None,
            score: 10_000.0 - i as f64,
            date: String::new(),
            is_rival: i % 19 == 7,
            is_self: i == count / 2,
            is_fail: false,
        })
        .collect();
    match order {
        "reverse" => entries.reverse(),
        "shuffle" => {
            let mut state = 173;
            for i in (1..count).rev() {
                entries.swap(i, random(&mut state) as usize % (i + 1));
            }
        }
        "no-rivals" => {
            for entry in &mut entries {
                entry.is_rival = false;
            }
        }
        "ties" => {
            for entry in &mut entries {
                entry.rank = 1;
            }
        }
        _ => {}
    }
    entries
}

fn assert_selection(entries: &[LeaderboardEntry], limit: usize) {
    // Pointer identity checks duplicate/tie winners and ordering, not just rank.
    for (actual, expected) in [
        (
            prioritized_leaderboard_entry_refs(entries, limit),
            baseline::prioritized_leaderboard_entry_refs(entries, limit),
        ),
        (
            neighboring_leaderboard_entry_refs(entries, limit),
            baseline::neighboring_leaderboard_entry_refs(entries, limit),
        ),
    ] {
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(&expected) {
            assert!(std::ptr::eq(*a, *b));
        }
    }
}

#[test]
fn leaderboard_prefix_preserves_priorities_ties_duplicates_and_limits() {
    for size in [0, 1, 2, 5, 10, 11, 64, 1024] {
        for order in ["sorted", "reverse", "shuffle", "ties", "no-rivals"] {
            let entries = leaderboard(size, order);
            for limit in [0, 1, 2, 5, 10, 11, 32, size, size + 1] {
                assert_selection(&entries, limit);
            }
        }
    }
    let mut state = 712_437;
    for _ in 0..1000 {
        let mut entries = leaderboard(random(&mut state) as usize % 200, "shuffle");
        for entry in &mut entries {
            entry.rank = [0, 1, 2, 5, 8, u32::MAX][random(&mut state) as usize % 6];
            entry.name = ["alice", "ALICE", "Bob", "bob", "Stra\u{df}e", "STRASSE"]
                [random(&mut state) as usize % 6]
                .into();
            entry.is_self = random(&mut state).is_multiple_of(3);
            entry.is_rival = random(&mut state).is_multiple_of(2);
        }
        assert_selection(&entries, random(&mut state) as usize % 35);
    }
}

fn assert_date(date: &str) {
    assert_eq!(
        format_leaderboard_date(date),
        baseline::format_leaderboard_date(date)
    );
    assert_eq!(
        format_leaderboard_date_or_placeholder(date),
        baseline::format_leaderboard_date_or_placeholder(date)
    );
}

#[test]
fn date_labels_preserve_permissive_parsing_and_fallbacks() {
    for date in [
        "",
        " \t\n",
        "2026-09-14 12:13:14",
        "2026-09-14T12:13:14Z",
        "year-+1-+2",
        "year-000001-0000002",
        "year-12-4294967295",
        "year-12-4294967296",
        "year-0-1",
        "year-13-1",
        "year-1-0",
        "year-1--2",
        "-1-2",
        "a-1-2-extra",
        "a-1-2\tother",
        "\u{2003}y-1-2\u{2003}",
        "\u{65e5}\u{672c}-9-7",
        "broken",
        "2026-01",
        "-0026-01-01",
    ] {
        assert_date(date);
    }
    for month in 0..=14 {
        for day in 0..=100 {
            for prefix in ["", "+", "0000"] {
                assert_date(&format!(" 2026-{prefix}{month}-{prefix}{day}T00:00:00 "));
            }
        }
    }
    let mut state = 13;
    for _ in 0..2000 {
        let text: String = (0..random(&mut state) as usize % 64)
            .map(|_| {
                ['a', '-', '+', 'T', ' ', '0', '9', '\u{e9}', '\u{2003}']
                    [random(&mut state) as usize % 9]
            })
            .collect();
        assert_date(&text);
    }
}

fn packs(count: usize) -> Vec<deadsync_chart::SongPack> {
    (0..count)
        .map(|i| {
            let charts = (0..4)
                .map(|j| fixtures::ranked_chart(&format!("{:016x}", i * 4 + j), "dance-single", ""))
                .collect();
            let mut pack = fixtures::song_pack(&format!("Pack {i:04}"), charts);
            pack.name = format!("Display {i:04}");
            pack
        })
        .collect()
}

#[test]
fn import_filter_preserves_aliases_unicode_empty_filters_and_hash_order() {
    let mut library = packs(64);
    for (i, pack) in library.iter_mut().enumerate() {
        pack.group_name = [
            " Pack ",
            "PACK",
            "Stra\u{df}e",
            "STRASSE",
            "\u{c9}",
            "\u{e9}",
            "\u{2003}",
        ][i % 7]
            .into();
        pack.name = ["Alias", " alias ", "", " ", "\u{c9}"][i % 5].into();
        let song = std::sync::Arc::make_mut(&mut pack.songs[0]);
        for (j, chart) in song.charts.iter_mut().enumerate() {
            chart.short_hash =
                ["", " ", "hash", " hash ", "HASH", "other", "done"][(i + j) % 7].into();
        }
    }
    let existing = HashSet::from(["done".to_string(), "HASH".into()]);
    let filters = [
        "",
        " ",
        "pack",
        "ALIAS",
        "Stra\u{df}e",
        "strasse",
        "\u{c9}",
        "\u{e9}",
        "unknown",
    ];
    for mask in 0..1 << filters.len() {
        let selected: Vec<_> = filters
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, s)| s.to_string())
            .collect();
        assert_eq!(
            collect_chart_hashes_per_pack_for_import(&library, &selected, &existing),
            baseline::collect_chart_hashes_per_pack_for_import(&library, &selected, &existing)
        );
    }
    let mut growing = packs(20);
    for (i, pack) in growing.iter_mut().enumerate() {
        pack.group_name = "G".repeat(i * 100);
        pack.name = "A".repeat(i * 200);
    }
    let filter = vec!["a".repeat(3800), "none".into()];
    assert_eq!(
        collect_chart_hashes_per_pack_for_import(&growing, &filter, &existing),
        baseline::collect_chart_hashes_per_pack_for_import(&growing, &filter, &existing)
    );
}

#[test]
fn score_preparation_allocation_budgets() {
    let entries = leaderboard(1024, "shuffle");
    perf::assert_no_churn(|| {
        black_box(prioritized_leaderboard_entry_refs(&entries, 5));
        black_box(neighboring_leaderboard_entry_refs(&entries, 5));
    });
    for date in ["2026-09-14", "2026-01-01", "year-12-4294967295"] {
        perf::assert_churn_budget(1, 20, || {
            black_box(format_leaderboard_date(date));
        });
    }
    let library = packs(512);
    let filters = vec!["unknown".into()];
    let existing = HashSet::new();
    perf::assert_reduced_churn(
        || {
            black_box(baseline::collect_chart_hashes_per_pack_for_import(
                &library, &filters, &existing,
            ));
        },
        || {
            black_box(collect_chart_hashes_per_pack_for_import(
                &library, &filters, &existing,
            ));
        },
    );
    perf::assert_no_churn(|| {
        black_box(collect_chart_hashes_per_pack_for_import(
            &library, &filters, &existing,
        ));
    });
    let filters = vec!["unknown".into(), "another".into()];
    perf::assert_reduced_churn(
        || {
            black_box(baseline::collect_chart_hashes_per_pack_for_import(
                &library, &filters, &existing,
            ));
        },
        || {
            black_box(collect_chart_hashes_per_pack_for_import(
                &library, &filters, &existing,
            ));
        },
    );
    perf::assert_churn_budget(4, 256, || {
        black_box(collect_chart_hashes_per_pack_for_import(
            &library, &filters, &existing,
        ));
    });
}

type Select = for<'a> fn(&'a [LeaderboardEntry], usize) -> PrioritizedLeaderboardEntryRefs<'a>;
type Import =
    fn(&[deadsync_chart::SongPack], &[String], &HashSet<String>) -> Vec<(String, Vec<String>)>;

#[test]
#[ignore = "manual old/new score preparation benchmark"]
fn benchmark_score_preparation() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (size, order) in [
        (5, "sorted"),
        (10, "sorted"),
        (100, "sorted"),
        (1024, "sorted"),
        (1024, "reverse"),
        (1024, "shuffle"),
        (1024, "ties"),
        (1024, "no-rivals"),
    ] {
        let entries = leaderboard(size, order);
        for neighbors in [false, true] {
            let functions: [Select; 2] = if neighbors {
                [
                    baseline::neighboring_leaderboard_entry_refs,
                    neighboring_leaderboard_entry_refs,
                ]
            } else {
                [
                    baseline::prioritized_leaderboard_entry_refs,
                    prioritized_leaderboard_entry_refs,
                ]
            };
            for new in [reverse, !reverse] {
                let run = black_box(functions[usize::from(new)]);
                perf::measure_sampled(
                    &format!(
                        "select/{size}-{order}-{}/{}",
                        if neighbors { "near" } else { "top" },
                        if new { "new" } else { "old" }
                    ),
                    1000,
                    size,
                    || {
                        black_box(run(black_box(&entries), black_box(5)));
                    },
                );
            }
        }
    }
    for (label, dates) in [
        (
            "normal",
            (1..=64)
                .map(|i| format!("2026-{:02}-{:02} 12:13:14", i % 12 + 1, i % 28 + 1))
                .collect::<Vec<_>>(),
        ),
        (
            "fallback",
            ["", "unknown", "  -----  ", "2026-13-0"]
                .repeat(16)
                .into_iter()
                .map(String::from)
                .collect(),
        ),
    ] {
        for new in [reverse, !reverse] {
            let run: fn(&str) -> String = black_box(if new {
                format_leaderboard_date
            } else {
                baseline::format_leaderboard_date
            });
            perf::measure_sampled(
                &format!("date/{label}/{}", if new { "new" } else { "old" }),
                1000,
                dates.len(),
                || {
                    for date in &dates {
                        black_box(run(black_box(date)));
                    }
                },
            );
        }
    }
    let library = packs(512);
    let existing = HashSet::new();
    for (label, filters) in [
        ("none", vec![]),
        ("blank", vec!["  ".into()]),
        ("miss", vec!["unknown".into()]),
        ("two-miss", vec!["unknown".into(), "another".into()]),
        ("one-pack-miss", vec!["unknown".into()]),
        ("group", vec!["pAcK 0256".into()]),
        ("alias", vec!["dIsPlAy 0256".into()]),
        (
            "many",
            (0..128).map(|i| format!("pack {:04}", i * 4)).collect(),
        ),
    ] {
        let library = if label == "one-pack-miss" {
            &library[..1]
        } else {
            &library[..]
        };
        for new in [reverse, !reverse] {
            let run: Import = black_box(if new {
                collect_chart_hashes_per_pack_for_import
            } else {
                baseline::collect_chart_hashes_per_pack_for_import
            });
            perf::measure_sampled(
                &format!("import/{label}/{}", if new { "new" } else { "old" }),
                150,
                library.len(),
                || {
                    black_box(run(
                        black_box(&library),
                        black_box(&filters),
                        black_box(&existing),
                    ));
                },
            );
        }
    }
}
