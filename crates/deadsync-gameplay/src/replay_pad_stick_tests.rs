// Gameplay regressions for the source/lane identity available in legacy replays.
// Physical binding IDs and the original wall-clock timeline are not recorded.

fn configure_pad_stick_replay(state: &mut State, edges: Vec<RecordedLaneEdge>) {
    state.progress.stage.autoplay_enabled = true;
    state.progress.replay =
        GameplayReplayRuntimeState::new(GameplayReplayInputState::new(edges), false, 0);
    assert!(state.progress.replay.mode);
    assert!(state.stage_autoplay_enabled());
    assert!(!state.live_autoplay_enabled());
}

fn replay_pad_stick_frame(
    state: &mut State,
    previous: SongTimeNs,
    now: SongTimeNs,
    spectator_age: f32,
) {
    state.set_current_music_time_ns(now);
    let mut live_ages = [0.0; 32];
    live_ages[VirtualAction::p1_left.ix()] = spectator_age;
    state.set_input_snapshot(live_ages, VirtualAction::p1_left.bit());
    assert_eq!(state.control.input_state.input_age_seconds[0], 0.0);

    state.run_autoplay_or_replay_frame(now);
    let clock = SongClockSnapshot {
        song_time_ns: now,
        seconds_per_second: state.music_rate(),
        mapped_audio: true,
        valid_at: Instant::now(),
        valid_at_host_nanos: 0,
        timing_diag_enabled: false,
        timing_diag_callback_gap_ns: 0,
    };
    process_input_edges(
        state,
        false,
        &mut GameplayUpdatePhaseTimings::default(),
        clock,
    );
    assert!(state.pending_input_is_empty());
    pad_stick_post_frame(state, previous, now);
}

#[test]
fn pad_stick_replay_ignores_spectator_ages_at_a_young_recorded_press() {
    for source in [InputSource::Keyboard, InputSource::Gamepad] {
        let mut state = pad_stick_mine_state(0.1);
        let head = state.chart_runtime.note_time_cache_ns[0];
        let press = head + 90_000_000;
        configure_pad_stick_replay(
            &mut state,
            vec![RecordedLaneEdge {
                lane_index: 0,
                pressed: true,
                source,
                event_music_time_ns: press,
            }],
        );

        // The physical replay press is outside the mine window. Its own age at
        // the shifted crossing is 11ms, even while a viewer supplies a long hold.
        replay_pad_stick_frame(&mut state, head - 1_000_000, press, 1_000.0);
        assert!(state.lane_is_pressed(0));
        assert_eq!(state.chart_runtime.notes[0].mine_result, None);
        let crossed = head + 101_000_000;
        replay_pad_stick_frame(&mut state, press, crossed, 1_000.0);
        assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
        assert_eq!(state.chart_runtime.notes[0].mine_result, None);
        assert_eq!(state.players_runtime.players[0].mines_hit, 0);

        let settled = head + state.timing_runtime.step_resolution_distance_ns + 50_000_000;
        replay_pad_stick_frame(&mut state, crossed, settled, 1_000.0);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Avoided)
        );
        assert_eq!(state.players_runtime.players[0].mines_hit, 0);
    }
}

#[test]
fn pad_stick_replay_keeps_keyboard_and_pad_ages_independent() {
    for (older_source, newer_source) in [
        (InputSource::Keyboard, InputSource::Gamepad),
        (InputSource::Gamepad, InputSource::Keyboard),
    ] {
        for release_older in [false, true] {
            let mut state = pad_stick_mine_state(0.1);
            let head = state.chart_runtime.note_time_cache_ns[0];
            let older_press = head - 200_000_000;
            let newer_press = head + 85_000_000;
            let before_crossing = head + 90_000_000;
            let mut edges = vec![
                RecordedLaneEdge {
                    lane_index: 0,
                    pressed: true,
                    source: older_source,
                    event_music_time_ns: older_press,
                },
                RecordedLaneEdge {
                    lane_index: 0,
                    pressed: true,
                    source: newer_source,
                    event_music_time_ns: newer_press,
                },
            ];
            if release_older {
                edges.push(RecordedLaneEdge {
                    lane_index: 0,
                    pressed: false,
                    source: older_source,
                    event_music_time_ns: before_crossing,
                });
            }
            configure_pad_stick_replay(&mut state, edges);

            replay_pad_stick_frame(&mut state, older_press - 1_000_000, older_press, 0.0);
            assert_eq!(state.chart_runtime.notes[0].mine_result, None);
            replay_pad_stick_frame(&mut state, older_press, before_crossing, 0.0);
            assert_eq!(state.chart_runtime.notes[0].mine_result, None);
            assert_eq!(
                state.lane_input_counts()[0],
                if release_older { 1 } else { 2 }
            );

            // Both physical presses missed the mine window. The synthetic held
            // step succeeds only while the older source retains its press age.
            let crossed = head + 101_000_000;
            replay_pad_stick_frame(&mut state, before_crossing, crossed, 0.0);
            assert_eq!(
                state.chart_runtime.notes[0].mine_result,
                (!release_older).then_some(MineResult::Hit),
                "older={older_source:?} newer={newer_source:?} release_older={release_older}",
            );
            assert_eq!(
                state.players_runtime.players[0].mines_hit,
                u32::from(!release_older)
            );
            assert!(state.lane_is_pressed(0));
            assert!(!state.live_autoplay_enabled());
        }
    }
}
