// Source-derived regressions for ITGmania Player::CrossedRows and Step. These
// deterministic fixtures use the real chart indices and gameplay paths; they
// are separate from recordings produced by running the original engine.

fn pad_stick_note(row_index: usize, note_type: NoteType) -> ParsedNote {
    ParsedNote {
        player: 0,
        row_index,
        column: 0,
        note_type,
        tail_row_index: matches!(note_type, NoteType::Hold | NoteType::Roll).then_some(192),
    }
}

#[test]
fn pad_stick_uses_inputs_already_held_when_gameplay_starts() {
    for seconds in [0.0, -0.1, 0.1] {
        let mut state = pad_stick_mine_state(seconds);
        let head = state.chart_runtime.note_time_cache_ns[0];
        let mut ages = [0.0; 32];
        ages[VirtualAction::p1_left.ix()] = 1.0;
        state.set_input_snapshot(ages, VirtualAction::p1_left.bit());
        let now = head + song_time_ns_from_seconds(seconds) + 1_000_000;
        pad_stick_post_frame(&mut state, now - 1_000_000, now);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Hit)
        );
        assert_eq!(
            state.lane_input_counts()[0],
            0,
            "no synthetic physical press"
        );
        assert!(state.pending_input_is_empty());
    }
}

#[test]
fn pad_stick_preexisting_pump_hold_uses_physical_snapshot_through_checkpoints() {
    for seconds in [0.0, 0.1] {
        let mut state = pad_stick_state(
            seconds,
            GameplayInputPlayStyle::PumpSingle,
            vec![pad_stick_note(48, NoteType::Hold)],
        );
        let head = state.chart_runtime.note_time_cache_ns[0];
        let mut ages = [0.0; 32];
        ages[VirtualAction::p1_left.ix()] = 1.0;
        state.set_input_snapshot(ages, VirtualAction::p1_left.bit());
        let crossed = head + song_time_ns_from_seconds(seconds) + 1_000_000;
        pad_stick_post_frame(&mut state, head - 1_000_000, crossed);
        assert_eq!(
            state.chart_runtime.notes[0].result.unwrap().grade,
            JudgeGrade::Fantastic
        );
        pad_stick_post_frame(&mut state, crossed, head + 300_000_000);
        assert!(
            state.hold_runtime.active_holds[0]
                .as_ref()
                .unwrap()
                .is_pressed
        );
        assert!(state.players_runtime.players[0].checkpoints_hit > 0);
        assert_eq!(state.players_runtime.players[0].checkpoints_missed, 0);
        assert_eq!(state.lane_input_counts()[0], 0);
    }
}

fn pad_stick_state_with_timing(
    seconds: f32,
    style: GameplayInputPlayStyle,
    timing: TimingSegments,
    notes: Vec<ParsedNote>,
) -> State {
    let mines = notes
        .iter()
        .filter(|note| note.note_type == NoteType::Mine)
        .count() as u32;
    let mut state = hold_regression_state_with_notes(style, timing, 384, notes);
    state.apply_global_offset_delta(-state.global_offset_seconds());
    state.progress.chart_totals.mines_total[0] = mines;
    state.set_pad_stick_seconds(seconds);
    state
}

fn pad_stick_state(seconds: f32, style: GameplayInputPlayStyle, notes: Vec<ParsedNote>) -> State {
    pad_stick_state_with_timing(
        seconds,
        style,
        TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..TimingSegments::default()
        },
        notes,
    )
}

fn pad_stick_mine_state(seconds: f32) -> State {
    pad_stick_state(
        seconds,
        GameplayInputPlayStyle::Single,
        vec![pad_stick_note(48, NoteType::Mine)],
    )
}

fn pad_stick_input_snapshot(state: &mut State, down: bool, age: f32) {
    state
        .control
        .input_state
        .update_slot(0, InputSource::Keyboard, 17, down);
    if down {
        state.control.input_state.press_lane(0, 0);
    } else {
        state.control.input_state.release_lane(0);
    }
    state.control.input_state.prev_inputs[0] = down;
    state.control.input_state.input_age_seconds[0] = age;
}

fn pad_stick_post_frame(state: &mut State, previous: SongTimeNs, now: SongTimeNs) {
    state.set_current_music_time_ns(now);
    state.run_post_input_gameplay_phases(
        previous,
        false,
        now,
        song_time_ns_to_seconds(now),
        0.0,
        false,
        &mut GameplayUpdatePhaseTimings::default(),
    );
}

#[test]
fn pad_stick_zero_hits_crossed_mine_and_positive_waits_for_shifted_row() {
    for seconds in [0.0, 0.1] {
        let mut state = pad_stick_mine_state(seconds);
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, true, 1.0);
        pad_stick_post_frame(&mut state, head - 1_000_000, head + 1_000_000);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            (seconds == 0.0).then_some(MineResult::Hit),
            "stick={seconds} at the unshifted row",
        );
        if seconds > 0.0 {
            pad_stick_post_frame(&mut state, head + 1_000_000, head + 99_000_000);
            assert_eq!(state.chart_runtime.notes[0].mine_result, None);
            pad_stick_post_frame(&mut state, head + 99_000_000, head + 101_000_000);
            assert_eq!(
                state.chart_runtime.notes[0].mine_result,
                Some(MineResult::Hit)
            );
        }
        assert_eq!(state.players_runtime.players[0].mines_hit, 1);
    }
}

#[test]
fn pad_stick_positive_uses_physical_age_and_does_not_revisit_short_inputs() {
    for (down, age, hit) in [(true, 0.099, false), (true, 0.1, true), (false, 0.1, true)] {
        let mut state = pad_stick_mine_state(0.1);
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, down, age);
        state.cross_pad_stick_rows(head + 101_000_000);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            hit.then_some(MineResult::Hit)
        );
        assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
        state.control.input_state.input_age_seconds[0] = 1.0;
        state.cross_pad_stick_rows(head + 151_000_000);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            hit.then_some(MineResult::Hit)
        );
    }
}

#[test]
fn pad_stick_retained_release_can_hit_with_positive_seconds_only() {
    // Native InputFilter can retain a released physical entry through debounce.
    // Input tests cover producing this snapshot; crossing must use its age even
    // though the virtual lane is currently up.
    for seconds in [0.0, 0.04] {
        let mut state = pad_stick_mine_state(seconds);
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, false, 0.09);
        pad_stick_post_frame(&mut state, head - 1_000_000, head + 41_000_000);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            (seconds > 0.0).then_some(MineResult::Hit),
        );
        // AvoidMine shares the late-resolution horizon, including Roll's
        // release window; passing the mine window alone does not finalize it.
        let settled = head + state.timing_runtime.step_resolution_distance_ns + 50_000_000;
        pad_stick_post_frame(&mut state, head + 41_000_000, settled);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            Some(if seconds > 0.0 {
                MineResult::Hit
            } else {
                MineResult::Avoided
            }),
        );
    }
}

#[test]
fn pad_stick_negative_advances_rows_and_uses_current_pressed_state() {
    for (down, age, hit) in [(true, 0.0, true), (false, 1.0, false)] {
        let mut state = pad_stick_mine_state(-0.1);
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, down, age);
        state.cross_pad_stick_rows(head - 101_000_000);
        assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 0);
        state.cross_pad_stick_rows(head - 99_000_000);
        assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            hit.then_some(MineResult::Hit)
        );
    }
}

#[test]
fn pad_stick_crossing_uses_song_seconds_but_judgment_backdates_by_rate() {
    // At half speed the synthetic press is 102 ms late in real time and misses
    // the mine window; at double speed it is 49.5 ms early and hits. Both cross
    // the row only after 100 ms of song time, independent of playback speed.
    for (rate, hit) in [(0.5, false), (2.0, true)] {
        let mut state = pad_stick_mine_state(0.1);
        state.set_music_rate(rate);
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, true, 1.0);
        state.cross_pad_stick_rows(head + 80_000_000);
        assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 0);
        state.cross_pad_stick_rows(head + 101_000_000);
        assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            hit.then_some(MineResult::Hit)
        );
    }
}

#[test]
fn pad_stick_crossing_an_already_hit_mine_can_judge_a_nearby_tap() {
    for seconds in [0.0, 0.1] {
        let mut state = pad_stick_state(
            seconds,
            GameplayInputPlayStyle::Single,
            vec![
                pad_stick_note(48, NoteType::Mine),
                pad_stick_note(60, NoteType::Tap),
            ],
        );
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, true, 1.0);
        assert!(state.judge_a_tap(0, head));
        state.apply_pending_mine_hits();
        state.cross_pad_stick_rows(head + 101_000_000);
        let tap = &state.chart_runtime.notes[1];
        let judgment = tap.result.as_ref().or(tap.early_result.as_ref());
        if seconds > 0.0 {
            let judgment = judgment.expect("the positive held step searches past the hit mine");
            assert_eq!(judgment.grade, JudgeGrade::Decent);
            assert!((judgment.time_error_ms + 124.0).abs() < 0.01);
        } else {
            assert!(judgment.is_none(), "zero uses the mine's explicit row");
        }
        assert_eq!(state.players_runtime.players[0].mines_hit, 1);
    }
}

#[test]
fn pad_stick_synthetic_press_does_not_judge_a_nearby_lift() {
    let mut state = pad_stick_state(
        0.1,
        GameplayInputPlayStyle::Single,
        vec![
            pad_stick_note(48, NoteType::Mine),
            pad_stick_note(52, NoteType::Lift),
        ],
    );
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_input_snapshot(&mut state, true, 1.0);
    assert!(state.judge_a_tap(0, head));
    state.cross_pad_stick_rows(head + 101_000_000);
    assert!(state.chart_runtime.notes[1].result.is_none());
    assert!(state.chart_runtime.notes[1].early_result.is_none());
}

#[test]
fn pad_stick_live_changes_keep_the_crossed_row_cursor_forward_only() {
    let mut state = pad_stick_state(
        0.0,
        GameplayInputPlayStyle::Single,
        vec![
            pad_stick_note(48, NoteType::Mine),
            pad_stick_note(96, NoteType::Mine),
        ],
    );
    let first = state.chart_runtime.note_time_cache_ns[0];
    let second = state.chart_runtime.note_time_cache_ns[1];
    pad_stick_input_snapshot(&mut state, false, 0.0);
    pad_stick_post_frame(&mut state, first - 1_000_000, first + 1_000_000);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
    state.set_pad_stick_seconds(1.0);
    pad_stick_input_snapshot(&mut state, true, 2.0);
    state.cross_pad_stick_rows(second + 101_000_000);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
    state.set_pad_stick_seconds(0.1);
    state.cross_pad_stick_rows(second + 101_000_000);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 2);
    assert_eq!(state.chart_runtime.notes[0].mine_result, None);
    assert_eq!(
        state.chart_runtime.notes[1].mine_result,
        Some(MineResult::Hit)
    );
}

#[test]
fn pad_stick_stop_allows_crossing_while_actual_delay_blocks_it() {
    for delay in [false, true] {
        let mut timing = TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..TimingSegments::default()
        };
        if delay {
            timing.delays.push(DelaySegment {
                beat: 1.0,
                duration: 0.3,
            });
        } else {
            timing.stops.push(deadsync_rules::timing::StopSegment {
                beat: 1.0,
                duration: 0.3,
            });
        }
        let mut state = pad_stick_state_with_timing(
            0.1,
            GameplayInputPlayStyle::Single,
            timing,
            vec![pad_stick_note(48, NoteType::Mine)],
        );
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, true, 1.0);
        if delay {
            pad_stick_post_frame(&mut state, head - 301_000_000, head - 151_000_000);
            assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 0);
            assert_eq!(state.chart_runtime.notes[0].mine_result, None);
            // The actual position has left the delay, but the shifted position
            // is still inside it. Crossing now backdates 99 ms before the mine.
            pad_stick_post_frame(&mut state, head - 151_000_000, head + 1_000_000);
            assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
            assert_eq!(state.chart_runtime.notes[0].mine_result, None);
            let settled = head + state.timing_runtime.step_resolution_distance_ns + 50_000_000;
            pad_stick_post_frame(&mut state, head + 1_000_000, settled);
            assert_eq!(
                state.chart_runtime.notes[0].mine_result,
                Some(MineResult::Avoided)
            );
        } else {
            pad_stick_post_frame(&mut state, head - 1_000_000, head + 101_000_000);
            assert_eq!(
                state.chart_runtime.notes[0].mine_result,
                Some(MineResult::Hit)
            );
        }
    }
}

#[test]
fn pad_stick_autoplay_shifts_crossing_without_changing_its_judgment() {
    let mut control = pad_stick_state(
        0.0,
        GameplayInputPlayStyle::Single,
        vec![pad_stick_note(48, NoteType::Tap)],
    );
    let control_head = control.chart_runtime.note_time_cache_ns[0];
    control.set_live_autoplay_enabled(true);
    control.control.autoplay_runtime = GameplayAutoplayRuntimeState::new(23, [0; MAX_PLAYERS]);
    control.run_autoplay(control_head + 1_000_000);
    let control_judgment = control.chart_runtime.notes[0].result.unwrap();
    assert_eq!(control_judgment.grade, JudgeGrade::Fantastic);
    for (seconds, before, after) in [
        (0.1, 99_000_000, 101_000_000),
        (-0.1, -101_000_000, -99_000_000),
    ] {
        let mut state = pad_stick_state(
            seconds,
            GameplayInputPlayStyle::Single,
            vec![pad_stick_note(48, NoteType::Tap)],
        );
        let head = state.chart_runtime.note_time_cache_ns[0];
        state.set_live_autoplay_enabled(true);
        state.control.autoplay_runtime = GameplayAutoplayRuntimeState::new(23, [0; MAX_PLAYERS]);
        state.run_autoplay(head + before);
        assert!(state.chart_runtime.notes[0].result.is_none());
        state.run_autoplay(head + after);
        let judgment = state.chart_runtime.notes[0]
            .result
            .as_ref()
            .expect("autoplay crosses the row");
        assert_eq!(judgment.grade, JudgeGrade::Fantastic);
        // The exact-row step receives existing seeded autoplay feedback. A
        // changed crossing time must produce the same grade, window and offset.
        assert_eq!(judgment.window, control_judgment.window);
        assert_eq!(
            judgment.time_error_music_ns,
            control_judgment.time_error_music_ns
        );
    }
}

#[test]
fn pad_stick_pump_preheld_head_waits_and_keeps_the_measured_offset() {
    let mut state = pad_stick_state(
        0.1,
        GameplayInputPlayStyle::PumpSingle,
        vec![pad_stick_note(48, NoteType::Hold)],
    );
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_input_snapshot(&mut state, true, 1.0);
    pad_stick_post_frame(&mut state, head - 1_000_000, head + 1_000_000);
    assert!(state.chart_runtime.notes[0].result.is_none());
    pad_stick_post_frame(&mut state, head + 1_000_000, head + 101_000_000);
    let judgment = state.chart_runtime.notes[0]
        .result
        .as_ref()
        .expect("delayed Pump held step");
    assert_eq!(judgment.grade, JudgeGrade::Fantastic);
    assert!((judgment.time_error_ms - 1.0).abs() < 0.01);
    assert!(state.hold_runtime.active_holds[0].is_some());
}

#[test]
fn pad_stick_pump_synthetic_head_uses_max_grade_outside_normal_tap_window() {
    let mut state = pad_stick_state(
        0.5,
        GameplayInputPlayStyle::PumpSingle,
        vec![pad_stick_note(48, NoteType::Hold)],
    );
    state.set_music_rate(0.5);
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_input_snapshot(&mut state, true, 1.0);
    state.cross_pad_stick_rows(head + 501_000_000);
    let judgment = state.chart_runtime.notes[0]
        .result
        .as_ref()
        .expect("Pump overrides the tap window");
    assert_eq!(judgment.grade, JudgeGrade::Fantastic);
    assert!((judgment.time_error_ms - 502.0).abs() < 0.01);
    assert!((judgment.time_error_music_ns - 251_000_000).abs() < 1_000);
}

#[test]
fn pad_stick_delayed_crossing_resets_life_of_an_already_judged_dance_hold() {
    let mut state = pad_stick_state(
        0.1,
        GameplayInputPlayStyle::Single,
        vec![pad_stick_note(48, NoteType::Hold)],
    );
    let head = state.chart_runtime.note_time_cache_ns[0];
    assert!(state.judge_a_tap(0, head));
    state.chart_runtime.notes[0].hold.as_mut().unwrap().life = 0.2;
    state.hold_runtime.active_holds[0].as_mut().unwrap().life = 0.2;
    state.cross_pad_stick_rows(head + 101_000_000);
    assert_eq!(
        state.chart_runtime.notes[0].hold.as_ref().unwrap().life,
        INITIAL_HOLD_LIFE
    );
    assert_eq!(
        state.hold_runtime.active_holds[0].as_ref().unwrap().life,
        INITIAL_HOLD_LIFE
    );
    assert_eq!(
        state.chart_runtime.notes[0]
            .result
            .as_ref()
            .unwrap()
            .time_error_music_ns,
        0
    );
}

#[test]
fn pad_stick_restart_reanchors_crossing_and_clears_physical_ages() {
    let mut state = pad_stick_mine_state(0.1);
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_input_snapshot(&mut state, true, 1.0);
    state.cross_pad_stick_rows(head + 101_000_000);
    assert_eq!(
        state.chart_runtime.notes[0].mine_result,
        Some(MineResult::Hit)
    );
    state.reset_practice_playback(0.0);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 0);
    assert_eq!(state.chart_runtime.notes[0].mine_result, None);
    assert_eq!(state.control.input_state.input_age_seconds[0], 0.0);
    assert!(!state.control.input_state.lane_is_pressed(0));
    assert_eq!(state.pad_stick_seconds(), 0.1);
    pad_stick_input_snapshot(&mut state, true, 1.0);
    state.cross_pad_stick_rows(head + 101_000_000);
    assert_eq!(
        state.chart_runtime.notes[0].mine_result,
        Some(MineResult::Hit)
    );
}

#[test]
fn pad_stick_does_not_delay_a_physical_press_on_a_mine() {
    let mut state = pad_stick_mine_state(0.1);
    let head = state.chart_runtime.note_time_cache_ns[0];
    assert_eq!(state.control.input_state.input_age_seconds[0], 0.0);
    assert!(state.judge_a_tap(0, head));
    assert_eq!(
        state.chart_runtime.notes[0].mine_result,
        Some(MineResult::Hit)
    );
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 0);
}

#[test]
fn pad_stick_negative_to_zero_does_not_replay_a_consumed_mine() {
    let mut state = pad_stick_mine_state(-0.1);
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_post_frame(&mut state, head - 101_000_000, head - 99_000_000);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
    assert_eq!(state.chart_runtime.notes[0].mine_result, None);
    state.set_pad_stick_seconds(0.0);
    let press = head - 80_000_000;
    assert!(
        !state.judge_a_tap(0, press),
        "the new press is outside the mine window"
    );
    pad_stick_input_snapshot(&mut state, true, 0.081);
    state.control.input_state.press_lane(0, press);
    pad_stick_post_frame(&mut state, press, head + 1_000_000);
    assert_eq!(state.chart_runtime.notes[0].mine_result, None);
    let settled = head + state.timing_runtime.step_resolution_distance_ns + 50_000_000;
    pad_stick_post_frame(&mut state, head + 1_000_000, settled);
    assert_eq!(
        state.chart_runtime.notes[0].mine_result,
        Some(MineResult::Avoided)
    );
}

#[test]
fn pad_stick_zero_crossing_runs_step_callback_before_the_next_column() {
    let first = pad_stick_note(48, NoteType::Mine);
    let mut second = pad_stick_note(48, NoteType::Mine);
    second.column = 1;
    let mut state = pad_stick_state(0.0, GameplayInputPlayStyle::Single, vec![first, second]);
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_input_snapshot(&mut state, true, 1.0);
    state
        .control
        .input_state
        .update_slot(1, InputSource::Keyboard, 18, true);
    state.control.input_state.press_lane(1, 0);
    state.control.input_state.prev_inputs[1] = true;
    state.control.input_state.input_age_seconds[1] = 0.05;
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed_calls = calls.clone();
    state.step_callback = Some(Box::new(move |game, column| {
        observed_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(column, 0);
        game.set_pad_stick_seconds(0.1);
    }));
    pad_stick_post_frame(&mut state, head - 1_000_000, head + 1_000_000);
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(state.pad_stick_seconds(), 0.1);
    assert_eq!(
        state.chart_runtime.notes[0].mine_result,
        Some(MineResult::Hit)
    );
    assert_eq!(state.chart_runtime.notes[1].mine_result, None);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 2);
}

#[test]
fn pad_stick_zero_held_mine_survives_a_frame_later_than_the_hit_window() {
    let mut state = pad_stick_mine_state(0.0);
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_input_snapshot(&mut state, true, 1.0);

    // An explicit-row held step uses zero offset even when a slow frame crosses
    // the mine later than a physical press could hit it.
    pad_stick_post_frame(&mut state, head - 100_000_000, head + 200_000_000);

    assert_eq!(
        state.chart_runtime.notes[0].mine_result,
        Some(MineResult::Hit)
    );
    assert_eq!(state.players_runtime.players[0].mines_hit, 1);
    assert_eq!(state.players_runtime.players[0].mines_hit_for_score, 1);
    assert!(
        state
            .chart_runtime
            .mine_scan
            .pending_mine_hit_indices
            .is_empty()
    );
    pad_stick_post_frame(&mut state, head + 200_000_000, head + 210_000_000);
    assert_eq!(state.players_runtime.players[0].mines_hit, 1);
}

#[test]
fn pad_stick_zero_pump_crossing_keeps_frame_offset_and_emits_one_step() {
    let mut state = pad_stick_state(
        0.0,
        GameplayInputPlayStyle::PumpSingle,
        vec![pad_stick_note(48, NoteType::Hold)],
    );
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_input_snapshot(&mut state, true, 1.0);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed_calls = calls.clone();
    state.step_callback = Some(Box::new(move |_, column| {
        assert_eq!(column, 0);
        observed_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }));

    pad_stick_post_frame(&mut state, head - 1_000_000, head + 15_000_000);

    // Native zero uses the current frame's timestamp, even though Pump takes
    // the highest enabled tap grade. It does not substitute the head timestamp.
    let judgment = state.chart_runtime.notes[0]
        .result
        .as_ref()
        .expect("held Pump head");
    assert_eq!(judgment.grade, JudgeGrade::Fantastic);
    assert!((judgment.time_error_ms - 15.0).abs() < 0.01);
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    pad_stick_post_frame(&mut state, head + 15_000_000, head + 25_000_000);
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[test]
fn pad_stick_negative_to_zero_does_not_reprocess_a_pump_head() {
    let mut state = pad_stick_state(
        -0.3,
        GameplayInputPlayStyle::PumpSingle,
        vec![pad_stick_note(48, NoteType::Hold)],
    );
    let head = state.chart_runtime.note_time_cache_ns[0];
    pad_stick_post_frame(&mut state, head - 301_000_000, head - 299_000_000);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
    assert!(state.chart_runtime.notes[0].result.is_none());

    // Native Pump auto-W1 has a W5 early bound. A press 250 ms early cannot
    // judge this head; a later held crossing must not replay the consumed row.
    let press = head - 250_000_000;
    assert!(!state.judge_a_tap(0, press));
    pad_stick_input_snapshot(&mut state, true, 0.265);
    state.control.input_state.press_lane(0, press);
    state.set_pad_stick_seconds(0.0);
    pad_stick_post_frame(&mut state, press, head + 15_000_000);
    assert_eq!(state.chart_runtime.mine_scan.crossed_note_cursor[0], 1);
    assert!(state.chart_runtime.notes[0].result.is_none());
}

#[test]
fn pad_stick_pump_head_resets_life_without_restarting_integrated_hold_state() {
    for down in [false, true] {
        let mut state = pad_stick_state(
            0.1,
            GameplayInputPlayStyle::PumpSingle,
            vec![pad_stick_note(48, NoteType::Hold)],
        );
        let head = state.chart_runtime.note_time_cache_ns[0];
        pad_stick_input_snapshot(&mut state, down, 1.0);
        pad_stick_post_frame(&mut state, head - 1_000_000, head + 1_000_000);
        let now = head + 101_000_000;
        pad_stick_post_frame(&mut state, head + 1_000_000, now);

        // Source-derived: CrossedRows assigns only HoldResult.fLife. An ordinary
        // Hold's synthetic Step must preserve the preceding hold integration;
        // only Roll has Step's refill behavior. The initial Pump metric is .05.
        let judgment = state.chart_runtime.notes[0]
            .result
            .as_ref()
            .expect("qualified held step");
        assert_eq!(judgment.grade, JudgeGrade::Fantastic);
        let hold = state.chart_runtime.notes[0].hold.as_ref().unwrap();
        let active = state.hold_runtime.active_holds[0].as_ref().unwrap();
        assert_eq!(hold.life, PUMP_INITIAL_HOLD_LIFE);
        assert_eq!(active.life, PUMP_INITIAL_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, now);
        assert_eq!(active.is_pressed, down);
    }
}

#[test]
fn pad_stick_negative_pump_roll_does_not_refill_before_its_actual_head() {
    let mut state = pad_stick_state(
        -0.1,
        GameplayInputPlayStyle::PumpSingle,
        vec![pad_stick_note(48, NoteType::Roll)],
    );
    let head = state.chart_runtime.note_time_cache_ns[0];
    let now = head - 99_000_000;
    pad_stick_input_snapshot(&mut state, true, 1.0);
    state.cross_pad_stick_rows(now);

    // Source-derived: Step's Roll refill requires the current song row to have
    // reached the head. A negative crossing can judge an early head without it.
    let judgment = state.chart_runtime.notes[0]
        .result
        .as_ref()
        .expect("early Pump Roll head");
    assert_eq!(judgment.grade, JudgeGrade::Fantastic);
    assert!((judgment.time_error_ms + 99.0).abs() < 0.01);
    let hold = state.chart_runtime.notes[0].hold.as_ref().unwrap();
    let active = state.hold_runtime.active_holds[0].as_ref().unwrap();
    assert_eq!(hold.life, PUMP_INITIAL_HOLD_LIFE);
    assert_eq!(active.life, PUMP_INITIAL_HOLD_LIFE);
    assert_eq!(active.last_update_time_ns, now);
}
