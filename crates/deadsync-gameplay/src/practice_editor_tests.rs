// Included inside runtime_regression_tests to exercise the editor against the
// real gameplay state without a theme, application, or filesystem fixture.
mod practice_editor_tests {
    use super::*;
    use crate::practice::editor::{
        PracticeEditor as Editor, PracticeEditorCommand as Command, PracticeEditorConfig as Config,
        PracticeEditorFeedback as Feedback, PracticeEditorFlash as Flash,
        PracticeEditorInputResult as InputResult, PracticeEditorRequest as Request,
        PracticeEditorSnapshot as Snapshot, PracticeEditorSound as Sound,
        PracticePlayRange as PlayRange, max_play_beat,
    };
    use deadsync_input::practice::{PracticeKey, PracticeKeyEvent};

    fn fixture() -> State {
        let mut game = regression_state_with_segments(
            TimingSegments {
                bpms: vec![(0.0, 120.0)],
                ..Default::default()
            },
            384,
        );
        game.apply_global_offset_delta(-game.global_offset_seconds());
        game.set_live_autoplay_enabled(false);
        Arc::make_mut(&mut game.source.song).music_length_seconds = 600.0;
        game.drain_audio_commands().for_each(drop);
        game
    }

    fn snapshot(cursor_beat: f32, snap_index: usize) -> Snapshot {
        Snapshot {
            cursor_beat,
            selection_anchor: None,
            selection_end: None,
            snap_index,
        }
    }

    fn key(code: PracticeKey, pressed: bool, repeat: bool) -> PracticeKeyEvent {
        PracticeKeyEvent {
            key: code,
            pressed,
            repeat,
            timestamp: Instant::now(),
        }
    }

    fn raw(
        editor: &mut Editor,
        game: &mut State,
        code: PracticeKey,
        pressed: bool,
        repeat: bool,
    ) -> (InputResult, Vec<Feedback>) {
        let mut feedback = Vec::new();
        let result = editor.handle_raw_key(
            game,
            &key(code, pressed, repeat),
            false,
            false,
            |_, event| {
                feedback.push(event);
            },
        );
        (result, feedback)
    }

    fn input(
        editor: &mut Editor,
        game: &State,
        action: VirtualAction,
        pressed: bool,
    ) -> (Request, Vec<Feedback>) {
        let mut feedback = Vec::new();
        let request = editor.handle_input(
            game,
            &test_input_event_with_source(action, pressed, InputSource::Keyboard),
            false,
            |_, event| feedback.push(event),
        );
        (request, feedback)
    }

    fn navigation(editor: &mut Editor, game: &State, dt: f32, menu_open: bool) -> Vec<Feedback> {
        let mut feedback = Vec::new();
        editor.update_navigation_holds(game, dt, menu_open, |_, event| feedback.push(event));
        feedback
    }

    fn rate_update(editor: &mut Editor, game: &mut State, dt: f32) -> Vec<Feedback> {
        let mut feedback = Vec::new();
        editor.update_music_rate_hold(game, dt, |_, event| feedback.push(event));
        feedback
    }

    fn execute(editor: &mut Editor, game: &State, command: Command) -> (Request, Vec<Feedback>) {
        let mut feedback = Vec::new();
        let request = editor.execute(game, command, |_, event| feedback.push(event));
        (request, feedback)
    }

    #[test]
    fn navigation_modes_route_cursor_snap_and_menu_actions() {
        let game = fixture();
        for (dedicated, three_key, action, cursor, snap) in [
            (false, false, VirtualAction::p1_down, 11.0, 0),
            (false, true, VirtualAction::p2_menu_down, 11.0, 0),
            (false, false, VirtualAction::p2_right, 10.0, 1),
            (false, false, VirtualAction::p2_menu_right, 10.0, 0),
            (true, false, VirtualAction::p1_up, 10.0, 0),
            (true, false, VirtualAction::p2_menu_down, 11.0, 0),
            (true, false, VirtualAction::p2_menu_right, 10.0, 1),
            (true, true, VirtualAction::p2_menu_left, 9.0, 0),
            (true, true, VirtualAction::p1_right, 10.0, 0),
        ] {
            let mut editor = Editor::new(Config {
                only_dedicated_menu_buttons: dedicated,
                three_key_navigation: three_key,
                ..Default::default()
            });
            editor.set_cursor(&game, 10.0);
            assert_eq!(input(&mut editor, &game, action, true).0, Request::None);
            assert_eq!(
                editor.cursor_beat(),
                cursor,
                "{dedicated}/{three_key}: {action:?}"
            );
            assert_eq!(
                editor.snap_index(),
                snap,
                "{dedicated}/{three_key}: {action:?}"
            );
        }

        let mut editor = Editor::new(Config {
            only_dedicated_menu_buttons: true,
            three_key_navigation: true,
            ..Default::default()
        });
        let before = editor.snapshot();
        assert_eq!(
            editor.handle_input(
                &game,
                &test_input_event(VirtualAction::p2_menu_left),
                true,
                |_, _| {}
            ),
            Request::StepMenu(-1)
        );
        assert_eq!(
            editor.snapshot(),
            before,
            "menu navigation must not move the edit cursor"
        );
    }

    #[test]
    fn cursor_holds_count_sources_and_resume_the_opposite_direction_after_a_fresh_delay() {
        let game = fixture();
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 20.0);
        input(&mut editor, &game, VirtualAction::p1_down, true);
        input(&mut editor, &game, VirtualAction::p2_down, true);
        assert_eq!(editor.cursor_beat(), 22.0);
        input(&mut editor, &game, VirtualAction::p1_down, false);
        assert!(navigation(&mut editor, &game, 0.25, false).is_empty());
        assert_eq!(
            navigation(&mut editor, &game, 0.125, false),
            [Feedback::Sound(Sound::Line)]
        );
        assert_eq!(
            editor.cursor_beat(),
            23.0,
            "the second Down input is still held"
        );

        input(&mut editor, &game, VirtualAction::p1_up, true);
        assert_eq!(editor.cursor_beat(), 22.0);
        navigation(&mut editor, &game, 0.375, false);
        assert_eq!(editor.cursor_beat(), 21.0, "last pressed direction wins");
        input(&mut editor, &game, VirtualAction::p1_up, false);
        assert!(navigation(&mut editor, &game, 0.25, false).is_empty());
        navigation(&mut editor, &game, 0.125, false);
        assert_eq!(
            editor.cursor_beat(),
            22.0,
            "releasing Up resumes the remaining Down hold"
        );
        input(&mut editor, &game, VirtualAction::p2_down, false);
        assert!(navigation(&mut editor, &game, 1.0, false).is_empty());
        assert_eq!(editor.cursor_beat(), 22.0);
    }

    #[test]
    fn page_inputs_ignore_repeat_and_keep_other_sources_held() {
        let mut game = fixture();
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 20.0);
        raw(&mut editor, &mut game, PracticeKey::PageDown, true, false);
        raw(&mut editor, &mut game, PracticeKey::PageDown, true, false);
        assert_eq!(editor.cursor_beat(), 28.0);
        assert!(
            raw(&mut editor, &mut game, PracticeKey::PageDown, true, true)
                .1
                .is_empty()
        );
        assert_eq!(editor.cursor_beat(), 28.0);
        raw(&mut editor, &mut game, PracticeKey::PageDown, false, false);
        navigation(&mut editor, &game, 0.375, false);
        assert_eq!(editor.cursor_beat(), 32.0);

        raw(&mut editor, &mut game, PracticeKey::PageUp, true, false);
        assert_eq!(editor.cursor_beat(), 28.0);
        raw(&mut editor, &mut game, PracticeKey::PageUp, false, false);
        assert!(navigation(&mut editor, &game, 0.25, false).is_empty());
        navigation(&mut editor, &game, 0.125, false);
        assert_eq!(editor.cursor_beat(), 32.0);
        raw(&mut editor, &mut game, PracticeKey::PageDown, false, false);
        assert!(navigation(&mut editor, &game, 1.0, false).is_empty());
    }

    #[test]
    fn navigation_repeats_obey_control_menu_tab_and_per_frame_bounds() {
        let mut game = fixture();
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 20.0);
        input(&mut editor, &game, VirtualAction::p1_down, true);
        raw(&mut editor, &mut game, PracticeKey::Control, true, false);
        assert!(navigation(&mut editor, &game, 1.0, false).is_empty());
        assert_eq!(editor.cursor_beat(), 21.0);
        raw(&mut editor, &mut game, PracticeKey::PageDown, true, false);
        navigation(&mut editor, &game, 0.375, false);
        assert_eq!(
            editor.cursor_beat(),
            29.0,
            "Control blocks cursor repeats, not page repeats"
        );
        editor.clear_navigation_holds();
        raw(&mut editor, &mut game, PracticeKey::Control, false, false);
        input(&mut editor, &game, VirtualAction::p1_down, true);
        assert!(navigation(&mut editor, &game, 1.0, true).is_empty());
        assert!(navigation(&mut editor, &game, 0.25, false).is_empty());
        navigation(&mut editor, &game, 0.125, false);
        assert_eq!(
            editor.cursor_beat(),
            31.0,
            "opening a menu pauses the repeat timer"
        );

        for enabled in [false, true] {
            let mut editor = Editor::new(Config {
                tab_acceleration: enabled,
                ..Default::default()
            });
            input(&mut editor, &game, VirtualAction::p1_down, true);
            raw(&mut editor, &mut game, PracticeKey::FastScroll, true, false);
            navigation(&mut editor, &game, 0.09375, false);
            assert_eq!(editor.cursor_beat(), if enabled { 2.0 } else { 1.0 });
        }

        let mut editor = Editor::new(Config::default());
        input(&mut editor, &game, VirtualAction::p1_down, true);
        assert!(navigation(&mut editor, &game, 0.0, false).is_empty());
        assert!(navigation(&mut editor, &game, -1.0, false).is_empty());
        let feedback = navigation(&mut editor, &game, 10.0, false);
        assert_eq!(
            feedback.len(),
            65,
            "one delay-edge movement plus at most 64 repeats"
        );
        assert!(
            feedback
                .iter()
                .all(|event| *event == Feedback::Sound(Sound::Line))
        );
        assert_eq!(editor.cursor_beat(), 66.0);
    }

    #[test]
    fn snap_changes_quantize_the_cursor_and_shift_selection_returns_to_its_anchor() {
        let mut game = fixture();
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 1.2);
        assert_eq!(
            input(&mut editor, &game, VirtualAction::p1_right, true).1,
            [Feedback::Sound(Sound::Snap)]
        );
        assert_eq!(editor.snap_step(), 0.5);
        assert_eq!(editor.cursor_beat(), 1.0);
        input(&mut editor, &game, VirtualAction::p1_left, true);
        assert!(
            input(&mut editor, &game, VirtualAction::p1_left, true)
                .1
                .is_empty()
        );
        assert_eq!(editor.snap_index(), 0);

        editor.set_cursor(&game, 4.0);
        raw(&mut editor, &mut game, PracticeKey::Shift, true, false);
        input(&mut editor, &game, VirtualAction::p1_down, true);
        input(&mut editor, &game, VirtualAction::p1_down, false);
        assert_eq!(editor.selection_range(), Some((4.0, 5.0)));
        input(&mut editor, &game, VirtualAction::p1_up, true);
        input(&mut editor, &game, VirtualAction::p1_up, false);
        assert_eq!(editor.selection_range(), None);
        input(&mut editor, &game, VirtualAction::p1_down, true);
        input(&mut editor, &game, VirtualAction::p1_down, false);
        raw(&mut editor, &mut game, PracticeKey::Shift, false, false);
        input(&mut editor, &game, VirtualAction::p1_down, true);
        assert_eq!(
            editor.selection_range(),
            Some((4.0, 5.0)),
            "unshifted movement preserves markers"
        );
        editor.clear_navigation_holds();
        editor.set_cursor(&game, 0.0);
        assert!(
            input(&mut editor, &game, VirtualAction::p1_up, true)
                .1
                .is_empty()
        );
        assert_eq!(editor.cursor_beat(), 0.0);
    }

    #[test]
    fn area_markers_cycle_and_invalid_explicit_endpoints_preserve_the_range() {
        let game = fixture();
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 8.0);
        assert_eq!(
            execute(&mut editor, &game, Command::PlaceAreaMarker).1,
            [
                Feedback::Sound(Sound::Marker),
                Feedback::Flash(Flash::AreaMarkerStartSet)
            ]
        );
        editor.set_cursor(&game, 4.0);
        execute(&mut editor, &game, Command::PlaceAreaMarker);
        assert_eq!(editor.selection_range(), Some((4.0, 8.0)));
        execute(&mut editor, &game, Command::PlaceAreaMarker);
        assert_eq!(editor.selection_anchor(), Some(4.0));
        assert_eq!(editor.selection_end(), None);
        assert_eq!(
            execute(&mut editor, &game, Command::PlaceAreaMarker).1,
            [
                Feedback::Sound(Sound::Marker),
                Feedback::Flash(Flash::SelectionCleared)
            ]
        );
        assert_eq!(editor.selection_range(), None);

        editor.restore(
            &game,
            Snapshot {
                selection_anchor: Some(4.0),
                selection_end: Some(8.0),
                ..snapshot(8.0, 0)
            },
        );
        assert_eq!(
            execute(&mut editor, &game, Command::SetSelectionStart).1,
            [
                Feedback::Flash(Flash::InvalidSelectionStart),
                Feedback::Sound(Sound::Invalid)
            ]
        );
        assert_eq!(editor.selection_range(), Some((4.0, 8.0)));
        editor.set_cursor(&game, 4.0);
        assert_eq!(
            execute(&mut editor, &game, Command::SetSelectionEnd).1,
            [
                Feedback::Flash(Flash::InvalidSelectionEnd),
                Feedback::Sound(Sound::Invalid)
            ]
        );
        assert_eq!(editor.selection_range(), Some((4.0, 8.0)));
        execute(&mut editor, &game, Command::ClearSelection);
        assert_eq!(editor.selection_anchor(), None);
        assert_eq!(editor.selection_end(), None);
    }

    #[test]
    fn playback_ranges_use_markers_cursor_and_a_minimum_snap_without_losing_the_song_bound() {
        let game = fixture();
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 10.0);
        assert_eq!(
            editor.playback_range(&game, PlayRange::WholeSong),
            (0.0, 1200.0)
        );
        assert_eq!(
            editor.playback_range(&game, PlayRange::CurrentToEnd),
            (10.0, 1200.0)
        );
        assert_eq!(
            editor.playback_range(&game, PlayRange::Selection),
            (10.0, 1200.0)
        );
        editor.restore(
            &game,
            Snapshot {
                selection_anchor: Some(8.0),
                ..snapshot(10.0, 0)
            },
        );
        assert_eq!(
            editor.playback_range(&game, PlayRange::Selection),
            (8.0, 1200.0)
        );
        editor.restore(
            &game,
            Snapshot {
                selection_anchor: Some(8.0),
                selection_end: Some(12.0),
                ..snapshot(10.0, 0)
            },
        );
        assert_eq!(
            execute(&mut editor, &game, Command::Play(PlayRange::Selection)).0,
            Request::StartPlayback {
                start_beat: 8.0,
                stop_beat: 12.0
            }
        );
        editor.restore(&game, snapshot(5000.0, 0));
        assert_eq!(editor.cursor_beat(), 1200.0);
        assert_eq!(
            editor.playback_range(&game, PlayRange::Selection),
            (1200.0, 1201.0)
        );
    }

    #[test]
    fn note_seek_skips_nonjudgable_and_nearby_notes_and_bounds_include_hold_tails() {
        let mut game = fixture();
        Arc::make_mut(&mut game.source.song).music_length_seconds = 3.0;
        let mut unjudgable = test_note(1, 96, NoteType::Tap);
        unjudgable.can_be_judged = false;
        let mut same_row = test_note(3, 48, NoteType::Tap);
        same_row.beat = 1.00005;
        game.chart_runtime.notes = vec![
            test_note(0, 48, NoteType::Tap),
            same_row,
            unjudgable,
            test_hold(2, 192, 384),
        ];
        assert_eq!(
            max_play_beat(&game),
            8.0,
            "the hold tail extends past the six-beat music"
        );
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 1.0);
        assert!(
            raw(&mut editor, &mut game, PracticeKey::NextNote, true, false)
                .0
                .consumed
        );
        assert_eq!(editor.cursor_beat(), 4.0);
        assert_eq!(
            raw(&mut editor, &mut game, PracticeKey::NextNote, true, false).1,
            [Feedback::Sound(Sound::Invalid)]
        );
        assert_eq!(editor.cursor_beat(), 4.0);
        raw(
            &mut editor,
            &mut game,
            PracticeKey::PreviousNote,
            true,
            false,
        );
        assert_eq!(editor.cursor_beat(), 1.00005);
        assert_eq!(
            raw(
                &mut editor,
                &mut game,
                PracticeKey::PreviousNote,
                true,
                false
            )
            .1,
            [Feedback::Sound(Sound::Invalid)]
        );
        raw(&mut editor, &mut game, PracticeKey::SongEnd, true, false);
        assert_eq!(editor.cursor_beat(), 8.0);
        Arc::make_mut(&mut game.source.song).music_length_seconds = 5.0;
        raw(&mut editor, &mut game, PracticeKey::SongEnd, true, false);
        assert_eq!(
            editor.cursor_beat(),
            10.0,
            "music can extend beyond the last hold"
        );
        raw(&mut editor, &mut game, PracticeKey::SongStart, true, false);
        assert_eq!(editor.cursor_beat(), 0.0);
    }

    #[test]
    fn rate_feedback_observes_core_change_then_persistence_audio_text_flash_and_sound() {
        let mut game = fixture();
        game.set_music_rate(0.93);
        let mut editor = Editor::new(Config::default());
        raw(&mut editor, &mut game, PracticeKey::Shift, true, false);
        let mut observed = Vec::new();
        let result = editor.handle_raw_key(
            &mut game,
            &key(PracticeKey::RateIncrease, true, false),
            false,
            false,
            |game, feedback| {
                observed.push((
                    feedback,
                    game.music_rate(),
                    game.boundary.commands.audio.len(),
                ))
            },
        );
        assert!(result.consumed);
        assert_eq!(result.request, Request::None);
        assert_eq!(observed.len(), 4);
        assert!(
            matches!(observed[0].0, Feedback::PersistMusicRate(rate) if (rate - 0.94).abs() < 1e-5)
        );
        assert_eq!(observed[1].0, Feedback::SyncMusicRateText);
        assert!(
            matches!(observed[2].0, Feedback::Flash(Flash::MusicRate { rate }) if (rate - 0.94).abs() < 1e-5)
        );
        assert_eq!(observed[3].0, Feedback::Sound(Sound::Line));
        assert_eq!(
            observed
                .iter()
                .map(|(_, _, count)| *count)
                .collect::<Vec<_>>(),
            [0, 1, 1, 1]
        );
        assert!(
            observed
                .iter()
                .all(|(_, rate, _)| (*rate - 0.94).abs() < 1e-5)
        );
        let audio = game.drain_audio_commands().collect::<Vec<_>>();
        assert!(
            matches!(audio.as_slice(), [GameplayAudioCommand::SetMusicRate(rate)] if (*rate - 0.94).abs() < 1e-5)
        );

        assert!(
            raw(
                &mut editor,
                &mut game,
                PracticeKey::RateIncrease,
                true,
                true
            )
            .1
            .is_empty()
        );
        raw(&mut editor, &mut game, PracticeKey::FastScroll, true, false);
        assert!(
            rate_update(&mut editor, &mut game, 0.09375).is_empty(),
            "Tab must not accelerate rate repeats"
        );
        let repeated = rate_update(&mut editor, &mut game, 0.28125);
        assert!(
            matches!(repeated.first(), Some(Feedback::PersistMusicRate(rate)) if (*rate - 0.95).abs() < 1e-5)
        );
        assert!((game.music_rate() - 0.95).abs() < 1e-5);
    }

    #[test]
    fn rate_bounds_emit_one_limit_feedback_and_no_persistence_or_audio_until_a_new_press() {
        for (rate, code) in [
            (0.5, PracticeKey::RateDecrease),
            (3.0, PracticeKey::RateIncrease),
        ] {
            let mut game = fixture();
            game.set_music_rate(rate);
            let mut editor = Editor::new(Config::default());
            raw(&mut editor, &mut game, PracticeKey::Shift, true, false);
            let expected = [
                Feedback::Sound(Sound::Invalid),
                Feedback::Flash(Flash::MusicRateLimit { rate }),
            ];
            assert_eq!(raw(&mut editor, &mut game, code, true, false).1, expected);
            assert!(rate_update(&mut editor, &mut game, 10.0).is_empty());
            assert!(
                raw(&mut editor, &mut game, code, true, false).1.is_empty(),
                "duplicate press does not retrigger a held key"
            );
            assert_eq!(game.music_rate(), rate);
            assert_eq!(game.drain_audio_commands().count(), 0);
            raw(&mut editor, &mut game, code, false, false);
            assert_eq!(raw(&mut editor, &mut game, code, true, false).1, expected);
        }
    }

    #[test]
    fn rate_holds_resume_the_opposite_at_a_bound_and_shift_release_clears_both() {
        let mut game = fixture();
        game.set_music_rate(2.99);
        let mut editor = Editor::new(Config::default());
        raw(&mut editor, &mut game, PracticeKey::Shift, true, false);
        raw(
            &mut editor,
            &mut game,
            PracticeKey::RateDecrease,
            true,
            false,
        );
        raw(
            &mut editor,
            &mut game,
            PracticeKey::RateIncrease,
            true,
            false,
        );
        rate_update(&mut editor, &mut game, 0.375);
        assert!((game.music_rate() - 3.0).abs() < 1e-5);
        assert_eq!(
            rate_update(&mut editor, &mut game, 0.05).len(),
            2,
            "reaching the bound emits the limit once"
        );
        assert!(rate_update(&mut editor, &mut game, 0.25).is_empty());
        rate_update(&mut editor, &mut game, 0.125);
        assert!(
            (game.music_rate() - 2.99).abs() < 1e-5,
            "the still-held lower key resumes after a fresh delay"
        );
        raw(&mut editor, &mut game, PracticeKey::Shift, false, false);
        assert!(rate_update(&mut editor, &mut game, 1.0).is_empty());
        let released = raw(
            &mut editor,
            &mut game,
            PracticeKey::RateDecrease,
            false,
            false,
        );
        assert!(
            released.0.consumed,
            "key release is consumed even after Shift is released"
        );
        assert!(released.1.is_empty());

        let mut game = fixture();
        let mut editor = Editor::new(Config::default());
        raw(&mut editor, &mut game, PracticeKey::Shift, true, false);
        raw(
            &mut editor,
            &mut game,
            PracticeKey::RateIncrease,
            true,
            false,
        );
        game.drain_audio_commands().for_each(drop);
        let feedback = rate_update(&mut editor, &mut game, 10.0);
        assert_eq!(
            feedback
                .iter()
                .filter(|event| matches!(event, Feedback::PersistMusicRate(_)))
                .count(),
            65
        );
        assert_eq!(game.drain_audio_commands().count(), 65);
        assert!((game.music_rate() - 1.66).abs() < 1e-5);
    }

    #[test]
    fn restore_clamps_snapshot_and_clears_navigation_rate_and_modifier_state() {
        let mut game = fixture();
        Arc::make_mut(&mut game.source.song).music_length_seconds = 20.0;
        let mut editor = Editor::new(Config::default());
        editor.set_cursor(&game, 20.0);
        input(&mut editor, &game, VirtualAction::p1_down, true);
        raw(&mut editor, &mut game, PracticeKey::PageDown, true, false);
        for code in [
            PracticeKey::Shift,
            PracticeKey::Control,
            PracticeKey::FastScroll,
        ] {
            raw(&mut editor, &mut game, code, true, false);
        }
        raw(
            &mut editor,
            &mut game,
            PracticeKey::RateIncrease,
            true,
            false,
        );
        game.drain_audio_commands().for_each(drop);
        let rate = game.music_rate();
        editor.restore(
            &game,
            Snapshot {
                cursor_beat: 9000.0,
                selection_anchor: Some(64.0),
                selection_end: Some(32.0),
                snap_index: usize::MAX,
            },
        );
        assert_eq!(
            editor.snapshot(),
            Snapshot {
                cursor_beat: 40.0,
                selection_anchor: Some(32.0),
                selection_end: Some(40.0),
                snap_index: 8,
            }
        );
        editor.set_cursor(&game, 20.0);
        assert!(navigation(&mut editor, &game, 1.0, false).is_empty());
        assert!(rate_update(&mut editor, &mut game, 1.0).is_empty());
        assert_eq!(game.music_rate(), rate);
        assert_eq!(game.drain_audio_commands().count(), 0);
        assert!(
            !raw(
                &mut editor,
                &mut game,
                PracticeKey::RateIncrease,
                true,
                false
            )
            .0
            .consumed
        );
        input(&mut editor, &game, VirtualAction::p1_down, true);
        let after_press = editor.cursor_beat();
        assert!(
            navigation(&mut editor, &game, 0.09375, false).is_empty(),
            "Tab state was cleared"
        );
        assert_eq!(
            navigation(&mut editor, &game, 0.28125, false),
            [Feedback::Sound(Sound::Line)],
            "Control state was cleared"
        );
        assert!(editor.cursor_beat() > after_press);
        assert_eq!(
            editor.selection_range(),
            Some((32.0, 40.0)),
            "Shift state was cleared"
        );

        for (anchor, end, expected_anchor, expected_end) in [
            (Some(96.0), Some(128.0), None, None),
            (Some(f32::NAN), None, Some(0.0), None),
            (None, Some(f32::INFINITY), None, Some(0.0)),
        ] {
            editor.restore(
                &game,
                Snapshot {
                    selection_anchor: anchor,
                    selection_end: end,
                    ..snapshot(4.0, 0)
                },
            );
            assert_eq!(editor.selection_anchor(), expected_anchor);
            assert_eq!(editor.selection_end(), expected_end);
        }
    }

    #[test]
    fn gameplay_function_keys_apply_core_modes_and_release_real_offset_holds() {
        let mut game = fixture();
        let mut editor = Editor::new(Config::default());
        assert_eq!(game.autosync_mode(), AutosyncMode::Off);
        assert_eq!(
            raw(&mut editor, &mut game, PracticeKey::Autosync, true, false).1,
            [Feedback::Flash(Flash::Autosync(AutosyncMode::Song))]
        );
        assert_eq!(game.autosync_mode(), AutosyncMode::Song);
        assert!(
            raw(&mut editor, &mut game, PracticeKey::Autosync, true, true)
                .1
                .is_empty()
        );
        assert!(
            raw(&mut editor, &mut game, PracticeKey::Autosync, false, false)
                .1
                .is_empty()
        );
        assert_eq!(game.autosync_mode(), AutosyncMode::Song);

        assert_eq!(game.tick_mode(), GameplayTimingTickMode::Off);
        assert_eq!(
            raw(&mut editor, &mut game, PracticeKey::TimingTick, true, false).1,
            [Feedback::Flash(Flash::TimingTick(
                GameplayTimingTickMode::Assist
            ))]
        );
        assert_eq!(game.tick_mode(), GameplayTimingTickMode::Assist);
        assert!(
            raw(&mut editor, &mut game, PracticeKey::TimingTick, true, true)
                .1
                .is_empty()
        );
        assert!(
            raw(
                &mut editor,
                &mut game,
                PracticeKey::TimingTick,
                false,
                false
            )
            .1
            .is_empty()
        );
        assert_eq!(game.tick_mode(), GameplayTimingTickMode::Assist);

        for (code, direction) in [
            (PracticeKey::OffsetDecrease, -1.0),
            (PracticeKey::OffsetIncrease, 1.0),
        ] {
            let before = game.song_offset_seconds();
            let pressed = raw(&mut editor, &mut game, code, true, false);
            let after = game.song_offset_seconds();
            assert!(pressed.0.consumed);
            assert!((after - before) * direction > 0.0);
            assert_eq!(
                pressed.1,
                [Feedback::Flash(Flash::Offset {
                    global: false,
                    seconds: after
                })]
            );
            assert!(game.offset_adjust_hold_active());
            assert!(raw(&mut editor, &mut game, code, true, true).1.is_empty());
            assert_eq!(game.song_offset_seconds(), after);
            let released = raw(&mut editor, &mut game, code, false, false);
            assert!(released.0.consumed);
            assert!(released.1.is_empty());
            assert!(!game.offset_adjust_hold_active());
            game.update_offset_adjust_hold(Instant::now() + std::time::Duration::from_secs(5));
            assert_eq!(
                game.song_offset_seconds(),
                after,
                "released offset keys cannot repeat later"
            );
        }
    }

    #[test]
    fn raw_global_hotkeys_precede_menu_and_playback_routes_without_os_repeat() {
        for (playing, menu_open) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut game = fixture();
            let mut editor = Editor::new(Config::default());
            let mut feedback = Vec::new();
            let result = editor.handle_raw_key(
                &mut game,
                &key(PracticeKey::Autoplay, true, false),
                playing,
                menu_open,
                |_, event| feedback.push(event),
            );
            assert!(result.consumed);
            assert_eq!(result.request, Request::None);
            assert!(game.autoplay_enabled());
            assert_eq!(
                feedback,
                [Feedback::Flash(Flash::Autoplay { enabled: true })]
            );
            feedback.clear();
            editor.handle_raw_key(
                &mut game,
                &key(PracticeKey::Autoplay, true, true),
                playing,
                menu_open,
                |_, event| feedback.push(event),
            );
            assert!(game.autoplay_enabled());
            assert!(feedback.is_empty());
            editor.handle_raw_key(
                &mut game,
                &key(PracticeKey::Shift, true, false),
                playing,
                menu_open,
                |_, _| {},
            );
            editor.handle_raw_key(
                &mut game,
                &key(PracticeKey::RateIncrease, true, false),
                playing,
                menu_open,
                |_, event| feedback.push(event),
            );
            assert!((game.music_rate() - 1.01).abs() < 1e-5);
            assert!(matches!(
                feedback.first(),
                Some(Feedback::PersistMusicRate(_))
            ));
            assert!(!rate_update(&mut editor, &mut game, 0.375).is_empty());
            let request = editor
                .handle_raw_key(
                    &mut game,
                    &key(PracticeKey::Cancel, true, false),
                    playing,
                    menu_open,
                    |_, _| {},
                )
                .request;
            assert_eq!(
                request,
                if playing {
                    Request::StopPlayback
                } else if menu_open {
                    Request::CloseMenu
                } else {
                    Request::OpenMainMenu
                }
            );
        }
    }
}
