use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/metadata_timing/baseline.rs"
    ));
}

fn chart(rows: usize, visual_count: usize, bpm_count: usize) -> SerializableChartData {
    let mut chart = test_serializable_chart("dance-single", "Challenge", 0, None);
    chart.row_to_beat = (0..rows).map(|row| row as f32 / 48.0).collect();
    chart.parsed_notes = (0..rows)
        .step_by(12)
        .map(|row| CachedParsedNote {
            row_index: row as u32,
            column: (row / 12 % 4) as u8,
            note_type: match row / 12 % 6 {
                0 => CachedNoteType::Hold,
                1 => CachedNoteType::Roll,
                2 => CachedNoteType::Mine,
                3 => CachedNoteType::Fake,
                4 => CachedNoteType::Lift,
                _ => CachedNoteType::Tap,
            },
            tail_row_index: (row / 12 % 6 < 2)
                .then_some((row + 48).min(rows.saturating_sub(1)) as u32),
        })
        .collect();
    chart.notes = b"1000\n0100\n".to_vec();
    chart.measure_nps_vec = vec![8.0; rows / 192];
    chart.offset = 0.125;
    let mut segments = TimingSegments {
        bpms: (0..bpm_count)
            .map(|i| (i as f32 * 8.0, 100.0 + (i % 8) as f32 * 20.0))
            .collect(),
        beat0_offset_adjust: -0.037,
        ..TimingSegments::default()
    };
    for i in 0..bpm_count.saturating_sub(1) {
        let beat = i as f32 * 8.0;
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
    for i in 0..visual_count {
        let beat = i as f32 * 2.0;
        segments.speeds.push(SpeedSegment {
            beat,
            ratio: 0.5 + (i % 4) as f32,
            delay: 1.5,
            unit: if i % 2 == 0 {
                SpeedUnit::Beats
            } else {
                SpeedUnit::Seconds
            },
        });
        segments.scrolls.push(ScrollSegment {
            beat,
            ratio: (i % 5) as f32 - 2.0,
        });
        segments.time_signatures.push(TimeSignatureSegment {
            beat,
            numerator: 3,
            denominator: 8,
        });
        segments
            .tickcounts
            .push(TickcountSegment { beat, ticks: 16 });
        segments.combos.push(ComboSegment {
            beat,
            combo: 3,
            miss_combo: 5,
        });
    }
    chart.timing_segments = CachedTimingSegments::from(&segments);
    chart
}

#[test]
fn metadata_and_bounds_keep_identical_bytes_without_visual_timing() {
    for rows in [0, 1, 192, 8192] {
        for visual in [0, 1, 64] {
            for kind in 0..5 {
                let mut chart = chart(rows, visual, 16);
                if kind == 1 {
                    chart.timing_segments.bpms.reverse();
                    chart.timing_segments.stops.reverse();
                    chart.timing_segments.fakes.reverse();
                    chart.parsed_notes.reverse();
                } else if kind == 2 {
                    chart.timing_segments.bpms.clear();
                    chart.timing_segments.time_signatures.clear();
                    chart.timing_segments.tickcounts.clear();
                    chart.timing_segments.combos.clear();
                } else if kind == 3 {
                    chart.timing_segments.stops[0].1 = -0.125;
                    chart.timing_segments.delays[0].1 = 0.0;
                    chart.timing_segments.warps[0].1 = -1.0;
                    chart.row_to_beat.fill(f32::NAN);
                } else if kind == 4 {
                    chart.parsed_notes.push(CachedParsedNote {
                        row_index: u32::MAX,
                        column: 0,
                        note_type: CachedNoteType::Hold,
                        tail_row_index: Some(u32::MAX),
                    });
                    chart.timing_segments.speeds.push(CachedSpeedSegment {
                        beat: f32::NAN,
                        ratio: f32::INFINITY,
                        delay: -1.0,
                        unit: CachedSpeedUnit::Seconds,
                    });
                }
                let before = bincode::encode_to_vec(&chart, bincode::config::standard()).unwrap();
                for offset in [-2.125, 0.0, 0.037] {
                    let old = baseline::build_cached_chart_meta(&chart, offset);
                    let new = build_cached_chart_meta(&chart, offset);
                    assert_eq!(
                        bincode::encode_to_vec(&old, bincode::config::standard()).unwrap(),
                        bincode::encode_to_vec(&new, bincode::config::standard()).unwrap(),
                        "rows={rows} visual={visual} kind={kind} offset={offset}"
                    );
                    let mut old = cached_song(Path::new("Songs/Pack/Song/chart.ssc"));
                    old.last_second_hint = 3.125;
                    old.charts = vec![chart.clone()];
                    let mut edit = chart.clone();
                    edit.difficulty = "eDiT".into();
                    old.charts.push(edit);
                    let mut lights = chart.clone();
                    lights.chart_type = "lights-cabinet".into();
                    old.charts.push(lights);
                    let mut new = old.clone();
                    baseline::update_precise_song_bounds(&mut old, offset);
                    update_precise_song_bounds(&mut new, offset);
                    assert_eq!(
                        bincode::encode_to_vec(&old, bincode::config::standard()).unwrap(),
                        bincode::encode_to_vec(&new, bincode::config::standard()).unwrap()
                    );
                }
                assert_eq!(
                    before,
                    bincode::encode_to_vec(&chart, bincode::config::standard()).unwrap()
                );
            }
        }
    }
}

#[test]
fn song_bounds_timing_reduces_churn_with_and_without_visual_segments() {
    for visual in [0, 32, 256] {
        let chart = chart(8192, visual, 16);
        let mut old = cached_song(Path::new("Songs/Pack/Song/chart.ssc"));
        old.charts = vec![chart];
        let mut new = old.clone();
        assert_reduced_churn(
            || baseline::update_precise_song_bounds(black_box(&mut old), 0.037),
            || update_precise_song_bounds(black_box(&mut new), 0.037),
        );
    }
}

fn pair<T>(name: &str, iterations: usize, old: impl FnMut() -> T, new: impl FnMut() -> T) {
    if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
        measure_sampled(
            &format!("metadata-timing/{name}/current"),
            iterations,
            1,
            new,
        );
        measure_sampled(
            &format!("metadata-timing/{name}/original"),
            iterations,
            1,
            old,
        );
    } else {
        measure_sampled(
            &format!("metadata-timing/{name}/original"),
            iterations,
            1,
            old,
        );
        measure_sampled(
            &format!("metadata-timing/{name}/current"),
            iterations,
            1,
            new,
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn metadata_timing_benchmark() {
    let original =
        black_box(baseline::update_precise_song_bounds as fn(&mut SerializableSongData, f32));
    let current = black_box(update_precise_song_bounds as fn(&mut SerializableSongData, f32));
    for (label, rows, visual, bpms) in [
        ("empty", 0, 0, 1),
        ("regular", 8192, 0, 1),
        ("visual32", 8192, 32, 16),
        ("visual256", 8192, 256, 64),
        ("dense", 65_536, 256, 64),
    ] {
        let chart = chart(rows, visual, bpms);
        let mut old = cached_song(Path::new("Songs/Pack/Song/chart.ssc"));
        old.charts = vec![chart];
        let mut new = old.clone();
        pair(
            &format!("bounds-{label}"),
            if rows == 0 { 65_536 } else { 128 },
            || {
                original(black_box(&mut old), 0.037);
                (old.first_second, old.precise_last_second_seconds)
            },
            || {
                current(black_box(&mut new), 0.037);
                (new.first_second, new.precise_last_second_seconds)
            },
        );
    }
}
