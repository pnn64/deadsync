// Included inside runtime_regression_tests so these cases exercise the real
// gameplay runtime without adding profile, theme, shell, or file fixtures.

fn practice_session_game(with_music: bool) -> State {
    let mut game = regression_state_with_segments(
        TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..Default::default()
        },
        384,
    );
    game.apply_global_offset_delta(-game.global_offset_seconds());
    game.set_live_autoplay_enabled(false);
    if with_music {
        Arc::make_mut(&mut game.source.charts[0]).music_path =
            Some(PathBuf::from("songs/Tests/practice.ogg"));
    }
    game.drain_audio_commands().for_each(drop);
    game
}

fn practice_session_snap(path: &std::path::Path, seconds: f64) -> f64 {
    assert_eq!(path, std::path::Path::new("songs/Tests/practice.ogg"));
    seconds + 0.125
}

fn practice_session_no_music_snap(_: &std::path::Path, _: f64) -> f64 {
    panic!("a chart without music must not ask the audio owner to snap a start")
}

fn practice_session_audio(seconds: f32) -> GameplayAudioSnapshot {
    GameplayAudioSnapshot {
        stream_clock: GameplayStreamClockSnapshot {
            music_nanos: song_time_ns_from_seconds(seconds),
            has_music_mapping: true,
            valid_at_host_nanos: 10_000_000_000,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn practice_session_dispatches_menu_actions_without_a_presenter() {
    use crate::practice::editor::{PracticeEditorFeedback, PracticeEditorSound};
    use crate::practice::{
        PracticeDestination, PracticeMenuKind, PracticeSession, PracticeViewRequest,
    };
    use deadsync_input::practice::{PracticeKey, PracticeKeyEvent};

    let mut game = practice_session_game(true);
    let mut session = PracticeSession::new(&mut game, Default::default());
    let input = |session: &mut PracticeSession, game: &mut State, action| {
        let mut sounds = Vec::new();
        let request = session.handle_input(
            game,
            &test_input_event(action),
            practice_session_snap,
            |_, event| {
                if let PracticeEditorFeedback::Sound(sound) = event {
                    sounds.push(sound);
                }
            },
        );
        (request, sounds)
    };
    let open_close = PracticeEditorSound::MenuOpenClose;
    let moved = PracticeEditorSound::MenuMove;
    assert_eq!(
        input(&mut session, &mut game, VirtualAction::p1_start),
        (PracticeViewRequest::None, vec![open_close])
    );
    assert_eq!(
        input(&mut session, &mut game, VirtualAction::p1_menu_up),
        (PracticeViewRequest::None, vec![moved])
    );
    let menu = session.menu().unwrap();
    assert_eq!(
        (menu.kind, menu.selected),
        (
            PracticeMenuKind::Main,
            PracticeMenuKind::Main.row_count() - 1
        )
    );
    assert_eq!(
        input(&mut session, &mut game, VirtualAction::p1_start),
        (
            PracticeViewRequest::Navigate(PracticeDestination::SelectMusic),
            vec![open_close]
        )
    );
    assert!(session.menu().is_none());

    let help = PracticeKeyEvent {
        key: PracticeKey::Help,
        pressed: true,
        repeat: false,
        timestamp: Instant::now(),
    };
    assert!(
        session
            .handle_raw_key(&mut game, &help, practice_session_snap, |_, _| {})
            .0
    );
    assert_eq!(session.menu().unwrap().kind, PracticeMenuKind::Help);
    assert_eq!(
        input(&mut session, &mut game, VirtualAction::p1_start),
        (PracticeViewRequest::None, vec![open_close])
    );
    assert!(session.menu().is_none() && !session.is_playing());

    input(&mut session, &mut game, VirtualAction::p1_start);
    assert_eq!(
        input(&mut session, &mut game, VirtualAction::p1_start),
        (PracticeViewRequest::PlaybackStarted, vec![open_close])
    );
    assert!(session.menu().is_none() && session.is_playing());
    assert!(
        game.drain_audio_commands()
            .any(|command| matches!(command, GameplayAudioCommand::PlayMusic { .. }))
    );
}

#[test]
fn practice_session_routes_playback_input_even_with_a_menu_open() {
    use crate::practice::editor::{PracticeEditorCommand, PracticePlayRange};
    use crate::practice::{PracticeSession, PracticeViewRequest};

    let mut game = practice_session_game(true);
    let mut session = PracticeSession::new(&mut game, Default::default());
    session.handle_input(
        &mut game,
        &test_input_event(VirtualAction::p1_start),
        practice_session_snap,
        |_, _| {},
    );
    assert!(session.menu().is_some());
    assert_eq!(
        session.execute(
            &mut game,
            PracticeEditorCommand::Play(PracticePlayRange::WholeSong),
            practice_session_snap,
            |_, _| {},
        ),
        PracticeViewRequest::PlaybackStarted
    );
    game.drain_audio_commands().for_each(drop);
    for pressed in [true, false] {
        assert_eq!(
            session.handle_input(
                &mut game,
                &test_input_event_with_source(
                    VirtualAction::p1_left,
                    pressed,
                    InputSource::Keyboard
                ),
                practice_session_snap,
                |_, _| {},
            ),
            PracticeViewRequest::None
        );
    }
    assert_eq!(game.pending_input.edges.len(), 2);
    game.set_current_music_time_ns(750_000_000);
    assert_eq!(
        session.handle_input(
            &mut game,
            &test_input_event(VirtualAction::p2_back),
            practice_session_snap,
            |_, _| {},
        ),
        PracticeViewRequest::PlaybackStopped
    );
    assert!(!session.is_playing());
    assert_eq!(session.editor().cursor_beat(), 1.5);
    assert_eq!(game.current_music_time_ns(), 750_000_000);
    assert!(game.pending_input_is_empty());
    assert_eq!(
        game.drain_audio_commands().collect::<Vec<_>>(),
        [GameplayAudioCommand::StopMusic]
    );
}

#[test]
fn practice_session_seeks_the_edit_clock_without_presentation() {
    use crate::practice::editor::PracticeEditorSnapshot;
    use crate::practice::{PracticePlaybackUpdate, PracticeSession};

    let mut game = practice_session_game(false);
    let mut session = PracticeSession::new(&mut game, Default::default());
    session.on_enter(&mut game);
    game.drain_audio_commands().for_each(drop);
    let start = game.current_music_time_ns();
    session.restore(
        &game,
        PracticeEditorSnapshot {
            cursor_beat: 4.0,
            ..Default::default()
        },
    );
    let target =
        song_time_ns_from_seconds(game.music_time_for_beat(session.editor().cursor_beat()));
    assert!(target > start);

    let advance = |session: &mut PracticeSession, game: &mut State| {
        assert_eq!(
            session.update(
                game,
                0.016,
                GameplayAudioSnapshot::default(),
                || panic!("the edit cursor drives the stage clock"),
                practice_session_no_music_snap,
                |_, _| {},
            ),
            PracticePlaybackUpdate::Editing
        );
    };
    advance(&mut session, &mut game);
    assert!(game.current_music_time_ns() > start && game.current_music_time_ns() < target);
    for _ in 0..240 {
        advance(&mut session, &mut game);
    }
    assert_eq!(game.current_music_time_ns(), target);
    assert_eq!(game.drain_audio_commands().count(), 0);
}

#[test]
fn practice_session_queues_rate_changes_without_a_presentation_consumer() {
    use crate::practice::editor::PracticeEditorFeedback;
    use crate::practice::{PracticePlaybackUpdate, PracticeSession};
    use deadsync_input::practice::{PracticeKey, PracticeKeyEvent};

    let mut game = practice_session_game(false);
    let mut session = PracticeSession::new(&mut game, Default::default());
    let feedback = |_: &State, event| {
        assert!(!matches!(
            event,
            PracticeEditorFeedback::PersistMusicRate(_)
        ));
    };
    for key in [PracticeKey::Shift, PracticeKey::RateIncrease] {
        assert!(
            session
                .handle_raw_key(
                    &mut game,
                    &PracticeKeyEvent {
                        key,
                        pressed: true,
                        repeat: false,
                        timestamp: Instant::now()
                    },
                    practice_session_no_music_snap,
                    feedback,
                )
                .0
        );
    }
    session.handle_input(
        &mut game,
        &test_input_event(VirtualAction::p1_start),
        practice_session_no_music_snap,
        feedback,
    );
    assert!(session.menu().is_some());
    let elapsed = game.boundary.total_elapsed_in_screen;
    assert_eq!(
        session.update(
            &mut game,
            1.0,
            GameplayAudioSnapshot::default(),
            || panic!("editing does not sample the gameplay clock"),
            practice_session_no_music_snap,
            feedback,
        ),
        PracticePlaybackUpdate::Editing
    );
    assert_eq!(game.boundary.total_elapsed_in_screen, elapsed + 1.0);
    let rates = session.drain_music_rates().collect::<Vec<_>>();
    assert!(rates.len() > 1);
    assert_eq!(rates.last().copied(), Some(game.music_rate()));
    assert!(rates.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        game.drain_audio_commands()
            .eq(rates.into_iter().map(GameplayAudioCommand::SetMusicRate))
    );
    assert_eq!(session.drain_music_rates().count(), 0);
}

#[test]
fn practice_session_initialization_entry_and_edit_restore_do_not_run_playback() {
    use crate::practice::{PracticePlayback, PracticePlaybackInput, PracticePlaybackUpdate};

    let mut game = practice_session_game(true);
    game.progress.stage.score_valid = [true; MAX_PLAYERS];
    game.progress.replay.mode = true;
    game.progress.replay.capture_enabled = true;
    let mut playback = PracticePlayback::new(&mut game);
    assert!(!playback.is_playing());
    assert_eq!(game.progress.stage.score_valid, [false; MAX_PLAYERS]);
    assert!(!game.progress.replay.mode);
    assert!(!game.progress.replay.capture_enabled);
    assert_eq!(game.drain_audio_commands().count(), 0);

    playback.on_enter(&mut game);
    assert_eq!(
        game.drain_audio_commands().collect::<Vec<_>>(),
        [GameplayAudioCommand::StopMusic]
    );
    let elapsed = game.boundary.total_elapsed_in_screen;
    let time = game.current_music_time_ns();
    assert!(matches!(
        playback.update(
            &mut game,
            0.25,
            practice_session_audio(10.0),
            || panic!("editing does not sample the gameplay clock"),
            practice_session_snap,
            || panic!("editing does not restart playback"),
        ),
        PracticePlaybackUpdate::Editing
    ));
    assert_eq!(game.boundary.total_elapsed_in_screen, elapsed);
    assert_eq!(game.current_music_time_ns(), time);
    assert!(matches!(
        playback.handle_input(
            &mut game,
            &test_input_event(VirtualAction::p1_left),
            || panic!("editing input belongs to the editor"),
        ),
        PracticePlaybackInput::Editing
    ));
    assert!(game.pending_input_is_empty());

    playback.start(&mut game, 2.0, 8.0, practice_session_snap);
    game.drain_audio_commands().for_each(drop);
    let time = game.current_music_time_ns();
    playback.restore_editing();
    assert!(!playback.is_playing());
    assert_eq!(game.current_music_time_ns(), time);
    assert_eq!(game.drain_audio_commands().count(), 0);
}

#[test]
fn practice_session_start_snaps_audio_but_keeps_the_requested_judgment_start() {
    use crate::practice::PracticePlayback;

    let mut game = practice_session_game(true);
    let mut playback = PracticePlayback::new(&mut game);
    playback.start(&mut game, 2.0, 8.0, practice_session_snap);

    assert!(playback.is_playing());
    assert_eq!(game.current_music_time_ns(), 125_000_000);
    assert_eq!(game.players_runtime.players[0].life_history, [(1.0, 0.5)]);
    assert_eq!(
        game.chart_runtime.mine_scan.next_tap_miss_cursor[0], 1,
        "the beat-1 note precedes the selected judgment range despite the audio lead-in"
    );
    assert_eq!(
        game.drain_audio_commands().collect::<Vec<_>>(),
        [GameplayAudioCommand::PlayMusic {
            path: PathBuf::from("songs/Tests/practice.ogg"),
            cut: GameplayMusicCut {
                start_sec: 0.125,
                ..Default::default()
            },
            looping: false,
            rate: 1.0,
        }]
    );

    let mut silent_game = practice_session_game(false);
    let mut silent = PracticePlayback::new(&mut silent_game);
    silent.start(&mut silent_game, 1.0, 8.0, practice_session_no_music_snap);
    assert!(silent.is_playing());
    assert_eq!(silent_game.current_music_time_ns(), -500_000_000);
    assert_eq!(
        silent_game.players_runtime.players[0].life_history,
        [(0.5, 0.5)]
    );
    assert_eq!(silent_game.drain_audio_commands().count(), 0);
}

#[test]
fn practice_session_loops_at_the_tail_boundary_and_resets_the_selected_range() {
    use crate::practice::{PracticePlayback, PracticePlaybackUpdate};

    let mut game = practice_session_game(true);
    let mut playback = PracticePlayback::new(&mut game);
    playback.start(&mut game, 1.0, 4.0, practice_session_snap);
    game.drain_audio_commands().for_each(drop);
    // Keep core completion separate from the selection's one-second tail.
    game.set_end_times(100_000_000_000, 100_000_000_000);
    // The real first-frame guard rejects jumps over one second. Sample the
    // snapped lead-in before moving the device clock beyond the selected note.
    assert!(matches!(
        playback.update(
            &mut game,
            0.001,
            practice_session_audio(-0.375),
            || 10_000_000_000,
            practice_session_snap,
            || panic!("the initial lead-in must not restart the practice range"),
        ),
        PracticePlaybackUpdate::Playing
    ));
    assert_eq!(game.current_music_time_ns(), -375_000_000);
    assert!(game.chart_runtime.notes[0].result.is_none());

    let mut restarts = 0;
    let before = playback.update(
        &mut game,
        0.01,
        practice_session_audio(2.999),
        || 10_000_000_000,
        practice_session_snap,
        || restarts += 1,
    );
    assert!(matches!(before, PracticePlaybackUpdate::Playing));
    assert_eq!(
        game.current_music_time_ns(),
        song_time_ns_from_seconds(2.999)
    );
    assert_eq!(restarts, 0);
    assert_eq!(game.drain_audio_commands().count(), 0);
    assert!(game.chart_runtime.notes[0].result.is_some());

    let at_boundary = playback.update(
        &mut game,
        0.001,
        practice_session_audio(3.0),
        || 10_000_000_000,
        practice_session_snap,
        || restarts += 1,
    );
    assert!(matches!(at_boundary, PracticePlaybackUpdate::Restarted));
    assert_eq!(restarts, 1);
    assert!(playback.is_playing());
    assert_eq!(game.current_music_time_ns(), -375_000_000);
    assert!(
        game.chart_runtime
            .notes
            .iter()
            .all(|note| note.result.is_none())
    );
    assert_eq!(game.players_runtime.players[0].life_history, [(0.5, 0.5)]);
    assert_eq!(game.drain_audio_commands().count(), 1);
}

#[test]
fn practice_session_core_exit_restarts_instead_of_navigating() {
    use crate::practice::{PracticePlayback, PracticePlaybackUpdate};

    let mut game = practice_session_game(true);
    let mut playback = PracticePlayback::new(&mut game);
    playback.start(&mut game, 2.0, 8.0, practice_session_snap);
    game.begin_exit_transition(ExitTransitionKind::Cancel);
    game.control
        .exit_input
        .exit_transition
        .as_mut()
        .unwrap()
        .started_at = Instant::now() - std::time::Duration::from_secs(10);
    game.drain_audio_commands().for_each(drop);
    let mut restarts = 0;

    assert!(matches!(
        playback.update(
            &mut game,
            0.01,
            practice_session_audio(0.25),
            || 10_000_000_000,
            practice_session_snap,
            || restarts += 1,
        ),
        PracticePlaybackUpdate::Restarted
    ));
    assert_eq!(restarts, 1);
    assert!(playback.is_playing());
    assert!(!game.exit_transition_active());
    assert_eq!(game.current_music_time_ns(), 125_000_000);
    assert_eq!(game.drain_audio_commands().count(), 1);
}

#[test]
fn practice_session_playing_input_preserves_arrow_edges_and_back_stops_immediately() {
    use crate::practice::{PracticePlayback, PracticePlaybackInput};

    let mut game = practice_session_game(true);
    let mut playback = PracticePlayback::new(&mut game);
    playback.start(&mut game, 1.0, 8.0, practice_session_snap);
    game.drain_audio_commands().for_each(drop);
    for pressed in [true, false] {
        assert!(matches!(
            playback.handle_input(
                &mut game,
                &test_input_event_with_source(
                    VirtualAction::p1_left,
                    pressed,
                    InputSource::Keyboard
                ),
                || panic!("arrow edges do not stop playback"),
            ),
            PracticePlaybackInput::Handled
        ));
    }
    assert_eq!(game.pending_input.edges.len(), 2);
    assert!(game.pending_input.edges[0].pressed);
    assert!(!game.pending_input.edges[1].pressed);
    assert!(
        game.pending_input
            .edges
            .iter()
            .all(|edge| !edge.record_replay)
    );
    assert!(matches!(
        playback.handle_input(
            &mut game,
            &test_input_event_with_source(VirtualAction::p2_back, false, InputSource::Keyboard),
            || panic!("a Back release does not stop playback"),
        ),
        PracticePlaybackInput::Handled
    ));
    assert!(playback.is_playing());
    assert_eq!(game.drain_audio_commands().count(), 0);

    game.set_current_music_time_ns(750_000_000);
    let mut stops = 0;
    let cursor =
        match playback.handle_input(&mut game, &test_input_event(VirtualAction::p2_back), || {
            stops += 1
        }) {
            PracticePlaybackInput::Stopped { cursor_beat } => cursor_beat,
            other => panic!("Back should stop practice immediately: {other:?}"),
        };
    assert_eq!(stops, 1);
    assert_eq!(cursor, 1.5);
    assert!(!playback.is_playing());
    assert!(game.pending_input_is_empty());
    assert_eq!(game.players_runtime.players[0].life_history, [(0.75, 0.5)]);
    assert_eq!(
        game.drain_audio_commands().collect::<Vec<_>>(),
        [GameplayAudioCommand::StopMusic]
    );

    playback.start(&mut game, 0.0, 8.0, practice_session_snap);
    game.drain_audio_commands().for_each(drop);
    assert_eq!(
        playback.stop(&mut game),
        0.0,
        "lead-in cannot move the editor before beat zero"
    );
    assert!(!playback.is_playing());
    assert_eq!(game.players_runtime.players[0].life_history, [(0.0, 0.5)]);
    assert_eq!(
        game.drain_audio_commands().collect::<Vec<_>>(),
        [GameplayAudioCommand::StopMusic]
    );
}
