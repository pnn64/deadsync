use super::*;
use crate::runtime_regression_tests::{
    NoSongLuaRuntime, State, TestProfile, regression_payload_with_segments, regression_song,
};
use deadsync_chart::notes::ParsedNote;

fn calibration_state(
    rate: f32,
    machine_offset: f32,
    player_shifts_ms: [i32; MAX_PLAYERS],
) -> State {
    let song = Arc::new(regression_song());
    let chart = Arc::new(song.charts[0].clone());
    let mut payload = regression_payload_with_segments(
        TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..TimingSegments::default()
        },
        240,
    );
    payload.parsed_notes = vec![
        ParsedNote {
            row_index: 48,
            column: 0,
            note_type: NoteType::Tap,
            tail_row_index: None,
        },
        ParsedNote {
            row_index: 96,
            column: 1,
            note_type: NoteType::Mine,
            tail_row_index: None,
        },
        ParsedNote {
            row_index: 144,
            column: 2,
            note_type: NoteType::Hold,
            tail_row_index: Some(192),
        },
    ];
    let payload = Arc::new(payload);
    let profiles = std::array::from_fn(|player| {
        let mut profile = TestProfile::default();
        profile.global_offset_shift_ms = player_shifts_ms[player];
        profile
    });
    init_gameplay_runtime(
        song,
        [chart.clone(), chart],
        [payload.clone(), payload],
        GameplayViewport::default(),
        GameplaySession {
            play_style: GameplayInputPlayStyle::Versus,
            ..Default::default()
        },
        GameplayConfig {
            global_offset_seconds: machine_offset,
            machine_allow_per_player_global_offsets: true,
            ..Default::default()
        },
        SyncPref::Default,
        GameplayMiniIndicatorData::default(),
        GameplayNoteskinData::default(),
        NoSongLuaRuntime,
        empty_crossover_annotations,
        5,
        rate,
        [ScrollSpeedSetting::default(); MAX_PLAYERS],
        profiles,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        [CourseLifeConfig::Bar; MAX_PLAYERS],
        false,
        [0; MAX_PLAYERS],
    )
}

fn assert_calibrated_caches(state: &State) {
    let rate = state.music_rate();
    assert_eq!(state.chart_runtime.notes.len(), 6);
    assert_eq!(
        state
            .chart_runtime
            .notes
            .iter()
            .filter(|note| note.hold.is_some())
            .count(),
        2
    );
    assert_eq!(
        state
            .chart_runtime
            .mine_scan
            .mine_note_ix
            .iter()
            .map(Vec::len)
            .sum::<usize>(),
        2
    );
    let expected_time = |player, beat: f32| {
        song_time_ns_from_seconds(beat * 0.5)
            - song_time_ns_from_seconds(rate * state.effective_player_global_offset_seconds(player))
    };
    for (index, note) in state.chart_runtime.notes.iter().enumerate() {
        let player = note.column / state.setup.cols_per_player;
        let expected = expected_time(player, note.beat);
        assert!(
            (state.chart_runtime.note_time_cache_ns[index] - expected).abs() <= 2,
            "note cache: rate={rate}, player={player}, beat={}",
            note.beat
        );
        assert!(
            (state.timing_runtime.timing_players[player].get_time_for_beat_ns(note.beat)
                - expected)
                .abs()
                <= 8
        );
        if let Some(hold) = &note.hold {
            assert!(
                (state.chart_runtime.hold_end_time_cache_ns[index]
                    - expected_time(player, hold.end_beat))
                .abs()
                    <= 8
            );
        }
    }
    for row in &state.chart_runtime.row_entries {
        let note = &state.chart_runtime.notes[row.note_indices()[0].get()];
        let player = note.column / state.setup.cols_per_player;
        assert!((row.time_ns - expected_time(player, note.beat)).abs() <= 8);
    }
    for (column, mine_indices) in state
        .chart_runtime
        .mine_scan
        .mine_note_ix
        .iter()
        .enumerate()
    {
        for (slot, &note_index) in mine_indices.iter().enumerate() {
            let note = &state.chart_runtime.notes[note_index.get()];
            let player = note.column / state.setup.cols_per_player;
            assert!(
                (state.chart_runtime.mine_scan.mine_note_time_ns[column][slot]
                    - expected_time(player, note.beat))
                .abs()
                    <= 8
            );
        }
    }
}

#[test]
fn calibration_preserves_real_time_offsets_for_both_players_and_clock_sources() {
    for rate in [0.5, 1.0, 1.5, 2.0] {
        for machine_offset in [-0.025, 0.025] {
            let mut state = calibration_state(rate, machine_offset, [15, -10]);
            assert_calibrated_caches(&state);
            for mapped in [false, true] {
                let now = Instant::now();
                let clock = current_song_clock_snapshot(
                    GameplayAudioSnapshot {
                        stream_clock: GameplayStreamClockSnapshot {
                            stream_seconds: 0.5 / rate + 2.0,
                            music_nanos: 500_000_000,
                            music_seconds_per_second: rate,
                            has_music_mapping: mapped,
                            valid_at: now,
                            valid_at_host_nanos: 2_000_000_000,
                        },
                        ..Default::default()
                    },
                    rate,
                    2.0,
                );
                let event_time =
                    song_clock_music_time_ns(clock, now + Duration::from_millis(5), 2_005_000_000);
                for player in 0..MAX_PLAYERS {
                    let index = state
                        .chart_runtime
                        .notes
                        .iter()
                        .position(|note| {
                            note.column == player * state.setup.cols_per_player
                                && note.note_type == NoteType::Tap
                        })
                        .unwrap();
                    let hit = state
                        .note_hit_eval(
                            player,
                            state.chart_runtime.note_time_cache_ns[index],
                            event_time,
                        )
                        .unwrap();
                    let actual = state
                        .build_final_note_hit_plan(player, hit, rate)
                        .judgment
                        .time_error_ms;
                    let expected =
                        state.effective_player_global_offset_seconds(player) * 1000.0 + 5.0;
                    assert!(
                        (actual - expected).abs() < 0.002,
                        "rate={rate}, mapped={mapped}, player={player}, machine={machine_offset}, actual={actual}, expected={expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn calibration_refreshes_note_hold_mine_and_row_times_after_rate_and_offset_changes() {
    let mut state = calibration_state(0.5, -0.025, [15, -10]);
    assert_calibrated_caches(&state);
    for rate in [1.5, 2.0, 1.0, 0.5] {
        assert!(state.set_music_rate(rate));
        assert_calibrated_caches(&state);
        let old_times = state.chart_runtime.note_time_cache_ns.clone();
        assert!(state.apply_global_offset_delta(0.010));
        assert_calibrated_caches(&state);
        for (old, new) in old_times
            .iter()
            .zip(&state.chart_runtime.note_time_cache_ns)
        {
            assert!((old - new - song_time_ns_from_seconds(rate * 0.010)).abs() <= 8);
        }
        assert!(
            (state.effective_player_global_offset_seconds(0)
                - state.global_offset_seconds()
                - 0.015)
                .abs()
                < 1e-6
        );
        assert!(
            (state.effective_player_global_offset_seconds(1) - state.global_offset_seconds()
                + 0.010)
                .abs()
                < 1e-6
        );
    }
}

#[test]
fn calibration_does_not_move_assist_rows_or_scheduled_music_times() {
    for rate in [0.5, 1.0, 1.5, 2.0] {
        let mut state = calibration_state(rate, -0.025, [15, -10]);
        let reference = calibration_state(rate, 0.0, [0, 0]);
        for offset_delta in [0.0, 0.050] {
            if offset_delta != 0.0 {
                assert!(state.apply_global_offset_delta(offset_delta));
            }
            for seconds in [0.499, 0.501, 0.999, 1.001, 1.999, 2.001] {
                let time = song_time_ns_from_seconds(seconds);
                let expected = assist_row_no_offset_for_timing(reference.timing(), 0.0, time);
                assert_eq!(
                    state.assist_row_no_offset_ns(time),
                    expected,
                    "rate={rate}, machine={}, seconds={seconds}",
                    state.global_offset_seconds()
                );
            }
            for row in [48, 96, 144, 192] {
                assert_eq!(
                    assist_clap_music_seconds_for_row(state.timing(), row),
                    assist_clap_music_seconds_for_row(reference.timing(), row)
                );
            }
        }
    }
}
