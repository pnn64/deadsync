use super::*;
use crate::perf_alloc as alloc;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "song_sort_original.rs"]
mod original;

#[test]
fn lowercase_comparisons_preserve_unicode_expansions_and_prefix_order() {
    let mut names: Vec<String> = [
        "",
        "a",
        "A",
        "abc",
        "ABCD",
        "a\0b",
        "é",
        "É",
        "İ",
        "i\u{307}",
        "Σ",
        "σ",
        "ς",
        "K",
        "K",
        "ẞ",
        "ß",
        "東京",
        "Zebra",
        "aé",
        "AÉ",
        "aİ",
        "Ai\u{307}",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for code in 0..=0x10ffff {
        if let Some(ch) = char::from_u32(code)
            && ch.to_lowercase().ne(std::iter::once(ch))
        {
            names.push(format!("Prefix{ch}tail"));
            names.push(format!("prefix{}tail", ch.to_lowercase()));
        }
    }
    for left in &names {
        for right in &names[..24] {
            assert_eq!(
                lowercase_cmp(left, right),
                original::lowercase_cmp(left, right)
            );
            assert_eq!(
                lowercase_cmp(right, left),
                original::lowercase_cmp(right, left)
            );
        }
    }
    let mut state = 0x243f6a8885a308d3u64;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let left = &names[state as usize % names.len()];
        let right = &names[(state >> 32) as usize % names.len()];
        assert_eq!(
            lowercase_cmp(left, right),
            original::lowercase_cmp(left, right)
        );
    }
    for len in [0, 1, 7, 32, 256, 4096] {
        let prefix = "Aa".repeat(len);
        for tail in ["", "A", "a", "é", "É", "İ", "i\u{307}"] {
            let left = format!("{prefix}{tail}");
            let right = format!("{prefix}a");
            assert_eq!(
                lowercase_cmp(&left, &right),
                original::lowercase_cmp(&left, &right)
            );
        }
    }
}

#[test]
fn edit_predicate_and_sort_preserve_all_keys_and_stable_ties() {
    let mut chart = chart_data();
    for kind in [
        "",
        "dance-single",
        "DANCE-SINGLE",
        "pump-single",
        "é",
        " dance-single",
    ] {
        chart.chart_type = kind.into();
        for difficulty in ["", "Edit", "EDIT", "edit", "Hard", "edit ", "Édit", "edİt"] {
            chart.difficulty = difficulty.into();
            for query in ["", "dance-single", "DANCE-SINGLE", "pump-single", "é"] {
                assert_eq!(
                    is_edit_chart(&chart, query),
                    original::is_edit_chart(&chart, query)
                );
            }
        }
    }
    for count in [0, 1, 5, 16, 64, 128, 2048] {
        for kind in [
            "none", "wrong", "first", "last", "sparse", "mixed", "all", "unicode",
        ] {
            let song = edit_song(count, kind);
            for query in ["dance-single", "DANCE-SINGLE", "pump-single", "", "other"] {
                let mut expected: Vec<_> = song
                    .charts
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| original::is_edit_chart(c, query).then_some(i))
                    .collect();
                expected
                    .sort_by(|&a, &b| original::edit_chart_cmp(&song.charts[a], &song.charts[b]));
                assert_eq!(song.edit_chart_indices_sorted(query), expected);
                assert_eq!(
                    song.edit_charts_sorted(query)
                        .into_iter()
                        .map(std::ptr::from_ref)
                        .collect::<Vec<_>>(),
                    original::edit_charts_sorted(&song, query)
                        .into_iter()
                        .map(std::ptr::from_ref)
                        .collect::<Vec<_>>()
                );
                let pointers: Vec<_> = song
                    .edit_charts_sorted(query)
                    .into_iter()
                    .map(std::ptr::from_ref)
                    .collect();
                let expected_pointers: Vec<_> = expected
                    .iter()
                    .map(|&i| std::ptr::from_ref(&song.charts[i]))
                    .collect();
                assert_eq!(pointers, expected_pointers);
            }
        }
    }
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = alloc::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_chart_shortcuts() {
    for (label, kind) in [
        ("early", 0),
        ("equal", 1),
        ("late", 2),
        ("unicode", 3),
        ("short", 4),
    ] {
        let values: Vec<_> = (0..256)
            .map(|i| match kind {
                0 => (
                    format!("Alpha {} {i:03}", "x".repeat(96)),
                    format!("Zebra {} {i:03}", "x".repeat(96)),
                ),
                1 => (
                    "COMMON PREFIX EQUAL".to_owned(),
                    "common prefix equal".to_owned(),
                ),
                2 => (
                    format!("{}a", "common prefix ".repeat(8)),
                    format!("{}z", "common prefix ".repeat(8)),
                ),
                3 => (
                    ["é", "Prefixİ", "prefixΣ", "東京"][i % 4].to_owned(),
                    ["É", "prefixi\u{307}", "Prefixσ", "東京"][i % 4].to_owned(),
                ),
                _ => (
                    ["", "a", "B", "Edit"][i % 4].to_owned(),
                    ["a", "A", "b", "edit"][i % 4].to_owned(),
                ),
            })
            .collect();
        measure(&format!("compare/{label}"), 128, || {
            for (a, b) in black_box(&values) {
                black_box(lowercase_cmp(black_box(a), black_box(b)));
            }
        });
    }
    for (label, count, kind, query) in [
        ("standard", 10, "none", "dance-single"),
        ("wrong", 10, "wrong", "dance-single"),
        ("one", 6, "last", "dance-single"),
        ("mixed", 64, "mixed", "dance-single"),
        ("edits", 32, "all", "dance-single"),
        ("five", 5, "all", "dance-single"),
        ("eight", 8, "all", "dance-single"),
        ("unicode", 32, "unicode", "dance-single"),
        ("large", 2048, "all", "dance-single"),
        ("huge", 8192, "unicode", "dance-single"),
    ] {
        let song = edit_song(count, kind);
        measure(
            &format!("filter/{label}"),
            if count >= 2048 { 8 } else { 2048 },
            || {
                for chart in black_box(&song.charts) {
                    black_box(is_edit_chart(black_box(chart), black_box(query)));
                }
            },
        );
        measure(
            &format!("refs/{label}"),
            if count >= 2048 { 8 } else { 128 },
            || {
                black_box(black_box(&song).edit_charts_sorted(black_box(query)));
            },
        );
        measure(
            &format!("sort/{label}"),
            if count >= 2048 { 8 } else { 128 },
            || {
                black_box(black_box(&song).edit_chart_indices_sorted(black_box(query)));
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
        precise_note_last_second_seconds: None,
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
        music_file: String::new(),
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
