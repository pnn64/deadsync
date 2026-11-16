use deadsync_gameplay::{
    BEAT_TICK_SFX_PATH, GameplayAssistClapState, GameplayTimeToBeatCaches, MEASURE_TICK_SFX_PATH,
    beat_tick_music_seconds_for_row, beat_tick_sfx_for_row, build_beat_tick_rows,
};
use deadsync_rules::timing::{
    DelaySegment, StopSegment, TimeSignatureSegment, TimingData, TimingSegments, WarpSegment,
};

fn timing(segments: TimingSegments) -> TimingData {
    TimingData::from_segments(0.0, 0.0, &segments, &[])
}

#[test]
fn time_signature_grid_matches_native_three_eighths() {
    let timing = timing(TimingSegments {
        bpms: vec![(0.0, 120.0)],
        time_signatures: vec![TimeSignatureSegment {
            beat: 0.0,
            numerator: 3,
            denominator: 8,
        }],
        ..Default::default()
    });
    // ITGmania TimingData::NoteRowToMeasureAndBeat and GameplayAssist::PlayTicks:
    // 72 rows per 3/8 measure, 48 rows per beat within each measure.
    let mut expected = vec![0];
    let mut previous = (0, 0);
    for row in 1..=288 {
        let position = (row / 72, (row % 72) / 48);
        if position != previous {
            expected.push((row - (row % 72) % 48) as usize);
        }
        previous = position;
    }
    assert_eq!(build_beat_tick_rows(&timing, 6.0), expected);
}

#[test]
fn time_signature_change_reanchors_native_grid() {
    let timing = timing(TimingSegments {
        bpms: vec![(0.0, 120.0)],
        time_signatures: vec![
            TimeSignatureSegment {
                beat: 0.0,
                numerator: 4,
                denominator: 4,
            },
            TimeSignatureSegment {
                beat: 2.5,
                numerator: 4,
                denominator: 4,
            },
        ],
        ..Default::default()
    });
    assert_eq!(
        build_beat_tick_rows(&timing, 5.0),
        vec![0, 48, 96, 120, 168, 216]
    );
}

#[test]
fn ordinary_bpm_stop_delay_and_warp_timestamps() {
    let timing = timing(TimingSegments {
        bpms: vec![(0.0, 120.0), (8.0, 240.0)],
        stops: vec![StopSegment {
            beat: 1.0,
            duration: 0.75,
        }],
        delays: vec![DelaySegment {
            beat: 2.0,
            duration: 0.25,
        }],
        warps: vec![WarpSegment {
            beat: 4.0,
            length: 4.0,
        }],
        ..Default::default()
    });
    let rows = build_beat_tick_rows(&timing, 10.0);
    assert_eq!(rows, vec![0, 48, 96, 144, 384, 432, 480]);
    let times: Vec<_> = rows
        .into_iter()
        .map(|row| beat_tick_music_seconds_for_row(&timing, row).unwrap())
        .collect();
    for (actual, expected) in times.into_iter().zip([0.0, 0.5, 2.0, 2.5, 3.0, 3.25, 3.5]) {
        assert!(
            (actual - expected).abs() < 0.000001,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn global_offset_cancels_at_multiple_music_rates() {
    for rate in [0.5, 1.0, 1.5, 2.0] {
        for offset in [-0.1, 0.0, 0.1] {
            let mut timing = timing(TimingSegments {
                bpms: vec![(0.0, 120.0)],
                ..Default::default()
            });
            timing.set_global_offset_seconds_with_rate(offset, rate);
            let mut caches = GameplayTimeToBeatCaches::new(&timing, &[&timing; 2]);
            let current = caches.beat_tick_row_no_offset(&timing, offset * rate, 1_990_000_000);
            let future = caches.beat_tick_future_row(
                &timing,
                offset * rate,
                0.02,
                1_990_000_000,
                rate,
                current,
            );
            assert!(current < 192);
            assert!(future >= 192);
            let mut state = GameplayAssistClapState::new(build_beat_tick_rows(&timing, 8.0));
            let result = state.schedule_update(current, future, true, true);
            assert_eq!(
                &state.rows[result.schedule_start..result.schedule_end],
                &[192]
            );
            assert_eq!(beat_tick_music_seconds_for_row(&timing, 192), Some(2.0));
        }
    }
}

#[test]
fn enabling_during_delay_preserves_upcoming_tick() {
    let timing = timing(TimingSegments {
        bpms: vec![(0.0, 120.0)],
        delays: vec![DelaySegment {
            beat: 4.0,
            duration: 2.0,
        }],
        ..Default::default()
    });
    let mut caches = GameplayTimeToBeatCaches::new(&timing, &[&timing; 2]);
    let current = caches.beat_tick_row_no_offset(&timing, 0.0, 3_000_000_000);
    let mut state = GameplayAssistClapState::new(build_beat_tick_rows(&timing, 6.0));
    state.reset_for_row(current);
    let future = caches.beat_tick_future_row(&timing, 0.0, 0.0, 3_990_000_000, 1.0, current);
    let result = state.schedule_update(current, future, true, false);
    assert_eq!(
        &state.rows[result.schedule_start..result.schedule_end],
        &[192],
        "beat 4 is still in the future: t=4s"
    );
}

#[test]
fn fractional_measure_boundary_is_scheduled_before_it_sounds() {
    let timing = timing(TimingSegments {
        bpms: vec![(0.0, 120.0)],
        time_signatures: vec![TimeSignatureSegment {
            beat: 0.0,
            numerator: 3,
            denominator: 8,
        }],
        ..Default::default()
    });
    let mut caches = GameplayTimeToBeatCaches::new(&timing, &[&timing; 2]);
    let current = caches.beat_tick_row_no_offset(&timing, 0.0, 710_000_000);
    let future = caches.beat_tick_future_row(&timing, 0.0, 0.0, 710_000_000, 1.0, current);
    let mut state = GameplayAssistClapState::new(build_beat_tick_rows(&timing, 4.0));
    let update = state.schedule_update(current, future, true, true);
    assert_eq!(
        &state.rows[update.schedule_start..update.schedule_end],
        &[72]
    );
    assert_eq!(beat_tick_music_seconds_for_row(&timing, 72), Some(0.75));
    assert_eq!(beat_tick_sfx_for_row(&timing, 72), MEASURE_TICK_SFX_PATH);
    assert_eq!(beat_tick_sfx_for_row(&timing, 120), BEAT_TICK_SFX_PATH);
}

#[test]
fn signature_change_selects_measure_sound_at_its_new_origin() {
    let timing = timing(TimingSegments {
        bpms: vec![(0.0, 120.0)],
        time_signatures: vec![
            TimeSignatureSegment {
                beat: 0.0,
                numerator: 4,
                denominator: 4,
            },
            TimeSignatureSegment {
                beat: 2.5,
                numerator: 3,
                denominator: 4,
            },
        ],
        ..Default::default()
    });
    for row in [0, 120, 264] {
        assert_eq!(beat_tick_sfx_for_row(&timing, row), MEASURE_TICK_SFX_PATH);
    }
    for row in [48, 96, 168, 216] {
        assert_eq!(beat_tick_sfx_for_row(&timing, row), BEAT_TICK_SFX_PATH);
    }
}

#[test]
fn restarting_during_stop_does_not_repeat_its_tick() {
    let timing = timing(TimingSegments {
        bpms: vec![(0.0, 120.0)],
        stops: vec![StopSegment {
            beat: 4.0,
            duration: 2.0,
        }],
        ..Default::default()
    });
    let mut caches = GameplayTimeToBeatCaches::new(&timing, &[&timing; 2]);
    let current = caches.beat_tick_row_no_offset(&timing, 0.0, 3_000_000_000);
    let future = caches.beat_tick_future_row(&timing, 0.0, 0.0, 3_000_000_000, 1.0, current);
    let mut state = GameplayAssistClapState::new(build_beat_tick_rows(&timing, 6.0));
    let update = state.schedule_update(current, future, true, true);
    assert_eq!(update.schedule_start, update.schedule_end);
}
