use super::*;
use crate::views::VisualBackgroundView;
use deadsync_core::session_flow::EvaluationOrigin;
use deadsync_theme::evaluation::ThemeEvaluation;
use deadsync_theme::views::HeartRateView;
use std::path::{Path, PathBuf};

impl ThemeEvaluation for crate::SimplyLoveTheme {
    type EvaluationState = State;
    type EvaluationNoteskin = Arc<Noteskin>;
    type EvaluationContext = EvaluationContextView;
    type EvaluationRuntime = EvaluationRuntimeView;
    type EvaluationInput = InputEvent;
    type EvaluationRawKey = RawKeyboardEvent;
    type EvaluationRawPad = PadEvent;
    type EvaluationVisualPolicy = crate::views::SimplyLoveVisualPolicyView;
    type EvaluationSide = profile_data::PlayerSide;

    fn init_evaluation(
        stage: EvaluationStageView<Arc<Noteskin>, profile_data::PlayerSide>,
        context: EvaluationContextView,
        color_index: i32,
    ) -> State {
        let mut state = init(stage, context);
        state.active_color_index = color_index;
        state
    }

    fn init_course_evaluation(
        scores: [Option<ScoreInfo>; MAX_PLAYERS],
        routine_shared_score_percent: Option<f64>,
        duration_seconds: f32,
        context: EvaluationContextView,
        color_index: i32,
    ) -> State {
        let mut state = init_from_score_info(
            scores,
            routine_shared_score_percent,
            duration_seconds,
            context,
        );
        state.active_color_index = color_index;
        state.allow_online_panes = false;
        state
    }

    fn evaluation_color_index(state: &State) -> i32 {
        state.active_color_index
    }

    fn prepare_course_evaluation<'a>(
        active: &State,
        summary: &mut State,
        songs: impl Iterator<Item = &'a mut State>,
        gameplay_elapsed: f32,
    ) where
        Self::EvaluationState: 'a,
    {
        summary.screen_elapsed = active.screen_elapsed;
        summary.sfx_paths = active.sfx_paths.clone();
        sync_elapsed(summary, active.session_elapsed, gameplay_elapsed);
        for song in songs {
            song.screen_elapsed = active.screen_elapsed;
            song.sfx_paths = active.sfx_paths.clone();
            sync_elapsed(song, active.session_elapsed, gameplay_elapsed);
            for player in summary.score_info.iter_mut().flatten() {
                if player.noteskin.is_none() {
                    player.noteskin = song
                        .score_info
                        .iter()
                        .flatten()
                        .filter(|record| record.side == player.side)
                        .find_map(|record| record.noteskin.clone());
                }
            }
        }
    }

    fn resume_evaluation_page(active: &State, page: &mut State) {
        page.screen_elapsed = active.screen_elapsed;
        sync_elapsed(page, active.session_elapsed, active.gameplay_elapsed);
    }

    fn prepare_evaluation_audio(
        state: &mut State,
        failed: bool,
        visual_policy: Self::EvaluationVisualPolicy,
    ) -> (impl Iterator<Item = &Path>, Option<PathBuf>) {
        state.sfx_paths = SfxPaths::choose();
        let entry = entry_sfx_for_style(
            failed,
            visual_policy.srpg10_tint,
            visual_policy.background == VisualBackgroundView::Ecs15,
        );
        (state.sfx_paths.iter(), entry)
    }

    fn evaluation_elapsed_times(state: &State) -> (f32, f32) {
        elapsed_times(state)
    }

    fn sync_evaluation_elapsed(state: &mut State, session_elapsed: f32, gameplay_elapsed: f32) {
        sync_elapsed(state, session_elapsed, gameplay_elapsed);
    }

    fn update_evaluation(state: &mut State, dt: f32) -> ThemeEffect {
        update(state, dt)
    }

    fn handle_evaluation_input(
        state: &mut State,
        input: &InputEvent,
        origin: EvaluationOrigin,
    ) -> ThemeEffect {
        handle_session_input(state, input, origin)
    }

    fn handle_evaluation_raw_key(state: &mut State, key: &RawKeyboardEvent) {
        handle_raw_key_event(state, key);
    }

    fn handle_evaluation_raw_pad(state: &mut State, pad: &PadEvent) {
        handle_raw_pad_event(state, pad);
    }

    fn evaluation_test_input_pane_active(state: &State) -> bool {
        test_input_pane_active(state)
    }

    fn sync_evaluation_snek_sensors(
        state: &mut State,
        connected: bool,
        read_error: bool,
        sensors: [[[bool; 4]; 4]; MAX_PLAYERS],
        revision: u64,
    ) {
        set_snek_sensor_state(state, connected, read_error, sensors, revision);
    }

    fn push_evaluation_actors(
        actors: &mut Vec<Actor>,
        state: &State,
        assets: &AssetManager,
        visual_policy: Self::EvaluationVisualPolicy,
    ) {
        push_actors(actors, state, assets, visual_policy);
    }

    fn sync_evaluation_runtime(state: &mut State, view: EvaluationRuntimeView) {
        sync_runtime_view(state, view);
    }

    fn evaluation_heart_rate_generation(state: &State) -> Option<(bool, u64, u64)> {
        heart_rate_generation(state)
    }

    fn sync_evaluation_heart_rate(
        state: &mut State,
        generation: (bool, u64, u64),
        view: HeartRateView,
    ) {
        set_heart_rate_view(state, generation, view);
    }

    fn sync_evaluation_result_dialog(
        state: &mut State,
        side: Self::EvaluationSide,
        texture_keys: Box<[Arc<str>]>,
    ) {
        sync_arrowcloud_result_dialog(state, side, texture_keys);
    }

    fn evaluation_capture_ready(state: &State) -> bool {
        auto_screenshot_ready(state)
    }

    fn evaluation_leaderboard_requests(state: &State) -> ([bool; MAX_PLAYERS], bool) {
        (leaderboard_requests(state), state.allow_online_panes)
    }
}
