use std::sync::Arc;

use deadsync_chart::{ChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_rules::timing::{
    ArrowTimingStats, HistogramMs, ScatterPoint, TimingStats, WindowCounts,
};

use crate::{Grade, GrooveStatsEvalState, ItlEvalState};

#[derive(Clone, Debug)]
pub struct StageSummary {
    pub song: Arc<SongData>,
    pub music_rate: f32,
    pub duration_seconds: f32,
    pub players: [Option<PlayerStageSummary>; MAX_PLAYERS],
}

#[derive(Clone, Debug)]
pub struct PlayerStageSummary {
    pub profile_name: String,
    pub chart: Arc<ChartData>,
    pub score_valid: bool,
    pub disqualified: bool,
    pub groovestats: GrooveStatsEvalState,
    pub itl: ItlEvalState,
    pub grade: Grade,
    pub score_percent: f64,
    pub earned_grade_points: i32,
    pub possible_grade_points: i32,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    pub hands_achieved: u32,
    pub hands_total: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_total: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    /// Total hit tapnotes this stage (counts jumps/hands as >1).
    pub notes_hit: u32,
    pub calories_burned: f32,
    pub window_counts: WindowCounts,
    pub window_counts_10ms: WindowCounts,
    pub timing: TimingStats,
    pub arrow_timing: ArrowTimingStats,
    pub scatter: Vec<ScatterPoint>,
    pub scatter_worst_window_ms: f32,
    pub histogram: HistogramMs,
    pub graph_first_second: f32,
    pub graph_last_second: f32,
    pub life_history: Vec<(f32, f32)>,
    pub fail_time: Option<f32>,
    pub show_w0: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub track_early_judgments: bool,
}
