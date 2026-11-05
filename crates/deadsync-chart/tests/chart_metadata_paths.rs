use deadsync_chart::chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
use deadsync_chart::{SongData, notes::step_type_lanes};
use std::hint::black_box;
use std::path::PathBuf;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/metadata_paths_original.rs"]
mod original;
#[path = "../../../tests/perf/core_alloc.rs"]
mod perf_alloc;

fn song(title: String, subtitle: String) -> SongData {
    SongData {
        simfile_path: PathBuf::new(),
        title,
        subtitle,
        translit_title: String::new(),
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
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_bpms: String::new(),
        song_timing: None,
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        precise_note_last_second_seconds: None,
        last_second_hint: 0.0,
        charts: Vec::new(),
    }
}

#[test]
fn lanes_match_frozen_dispatch_for_case_mutations_and_unicode() {
    for base in [
        "dance-single",
        "dance-double",
        "dance-couple",
        "dance-routine",
        "pump-single",
        "pump-double",
        "pump-couple",
        "pump-routine",
    ] {
        for mask in 0..1usize << base.len() {
            let text: String = base
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    char::from(if mask & (1 << i) == 0 {
                        b
                    } else {
                        b.to_ascii_uppercase()
                    })
                })
                .collect();
            for text in [
                text.clone(),
                text.replace('-', "_"),
                format!("\u{2003}{text}\r\n"),
            ] {
                assert_eq!(
                    step_type_lanes(&text),
                    original::step_type_lanes(&text),
                    "{text:?}"
                );
            }
        }
        for i in 0..base.len() {
            for byte in 0..=127 {
                let mut bytes = base.as_bytes().to_vec();
                bytes[i] = byte;
                let text = std::str::from_utf8(&bytes).unwrap();
                assert_eq!(
                    step_type_lanes(text),
                    original::step_type_lanes(text),
                    "{text:?}"
                );
            }
            for ch in ["\u{e9}", "\u{130}", "\u{2003}", "\u{96ea}"] {
                let text = format!("{}{ch}{}", &base[..i], &base[i + 1..]);
                assert_eq!(
                    step_type_lanes(&text),
                    original::step_type_lanes(&text),
                    "{text:?}"
                );
            }
        }
    }
}

#[test]
fn no_cmod_matches_for_boundaries_controls_and_multibyte_text() {
    let mut input = song(String::new(), String::new());
    for title in [
        "",
        "no",
        "NO",
        "no ",
        "no cmod",
        "NO CMOD",
        "nono cmod",
        "\u{96ea} no",
        "snow",
        "\0no",
        "\u{2003}",
    ] {
        for subtitle in [
            "", " ", "\u{2003}", "cmod", "CMOD", " cmod", "no cmod", "no\0cmod", "\u{96ea}",
        ] {
            input.title = title.into();
            input.subtitle = subtitle.into();
            assert_eq!(
                input.is_no_cmod(),
                original::is_no_cmod(&input),
                "{title:?} {subtitle:?}"
            );
        }
    }
    for i in 0..7 {
        for byte in 0..=127 {
            let mut text = b"no cmod".to_vec();
            text[i] = byte;
            let text = String::from_utf8(text).unwrap();
            for split in 0..=text.len() {
                input.title = text[..split].into();
                input.subtitle = text[split..].into();
                assert_eq!(input.is_no_cmod(), original::is_no_cmod(&input));
            }
        }
    }
    let mut state = 0x1234_5678_9abc_def0u64;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        input.title = (0..state as usize % 80)
            .map(|i| {
                char::from(
                    b"nNoO cCmMdD_\t\0"
                        [(state.rotate_left(i as u32) as usize) % b"nNoO cCmMdD_\t\0".len()],
                )
            })
            .collect();
        input.subtitle = input.title.chars().rev().collect();
        assert_eq!(
            input.is_no_cmod(),
            original::is_no_cmod(&input),
            "{:?} {:?}",
            input.title,
            input.subtitle
        );
    }
}

#[test]
fn standard_difficulty_presence_preserves_types_names_and_missing_indices() {
    let mut input = song(String::new(), String::new());
    for kind in [
        "dance-single",
        "DANCE_SINGLE",
        "pump-double",
        "\u{96ea}",
        "",
    ] {
        for difficulty in [
            "Beginner",
            "easy",
            "MEDIUM",
            "hArD",
            "Challenge",
            "Edit",
            "",
            "\u{130}ard",
        ] {
            let mut chart = chart_data();
            chart.chart_type = kind.into();
            chart.difficulty = difficulty.into();
            input.charts.push(chart);
            for query_type in [
                "dance-single",
                "DANCE-SINGLE",
                "pump-double",
                "\u{96ea}",
                "unknown",
                "",
            ] {
                for index in [0, 1, 2, 3, 4, 5, usize::MAX] {
                    assert_eq!(
                        input.has_standard_difficulty(query_type, index),
                        original::has_standard_difficulty(&input, query_type, index)
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "release paired CPU and allocation benchmark"]
fn benchmark_chart_metadata_paths() {
    for kind in ["standard", "mixed", "missing"] {
        let mut input = song(String::new(), String::new());
        let count = if kind == "standard" { 5 } else { 64 };
        for i in 0..count {
            let mut chart = chart_data();
            chart.chart_type = if kind == "standard" || i % 3 == 0 {
                "dance-single"
            } else {
                "pump-double"
            }
            .into();
            chart.difficulty = deadsync_chart::STANDARD_DIFFICULTY_NAMES[i % 5].into();
            input.charts.push(chart);
        }
        let query = if kind == "missing" {
            "unknown-chart-type"
        } else {
            "dance-single"
        };
        let (_, before) = crate::perf_alloc::measure(|| {
            for _ in 0..256 {
                black_box(original::has_standard_difficulty(&input, query, 4));
            }
        });
        let (_, after) = crate::perf_alloc::measure(|| {
            for _ in 0..256 {
                black_box(input.has_standard_difficulty(query, 4));
            }
        });
        assert_eq!(before, after);
        assert_eq!(after, crate::perf_alloc::Churn::default());
        bench::compare(&format!("chart/presence/{kind}/256"), 500, |current| {
            let check: fn(&SongData, &str, usize) -> bool = black_box(if current {
                SongData::has_standard_difficulty
            } else {
                original::has_standard_difficulty
            });
            for _ in 0..256 {
                black_box(check(black_box(&input), black_box(query), black_box(4)));
            }
        });
    }
    for (kind, choices) in [
        (
            "common",
            vec![
                "dance-single",
                "dance-double",
                "pump-single",
                "pump-double",
                "dance-couple",
                "pump-couple",
                "dance-routine",
                "pump-routine",
            ],
        ),
        (
            "fallback",
            vec![
                "",
                "unknown",
                "dance__double",
                "pump-halfdouble",
                "\u{2003}PUMP_SINGLE\r\n",
                "dance-doubl\u{e9}",
            ],
        ),
    ] {
        let inputs: Vec<_> = (0..256).map(|i| choices[i % choices.len()]).collect();
        for current in [false, true] {
            let (_, counts) = crate::perf_alloc::measure(|| {
                for input in &inputs {
                    black_box(if current {
                        step_type_lanes(input)
                    } else {
                        original::step_type_lanes(input)
                    });
                }
            });
            assert_eq!(counts, crate::perf_alloc::Churn::default());
        }
        bench::compare(&format!("chart/lanes/{kind}/256"), 1000, |current| {
            let parse: fn(&str) -> usize = black_box(if current {
                step_type_lanes
            } else {
                original::step_type_lanes
            });
            for input in black_box(&inputs) {
                black_box(parse(black_box(input)));
            }
        });
    }
    for kind in ["plain", "tagged", "long"] {
        let songs: Vec<_> = (0..256)
            .map(|i| match kind {
                "plain" => song(
                    format!("Snowfall Night - Chapter {i}"),
                    "Final Version".into(),
                ),
                "tagged" => match i % 3 {
                    0 => song(format!("Song {i} (NO CMOD)"), String::new()),
                    1 => song(format!("Song {i} no"), "cmod".into()),
                    _ => song(format!("Song {i}"), "no cmod".into()),
                },
                _ => song(
                    format!("{} {i}", "\u{96ea}Hyperdrive ".repeat(12)),
                    "Extra mix".into(),
                ),
            })
            .collect();
        for current in [false, true] {
            let (_, counts) = crate::perf_alloc::measure(|| {
                for input in &songs {
                    black_box(if current {
                        input.is_no_cmod()
                    } else {
                        original::is_no_cmod(input)
                    });
                }
            });
            assert_eq!(counts, crate::perf_alloc::Churn::default());
        }
        bench::compare(&format!("chart/cmod/{kind}/256"), 500, |current| {
            let check: fn(&SongData) -> bool = black_box(if current {
                SongData::is_no_cmod
            } else {
                original::is_no_cmod
            });
            for input in black_box(&songs) {
                black_box(check(black_box(input)));
            }
        });
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
        music_file: String::new(),
        music_path: None,
        short_hash: "hash".to_string(),
        stats: ArrowStats::default(),
        tech_counts: TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: StaminaCounts::default(),
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
