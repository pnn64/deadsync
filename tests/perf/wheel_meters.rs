use super::super::*;
use super::*;

mod original {
    use super::*;
    include!("original_wheel_meters.rs");
}

fn song(charts: Vec<ChartData>) -> SongData {
    let mut song = Arc::try_unwrap(song_with_art(None, None)).unwrap();
    song.charts = charts;
    song
}

fn chart(kind: &str, difficulty: &str, meter: u32, steps: u32, description: &str) -> ChartData {
    let mut chart = chart_with_difficulty(kind, difficulty, description, false);
    chart.meter = meter;
    chart.stats.total_steps = steps;
    chart.description = description.to_owned();
    chart
}

fn assert_meters(song: &SongData, kind: &str) {
    let slots = chart_indices(song, kind);
    let expected = original::meter_indices(song, kind, &slots);
    let actual = meter_indices(song, kind, &slots);
    assert_eq!(actual, expected);
    for meter in 0..=65 {
        assert_eq!(
            chart_for_meter(song, meter, &actual).map(std::ptr::from_ref),
            chart_for_meter(song, meter, &expected).map(std::ptr::from_ref)
        );
    }
}

#[test]
fn meter_winners_preserve_standard_priority_and_stable_edit_ties() {
    let song = song(vec![
        chart("dance-single", "Easy", 9, 999, "standard"),
        chart("dance-single", "Hard", 9, 999, "standard later"),
        chart("dance-single", "Edit", 9, 10, "z"),
        chart("DANCE-SINGLE", "eDiT", 9, 11, "Alpha"),
        chart("dance-single", "Edit", 9, 11, "alpha"),
        chart("dance-double", "Edit", 9, 999, "excluded"),
        chart("dance-single", "Unknown", 9, 999, "excluded"),
        chart("dance-single", "Edit", 10, 12, "İ"),
        chart("dance-single", "Edit", 10, 12, "i\u{307}"),
        chart("dance-single", "Edit", 10, 11, "🦀"),
        chart("dance-single", "Beginner", 2, 999, "first"),
        chart("dance-single", "Challenge", 2, 999, "later difficulty"),
    ]);
    let slots = chart_indices(&song, "dance-single");
    assert_eq!(
        meter_indices(&song, "dance-single", &slots).as_slice(),
        &[(2, 11), (9, 4), (10, 8)]
    );
    assert_meters(&song, "dance-single");
    assert_meters(&song, "DANCE-DOUBLE");
}

#[test]
fn meter_lookup_matches_original_for_mixed_charts_and_orders() {
    let descriptions = ["", "A", "a", "ä", "Ä", "İ", "i\u{307}", "Z🦀", "Straße"];
    let difficulties = [
        "Beginner",
        "Easy",
        "Medium",
        "Hard",
        "Challenge",
        "Edit",
        "eDiT",
        "Other",
    ];
    let mut seed = 0x9d24_7a31u64;
    for count in [0, 1, 6, 7, 64, 257] {
        let mut charts = Vec::new();
        for index in 0..count {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            charts.push(chart(
                if seed & 3 == 0 {
                    "dance-double"
                } else {
                    "DANCE-SINGLE"
                },
                difficulties[(seed >> 8) as usize % difficulties.len()],
                (seed >> 16) as u32 % 65,
                (seed >> 24) as u32 % 20,
                descriptions[index % descriptions.len()],
            ));
        }
        for _ in 0..4 {
            let mut song = song(charts);
            for kind in ["dance-single", "DANCE-DOUBLE", "pump-single", ""] {
                assert_meters(&song, kind);
            }
            song.charts.reverse();
            charts = song.charts;
            if !charts.is_empty() {
                charts.rotate_left(count / 3);
            }
        }
    }
}

#[test]
fn standard_meter_lookup_avoids_the_edit_buffer_allocation() {
    let song = song(vec![
        chart("dance-single", "Beginner", 1, 0, ""),
        chart("dance-single", "Easy", 3, 0, ""),
        chart("dance-single", "Medium", 5, 0, ""),
        chart("dance-single", "Hard", 9, 0, ""),
    ]);
    let slots = chart_indices(&song, "dance-single");
    let (expected, before) =
        crate::scan_alloc::measure(|| original::meter_indices(&song, "dance-single", &slots));
    let (actual, after) =
        crate::scan_alloc::measure(|| meter_indices(&song, "dance-single", &slots));
    assert_eq!(actual, expected);
    assert_eq!(before.allocs, 1);
    assert_eq!(
        (after.allocs, after.reallocs, after.allocated_bytes),
        (0, 0, 0)
    );
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --nocapture --test-threads=1"]
fn benchmark_direct_scans() {
    let standard = song(vec![
        chart("dance-single", "Beginner", 1, 0, ""),
        chart("dance-single", "Easy", 3, 0, ""),
        chart("dance-single", "Medium", 5, 0, ""),
        chart("dance-single", "Hard", 9, 0, ""),
    ]);
    let edits = |count, unique| {
        song(
            (0..count)
                .map(|i| {
                    chart(
                        "dance-single",
                        "Edit",
                        if unique { i as u32 } else { i as u32 % 4 },
                        i as u32 % 13,
                        ["Alpha", "İ", "Ä", "z"][i % 4],
                    )
                })
                .collect(),
        )
    };
    for (label, song) in [
        ("meter/standard_4", standard),
        ("meter/edits_4", edits(4, false)),
        ("meter/duplicates_64", edits(64, false)),
        ("meter/unique_64", edits(64, true)),
    ] {
        let slots = chart_indices(&song, "dance-single");
        assert_meters(&song, "dance-single");
        crate::scan_perf::compare(
            label,
            || (&song, &slots),
            |(song, slots)| original::meter_indices(song, "dance-single", slots),
            |(song, slots)| meter_indices(song, "dance-single", slots),
        );
    }
}
