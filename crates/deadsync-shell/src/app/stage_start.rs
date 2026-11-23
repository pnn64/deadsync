use super::{
    ActiveTheme, App, Arc, CurrentScreen, MAX_PLAYERS, SongData, StageOptions, TransitionState,
    gameplay_play_style_from_profile, noteskin_catalog_view, profile, profile_data, select_course,
    select_music,
};
use crate::player_options::PlayerOptionsState;
use deadsync_gameplay::song_launch::{SongSelection, resolve_song_chart_slots};
use deadsync_gameplay::stage_launch::{StageLaunchRequest, StageLaunchTarget};
use deadsync_profile_gameplay::stage_charts::stage_difficulty_indices;
use deadsync_profile_gameplay::stage_replay::StageReplayRequest;
use deadsync_simfile::course_selection::ConfirmedCourse;
use deadsync_theme::ThemeScreenFlow;
use deadsync_theme::player_options::PlayerOptionsEntry;

pub(super) enum StageSelection {
    Song(SongSelection),
    Course(ConfirmedCourse),
}

pub(super) fn stage_launch_target(screen: CurrentScreen) -> Option<StageLaunchTarget> {
    if screen == ActiveTheme::SCREENS.gameplay {
        Some(StageLaunchTarget::Gameplay)
    } else if screen == ActiveTheme::SCREENS.player_options {
        Some(StageLaunchTarget::PlayerOptions)
    } else {
        None
    }
}

/// A one-shot request to enter Gameplay or Practice.
pub(super) struct StageStart {
    pub song: Arc<SongData>,
    pub chart_difficulty_index: [usize; MAX_PLAYERS],
    pub options: StageOptions,
    pub color_index: i32,
    pub return_screen: CurrentScreen,
    pub fixed_stepchart_label: Option<String>,
    pub replay: Option<StageReplayRequest>,
}

impl StageStart {
    pub(super) fn from_profiles(
        song: Arc<SongData>,
        steps: [usize; MAX_PLAYERS],
        preferred: [usize; MAX_PLAYERS],
        color_index: i32,
        return_screen: CurrentScreen,
        session: profile_data::SessionSnapshot,
        profiles: &[profile_data::Profile; MAX_PLAYERS],
    ) -> Self {
        Self {
            song,
            chart_difficulty_index: stage_difficulty_indices(steps, preferred),
            options: StageOptions::from_profiles(
                session.play_style,
                steps,
                session.music_rate,
                profiles,
            ),
            color_index,
            return_screen,
            fixed_stepchart_label: None,
            replay: None,
        }
    }

    pub(super) fn from_current_profiles(
        song: Arc<SongData>,
        steps: [usize; MAX_PLAYERS],
        preferred: [usize; MAX_PLAYERS],
        color_index: i32,
        return_screen: CurrentScreen,
    ) -> Self {
        Self::from_profiles(
            song,
            steps,
            preferred,
            color_index,
            return_screen,
            profile::get_session_snapshot(),
            &crate::player_options::active_profiles(),
        )
    }

    fn from_player_options(state: &PlayerOptionsState) -> Self {
        let selection = state.selection();
        let mut options = state.stage();
        let (steps, chart_difficulty_index) = resolve_song_chart_slots(
            selection.song,
            gameplay_play_style_from_profile(options.play_style),
            options.chart_steps_index,
            selection.preferred,
        );
        options.chart_steps_index = steps;
        Self {
            song: Arc::clone(selection.song),
            chart_difficulty_index,
            options,
            color_index: selection.color_index,
            return_screen: selection.return_screen,
            fixed_stepchart_label: selection.fixed_stepchart_label.map(str::to_owned),
            replay: None,
        }
    }
}

impl App {
    pub(super) fn request_stage_launch(
        &mut self,
        source: CurrentScreen,
        request: StageLaunchRequest<StageSelection>,
    ) {
        if self.state.screens.current_screen == source {
            self.state.screens.stage_launch.request(request);
            self.sync_stage_launch_prompt();
        }
    }

    fn sync_stage_launch_prompt(&mut self) {
        let screens = &mut self.state.screens;
        let prompt = screens.stage_launch.prompt();
        if screens.current_screen == ActiveTheme::SCREENS.song_selection {
            select_music::sync_song_launch_prompt(&mut screens.select_music_state, prompt);
        } else if screens.current_screen == ActiveTheme::SCREENS.course_selection {
            select_course::sync_stage_launch_prompt(&mut screens.select_course_state, prompt);
        }
    }

    pub(super) fn clear_stage_launch(&mut self) {
        self.state.screens.stage_launch = Default::default();
        self.sync_stage_launch_prompt();
    }

    pub(super) fn advance_stage_launch(&mut self, delta_time: f32) -> Option<CurrentScreen> {
        let current = self.state.screens.current_screen;
        let song_selection = current == ActiveTheme::SCREENS.song_selection;
        if (!song_selection && current != ActiveTheme::SCREENS.course_selection)
            || !matches!(self.state.shell.transition, TransitionState::Idle)
        {
            return None;
        }
        // A content reload can replace the wheel during the confirmation. Keep
        // the confirmed choice and wait for the active load to finish.
        let target = if song_selection && self.content_reload.is_busy() {
            None
        } else {
            self.state.screens.stage_launch.advance(delta_time)
        };
        self.sync_stage_launch_prompt();
        match target? {
            StageLaunchTarget::Gameplay => {
                if song_selection {
                    select_music::stop_song_launch_preview(
                        &mut self.state.screens.select_music_state,
                        &mut self.theme_effect_scratch,
                    );
                }
                Some(ActiveTheme::SCREENS.gameplay)
            }
            StageLaunchTarget::PlayerOptions => Some(ActiveTheme::SCREENS.player_options),
        }
    }

    pub(super) fn take_ready_song_selection(
        &mut self,
        target: StageLaunchTarget,
    ) -> Option<SongSelection> {
        let StageSelection::Song(selection) = self.state.screens.stage_launch.take_ready(target)?
        else {
            return None;
        };
        Some(selection)
    }

    pub(super) fn stage_start_for_song_selection(&self, selection: SongSelection) -> StageStart {
        let session = profile::get_session_snapshot();
        let (steps, preferred) =
            selection.resolved_chart_slots(gameplay_play_style_from_profile(session.play_style));
        StageStart::from_profiles(
            selection.song,
            steps,
            preferred,
            self.state.screens.select_music_state.active_color_index,
            ActiveTheme::SCREENS.song_selection,
            session,
            &crate::player_options::active_profiles(),
        )
    }

    pub(super) fn queue_replay_stage(&mut self, replay: StageReplayRequest) -> bool {
        if self.state.screens.current_screen != CurrentScreen::SelectMusic
            || !matches!(self.state.shell.transition, TransitionState::Idle)
        {
            return false;
        }
        let Some(selection) =
            select_music::selected_song_choice(&self.state.screens.select_music_state)
        else {
            return false;
        };
        self.clear_stage_launch();
        let mut start = self.stage_start_for_song_selection(selection);
        start.replay = Some(replay);
        self.state.pending_stage = Some(start);
        true
    }

    pub(super) fn take_stage_start(&mut self) -> Option<(StageStart, Option<PlayerOptionsState>)> {
        let screen = self.state.screens.player_options_state.take();
        if let Some(start) = self.state.pending_stage.take() {
            return Some((start, None));
        }
        let start = StageStart::from_player_options(screen.as_ref()?);
        Some((start, screen))
    }

    pub(super) fn recover_player_options(
        &mut self,
        start: StageStart,
        screen: Option<PlayerOptionsState>,
    ) {
        let mut screen = screen.unwrap_or_else(|| {
            let mut view = crate::player_options::init_view();
            view.play_style = start.options.play_style;
            view.music_rate = start.options.music_rate;
            let StageOptions {
                chart_steps_index,
                player_options,
                scroll_speeds,
                judgment_palette_ids,
                heart_rate_device_ids,
                ..
            } = start.options;
            for (((player, options), palette), monitor) in view
                .players
                .iter_mut()
                .zip(player_options)
                .zip(judgment_palette_ids)
                .zip(heart_rate_device_ids)
            {
                player.options = options;
                player.judgment_palette_id = palette;
                player.heart_rate_device_id = monitor;
            }
            let theme = self.state.screens.menu_state.theme_flag();
            PlayerOptionsState::new(theme, PlayerOptionsEntry {
                song: start.song,
                steps: chart_steps_index,
                preferred: start.chart_difficulty_index,
                color_index: start.color_index,
                return_screen: start.return_screen,
                fixed_stepchart_label: start.fixed_stepchart_label,
                noteskin_catalog: noteskin_catalog_view(&self.dirs),
                smx_gif_catalog: crate::smx_config::smx_gif_catalog_view(),
                heart_rate_devices: crate::heart_rate::devices_view(),
                view,
                scroll_speeds: Some(scroll_speeds),
            })
        });
        screen.prepare(&self.asset_manager);
        self.commit_screen_change(ActiveTheme::SCREENS.player_options);
        self.state.screens.player_options_state = Some(screen);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{SimplyLoveRuntimeRequest, ThemeEffect};
    use super::*;
    use crate::app::course_transition_tests::{finish_pending_stage, on_app_thread, test_app};
    use crate::gameplay_regression_tests::tests::with_session;
    use deadsync_core::input::InputSource;
    use deadsync_gameplay::stage_launch::{OPTIONS_ENTRY_SECONDS, OPTIONS_WINDOW_SECONDS};
    use deadsync_score::ReplayEdge;
    use deadsync_theme::AudioRequest;

    fn confirm_test_song(app: &mut App, root: &std::path::Path) -> Arc<SongData> {
        let path = root.join("confirmed.sm");
        std::fs::write(
            &path,
            "#TITLE:Confirmed song;#BPMS:0=120;\n\
             #NOTES:dance-single::Medium:4:0,0,0,0,0:\n0100\n0000\n0000\n0000\n;\n\
             #NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0000\n0000\n0000\n;",
        )
        .unwrap();
        let song =
            Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&path, 0.0).unwrap());
        app.state.screens.current_screen = CurrentScreen::SelectMusic;
        app.state.shell.transition = TransitionState::Idle;
        let wheel = &mut app.state.screens.select_music_state;
        wheel.entries = vec![select_music::MusicWheelEntry::Song(Arc::clone(&song))];
        wheel.selected_index = 0;
        wheel.selected_steps_index = 3;
        wheel.preferred_difficulty_index = 3;
        let mut effects = Vec::new();
        select_music::handle_confirm(wheel, &mut effects);
        let request = effects
            .into_iter()
            .find_map(|effect| match effect {
                ThemeEffect::Runtime(SimplyLoveRuntimeRequest::SongLaunch(request)) => {
                    Some(request)
                }
                _ => None,
            })
            .expect("confirming the wheel emits a song launch request");
        app.request_stage_launch(
            ActiveTheme::SCREENS.song_selection,
            request.map_selection(StageSelection::Song),
        );
        song
    }

    #[test]
    fn song_launch_uses_confirmed_song_and_chart_after_wheel_changes() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    let root = tempfile::tempdir().unwrap();
                    let mut app = test_app(root.path());
                    let song = confirm_test_song(&mut app, root.path());
                    let Some(StageSelection::Song(preload)) =
                        app.state.screens.stage_launch.preload_selection()
                    else {
                        panic!("song confirmation should supply its selection for preload");
                    };
                    assert!(Arc::ptr_eq(&preload.song, &song));
                    assert_eq!(preload.steps[0], 3);
                    app.state.screens.select_music_state.entries.clear();
                    app.state.screens.select_music_state.selected_steps_index = 2;

                    assert_eq!(app.advance_stage_launch(OPTIONS_WINDOW_SECONDS / 2.0), None);
                    assert_eq!(
                        app.advance_stage_launch(OPTIONS_WINDOW_SECONDS / 2.0),
                        Some(CurrentScreen::Gameplay)
                    );
                    assert!(matches!(
                        app.theme_effect_scratch.as_slice(),
                        [ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(
                            AudioRequest::StopMusic
                        ))]
                    ));
                    assert_eq!(app.advance_stage_launch(1.0), None);

                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Gameplay);
                    assert_eq!(target, CurrentScreen::Gameplay);
                    assert!(
                        !app.state
                            .screens
                            .stage_launch
                            .ready_for(StageLaunchTarget::Gameplay)
                    );
                    finish_pending_stage(&mut app);
                    let state = app.state.screens.gameplay_state.as_ref().unwrap();
                    assert!(Arc::ptr_eq(&state.song_arc(), &song));
                    let expected = song.chart_for_steps_index("dance-single", 3).unwrap();
                    assert_eq!(state.chart(0).unwrap().short_hash, expected.short_hash);
                },
            )
        });
    }

    #[test]
    fn versus_song_launch_uses_one_music_choice_in_gameplay_and_options() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Versus,
                profile_data::PlayerSide::P1,
                true,
                true,
                || {
                    let root = tempfile::tempdir().unwrap();
                    let mut app = test_app(root.path());
                    let mut song = confirm_test_song(&mut app, root.path());
                    app.clear_stage_launch();
                    for (index, chart) in Arc::make_mut(&mut song).charts.iter_mut().enumerate() {
                        chart.music_path = Some(root.path().join(format!("chart-{index}.ogg")));
                    }
                    let selection = SongSelection {
                        song,
                        steps: [3, 2],
                        preferred: [3, 2],
                    };
                    let start = app.stage_start_for_song_selection(selection.clone());
                    assert_eq!(start.options.chart_steps_index, [2, 2]);
                    assert_eq!(start.chart_difficulty_index, [2, 2]);

                    app.request_stage_launch(
                        ActiveTheme::SCREENS.song_selection,
                        StageLaunchRequest::Confirm(StageSelection::Song(selection)),
                    );
                    app.request_stage_launch(
                        ActiveTheme::SCREENS.song_selection,
                        StageLaunchRequest::PlayerOptions,
                    );
                    assert_eq!(
                        app.advance_stage_launch(OPTIONS_ENTRY_SECONDS),
                        Some(CurrentScreen::PlayerOptions),
                    );
                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::PlayerOptions);
                    assert_eq!(target, CurrentScreen::PlayerOptions);
                    let options = app.state.screens.player_options_state.as_mut().unwrap()
                        .simply_love_mut().expect("Simply Love chart edit fixture");
                    assert_eq!(options.chart_steps_index, [2, 2]);
                    assert_eq!(options.chart_difficulty_index, [2, 2]);

                    options.chart_steps_index = [3, usize::MAX];
                    options.chart_difficulty_index = [3, 2];
                    let (start, _) = app.take_stage_start().unwrap();
                    assert_eq!(start.options.chart_steps_index, [2, 2]);
                    assert_eq!(start.chart_difficulty_index, [2, 2]);
                },
            )
        });
    }

    #[test]
    fn song_launch_second_start_opens_options_for_confirmed_chart() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P2,
                false,
                true,
                || {
                    let root = tempfile::tempdir().unwrap();
                    let mut app = test_app(root.path());
                    let song = confirm_test_song(&mut app, root.path());
                    assert_eq!(
                        app.advance_stage_launch(OPTIONS_WINDOW_SECONDS - 0.25),
                        None
                    );
                    app.request_stage_launch(
                        ActiveTheme::SCREENS.song_selection,
                        StageLaunchRequest::PlayerOptions,
                    );
                    assert!(app.state.screens.stage_launch.preload_selection().is_none());
                    app.state.screens.select_music_state = select_music::init_placeholder();

                    assert_eq!(app.advance_stage_launch(OPTIONS_ENTRY_SECONDS / 2.0), None);
                    app.request_stage_launch(
                        ActiveTheme::SCREENS.song_selection,
                        StageLaunchRequest::PlayerOptions,
                    );
                    assert_eq!(
                        app.advance_stage_launch(OPTIONS_ENTRY_SECONDS / 2.0),
                        Some(CurrentScreen::PlayerOptions)
                    );
                    assert!(app.theme_effect_scratch.is_empty());

                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::PlayerOptions);
                    assert_eq!(target, CurrentScreen::PlayerOptions);
                    let options = app.state.screens.player_options_state.as_ref().unwrap().selection();
                    assert!(Arc::ptr_eq(options.song, &song));
                    assert_eq!(options.steps, [3; MAX_PLAYERS]);
                    assert_eq!(options.preferred, [3; MAX_PLAYERS]);
                    assert!(
                        !app.state
                            .screens
                            .stage_launch
                            .ready_for(StageLaunchTarget::PlayerOptions)
                    );
                },
            )
        });
    }

    #[test]
    fn both_player_options_mounts_restore_and_submit_the_same_stage_selection() {
        use deadsync_config::theme::ThemeFlag;
        use deadsync_rules::scroll::ScrollSpeedSetting;

        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P2,
                false,
                true,
                || {
                    for flag in [ThemeFlag::SimplyLove, ThemeFlag::OpenItg] {
                        let root = tempfile::tempdir().unwrap();
                        let mut app = test_app(root.path());
                        app.state.screens.menu_state = crate::title_menu::InstalledMenu::new(flag, 2);
                        let song = confirm_test_song(&mut app, root.path());
                        app.request_stage_launch(
                            ActiveTheme::SCREENS.song_selection,
                            StageLaunchRequest::PlayerOptions,
                        );
                        assert_eq!(
                            app.advance_stage_launch(OPTIONS_ENTRY_SECONDS),
                            Some(CurrentScreen::PlayerOptions),
                        );
                        assert_eq!(
                            app.prepare_global_fade_entry(CurrentScreen::PlayerOptions).0,
                            CurrentScreen::PlayerOptions,
                        );
                        let (mut start, old) = app.take_stage_start().unwrap();
                        assert_eq!(
                            matches!(old, Some(PlayerOptionsState::OpenItg { .. })),
                            flag == ThemeFlag::OpenItg,
                        );

                        let speeds = [ScrollSpeedSetting::CMod(605.0), ScrollSpeedSetting::MMod(775.0)];
                        start.options.scroll_speeds = speeds;
                        start.options.music_rate = 1.25;
                        app.recover_player_options(start, None);
                        let (resubmitted, _) = app.take_stage_start().unwrap();
                        assert!(Arc::ptr_eq(&resubmitted.song, &song));
                        assert_eq!(resubmitted.options.chart_steps_index, [3; MAX_PLAYERS]);
                        assert_eq!(resubmitted.options.scroll_speeds, speeds);
                        assert_eq!(resubmitted.options.music_rate, 1.25);
                        assert_eq!(resubmitted.return_screen, CurrentScreen::SelectMusic);
                    }
                },
            )
        });
    }

    #[test]
    fn replay_stage_uses_the_queued_song_and_recorded_offset() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    let root = tempfile::tempdir().unwrap();
                    let mut app = test_app(root.path());
                    let path = root.path().join("replay.sm");
                    std::fs::write(
                        &path,
                        "#TITLE:Replay test;#BPMS:0=120;\n\
                         #NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0000\n0000\n0000\n;",
                    )
                    .unwrap();
                    let song = Arc::new(
                        deadsync_simfile::app_runtime::parse_song_for_test(&path, 0.0).unwrap(),
                    );
                    app.state.screens.current_screen = CurrentScreen::SelectMusic;
                    app.state.shell.transition = TransitionState::Idle;
                    let wheel = &mut app.state.screens.select_music_state;
                    wheel.entries = vec![select_music::MusicWheelEntry::Song(Arc::clone(&song))];
                    wheel.selected_index = 0;
                    wheel.selected_steps_index = 3;

                    assert!(
                        app.queue_replay_stage(StageReplayRequest {
                            replay: [true, false]
                                .map(|pressed| ReplayEdge {
                                    event_music_time_ns: 1_250_000_000,
                                    lane_index: 0,
                                    pressed,
                                    source: InputSource::Keyboard,
                                })
                                .to_vec(),
                            name: "AAA".into(),
                            score: 9876.0,
                            replay_beat0_time_ns: 1_000_000_000,
                        })
                    );
                    app.state.screens.select_music_state.entries.clear();
                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Gameplay);
                    assert_eq!(target, CurrentScreen::Gameplay);
                    finish_pending_stage(&mut app);
                    assert!(app.state.pending_stage.is_none());

                    let state = app.state.screens.gameplay_state.as_mut().unwrap();
                    assert!(Arc::ptr_eq(&state.song_arc(), &song));
                    assert_eq!(
                        state.presentation.replay_status_text.as_deref(),
                        Some("Autoplay - AAA 98.76%")
                    );
                    assert!(state.progress.replay.mode);
                    let beat_zero = state
                        .timing_for_player(0)
                        .unwrap()
                        .get_time_for_beat_ns(0.0);
                    let event_time = beat_zero + 250_000_000;
                    let mut edges = [None; 2];
                    let input = &mut state.progress.replay.input;
                    assert_eq!(input.collect_ready(event_time - 1, 4, &mut edges), 0);
                    assert_eq!(input.collect_ready(event_time, 4, &mut edges), 2);
                    let [press, release] = edges.map(Option::unwrap);
                    assert!(press.pressed);
                    assert!(!release.pressed);
                    assert_eq!(press.event_music_time_ns, event_time);
                    assert_eq!(release.event_music_time_ns, event_time);
                },
            )
        });
    }
}
