use super::*;
use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/chart_metadata/baseline.rs"
    ));
}

fn segments(kind: usize, count: usize) -> TimingSegments {
    let mut segments = TimingSegments::default();
    segments.bpms = vec![(0.0, 150.0)];
    if kind > 0 {
        for i in 1..count {
            let beat = i as f32 * 8.0;
            segments.bpms.push((beat, 90.0 + (i % 9) as f32 * 20.0));
            segments.stops.push(StopSegment {
                beat: beat + 1.0,
                duration: 0.125,
            });
            segments.delays.push(DelaySegment {
                beat: beat + 2.0,
                duration: 0.0625,
            });
            segments.warps.push(WarpSegment {
                beat: beat + 3.0,
                length: 1.0,
            });
            segments.fakes.push(FakeSegment {
                beat: beat + 5.0,
                length: 0.5,
            });
        }
    }
    if kind == 2 {
        segments.bpms.extend([(0.001, 130.0), (0.001, 170.0)]);
    }
    segments
}

fn chart(rows: usize, chord: usize, kind: usize) -> SerializableChartData {
    let mut chart = test_serializable_chart("dance-single", "Challenge", 0, None);
    chart.row_to_beat = (0..rows).map(|row| row as f32 / 48.0).collect();
    chart.parsed_notes = (0..rows)
        .step_by(12)
        .flat_map(|row| {
            (0..chord).map(move |column| CachedParsedNote {
                row_index: row as u32,
                column: column as u8,
                note_type: match (row / 12 + column) % 13 {
                    0 => CachedNoteType::Hold,
                    1 => CachedNoteType::Roll,
                    2 => CachedNoteType::Mine,
                    3 => CachedNoteType::Fake,
                    4 => CachedNoteType::Lift,
                    _ => CachedNoteType::Tap,
                },
                tail_row_index: ((row / 12 + column) % 13 < 2)
                    .then_some((row + 24).min(rows.saturating_sub(1)) as u32),
            })
        })
        .collect();
    chart.timing_segments = CachedTimingSegments::from(&segments(kind, rows / 384));
    chart.measure_nps_vec = vec![8.0; rows / 192];
    chart.notes = b"1000\n0100\n0010\n0001\n".to_vec();
    chart.offset = 0.125;
    chart
}

fn song(charts: Vec<SerializableChartData>) -> SerializableSongData {
    SerializableSongData {
        simfile_path: "Songs/Pack/Song/chart.ssc".into(),
        title: "Song".into(),
        subtitle: String::new(),
        translit_title: "Song".into(),
        translit_subtitle: String::new(),
        artist: "Artist".into(),
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
        display_bpm: "150".into(),
        offset: 0.125,
        sample_start: None,
        sample_length: None,
        min_bpm: 90.0,
        max_bpm: 250.0,
        normalized_bpms: String::new(),
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 2,
        precise_last_second_seconds: 2.0,
        charts,
    }
}

fn shuffle(notes: &mut [CachedParsedNote], seed: &mut u64) {
    for i in (1..notes.len()).rev() {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        notes.swap(i, (*seed >> 32) as usize % (i + 1));
    }
}

#[test]
fn scoring_totals_match_old_ordered_unordered_and_invalid_rows() {
    let mut seed = 491;
    for rows in [0, 1, 12, 192, 4096] {
        for chord in [1, 4, 10] {
            for kind in 0..3 {
                let mut chart = chart(rows, chord, kind);
                let timing = TimingData::from_segments(
                    -0.125,
                    0.037,
                    &chart.timing_segments.clone().into(),
                    &chart.row_to_beat,
                );
                for variant in 0..4 {
                    if variant == 1 {
                        chart.parsed_notes.reverse();
                    }
                    if variant == 2 {
                        shuffle(&mut chart.parsed_notes, &mut seed);
                    }
                    if variant == 3 {
                        chart.parsed_notes.push(CachedParsedNote {
                            row_index: u32::MAX,
                            column: 0,
                            note_type: CachedNoteType::Hold,
                            tail_row_index: None,
                        });
                        chart.parsed_notes.extend(chart.parsed_notes.clone());
                    }
                    assert_eq!(
                        build_chart_totals(&chart.parsed_notes, &timing),
                        baseline::build_chart_totals(&chart.parsed_notes, &timing),
                        "rows={rows} chord={chord} kind={kind} variant={variant}"
                    );
                }
            }
        }
    }
}

#[test]
fn ordered_scoring_totals_have_no_heap_churn() {
    for rows in [0, 1, 192, 65_536] {
        let chart = chart(rows, 4, 1);
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &chart.timing_segments.clone().into(),
            &chart.row_to_beat,
        );
        assert_no_churn(|| {
            black_box(build_chart_totals(
                black_box(&chart.parsed_notes),
                black_box(&timing),
            ));
        });
    }
}

#[test]
fn transient_bounds_timing_does_not_copy_the_row_table() {
    for rows in [4096, 1_048_576] {
        let mut song = song(vec![chart(rows, 1, 0)]);
        crate::perf::assert_churn_budget(32, 1024, || {
            update_precise_song_bounds(black_box(&mut song), 0.037);
        });
        assert!(song.precise_last_second_seconds > 2.0);
    }
}

#[test]
fn measure_times_match_old_bits_across_cursor_boundaries_and_fallbacks() {
    let mut variants = vec![segments(0, 0), segments(1, 128), segments(2, 16)];
    for bpms in [
        vec![],
        vec![(-8.0, 90.0), (0.0, 120.0), (32.0, 180.0)],
        vec![(0.0, 0.0), (4.0, 120.0)],
        vec![(0.0, -120.0), (4.0, 120.0)],
        vec![(0.0, f32::INFINITY)],
        vec![(0.0, f32::NAN)],
        vec![(0.0, 120.0), (1.0 / 48.0, 150.0)],
        vec![(0.0, 120.0), (0.001, 150.0)],
    ] {
        let mut s = segments(1, 16);
        s.bpms = bpms;
        variants.push(s);
    }
    let mut seed = 831u64;
    for _ in 0..48 {
        let mut s = segments(1, 32);
        for i in 0..s.stops.len() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            s.stops[i].duration = ((seed >> 32) % 17) as f32 * 0.03 - 0.12;
            s.delays[i].duration = ((seed >> 36) % 17) as f32 * 0.02 - 0.06;
            s.warps[i].length = ((seed >> 40) % 31) as f32 / 4.0;
        }
        variants.push(s);
    }
    for (variant, s) in variants.iter().enumerate() {
        for offset in [-2.125, 0.0, 0.037] {
            let timing = TimingData::from_segments(offset, 0.015, s, &[]);
            for count in [0, 1, 15, 16, 17, 128, 1024] {
                let old = baseline::build_measure_seconds(&timing, count);
                let new = build_measure_seconds(&timing, count);
                assert_eq!(
                    old.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    new.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "variant={variant} offset={offset} count={count}"
                );
            }
        }
    }
}

#[test]
fn metadata_matches_old_serialized_bytes_and_owned_chart_fields() {
    for rows in [0, 1, 192, 4096, 65_536] {
        for kind in 0..3 {
            let chart = chart(rows, 4, kind);
            for offset in [-0.025, 0.0, 0.031] {
                let old = baseline::build_cached_chart_meta(&chart, offset);
                let new = build_cached_chart_meta(&chart, offset);
                assert_eq!(
                    bincode::encode_to_vec(&old, bincode::config::standard()).unwrap(),
                    bincode::encode_to_vec(&new, bincode::config::standard()).unwrap(),
                    "rows={rows} kind={kind}"
                );
                assert_eq!(
                    format!("{:?}", baseline::build_chart_meta(chart.clone(), offset)),
                    format!("{:?}", build_chart_meta(chart.clone(), offset))
                );
            }
        }
    }
}

#[test]
fn song_bounds_match_old_with_edits_lights_tails_and_missing_rows() {
    for kind in 0..3 {
        let mut playable = chart(4096, 4, kind);
        let mut edit = chart(8192, 1, kind);
        edit.difficulty = "eDiT".into();
        let mut lights = chart(16_384, 4, kind);
        lights.chart_type = "LiGhTs-CaBiNeT".into();
        let mut invalid = chart(192, 1, kind);
        invalid.parsed_notes[0].tail_row_index = Some(u32::MAX);
        for input in [
            vec![],
            vec![edit.clone()],
            vec![lights.clone()],
            vec![playable.clone(), edit, lights, invalid],
            {
                playable.row_to_beat.fill(f32::NAN);
                vec![playable]
            },
        ] {
            for offset in [-2.125, 0.0, 0.037] {
                let mut old = song(input.clone());
                let mut new = old.clone();
                baseline::update_precise_song_bounds(&mut old, offset);
                update_precise_song_bounds(&mut new, offset);
                assert_eq!(
                    bincode::encode_to_vec(&old, bincode::config::standard()).unwrap(),
                    bincode::encode_to_vec(&new, bincode::config::standard()).unwrap()
                );
            }
        }
    }
}

fn pair<T>(
    name: &str,
    iterations: usize,
    units: usize,
    old: impl FnMut() -> T,
    new: impl FnMut() -> T,
) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        measure_sampled(&format!("{name}_new"), iterations, units, new);
        measure_sampled(&format!("{name}_old"), iterations, units, old);
    } else {
        measure_sampled(&format!("{name}_old"), iterations, units, old);
        measure_sampled(&format!("{name}_new"), iterations, units, new);
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn chart_metadata_bench() {
    for (label, rows, chord, kind, unordered) in [
        ("empty", 0, 1, 0, false),
        ("small", 192, 1, 0, false),
        ("single", 16_384, 1, 0, false),
        ("chords", 65_536, 4, 0, false),
        ("dense", 65_536, 4, 1, false),
        ("unordered", 65_536, 4, 1, true),
    ] {
        let mut chart = chart(rows, chord, kind);
        if unordered {
            shuffle(&mut chart.parsed_notes, &mut 19);
        }
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &chart.timing_segments.clone().into(),
            &chart.row_to_beat,
        );
        let iterations = (500_000 / chart.parsed_notes.len().max(1)).clamp(16, 50_000);
        pair(
            &format!("totals_{label}"),
            iterations,
            chart.parsed_notes.len().max(1),
            || baseline::build_chart_totals(black_box(&chart.parsed_notes), black_box(&timing)),
            || build_chart_totals(black_box(&chart.parsed_notes), black_box(&timing)),
        );
    }
    for (label, count, kind) in [
        ("empty", 0, 0),
        ("small", 8, 0),
        ("constant16", 16, 0),
        ("constant128", 128, 0),
        ("constant1024", 1024, 0),
        ("dense64", 64, 1),
        ("dense512", 512, 1),
        ("dense2048", 2048, 1),
        ("fallback512", 512, 2),
    ] {
        let timing = TimingData::from_segments(-0.125, 0.037, &segments(kind, count / 2), &[]);
        pair(
            &format!("measures_{label}"),
            if kind == 0 { 2048 } else { 16 },
            count.max(1),
            || baseline::build_measure_seconds(black_box(&timing), black_box(count)),
            || build_measure_seconds(black_box(&timing), black_box(count)),
        );
    }
    for (label, rows, kind) in [
        ("small", 192, 0),
        ("long", 65_536, 0),
        ("million", 1_048_576, 0),
        ("dense", 65_536, 1),
    ] {
        let mut old = song(vec![chart(rows, 4, kind)]);
        let mut new = old.clone();
        pair(
            &format!("bounds_{label}"),
            64,
            1,
            || {
                baseline::update_precise_song_bounds(black_box(&mut old), 0.037);
                (old.first_second, old.precise_last_second_seconds)
            },
            || {
                update_precise_song_bounds(black_box(&mut new), 0.037);
                (new.first_second, new.precise_last_second_seconds)
            },
        );
    }
    for (label, rows, kind) in [
        ("small", 192, 0),
        ("long", 65_536, 0),
        ("dense", 65_536, 1),
        ("fallback", 65_536, 2),
    ] {
        let chart = chart(rows, 4, kind);
        pair(
            &format!("metadata_{label}"),
            32,
            1,
            || baseline::build_cached_chart_meta(black_box(&chart), 0.037),
            || build_cached_chart_meta(black_box(&chart), 0.037),
        );
    }
}
