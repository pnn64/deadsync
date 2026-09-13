#[path = "perf/edit_lookup_baseline.rs"]
mod baseline;
#[path = "perf/fixtures.rs"]
mod fixtures;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

use baseline::BaselineSong;
use deadsync_chart::{STANDARD_DIFFICULTY_COUNT, SongData};
use std::hint::black_box;

fn fixture(edits: usize, seed: usize) -> SongData {
    let mut song = fixtures::song_data();
    for difficulty in [
        "Beginner",
        "Easy",
        "Medium",
        "Hard",
        "Challenge",
        "Challenge",
        "Routine",
    ] {
        let mut chart = fixtures::chart_data();
        chart.difficulty = difficulty.into();
        song.charts.push(chart);
    }
    let descriptions = [
        "Alpha",
        "alpha",
        "BETA",
        "beta",
        "\u{130}",
        "i\u{307}",
        "\u{65e5}\u{672c}\u{8a9e}",
        "",
    ];
    for index in 0..edits {
        let key = (index * 7919 + seed * 103) % 101;
        let mut chart = fixtures::chart_data();
        chart.chart_type = if index % 3 == 0 {
            "DANCE-single"
        } else {
            "dance-single"
        }
        .into();
        chart.difficulty = if index % 2 == 0 { "Edit" } else { "eDiT" }.into();
        chart.meter = (key % 5 + 1) as u32;
        chart.stats.total_steps = (key % 3) as u32;
        chart.description = descriptions[key % descriptions.len()].into();
        chart.short_hash = format!("{seed}-{index}");
        if index % 7 == 0 {
            let mut other_type = chart.clone();
            other_type.chart_type = "dance-double".into();
            song.charts.push(other_type);
        }
        song.charts.push(chart);
    }
    song
}

fn assert_same(song: &SongData, chart_type: &str, steps: usize) {
    let old = song.baseline_chart_for_steps_index(chart_type, steps);
    let new = song.chart_for_steps_index(chart_type, steps);
    assert_eq!(
        old.map(std::ptr::from_ref),
        new.map(std::ptr::from_ref),
        "type {chart_type}, steps {steps}, {} charts",
        song.charts.len()
    );
}

#[test]
fn selection_matches_stable_sort_for_every_rank_and_chart_type() {
    for edits in [0, 1, 2, 8, 15, 16, 17, 32, 127] {
        for seed in 0..10 {
            let song = fixture(edits, seed);
            for chart_type in [
                "dance-single",
                "DANCE-SINGLE",
                "dance-double",
                "pump-single",
                "",
            ] {
                for steps in 0..edits + STANDARD_DIFFICULTY_COUNT + 2 {
                    assert_same(&song, chart_type, steps);
                }
                assert_same(&song, chart_type, usize::MAX);
            }
        }
    }
}

#[test]
fn equal_unicode_sort_keys_keep_source_order_and_never_mutate_the_song() {
    for edits in [16, 17, 128] {
        let mut song = fixture(edits, 0);
        song.charts.retain(|chart| {
            chart.difficulty.eq_ignore_ascii_case("edit")
                && chart.chart_type.eq_ignore_ascii_case("dance-single")
        });
        for (index, chart) in song.charts.iter_mut().enumerate() {
            chart.meter = 12;
            chart.stats.total_steps = 400;
            chart.description = if index % 2 == 0 {
                "\u{130}"
            } else {
                "i\u{307}"
            }
            .into();
        }
        for rank in 0..edits {
            let chosen = song
                .chart_for_steps_index("dance-single", STANDARD_DIFFICULTY_COUNT + rank)
                .unwrap();
            assert!(std::ptr::eq(chosen, &song.charts[rank]));
            assert_eq!(chosen.short_hash, format!("0-{rank}"));
        }
    }
}

#[test]
fn up_to_sixteen_matching_edits_have_zero_lookup_churn() {
    for edits in [0, 1, 2, 8, 16] {
        let song = fixture(edits, 3);
        perf::assert_no_churn(|| {
            for steps in 0..edits + STANDARD_DIFFICULTY_COUNT + 2 {
                black_box(song.chart_for_steps_index(black_box("dance-single"), black_box(steps)));
            }
        });
    }
}

#[test]
#[ignore = "manual release benchmark; use --ignored --nocapture --test-threads=1"]
fn benchmark_edit_lookup() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, count, steps) in [
        ("edit_standard_control", 16, 4),
        ("edit_first_control", 16, STANDARD_DIFFICULTY_COUNT),
        ("edit_second_2", 2, STANDARD_DIFFICULTY_COUNT + 1),
        ("edit_middle_8", 8, STANDARD_DIFFICULTY_COUNT + 4),
        ("edit_middle_16", 16, STANDARD_DIFFICULTY_COUNT + 8),
        ("edit_middle_17", 17, STANDARD_DIFFICULTY_COUNT + 8),
        ("edit_middle_128", 128, STANDARD_DIFFICULTY_COUNT + 64),
        ("edit_missing", 16, usize::MAX),
    ] {
        let song = fixture(count, 4);
        assert_same(&song, "dance-single", steps);
        let iterations = if count > 32 { 10_000 } else { 100_000 };
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            if old {
                perf::measure_sampled(&format!("{name}/old"), iterations, 1, || {
                    black_box(&song)
                        .baseline_chart_for_steps_index(black_box("dance-single"), black_box(steps))
                });
            } else {
                perf::measure_sampled(&format!("{name}/new"), iterations, 1, || {
                    black_box(&song)
                        .chart_for_steps_index(black_box("dance-single"), black_box(steps))
                });
            }
        }
    }
}
