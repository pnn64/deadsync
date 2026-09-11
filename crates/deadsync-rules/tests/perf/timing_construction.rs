// Reference routines frozen from c4aeed4fe / 0.5.1135.
use super::*;
use std::hint::black_box;

fn legacy_sorted_timing_table<T: Clone>(values: &[T], beat: impl Fn(&T) -> f32) -> Arc<[T]> {
    let mut output: Arc<[T]> = Arc::from(values);
    Arc::make_mut(&mut output)
        .sort_by(|a, b| beat(a).partial_cmp(&beat(b)).unwrap_or(Ordering::Less));
    output
}

impl TimingData {
    fn legacy_from_segments(
        song_offset_sec: f32,
        global_offset_sec: f32,
        segments: &TimingSegments,
        row_to_beat: &[f32],
    ) -> Self {
        let mut parsed_bpms = segments.bpms.clone();
        if parsed_bpms.is_empty() {
            parsed_bpms.push((0.0, 60.0));
        }
        parsed_bpms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Less));

        let stops = legacy_sorted_timing_table(&segments.stops, |segment| segment.beat);
        let delays = legacy_sorted_timing_table(&segments.delays, |segment| segment.beat);
        let warps = legacy_sorted_timing_table(&segments.warps, |segment| segment.beat);
        let speeds = legacy_sorted_timing_table(&segments.speeds, |segment| segment.beat);
        let scrolls = legacy_sorted_timing_table(&segments.scrolls, |segment| segment.beat);
        let fakes = legacy_sorted_timing_table(&segments.fakes, |segment| segment.beat);

        let song_offset_sec = song_offset_sec + segments.beat0_offset_adjust;
        let song_offset_ns = timing_ns_from_seconds(song_offset_sec);
        let global_offset_ns = timing_ns_from_seconds(global_offset_sec);

        let mut beat_to_time = Vec::with_capacity(parsed_bpms.len());
        let mut current_time = 0.0;
        let mut last_beat = 0.0;
        let mut last_bpm = parsed_bpms[0].1;
        let mut max_bpm = 0.0;

        for &(beat, bpm) in &parsed_bpms {
            if beat > last_beat && last_bpm > 0.0 {
                current_time = (beat - last_beat).mul_add(60.0 / last_bpm, current_time);
            }
            beat_to_time.push(BeatTimePoint {
                beat,
                time_ns: timing_ns_add_seconds(song_offset_ns, current_time),
                bpm,
            });
            if bpm.is_finite() && bpm > max_bpm {
                max_bpm = bpm;
            }
            last_beat = beat;
            last_bpm = bpm;
        }

        let mut timing_with_stops = Self {
            row_to_beat: Arc::new(vec![]),
            beat_to_time: Arc::new(beat_to_time),
            stops,
            delays,
            warps,
            speeds,
            scrolls,
            fakes,
            speed_runtime: Arc::default(),
            scroll_prefix: Arc::default(),
            global_offset_sec,
            global_offset_ns,
            max_bpm,
        };

        let mut beat_time_cache = BeatTimeCache::new(&timing_with_stops);
        let re_beat_to_time: Vec<_> = timing_with_stops
            .beat_to_time
            .iter()
            .map(|point| {
                let mut new_point = *point;
                new_point.time_ns = timing_with_stops
                    .get_time_for_beat_internal_ns_cached(point.beat, &mut beat_time_cache);
                new_point
            })
            .collect();
        timing_with_stops.beat_to_time = Arc::new(re_beat_to_time);

        timing_with_stops.rebuild_speed_runtime();

        if !timing_with_stops.scrolls.is_empty() {
            let mut cum_displayed = 0.0_f32;
            let mut last_real_beat = 0.0_f32;
            let mut last_ratio = 1.0_f32;
            timing_with_stops.scroll_prefix = exact_arc(timing_with_stops.scrolls.len(), |index| {
                let seg = timing_with_stops.scrolls[index];
                cum_displayed = (seg.beat - last_real_beat).mul_add(last_ratio, cum_displayed);
                let prefix = ScrollPrefix {
                    beat: seg.beat,
                    cum_displayed,
                    ratio: seg.ratio,
                };
                last_real_beat = seg.beat;
                last_ratio = seg.ratio;
                prefix
            });
        }

        let row_to_beat = row_to_beat.to_vec();
        debug!("TimingData processed {} note rows.", row_to_beat.len());
        timing_with_stops.row_to_beat = Arc::new(row_to_beat);

        timing_with_stops
    }
}

fn construction_fixture(count: usize, modifiers: bool) -> TimingSegments {
    let mut segments = TimingSegments {
        bpms: (0..count)
            .map(|i| (i as f32 * 4.0, [120.0, 175.5, 90.25][i % 3]))
            .collect(),
        beat0_offset_adjust: 0.023,
        ..TimingSegments::default()
    };
    if modifiers {
        for i in 0..count {
            let beat = i as f32 * 4.0;
            segments.stops.push(StopSegment {
                beat: beat + 1.0,
                duration: 0.125,
            });
            segments.delays.push(DelaySegment {
                beat: beat + 2.0,
                duration: 0.031,
            });
            segments.warps.push(WarpSegment {
                beat: beat + 2.5,
                length: 0.25,
            });
            segments.speeds.push(SpeedSegment {
                beat,
                ratio: 1.25,
                delay: 0.5,
                unit: SpeedUnit::Beats,
            });
            segments.scrolls.push(ScrollSegment {
                beat,
                ratio: if i % 2 == 0 { 0.75 } else { 1.25 },
            });
            segments.fakes.push(FakeSegment {
                beat: beat + 3.0,
                length: 0.125,
            });
        }
    }
    segments
}

fn assert_same_timing(old: &TimingData, new: &TimingData) {
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
    for (a, b) in old.beat_to_time.iter().zip(new.beat_to_time.iter()) {
        assert_eq!(
            (a.beat.to_bits(), a.time_ns, a.bpm.to_bits()),
            (b.beat.to_bits(), b.time_ns, b.bpm.to_bits())
        );
    }
    assert_eq!(
        old.row_to_beat
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        new.row_to_beat
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
    for beat in [
        -4.0, -0.01, -0.0, 0.0, 0.0105, 1.0, 2.49, 2.51, 3.0, 4.0, 4.001, 17.0, 64.0, 1024.0,
    ] {
        let time = old.get_time_for_beat_ns(beat);
        assert_eq!(time, new.get_time_for_beat_ns(beat));
        assert_eq!(
            old.get_time_for_beat_exact(beat).to_bits(),
            new.get_time_for_beat_exact(beat).to_bits()
        );
        assert_eq!(
            old.get_bpm_for_beat(beat).to_bits(),
            new.get_bpm_for_beat(beat).to_bits()
        );
        assert_eq!(
            old.get_displayed_beat(beat).to_bits(),
            new.get_displayed_beat(beat).to_bits()
        );
        assert_eq!(
            old.get_speed_multiplier_ns(beat, time).to_bits(),
            new.get_speed_multiplier_ns(beat, time).to_bits()
        );
        assert_eq!(
            format!("{:?}", old.get_beat_info_from_time_ns(time)),
            format!("{:?}", new.get_beat_info_from_time_ns(time))
        );
    }
}

#[test]
fn timing_construction_matches_legacy_tables_and_queries() {
    let rows: Vec<_> = (0..2048).map(|i| i as f32 / 48.0).collect();
    for count in [0, 1, 2, 32, 256] {
        for modifiers in [false, true] {
            for reverse in [false, true] {
                let mut segments = construction_fixture(count, modifiers);
                if reverse {
                    segments.bpms.reverse();
                    segments.stops.reverse();
                    segments.delays.reverse();
                    segments.warps.reverse();
                    segments.speeds.reverse();
                    segments.scrolls.reverse();
                    segments.fakes.reverse();
                }
                let original = format!("{segments:?}");
                for (song, global) in [(0.0, 0.0), (0.173, -0.011), (-0.25, 0.015)] {
                    let old = TimingData::legacy_from_segments(song, global, &segments, &rows);
                    let new = TimingData::from_segments(song, global, &segments, &rows);
                    assert_same_timing(&old, &new);
                }
                assert_eq!(original, format!("{segments:?}"));
            }
        }
    }
}

#[test]
fn timing_construction_preserves_duplicate_beats_and_offset_detachment() {
    let mut segments = construction_fixture(8, true);
    segments.bpms = vec![
        (0.0, 120.0),
        (4.0, 150.0),
        (4.0, 90.0),
        (4.001, 175.0),
        (8.0, 160.0),
    ];
    segments.stops.push(StopSegment {
        beat: 1.0,
        duration: -0.025,
    });
    let old = TimingData::legacy_from_segments(0.125, 0.007, &segments, &[0.0, 4.0, 4.0, f32::NAN]);
    let new = TimingData::from_segments(0.125, 0.007, &segments, &[0.0, 4.0, 4.0, f32::NAN]);
    let mut shifted_old = old.clone();
    let mut shifted_new = new.clone();
    shifted_old.shift_song_offset_seconds(0.033);
    shifted_new.shift_song_offset_seconds(0.033);
    shifted_old.set_global_offset_seconds(-0.025);
    shifted_new.set_global_offset_seconds(-0.025);
    assert_same_timing(&shifted_old, &shifted_new);
    assert_same_timing(&old, &new);
    assert!(Arc::ptr_eq(&new.row_to_beat, &shifted_new.row_to_beat));
    assert!(!Arc::ptr_eq(&new.beat_to_time, &shifted_new.beat_to_time));
}

#[test]
#[ignore = "manual old/new timing construction benchmark; run in release"]
fn timing_construction_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    let rows: Vec<_> = (0..8192).map(|i| i as f32 / 48.0).collect();
    for (label, count, modifiers, reversed, iterations) in [
        ("default", 0, false, false, 1000),
        ("single", 1, false, false, 1000),
        ("bpms", 1024, false, false, 100),
        ("modifiers", 256, true, false, 100),
        ("reversed", 256, true, true, 100),
    ] {
        let mut segments = construction_fixture(count, modifiers);
        if reversed {
            segments.bpms.reverse();
            segments.stops.reverse();
            segments.delays.reverse();
            segments.warps.reverse();
            segments.speeds.reverse();
            segments.scrolls.reverse();
            segments.fakes.reverse();
        }
        for old in order {
            let suffix = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("construct_{label}_{suffix}"),
                iterations,
                1,
                || {
                    if old {
                        TimingData::legacy_from_segments(
                            black_box(0.125),
                            -0.01,
                            black_box(&segments),
                            black_box(&rows),
                        )
                    } else {
                        TimingData::from_segments(
                            black_box(0.125),
                            -0.01,
                            black_box(&segments),
                            black_box(&rows),
                        )
                    }
                },
            );
        }
    }
}

#[test]
fn timing_construction_avoids_temporary_allocation_churn() {
    let segments = construction_fixture(1, false);
    let rows = [0.0; 128];
    crate::perf::assert_churn_budget(13, 1024, || {
        black_box(TimingData::from_segments(0.1, -0.01, &segments, &rows));
    });
}

#[test]
fn row_time_cache_requires_usable_unique_grid_aligned_bpms() {
    for (bpms, supported) in [
        (vec![(0.0, 120.0), (4.0, 175.0)], true),
        (vec![(0.0, 120.0), (4.001, 175.0)], false),
        (vec![(0.0, 120.0), (4.0, 175.0), (4.0, 90.0)], false),
        (vec![(0.0, 0.0)], false),
        (vec![(0.0, -120.0)], false),
        (vec![(0.0, f32::NAN)], false),
        (vec![(0.0, f32::INFINITY)], false),
    ] {
        let segments = TimingSegments {
            bpms,
            ..TimingSegments::default()
        };
        let old = TimingData::legacy_from_segments(0.0, 0.0, &segments, &[]);
        let new = TimingData::from_segments(0.0, 0.0, &segments, &[]);
        assert_eq!(format!("{old:?}"), format!("{new:?}"));
        assert_eq!(new.supports_row_time_cache(), supported);
    }
}
