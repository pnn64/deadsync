use super::*;
use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/title_search/baseline.rs"
    ));
}

#[test]
fn direct_title_search_matches_virtual_join_at_every_character_boundary() {
    let parts = [
        "",
        " ",
        "\t\r\n",
        "\u{a0}",
        "Alpha",
        "aAaAa",
        "日本語É",
        " A Mix ",
        "(Remix)",
    ];
    for left in parts {
        for right in parts {
            let joined = if right.trim().is_empty() {
                left.to_owned()
            } else {
                format!("{left} {right}")
            };
            let mut boundaries: Vec<_> = joined.char_indices().map(|(index, _)| index).collect();
            boundaries.push(joined.len());
            for (index, &start) in boundaries.iter().enumerate() {
                for &end in &boundaries[index..] {
                    for needle in [
                        joined[start..end].to_owned(),
                        joined[start..end].to_ascii_uppercase(),
                        format!("{}x", &joined[start..end]),
                    ] {
                        assert_eq!(
                            joined_contains_ignore_ascii_case(left, right, &needle),
                            baseline::joined_contains_ignore_ascii_case(left, right, &needle),
                            "{left:?} {right:?} {needle:?}"
                        );
                    }
                }
            }
        }
    }
    assert_no_churn(|| {
        black_box(joined_contains_ignore_ascii_case("Song", "", "SON"));
    });
}

fn catalog(count: usize, subtitle: &str) -> Vec<Arc<SongData>> {
    (0..count)
        .map(|index| {
            let mut song = test_song(
                &format!("A familiar title with repeated AAA words {index:04}"),
                subtitle,
            );
            Arc::get_mut(&mut song)
                .unwrap()
                .charts
                .push(test_chart("dance-single"));
            song
        })
        .collect()
}

fn entries(songs: &[Arc<SongData>]) -> impl Iterator<Item = SongSearchCatalogEntry<'_>> {
    std::iter::once(SongSearchCatalogEntry::PackHeader("Pack"))
        .chain(songs.iter().map(SongSearchCatalogEntry::Song))
}

#[test]
fn search_catalog_preserves_filters_order_and_song_handles() {
    let mut songs = catalog(32, "");
    songs.extend(catalog(32, "(Remix)"));
    for query in [
        "",
        "title",
        "missing",
        "Pack/title",
        "[12] title",
        "[120] familiar",
        "[140] familiar",
        "title [140] [5]",
        "0031 (REMIX)",
        "remix",
        " ",
    ] {
        let old = baseline::build_song_search_candidates(entries(&songs), query, "dance-single");
        let new = build_song_search_candidates(entries(&songs), query, "dance-single");
        assert_eq!(format!("{old:?}"), format!("{new:?}"), "query={query:?}");
        for (old, new) in old.iter().zip(&new) {
            assert!(Arc::ptr_eq(&old.song, &new.song));
        }
    }
    assert_eq!(
        build_song_search_candidates(entries(&songs), "WORDS", "dance-single").len(),
        64
    );
    assert!(build_song_search_candidates(entries(&songs), "missing", "dance-single").is_empty());
}

fn pair<T>(
    name: &str,
    iterations: usize,
    units: usize,
    old: impl FnMut() -> T,
    new: impl FnMut() -> T,
) {
    if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
        measure_sampled(
            &format!("title-search/{name}/current"),
            iterations,
            units,
            new,
        );
        measure_sampled(
            &format!("title-search/{name}/original"),
            iterations,
            units,
            old,
        );
    } else {
        measure_sampled(
            &format!("title-search/{name}/original"),
            iterations,
            units,
            old,
        );
        measure_sampled(
            &format!("title-search/{name}/current"),
            iterations,
            units,
            new,
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn title_search_benchmark() {
    for (label, left, right, needle) in [
        (
            "plain-miss",
            "A familiar title with repeated AAA words 0031",
            "",
            "AAB",
        ),
        (
            "plain-hit",
            "A familiar title with repeated AAA words 0031",
            "",
            "WORDS",
        ),
        (
            "blank-subtitle",
            "A familiar title with repeated AAA words 0031",
            " \t\u{a0}",
            "AAB",
        ),
        (
            "subtitle-miss",
            "A familiar title with repeated AAA words 0031",
            "(Remix)",
            "AAB",
        ),
        (
            "subtitle-crossing",
            "A familiar title with repeated AAA words 0031",
            "(Remix)",
            "0031 (REM",
        ),
    ] {
        pair(
            label,
            65_536,
            1,
            || {
                baseline::joined_contains_ignore_ascii_case(
                    black_box(left),
                    black_box(right),
                    black_box(needle),
                )
            },
            || {
                joined_contains_ignore_ascii_case(
                    black_box(left),
                    black_box(right),
                    black_box(needle),
                )
            },
        );
    }
    for (label, subtitle, query) in [
        ("catalog-plain-miss", "", "AAB"),
        ("catalog-plain-hit", "", "WORDS"),
        ("catalog-subtitle-miss", "(Remix)", "AAB"),
    ] {
        let songs = catalog(1024, subtitle);
        pair(
            label,
            64,
            1024,
            || {
                baseline::build_song_search_candidates(
                    entries(black_box(&songs)),
                    black_box(query),
                    "dance-single",
                )
            },
            || {
                build_song_search_candidates(
                    entries(black_box(&songs)),
                    black_box(query),
                    "dance-single",
                )
            },
        );
    }
}
