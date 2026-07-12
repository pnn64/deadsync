//! Plain data prepared for concrete theme screens.

use deadsync_chart::{ChartData, SongData};
use deadsync_rules::judgment::{self, JudgeGrade};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{
    ArrowTimingStats, HistogramMs, ScatterPoint, TimingStats, WindowCounts,
};
use deadsync_score::{
    ColumnJudgments, Grade, GrooveStatsEvalState, ItlEvalState, LeaderboardEntry,
};
use std::path::PathBuf;
use std::sync::Arc;

/// Shell-prepared audio clock values used by theme preview presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioPlaybackView {
    pub music_stream_position_seconds: f64,
}

/// One shell-discovered audio output exposed without backend runtime types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioOutputDeviceView {
    pub name: String,
    pub is_default: bool,
    pub sample_rates_hz: Vec<u32>,
}

/// Startup audio choices prepared by the shell for a theme options screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioOptionsView {
    pub output_devices: Vec<AudioOutputDeviceView>,
    pub available_backend_names: Vec<String>,
}

/// One resolved chart in a course selection.
#[derive(Clone, Debug)]
pub struct CourseStageView {
    pub song: Arc<SongData>,
    pub chart_hash: String,
}

/// A selected course prepared by a theme for runtime startup.
#[derive(Clone, Debug)]
pub struct SelectedCourseView {
    pub path: PathBuf,
    pub name: String,
    pub banner_path: Option<PathBuf>,
    pub score_hash: String,
    pub song_stub: Arc<SongData>,
    pub course_difficulty_name: String,
    pub course_meter: Option<u32>,
    pub course_stepchart_label: String,
    pub stages: Vec<CourseStageView>,
}

/// Chart source and duration used by a course-summary density graph.
#[derive(Clone, Debug)]
pub struct CourseGraphStageView {
    pub chart: Arc<ChartData>,
    pub song_last_second: f32,
}

/// Final score snapshot consumed by evaluation screens and course summaries.
///
/// The concrete noteskin handle and player-side identity remain generic so
/// this contract does not depend on an asset manager or profile runtime.
#[derive(Clone)]
pub struct EvaluationView<N, S> {
    pub song: Arc<SongData>,
    pub chart: Arc<ChartData>,
    pub course_graph_stages: Vec<CourseGraphStageView>,
    pub side: S,
    pub profile_name: String,
    pub score_valid: bool,
    pub disqualified: bool,
    pub expected_groovestats_submit: bool,
    pub expected_arrowcloud_submit: bool,
    pub groovestats: GrooveStatsEvalState,
    pub itl: ItlEvalState,
    pub judgment_counts: judgment::JudgeCounts,
    pub score_percent: f64,
    pub earned_grade_points: i32,
    pub possible_grade_points: i32,
    pub grade: Grade,
    pub speed_mod: ScrollSpeedSetting,
    pub mods_text: Arc<str>,
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
    pub timing: TimingStats,
    pub arrow_timing: ArrowTimingStats,
    pub scatter: Vec<ScatterPoint>,
    pub scatter_worst_window_ms: f32,
    pub histogram: HistogramMs,
    pub graph_first_second: f32,
    pub graph_last_second: f32,
    pub music_rate: f32,
    pub life_history: Vec<(f32, f32)>,
    pub fail_time: Option<f32>,
    pub window_counts: WindowCounts,
    pub window_counts_10ms: WindowCounts,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    pub calories_burned: f32,
    pub column_judgments: Vec<ColumnJudgments>,
    pub noteskin: Option<N>,
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub track_early_judgments: bool,
    pub disabled_timing_windows: [bool; 5],
    pub machine_records: Vec<LeaderboardEntry>,
    pub machine_record_highlight_rank: Option<u32>,
    pub personal_records: Vec<LeaderboardEntry>,
    pub personal_record_highlight_rank: Option<u32>,
    pub show_machine_personal_split: bool,
}

impl<N, S> EvaluationView<N, S> {
    #[inline(always)]
    pub fn judgment_count(&self, grade: JudgeGrade) -> u32 {
        self.judgment_counts[judgment::judge_grade_ix(grade)]
    }

    #[inline(always)]
    pub fn is_course_summary(&self) -> bool {
        !self.course_graph_stages.is_empty()
    }
}

/// Density data requested by a theme for a chart preview.
#[derive(Clone, Debug)]
pub struct DensityGraphView {
    pub max_nps: f64,
    pub measure_nps_vec: Vec<f64>,
    pub measure_seconds_vec: Vec<f32>,
    pub first_second: f32,
    pub last_second: f32,
}

/// One profile row displayed by a theme's pad-configuration UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadProfileEntry {
    pub name: String,
    pub is_default: bool,
    pub is_active: bool,
}

/// Presentation-facing timing health prepared by the process shell.
#[derive(Clone, Copy, Debug)]
pub struct TimingHealthView<P, C, A> {
    pub interval_ns: u64,
    pub display_error_ms: f32,
    pub display_catching_up: bool,
    pub present_mode: P,
    pub display_clock: C,
    pub host_clock: C,
    pub in_flight_images: u8,
    pub waited_for_image: bool,
    pub applied_back_pressure: bool,
    pub queue_idle_waited: bool,
    pub suboptimal: bool,
    pub submitted_present_id: u32,
    pub completed_present_id: u32,
    pub calibration_error_ns: u64,
    pub host_mapped: bool,
    pub audio: Option<A>,
}

/// Which screen corner or center seam a frame-statistics overlay uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
}

impl OverlayAnchor {
    #[inline(always)]
    pub const fn to_key(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
            Self::TopCenter => "top-center",
            Self::BottomCenter => "bottom-center",
        }
    }

    #[inline(always)]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "top-left" => Some(Self::TopLeft),
            "top-right" => Some(Self::TopRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-right" => Some(Self::BottomRight),
            "top-center" => Some(Self::TopCenter),
            "bottom-center" => Some(Self::BottomCenter),
            _ => None,
        }
    }
}

/// Presentation detail shown by a frame-statistics overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayStyle {
    Detailed,
    Minimal,
}

impl OverlayStyle {
    #[inline(always)]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Detailed => Self::Minimal,
            Self::Minimal => Self::Detailed,
        }
    }

    #[inline(always)]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Detailed => "detailed",
            Self::Minimal => "minimal",
        }
    }

    #[inline(always)]
    pub fn from_key(key: &str) -> Self {
        match key.trim().to_ascii_lowercase().as_str() {
            "minimal" => Self::Minimal,
            _ => Self::Detailed,
        }
    }

    #[inline(always)]
    pub const fn show_p99(self) -> bool {
        matches!(self, Self::Detailed)
    }

    #[inline(always)]
    pub const fn show_histogram(self) -> bool {
        matches!(self, Self::Detailed)
    }
}

/// One captured frame's per-phase timing plus sync state.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSample {
    pub host_nanos: u64,
    pub frame_us: u32,
    pub input_us: u32,
    pub update_us: u32,
    pub compose_us: u32,
    pub upload_us: u32,
    pub draw_us: u32,
    pub gpu_wait_us: u32,
    pub display_error_us: i32,
    pub catching_up: bool,
}

impl FrameStatsSample {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            host_nanos: 0,
            frame_us: 0,
            input_us: 0,
            update_us: 0,
            compose_us: 0,
            upload_us: 0,
            draw_us: 0,
            gpu_wait_us: 0,
            display_error_us: 0,
            catching_up: false,
        }
    }

    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.host_nanos == 0
    }

    #[inline(always)]
    pub const fn measured_us(&self) -> u32 {
        self.input_us
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
            .saturating_add(self.gpu_wait_us)
    }

    #[inline(always)]
    pub const fn idle_us(&self) -> u32 {
        self.frame_us.saturating_sub(self.measured_us())
    }

    #[inline(always)]
    pub const fn cpu_work_us(&self) -> u32 {
        self.input_us
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
    }
}

/// Precomputed sync-health readouts supplied to an overlay renderer.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSummary {
    pub avg_frame_us: u32,
    pub p99_frame_us: u32,
    pub max_frame_us: u32,
    pub fps: f32,
    pub display_error_ms: f32,
    pub display_error_p99_ms: f32,
    pub display_catching_up: bool,
    pub in_gameplay: bool,
    pub audio_callback_gap_ms: f32,
    pub audio_underruns: u64,
    pub audio_output_delay_ms: f32,
    pub audio_queued_frames: u32,
    pub frame_jitter_us: u32,
    pub display_error_jitter_us: u32,
    pub spike_hold_us: u32,
    pub target_frame_us: u32,
    pub cpu_work_us: u32,
    pub gpu_wait_us: u32,
    pub over_budget_count: u32,
    pub catch_up_count: u32,
}

#[cfg(test)]
mod tests {
    use super::{
        AudioOptionsView, AudioOutputDeviceView, FrameStatsSample, OverlayAnchor, OverlayStyle,
    };

    #[test]
    fn audio_options_view_preserves_plain_device_data() {
        let view = AudioOptionsView {
            output_devices: vec![AudioOutputDeviceView {
                name: "Primary".to_owned(),
                is_default: true,
                sample_rates_hz: vec![44_100, 48_000],
            }],
            available_backend_names: vec!["Auto".to_owned(), "ALSA".to_owned()],
        };

        assert_eq!(view.output_devices[0].name, "Primary");
        assert_eq!(view.output_devices[0].sample_rates_hz, [44_100, 48_000]);
        assert_eq!(view.available_backend_names, ["Auto", "ALSA"]);
    }

    #[test]
    fn frame_sample_preserves_phase_and_idle_math() {
        let sample = FrameStatsSample {
            host_nanos: 1,
            frame_us: 10_000,
            input_us: 100,
            update_us: 200,
            compose_us: 300,
            upload_us: 400,
            draw_us: 500,
            gpu_wait_us: 600,
            display_error_us: 0,
            catching_up: false,
        };
        assert_eq!(sample.cpu_work_us(), 1_500);
        assert_eq!(sample.measured_us(), 2_100);
        assert_eq!(sample.idle_us(), 7_900);
    }

    #[test]
    fn overlay_keys_round_trip() {
        for anchor in [
            OverlayAnchor::TopLeft,
            OverlayAnchor::TopRight,
            OverlayAnchor::BottomLeft,
            OverlayAnchor::BottomRight,
            OverlayAnchor::TopCenter,
            OverlayAnchor::BottomCenter,
        ] {
            assert_eq!(OverlayAnchor::from_key(anchor.to_key()), Some(anchor));
        }
        assert_eq!(OverlayAnchor::from_key("auto"), None);
        assert_eq!(OverlayStyle::from_key("minimal"), OverlayStyle::Minimal);
        assert_eq!(OverlayStyle::from_key("unknown"), OverlayStyle::Detailed);
    }
}
