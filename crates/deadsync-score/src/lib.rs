use bincode::{Decode, Encode};
use chrono::{DateTime, TimeZone, Utc};
use deadsync_core::input::InputSource;
use deadsync_core::song_time::SongTimeNs;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum Grade {
    Quint,
    Tier01,
    Tier02,
    Tier03,
    Tier04,
    Tier05,
    Tier06,
    Tier07,
    Tier08,
    Tier09,
    Tier10,
    Tier11,
    Tier12,
    Tier13,
    Tier14,
    Tier15,
    Tier16,
    Tier17,
    Failed,
}

impl Grade {
    #[inline(always)]
    pub const fn to_sprite_state(&self) -> u32 {
        match self {
            Self::Quint => 0,
            Self::Tier01 => 1,
            Self::Tier02 => 2,
            Self::Tier03 => 3,
            Self::Tier04 => 4,
            Self::Tier05 => 5,
            Self::Tier06 => 6,
            Self::Tier07 => 7,
            Self::Tier08 => 8,
            Self::Tier09 => 9,
            Self::Tier10 => 10,
            Self::Tier11 => 11,
            Self::Tier12 => 12,
            Self::Tier13 => 13,
            Self::Tier14 => 14,
            Self::Tier15 => 15,
            Self::Tier16 => 16,
            Self::Tier17 => 17,
            Self::Failed => 18,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct CachedScore {
    pub grade: Grade,
    pub score_percent: f64,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalScalarScore {
    pub percent: f64,
    pub is_fail: bool,
}

#[derive(Debug, Clone)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub name: String,
    pub machine_tag: Option<String>,
    pub score: f64,
    pub date: String,
    pub is_rival: bool,
    pub is_self: bool,
    pub is_fail: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ReplayEdge {
    pub event_music_time_ns: SongTimeNs,
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
}

#[derive(Debug, Clone)]
pub struct MachineReplayEntry {
    pub rank: u32,
    pub name: String,
    pub score: f64,
    pub date: String,
    pub is_fail: bool,
    pub replay_beat0_time_ns: SongTimeNs,
    pub replay: Vec<ReplayEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudPaneKind {
    Itg,
    Ex,
    HardEx,
}

#[derive(Debug, Clone)]
pub struct LeaderboardPane {
    pub name: String,
    pub entries: Vec<LeaderboardEntry>,
    pub is_ex: bool,
    pub disabled: bool,
    pub personalized: bool,
    pub arrowcloud_kind: Option<ArrowCloudPaneKind>,
}

impl LeaderboardPane {
    #[inline(always)]
    pub fn is_groovestats(&self) -> bool {
        self.name.eq_ignore_ascii_case("GrooveStats")
    }

    #[inline(always)]
    pub fn is_arrowcloud(&self) -> bool {
        self.arrowcloud_kind.is_some() || self.name.eq_ignore_ascii_case("ArrowCloud")
    }

    #[inline(always)]
    pub fn is_hard_ex(&self) -> bool {
        self.arrowcloud_kind == Some(ArrowCloudPaneKind::HardEx)
            || (self.arrowcloud_kind.is_none() && self.name.eq_ignore_ascii_case("ArrowCloud"))
    }
}

#[derive(Debug, Clone)]
pub struct PlayerLeaderboardData {
    pub panes: Vec<LeaderboardPane>,
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CachedPlayerLeaderboardData {
    pub loading: bool,
    pub data: Option<PlayerLeaderboardData>,
    pub error: Option<String>,
}

/// Maximum number of attempts before the backoff schedule saturates. For
/// auto-retryable statuses this is also the auto-retry budget. For manual-only
/// statuses the cooldown caps here and stays there for subsequent failures.
pub const SUBMIT_RETRY_MAX_ATTEMPTS: u8 = 5;

/// Exponential backoff schedule used by every score-submission backend.
/// `attempt` is 1-based: 1 -> 2s, 2 -> 4s, 3 -> 8s, 4 -> 16s, 5 -> 32s.
#[inline(always)]
pub const fn submit_retry_delay_secs(attempt: u8) -> u64 {
    1u64 << attempt
}

/// Convert a remaining duration into whole seconds, rounding up so UI
/// countdowns show the configured delay instead of truncating immediately.
#[inline]
pub fn duration_to_ceil_secs(remaining: Duration) -> u32 {
    let secs = remaining.as_secs();
    let bumped = secs.saturating_add(if remaining.subsec_nanos() > 0 { 1 } else { 0 });
    bumped.min(u32::MAX as u64) as u32
}

/// Why a submitted score was rejected by the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectReason {
    /// The backend accepted the request but considers the score invalid.
    InvalidScore,
    /// HTTP 401 / 403: the API key was not accepted.
    Unauthorized,
    /// HTTP 404: the chart hash is unknown to the backend.
    NotFound,
}

impl RejectReason {
    /// Human-readable label suitable for the evaluation footer.
    pub const fn label(self) -> &'static str {
        match self {
            Self::InvalidScore => "Invalid Score",
            Self::Unauthorized => "Unauthorized",
            Self::NotFound => "Unknown Chart",
        }
    }
}

/// Server-side grade name returned by ArrowCloud's grading systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ArrowCloudServerGrade {
    Sex,
    Quint,
    Quad,
    Tristar,
    Twostar,
    Star,
    SPlus,
    S,
    SMinus,
    APlus,
    A,
    AMinus,
    BPlus,
    B,
    BMinus,
    CPlus,
    C,
    CMinus,
    D,
    Failed,
}

impl ArrowCloudServerGrade {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sex => "Sex",
            Self::Quint => "Quint",
            Self::Quad => "Quad",
            Self::Tristar => "Tristar",
            Self::Twostar => "Twostar",
            Self::Star => "Star",
            Self::SPlus => "S+",
            Self::S => "S",
            Self::SMinus => "S-",
            Self::APlus => "A+",
            Self::A => "A",
            Self::AMinus => "A-",
            Self::BPlus => "B+",
            Self::B => "B",
            Self::BMinus => "B-",
            Self::CPlus => "C+",
            Self::C => "C",
            Self::CMinus => "C-",
            Self::D => "D",
            Self::Failed => "F",
        }
    }

    /// Parse the canonical AC grade string. Case-sensitive; whitespace is trimmed.
    pub fn from_server_str(s: &str) -> Option<Self> {
        match s.trim() {
            "Sex" => Some(Self::Sex),
            "Quint" => Some(Self::Quint),
            "Quad" => Some(Self::Quad),
            "Tristar" => Some(Self::Tristar),
            "Twostar" => Some(Self::Twostar),
            "Star" => Some(Self::Star),
            "S+" => Some(Self::SPlus),
            "S" => Some(Self::S),
            "S-" => Some(Self::SMinus),
            "A+" => Some(Self::APlus),
            "A" => Some(Self::A),
            "A-" => Some(Self::AMinus),
            "B+" => Some(Self::BPlus),
            "B" => Some(Self::B),
            "B-" => Some(Self::BMinus),
            "C+" => Some(Self::CPlus),
            "C" => Some(Self::C),
            "C-" => Some(Self::CMinus),
            "D" => Some(Self::D),
            "F" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// A single ArrowCloud score for one chart/leaderboard pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowCloudScore {
    /// 0.0..=1.0, from the API percent divided by 100.
    pub score_percent: f64,
    pub server_grade: Option<ArrowCloudServerGrade>,
    pub played_at: Option<DateTime<Utc>>,
    pub play_id: Option<i64>,
    pub is_fail: bool,
}

impl Encode for ArrowCloudScore {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.score_percent.encode(encoder)?;
        self.server_grade.encode(encoder)?;
        self.played_at
            .map(|d| d.timestamp_millis())
            .encode(encoder)?;
        self.play_id.encode(encoder)?;
        self.is_fail.encode(encoder)?;
        Ok(())
    }
}

impl<C> Decode<C> for ArrowCloudScore {
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let score_percent = f64::decode(decoder)?;
        let server_grade = Option::<ArrowCloudServerGrade>::decode(decoder)?;
        let played_at_ms = Option::<i64>::decode(decoder)?;
        let play_id = Option::<i64>::decode(decoder)?;
        let is_fail = bool::decode(decoder)?;
        let played_at = played_at_ms.and_then(|ms| Utc.timestamp_millis_opt(ms).single());
        Ok(Self {
            score_percent,
            server_grade,
            played_at,
            play_id,
            is_fail,
        })
    }
}

impl<'de, C> bincode::BorrowDecode<'de, C> for ArrowCloudScore {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Self::decode(decoder)
    }
}

impl ArrowCloudScore {
    /// Adapter used to merge AC cache entries with local and GrooveStats scores.
    pub fn to_cached_score(&self) -> CachedScore {
        let score_10000 = (self.score_percent * 10000.0).clamp(0.0, 10000.0);
        if self.is_fail {
            return CachedScore {
                grade: Grade::Failed,
                score_percent: score_10000 / 10000.0,
                lamp_index: None,
                lamp_judge_count: None,
            };
        }
        CachedScore {
            grade: score_to_grade(score_10000),
            score_percent: score_10000 / 10000.0,
            lamp_index: (((score_10000 / 10000.0) - 1.0).abs() <= 1e-9).then_some(1),
            lamp_judge_count: None,
        }
    }
}

/// Cached AC scores for a single chart, one entry per global leaderboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Encode, Decode)]
pub struct ArrowCloudScores {
    pub itg: Option<ArrowCloudScore>,
    pub ex: Option<ArrowCloudScore>,
    pub hard_ex: Option<ArrowCloudScore>,
}

/// Global ArrowCloud leaderboard variants. Numeric values match server IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ArrowCloudLeaderboard {
    Ex = 2,
    Itg = 3,
    HardEx = 4,
}

impl ArrowCloudLeaderboard {
    pub const ALL_GLOBAL: [Self; 3] = [Self::HardEx, Self::Ex, Self::Itg];

    #[inline(always)]
    pub const fn id(self) -> u32 {
        self as u32
    }

    #[inline(always)]
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            2 => Some(Self::Ex),
            3 => Some(Self::Itg),
            4 => Some(Self::HardEx),
            _ => None,
        }
    }
}

impl Serialize for ArrowCloudLeaderboard {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.id())
    }
}

// Lua charts stay blocked from online submit unless their effects have been
// verified closely enough to match ITGmania for scoring purposes.
const LUA_SCORE_SUBMIT_ALLOWLIST: [&str; 8] = [
    "d5bd4dd7224f68ff", // Spooky (SM)
    "c9e45c5e534f058d", // media offline (SM)
    "596b42ed8317d9b8", // Godspeed (SX)
    "3926ec3e5f1aaede", // CO5M1C R4ILR0AD (SH)
    "a147dd828cd08fc7", // Riddle (DX)
    "f95bc209c6f2cbfe", // Levels (SM)
    "b50d0c3916e75b84", // Levels (SH)
    "f41a24722a37758f", // Levels (SX)
];

#[inline(always)]
pub fn lua_chart_submit_allowed(chart_hash: &str) -> bool {
    let hash = chart_hash.trim();
    !hash.is_empty()
        && LUA_SCORE_SUBMIT_ALLOWLIST
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(hash))
}

#[inline(always)]
pub fn lua_submit_allowed(song_has_lua: bool, chart_hash: &str) -> bool {
    !song_has_lua || lua_chart_submit_allowed(chart_hash)
}

#[inline(always)]
pub const fn gameplay_run_passed(
    song_completed_naturally: bool,
    is_failing: bool,
    life: f32,
    has_fail_time: bool,
) -> bool {
    song_completed_naturally && !gameplay_run_failed(is_failing, has_fail_time) && life > 0.0
}

#[inline(always)]
pub const fn gameplay_run_failed(is_failing: bool, has_fail_time: bool) -> bool {
    is_failing || has_fail_time
}

#[inline(always)]
pub fn promote_quint_grade(grade: Grade, ex_score_percent: f64) -> Grade {
    if grade != Grade::Failed && ex_score_percent >= 100.0 {
        Grade::Quint
    } else {
        grade
    }
}

pub fn score_to_grade(score: f64) -> Grade {
    let percent = score / 10000.0;
    if percent >= 1.00 {
        Grade::Tier01
    } else if percent >= 0.99 {
        Grade::Tier02
    } else if percent >= 0.98 {
        Grade::Tier03
    } else if percent >= 0.96 {
        Grade::Tier04
    } else if percent >= 0.94 {
        Grade::Tier05
    } else if percent >= 0.92 {
        Grade::Tier06
    } else if percent >= 0.89 {
        Grade::Tier07
    } else if percent >= 0.86 {
        Grade::Tier08
    } else if percent >= 0.83 {
        Grade::Tier09
    } else if percent >= 0.80 {
        Grade::Tier10
    } else if percent >= 0.76 {
        Grade::Tier11
    } else if percent >= 0.72 {
        Grade::Tier12
    } else if percent >= 0.68 {
        Grade::Tier13
    } else if percent >= 0.64 {
        Grade::Tier14
    } else if percent >= 0.60 {
        Grade::Tier15
    } else if percent >= 0.55 {
        Grade::Tier16
    } else {
        Grade::Tier17
    }
}

#[inline(always)]
pub fn leaderboard_rank_for_score(entries: &[LeaderboardEntry], score_percent: f64) -> Option<u32> {
    if !score_percent.is_finite() {
        return None;
    }
    let target = (score_percent * 10000.0).round();
    let higher_scores = entries
        .iter()
        .filter(|entry| entry.score - target > 0.5)
        .count();
    Some((higher_scores as u32).saturating_add(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreImportEndpoint {
    GrooveStats,
    BoogieStats,
    ArrowCloud,
}

impl ScoreImportEndpoint {
    #[inline(always)]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::GrooveStats => "GrooveStats",
            Self::BoogieStats => "BoogieStats",
            Self::ArrowCloud => "ArrowCloud",
        }
    }

    #[inline(always)]
    pub const fn requires_username(self) -> bool {
        !matches!(self, Self::ArrowCloud)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScoreBulkImportSummary {
    pub requested_charts: usize,
    pub imported_scores: usize,
    pub missing_scores: usize,
    pub failed_requests: usize,
    pub rate_limit_per_second: u32,
    pub elapsed_seconds: f32,
    pub canceled: bool,
}

#[derive(Debug, Clone)]
pub struct ScoreImportProgress {
    pub processed_charts: usize,
    pub total_charts: usize,
    pub imported_scores: usize,
    pub missing_scores: usize,
    pub failed_requests: usize,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachedItlScore {
    pub ex_hundredths: u32,
    pub clear_type: u8,
    pub points: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ItlEvalState {
    pub active: bool,
    pub eligible: bool,
    pub chart_no_cmod: bool,
    pub used_cmod: bool,
    pub reason_lines: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GrooveStatsEvalState {
    pub valid: bool,
    pub reason_lines: Vec<String>,
    pub manual_qr_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatsSubmitUiStatus {
    Submitting,
    Submitted,
    TimedOut,
    NetworkError,
    ServerError { http_status: u16 },
    Rejected { reason: RejectReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudSubmitUiStatus {
    Submitting,
    Submitted,
    TimedOut,
    NetworkError,
    ServerError { http_status: u16 },
    Rejected { reason: RejectReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrooveStatsSubmitRecordBanner {
    PersonalBest,
    WorldRecord,
    WorldRecordEx,
}

#[derive(Clone, Debug)]
pub enum ItlOverlayPage {
    Text(String),
    Leaderboard(Vec<LeaderboardEntry>),
}

#[derive(Clone, Debug, Default)]
pub struct ItlEventProgress {
    pub name: String,
    pub is_doubles: bool,
    pub score_hundredths: u32,
    pub score_delta_hundredths: i32,
    pub current_points: u32,
    pub point_delta: i32,
    pub current_ranking_points: u32,
    pub ranking_delta: i32,
    pub current_song_points: u32,
    pub song_delta: i32,
    pub current_ex_points: u32,
    pub ex_delta: i32,
    pub current_total_points: u32,
    pub total_delta: i32,
    pub total_passes: u32,
    pub clear_type_before: Option<u8>,
    pub clear_type_after: Option<u8>,
    pub overlay_pages: Vec<ItlOverlayPage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(score: f64) -> LeaderboardEntry {
        LeaderboardEntry {
            rank: 0,
            name: String::new(),
            machine_tag: None,
            score,
            date: String::new(),
            is_rival: false,
            is_self: false,
            is_fail: false,
        }
    }

    #[test]
    fn grade_sprite_indices_are_stable() {
        assert_eq!(Grade::Quint.to_sprite_state(), 0);
        assert_eq!(Grade::Tier01.to_sprite_state(), 1);
        assert_eq!(Grade::Failed.to_sprite_state(), 18);
    }

    #[test]
    fn score_thresholds_match_itg_tiers() {
        assert_eq!(score_to_grade(10000.0), Grade::Tier01);
        assert_eq!(score_to_grade(9900.0), Grade::Tier02);
        assert_eq!(score_to_grade(9400.0), Grade::Tier05);
        assert_eq!(score_to_grade(5499.0), Grade::Tier17);
    }

    #[test]
    fn quint_promotion_preserves_failed_runs() {
        assert_eq!(promote_quint_grade(Grade::Tier01, 100.0), Grade::Quint);
        assert_eq!(promote_quint_grade(Grade::Failed, 100.0), Grade::Failed);
    }

    #[test]
    fn leaderboard_rank_places_current_run_before_equal_scores() {
        let entries = [entry(9800.0), entry(9750.0), entry(9700.0)];
        assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
        assert_eq!(leaderboard_rank_for_score(&entries, f64::NAN), None);
    }

    #[test]
    fn leaderboard_pane_kind_helpers_match_legacy_names() {
        let hard_ex = LeaderboardPane {
            name: "Hard EX".to_string(),
            entries: Vec::new(),
            is_ex: true,
            disabled: false,
            personalized: true,
            arrowcloud_kind: Some(ArrowCloudPaneKind::HardEx),
        };
        assert!(hard_ex.is_arrowcloud());
        assert!(hard_ex.is_hard_ex());

        let legacy_arrowcloud = LeaderboardPane {
            name: "ArrowCloud".to_string(),
            entries: Vec::new(),
            is_ex: true,
            disabled: false,
            personalized: true,
            arrowcloud_kind: None,
        };
        assert!(legacy_arrowcloud.is_arrowcloud());
        assert!(legacy_arrowcloud.is_hard_ex());
    }

    #[test]
    fn submit_retry_policy_rounds_remaining_duration_up() {
        assert_eq!(SUBMIT_RETRY_MAX_ATTEMPTS, 5);
        assert_eq!(submit_retry_delay_secs(1), 2);
        assert_eq!(submit_retry_delay_secs(5), 32);
        assert_eq!(duration_to_ceil_secs(Duration::from_secs(16)), 16);
        assert_eq!(duration_to_ceil_secs(Duration::from_millis(15_001)), 16);
    }

    #[test]
    fn reject_reason_labels_match_ui_copy() {
        assert_eq!(RejectReason::InvalidScore.label(), "Invalid Score");
        assert_eq!(RejectReason::Unauthorized.label(), "Unauthorized");
        assert_eq!(RejectReason::NotFound.label(), "Unknown Chart");
    }

    #[test]
    fn arrowcloud_leaderboard_id_round_trips() {
        for v in ArrowCloudLeaderboard::ALL_GLOBAL {
            assert_eq!(ArrowCloudLeaderboard::from_id(v.id()), Some(v));
        }
        assert_eq!(ArrowCloudLeaderboard::from_id(0), None);
        assert_eq!(ArrowCloudLeaderboard::from_id(9), None);
    }

    #[test]
    fn arrowcloud_leaderboard_serializes_as_integer() {
        let json = serde_json::to_string(&ArrowCloudLeaderboard::Itg).unwrap();
        assert_eq!(json, "3");
        let json = serde_json::to_string(&ArrowCloudLeaderboard::ALL_GLOBAL).unwrap();
        assert_eq!(json, "[4,2,3]");
    }

    #[test]
    fn arrowcloud_server_grade_round_trips_through_str() {
        for grade in [
            ArrowCloudServerGrade::Sex,
            ArrowCloudServerGrade::Quint,
            ArrowCloudServerGrade::Quad,
            ArrowCloudServerGrade::Tristar,
            ArrowCloudServerGrade::SPlus,
            ArrowCloudServerGrade::SMinus,
            ArrowCloudServerGrade::Failed,
        ] {
            assert_eq!(
                ArrowCloudServerGrade::from_server_str(grade.as_str()),
                Some(grade)
            );
        }
    }

    #[test]
    fn arrowcloud_score_bincode_round_trip_preserves_date() {
        let original = ArrowCloudScore {
            score_percent: 0.9989,
            server_grade: Some(ArrowCloudServerGrade::Tristar),
            played_at: Some(Utc.timestamp_millis_opt(1_777_835_417_504).unwrap()),
            play_id: Some(12345),
            is_fail: false,
        };
        let cfg = bincode::config::standard();
        let bytes = bincode::encode_to_vec(original, cfg).unwrap();
        let (decoded, _): (ArrowCloudScore, _) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn lua_submit_allowlist_matches_known_hashes() {
        assert!(lua_chart_submit_allowed("d5bd4dd7224f68ff"));
        assert!(lua_chart_submit_allowed(" D5BD4DD7224F68FF "));
        assert!(!lua_chart_submit_allowed("deadbeefcafebabe"));
        assert!(lua_submit_allowed(false, "deadbeefcafebabe"));
        assert!(!lua_submit_allowed(true, "deadbeefcafebabe"));
        assert!(lua_submit_allowed(true, "d5bd4dd7224f68ff"));
    }

    #[test]
    fn gameplay_pass_fail_helpers_use_completion_and_life() {
        assert!(gameplay_run_passed(true, false, 1.0, false));
        assert!(!gameplay_run_passed(false, false, 1.0, false));
        assert!(!gameplay_run_passed(true, true, 1.0, false));
        assert!(!gameplay_run_passed(true, false, 1.0, true));
        assert!(!gameplay_run_passed(true, false, 0.0, false));
        assert!(!gameplay_run_failed(false, false));
        assert!(gameplay_run_failed(true, false));
        assert!(gameplay_run_failed(false, true));
    }

    #[test]
    fn score_import_endpoint_metadata_matches_backends() {
        assert_eq!(
            ScoreImportEndpoint::GrooveStats.display_name(),
            "GrooveStats"
        );
        assert_eq!(
            ScoreImportEndpoint::BoogieStats.display_name(),
            "BoogieStats"
        );
        assert_eq!(ScoreImportEndpoint::ArrowCloud.display_name(), "ArrowCloud");
        assert!(ScoreImportEndpoint::GrooveStats.requires_username());
        assert!(ScoreImportEndpoint::BoogieStats.requires_username());
        assert!(!ScoreImportEndpoint::ArrowCloud.requires_username());
    }

    #[test]
    fn itl_eval_state_defaults_to_inactive() {
        let state = ItlEvalState::default();
        assert!(!state.active);
        assert!(!state.eligible);
        assert!(!state.chart_no_cmod);
        assert!(!state.used_cmod);
        assert!(state.reason_lines.is_empty());
    }

    #[test]
    fn groovestats_eval_state_defaults_to_invalid() {
        let state = GrooveStatsEvalState::default();
        assert!(!state.valid);
        assert!(state.reason_lines.is_empty());
        assert!(state.manual_qr_url.is_none());
    }
}
