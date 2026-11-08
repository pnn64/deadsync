mod selection_original {
    include!("song_search_selection_original.rs");
}

fn assert_search_candidates_equal(old: &[SongSearchCandidate], new: &[SongSearchCandidate]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert!(Arc::ptr_eq(&old.song, &new.song));
        assert_eq!(
            (
                &old.pack_name,
                &old.title,
                &old.subtitle,
                &old.bpm,
                &old.difficulties
            ),
            (
                &new.pack_name,
                &new.title,
                &new.subtitle,
                &new.bpm,
                &new.difficulties
            )
        );
    }
}

#[test]
fn selection_search_matches_original_transliteration_and_filters() {
    let mut songs = Vec::new();
    for title in ["Alpha", "", "\u{66f2}", "Long Repeated Title"] {
        for subtitle in ["", "Mix", "\u{2003}"] {
            for translit_title in ["", "Alpha", "Other", "\u{2003}"] {
                for translit_subtitle in ["", "Mix", "Other", "\u{a0}"] {
                    let mut song = (*test_song(title, subtitle)).clone();
                    song.translit_title = translit_title.into();
                    song.translit_subtitle = translit_subtitle.into();
                    song.charts = vec![test_chart("dance-single")];
                    songs.push(Arc::new(song));
                }
            }
        }
    }
    for query in [
        "",
        "alpha",
        "mix",
        "alpha mix",
        "other mix",
        "alpha other",
        " other",
        "\u{66f2}",
        "missing",
        "repeat",
        "[12]",
        "[13]",
        "[130]",
        "[90]",
        "pack/alpha",
        "other/alpha",
    ] {
        let entries = || {
            std::iter::once(SongSearchCatalogEntry::PackHeader("Pack"))
                .chain(songs.iter().map(SongSearchCatalogEntry::Song))
        };
        assert_search_candidates_equal(
            &selection_original::build_song_search_candidates(entries(), query, "dance-single"),
            &build_song_search_candidates(entries(), query, "dance-single"),
        );
    }
}

#[test]
fn selection_search_reuses_identical_titles_without_allocating() {
    let song = test_song("Ordinary Title", "Extended Mix");
    for query in [
        "missing",
        "title extended",
        "ORDINARY",
        "",
        "too long to match this song at all",
    ] {
        let (old, old_churn) = crate::metadata_perf::measure(|| {
            selection_original::song_title_contains(&song, false, query)
                || selection_original::song_title_contains(&song, true, query)
        });
        let (new, new_churn) = crate::metadata_perf::measure(|| song_title_contains(&song, query));
        assert_eq!(old, new);
        assert_eq!(old_churn.allocs + old_churn.reallocs, 0);
        assert_eq!(new_churn.allocs + new_churn.reallocs, 0);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_selection_search() {
    use crate::metadata_perf::compare;
    use std::hint::black_box;
    for (name, title, subtitle, translit_title, translit_subtitle, query) in [
        (
            "fallback-miss",
            "An Ordinary Song Title",
            "Extended Mix",
            "",
            "",
            "missing",
        ),
        (
            "identical-miss",
            "An Ordinary Song Title",
            "Extended Mix",
            "An Ordinary Song Title",
            "Extended Mix",
            "missing",
        ),
        (
            "whitespace-miss",
            "An Ordinary Song Title",
            "Extended Mix",
            "\u{2003}",
            "\u{a0}",
            "missing",
        ),
        (
            "original-hit",
            "An Ordinary Song Title",
            "Extended Mix",
            "",
            "",
            "ordinary",
        ),
        (
            "boundary-hit",
            "An Ordinary Song Title",
            "Extended Mix",
            "",
            "",
            "title extended",
        ),
        (
            "translit-hit",
            "\u{66f2}",
            "Extended Mix",
            "Alternate Title",
            "",
            "alternate",
        ),
        (
            "translit-miss",
            "An Ordinary Song Title",
            "Extended Mix",
            "Alternate Title",
            "Alternate Mix",
            "missing",
        ),
        ("short-miss", "A", "", "", "", "z"),
        ("empty", "", "", "", "", "missing"),
    ] {
        let mut song = (*test_song(title, subtitle)).clone();
        song.translit_title = translit_title.into();
        song.translit_subtitle = translit_subtitle.into();
        compare(
            &format!("search-title-{name}"),
            2000,
            || {
                black_box(
                    selection_original::song_title_contains(
                        black_box(&song),
                        false,
                        black_box(query),
                    ) || selection_original::song_title_contains(
                        black_box(&song),
                        true,
                        black_box(query),
                    ),
                );
            },
            || {
                black_box(song_title_contains(black_box(&song), black_box(query)));
            },
        );
    }
    for (name, translit, query) in [
        ("fallback-miss", false, "missing"),
        ("translit-miss", true, "missing"),
        ("all-match", false, "ordinary"),
        ("selective", false, "00031"),
    ] {
        let songs: Vec<_> = (0..1024)
            .map(|i| {
                let mut song =
                    (*test_song(&format!("An Ordinary Song {i:05}"), "Extended Mix")).clone();
                song.charts = vec![test_chart("dance-single")];
                if translit {
                    song.translit_title = format!("Alternate Song {i:05}");
                }
                Arc::new(song)
            })
            .collect();
        compare(
            &format!("search-catalog-{name}-1024"),
            8,
            || {
                black_box(selection_original::build_song_search_candidates(
                    black_box(&songs).iter().map(SongSearchCatalogEntry::Song),
                    black_box(query),
                    "dance-single",
                ));
            },
            || {
                black_box(build_song_search_candidates(
                    black_box(&songs).iter().map(SongSearchCatalogEntry::Song),
                    black_box(query),
                    "dance-single",
                ));
            },
        );
    }
}

#[test]
fn selection_candidate_meter_storage_matches_original_and_reduces_churn() {
    let songs: Vec<_> = (0..128)
        .map(|i| {
            let mut song = (*test_song("Song", "Mix")).clone();
            for (difficulty, meter) in [
                ("Beginner", 1),
                ("Easy", 4),
                ("Medium", 8),
                ("Hard", 12),
                ("Challenge", u32::MAX),
            ] {
                let mut chart = test_chart("dance-single");
                chart.difficulty = difficulty.into();
                chart.meter = meter;
                song.charts.push(chart);
            }
            if i % 3 == 1 {
                song.charts.truncate(1);
            }
            if i % 3 == 2 {
                song.charts[0].difficulty = "Edit".into();
                song.charts.truncate(1);
            }
            Arc::new(song)
        })
        .collect();
    let entries = || songs.iter().map(SongSearchCatalogEntry::Song);
    let (old, old_churn) = crate::metadata_perf::measure(|| {
        selection_original::build_song_search_candidates(entries(), "", "dance-single")
    });
    let (new, new_churn) = crate::metadata_perf::measure(|| {
        build_song_search_candidates(entries(), "", "dance-single")
    });
    assert_search_candidates_equal(&old, &new);
    assert_eq!(old_churn.allocs - new_churn.allocs, songs.len() - 1);
    assert!(new_churn.allocated_bytes < old_churn.allocated_bytes);
    assert_eq!(new[0].difficulties.as_ref(), "1   4   8   12   4294967295");
    assert_eq!(new[1].difficulties.as_ref(), "1");
    assert_eq!(new[2].difficulties.as_ref(), "-");
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_selection_candidate_meters() {
    use crate::metadata_perf::{compare, measure};
    use std::hint::black_box;
    for count in [1, 16, 1024] {
        for (name, meters) in [
            ("standard", [1, 4, 8, 12, 15]),
            ("wide", [u32::MAX; 5]),
            ("edits", [12; 5]),
        ] {
            let songs: Vec<_> = (0..count)
                .map(|i| {
                    let mut song = (*test_song(&format!("Song {i:05}"), "Mix")).clone();
                    song.charts = ["Beginner", "Easy", "Medium", "Hard", "Challenge"]
                        .into_iter()
                        .zip(meters)
                        .map(|(difficulty, meter)| {
                            let mut chart = test_chart("dance-single");
                            chart.difficulty =
                                if name == "edits" { "Edit" } else { difficulty }.into();
                            chart.meter = meter;
                            chart
                        })
                        .collect();
                    Arc::new(song)
                })
                .collect();
            let entries = || songs.iter().map(SongSearchCatalogEntry::Song);
            let label = format!("candidate-meters-{name}-{count}");
            let (old, old_churn) = measure(|| {
                selection_original::build_song_search_candidates(entries(), "", "dance-single")
            });
            let (new, new_churn) =
                measure(|| build_song_search_candidates(entries(), "", "dance-single"));
            assert_search_candidates_equal(&old, &new);
            println!("ALLOC {label}: original {old_churn:?}, current {new_churn:?}");
            drop((old, new));
            compare(
                &label,
                if count < 100 { 128 } else { 8 },
                || {
                    black_box(selection_original::build_song_search_candidates(
                        black_box(&songs).iter().map(SongSearchCatalogEntry::Song),
                        "",
                        "dance-single",
                    ));
                },
                || {
                    black_box(build_song_search_candidates(
                        black_box(&songs).iter().map(SongSearchCatalogEntry::Song),
                        "",
                        "dance-single",
                    ));
                },
            );
        }
    }
}
