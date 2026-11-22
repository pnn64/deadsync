use super::{
    App, Arc, CurrentScreen, MAX_PLAYERS, SongData, StageOptions, noteskin_catalog_view,
    player_options, profile, profile_data,
};
use deadsync_profile_gameplay::stage_charts::stage_difficulty_indices;

/// A one-shot request to enter Gameplay or Practice.
pub(super) struct StageStart {
    pub song: Arc<SongData>,
    pub chart_difficulty_index: [usize; MAX_PLAYERS],
    pub options: StageOptions,
    pub color_index: i32,
    pub return_screen: CurrentScreen,
    pub fixed_stepchart_label: Option<String>,
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
        }
    }
}

impl App {
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
