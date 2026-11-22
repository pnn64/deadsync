use super::*;
use deadsync_core::session_flow::EvaluationOrigin;
use deadsync_theme::evaluation::ThemeEvaluation;

impl ThemeEvaluation for crate::SimplyLoveTheme {
    type EvaluationState = State;
    type EvaluationEntry = EvaluationInitView;
    type EvaluationCourseScore = ScoreInfo;
    type EvaluationContext = EvaluationContextView;
    type EvaluationRuntime = EvaluationRuntimeView;
    type EvaluationInput = InputEvent;
    type EvaluationRawKey = RawKeyboardEvent;
    type EvaluationRawPad = PadEvent;
    type EvaluationVisualPolicy = crate::views::SimplyLoveVisualPolicyView;

    fn init_evaluation(entry: EvaluationInitView, color_index: i32) -> State {
        let mut state = init(entry);
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

    fn evaluation_leaderboard_requests(state: &State) -> ([bool; MAX_PLAYERS], bool) {
        (leaderboard_requests(state), state.allow_online_panes)
    }
}
