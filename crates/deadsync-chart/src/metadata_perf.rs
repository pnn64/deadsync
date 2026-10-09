use super::*;
use std::hint::black_box;
#[path = "../../../tests/perf/core_alloc.rs"]
mod alloc;
#[path = "metadata_original.rs"]
mod original;
#[path = "../../../tests/perf/core_support.rs"]
mod support;

#[test]
fn full_titles_preserve_transliteration_whitespace_and_unicode() {
    let values = [
        "",
        " \t\r\n",
        "Song",
        "\u{65e5}\u{672c}\u{8a9e}",
        "\u{2003}",
    ];
    let mut song = song_data();
    for title in values {
        for subtitle in values {
            for translit_title in values {
                for translit_subtitle in values {
                    song.title = title.into();
                    song.subtitle = subtitle.into();
                    song.translit_title = translit_title.into();
                    song.translit_subtitle = translit_subtitle.into();
                    for translit in [false, true] {
                        assert_eq!(
                            original::display_full_title(&song, translit),
                            song.display_full_title(translit)
                        );
                    }
                }
            }
        }
    }
    song.title = "Keep ".into();
    song.subtitle = " subtitle ".into();
    assert_eq!(song.display_full_title(false), "Keep   subtitle ");
    for title_len in [0, 1, 7, 32, 256, 4096] {
        for subtitle_len in [0, 1, 32, 1024] {
            song.title = "a".repeat(title_len);
            song.subtitle = "b".repeat(subtitle_len);
            let (old, old_churn) = alloc::measure(|| original::display_full_title(&song, false));
            let (new, new_churn) = alloc::measure(|| song.display_full_title(false));
            assert_eq!(old, new);
            assert_eq!(new_churn.reallocs, 0);
            assert!(new_churn.allocated_bytes <= old_churn.allocated_bytes);
        }
    }
}

fn ascii_case_variants(text: &str, mut visit: impl FnMut(&str)) {
    for mask in 0..(1usize << text.len()) {
        let bytes: Vec<_> = text
            .bytes()
            .enumerate()
            .map(|(i, b)| {
                if mask & (1 << i) != 0 {
                    b.to_ascii_uppercase()
                } else {
                    b.to_ascii_lowercase()
                }
            })
            .collect();
        visit(std::str::from_utf8(&bytes).unwrap());
    }
}

#[test]
fn difficulty_dispatch_preserves_all_names_cases_and_rejections() {
    for (index, name) in STANDARD_DIFFICULTY_NAMES.iter().enumerate() {
        ascii_case_variants(name, |variant| {
            assert_eq!(standard_difficulty_index(variant), Some(index));
            assert_eq!(
                standard_difficulty_index(variant),
                original::standard_difficulty_index(variant)
            );
        });
        for position in 0..name.len() {
            for byte in 0..=127u8 {
                let mut candidate = name.as_bytes().to_vec();
                candidate[position] = byte;
                let candidate = std::str::from_utf8(&candidate).unwrap();
                assert_eq!(
                    standard_difficulty_index(candidate),
                    original::standard_difficulty_index(candidate)
                );
            }
        }
    }
    for invalid in [
        "",
        " Beginner",
        "Hard ",
        "Expert",
        "Edit",
        "\u{212a}ard",
        "\u{65e5}\u{672c}",
        "Medium\0",
    ] {
        assert_eq!(standard_difficulty_index(invalid), None);
    }
}

#[test]
fn edit_indices_preserve_filtering_sort_keys_and_stable_ties() {
    for count in [0, 1, 5, 16, 64, 128] {
        for kind in [
            "none", "wrong", "first", "last", "sparse", "mixed", "all", "unicode",
        ] {
            let song = edit_song(count, kind);
            for chart_type in ["dance-single", "DANCE-SINGLE", "pump-single", "", "other"] {
                let (old, old_churn) =
                    alloc::measure(|| original::edit_chart_indices_sorted(&song, chart_type));
                let (new, new_churn) =
                    alloc::measure(|| song.edit_chart_indices_sorted(chart_type));
                assert_eq!(old, new, "{count} {kind} {chart_type}");
                assert!(new_churn.allocs <= old_churn.allocs);
                assert!(new_churn.allocated_bytes <= old_churn.allocated_bytes);
                if new.is_empty() {
                    assert_eq!(new_churn.allocs, 0);
                }
            }
        }
    }
    let mut song = edit_song(8, "all");
    for (i, chart) in song.charts.iter_mut().enumerate() {
        chart.meter = 10;
        chart.stats.total_steps = 100;
        chart.description = ["Alpha", "ALPHA", "alpha", "aLpHa"][i % 4].into();
    }
    assert_eq!(
        song.edit_chart_indices_sorted("dance-single"),
        (0..8).collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_metadata_dataflows() {
    let old_title = black_box(original::display_full_title as fn(&SongData, bool) -> String);
    let new_title = black_box(SongData::display_full_title as fn(&SongData, bool) -> String);
    for (label, title, subtitle, translit_title, translit_subtitle) in [
        ("empty", "", "", "", ""),
        ("title-only", "Destiny", "", "Destiny T", ""),
        ("ascii", "Into the Night", "(Extended Mix)", "Night", ""),
        (
            "unicode",
            "\u{65e5}\u{672c}\u{8a9e}",
            "\u{96ea}\u{306e}\u{5922}",
            "Nihongo",
            "Yuki no Yume",
        ),
        ("blank-subtitle", "Song", " \t\r\n", "Song T", "\u{2003}"),
        ("long", "long", "long", "long", "long"),
        ("empty-title", "", "subtitle", "", "Sub"),
        (
            "translit-fallback",
            "Title",
            "Subtitle",
            " \t",
            " Alternate Subtitle",
        ),
    ] {
        let mut song = song_data();
        song.title = title.into();
        song.subtitle = subtitle.into();
        song.translit_title = translit_title.into();
        song.translit_subtitle = translit_subtitle.into();
        if label == "long" {
            song.title = "t".repeat(4096);
            song.subtitle = "s".repeat(128);
            song.translit_title = "T".repeat(4096);
            song.translit_subtitle = "S".repeat(128);
        }
        for translit in [false, true] {
            let name = format!("title-{label}-translit{translit}");
            assert_eq!(old_title(&song, translit), new_title(&song, translit));
            let (_, old) = alloc::measure(|| drop(black_box(old_title(&song, translit))));
            let (_, new) = alloc::measure(|| drop(black_box(new_title(&song, translit))));
            println!("ALLOC {name}: original {old:?}, current {new:?}");
            support::compare(
                &name,
                100,
                || {
                    black_box(old_title(black_box(&song), black_box(translit)));
                },
                || {
                    black_box(new_title(black_box(&song), black_box(translit)));
                },
            );
        }
    }
    let old_difficulty =
        black_box(original::standard_difficulty_index as fn(&str) -> Option<usize>);
    let new_difficulty = black_box(standard_difficulty_index as fn(&str) -> Option<usize>);
    for (label, value) in [
        ("beginner", "Beginner"),
        ("easy", "Easy"),
        ("medium", "Medium"),
        ("hard", "Hard"),
        ("challenge", "Challenge"),
        ("empty", ""),
        ("unknown", "Expert"),
        ("unicode", "\u{65e5}\u{672c}"),
        ("uppercase", "CHALLENGE"),
        ("mixed", ""),
    ] {
        let inputs: Vec<_> = (0..256)
            .map(|i| {
                if label == "mixed" {
                    [
                        "Beginner",
                        "Easy",
                        "Medium",
                        "Hard",
                        "Challenge",
                        "Edit",
                        "easy",
                        "UNKNOWN",
                    ][i % 8]
                } else {
                    value
                }
            })
            .collect();
        support::compare(
            &format!("difficulty-{label}"),
            100,
            || {
                for &value in black_box(&inputs) {
                    black_box(old_difficulty(black_box(value)));
                }
            },
            || {
                for &value in black_box(&inputs) {
                    black_box(new_difficulty(black_box(value)));
                }
            },
        );
    }
    let old_edits =
        black_box(original::edit_chart_indices_sorted as fn(&SongData, &str) -> Vec<usize>);
    let new_edits =
        black_box(SongData::edit_chart_indices_sorted as fn(&SongData, &str) -> Vec<usize>);
    for (label, count, kind) in [
        ("empty", 0, "none"),
        ("standard-5", 5, "none"),
        ("standard-10", 10, "none"),
        ("wrong-type-10", 10, "wrong"),
        ("one-last-6", 6, "last"),
        ("one-first-6", 6, "first"),
        ("sparse-64", 64, "sparse"),
        ("mixed-16", 16, "mixed"),
        ("all-5", 5, "all"),
        ("all-32", 32, "all"),
        ("all-128", 128, "all"),
        ("unicode-ties-32", 32, "unicode"),
    ] {
        let song = edit_song(count, kind);
        let name = format!("edits-{label}");
        assert_eq!(
            old_edits(&song, "dance-single"),
            new_edits(&song, "dance-single")
        );
        let (_, old) = alloc::measure(|| drop(black_box(old_edits(&song, "dance-single"))));
        let (_, new) = alloc::measure(|| drop(black_box(new_edits(&song, "dance-single"))));
        println!("ALLOC {name}: original {old:?}, current {new:?}");
        support::compare(
            &name,
            100,
            || {
                black_box(old_edits(black_box(&song), black_box("dance-single")));
            },
            || {
                black_box(new_edits(black_box(&song), black_box("dance-single")));
            },
        );
    }
}

fn edit_song(count: usize, kind: &str) -> SongData {
    let mut song = song_data();
    song.charts = (0..count)
        .map(|i| {
            let mut chart = chart_data();
            chart.chart_type = if kind == "wrong" {
                "pump-single"
            } else {
                "dance-single"
            }
            .into();
            let edit = match kind {
                "none" => false,
                "first" => i == 0,
                "last" => i + 1 == count,
                "sparse" => i % 16 == 15,
                "mixed" => i % 3 == 0,
                _ => true,
            };
            chart.difficulty = if edit {
                ["Edit", "EDIT", "edit"][i % 3]
            } else {
                STANDARD_DIFFICULTY_NAMES[i % 5]
            }
            .into();
            chart.meter = if kind == "unicode" {
                10
            } else {
                (i % 4 + 5) as u32
            };
            chart.stats.total_steps = if kind == "unicode" {
                100
            } else {
                ((i / 4) % 3 * 100) as u32
            };
            chart.description = [
                "same",
                "Same",
                "ALPHA",
                "alpha",
                "\u{c9}clair",
                "\u{e9}clair",
                "\u{130}",
                "i\u{307}",
            ][i % 8]
                .into();
            chart
        })
        .collect();
    song
}

fn song_data() -> SongData {
    SongData {
        simfile_path: PathBuf::from("song.ssc"),
        title: "Original".to_string(),
        subtitle: "Mix".to_string(),
        translit_title: "Translit".to_string(),
        translit_subtitle: String::new(),
        artist: String::new(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: String::new(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 120.0,
        max_bpm: 180.0,
        normalized_bpms: String::new(),
        song_timing: None,
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        last_second_hint: 0.0,
        charts: Vec::new(),
    }
}

fn chart_data() -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "Challenge".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter: 12,
        step_artist: String::new(),
        music_path: None,
        short_hash: "hash".to_string(),
        stats: Default::default(),
        tech_counts: Default::default(),
        mines_nonfake: 0,
        stamina_counts: Default::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        matrix_profile: Box::default(),
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: 0.0,
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 150.0,
        max_bpm: 210.0,
    }
}
