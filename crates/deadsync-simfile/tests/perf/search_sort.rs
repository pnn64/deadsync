use super::*;
use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/search_sort/baseline.rs"
    ));
}

#[test]
fn search_sort_preserves_joined_byte_order_at_prefix_and_unicode_boundaries() {
    let parts = [
        "",
        " ",
        "\t\r\n",
        "\u{a0}",
        "Alpha",
        "ALPHA",
        "Alpha!",
        "Alpha ",
        "Alpha B",
        "日本語É",
        "É",
        "Été",
        "Éx",
        "(Mix)",
    ];
    let songs: Vec<_> = parts
        .iter()
        .flat_map(|title| parts.iter().map(move |subtitle| test_song(title, subtitle)))
        .collect();
    for left in &songs {
        for right in &songs {
            let joined = |song: &SongData| {
                if song.subtitle.trim().is_empty() {
                    song.title.to_ascii_lowercase()
                } else {
                    format!("{} {}", song.title, song.subtitle).to_ascii_lowercase()
                }
            };
            let want = joined(left).cmp(&joined(right));
            assert_eq!(baseline::display_full_title_cmp(left, right), want);
            assert_eq!(
                display_full_title_cmp(left, right),
                want,
                "{:?}/{:?} vs {:?}/{:?}",
                left.title,
                left.subtitle,
                right.title,
                right.subtitle
            );
        }
    }
    assert_no_churn(|| {
        black_box(display_full_title_cmp(&songs[35], &songs[88]));
    });
}

fn catalog(count: usize, subtitle: &str, common: bool) -> Vec<Arc<SongData>> {
    (0..count)
        .map(|index| {
            let title = if common {
                format!(
                    "A familiar title with repeated AAA words {:04}",
                    (index * 73) % count
                )
            } else {
                format!(
                    "{} Title {}",
                    char::from(b'A' + (index % 26) as u8),
                    (index * 73) % count
                )
            };
            let mut song = test_song(&title, subtitle);
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

fn original_catalog(songs: &[Arc<SongData>]) -> Vec<SongSearchCandidate> {
    baseline::build_song_search_candidates(entries(songs), "", "dance-single")
}

fn current_catalog(songs: &[Arc<SongData>]) -> Vec<SongSearchCandidate> {
    build_song_search_candidates(entries(songs), "", "dance-single")
}

#[test]
fn search_sort_preserves_stable_ties_filters_and_retained_handles() {
    let mut songs = catalog(64, "", true);
    songs.extend(catalog(64, "(Remix)", true));
    songs.push(test_song("No Charts", ""));
    for query in [
        "",
        "title",
        "missing",
        "Pack/title",
        "[12] title",
        "[120] familiar",
        "[140] familiar",
        "title [140] [5]",
        "0063 (REMIX)",
        "remix",
    ] {
        let old = baseline::build_song_search_candidates(entries(&songs), query, "dance-single");
        let new = build_song_search_candidates(entries(&songs), query, "dance-single");
        assert_eq!(format!("{old:?}"), format!("{new:?}"), "query={query:?}");
        for (old, new) in old.iter().zip(&new) {
            assert!(Arc::ptr_eq(&old.song, &new.song));
        }
    }
    let mut ties = Vec::new();
    for (title, subtitle) in [
        ("Song", ""),
        ("SONG", " \t"),
        ("Song", ""),
        ("Song X", ""),
        ("Song", "X"),
        ("Song ", ""),
        ("Song", "X"),
    ] {
        let mut song = test_song(title, subtitle);
        Arc::get_mut(&mut song)
            .unwrap()
            .charts
            .push(test_chart("dance-single"));
        ties.push(song);
    }
    let old = baseline::build_song_search_candidates(entries(&ties), "", "dance-single");
    let new = build_song_search_candidates(entries(&ties), "", "dance-single");
    assert_eq!(old.len(), ties.len());
    for (old, new) in old.iter().zip(&new) {
        assert!(Arc::ptr_eq(&old.song, &new.song));
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn search_sort_benchmark() {
    let original =
        black_box(baseline::display_full_title_cmp as fn(&SongData, &SongData) -> Ordering);
    let current = black_box(display_full_title_cmp as fn(&SongData, &SongData) -> Ordering);
    for (name, a, a_sub, b, b_sub) in [
        ("early", "Alpha", "(Remix)", "Beta", "(Remix)"),
        ("short", "Song A", "", "Song B", ""),
        (
            "long",
            "A familiar title with repeated AAA words 0031",
            "",
            "A familiar title with repeated AAA words 0032",
            "",
        ),
        ("subtitle", "Same Title", "(Mix A)", "Same Title", "(Mix B)"),
        ("prefix", "Alpha", "B", "Alpha A", ""),
        (
            "equal",
            "A familiar title with repeated AAA words 0031",
            "(Remix)",
            "A familiar title with repeated AAA words 0031",
            "(Remix)",
        ),
        ("unicode", "日本語Été", "", "日本語Éx", ""),
    ] {
        let a = test_song(a, a_sub);
        let b = test_song(b, b_sub);
        let run = |variant, f: fn(&SongData, &SongData) -> Ordering| {
            measure_sampled(
                &format!("search-sort/compare-{name}/{variant}"),
                65536,
                1,
                || f(black_box(&a), black_box(&b)),
            );
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", current);
            run("original", original);
        } else {
            run("original", original);
            run("current", current);
        }
    }
    for (name, songs) in [
        ("plain", catalog(256, "", true)),
        ("subtitles", catalog(256, "(Remix)", true)),
        ("varied", catalog(256, "", false)),
    ] {
        let run = |variant, f: fn(&[Arc<SongData>]) -> Vec<SongSearchCandidate>| {
            measure_sampled(
                &format!("search-sort/catalog-{name}/{variant}"),
                64,
                songs.len(),
                || f(black_box(&songs)),
            );
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run(
                "current",
                black_box(current_catalog as fn(&[Arc<SongData>]) -> Vec<SongSearchCandidate>),
            );
            run(
                "original",
                black_box(original_catalog as fn(&[Arc<SongData>]) -> Vec<SongSearchCandidate>),
            );
        } else {
            run(
                "original",
                black_box(original_catalog as fn(&[Arc<SongData>]) -> Vec<SongSearchCandidate>),
            );
            run(
                "current",
                black_box(current_catalog as fn(&[Arc<SongData>]) -> Vec<SongSearchCandidate>),
            );
        }
    }
}
