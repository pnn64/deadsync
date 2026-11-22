use super::{
    App, Arc, CurrentScreen, MAX_PLAYERS, SongData, StageOptions, TransitionState,
    noteskin_catalog_view, player_options, profile, profile_data,
};
use deadsync_profile_gameplay::stage_charts::stage_difficulty_indices;
use deadsync_profile_gameplay::stage_replay::StageReplayRequest;

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

    fn from_player_options(state: &player_options::State) -> Self {
        Self {
            song: Arc::clone(&state.song),
            chart_difficulty_index: state.chart_difficulty_index,
            options: player_options::stage_options(state),
            color_index: state.active_color_index,
            return_screen: state.return_screen,
            fixed_stepchart_label: state
                .fixed_stepchart
                .as_ref()
                .map(|chart| chart.label.clone()),
            replay: None,
        }
    }
}

impl App {
    pub(super) fn queue_replay_stage(&mut self, replay: StageReplayRequest) -> bool {
        if self.state.screens.current_screen != CurrentScreen::SelectMusic
            || !matches!(self.state.shell.transition, TransitionState::Idle)
        {
            return false;
        }
        let Some(mut start) = self.selected_music_stage_start() else {
            return false;
        };
        start.replay = Some(replay);
        self.state.pending_stage = Some(start);
        true
    }

    pub(super) fn take_stage_start(
        &mut self,
    ) -> Option<(StageStart, Option<player_options::State>)> {
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
        screen: Option<player_options::State>,
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
            let mut state = player_options::init(
                start.song,
                chart_steps_index,
                start.chart_difficulty_index,
                start.color_index,
                start.return_screen,
                start
                    .fixed_stepchart_label
                    .map(|label| player_options::FixedStepchart { label }),
                noteskin_catalog_view(&self.dirs),
                crate::smx_config::smx_gif_catalog_view(),
                crate::heart_rate::devices_view(),
                view,
            );
            state.speed_mod = scroll_speeds.map(player_options::SpeedMod::from);
            player_options::sync_speed_mod_type_rows(&mut state);
            state
        });
        player_options::prepare_presentation(&mut screen, &self.asset_manager);
        self.commit_screen_change(CurrentScreen::PlayerOptions);
        self.state.screens.player_options_state = Some(screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::course_transition_tests::{finish_pending_stage, on_app_thread, test_app};
    use crate::app::select_music;
    use crate::gameplay_regression_tests::tests::with_session;
    use deadsync_core::input::InputSource;
    use deadsync_score::ReplayEdge;

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
                        state.replay_status_text.as_deref(),
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
