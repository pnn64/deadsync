use rssp::TechCounts;
use rssp::stats::ArrowStats;

#[derive(Clone, Debug, PartialEq)]
pub struct ChartData {
    pub chart_type: String,
    pub difficulty: String,
    pub meter: u32,
    pub step_artist: String,
    pub notes: Vec<u8>, // This is the minimized raw data we will parse
    pub short_hash: String,
    pub stats: ArrowStats,
    pub tech_counts: TechCounts,
    /// Count of mines that are actually judgable in gameplay, excluding
    /// any mines that fall within fake or warp segments.
    pub mines_nonfake: u32,
    pub total_streams: u32,
    pub max_nps: f64,
    pub detailed_breakdown: String,
    pub partial_breakdown: String,
    pub simple_breakdown: String,
    pub total_measures: usize,
    pub measure_nps_vec: Vec<f64>,
    pub chart_bpms: Option<String>,
    pub chart_stops: Option<String>,
    pub chart_delays: Option<String>,
    pub chart_warps: Option<String>,
    pub chart_speeds: Option<String>,
    pub chart_scrolls: Option<String>,
    pub chart_fakes: Option<String>,
}
