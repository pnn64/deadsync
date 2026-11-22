use super::*;
use deadsync_profile_gameplay::stage_options::StageOptions;
use deadsync_theme::player_options::{
    PlayerOptionsEntry, PlayerOptionsSelection, ThemePlayerOptions,
};

impl ThemePlayerOptions for crate::SimplyLoveTheme {
    type PlayerOptionsState = State;
    type PlayerOptionsView = PlayerOptionsView;
    type PlayerOptionsStage = StageOptions;
    type PlayerOptionsInput = InputEvent;
    type PlayerOptionsRawKey = deadlib_platform::input::RawKeyboardEvent;
    type PlayerOptionsVisualPolicy = crate::views::SimplyLoveVisualPolicyView;
    type PlayerOptionsNoteskin = Noteskin;

    fn init_player_options(entry: PlayerOptionsEntry<Screen, PlayerOptionsView>) -> State {
        let mut state = init_state(
            entry.song,
            entry.steps,
            entry.preferred,
            entry.color_index,
            entry.return_screen,
            entry
                .fixed_stepchart_label
                .map(|label| FixedStepchart { label }),
            entry.noteskin_catalog,
            entry.smx_gif_catalog,
            entry.heart_rate_devices,
            entry.view,
        );
        if let Some(scroll_speeds) = entry.scroll_speeds {
            state.speed_mod = scroll_speeds.map(SpeedMod::from);
            sync_speed_mod_type_rows(&mut state);
        }
        state
    }

    fn player_options_selection(state: &State) -> PlayerOptionsSelection<'_, Screen> {
        PlayerOptionsSelection {
            song: &state.song,
            steps: state.chart_steps_index,
            preferred: state.chart_difficulty_index,
            color_index: state.active_color_index,
            return_screen: state.return_screen,
            fixed_stepchart_label: state
                .fixed_stepchart
                .as_ref()
                .map(|chart| chart.label.as_str()),
            scroll_speeds: state.speed_mod.each_ref().map(scroll_speed_for_mod),
            music_rate: state.music_rate,
        }
    }

    fn player_options_stage(state: &State) -> StageOptions {
        stage_options(state)
    }

    fn player_options_song_mut(state: &mut State) -> &mut Arc<SongData> {
        &mut state.song
    }

    fn prepare_player_options(state: &mut State, assets: &AssetManager) {
        prepare_presentation(state, assets);
    }

    fn update_player_options(
        state: &mut State,
        dt: f32,
        assets: &AssetManager,
        effects: &mut Vec<ThemeEffect>,
    ) {
        update(state, dt, assets, effects);
    }

    fn handle_player_options_input(
        state: &mut State,
        assets: &AssetManager,
        input: &InputEvent,
        effects: &mut Vec<ThemeEffect>,
    ) {
        handle_input(state, assets, input, effects);
    }

    fn handle_player_options_raw_key(
        state: &mut State,
        key: Option<&Self::PlayerOptionsRawKey>,
        text: Option<&str>,
        ctrl_held: bool,
        effects: &mut Vec<ThemeEffect>,
    ) -> bool {
        handle_raw_key_event(state, key, text, ctrl_held, effects)
    }

    fn push_player_options_actors(
        actors: &mut Vec<Actor>,
        state: &State,
        assets: &AssetManager,
        visual_policy: Self::PlayerOptionsVisualPolicy,
    ) {
        push_actors(actors, state, assets, visual_policy);
    }

    fn sync_player_options_heart_rate(state: &mut State, devices: &HeartRateDevicesView) {
        set_heart_rate_devices(state, devices);
    }

    fn player_options_pad_light_preview(state: &State) -> [Option<u8>; PLAYER_SLOTS] {
        pad_light_brightness_preview(state)
    }

    fn take_player_options_noteskin_requests(
        state: &mut State,
        requests: &mut Vec<NoteskinPreviewRequest>,
    ) -> Option<usize> {
        Some(take_noteskin_preview_requests(state, requests))
    }

    fn retain_player_options_noteskin_previews(
        state: &mut State,
        requests: &[NoteskinPreviewRequest],
    ) {
        retain_noteskin_previews(state, requests);
    }

    fn set_player_options_noteskin_preview(
        state: &mut State,
        name: &str,
        parts: u16,
        skin: Option<Arc<Noteskin>>,
    ) {
        set_noteskin_preview(state, name, parts, skin);
    }

    fn player_options_noteskin_textures(skin: &Noteskin, parts: u16) -> Vec<(Arc<str>, bool)> {
        noteskin_preview_textures(skin, parts)
    }
}
