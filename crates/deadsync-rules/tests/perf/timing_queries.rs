use crate::perf;
use std::hint::black_box;

include!("timing_queries_baseline.rs");

fn fixture(size: usize) -> TimingSegments {
    TimingSegments {
        bpms: vec![(0.0, 120.0)],
        stops: (0..size)
            .map(|i| StopSegment {
                beat: i as f32 * 2.0 + 1.0,
                duration: if i % 5 == 0 { 0.0 } else { 0.125 },
            })
            .collect(),
        delays: (0..size)
            .map(|i| DelaySegment {
                beat: i as f32 * 2.0 + 1.0,
                duration: if i % 5 == 0 { -0.125 } else { 0.0 },
            })
            .collect(),
        scrolls: (0..size)
            .map(|i| ScrollSegment {
                beat: i as f32,
                ratio: [1.0, 0.0, -0.5, 2.0][i % 4],
            })
            .collect(),
        ..TimingSegments::default()
    }
}

fn float_eq(a: f32, b: f32) {
    assert!(
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
        "{a:?} != {b:?}"
    );
}

#[test]
fn pause_row_search_matches_parent_for_boundaries_duplicates_and_invalid_durations() {
    let values = [
        0.0,
        -0.0,
        0.25,
        -0.25,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    for size in [0, 1, 8, 9, 32, 1024] {
        let mut segments = fixture(size);
        segments.stops = (0..size)
            .map(|i| StopSegment {
                beat: (i / 7) as f32 / 48.0,
                duration: values[i % values.len()],
            })
            .collect();
        segments.delays.reverse();
        let timing = TimingData::from_segments(0.0, 0.0, &segments, &[]);
        for row in -3..size as i32 + 3 {
            assert_eq!(
                timing.old_has_stop_or_delay_at_row(row),
                timing.has_stop_or_delay_at_row(row),
                "size {size}, row {row}"
            );
        }
    }
    for with_nan in [false, true] {
        let mut beats = vec![
            f32::NEG_INFINITY,
            f32::MIN,
            -1.0,
            -0.0,
            0.0,
            f32::MIN_POSITIVE,
            0.0104,
            0.0105,
            1.0,
            f32::MAX,
            f32::INFINITY,
        ];
        if with_nan {
            beats.push(f32::NAN);
        }
        let segments = TimingSegments {
            stops: beats
                .into_iter()
                .map(|beat| StopSegment {
                    beat,
                    duration: -0.125,
                })
                .collect(),
            ..TimingSegments::default()
        };
        let timing = TimingData::from_segments(0.0, 0.0, &segments, &[]);
        assert_eq!(timing.pause_rows_sorted[0], !with_nan);
        for row in [i32::MIN, -49, -48, -1, 0, 1, 47, 48, 49, i32::MAX] {
            assert_eq!(
                timing.old_has_stop_or_delay_at_row(row),
                timing.has_stop_or_delay_at_row(row)
            );
        }
    }
}

#[test]
fn unordered_pause_rows_keep_the_parent_scan_and_warp_exceptions() {
    let mut segments = fixture(32);
    segments.stops[0].beat = f32::NAN;
    segments.stops[1].beat = -20.0;
    let mut timing = TimingData::from_segments(0.0, 0.0, &segments, &[]);
    assert_eq!(
        timing.pause_rows_sorted[0],
        timing
            .stops
            .windows(2)
            .all(|p| beat_to_note_row(p[0].beat) <= beat_to_note_row(p[1].beat))
    );
    for row in [-960, -1, 0, 1, 48, 144, i32::MIN, i32::MAX] {
        assert_eq!(
            timing.old_has_stop_or_delay_at_row(row),
            timing.has_stop_or_delay_at_row(row)
        );
    }
    // Exercise the fallback directly with a deliberately unordered private table.
    timing.stops = [
        StopSegment {
            beat: 8.0,
            duration: 0.25,
        },
        StopSegment {
            beat: 2.0,
            duration: 0.25,
        },
    ]
    .repeat(8)
    .into();
    timing.pause_rows_sorted[0] = false;
    timing.warps = vec![WarpSegment {
        beat: 0.0,
        length: 16.0,
    }]
    .into();
    for beat in [0.0, 1.0, 2.0, 2.001, 3.0, 8.0, 15.0, 16.0] {
        let row = beat_to_note_row(beat);
        let inside = note_row_to_beat(row) < 16.0;
        assert_eq!(
            timing.is_warp_at_beat(beat),
            inside && !timing.old_has_stop_or_delay_at_row(row)
        );
    }
}

#[test]
fn seek_cache_matches_parent_values_and_state_for_rewinds_and_nonfinite_beats() {
    for size in [0, 1, 8, 64, 4096] {
        let timing = TimingData::from_segments(0.0, 0.0, &fixture(size), &[]);
        let mut old = DisplayedBeatCache::new();
        let mut new = DisplayedBeatCache::new();
        for beat in [
            size as f32,
            size as f32 - 0.5,
            -1.0,
            0.0,
            0.125,
            1.0,
            12.0,
            2.0,
            size as f32 + 1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0,
        ] {
            float_eq(
                timing.old_get_displayed_beat_cached(beat, &mut old),
                timing.get_displayed_beat_cached(beat, &mut new),
            );
            assert_eq!(
                (old.next_prefix, old.initialized, old.last_beat.to_bits()),
                (new.next_prefix, new.initialized, new.last_beat.to_bits())
            );
        }
        old.next_prefix = size + 3;
        new.next_prefix = size + 3;
        float_eq(
            timing.old_get_displayed_beat_cached(3.0, &mut old),
            timing.get_displayed_beat_cached(3.0, &mut new),
        );
    }
    for beats in [
        vec![0.0, f32::NAN, 2.0],
        vec![3.0, f32::NAN, 1.0],
        vec![f32::NAN],
        vec![0.0, 0.0, 1.0],
    ] {
        let segments = TimingSegments {
            scrolls: beats
                .into_iter()
                .map(|beat| ScrollSegment { beat, ratio: 0.5 })
                .collect(),
            ..Default::default()
        };
        let timing = TimingData::from_segments(0.0, 0.0, &segments, &[]);
        let mut old = DisplayedBeatCache::new();
        let mut new = DisplayedBeatCache::new();
        for beat in [10.0, 4.0, 1.0, 0.0, -1.0, 2.0] {
            float_eq(
                timing.old_get_displayed_beat_cached(beat, &mut old),
                timing.get_displayed_beat_cached(beat, &mut new),
            );
            assert_eq!(old.next_prefix, new.next_prefix);
        }
    }
}

#[test]
fn timing_queries_and_seeks_have_no_allocator_churn() {
    let timing = TimingData::from_segments(0.0, 0.0, &fixture(4096), &[]);
    let mut cache = DisplayedBeatCache::new();
    perf::assert_no_churn(|| {
        for i in 0..128 {
            black_box(timing.has_stop_or_delay_at_row(i * 79));
            black_box(timing.get_displayed_beat_cached((4096 - i * 3) as f32, &mut cache));
        }
    });
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_timing_queries() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (size, mode) in [
        (4, "miss"),
        (64, "miss"),
        (4096, "miss"),
        (4096, "late_hit"),
        (4096, "first_hit"),
    ] {
        let mut segments = fixture(size);
        if mode == "first_hit" {
            segments.stops[0].duration = 0.125;
        }
        let timing = TimingData::from_segments(0.0, 0.0, &segments, &[]);
        let row = match mode {
            "first_hit" => 48,
            "late_hit" => (size as i32 * 2 - 1) * 48,
            _ => (size as i32 * 2) * 48,
        };
        assert_eq!(
            timing.old_has_stop_or_delay_at_row(row),
            timing.has_stop_or_delay_at_row(row)
        );
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("pause_{size}_{mode}_{}", if old { "old" } else { "new" });
            perf::measure_sampled(&label, 128, 128, || {
                for _ in 0..128 {
                    black_box(if old {
                        timing.old_has_stop_or_delay_at_row(black_box(row))
                    } else {
                        timing.has_stop_or_delay_at_row(black_box(row))
                    });
                }
            });
        }
    }
    for (size, mode) in [
        (4, "rewind"),
        (64, "rewind"),
        (4096, "rewind"),
        (65_536, "initial"),
        (4096, "steady"),
        (4096, "forward"),
    ] {
        let timing = TimingData::from_segments(0.0, 0.0, &fixture(size), &[]);
        let queries: Vec<_> = (0..128)
            .map(|i| match mode {
                "rewind" => (size - 1) as f32 - i as f32 * 0.01,
                "steady" => 2.25,
                "forward" => i as f32 * 0.25,
                _ => (size - 1) as f32,
            })
            .collect();
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("display_{size}_{mode}_{}", if old { "old" } else { "new" });
            perf::measure_sampled(&label, 64, queries.len(), || {
                let mut cache = DisplayedBeatCache::new();
                for &beat in &queries {
                    if mode == "initial" {
                        cache.reset();
                    }
                    black_box(if old {
                        timing.old_get_displayed_beat_cached(black_box(beat), &mut cache)
                    } else {
                        timing.get_displayed_beat_cached(black_box(beat), &mut cache)
                    });
                }
            });
        }
    }
    for size in [4, 64, 4096] {
        let segments = fixture(size);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("timing_build_{size}_{}", if old { "old" } else { "new" });
            perf::measure_sampled(&label, if size < 4096 { 128 } else { 8 }, size, || {
                black_box(if old {
                    TimingData::old_from_segments(0.0, 0.0, &segments, &[])
                } else {
                    TimingData::from_segments(0.0, 0.0, &segments, &[])
                });
            });
        }
    }
}
