use crate::game::parsing::notes::ParsedNote;
use crate::game::timing::{TimingData, TimingSegments};
use rssp::TechCounts;
use rssp::stats::ArrowStats;

/// Chart-level display BPM override parsed from `#DISPLAYBPM` inside a `#NOTEDATA` block.
#[derive(Clone, Debug)]
pub enum ChartDisplayBpm {
    /// A specific BPM or range specified via `#DISPLAYBPM` (min == max for a single value).
    Specified { min: f64, max: f64 },
    /// `#DISPLAYBPM:*` — show randomly cycling values (ITGmania shows animated random numbers).
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

#[derive(Clone, Debug)]
pub struct ChartData {
    pub chart_type: String,
    pub difficulty: String,
    pub description: String,
    pub chart_name: String,
    pub meter: u32,
    pub step_artist: String,
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
    pub has_significant_timing_changes: bool,
    pub possible_grade_points: i32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
    pub display_bpm: Option<ChartDisplayBpm>,
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
