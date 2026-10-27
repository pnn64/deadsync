mod text_original {
    include!("song_text_original.rs");
}

#[path = "../../../tests/support/kernel_bench.rs"]
mod text_bench;

#[test]
fn song_text_marker_preserves_case_unicode_whitespace_and_join_boundaries() {
    let mut song = (*test_song("", "")).clone();
    let check = |song: &SongData| {
        assert_eq!(
            song.is_no_cmod(),
            text_original::title_subtitle_contains_ignore_ascii_case(
                &song.title,
                &song.subtitle,
                "no cmod",
            ),
            "{:?} / {:?}",
            song.title,
            song.subtitle
        );
    };
    for mask in 0..128 {
        let marker: String = b"no cmod"
            .iter()
            .enumerate()
            .map(|(i, b)| {
                char::from(if mask & (1 << i) == 0 {
                    *b
                } else {
                    b.to_ascii_uppercase()
                })
            })
            .collect();
        for split in 0..=marker.len() {
            for prefix in ["", "Song ", "\u{96ea}\u{e9} "] {
                song.title = format!("{prefix}{}", &marker[..split]);
                song.subtitle = marker[split..].to_owned();
                check(&song);
                song.subtitle = format!(" {} ", song.subtitle);
                check(&song);
            }
        }
        for position in 0..=marker.len() {
            for byte in 0..=127 {
                song.title = format!(
                    "{}{}{}",
                    &marker[..position],
                    char::from(byte),
                    &marker[position..]
                );
                song.subtitle.clear();
                check(&song);
                std::mem::swap(&mut song.title, &mut song.subtitle);
                check(&song);
            }
        }
    }
    for title in [
        "",
        "No",
        "no ",
        "NO CMOD",
        "\u{96ea}",
        "\0no cmod",
        "Ordinary Title",
    ] {
        for subtitle in [
            "", "Cmod", " cmod", "\u{2003}", "\u{a0}", "No Cmod", "\u{96ea}",
        ] {
            song.title = title.into();
            song.subtitle = subtitle.into();
            check(&song);
        }
    }
}

#[test]
fn song_text_contains_preserves_utf8_empty_inputs_and_dense_candidates() {
    let check = |haystack: &str, needle: &str| {
        assert_eq!(
            contains_ignore_ascii_case(haystack, needle),
            text_original::contains_ignore_ascii_case(haystack, needle),
            "{haystack:?} / {needle:?}"
        );
    };
    for len in [0, 1, 2, 7, 15, 16, 31, 32, 33, 63, 64, 65, 127, 128, 256] {
        for byte in 0..=127 {
            let base = char::from(byte).to_string().repeat(len);
            for needle in ["", "x", "X", "xy", "needle", "\0", "\u{96ea}", "\u{e9}"] {
                check(&base, needle);
            }
            for index in 0..=len {
                let text = format!("{}nEeDlE{}", &base[..index], &base[index..]);
                check(&text, "needle");
                check(&text, "NEEDLE");
            }
        }
        for base in ["n", "NE", "\u{96ea}", "\u{e9}", "\u{212a}", "\u{1f600}"] {
            let text = base.repeat(len);
            for needle in [
                "",
                "n",
                "needle",
                "neNE",
                "\u{96ea}",
                "\u{e9}",
                "\u{212a}",
                "\u{1f600}",
                "\u{96ea} \u{e9}",
            ] {
                check(&text, needle);
            }
        }
    }
    let text = "A\u{96ea}B\u{e9}C\u{1f600}d".repeat(16);
    let boundaries: Vec<_> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    for &start in &boundaries {
        for &end in boundaries.iter().filter(|&&end| end >= start) {
            check(&text, &text[start..end]);
        }
    }
}

fn check_text_query(query: &str) {
    let filter = parse_song_search_filter(query);
    let state = format!(
        "{:?}",
        (
            &filter.terms,
            &filter.pack_term,
            &filter.song_term,
            filter.difficulty,
            filter.bpm_tier
        )
    );
    assert_eq!(state, text_original::filter_debug(query), "{query:?}");
    let old = text_original::parse_song_search_live(query);
    let new = parse_song_search_live(query);
    assert_eq!(
        (new.text, new.difficulty, new.bpm_tier),
        (old.text, old.difficulty, old.bpm_tier),
        "{query:?}"
    );
}

#[test]
fn song_text_queries_preserve_grammar_overflow_ranges_and_filter_order() {
    for token in [
        "",
        "[]",
        "[0]",
        "[35]",
        "[36]",
        "[130]",
        "[4294967295]",
        "[4294967296]",
        "[999999999999999999999999]",
        "[-1]",
        "[+1]",
        "[ 1]",
        "[1 ]",
        "[1x]",
        "[[12]",
        "[12]]",
        "[\u{ff11}]",
        "[\u{661}]",
    ] {
        for left in [
            "",
            "  ",
            "Pack/",
            "/Song",
            "A/B/C",
            "\u{96ea}\u{e9}",
            "\u{2003} A",
        ] {
            for right in ["", " ", "/Tail", "[11]", "[180]", "[2][90]", "\u{a0}"] {
                check_text_query(&format!("{left}{token}{right}"));
            }
        }
    }
    for value in 0..256 {
        check_text_query(&format!(" X [{value}] Y [{}] Z [{value}] ", 255 - value));
    }
    for byte in 0..=127 {
        for position in 0..=8 {
            let base = "Ab[12]/X";
            check_text_query(&format!(
                "{}{}{}",
                &base[..position],
                char::from(byte),
                &base[position..]
            ));
        }
    }
    for len in [1, 2, 8, 32, 256] {
        check_text_query(&"Abc[11]".repeat(len));
        check_text_query(&"[99999999999999999x[35]".repeat(len));
        check_text_query(&"\u{96ea}\u{e9}/A[36]b".repeat(len));
    }
}

fn text_catalog() -> Vec<Arc<SongData>> {
    (0..64)
        .map(|i| {
            let mut song = (*test_song(
                &format!("{} {i}", "ab".repeat(32)),
                if i % 2 == 0 { "Extended Mix" } else { "" },
            ))
            .clone();
            if i % 3 == 0 {
                song.translit_title = format!("Translated Needle {i}");
            }
            song.charts = vec![test_chart("dance-single")];
            song.charts[0].meter = 1 + i % 35;
            Arc::new(song)
        })
        .collect()
}

#[test]
fn song_text_catalog_preserves_fields_identity_filters_and_order() {
    let songs = text_catalog();
    for query in [
        "",
        "missing",
        "needle",
        "mix",
        "ab ab",
        "pack/needle",
        "missing/",
        "[12]",
        "[36]",
        "[130]",
        "[128]",
        " / [35]",
        "\u{96ea}",
    ] {
        for chart_type in ["dance-single", "DANCE-SINGLE", "pump-single"] {
            let new = build_song_search_candidates(
                std::iter::once(SongSearchCatalogEntry::PackHeader("Pack"))
                    .chain(songs.iter().map(SongSearchCatalogEntry::Song)),
                query,
                chart_type,
            );
            let old = text_original::build_song_search_candidates(
                std::iter::once(text_original::SongSearchCatalogEntry::PackHeader("Pack")).chain(
                    songs
                        .iter()
                        .map(text_original::SongSearchCatalogEntry::Song),
                ),
                query,
                chart_type,
            );
            assert_eq!(new.len(), old.len(), "{query:?}");
            for (new, old) in new.iter().zip(&old) {
                assert!(Arc::ptr_eq(&new.song, &old.song));
                assert_eq!(
                    (
                        &new.pack_name,
                        &new.title,
                        &new.subtitle,
                        &new.bpm,
                        &new.difficulties
                    ),
                    (
                        &old.pack_name,
                        &old.title,
                        &old.subtitle,
                        &old.bpm,
                        &old.difficulties
                    )
                );
            }
        }
    }
}

fn text_measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = crate::metadata_perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    text_bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_song_text() {
    use std::hint::black_box;
    for (label, title, subtitle) in [
        (
            "marker/ordinary",
            "Ordinary Title".to_owned(),
            "Extended Mix",
        ),
        ("marker/long_missing", "a".repeat(128), "Extended Mix"),
        ("marker/early", "No Cmod Song".to_owned(), ""),
        ("marker/boundary", "Song No".to_owned(), "Cmod Edit"),
        ("marker/short_missing", "Noice".to_owned(), "Mix"),
    ] {
        let song = test_song(&title, subtitle);
        text_measure(label, 128, || {
            for _ in 0..256 {
                black_box(black_box(&song).is_no_cmod());
            }
        });
    }
    for (label, title, needle) in [
        ("contains/short", "Ordinary Title".to_owned(), "missing"),
        ("contains/long_missing", "a".repeat(128), "missing"),
        (
            "contains/late",
            format!("{}nEeDlE", "ab".repeat(64)),
            "needle",
        ),
        ("contains/dense", "n".repeat(128), "needle"),
        (
            "contains/early",
            format!("needle{}", "x".repeat(128)),
            "needle",
        ),
        (
            "contains/unicode",
            format!("{}\u{e9}", "\u{96ea}".repeat(64)),
            "\u{e9}",
        ),
    ] {
        text_measure(label, 128, || {
            for _ in 0..512 {
                black_box(contains_ignore_ascii_case(
                    black_box(&title),
                    black_box(needle),
                ));
            }
        });
    }
    for (label, query) in [
        ("query/plain", "Ordinary Typeahead Query".to_owned()),
        ("query/filtered", "Pack/Song [12] [130]".to_owned()),
        ("query/tiny", "abc".to_owned()),
        ("query/unicode", "\u{96ea}\u{e9} [42]".to_owned()),
        ("query/many", "Abc[11]".repeat(32)),
        ("query/invalid", "Abc[99999999999999x".repeat(16)),
    ] {
        text_measure(label, 128, || {
            for _ in 0..128 {
                black_box(parse_song_search_live(black_box(&query)));
            }
        });
    }
    let songs = text_catalog();
    for (label, query) in [("catalog/miss", "missing"), ("catalog/hit", "needle")] {
        text_measure(label, 16, || {
            let entries = std::iter::once(SongSearchCatalogEntry::PackHeader("Pack"))
                .chain(black_box(&songs).iter().map(SongSearchCatalogEntry::Song));
            black_box(build_song_search_candidates(
                entries,
                black_box(query),
                "dance-single",
            ));
        });
    }
    text_measure("control/difficulty", 128, || {
        for _ in 0..512 {
            black_box(search_standard_difficulty_index(black_box("Challenge")));
        }
    });
}
