//! Presentation interface for a completed stage or course.

use crate::{ThemeEffect, ThemeTransitions};
use deadlib_assets::AssetManager;
use deadsync_core::{input::MAX_PLAYERS, session_flow::EvaluationOrigin};

pub trait ThemeEvaluation: ThemeTransitions {
    type EvaluationState: Clone;
    type EvaluationEntry;
    type EvaluationCourseScore;
    type EvaluationContext;
    type EvaluationRuntime;
    type EvaluationInput;
    type EvaluationRawKey;
    type EvaluationRawPad;
    type EvaluationVisualPolicy;

    fn init_evaluation(entry: Self::EvaluationEntry, color_index: i32) -> Self::EvaluationState;
    fn init_course_evaluation(
        scores: [Option<Self::EvaluationCourseScore>; MAX_PLAYERS],
        routine_shared_score_percent: Option<f64>,
        duration_seconds: f32,
        context: Self::EvaluationContext,
        color_index: i32,
    ) -> Self::EvaluationState;
    fn evaluation_color_index(state: &Self::EvaluationState) -> i32;
    fn evaluation_elapsed_times(state: &Self::EvaluationState) -> (f32, f32);
    fn sync_evaluation_elapsed(
        state: &mut Self::EvaluationState,
        session_elapsed: f32,
        gameplay_elapsed: f32,
    );
    fn update_evaluation(
        state: &mut Self::EvaluationState,
        dt: f32,
    ) -> ThemeEffect<Self::Screen, Self::RuntimeRequest>;
    fn handle_evaluation_input(
        state: &mut Self::EvaluationState,
        input: &Self::EvaluationInput,
        origin: EvaluationOrigin,
    ) -> ThemeEffect<Self::Screen, Self::RuntimeRequest>;
    fn handle_evaluation_raw_key(state: &mut Self::EvaluationState, key: &Self::EvaluationRawKey);
    fn handle_evaluation_raw_pad(state: &mut Self::EvaluationState, pad: &Self::EvaluationRawPad);
    fn evaluation_test_input_pane_active(state: &Self::EvaluationState) -> bool;
    fn sync_evaluation_snek_sensors(
        state: &mut Self::EvaluationState,
        connected: bool,
        read_error: bool,
        sensors: [[[bool; 4]; 4]; MAX_PLAYERS],
        revision: u64,
    );
    fn push_evaluation_actors(
        actors: &mut Vec<Self::Actor>,
        state: &Self::EvaluationState,
        assets: &AssetManager,
        visual_policy: Self::EvaluationVisualPolicy,
    );
    fn sync_evaluation_runtime(state: &mut Self::EvaluationState, view: Self::EvaluationRuntime);
    /// Returns the requested sides and whether online results can be shown on this page.
    fn evaluation_leaderboard_requests(
        state: &Self::EvaluationState,
    ) -> ([bool; MAX_PLAYERS], bool);
}
