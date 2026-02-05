use std::sync::Arc;

use crate::game::chart::ChartData;
use crate::game::gameplay::MAX_PLAYERS;
use crate::game::scores;
use crate::game::song::SongData;
use crate::game::timing::WindowCounts;

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
    pub grade: scores::Grade,
    pub score_percent: f64,
    pub ex_score_percent: f64,
    /// Total hit tapnotes this stage (counts jumps/hands as >1).
    pub notes_hit: u32,
    pub window_counts: WindowCounts,
    pub show_w0: bool,
    pub show_ex_score: bool,
}

