use super::*;
use std::hint::black_box;

#[path = "timing_lookup_original.rs"]
mod original;
#[path = "../../../tests/perf/core_lookup_support.rs"]
mod support;

fn fixture(count: usize, duplicate: bool) -> TimingData {
    TimingData::from_segments(
        0.125,
        -0.007,
        &TimingSegments {
            bpms: (0..count)
                .map(|i| {
                    (
                        (if duplicate { i / 2 } else { i }) as f32 * 4.0,
                        120.0 + (i % 5) as f32 * 15.0,
                    )
                })
                .collect(),
            ..Default::default()
        },
        &[],
    )
}

#[test]
fn cursor_bpm_matches_original_for_every_hint_and_row_boundary() {
    for beats in [
        vec![],
        vec![f32::NAN],
        vec![0.0],
        vec![-4.0, 0.0, 4.0],
        vec![-0.0, 0.0, 4.0, 4.0],
        vec![0.001, 0.002, 0.009, 0.011, 1.0],
        vec![f32::NEG_INFINITY, 0.0, f32::INFINITY],
        vec![0.0, f32::NAN, -5.0, 9.0],
    ] {
        // Include malformed tables without running their event state machine.
        let bpms: Arc<[BpmPoint]> = beats
            .iter()
            .enumerate()
            .map(|(i, &beat)| BpmPoint {
                beat,
                bpm: 100.0 + i as f32,
            })
            .collect();
        let timing = TimingData {
            bpms_strictly_sorted: bpms.windows(2).all(|pair| pair[0].beat < pair[1].beat),
            bpms,
            ..TimingData::default()
        };
        for row in [
            i32::MIN,
            -193,
            -192,
            -1,
            0,
            1,
            2,
            47,
            48,
            191,
            192,
            193,
            i32::MAX,
        ] {
            for bpm_idx in 0..=beats.len() + 1 {
                let start = GetBeatStarts {
                    last_row: row,
                    bpm_idx,
                    ..Default::default()
                };
                assert_eq!(
                    timing.bpm_at_start(&start).to_bits(),
                    timing.get_bpm_for_beat(note_row_to_beat(row)).to_bits(),
                    "{beats:?}, {row}, {bpm_idx}"
                );
            }
        }
    }
}

fn assert_start(a: &GetBeatStarts, b: &GetBeatStarts) {
    assert_eq!(
        (
            a.bpm_idx,
            a.stop_idx,
            a.delay_idx,
            a.warp_idx,
            a.last_row,
            a.last_time_ns,
            a.warp_destination.to_bits(),
            a.is_warping
        ),
        (
            b.bpm_idx,
            b.stop_idx,
            b.delay_idx,
            b.warp_idx,
            b.last_row,
            b.last_time_ns,
            b.warp_destination.to_bits(),
            b.is_warping
        )
    );
}

#[test]
fn timing_cursors_match_original_across_events_offsets_and_rewinds() {
    for duplicate in [false, true] {
        for count in [1, 2, 64, 1024] {
            let mut timing = fixture(count, duplicate);
            for shift in [0.0, 0.1, -0.25] {
                timing.shift_song_offset_seconds(shift);
                timing.set_global_offset_seconds(shift);
                let initial = GetBeatStarts {
                    last_time_ns: timing.beat_start_time_ns(),
                    ..Default::default()
                };
                let mut a = initial;
                let mut b = initial;
                let mut c = initial;
                let mut d = initial;
                for i in 0..4096 {
                    // Restart halfway to exercise a rewind, then resume forward.
                    if i == 2048 {
                        a = initial;
                        b = initial;
                        c = initial;
                        d = initial;
                    }
                    let seconds = (i % 2048) as f32 * count as f32 / 1024.0 - 0.25;
                    let mut aa = GetBeatArgs {
                        elapsed_time_ns: timing_ns_from_seconds(seconds),
                        ..Default::default()
                    };
                    let mut bb = aa;
                    timing.get_beat_internal(&mut a, &mut aa, usize::MAX);
                    timing.get_beat_internal_original(&mut b, &mut bb, usize::MAX);
                    assert_eq!(
                        (
                            aa.beat.to_bits(),
                            aa.bpm_out.to_bits(),
                            aa.warp_dest_out.to_bits(),
                            aa.warp_begin_out,
                            aa.freeze_out,
                            aa.delay_out
                        ),
                        (
                            bb.beat.to_bits(),
                            bb.bpm_out.to_bits(),
                            bb.warp_dest_out.to_bits(),
                            bb.warp_begin_out,
                            bb.freeze_out,
                            bb.delay_out
                        )
                    );
                    assert_start(&a, &b);
                    for continuous in [false, true] {
                        let beat = seconds * 2.0;
                        let actual = timing.get_elapsed_time_internal_mut(
                            &mut c,
                            beat,
                            usize::MAX,
                            continuous,
                        );
                        let expected = timing.get_elapsed_time_internal_mut_original(
                            &mut d,
                            beat,
                            usize::MAX,
                            continuous,
                        );
                        assert_eq!(actual, expected);
                        assert_start(&c, &d);
                    }
                }
            }
        }
    }
    let timing = TimingData::from_segments(
        0.1,
        -0.01,
        &TimingSegments {
            bpms: vec![
                (-1.0, 120.0),
                (0.001, 180.0),
                (0.009, 100.0),
                (4.0, 240.0),
                (8.0, 150.0),
            ],
            stops: vec![StopSegment {
                beat: 2.0,
                duration: 0.2,
            }],
            delays: vec![DelaySegment {
                beat: 4.0,
                duration: 0.3,
            }],
            warps: vec![WarpSegment {
                beat: 6.0,
                length: 1.5,
            }],
            ..Default::default()
        },
        &[],
    );
    for i in -100..2000 {
        let mut a = GetBeatStarts {
            last_time_ns: timing.beat_start_time_ns(),
            ..Default::default()
        };
        let mut b = a;
        let mut aa = GetBeatArgs {
            elapsed_time_ns: i64::from(i) * 5_000_000,
            ..Default::default()
        };
        let mut bb = aa;
        timing.get_beat_internal(&mut a, &mut aa, usize::MAX);
        timing.get_beat_internal_original(&mut b, &mut bb, usize::MAX);
        assert_eq!(format!("{aa:?}"), format!("{bb:?}"));
        assert_start(&a, &b);
    }
}

#[test]
fn speed_lookup_matches_original_bitwise_including_nonfinite_values() {
    for delay in [
        f32::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        0.5,
        f32::INFINITY,
        f32::NAN,
    ] {
        for unit in [SpeedUnit::Seconds, SpeedUnit::Beats] {
            let timing = TimingData::from_segments(
                0.125,
                -0.01,
                &TimingSegments {
                    bpms: vec![(0.0, 120.0), (4.0, 180.0)],
                    speeds: vec![
                        SpeedSegment {
                            beat: 2.0,
                            ratio: 0.5,
                            delay,
                            unit,
                        },
                        SpeedSegment {
                            beat: 4.0,
                            ratio: 2.0,
                            delay,
                            unit,
                        },
                    ],
                    ..Default::default()
                },
                &[],
            );
            for beat in [
                f32::NEG_INFINITY,
                -1.0,
                0.0,
                1.999,
                2.0,
                3.0,
                4.0,
                5.0,
                f32::INFINITY,
                f32::NAN,
            ] {
                for seconds in [
                    f32::NEG_INFINITY,
                    -10.0,
                    0.0,
                    0.875,
                    1.0,
                    1.5,
                    2.0,
                    10.0,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    assert_eq!(
                        timing.get_speed_multiplier(beat, seconds).to_bits(),
                        timing
                            .get_speed_multiplier_with_original(beat, || timing_ns_from_seconds(
                                seconds
                            ))
                            .to_bits()
                    );
                }
                for ns in [i64::MIN, -1, 0, 875_000_000, 1_000_000_000, i64::MAX] {
                    assert_eq!(
                        timing.get_speed_multiplier_ns(beat, ns).to_bits(),
                        timing
                            .get_speed_multiplier_with_original(beat, || ns)
                            .to_bits()
                    );
                }
            }
        }
    }
    let timing = fixture(1, false);
    assert_eq!(timing.get_speed_multiplier(0.0, f32::NAN), 1.0);
}

fn clock_batch(timing: &TimingData, old: bool, count: usize, elapsed: bool) {
    let mut start = GetBeatStarts {
        last_time_ns: timing.beat_start_time_ns(),
        ..Default::default()
    };
    for i in 0..1024 {
        let beat = black_box(i as f32 * count as f32 / 256.0);
        if elapsed {
            let value = if old {
                timing.get_elapsed_time_internal_mut_original(&mut start, beat, usize::MAX, false)
            } else {
                timing.get_elapsed_time_internal_mut(&mut start, beat, usize::MAX, false)
            };
            black_box(value);
        } else {
            let mut args = GetBeatArgs {
                elapsed_time_ns: timing_ns_from_seconds(beat * 0.3),
                ..Default::default()
            };
            if old {
                timing.get_beat_internal_original(&mut start, &mut args, usize::MAX)
            } else {
                timing.get_beat_internal(&mut start, &mut args, usize::MAX)
            };
            black_box(args);
        }
    }
    black_box(start);
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_core_lookups() {
    for count in [1, 2, 64, 1024] {
        for duplicate in [false, true] {
            if duplicate && count != 64 {
                continue;
            }
            let timing = fixture(count, duplicate);
            for elapsed in [false, true] {
                support::compare(
                    &format!("clock/{count}/duplicate={duplicate}/elapsed={elapsed}"),
                    2,
                    || clock_batch(black_box(&timing), true, count, elapsed),
                    || clock_batch(black_box(&timing), false, count, elapsed),
                );
            }
        }
    }
    for count in [0, 1, 64] {
        for delay in [0.0, 0.5] {
            let timing = TimingData::from_segments(
                0.0,
                0.0,
                &TimingSegments {
                    speeds: (0..count)
                        .map(|i| SpeedSegment {
                            beat: i as f32 * 4.0,
                            ratio: 0.5 + (i % 4) as f32,
                            delay,
                            unit: SpeedUnit::Seconds,
                        })
                        .collect(),
                    ..Default::default()
                },
                &[],
            );
            for before in [false, true] {
                for ns in [false, true] {
                    let beat = if before {
                        -1.0
                    } else {
                        (count / 2) as f32 * 4.0 + 0.125
                    };
                    let seconds = beat + 0.1;
                    let time = timing_ns_from_seconds(seconds);
                    support::compare(
                        &format!("speed/{count}/delay={delay}/before={before}/ns={ns}"),
                        1000,
                        || {
                            let timing = black_box(&timing);
                            let time = black_box(time);
                            let seconds = black_box(seconds);
                            black_box(timing.get_speed_multiplier_with_original(
                                black_box(beat),
                                || {
                                    if ns {
                                        time
                                    } else {
                                        timing_ns_from_seconds(seconds)
                                    }
                                },
                            ));
                        },
                        || {
                            let timing = black_box(&timing);
                            let time = black_box(time);
                            let seconds = black_box(seconds);
                            black_box(if ns {
                                timing.get_speed_multiplier_ns(black_box(beat), time)
                            } else {
                                timing.get_speed_multiplier(black_box(beat), seconds)
                            });
                        },
                    );
                }
            }
        }
    }
}
