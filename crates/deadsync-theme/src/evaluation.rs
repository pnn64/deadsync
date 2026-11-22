//! Presentation interface for a completed stage or course.

use crate::{ThemeEffect, ThemeTransitions};
use crate::views::HeartRateView;
use deadlib_assets::AssetManager;
use deadsync_core::{input::MAX_PLAYERS, session_flow::EvaluationOrigin};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    type EvaluationSide;

    fn init_evaluation(entry: Self::EvaluationEntry, color_index: i32) -> Self::EvaluationState;
    fn init_course_evaluation(
        scores: [Option<Self::EvaluationCourseScore>; MAX_PLAYERS],
        routine_shared_score_percent: Option<f64>,
        duration_seconds: f32,
        context: Self::EvaluationContext,
        color_index: i32,
    ) -> Self::EvaluationState;
    /// Installs the visit's shared animation, audio, and artwork on course pages.
    fn prepare_course_evaluation<'a>(
        active: &Self::EvaluationState,
        summary: &mut Self::EvaluationState,
        songs: impl Iterator<Item = &'a mut Self::EvaluationState>,
        gameplay_elapsed: f32,
    ) where
        Self::EvaluationState: 'a;
    /// Continues the active visit when the host changes the result page.
    fn resume_evaluation_page(active: &Self::EvaluationState, page: &mut Self::EvaluationState);
    /// Selects visit audio and returns deferred cues to preload and an entry cue to play.
    fn prepare_evaluation_audio(
        state: &mut Self::EvaluationState,
        failed: bool,
        visual_policy: Self::EvaluationVisualPolicy,
    ) -> (impl Iterator<Item = &Path>, Option<PathBuf>);
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
    fn evaluation_heart_rate_generation(state: &Self::EvaluationState) -> Option<(bool, u64, u64)>;
    fn sync_evaluation_heart_rate(
        state: &mut Self::EvaluationState,
        generation: (bool, u64, u64),
        view: HeartRateView,
    );
    fn sync_evaluation_result_dialog(
        state: &mut Self::EvaluationState,
        side: Self::EvaluationSide,
        texture_keys: Box<[Arc<str>]>,
    );
    /// Whether the result can be captured with its presentation settled.
    fn evaluation_capture_ready(state: &Self::EvaluationState) -> bool;
    /// Returns the requested sides and whether online results can be shown on this page.
    fn evaluation_leaderboard_requests(
        state: &Self::EvaluationState,
    ) -> ([bool; MAX_PLAYERS], bool);
}
