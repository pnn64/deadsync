use deadsync_score::{
    collect_chart_hashes_per_pack_for_import, grade_from_itg, score_import_pack_complete_detail,
    score_import_pack_detail,
};
use std::{collections::HashSet, hint::black_box, sync::Arc};

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/import_catalog.rs"]
mod fixture;
#[path = "support/import_kernels_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn catalog(pack_count: usize, songs: usize) -> Vec<deadsync_chart::SongPack> {
    (0..pack_count)
        .map(|pack| {
            let mut value = fixture::library_with_charts(Vec::new()).pop().unwrap();
            value.group_name = format!(" Group {pack} ");
            value.name = format!(" Display {pack} ");
            value.songs = (0..songs)
                .map(|song| {
                    Arc::new(fixture::song(
                        "chart.ssc",
                        (0..6)
                            .map(|chart| {
                                fixture::chart(
                                    "Hard",
                                    &format!(" {:016x} ", (pack * songs + song) * 4 + chart),
                                )
                            })
                            .collect(),
                    ))
                })
                .collect();
            value
        })
        .collect()
}

#[test]
fn catalog_selection_preserves_filters_duplicates_order_and_hash_spelling() {
    let mut packs = catalog(4, 7);
    let song = Arc::make_mut(&mut packs[0].songs[0]);
    song.charts.extend(
        ["", " \t ", "ABC", "abc", "é", " \nabc "].map(|hash| fixture::chart("Edit", hash)),
    );
    packs[2].name.clear();
    let existing: HashSet<_> = ["0000000000000000", "ABC", "é"].map(String::from).into();
    for filters in [
        vec![],
        vec!["".into()],
        vec![" display 1 ".into()],
        vec!["group 0".into(), "GROUP 2".into()],
        vec!["Group 0".into(), "Group 0".into(), "\u{2003}".into()],
        vec!["missing".into(), "Display 3".into()],
    ] {
        assert_eq!(
            collect_chart_hashes_per_pack_for_import(&packs, &filters, &existing),
            original::collect_chart_hashes_per_pack_for_import(&packs, &filters, &existing)
        );
    }
}

#[test]
fn imported_grades_preserve_all_tiers_prefixes_whitespace_and_rejections() {
    for a in 0..=255_u8 {
        for b in 0..=255_u8 {
            let tier = format!("Tier{}{}", char::from(a), char::from(b));
            for value in [tier.clone(), format!(" \u{2003}Grade_{tier}\t")] {
                assert_eq!(
                    grade_from_itg(&value),
                    original::grade_from_itg(&value),
                    "{value:?}"
                );
            }
        }
    }
    for value in [
        "Failed",
        " Grade_Failed ",
        "failed",
        "Grade_Quint",
        "Tier1",
        "Tier001",
        "Tier17 ",
        "\0Tier01",
        "Grade_Grade_Tier01",
        "Grade_\u{2003}Tier01",
        "",
    ] {
        assert_eq!(
            grade_from_itg(value),
            original::grade_from_itg(value),
            "{value:?}"
        );
    }
    perf::assert_no_churn(|| {
        black_box(grade_from_itg(black_box("Grade_Tier04")));
    });
}

#[test]
#[ignore = "release paired CPU benchmark"]
fn benchmark_import_kernels() {
    let packs = catalog(16, 32);
    let existing = (0..2048).step_by(3).map(|i| format!("{i:016x}")).collect();
    for (kind, filters) in [
        ("all", vec![]),
        ("one", vec!["Display 7".into()]),
        ("many", (0..12).map(|i| format!("Display {i}")).collect()),
    ] {
        let label = format!("hashes/{kind}/3072");
        for current in [false, true] {
            let work = || {
                if current {
                    collect_chart_hashes_per_pack_for_import(&packs, &filters, &existing)
                } else {
                    original::collect_chart_hashes_per_pack_for_import(&packs, &filters, &existing)
                }
            };
            let (result, counts) = perf::measure(work);
            black_box(result);
            println!(
                "{label}/{}: allocation {counts:?}",
                if current { "current" } else { "original" }
            );
        }
        bench::compare(&label, 40, |current| {
            let run = black_box(if current {
                collect_chart_hashes_per_pack_for_import
            } else {
                original::collect_chart_hashes_per_pack_for_import
            });
            black_box(run(
                black_box(&packs),
                black_box(&filters),
                black_box(&existing),
            ));
        });
    }
    for kind in ["tiers", "prefixed", "invalid"] {
        let values: Vec<_> = (0..128)
            .map(|i| match kind {
                "tiers" => format!("Tier{:02}", i % 17 + 1),
                "prefixed" => format!(" \u{2003}Grade_Tier{:02}\t", i % 17 + 1),
                _ => [
                    "Failed",
                    "Tier00",
                    "Tier18",
                    "Tier1",
                    "Tier0é",
                    "Grade_Quint",
                    "",
                    "Grade_Failed",
                ][i % 8]
                    .to_owned(),
            })
            .collect();
        let label = format!("grades/{kind}/128");
        for current in [false, true] {
            let (_, counts) = perf::measure(|| {
                for value in &values {
                    black_box(if current {
                        grade_from_itg(value)
                    } else {
                        original::grade_from_itg(value)
                    });
                }
            });
            println!(
                "{label}/{}: allocation {counts:?}",
                if current { "current" } else { "original" }
            );
        }
        bench::compare(&label, 2000, |current| {
            let run = black_box(if current {
                grade_from_itg
            } else {
                original::grade_from_itg
            });
            for value in black_box(&values) {
                black_box(run(black_box(value)));
            }
        });
    }
}

#[test]
fn pack_messages_preserve_all_text_counts_and_optional_failure_suffixes() {
    for name in [
        "",
        "Pack A",
        "Cafe\u{301} / \u{6771}\u{4eac} \n\0",
        "A very long pack name with punctuation () {} and spaces",
    ] {
        for count in [0, 1, 9, 10, 999, usize::MAX] {
            for failures in [0, 1, 10, usize::MAX] {
                assert_eq!(
                    score_import_pack_detail(7, 10, name, count, count, failures),
                    original::score_import_pack_detail(7, 10, name, count, count, failures)
                );
                assert_eq!(
                    score_import_pack_complete_detail(7, 10, name, count, count, count, failures),
                    original::score_import_pack_complete_detail(
                        7, 10, name, count, count, count, failures
                    )
                );
            }
        }
    }
}

#[test]
#[ignore = "release paired CPU and allocation benchmark"]
fn benchmark_pack_messages() {
    for kind in ["normal", "failed", "long"] {
        let names: Vec<_> = (0..128)
            .map(|i| {
                if kind == "long" {
                    format!("{} / {i} / \u{6771}\u{4eac}", "Long pack name ".repeat(16))
                } else {
                    format!("Pack {i}")
                }
            })
            .collect();
        let failures = usize::from(kind != "normal") * 17;
        for complete in [false, true] {
            let work = |current: bool| {
                for (i, name) in names.iter().enumerate() {
                    if complete {
                        let run = black_box(if current {
                            score_import_pack_complete_detail
                        } else {
                            original::score_import_pack_complete_detail
                        });
                        black_box(run(
                            black_box(i),
                            128,
                            black_box(name),
                            1000,
                            750,
                            250,
                            black_box(failures),
                        ));
                    } else {
                        let run = black_box(if current {
                            score_import_pack_detail
                        } else {
                            original::score_import_pack_detail
                        });
                        black_box(run(
                            black_box(i),
                            128,
                            black_box(name),
                            750,
                            250,
                            black_box(failures),
                        ));
                    }
                }
            };
            let label = format!(
                "messages/{kind}/{}/128",
                if complete { "complete" } else { "progress" }
            );
            for current in [false, true] {
                let (_, counts) = perf::measure(|| work(current));
                println!(
                    "{label}/{}: allocation {counts:?}",
                    if current { "current" } else { "original" }
                );
            }
            bench::compare(&label, 300, work);
        }
    }
}
