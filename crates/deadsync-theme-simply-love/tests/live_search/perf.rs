use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/live_search/baseline.rs"
    ));
}

fn catalog(size: usize, style: &str, packs: bool) -> SongSearchIndex {
    let mut wheel = Vec::new();
    for i in (0..size).rev() {
        let label = match style {
            "prefix" => format!("Album Collection With A Long Identical Search Prefix {i:05}"),
            "case" => format!(
                "{} Collection With A Long Identical Search Prefix {i:05}",
                if i % 2 == 0 { "ALBUM" } else { "album" }
            ),
            "unicode" => format!("Caf\u{e9} \u{6771}\u{4eac} Collection {i:05}"),
            "diverse" => format!(
                "{} Album Collection Number {i:05}",
                char::from(b'A' + (i % 26) as u8)
            ),
            _ => format!("Album {i:05}"),
        };
        if packs || wheel.is_empty() {
            wheel.push(MusicWheelEntry::PackHeader {
                name: Arc::from(if packs { label.as_str() } else { "Pack" }),
                original_index: i,
                banner_path: None,
                song_count: if packs { 1 } else { size },
                pack_key: Some(Arc::from(label.as_str())),
                parent_series: None,
            });
        }
        let mut song = test_song(&label, 120.0);
        let data = Arc::get_mut(&mut song).unwrap();
        data.display_bpm = "119.5:180.5".to_string();
        data.subtitle = format!("Subtitle {i}");
        data.charts = ["Beginner", "Easy", "Medium", "Hard", "Challenge"]
            .into_iter()
            .enumerate()
            .map(|(j, difficulty)| {
                let mut chart = test_chart((j + 1) as u32 * 3);
                chart.difficulty = difficulty.to_string();
                chart
            })
            .collect();
        wheel.push(MusicWheelEntry::Song(song));
    }
    build_song_search_index(&wheel)
}

fn same_matches(old: &[SongSearchMatch], new: &[SongSearchMatch]) {
    assert_eq!(old.len(), new.len());
    for (a, b) in old.iter().zip(new) {
        match (a, b) {
            (
                SongSearchMatch::Song {
                    candidate: a,
                    score: sa,
                },
                SongSearchMatch::Song {
                    candidate: b,
                    score: sb,
                },
            ) => {
                assert_eq!(sa, sb);
                assert_eq!(a.pack_name, b.pack_name);
                assert_eq!(a.title, b.title);
                assert_eq!(a.subtitle, b.subtitle);
                assert_eq!(a.bpm, b.bpm);
                assert_eq!(a.difficulties, b.difficulties);
                assert!(Arc::ptr_eq(&a.song, &b.song));
                assert!(Arc::ptr_eq(&a.title, &b.title));
            }
            (
                SongSearchMatch::Pack {
                    name: a,
                    song_count: ca,
                    score: sa,
                },
                SongSearchMatch::Pack {
                    name: b,
                    song_count: cb,
                    score: sb,
                },
            ) => {
                assert_eq!((a, ca, sa), (b, cb, sb));
                assert!(Arc::ptr_eq(a, b));
            }
            _ => panic!("result variant changed"),
        }
    }
}

#[test]
fn live_results_match_parent_across_queries_and_catalogs() {
    for style in ["short", "prefix", "case", "unicode", "diverse"] {
        for size in [0, 1, 8, 9, 10, 64, 1024] {
            let index = catalog(size, style, true);
            for query in [
                "",
                "album",
                "albm",
                "collection",
                "cafe",
                "\u{6771}\u{4eac}",
                "zzzz",
                "[120]",
                "[180] album",
                "[200]",
                "[12]",
                "[15][120]",
                "[120][12] al",
                "[99999999999999999999]",
                "[0]",
                "[12",
                "  [120] album  ",
                "pack/album",
            ] {
                for chart_type in ["dance-single", "DANCE-SINGLE", "pump-single"] {
                    same_matches(
                        &baseline::build_song_matches(&index, query, chart_type),
                        &build_song_matches(&index, query, chart_type),
                    );
                }
                same_matches(
                    &baseline::build_pack_matches(&index, query),
                    &build_pack_matches(&index, query),
                );
            }
        }
    }
}

#[test]
fn bpm_cache_preserves_tags_boundaries_missing_charts_and_reload() {
    let mut songs = Vec::new();
    for (i, tag) in [
        "",
        "*",
        "bad",
        "NaN",
        "inf",
        "-10",
        "0",
        "119.49",
        "119.5",
        "129.49",
        "129.5",
        "180:120",
        "120:180",
        "120:120",
        "120:bad",
        "120:180:200",
        "1e308",
    ]
    .into_iter()
    .enumerate()
    {
        let mut song = test_song(&format!("Song {i}"), 120.0);
        let data = Arc::get_mut(&mut song).unwrap();
        data.display_bpm = tag.to_string();
        if i % 3 == 0 {
            data.charts[0].difficulty = "Edit".to_string();
        }
        if i % 5 == 0 {
            data.charts[0].chart_type = "pump-single".to_string();
        }
        if i % 7 == 0 {
            data.charts.clear();
        }
        songs.push(("Pack", song));
    }
    let index = index_from(&songs);
    for query in [
        "[40]",
        "[110]",
        "[120]",
        "[130]",
        "[180]",
        "[190]",
        "[10][120]",
        "[120] song",
        "[999999999] song",
    ] {
        for chart_type in ["dance-single", "pump-single"] {
            same_matches(
                &baseline::build_song_matches(&index, query, chart_type),
                &build_song_matches(&index, query, chart_type),
            );
        }
    }
    let replacement = index_from(&[("Pack", test_song("Replacement", 200.0))]);
    assert!(build_song_matches(&replacement, "[120]", "dance-single").is_empty());
    assert_eq!(
        build_song_matches(&replacement, "[200]", "dance-single").len(),
        1
    );
}

#[test]
fn bpm_cache_is_lazy_shared_and_thread_safe() {
    let mut index = catalog(1024, "short", false);
    for entry in index.songs.iter_mut().step_by(2) {
        Arc::make_mut(&mut entry.song)
            .charts
            .retain(|chart| chart.difficulty != "Hard");
    }
    black_box(build_song_matches(&index, "album", "dance-single"));
    assert!(index.bpm_tiers.get().is_none());
    black_box(build_song_matches(&index, "[120]", "dance-single"));
    assert!(index.bpm_tiers.get().is_none());
    black_box(build_song_matches(
        &index,
        "[12][120] album",
        "dance-single",
    ));
    let slots = index.bpm_tiers.get().unwrap();
    assert_eq!(
        slots.iter().filter(|slot| slot.get().is_some()).count(),
        512
    );
    let clone = index.clone();
    assert!(Arc::ptr_eq(slots, clone.bpm_tiers.get().unwrap()));
    std::thread::scope(|scope| {
        for query in [
            "[120] album",
            "[180] album",
            "[200] album",
            "[12][120] album",
        ] {
            let index = &index;
            scope.spawn(move || {
                same_matches(
                    &baseline::build_song_matches(index, query, "dance-single"),
                    &build_song_matches(index, query, "dance-single"),
                )
            });
        }
    });
    assert!(slots.iter().all(|slot| slot.get().is_some()));
    perf::assert_no_churn(|| {
        for (slot, song) in slots.iter().zip(&index.songs) {
            black_box(slot.get_or_init(|| song_search_bpm_tiers(&song.song)));
        }
    });
}

#[test]
fn tie_order_matches_parent_for_all_bytes_and_long_names() {
    let mut strings = vec![
        String::new(),
        "A".to_string(),
        "a".to_string(),
        "A\0z".to_string(),
        "\u{6771}\u{4eac}".to_string(),
    ];
    for byte in 0..=127 {
        strings.push(char::from(byte).to_string());
    }
    for n in [3, 4, 15, 16, 31, 32, 33, 63, 64, 65, 256] {
        for suffix in ["", "A", "a", "B", "\u{e9}", "\u{6771}"] {
            strings.push(format!("{}{suffix}", "Shared Prefix ".repeat(n)));
        }
    }
    for a in &strings {
        for b in &strings {
            assert_eq!(baseline::cmp_ascii_ci(a, b), cmp_ascii_ci(a, b));
        }
    }
}

#[test]
fn reusable_formatters_match_parent_and_clear_previous_contents() {
    use deadsync_chart::song::{format_display_bpm_range, format_display_bpm_range_into};
    let values = [
        0.0,
        -0.0,
        -150.5,
        119.499999,
        119.5,
        120.0,
        120.0000001,
        180.5,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    let mut scratch = String::from("previous contents");
    for rate in [
        0.0,
        -1.0,
        1.0,
        1.0009,
        1.0011,
        1.5,
        f32::MAX,
        f32::INFINITY,
        f32::NAN,
    ] {
        for lo in values {
            for hi in values {
                let range = Some((lo, hi));
                let expected = baseline::format_display_bpm_range(range, rate);
                format_display_bpm_range_into(range, rate, &mut scratch);
                assert_eq!(scratch, expected);
                assert_eq!(format_display_bpm_range(range, rate), expected);
            }
        }
    }
    format_display_bpm_range_into(None, 1.0, &mut scratch);
    assert!(scratch.is_empty());
    let mut song = test_song("Song", 120.0);
    let data = Arc::get_mut(&mut song).unwrap();
    for (i, name) in [
        "Challenge",
        "Hard",
        "Medium",
        "Easy",
        "Beginner",
        "hard",
        "Edit",
        "Hardly",
        "",
    ]
    .into_iter()
    .enumerate()
    {
        let mut chart = test_chart(if i < 5 { u32::MAX - i as u32 } else { 0 });
        chart.difficulty = name.to_string();
        data.charts.push(chart);
    }
    for chart_type in ["dance-single", "DANCE-SINGLE", "pump-single", ""] {
        let expected = baseline::song_search_difficulties_text(&song, chart_type);
        song_search_difficulties_text_into(&song, chart_type, &mut scratch);
        assert_eq!(scratch, expected);
        assert_eq!(
            deadsync_simfile::song_search::song_search_difficulties_text(&song, chart_type),
            expected
        );
    }
    // Warmed storage accommodates every u32 meter and ordinary display ranges.
    scratch.reserve(64);
    perf::assert_no_churn(|| {
        for _ in 0..32 {
            format_display_bpm_range_into(Some((120.0, 180.0)), 1.5, &mut scratch);
            song_search_difficulties_text_into(&song, "dance-single", &mut scratch);
            song_search_difficulties_text_into(&song, "missing", &mut scratch);
            format_display_bpm_range_into(None, 1.0, &mut scratch);
        }
    });
}

#[test]
fn visible_rows_reduce_complete_owning_churn() {
    let index = catalog(64, "short", false);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::build_song_matches(&index, "", "dance-single"));
        },
        || {
            black_box(build_song_matches(&index, "", "dance-single"));
        },
    );
    let empty = catalog(0, "short", false);
    perf::assert_no_churn(|| {
        black_box(build_song_matches(&empty, "", "dance-single"));
    });
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_live_search() {
    for style in ["short", "prefix", "case", "unicode", "diverse"] {
        let packs = catalog(4096, style, true);
        pairs(|label, new| {
            perf::measure_sampled(&format!("live_pack_{style}_{label}"), 64, 4096, || {
                if new {
                    build_pack_matches(black_box(&packs), black_box(""))
                } else {
                    baseline::build_pack_matches(black_box(&packs), black_box(""))
                }
            })
        });
        let songs = catalog(4096, style, false);
        let query = if style == "unicode" { "cafe" } else { "album" };
        pairs(|label, new| {
            perf::measure_sampled(&format!("live_song_{style}_{label}"), 32, 4096, || {
                if new {
                    build_song_matches(black_box(&songs), black_box(query), "dance-single")
                } else {
                    baseline::build_song_matches(
                        black_box(&songs),
                        black_box(query),
                        "dance-single",
                    )
                }
            })
        });
    }
    for size in [1, 9, 4096] {
        let index = catalog(size, "short", false);
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("live_rows_{size}_{label}"),
                1024,
                size.min(9),
                || {
                    if new {
                        build_song_matches(black_box(&index), black_box(""), "dance-single")
                    } else {
                        baseline::build_song_matches(
                            black_box(&index),
                            black_box(""),
                            "dance-single",
                        )
                    }
                },
            )
        });
    }
    let index = catalog(4096, "short", false);
    for (tag, query) in [
        ("hit", "[120] album"),
        ("miss", "[200] album"),
        ("no_text", "[120]"),
        ("difficulty", "[12][120] album"),
    ] {
        black_box(build_song_matches(&index, query, "dance-single"));
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("live_bpm_warm_{tag}_{label}"),
                64,
                if tag == "no_text" { 9 } else { 4096 },
                || {
                    if new {
                        build_song_matches(black_box(&index), black_box(query), "dance-single")
                    } else {
                        baseline::build_song_matches(
                            black_box(&index),
                            black_box(query),
                            "dance-single",
                        )
                    }
                },
            )
        });
        pairs(|label, new| {
            perf::measure_sampled_with_setup(
                &format!("live_bpm_cold_{tag}_{label}"),
                64,
                if tag == "no_text" { 9 } else { 4096 },
                || {
                    let mut fresh = index.clone();
                    fresh.bpm_tiers = OnceLock::new();
                    fresh
                },
                |fresh| {
                    if new {
                        black_box(build_song_matches(
                            black_box(fresh),
                            black_box(query),
                            "dance-single",
                        ));
                    } else {
                        black_box(baseline::build_song_matches(
                            black_box(fresh),
                            black_box(query),
                            "dance-single",
                        ));
                    }
                },
            )
        });
    }
    eprintln!(
        "cache storage: {} bytes/slot, {} bytes/index cache field",
        std::mem::size_of::<BpmTierSlot>(),
        std::mem::size_of::<OnceLock<Arc<[BpmTierSlot]>>>()
    );
}
