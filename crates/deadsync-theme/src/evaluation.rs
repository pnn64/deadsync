//! Presentation interface for a completed stage or course.

use crate::{ThemeEffect, ThemeTransitions, UiCue};
use crate::views::{
    EvaluationOverview, EvaluationPlayerOverview, EvaluationScores, EvaluationStageView,
    HeartRateView,
};
use deadlib_assets::AssetManager;
use deadsync_core::{input::MAX_PLAYERS, session_flow::EvaluationOrigin};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct EvaluationCaptureView {
    pub elapsed: f32,
    pub overview: bool,
}

/// A retained result page prepared from the shared score records at screen entry.
#[derive(Clone)]
pub struct EvaluationOverviewPage<N, S> {
    scores: EvaluationScores<N, S>,
    titles: [Arc<str>; 2],
    artist: Arc<str>,
    banner_key: Option<Arc<str>>,
    sides_p2: [bool; MAX_PLAYERS],
    routine_percent: Option<f64>,
    routine_grade: Option<deadsync_score::Grade>,
    stage_number: Option<usize>,
    course_summary: bool,
    double_side_style: bool,
}

impl<N, S> EvaluationOverviewPage<N, S> {
    pub fn new(
        scores: &EvaluationScores<N, S>,
        routine_percent: Option<f64>,
        stage_number: Option<usize>,
        double_side_style: bool,
        is_p2: impl Fn(&S) -> bool,
    ) -> Option<Self> {
        let score = scores.iter().flatten().next()?;
        let course_summary = score.is_course_summary();
        let titles = if course_summary && score.song.title.is_empty() {
            let empty: Arc<str> = "".into();
            [Arc::clone(&empty), empty]
        } else {
            let title: Arc<str> = score.song.display_full_title(false).into();
            let translated = score.song.display_full_title(true);
            let translated = if translated == title.as_ref() {
                Arc::clone(&title)
            } else {
                translated.into()
            };
            [title, translated]
        };
        let routine_grade = routine_percent.map(|percent| {
            let failed = scores
                .iter()
                .flatten()
                .any(|score| score.grade == deadsync_score::Grade::Failed);
            deadsync_score::stage_stats::routine_result_grade(percent, failed)
        });
        Some(Self {
            scores: Arc::clone(scores),
            titles,
            artist: score.song.artist.as_str().into(),
            banner_key: score.song.banner_path.as_deref().map(deadlib_assets::media_path_key),
            sides_p2: scores.each_ref().map(|score| {
                score.as_ref().is_some_and(|score| is_p2(&score.side))
            }),
            routine_percent,
            routine_grade,
            stage_number,
            course_summary,
            double_side_style,
        })
    }

    /// Borrows the score records and the prepared artwork and text keys.
    pub fn view(
        &self,
        assets: &AssetManager,
        transliterated: bool,
        elapsed: f32,
        departure_elapsed: Option<f32>,
    ) -> EvaluationOverview<'_> {
        EvaluationOverview {
            title: &self.titles[usize::from(transliterated)],
            artist: &self.artist,
            banner_key: self.banner_key.as_ref().filter(|key| assets.has_texture_key(key)),
            course_summary: self.course_summary,
            stage_number: self.stage_number,
            double_side_style: self.double_side_style,
            elapsed,
            departure_elapsed,
            players: std::array::from_fn(|slot| {
                self.scores[slot].as_ref().map(|score| EvaluationPlayerOverview {
                    p2: self.sides_p2[slot],
                    grade: self.routine_grade.unwrap_or(score.grade),
                    percent: self.routine_percent.unwrap_or(score.score_percent) * 100.0,
                    numeric_score: score.numeric_score,
                    individual_percent: self.routine_percent.map(|_| score.score_percent * 100.0),
                    ex_percent: score.show_ex_score.then_some(score.ex_score_percent.max(0.0)),
                    hard_ex_percent: score
                        .show_hard_ex_score
                        .then_some(score.hard_ex_score_percent.max(0.0)),
                    difficulty: score.chart.difficulty.as_str(),
                    meter: score.chart.meter,
                    mods: &score.mods_text,
                    judgments: score.judgment_counts,
                    row_window_counts: score.window_counts,
                    score_valid: score.score_valid,
                    dropped_hold_or_roll: score.dropped_hold_or_roll,
                    jumps: score.jumps,
                    holds: [score.holds_held, score.holds_total],
                    mines: [score.mines_avoided, score.mines_total],
                    hands: [score.hands_achieved, score.hands_total],
                    rolls: [score.rolls_held, score.rolls_total],
                    max_combo: score.max_combo,
                    combo_runs: &score.combo_runs,
                    graph_seconds: [score.graph_first_second, score.graph_last_second],
                    course_graph_stages: &score.course_graph_stages,
                    steps_life: &score.steps_life,
                    machine_rank: score.machine_record_highlight_rank,
                    personal_rank: score.personal_record_highlight_rank,
                    disqualified: score.disqualified,
                })
            }),
        }
    }
}

pub trait ThemeEvaluation: ThemeTransitions {
    type EvaluationState: Clone;
    type EvaluationNoteskin;
    type EvaluationContext;
    type EvaluationRuntime;
    type EvaluationInput;
    type EvaluationRawKey;
    type EvaluationRawPad;
    type EvaluationVisualPolicy;
    type EvaluationSide;

    fn init_evaluation(
        stage: EvaluationStageView<Self::EvaluationNoteskin, Self::EvaluationSide>,
        context: Self::EvaluationContext,
        color_index: i32,
    ) -> Self::EvaluationState;
    fn init_course_evaluation(
        scores: EvaluationScores<Self::EvaluationNoteskin, Self::EvaluationSide>,
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
    ) -> ThemeEffect<Self::Screen, Self::RuntimeRequest>;
    /// Cue for the theme's standard result departure.
    fn evaluation_departure_cue(_origin: EvaluationOrigin) -> Option<UiCue> {
        None
    }
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
    /// Existing service overlays usable over a mounted overview.
    fn push_evaluation_overlays(
        _actors: &mut Vec<Self::Actor>,
        _state: &Self::EvaluationState,
        _assets: &AssetManager,
    ) {}
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
    fn evaluation_capture_view(state: &Self::EvaluationState) -> EvaluationCaptureView;
    fn evaluation_capture_ready(state: &Self::EvaluationState) -> bool;
    /// Returns the requested sides and whether online results can be shown on this page.
    fn evaluation_leaderboard_requests(
        state: &Self::EvaluationState,
    ) -> ([bool; MAX_PLAYERS], bool);
}
