use crate::notes::ParsedNote;
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::path::PathBuf;

/// Chart-level display BPM override parsed from `#DISPLAYBPM` inside a `#NOTEDATA` block.
#[derive(Clone, Debug)]
pub enum ChartDisplayBpm {
    /// A specific BPM or range specified via `#DISPLAYBPM` (min == max for a single value).
    Specified { min: f64, max: f64 },
    /// `#DISPLAYBPM:*` - show randomly cycling values (ITGmania shows animated random numbers).
    Random,
}

#[derive(Clone, Debug, Default)]
pub struct StaminaCounts {
    pub anchors: u32,
    pub triangles: u32,
    pub boxes: u32,
    pub towers: u32,
    pub doritos: u32,
    pub hip_breakers: u32,
    pub copters: u32,
    pub spirals: u32,
    pub candles: u32,
    pub candle_percent: f64,
    pub staircases: u32,
    pub mono: u32,
    pub mono_percent: f64,
    pub sweeps: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArrowStats {
    pub total_arrows: u32,
    pub left: u32,
    pub down: u32,
    pub up: u32,
    pub right: u32,
    pub total_steps: u32,
    pub jumps: u32,
    pub hands: u32,
    pub mines: u32,
    pub holds: u32,
    pub rolls: u32,
    pub lifts: u32,
    pub fakes: u32,
    pub holding: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TechCounts {
    pub crossovers: u32,
    pub half_crossovers: u32,
    pub full_crossovers: u32,
    pub footswitches: u32,
    pub up_footswitches: u32,
    pub down_footswitches: u32,
    pub sideswitches: u32,
    pub jacks: u32,
    pub brackets: u32,
    pub doublesteps: u32,
}

#[derive(Clone, Debug)]
pub struct ChartData {
    pub chart_type: String,
    pub difficulty: String,
    pub description: String,
    pub chart_name: String,
    pub meter: u32,
    pub step_artist: String,
    /// Effective gameplay music path for this chart, after song-level fallback.
    pub music_path: Option<PathBuf>,
    pub short_hash: String,
    pub stats: ArrowStats,
    pub tech_counts: TechCounts,
    /// Count of mines that are actually judgable in gameplay, excluding
    /// any mines that fall within fake or warp segments.
    pub mines_nonfake: u32,
    pub stamina_counts: StaminaCounts,
    pub total_streams: u32,
    pub matrix_rating: f64,
    pub max_nps: f64,
    pub sn_detailed_breakdown: String,
    pub sn_partial_breakdown: String,
    pub sn_simple_breakdown: String,
    pub detailed_breakdown: String,
    pub partial_breakdown: String,
    pub simple_breakdown: String,
    pub total_measures: usize,
    pub measure_nps_vec: Vec<f64>,
    pub measure_seconds_vec: Vec<f32>,
    pub first_second: f32,
    pub has_note_data: bool,
    pub has_chart_attacks: bool,
    pub possible_grade_points: i32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
    pub display_bpm: Option<ChartDisplayBpm>,
    pub min_bpm: f64,
    pub max_bpm: f64,
}

#[derive(Clone, Debug)]
pub struct GameplayChartData {
    pub notes: Vec<u8>, // This is the minimized raw data we will parse
    pub parsed_notes: Vec<ParsedNote>,
    pub row_to_beat: Vec<f32>,
    pub timing_segments: TimingSegments,
    pub timing: TimingData,
    pub chart_attacks: Option<String>,
}
