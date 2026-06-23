use bincode::{Decode, Encode};
use chrono::{DateTime, Local, TimeZone, Utc};
use deadsync_core::input::InputSource;
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_invalid};
use deadsync_rules::note::{HoldResult, MineResult, Note};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::{judgment, timing};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub mod import;
pub mod itl;
pub mod stage_stats;
pub use import::{ImportedHighScore, grade_from_itg, local_score_from_itg, parse_itg_datetime_ms};
pub use itl::{
    ItlFileData, ItlHashEntry, ItlJudgments, ItlPointTotals, ex_hundredths, itl_clear_type,
    itl_data_from_json, itl_judgments_better, itl_point_totals, itl_points_for_chart,
    itl_points_for_song, itl_rebuild_song_ranks, itl_score_from_entry, parse_itl_points,
};

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

#[derive(Clone, Copy, Debug, PartialEq, Encode, Decode)]
pub struct LocalScoreBestScalar {
    pub grade: Grade,
    pub percent: f64,
}

#[derive(Debug, Default, Clone, PartialEq, Encode, Decode)]
pub struct LocalScoreIndex {
    pub best_itg: HashMap<String, CachedScore>,
    pub best_ex: HashMap<String, LocalScoreBestScalar>,
    pub best_hard_ex: HashMap<String, LocalScoreBestScalar>,
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

#[derive(Debug)]
pub struct MachineLeaderboardPlay {
    pub name: String,
    pub machine_tag: Option<String>,
    pub score_percent: f64,
    pub played_at_ms: i64,
    pub is_fail: bool,
}

#[derive(Debug)]
pub struct MachineReplayPlay {
    pub initials: String,
    pub score_percent: f64,
    pub played_at_ms: i64,
    pub is_fail: bool,
    pub replay_beat0_time_ns: SongTimeNs,
    pub replay: Vec<LocalReplayEdge>,
}

fn local_score_date_string(played_at_ms: i64) -> String {
    let Some(dt) = Local.timestamp_millis_opt(played_at_ms).single() else {
        return String::new();
    };
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn machine_leaderboard_entries(
    mut plays: Vec<MachineLeaderboardPlay>,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    plays.sort_by(|a, b| {
        b.score_percent
            .partial_cmp(&a.score_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.machine_tag.cmp(&b.machine_tag))
    });

    let take_len = max_entries.min(plays.len());
    let mut out = Vec::with_capacity(take_len);
    for (i, play) in plays.into_iter().take(take_len).enumerate() {
        out.push(LeaderboardEntry {
            rank: (i as u32).saturating_add(1),
            name: play.name,
            machine_tag: play.machine_tag,
            score: (play.score_percent * 10000.0).round(),
            date: local_score_date_string(play.played_at_ms),
            is_rival: false,
            is_self: false,
            is_fail: play.is_fail,
        });
    }
    out
}

pub fn machine_replay_entries(
    mut plays: Vec<MachineReplayPlay>,
    max_entries: usize,
) -> Vec<MachineReplayEntry> {
    plays.sort_by(|a, b| {
        b.score_percent
            .partial_cmp(&a.score_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
            .then_with(|| a.initials.cmp(&b.initials))
    });

    let take_len = max_entries.min(plays.len());
    let mut out = Vec::with_capacity(take_len);
    for (i, play) in plays.into_iter().take(take_len).enumerate() {
        let mut replay = Vec::with_capacity(play.replay.len());
        for edge in play.replay {
            if song_time_ns_invalid(edge.event_music_time_ns) {
                continue;
            }
            replay.push(ReplayEdge {
                event_music_time_ns: edge.event_music_time_ns,
                lane_index: edge.lane,
                pressed: edge.pressed,
                source: edge.input_source(),
            });
        }
        out.push(MachineReplayEntry {
            rank: (i as u32).saturating_add(1),
            name: play.initials,
            score: (play.score_percent * 10000.0).round(),
            date: local_score_date_string(play.played_at_ms),
            is_fail: play.is_fail,
            replay_beat0_time_ns: play.replay_beat0_time_ns,
            replay,
        });
    }
    out
}

#[inline(always)]
pub fn leaderboard_username_matches(entry_name: &str, username: &str) -> bool {
    !username.trim().is_empty() && entry_name.eq_ignore_ascii_case(username)
}

#[inline(always)]
pub fn leaderboard_score_10000(score: f64, is_fail: bool) -> Option<f64> {
    if is_fail || !score.is_finite() {
        None
    } else {
        Some(score.clamp(0.0, 10000.0))
    }
}

#[inline(always)]
pub const fn leaderboard_nonzero_rank(rank: u32) -> Option<u32> {
    if rank == 0 { None } else { Some(rank) }
}

#[inline(always)]
pub fn score_import_entry_matches_profile(
    entry_name: &str,
    is_self: bool,
    endpoint: ScoreImportEndpoint,
    username: &str,
) -> bool {
    if is_self {
        return true;
    }
    endpoint.requires_username() && entry_name.eq_ignore_ascii_case(username)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudPaneKind {
    Itg,
    Ex,
    HardEx,
}

pub fn arrowcloud_pane_kind_from_type(lb_type: &str) -> Option<ArrowCloudPaneKind> {
    if lb_type.is_empty() {
        return None;
    }
    let mut compact = String::with_capacity(lb_type.len());
    for ch in lb_type.chars() {
        if ch.is_ascii_alphanumeric() {
            compact.push(ch.to_ascii_lowercase());
        }
    }
    match compact.as_str() {
        "itg" => Some(ArrowCloudPaneKind::Itg),
        "ex" => Some(ArrowCloudPaneKind::Ex),
        "hardex" | "hex" => Some(ArrowCloudPaneKind::HardEx),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArrowCloudUserContext {
    pub self_user_id: Option<String>,
    pub rival_user_ids: HashSet<String>,
}

pub fn arrowcloud_user_id(raw: &str) -> Option<&str> {
    let user_id = raw.trim();
    (!user_id.is_empty()).then_some(user_id)
}

pub fn arrowcloud_target_user_ids(context: Option<&ArrowCloudUserContext>) -> HashSet<String> {
    let Some(context) = context else {
        return HashSet::new();
    };
    let mut out = HashSet::with_capacity(
        usize::from(context.self_user_id.is_some()) + context.rival_user_ids.len(),
    );
    if let Some(self_user_id) = context.self_user_id.as_ref() {
        out.insert(self_user_id.clone());
    }
    out.extend(context.rival_user_ids.iter().cloned());
    out
}

pub fn arrowcloud_entry_flags(
    entry_user_id: Option<&str>,
    entry_is_self: bool,
    entry_is_rival: bool,
    context: Option<&ArrowCloudUserContext>,
) -> (bool, bool) {
    let is_self = entry_is_self
        || context
            .and_then(|context| context.self_user_id.as_deref())
            .is_some_and(|self_user_id| entry_user_id == Some(self_user_id));
    let is_rival = entry_is_rival
        || context.is_some_and(|context| {
            entry_user_id.is_some_and(|user_id| context.rival_user_ids.contains(user_id))
        });
    (is_self, is_rival)
}

pub fn arrowcloud_leaderboard_entry(
    rank: u32,
    alias: String,
    score_percent: f64,
    date: String,
    is_self: bool,
    is_rival: bool,
) -> LeaderboardEntry {
    let score = if score_percent.is_finite() {
        (score_percent * 100.0).clamp(0.0, 10000.0)
    } else {
        0.0
    };
    LeaderboardEntry {
        rank,
        name: alias,
        machine_tag: None,
        score,
        date,
        is_rival,
        is_self,
        is_fail: false,
    }
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

pub fn leaderboard_pane(
    name: &str,
    entries: Vec<LeaderboardEntry>,
    is_ex: bool,
) -> Option<LeaderboardPane> {
    if entries.is_empty() {
        return None;
    }
    Some(LeaderboardPane {
        name: name.to_string(),
        entries,
        is_ex,
        disabled: false,
        personalized: true,
        arrowcloud_kind: None,
    })
}

pub fn arrowcloud_hard_ex_leaderboard_pane(
    entries: Vec<LeaderboardEntry>,
    personalized: bool,
) -> LeaderboardPane {
    LeaderboardPane {
        name: "ArrowCloud".to_string(),
        entries,
        is_ex: false,
        disabled: false,
        personalized,
        arrowcloud_kind: Some(ArrowCloudPaneKind::HardEx),
    }
}

#[inline(always)]
pub fn arrowcloud_empty_hard_ex_leaderboard_pane() -> LeaderboardPane {
    arrowcloud_hard_ex_leaderboard_pane(Vec::new(), false)
}

#[derive(Debug, Clone)]
pub struct PlayerLeaderboardData {
    pub panes: Vec<LeaderboardPane>,
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerLeaderboardCacheKey {
    pub chart_hash: String,
    pub api_key: String,
    pub arrowcloud_api_key: String,
    pub include_arrowcloud: bool,
    pub show_ex_score: bool,
}

#[derive(Debug, Clone)]
pub struct CachedPlayerLeaderboardData {
    pub loading: bool,
    pub data: Option<PlayerLeaderboardData>,
    pub error: Option<String>,
}

impl CachedPlayerLeaderboardData {
    #[inline(always)]
    pub const fn loading() -> Self {
        Self {
            loading: true,
            data: None,
            error: None,
        }
    }

    #[inline(always)]
    pub fn ready(data: PlayerLeaderboardData) -> Self {
        Self {
            loading: false,
            data: Some(data),
            error: None,
        }
    }

    #[inline(always)]
    pub fn error(error: String) -> Self {
        Self {
            loading: false,
            data: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameplayScoreboxProfileSnapshot {
    pub display_scorebox: bool,
    pub gs_active: bool,
    pub show_ex_score: bool,
    api_key: String,
    arrowcloud_api_key: String,
    include_arrowcloud: bool,
    gs_username: String,
    persistent_profile_id: Option<String>,
    auto_profile_id: Option<String>,
    should_auto_populate: bool,
}

impl GameplayScoreboxProfileSnapshot {
    pub fn new(
        display_scorebox: bool,
        gs_active: bool,
        show_ex_score: bool,
        api_key: String,
        arrowcloud_api_key: String,
        include_arrowcloud: bool,
        gs_username: String,
        persistent_profile_id: Option<String>,
        auto_profile_id: Option<String>,
        should_auto_populate: bool,
    ) -> Self {
        Self {
            display_scorebox,
            gs_active,
            show_ex_score,
            api_key,
            arrowcloud_api_key,
            include_arrowcloud,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
        }
    }

    #[inline(always)]
    pub fn api_key(&self) -> &str {
        self.api_key.as_str()
    }

    #[inline(always)]
    pub fn arrowcloud_api_key(&self) -> &str {
        self.arrowcloud_api_key.as_str()
    }

    #[inline(always)]
    pub const fn include_arrowcloud(&self) -> bool {
        self.include_arrowcloud
    }

    #[inline(always)]
    pub fn gs_username(&self) -> &str {
        self.gs_username.as_str()
    }

    #[inline(always)]
    pub fn persistent_profile_id(&self) -> Option<&str> {
        self.persistent_profile_id.as_deref()
    }

    #[inline(always)]
    pub fn auto_profile_id(&self) -> Option<&str> {
        self.auto_profile_id.as_deref()
    }

    #[inline(always)]
    pub const fn should_auto_populate(&self) -> bool {
        self.should_auto_populate
    }
}

pub fn scorebox_snapshot(
    display_scorebox: bool,
    show_ex_score: bool,
    side_joined: bool,
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
    api_key: &str,
    arrowcloud_api_key: &str,
    gs_username: &str,
    persistent_profile_id: Option<String>,
) -> GameplayScoreboxProfileSnapshot {
    let api_key = api_key.trim().to_string();
    let arrowcloud_api_key = arrowcloud_api_key.trim().to_string();
    let gs_username = gs_username.trim().to_string();
    let include_arrowcloud = enable_arrowcloud && !arrowcloud_api_key.is_empty();
    let auto_profile_id = if auto_populate_gs_scores {
        persistent_profile_id.clone()
    } else {
        None
    };
    let should_auto_populate =
        auto_populate_gs_scores && auto_profile_id.is_some() && !gs_username.is_empty();
    GameplayScoreboxProfileSnapshot::new(
        display_scorebox,
        enable_groovestats && side_joined && !api_key.is_empty(),
        show_ex_score,
        api_key,
        arrowcloud_api_key,
        include_arrowcloud,
        gs_username,
        persistent_profile_id,
        auto_profile_id,
        should_auto_populate,
    )
}

pub fn player_leaderboard_cache_key(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<PlayerLeaderboardCacheKey> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() || !profile_snapshot.gs_active {
        return None;
    }

    Some(PlayerLeaderboardCacheKey {
        chart_hash: chart_hash.to_string(),
        api_key: profile_snapshot.api_key.clone(),
        arrowcloud_api_key: profile_snapshot.arrowcloud_api_key.clone(),
        include_arrowcloud: profile_snapshot.include_arrowcloud,
        show_ex_score: profile_snapshot.show_ex_score,
    })
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

pub fn arrowcloud_score_from_retrieve_fields(
    score: Option<f64>,
    grade: Option<&str>,
    date: Option<&str>,
    play_id: Option<i64>,
    is_fail: bool,
) -> Option<ArrowCloudScore> {
    let percent_0_100 = score?.clamp(0.0, 100.0);
    let server_grade = grade.and_then(ArrowCloudServerGrade::from_server_str);
    let played_at = date
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    Some(ArrowCloudScore {
        score_percent: percent_0_100 / 100.0,
        server_grade,
        played_at,
        play_id,
        is_fail,
    })
}

pub fn arrowcloud_score_from_submit_percent(
    percent_0_100: f64,
    is_fail: bool,
    submitted_at: DateTime<Utc>,
) -> Option<ArrowCloudScore> {
    if !percent_0_100.is_finite() {
        return None;
    }
    Some(ArrowCloudScore {
        score_percent: percent_0_100.clamp(0.0, 100.0) / 100.0,
        server_grade: None,
        played_at: Some(submitted_at),
        play_id: None,
        is_fail,
    })
}

pub fn set_arrowcloud_score_for_leaderboard(
    scores: &mut ArrowCloudScores,
    leaderboard_id: u32,
    score: ArrowCloudScore,
) -> bool {
    match ArrowCloudLeaderboard::from_id(leaderboard_id) {
        Some(ArrowCloudLeaderboard::Itg) => scores.itg = Some(score),
        Some(ArrowCloudLeaderboard::Ex) => scores.ex = Some(score),
        Some(ArrowCloudLeaderboard::HardEx) => scores.hard_ex = Some(score),
        None => return false,
    }
    true
}

#[inline]
pub fn merge_arrowcloud_score_slot(
    existing: &mut Option<ArrowCloudScore>,
    incoming: Option<ArrowCloudScore>,
) {
    let Some(new_score) = incoming else {
        return;
    };
    match existing {
        None => *existing = Some(new_score),
        Some(prev) => {
            if prev.is_fail && !new_score.is_fail {
                *prev = new_score;
            } else if !prev.is_fail && new_score.is_fail {
                // Keep prev: the existing non-failed score wins.
            } else if new_score.score_percent > prev.score_percent {
                *prev = new_score;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ArrowCloudSubmitStats {
    pub judgment_counts: judgment::JudgeCounts,
    pub window_counts: timing::WindowCounts,
    pub holds_held: u32,
    pub mines_hit: u32,
    pub mines_avoided: u32,
    pub rolls_held: u32,
}

#[inline(always)]
pub fn arrowcloud_time_in_submit_window(time_ns: i64, fail_time_ns: Option<i64>) -> bool {
    match fail_time_ns {
        Some(fail_time) => !song_time_ns_invalid(time_ns) && time_ns <= fail_time,
        None => true,
    }
}

pub fn arrowcloud_submit_stats_from_results(
    notes: &[Note],
    note_times: &[i64],
    hold_end_times: &[Option<i64>],
    fail_time_ns: Option<i64>,
) -> ArrowCloudSubmitStats {
    let mut stats = ArrowCloudSubmitStats::default();
    let mut idx = 0usize;
    while idx < notes.len() {
        let row_index = notes[idx].row_index;
        let row_start = idx;
        let row_time = note_times.get(idx).copied().unwrap_or(i64::MIN);
        while idx < notes.len() && notes[idx].row_index == row_index {
            idx += 1;
        }
        if !arrowcloud_time_in_submit_window(row_time, fail_time_ns) {
            continue;
        }
        let Some(row_judgment) =
            judgment::aggregate_row_final_judgment(notes[row_start..idx].iter().filter_map(|n| {
                if n.is_fake || !n.can_be_judged || matches!(n.note_type, NoteType::Mine) {
                    None
                } else {
                    n.result.as_ref()
                }
            }))
        else {
            continue;
        };
        stats.judgment_counts[judgment::judge_grade_ix(row_judgment.grade)] =
            stats.judgment_counts[judgment::judge_grade_ix(row_judgment.grade)].saturating_add(1);
        judgment::add_judgment_to_window_counts(
            &mut stats.window_counts,
            row_judgment,
            timing::FA_PLUS_W0_MS,
        );
    }

    for (i, note) in notes.iter().enumerate() {
        if note.is_fake || !note.can_be_judged {
            continue;
        }
        let note_time = note_times.get(i).copied().unwrap_or(i64::MIN);
        match note.note_type {
            NoteType::Hold | NoteType::Roll => {
                let result_time = hold_end_times
                    .get(i)
                    .and_then(|time| *time)
                    .unwrap_or(note_time);
                if !arrowcloud_time_in_submit_window(result_time, fail_time_ns) {
                    continue;
                }
                if note.hold.as_ref().and_then(|h| h.result) == Some(HoldResult::Held) {
                    if note.note_type == NoteType::Hold {
                        stats.holds_held = stats.holds_held.saturating_add(1);
                    } else {
                        stats.rolls_held = stats.rolls_held.saturating_add(1);
                    }
                }
            }
            NoteType::Mine => {
                if !arrowcloud_time_in_submit_window(note_time, fail_time_ns) {
                    continue;
                }
                match note.mine_result {
                    Some(MineResult::Hit) => {
                        stats.mines_hit = stats.mines_hit.saturating_add(1);
                    }
                    Some(MineResult::Avoided) => {
                        stats.mines_avoided = stats.mines_avoided.saturating_add(1);
                    }
                    None => {}
                }
            }
            NoteType::Tap | NoteType::Lift | NoteType::Fake => {}
        }
    }

    stats
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GsCommentCounts {
    pub w: u32,
    pub e: u32,
    pub g: u32,
    pub d: u32,
    pub wo: u32,
    pub m: u32,
}

pub fn parse_gs_comment_counts(comment: &str) -> GsCommentCounts {
    let mut counts = GsCommentCounts::default();
    for part in comment.split(',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }

        let mut value: u32 = 0;
        let mut idx = 0usize;
        for (i, ch) in s.char_indices() {
            if let Some(d) = ch.to_digit(10) {
                value = value.saturating_mul(10).saturating_add(d);
                idx = i + ch.len_utf8();
            } else {
                break;
            }
        }
        if value == 0 {
            continue;
        }

        let suffix = s[idx..].trim().to_ascii_lowercase();
        match suffix.as_str() {
            "w" => counts.w = value,
            "e" => counts.e = value,
            "g" => counts.g = value,
            "d" => counts.d = value,
            "wo" => counts.wo = value,
            "m" => counts.m = value,
            _ => {}
        }
    }
    counts
}

pub fn parse_gs_comment_ex_percent(comment: &str) -> Option<f64> {
    let bytes = comment.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        if !bytes[idx].is_ascii_digit() {
            idx += 1;
            continue;
        }

        let start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx < bytes.len() && bytes[idx] == b'.' {
            idx += 1;
            while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                idx += 1;
            }
        }

        let value_end = idx;
        if idx < bytes.len() && bytes[idx] == b'%' {
            idx += 1;
        }
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }

        if idx + 2 <= bytes.len()
            && bytes[idx].eq_ignore_ascii_case(&b'e')
            && bytes[idx + 1].eq_ignore_ascii_case(&b'x')
            && bytes
                .get(idx + 2)
                .is_none_or(|next| !next.is_ascii_alphabetic())
        {
            return comment[start..value_end].parse::<f64>().ok();
        }
    }
    None
}

#[inline(always)]
pub fn groovestats_score_10000_from_counts(
    scoring_counts: &deadsync_rules::judgment::JudgeCounts,
    holds_held_for_score: u32,
    rolls_held_for_score: u32,
    mines_hit_for_score: u32,
    possible_grade_points: i32,
) -> u32 {
    let score_percent = deadsync_rules::judgment::calculate_itg_score_percent_from_counts(
        scoring_counts,
        holds_held_for_score,
        rolls_held_for_score,
        mines_hit_for_score,
        possible_grade_points,
    );
    (score_percent * 10000.0).round().clamp(0.0, 10000.0) as u32
}

#[inline(always)]
pub fn groovestats_rate_hundredths(music_rate: f32) -> u32 {
    if music_rate.is_finite() && music_rate > 0.0 {
        (music_rate * 100.0).round().clamp(0.0, u32::MAX as f32) as u32
    } else {
        100
    }
}

#[inline(always)]
pub const fn groovestats_used_cmod(scroll_speed: ScrollSpeedSetting) -> bool {
    matches!(scroll_speed, ScrollSpeedSetting::CMod(_))
}

#[inline(always)]
pub fn gs_ex_scoreboard_is_quint(score_10000: f64) -> bool {
    score_10000.is_finite() && score_10000.round().clamp(0.0, 10000.0) >= 10000.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GsExEvidence {
    pub leaderboard_score_10000: Option<f64>,
    pub comment_percent: Option<f64>,
}

impl GsExEvidence {
    pub fn from_sources(leaderboard_score_10000: Option<f64>, comment: Option<&str>) -> Self {
        Self {
            leaderboard_score_10000,
            comment_percent: comment.and_then(parse_gs_comment_ex_percent),
        }
    }

    #[inline(always)]
    pub fn is_quint(self) -> Option<bool> {
        if let Some(score) = self.leaderboard_score_10000 {
            return Some(gs_ex_scoreboard_is_quint(score));
        }
        self.comment_percent.map(|ex| ex >= 100.0)
    }

    #[inline(always)]
    pub fn proves_nonquint(self) -> bool {
        self.is_quint() == Some(false)
    }
}

pub fn gs_lamp_judge_count(lamp_index: Option<u8>, comment: Option<&str>) -> Option<u8> {
    let lamp_index = lamp_index?;
    let comment = comment?;
    let counts = parse_gs_comment_counts(comment);

    // zmod-style single-digit overlay:
    // - lamp 1 shows #white fantastics
    // - lamp 2 shows #W2
    // - lamp 3 shows #W3
    let count = match lamp_index {
        1 => counts.w,
        2 => counts.e,
        3 => counts.g,
        _ => return None,
    };
    if (1..=9).contains(&count) {
        Some(count as u8)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GsLampChartStats {
    pub total_steps: u32,
    pub holds: u32,
    pub rolls: u32,
}

const GS_DP_W1: i32 = 5;
const GS_DP_W2: i32 = 4;
const GS_DP_W3: i32 = 2;
const GS_DP_W4: i32 = 0;
const GS_DP_W5: i32 = -6;
const GS_DP_MISS: i32 = -12;
const GS_DP_HELD: i32 = 5;

pub fn gs_lamp_index_from_chart_stats(
    score_10000: f64,
    comment: Option<&str>,
    ex_evidence: GsExEvidence,
    stats: Option<GsLampChartStats>,
) -> Option<u8> {
    let score_percent = score_10000 / 10000.0;

    // Perfect 100% ITG can still be a quad. Trust the GS/BS EX scoreboard
    // first; only fall back to the EX comment when that board is unavailable.
    if (score_percent - 1.0).abs() <= 1e-9 {
        if let Some(is_quint) = ex_evidence.is_quint() {
            return Some(if is_quint { 0 } else { 1 });
        }
        if let Some(comment) = comment {
            let counts = parse_gs_comment_counts(comment);
            let explicit_nonquint_counts = counts.w != 0
                || counts.e != 0
                || counts.g != 0
                || counts.d != 0
                || counts.wo != 0
                || counts.m != 0;
            if explicit_nonquint_counts {
                return Some(1);
            }
        }
        return Some(1);
    }

    let comment = comment?;
    let counts = parse_gs_comment_counts(comment);

    // Any explicit Miss or Way Off disqualifies lamps immediately.
    if counts.m > 0 || counts.wo > 0 {
        return None;
    }

    let stats = stats?;
    let taps_rows = stats.total_steps as i32;
    let holds = stats.holds as i32;
    let rolls = stats.rolls as i32;

    if taps_rows <= 0 {
        return None;
    }

    // Reconstruct W1 count as "everything not explicitly listed".
    let non_w1_from_suffixes = counts.e + counts.g + counts.d + counts.wo + counts.m + counts.w;
    let inferred_w1 = if (non_w1_from_suffixes as i32) > taps_rows {
        0
    } else {
        stats
            .total_steps
            .saturating_sub(counts.e + counts.g + counts.d + counts.wo + counts.m)
    };
    let w1_total = counts.w.max(inferred_w1);

    // Dance Points from tap judgments (rows) only, per ITG PercentScoreWeight.
    let dp_taps: i32 = (w1_total as i32) * GS_DP_W1
        + (counts.e as i32) * GS_DP_W2
        + (counts.g as i32) * GS_DP_W3
        + (counts.d as i32) * GS_DP_W4
        + (counts.wo as i32) * GS_DP_W5
        + (counts.m as i32) * GS_DP_MISS;

    // Holds + rolls assumed fully held for the "no hidden errors" hypothesis.
    let dp_hold_roll: i32 = (holds + rolls) * GS_DP_HELD;

    // Maximum possible DP if every tap was W1 and all holds/rolls fully held.
    let dp_possible_max: i32 = (taps_rows * GS_DP_W1 + dp_hold_roll).max(1);
    let dp_expect_no_hidden_errors: i32 = dp_taps + dp_hold_roll;

    let dp_expect_frac = f64::from(dp_expect_no_hidden_errors) / f64::from(dp_possible_max);
    let dp_diff = (score_percent - dp_expect_frac).abs();
    let dp_consistent = dp_diff <= 0.0005;

    if !dp_consistent {
        return None;
    }

    // At this point, we know there were no hidden hold/mine mistakes.
    // Classify the lamp tier, mirroring Simply Love's StageAward semantics.
    if counts.g == 0 && counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // Only W1/W2 present (and W1 reconstructed) => W2 full combo (FEC).
        if counts.e > 0 || w1_total > 0 {
            return Some(2);
        }
    }

    if counts.d == 0 && counts.wo == 0 && counts.m == 0 && counts.g > 0 {
        return Some(3);
    }

    // No WayOff/Miss and DP-consistent => at worst a W4 full combo.
    if counts.wo == 0 && counts.m == 0 {
        return Some(4);
    }

    None
}

#[inline(always)]
pub fn cached_failed_gs_score(score_10000: f64) -> CachedScore {
    cached_score(Grade::Failed, score_10000 / 10000.0, None, None)
}

#[inline(always)]
pub fn cached_missing_gs_score() -> CachedScore {
    cached_failed_gs_score(0.0)
}

#[inline(always)]
pub fn cached_gs_score_from_lamp(
    score_10000: f64,
    is_fail: bool,
    lamp_index: Option<u8>,
    comment: Option<&str>,
) -> CachedScore {
    if is_fail {
        return cached_failed_gs_score(score_10000);
    }
    let lamp_judge_count = gs_lamp_judge_count(lamp_index, comment);
    let mut grade = score_to_grade(score_10000);
    if lamp_index == Some(0) {
        grade = promote_quint_grade(grade, 100.0);
    }
    cached_score(grade, score_10000 / 10000.0, lamp_index, lamp_judge_count)
}

#[inline(always)]
pub fn cached_gs_score_from_chart_stats(
    score_10000: f64,
    is_fail: bool,
    comment: Option<&str>,
    ex_evidence: GsExEvidence,
    stats: Option<GsLampChartStats>,
) -> CachedScore {
    let lamp_index = gs_lamp_index_from_chart_stats(score_10000, comment, ex_evidence, stats);
    cached_gs_score_from_lamp(score_10000, is_fail, lamp_index, comment)
}

#[inline(always)]
pub fn score_file_shard(hash: &str) -> &str {
    hash.get(..2).unwrap_or("00")
}

pub fn parse_score_file_name(name: &str) -> Option<(&str, i64)> {
    let base = name.strip_suffix(".bin")?;
    let idx_dash = base.rfind('-')?;
    if idx_dash == 0 {
        return None;
    }
    let played_at_ms = base[(idx_dash + 1)..].parse::<i64>().ok()?;
    Some((&base[..idx_dash], played_at_ms))
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

pub const fn grade_to_code(grade: Grade) -> u8 {
    match grade {
        Grade::Quint => 0,
        Grade::Tier01 => 1,
        Grade::Tier02 => 2,
        Grade::Tier03 => 3,
        Grade::Tier04 => 4,
        Grade::Tier05 => 5,
        Grade::Tier06 => 6,
        Grade::Tier07 => 7,
        Grade::Tier08 => 8,
        Grade::Tier09 => 9,
        Grade::Tier10 => 10,
        Grade::Tier11 => 11,
        Grade::Tier12 => 12,
        Grade::Tier13 => 13,
        Grade::Tier14 => 14,
        Grade::Tier15 => 15,
        Grade::Tier16 => 16,
        Grade::Tier17 => 17,
        Grade::Failed => 18,
    }
}

pub const fn grade_from_code(code: u8) -> Grade {
    match code {
        0 => Grade::Quint,
        1 => Grade::Tier01,
        2 => Grade::Tier02,
        3 => Grade::Tier03,
        4 => Grade::Tier04,
        5 => Grade::Tier05,
        6 => Grade::Tier06,
        7 => Grade::Tier07,
        8 => Grade::Tier08,
        9 => Grade::Tier09,
        10 => Grade::Tier10,
        11 => Grade::Tier11,
        12 => Grade::Tier12,
        13 => Grade::Tier13,
        14 => Grade::Tier14,
        15 => Grade::Tier15,
        16 => Grade::Tier16,
        17 => Grade::Tier17,
        _ => Grade::Failed,
    }
}

#[inline(always)]
pub const fn cached_score(
    grade: Grade,
    score_percent: f64,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
) -> CachedScore {
    if matches!(grade, Grade::Failed) {
        CachedScore {
            grade,
            score_percent,
            lamp_index: None,
            lamp_judge_count: None,
        }
    } else {
        CachedScore {
            grade,
            score_percent,
            lamp_index,
            lamp_judge_count,
        }
    }
}

#[inline(always)]
pub fn cached_score_10000(score: &CachedScore) -> f64 {
    score.score_percent * 10000.0
}

#[inline(always)]
pub fn same_score_10000(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a.round() - b.round()).abs() <= 1.0
}

#[inline(always)]
const fn grade_priority(grade: Grade) -> u8 {
    match grade {
        Grade::Quint => 0,
        Grade::Tier01 => 1,
        Grade::Tier02 => 2,
        Grade::Tier03 => 3,
        Grade::Tier04 => 4,
        Grade::Tier05 => 5,
        Grade::Tier06 => 6,
        Grade::Tier07 => 7,
        Grade::Tier08 => 8,
        Grade::Tier09 => 9,
        Grade::Tier10 => 10,
        Grade::Tier11 => 11,
        Grade::Tier12 => 12,
        Grade::Tier13 => 13,
        Grade::Tier14 => 14,
        Grade::Tier15 => 15,
        Grade::Tier16 => 16,
        Grade::Tier17 => 17,
        Grade::Failed => u8::MAX,
    }
}

#[inline(always)]
const fn lamp_priority(lamp_index: Option<u8>) -> u8 {
    match lamp_index {
        Some(idx @ 0..=4) => idx,
        Some(_) | None => u8::MAX,
    }
}

#[inline(always)]
const fn lamp_judge_count_priority(count: Option<u8>) -> u8 {
    match count {
        Some(value) => value,
        None => u8::MAX,
    }
}

#[inline(always)]
pub fn is_better_itg(new: &CachedScore, old: &CachedScore) -> bool {
    match (old.grade == Grade::Failed, new.grade == Grade::Failed) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    if !same_score_10000(cached_score_10000(new), cached_score_10000(old)) {
        return new.score_percent > old.score_percent;
    }

    let new_grade = grade_priority(new.grade);
    let old_grade = grade_priority(old.grade);
    if new_grade != old_grade {
        return new_grade < old_grade;
    }

    let new_lamp = lamp_priority(new.lamp_index);
    let old_lamp = lamp_priority(old.lamp_index);
    if new_lamp != old_lamp {
        return new_lamp < old_lamp;
    }

    let new_count = lamp_judge_count_priority(new.lamp_judge_count);
    let old_count = lamp_judge_count_priority(old.lamp_judge_count);
    new_count < old_count
}

#[inline(always)]
pub fn is_better_scalar_score(
    new_grade: Grade,
    new_percent: f64,
    old_grade: Grade,
    old_percent: f64,
) -> bool {
    match (old_grade == Grade::Failed, new_grade == Grade::Failed) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    new_percent > old_percent
}

#[inline(always)]
pub fn failed_score_override(a: &CachedScore, b: &CachedScore) -> Option<CachedScore> {
    if a.grade == Grade::Failed
        && b.grade != Grade::Failed
        && same_score_10000(cached_score_10000(a), cached_score_10000(b))
    {
        Some(*a)
    } else if b.grade == Grade::Failed
        && a.grade != Grade::Failed
        && same_score_10000(cached_score_10000(a), cached_score_10000(b))
    {
        Some(*b)
    } else {
        None
    }
}

#[inline(always)]
pub fn replaces_stale_quint(
    score: &CachedScore,
    existing: &CachedScore,
    proves_nonquint_ex: bool,
) -> bool {
    proves_nonquint_ex
        && existing.grade == Grade::Quint
        && existing.lamp_index == Some(0)
        && score.grade == Grade::Tier01
        && score.lamp_index == Some(1)
        && same_score_10000(cached_score_10000(score), cached_score_10000(existing))
}

#[inline(always)]
pub fn should_replace_cached_gs_score(
    score: &CachedScore,
    existing: &CachedScore,
    proves_nonquint_ex: bool,
) -> bool {
    replaces_stale_quint(score, existing, proves_nonquint_ex)
        || failed_score_override(score, existing) == Some(*score)
        || is_better_itg(score, existing)
}

#[inline(always)]
const fn non_quint_grade(grade: Grade) -> Grade {
    match grade {
        Grade::Quint => Grade::Tier01,
        _ => grade,
    }
}

#[inline(always)]
fn fix_quint_grade_lamp(grade: Grade, score_percent: f64, lamp_index: Option<u8>) -> Grade {
    if grade == Grade::Failed {
        return Grade::Failed;
    }
    let grade = if grade == Grade::Quint {
        score_to_grade(score_percent.clamp(0.0, 1.0) * 10000.0)
    } else {
        grade
    };
    if lamp_index == Some(0) {
        Grade::Quint
    } else {
        grade
    }
}

#[inline(always)]
pub fn fix_gs_cached_score(score: CachedScore) -> CachedScore {
    cached_score(
        fix_quint_grade_lamp(score.grade, score.score_percent, score.lamp_index),
        score.score_percent,
        score.lamp_index,
        score.lamp_judge_count,
    )
}

#[inline(always)]
pub fn fix_local_ex_grade(grade: Grade, ex_score_percent: f64) -> Grade {
    if grade == Grade::Failed {
        Grade::Failed
    } else {
        promote_quint_grade(non_quint_grade(grade), ex_score_percent)
    }
}

pub const LOCAL_SCORE_VERSION: u16 = 1;
pub const LOCAL_SCORE_INDEX_VERSION: u16 = 2;

#[derive(Debug, Clone, Encode, Decode)]
struct LocalScoreIndexFile {
    version: u16,
    index: LocalScoreIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct LocalReplayEdge {
    pub event_music_time_ns: SongTimeNs,
    pub lane: u8,
    pub pressed: bool,
    // 0 = Keyboard, 1 = Gamepad
    pub source: u8,
}

impl LocalReplayEdge {
    #[inline(always)]
    pub const fn new(
        event_music_time_ns: SongTimeNs,
        lane: u8,
        pressed: bool,
        source: InputSource,
    ) -> Self {
        Self {
            event_music_time_ns,
            lane,
            pressed,
            source: Self::source_code(source),
        }
    }

    #[inline(always)]
    pub const fn input_source(self) -> InputSource {
        match self.source {
            1 => InputSource::Gamepad,
            _ => InputSource::Keyboard,
        }
    }

    #[inline(always)]
    pub const fn source_code(source: InputSource) -> u8 {
        match source {
            InputSource::Keyboard => 0,
            InputSource::Gamepad => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct LocalScoreHeader {
    pub version: u16,
    pub played_at_ms: i64,
    pub music_rate: f32,
    pub score_percent: f64,
    pub grade_code: u8,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    // Fantastic, Excellent, Great, Decent, WayOff, Miss (row judgments)
    pub judgment_counts: [u32; 6],
    pub holds_held: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_total: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    pub hands_achieved: u32,
    pub fail_time: Option<f32>,
    pub beat0_time_ns: SongTimeNs,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct LocalScoreEntry {
    pub version: u16,
    pub played_at_ms: i64,
    pub music_rate: f32,
    pub score_percent: f64,
    pub grade_code: u8,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    pub judgment_counts: [u32; 6],
    pub holds_held: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_total: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    pub hands_achieved: u32,
    pub fail_time: Option<f32>,
    pub beat0_time_ns: SongTimeNs,
    pub replay: Vec<LocalReplayEdge>,
}

impl LocalScoreEntry {
    pub fn header(&self) -> LocalScoreHeader {
        LocalScoreHeader {
            version: self.version,
            played_at_ms: self.played_at_ms,
            music_rate: self.music_rate,
            score_percent: self.score_percent,
            grade_code: self.grade_code,
            lamp_index: self.lamp_index,
            lamp_judge_count: self.lamp_judge_count,
            ex_score_percent: self.ex_score_percent,
            hard_ex_score_percent: self.hard_ex_score_percent,
            judgment_counts: self.judgment_counts,
            holds_held: self.holds_held,
            holds_total: self.holds_total,
            rolls_held: self.rolls_held,
            rolls_total: self.rolls_total,
            mines_avoided: self.mines_avoided,
            mines_total: self.mines_total,
            hands_achieved: self.hands_achieved,
            fail_time: self.fail_time,
            beat0_time_ns: self.beat0_time_ns,
        }
    }
}

#[inline(always)]
fn local_lamp_judge_count(count: u32) -> Option<u8> {
    if (1..=9).contains(&count) {
        Some(count as u8)
    } else {
        None
    }
}

pub fn compute_local_lamp(
    counts: [u32; 6],
    grade: Grade,
    white_fantastics: Option<u32>,
) -> (Option<u8>, Option<u8>) {
    if grade == Grade::Failed {
        return (None, None);
    }
    if grade == Grade::Quint {
        return (Some(0), None);
    }

    let excellent = counts[1];
    let great = counts[2];
    let decent = counts[3];
    let wayoff = counts[4];
    let miss = counts[5];

    if miss == 0 && wayoff == 0 && decent == 0 && great == 0 && excellent == 0 {
        return (Some(1), white_fantastics.and_then(local_lamp_judge_count));
    }
    if miss == 0 && wayoff == 0 && decent == 0 && great == 0 {
        return (Some(2), local_lamp_judge_count(excellent));
    }
    if miss == 0 && wayoff == 0 && decent == 0 {
        return (Some(3), local_lamp_judge_count(great));
    }
    if miss == 0 && wayoff == 0 {
        return (Some(4), local_lamp_judge_count(decent));
    }
    (None, None)
}

#[inline(always)]
const fn local_score_grade(grade_code: u8, has_fail_time: bool) -> Grade {
    if has_fail_time {
        Grade::Failed
    } else {
        grade_from_code(grade_code)
    }
}

#[inline(always)]
pub fn cached_score_from_local_header(h: &LocalScoreHeader) -> CachedScore {
    let grade = fix_local_ex_grade(
        local_score_grade(h.grade_code, h.fail_time.is_some()),
        h.ex_score_percent,
    );
    let (lamp_index, lamp_judge_count) = if grade == Grade::Quint {
        (Some(0), None)
    } else if h.lamp_index == Some(0) {
        compute_local_lamp(h.judgment_counts, grade, None)
    } else {
        (h.lamp_index, h.lamp_judge_count)
    };
    cached_score(grade, h.score_percent, lamp_index, lamp_judge_count)
}

pub fn decode_local_score_header(bytes: &[u8]) -> Option<LocalScoreHeader> {
    let Ok((h, _)) =
        bincode::decode_from_slice::<LocalScoreHeader, _>(bytes, bincode::config::standard())
    else {
        return None;
    };
    if h.version != LOCAL_SCORE_VERSION {
        return None;
    }
    Some(h)
}

pub fn decode_local_score_entry(bytes: &[u8]) -> Option<LocalScoreEntry> {
    let Ok((entry, _)) =
        bincode::decode_from_slice::<LocalScoreEntry, _>(bytes, bincode::config::standard())
    else {
        return None;
    };
    if entry.version != LOCAL_SCORE_VERSION {
        return None;
    }
    Some(entry)
}

pub fn encode_local_score_entry(entry: &LocalScoreEntry) -> Option<Vec<u8>> {
    bincode::encode_to_vec(entry, bincode::config::standard()).ok()
}

pub fn decode_local_score_index(bytes: &[u8]) -> Option<LocalScoreIndex> {
    let (file, _) =
        bincode::decode_from_slice::<LocalScoreIndexFile, _>(bytes, bincode::config::standard())
            .ok()?;
    (file.version == LOCAL_SCORE_INDEX_VERSION).then_some(file.index)
}

pub fn encode_local_score_index(index: &LocalScoreIndex) -> Option<Vec<u8>> {
    bincode::encode_to_vec(
        LocalScoreIndexFile {
            version: LOCAL_SCORE_INDEX_VERSION,
            index: index.clone(),
        },
        bincode::config::standard(),
    )
    .ok()
}

fn update_local_score_best_scalar(
    best_by_chart: &mut HashMap<String, LocalScoreBestScalar>,
    chart_hash: &str,
    new: LocalScoreBestScalar,
) {
    match best_by_chart.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_scalar_score(new.grade, new.percent, existing.grade, existing.percent) {
                *existing = new;
            }
        }
        None => {
            best_by_chart.insert(chart_hash.to_string(), new);
        }
    }
}

pub fn update_local_score_index(
    index: &mut LocalScoreIndex,
    chart_hash: &str,
    header: &LocalScoreHeader,
) {
    let cached = cached_score_from_local_header(header);
    let grade = cached.grade;

    match index.best_itg.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_itg(&cached, existing) {
                *existing = cached;
            }
        }
        None => {
            index.best_itg.insert(chart_hash.to_string(), cached);
        }
    }

    update_local_score_best_scalar(
        &mut index.best_ex,
        chart_hash,
        LocalScoreBestScalar {
            grade,
            percent: header.ex_score_percent,
        },
    );
    update_local_score_best_scalar(
        &mut index.best_hard_ex,
        chart_hash,
        LocalScoreBestScalar {
            grade,
            percent: header.hard_ex_score_percent,
        },
    );
}

#[derive(Debug, Clone, Encode, Decode)]
struct GsScoreEntryV1 {
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    username: String,
    fetched_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct GsScoreEntry {
    pub score_percent: f64,
    pub grade_code: u8,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
    pub username: String,
    pub fetched_at_ms: i64,
}

pub fn gs_score_entry_from_cached(
    score: CachedScore,
    username: &str,
    fetched_at_ms: i64,
) -> GsScoreEntry {
    let score = fix_gs_cached_score(score);
    GsScoreEntry {
        score_percent: score.score_percent,
        grade_code: grade_to_code(score.grade),
        lamp_index: score.lamp_index,
        lamp_judge_count: score.lamp_judge_count,
        username: username.to_string(),
        fetched_at_ms,
    }
}

pub fn cached_score_from_gs_entry(entry: &GsScoreEntry) -> CachedScore {
    fix_gs_cached_score(cached_score(
        grade_from_code(entry.grade_code),
        entry.score_percent,
        entry.lamp_index,
        entry.lamp_judge_count,
    ))
}

pub fn decode_gs_score_entry(bytes: &[u8]) -> Option<GsScoreEntry> {
    if let Ok((entry, _)) =
        bincode::decode_from_slice::<GsScoreEntry, _>(bytes, bincode::config::standard())
    {
        return Some(entry);
    }
    if let Ok((v1, _)) =
        bincode::decode_from_slice::<GsScoreEntryV1, _>(bytes, bincode::config::standard())
    {
        return Some(GsScoreEntry {
            score_percent: v1.score_percent,
            grade_code: v1.grade_code,
            lamp_index: v1.lamp_index,
            lamp_judge_count: None,
            username: v1.username,
            fetched_at_ms: v1.fetched_at_ms,
        });
    }
    None
}

pub fn encode_gs_score_entry(entry: &GsScoreEntry) -> Option<Vec<u8>> {
    bincode::encode_to_vec(entry, bincode::config::standard()).ok()
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

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPlayerScore {
    pub score_10000: f64,
    pub comments: Option<String>,
    pub is_fail: bool,
    pub ex_evidence: GsExEvidence,
}

impl ImportedPlayerScore {
    #[inline(always)]
    pub fn needs_chart_stats(&self) -> bool {
        let score_percent = self.score_10000 / 10000.0;
        (score_percent - 1.0).abs() > 1e-9 && self.comments.is_some()
    }
}

pub fn merge_local_fail(
    mut imported: ImportedPlayerScore,
    local_score: Option<CachedScore>,
) -> ImportedPlayerScore {
    if local_score.is_some_and(|score| {
        score.grade == Grade::Failed
            && same_score_10000(cached_score_10000(&score), imported.score_10000)
    }) {
        imported.is_fail = true;
    }
    imported
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerScoreImportResult {
    pub score: Option<ImportedPlayerScore>,
    pub score_proves_nonquint_ex: bool,
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
    pub itl_self_found: bool,
}

impl PlayerScoreImportResult {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            score: None,
            score_proves_nonquint_ex: false,
            itl_self_score: None,
            itl_self_rank: None,
            itl_self_found: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CachedScoreImportResult {
    pub score: Option<CachedScore>,
    pub score_proves_nonquint_ex: bool,
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
    pub itl_self_found: bool,
}

pub fn cached_score_from_imported_player_score(
    score: ImportedPlayerScore,
    stats: Option<GsLampChartStats>,
) -> CachedScore {
    let ImportedPlayerScore {
        score_10000,
        comments,
        is_fail,
        ex_evidence,
    } = score;
    let stats = if (score_10000 / 10000.0 - 1.0).abs() > 1e-9 && comments.is_some() {
        stats
    } else {
        None
    };
    cached_gs_score_from_chart_stats(
        score_10000,
        is_fail,
        comments.as_deref(),
        ex_evidence,
        stats,
    )
}

pub fn cached_score_import_result_from_imported(
    imported: PlayerScoreImportResult,
    stats: Option<GsLampChartStats>,
) -> CachedScoreImportResult {
    CachedScoreImportResult {
        score: imported
            .score
            .map(|score| cached_score_from_imported_player_score(score, stats)),
        score_proves_nonquint_ex: imported.score_proves_nonquint_ex,
        itl_self_score: imported.itl_self_score,
        itl_self_rank: imported.itl_self_rank,
        itl_self_found: imported.itl_self_found,
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

#[derive(Debug, Clone, Copy)]
pub struct GrooveStatsEvalInput<'a> {
    pub chart_type: &'a str,
    pub music_rate: f32,
    pub remove_mask: u8,
    pub insert_mask: u8,
    pub holds_mask: u8,
    pub fail_type_ok: bool,
    pub autoplay_used: bool,
    pub is_course_mode: bool,
    pub course_submit_allowed: bool,
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
}

// Mirrors zmod's old-api submit path bit layout from gameplay.rs/player options.
pub const GS_INVALID_REMOVE_MASK: u8 =
    (1u8 << 0) | (1u8 << 2) | (1u8 << 3) | (1u8 << 4) | (1u8 << 5) | (1u8 << 6) | (1u8 << 7);
pub const GS_INVALID_INSERT_MASK: u8 = u8::MAX;
pub const GS_INVALID_HOLDS_MASK: u8 = 1u8 << 3;
pub const GROOVESTATS_REASON_COUNT: usize = 13;

pub fn groovestats_reason_lines(
    checks: &[bool; GROOVESTATS_REASON_COUNT],
    bad: &[String],
) -> Vec<String> {
    let mut out = Vec::with_capacity(6);
    for (idx, passed) in checks.iter().enumerate() {
        if *passed {
            continue;
        }
        match idx {
            0 => out.push("GrooveStats only supports dance and pump charts.".to_string()),
            1 => out.push("GrooveStats does not support dance-solo charts.".to_string()),
            2 => out.push("GrooveStats QR is unavailable in course mode.".to_string()),
            3 => out.push("GrooveStats requires ITG mode.".to_string()),
            4 => out.push("Timing windows must be at ITG or harder.".to_string()),
            5 => out.push("Life difficulty must be at ITG or harder.".to_string()),
            6 => {
                out.push("Metrics or preferences are incorrect.".to_string());
                out.extend(bad.iter().cloned());
            }
            7 => out.push("Music rate must be between 1.0x and 3.0x.".to_string()),
            8 => out.push("Note-removal modifiers are enabled.".to_string()),
            9 => out.push("Note-insertion modifiers are enabled.".to_string()),
            10 => out.push("Fail type must be Immediate or ImmediateContinue.".to_string()),
            11 => out.push("Autoplay or replay is not allowed.".to_string()),
            12 => out.push("MinTNSToScoreNotes cannot be W1 or W2.".to_string()),
            _ => {}
        }
    }
    out
}

pub fn groovestats_eval_state_from_parts(input: GrooveStatsEvalInput<'_>) -> GrooveStatsEvalState {
    let chart_type = input.chart_type.trim().to_ascii_lowercase();
    let rate = if input.music_rate.is_finite() && input.music_rate > 0.0 {
        input.music_rate
    } else {
        1.0
    };

    let mut checks = [true; GROOVESTATS_REASON_COUNT];
    checks[0] = chart_type.starts_with("dance") || chart_type.starts_with("pump");
    checks[1] = !chart_type.contains("solo");
    checks[2] = !input.is_course_mode || input.course_submit_allowed;
    checks[3] = true;
    checks[4] = true;
    checks[5] = true;
    checks[6] = !input.custom_fantastic_window;
    checks[7] = (1.0..=3.0).contains(&rate);
    checks[8] = (input.remove_mask & GS_INVALID_REMOVE_MASK) == 0;
    checks[9] = (input.insert_mask & GS_INVALID_INSERT_MASK) == 0;
    checks[10] = input.fail_type_ok;
    checks[11] = !input.autoplay_used;
    checks[12] = true;
    if (input.holds_mask & GS_INVALID_HOLDS_MASK) != 0 {
        checks[8] = false;
    }

    let mut bad = Vec::with_capacity(1);
    if input.custom_fantastic_window {
        bad.push(format!(
            "- Custom Fantastic window ({}ms)",
            input.custom_fantastic_window_ms
        ));
    }

    GrooveStatsEvalState {
        valid: checks.iter().all(|passed| *passed),
        reason_lines: groovestats_reason_lines(&checks, bad.as_slice()),
        manual_qr_url: None,
    }
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

impl GrooveStatsSubmitUiStatus {
    #[inline(always)]
    pub const fn can_retry(self) -> bool {
        matches!(
            self,
            Self::TimedOut | Self::NetworkError | Self::ServerError { .. }
        )
    }

    #[inline(always)]
    pub const fn is_auto_retryable(self) -> bool {
        matches!(self, Self::TimedOut)
    }

    #[inline(always)]
    pub const fn from_http_status(status_code: u16) -> Self {
        match status_code {
            408 | 504 => Self::TimedOut,
            500..=599 => Self::ServerError {
                http_status: status_code,
            },
            401 | 403 => Self::Rejected {
                reason: RejectReason::Unauthorized,
            },
            404 => Self::Rejected {
                reason: RejectReason::NotFound,
            },
            _ => Self::Rejected {
                reason: RejectReason::InvalidScore,
            },
        }
    }
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

impl ArrowCloudSubmitUiStatus {
    #[inline(always)]
    pub const fn can_retry(self) -> bool {
        matches!(
            self,
            Self::TimedOut | Self::NetworkError | Self::ServerError { .. }
        )
    }

    #[inline(always)]
    pub const fn is_auto_retryable(self) -> bool {
        matches!(self, Self::TimedOut)
    }

    #[inline(always)]
    pub const fn from_http_status(status_code: u16) -> Self {
        match status_code {
            408 | 504 => Self::TimedOut,
            500..=599 => Self::ServerError {
                http_status: status_code,
            },
            401 | 403 => Self::Rejected {
                reason: RejectReason::Unauthorized,
            },
            404 => Self::Rejected {
                reason: RejectReason::NotFound,
            },
            _ => Self::Rejected {
                reason: RejectReason::InvalidScore,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrooveStatsSubmitRecordBanner {
    PersonalBest,
    WorldRecord,
    WorldRecordEx,
}

#[inline(always)]
fn submit_result_improved(result: &str) -> bool {
    result.eq_ignore_ascii_case("score-added") || result.eq_ignore_ascii_case("improved")
}

pub fn groovestats_submit_record_banner(
    result: &str,
    show_ex_score: bool,
    gs_self_rank: Option<u32>,
    ex_self_rank: Option<u32>,
    ex_has_entries: bool,
) -> Option<GrooveStatsSubmitRecordBanner> {
    if !submit_result_improved(result) {
        return None;
    }
    if show_ex_score && ex_has_entries {
        return Some(if ex_self_rank == Some(1) {
            GrooveStatsSubmitRecordBanner::WorldRecordEx
        } else {
            GrooveStatsSubmitRecordBanner::PersonalBest
        });
    }
    Some(if gs_self_rank == Some(1) {
        GrooveStatsSubmitRecordBanner::WorldRecord
    } else {
        GrooveStatsSubmitRecordBanner::PersonalBest
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventProgressKind {
    #[default]
    Itl,
    Srpg,
}

#[derive(Clone, Debug)]
pub struct EventStatImprovement {
    pub name: String,
    pub gained: u32,
    pub current: i32,
}

#[derive(Clone, Debug)]
pub enum EventOverlayPage {
    Text(String),
    Leaderboard(Vec<LeaderboardEntry>),
}

#[derive(Clone, Debug, Default)]
pub struct EventProgress {
    pub kind: EventProgressKind,
    pub name: String,
    pub is_doubles: bool,
    pub score_hundredths: u32,
    pub score_delta_hundredths: i32,
    pub rate_hundredths: Option<u32>,
    pub rate_delta_hundredths: Option<i32>,
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
    pub stat_improvements: Vec<EventStatImprovement>,
    pub overlay_pages: Vec<EventOverlayPage>,
}

pub type ItlEventProgress = EventProgress;
pub type ItlOverlayPage = EventOverlayPage;

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
    fn grade_storage_codes_round_trip_stable_variants() {
        for (grade, code) in [
            (Grade::Quint, 0),
            (Grade::Tier01, 1),
            (Grade::Tier02, 2),
            (Grade::Tier03, 3),
            (Grade::Tier04, 4),
            (Grade::Tier05, 5),
            (Grade::Tier06, 6),
            (Grade::Tier07, 7),
            (Grade::Tier08, 8),
            (Grade::Tier09, 9),
            (Grade::Tier10, 10),
            (Grade::Tier11, 11),
            (Grade::Tier12, 12),
            (Grade::Tier13, 13),
            (Grade::Tier14, 14),
            (Grade::Tier15, 15),
            (Grade::Tier16, 16),
            (Grade::Tier17, 17),
            (Grade::Failed, 18),
        ] {
            assert_eq!(grade_to_code(grade), code);
            assert_eq!(grade_from_code(code), grade);
        }
        assert_eq!(grade_from_code(255), Grade::Failed);
    }

    #[test]
    fn cached_score_clears_lamp_fields_for_failed_runs() {
        let score = cached_score(Grade::Failed, 0.91, Some(3), Some(2));
        assert_eq!(score.grade, Grade::Failed);
        assert_eq!(score.score_percent, 0.91);
        assert_eq!(score.lamp_index, None);
        assert_eq!(score.lamp_judge_count, None);
    }

    #[test]
    fn itg_score_order_prefers_richer_tied_score() {
        let quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
        };
        let quint = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        let bland_quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: None,
        };

        assert!(is_better_itg(&quint, &quad));
        assert!(is_better_itg(&quad, &bland_quad));
        assert!(!is_better_itg(&bland_quad, &quad));
    }

    #[test]
    fn stale_quint_replacement_requires_nonperfect_ex() {
        let stale_quint = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        let corrected_quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: None,
        };

        assert!(replaces_stale_quint(&corrected_quad, &stale_quint, true));
        assert!(!replaces_stale_quint(&corrected_quad, &stale_quint, false));
        assert!(!is_better_itg(&corrected_quad, &stale_quint));
    }

    #[test]
    fn failed_score_override_preserves_known_failed_run() {
        let failed = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        let passed = CachedScore {
            grade: Grade::Tier17,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };

        assert_eq!(failed_score_override(&failed, &passed), Some(failed));
        assert_eq!(failed_score_override(&passed, &failed), Some(failed));
    }

    #[test]
    fn cached_gs_replacement_policy_allows_score_corrections() {
        let stale_quint = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        let corrected_quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: None,
        };
        assert!(should_replace_cached_gs_score(
            &corrected_quad,
            &stale_quint,
            true
        ));
        assert!(!should_replace_cached_gs_score(
            &corrected_quad,
            &stale_quint,
            false
        ));

        let passed = CachedScore {
            grade: Grade::Tier17,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        let failed = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        assert!(should_replace_cached_gs_score(&failed, &passed, false));

        let better_existing = CachedScore {
            grade: Grade::Tier10,
            score_percent: 0.91,
            lamp_index: None,
            lamp_judge_count: None,
        };
        let worse = CachedScore {
            grade: Grade::Tier12,
            score_percent: 0.75,
            lamp_index: None,
            lamp_judge_count: None,
        };
        assert!(!should_replace_cached_gs_score(
            &worse,
            &better_existing,
            false
        ));
    }

    #[test]
    fn gs_cached_score_recomputes_stale_quint_from_lamp() {
        let stale = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
        };
        assert_eq!(fix_gs_cached_score(stale).grade, Grade::Tier01);

        let quint_lamp = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        assert_eq!(fix_gs_cached_score(quint_lamp).grade, Grade::Quint);
    }

    #[test]
    fn gs_score_entry_roundtrip_preserves_lamp_judge_count() {
        let score = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
        };

        let entry = gs_score_entry_from_cached(score, "Player", 1234);
        assert_eq!(entry.grade_code, grade_to_code(Grade::Tier01));
        assert_eq!(entry.username, "Player");
        assert_eq!(entry.fetched_at_ms, 1234);

        let bytes = encode_gs_score_entry(&entry).expect("GS entry should encode");
        let decoded = decode_gs_score_entry(&bytes).expect("GS entry should decode");
        assert_eq!(decoded, entry);
        assert_eq!(cached_score_from_gs_entry(&decoded), score);
    }

    #[test]
    fn gs_score_entry_decode_accepts_legacy_payload_without_judge_count() {
        let bytes = bincode::encode_to_vec(
            GsScoreEntryV1 {
                score_percent: 0.985,
                grade_code: grade_to_code(Grade::Tier03),
                lamp_index: Some(2),
                username: "Legacy".to_string(),
                fetched_at_ms: 4321,
            },
            bincode::config::standard(),
        )
        .expect("legacy GS entry should encode");

        let entry = decode_gs_score_entry(&bytes).expect("legacy GS entry should decode");
        assert_eq!(entry.score_percent, 0.985);
        assert_eq!(entry.grade_code, grade_to_code(Grade::Tier03));
        assert_eq!(entry.lamp_index, Some(2));
        assert_eq!(entry.lamp_judge_count, None);
        assert_eq!(entry.username, "Legacy");
        assert_eq!(entry.fetched_at_ms, 4321);
    }

    #[test]
    fn local_ex_grade_promotion_ignores_stale_quint_without_perfect_ex() {
        assert_eq!(fix_local_ex_grade(Grade::Quint, 99.71), Grade::Tier01);
        assert_eq!(fix_local_ex_grade(Grade::Tier01, 100.0), Grade::Quint);
        assert_eq!(fix_local_ex_grade(Grade::Failed, 100.0), Grade::Failed);
    }

    #[test]
    fn local_lamp_uses_white_fantastics_for_quad() {
        assert_eq!(
            compute_local_lamp([12, 0, 0, 0, 0, 0], Grade::Tier01, Some(5)),
            (Some(1), Some(5))
        );
        assert_eq!(
            compute_local_lamp([12, 0, 0, 0, 0, 0], Grade::Quint, Some(0)),
            (Some(0), None)
        );
    }

    #[test]
    fn cached_score_from_local_header_treats_fail_time_as_failed() {
        let header = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 0,
            music_rate: 1.0,
            score_percent: 0.9482,
            grade_code: grade_to_code(Grade::Tier06),
            lamp_index: Some(4),
            lamp_judge_count: Some(2),
            ex_score_percent: 92.0,
            hard_ex_score_percent: 88.0,
            judgment_counts: [0; 6],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: Some(12.0),
            beat0_time_ns: 0,
        };

        let cached = cached_score_from_local_header(&header);

        assert_eq!(cached.grade, Grade::Failed);
        assert_eq!(cached.score_percent, 0.9482);
        assert_eq!(cached.lamp_index, None);
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_local_header_downgrades_stale_quint() {
        let header = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 0,
            music_rate: 1.0,
            score_percent: 1.0,
            grade_code: grade_to_code(Grade::Quint),
            lamp_index: Some(0),
            lamp_judge_count: None,
            ex_score_percent: 99.71,
            hard_ex_score_percent: 99.71,
            judgment_counts: [320, 0, 0, 0, 0, 0],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: None,
            beat0_time_ns: 0,
        };

        let cached = cached_score_from_local_header(&header);

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_local_header_promotes_perfect_ex_to_quint() {
        let header = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 0,
            music_rate: 1.0,
            score_percent: 1.0,
            grade_code: grade_to_code(Grade::Tier01),
            lamp_index: Some(1),
            lamp_judge_count: Some(2),
            ex_score_percent: 100.0,
            hard_ex_score_percent: 100.0,
            judgment_counts: [320, 0, 0, 0, 0, 0],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: None,
            beat0_time_ns: 0,
        };

        let cached = cached_score_from_local_header(&header);

        assert_eq!(cached.grade, Grade::Quint);
        assert_eq!(cached.lamp_index, Some(0));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn local_score_entry_roundtrip_decodes_header_prefix() {
        let entry = LocalScoreEntry {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 1234,
            music_rate: 1.1,
            score_percent: 0.9876,
            grade_code: grade_to_code(Grade::Tier03),
            lamp_index: Some(2),
            lamp_judge_count: Some(4),
            ex_score_percent: 98.25,
            hard_ex_score_percent: 97.75,
            judgment_counts: [100, 4, 3, 2, 1, 0],
            holds_held: 5,
            holds_total: 6,
            rolls_held: 7,
            rolls_total: 8,
            mines_avoided: 9,
            mines_total: 10,
            hands_achieved: 11,
            fail_time: None,
            beat0_time_ns: -250_000_000,
            replay: vec![LocalReplayEdge::new(
                1_500_000_000,
                2,
                true,
                InputSource::Gamepad,
            )],
        };

        let bytes = encode_local_score_entry(&entry).expect("local score should encode");
        let decoded = decode_local_score_entry(&bytes).expect("local score should decode");
        let header = decode_local_score_header(&bytes).expect("local header should decode");

        assert_eq!(decoded, entry);
        assert_eq!(header, entry.header());
        assert_eq!(decoded.replay[0].input_source(), InputSource::Gamepad);
    }

    #[test]
    fn local_score_index_update_tracks_each_score_type() {
        let older = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 1,
            music_rate: 1.0,
            score_percent: 0.9500,
            grade_code: grade_to_code(Grade::Tier06),
            lamp_index: Some(3),
            lamp_judge_count: Some(2),
            ex_score_percent: 94.0,
            hard_ex_score_percent: 91.0,
            judgment_counts: [100, 1, 2, 0, 0, 0],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: None,
            beat0_time_ns: 0,
        };
        let mut newer = older;
        newer.score_percent = 0.9900;
        newer.grade_code = grade_to_code(Grade::Tier02);
        newer.ex_score_percent = 93.0;
        newer.hard_ex_score_percent = 96.0;

        let mut index = LocalScoreIndex::default();
        update_local_score_index(&mut index, "deadbeef", &older);
        update_local_score_index(&mut index, "deadbeef", &newer);

        assert_eq!(index.best_itg["deadbeef"].score_percent, 0.9900);
        assert_eq!(index.best_ex["deadbeef"].percent, 94.0);
        assert_eq!(index.best_hard_ex["deadbeef"].percent, 96.0);
    }

    #[test]
    fn local_score_index_roundtrip_rejects_wrong_version() {
        let mut index = LocalScoreIndex::default();
        index.best_itg.insert(
            "deadbeef".to_string(),
            cached_score(Grade::Tier04, 0.975, Some(3), Some(2)),
        );

        let bytes = encode_local_score_index(&index).expect("local index should encode");
        let decoded = decode_local_score_index(&bytes).expect("local index should decode");
        assert_eq!(decoded, index);

        let wrong_version = bincode::encode_to_vec(
            LocalScoreIndexFile {
                version: LOCAL_SCORE_INDEX_VERSION + 1,
                index,
            },
            bincode::config::standard(),
        )
        .expect("wrong-version local index should encode");
        assert_eq!(decode_local_score_index(&wrong_version), None);
    }

    #[test]
    fn leaderboard_rank_places_current_run_before_equal_scores() {
        let entries = [entry(9800.0), entry(9750.0), entry(9700.0)];
        assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
        assert_eq!(leaderboard_rank_for_score(&entries, f64::NAN), None);
    }

    #[test]
    fn leaderboard_rank_uses_scores_not_identity() {
        let mut top = entry(9999.0);
        top.name = "AAA".to_string();
        let mut target = entry(9750.0);
        target.name = "BBB".to_string();
        let entries = [top, target];

        assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
    }

    #[test]
    fn leaderboard_identity_helpers_match_self_and_username() {
        assert!(leaderboard_username_matches("PerfectTaste", "perfecttaste"));
        assert!(!leaderboard_username_matches("PerfectTaste", " "));
        assert_eq!(leaderboard_nonzero_rank(25), Some(25));
        assert_eq!(leaderboard_nonzero_rank(0), None);
    }

    #[test]
    fn leaderboard_score_10000_clamps_and_drops_failed_scores() {
        assert_eq!(leaderboard_score_10000(10_001.2, false), Some(10_000.0));
        assert_eq!(leaderboard_score_10000(-1.0, false), Some(0.0));
        assert_eq!(leaderboard_score_10000(9_500.0, true), None);
        assert_eq!(leaderboard_score_10000(f64::NAN, false), None);
    }

    #[test]
    fn score_import_profile_match_uses_endpoint_rules() {
        assert!(score_import_entry_matches_profile(
            "Someone",
            true,
            ScoreImportEndpoint::ArrowCloud,
            ""
        ));
        assert!(score_import_entry_matches_profile(
            "PerfectTaste",
            false,
            ScoreImportEndpoint::GrooveStats,
            "perfecttaste"
        ));
        assert!(!score_import_entry_matches_profile(
            "PerfectTaste",
            false,
            ScoreImportEndpoint::ArrowCloud,
            "perfecttaste"
        ));
    }

    #[test]
    fn machine_leaderboard_entries_sort_and_limit_local_plays() {
        let entries = machine_leaderboard_entries(
            vec![
                MachineLeaderboardPlay {
                    name: "Bob".to_string(),
                    machine_tag: None,
                    score_percent: 0.99,
                    played_at_ms: 10,
                    is_fail: false,
                },
                MachineLeaderboardPlay {
                    name: "Cara".to_string(),
                    machine_tag: Some("C".to_string()),
                    score_percent: 0.99,
                    played_at_ms: 30,
                    is_fail: true,
                },
                MachineLeaderboardPlay {
                    name: "Alice".to_string(),
                    machine_tag: None,
                    score_percent: 0.97,
                    played_at_ms: 20,
                    is_fail: false,
                },
            ],
            2,
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].rank, 1);
        assert_eq!(entries[0].name, "Cara");
        assert_eq!(entries[0].machine_tag.as_deref(), Some("C"));
        assert_eq!(entries[0].score, 9900.0);
        assert!(entries[0].is_fail);
        assert_eq!(entries[1].rank, 2);
        assert_eq!(entries[1].name, "Bob");
    }

    #[test]
    fn machine_replay_entries_sort_and_drop_invalid_edges() {
        let entries = machine_replay_entries(
            vec![
                MachineReplayPlay {
                    initials: "AAA".to_string(),
                    score_percent: 0.91,
                    played_at_ms: 30,
                    is_fail: false,
                    replay_beat0_time_ns: 0,
                    replay: Vec::new(),
                },
                MachineReplayPlay {
                    initials: "BBB".to_string(),
                    score_percent: 0.98,
                    played_at_ms: 10,
                    is_fail: false,
                    replay_beat0_time_ns: -250_000_000,
                    replay: vec![
                        LocalReplayEdge::new(
                            deadsync_core::song_time::INVALID_SONG_TIME_NS,
                            1,
                            true,
                            InputSource::Keyboard,
                        ),
                        LocalReplayEdge::new(1_250_000_000, 2, false, InputSource::Gamepad),
                    ],
                },
            ],
            1,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rank, 1);
        assert_eq!(entries[0].name, "BBB");
        assert_eq!(entries[0].score, 9800.0);
        assert_eq!(entries[0].replay_beat0_time_ns, -250_000_000);
        assert_eq!(entries[0].replay.len(), 1);
        assert_eq!(entries[0].replay[0].lane_index, 2);
        assert_eq!(entries[0].replay[0].source, InputSource::Gamepad);
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
    fn leaderboard_pane_skips_empty_entries() {
        assert!(leaderboard_pane("GrooveStats", Vec::new(), false).is_none());
    }

    #[test]
    fn leaderboard_pane_builds_default_online_metadata() {
        let pane = leaderboard_pane("GrooveStats", vec![entry(9876.0)], true).unwrap();

        assert_eq!(pane.name, "GrooveStats");
        assert_eq!(pane.entries.len(), 1);
        assert_eq!(pane.entries[0].score, 9876.0);
        assert!(pane.is_ex);
        assert!(!pane.disabled);
        assert!(pane.personalized);
        assert_eq!(pane.arrowcloud_kind, None);
    }

    #[test]
    fn arrowcloud_pane_kind_accepts_server_variants() {
        assert_eq!(
            arrowcloud_pane_kind_from_type("HardEX"),
            Some(ArrowCloudPaneKind::HardEx)
        );
        assert_eq!(
            arrowcloud_pane_kind_from_type("hard-ex"),
            Some(ArrowCloudPaneKind::HardEx)
        );
        assert_eq!(
            arrowcloud_pane_kind_from_type("HEX"),
            Some(ArrowCloudPaneKind::HardEx)
        );
        assert_eq!(
            arrowcloud_pane_kind_from_type("EX"),
            Some(ArrowCloudPaneKind::Ex)
        );
        assert_eq!(arrowcloud_pane_kind_from_type("event"), None);
    }

    #[test]
    fn arrowcloud_user_context_marks_self_and_rivals() {
        let context = ArrowCloudUserContext {
            self_user_id: Some("self".to_string()),
            rival_user_ids: HashSet::from([String::from("rival")]),
        };
        assert_eq!(arrowcloud_user_id("  self  "), Some("self"));
        assert_eq!(arrowcloud_user_id("   "), None);

        let targets = arrowcloud_target_user_ids(Some(&context));
        assert!(targets.contains("self"));
        assert!(targets.contains("rival"));

        assert_eq!(
            arrowcloud_entry_flags(Some("self"), false, false, Some(&context)),
            (true, false)
        );
        assert_eq!(
            arrowcloud_entry_flags(Some("rival"), false, false, Some(&context)),
            (false, true)
        );
        assert_eq!(
            arrowcloud_entry_flags(None, true, true, Some(&context)),
            (true, true)
        );
    }

    #[test]
    fn arrowcloud_leaderboard_entry_normalizes_percent_score() {
        let entry = arrowcloud_leaderboard_entry(
            12,
            "YOU".to_string(),
            98.31,
            "2026-05-03T19:10:17.504Z".to_string(),
            true,
            false,
        );

        assert_eq!(entry.rank, 12);
        assert_eq!(entry.name, "YOU");
        assert_eq!(entry.score, 9831.0);
        assert!(entry.is_self);
        assert!(!entry.is_rival);
        assert!(!entry.is_fail);

        let invalid = arrowcloud_leaderboard_entry(
            1,
            "BAD".to_string(),
            f64::NAN,
            String::new(),
            false,
            true,
        );
        assert_eq!(invalid.score, 0.0);
        assert!(invalid.is_rival);
    }

    #[test]
    fn arrowcloud_hard_ex_pane_helpers_set_stable_metadata() {
        let pane = arrowcloud_hard_ex_leaderboard_pane(vec![entry(9980.0)], true);
        assert_eq!(pane.name, "ArrowCloud");
        assert!(pane.personalized);
        assert!(!pane.is_ex);
        assert!(pane.is_arrowcloud());
        assert!(pane.is_hard_ex());
        assert_eq!(pane.entries.len(), 1);

        let empty = arrowcloud_empty_hard_ex_leaderboard_pane();
        assert!(empty.entries.is_empty());
        assert!(!empty.personalized);
        assert!(empty.is_arrowcloud());
        assert!(empty.is_hard_ex());
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
    fn groovestats_submit_status_policy_classifies_retry_and_http() {
        assert!(!GrooveStatsSubmitUiStatus::Submitting.can_retry());
        assert!(!GrooveStatsSubmitUiStatus::Submitted.can_retry());
        assert!(GrooveStatsSubmitUiStatus::TimedOut.can_retry());
        assert!(GrooveStatsSubmitUiStatus::NetworkError.can_retry());
        assert!(GrooveStatsSubmitUiStatus::ServerError { http_status: 500 }.can_retry());
        assert!(
            !GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
            .can_retry()
        );
        assert!(GrooveStatsSubmitUiStatus::TimedOut.is_auto_retryable());
        assert!(!GrooveStatsSubmitUiStatus::NetworkError.is_auto_retryable());

        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(408),
            GrooveStatsSubmitUiStatus::TimedOut
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(503),
            GrooveStatsSubmitUiStatus::ServerError { http_status: 503 }
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(401),
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(404),
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::NotFound,
            }
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(400),
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
        );
    }

    #[test]
    fn groovestats_eval_masks_match_old_submit_bits() {
        assert_eq!(
            GS_INVALID_REMOVE_MASK,
            (1u8 << 0)
                | (1u8 << 2)
                | (1u8 << 3)
                | (1u8 << 4)
                | (1u8 << 5)
                | (1u8 << 6)
                | (1u8 << 7)
        );
        assert_eq!(GS_INVALID_INSERT_MASK, u8::MAX);
        assert_eq!(GS_INVALID_HOLDS_MASK, 1u8 << 3);
    }

    #[test]
    fn groovestats_reason_lines_keep_legacy_order_and_details() {
        let mut checks = [true; GROOVESTATS_REASON_COUNT];
        checks[1] = false;
        checks[6] = false;
        checks[7] = false;
        let reasons =
            groovestats_reason_lines(&checks, &[String::from("- Custom Fantastic window (18ms)")]);

        assert_eq!(
            reasons,
            vec![
                "GrooveStats does not support dance-solo charts.",
                "Metrics or preferences are incorrect.",
                "- Custom Fantastic window (18ms)",
                "Music rate must be between 1.0x and 3.0x.",
            ]
        );
    }

    #[test]
    fn groovestats_eval_state_from_parts_accepts_valid_play() {
        let state = groovestats_eval_state_from_parts(GrooveStatsEvalInput {
            chart_type: "dance-single",
            music_rate: 1.5,
            remove_mask: 1u8 << 1,
            insert_mask: 0,
            holds_mask: 0,
            fail_type_ok: true,
            autoplay_used: false,
            is_course_mode: false,
            course_submit_allowed: false,
            custom_fantastic_window: false,
            custom_fantastic_window_ms: 10,
        });

        assert!(state.valid);
        assert!(state.reason_lines.is_empty());
        assert!(state.manual_qr_url.is_none());
    }

    #[test]
    fn groovestats_eval_state_from_parts_reports_policy_reasons() {
        let state = groovestats_eval_state_from_parts(GrooveStatsEvalInput {
            chart_type: "dance-solo",
            music_rate: 0.75,
            remove_mask: 0,
            insert_mask: 0,
            holds_mask: 0,
            fail_type_ok: false,
            autoplay_used: true,
            is_course_mode: true,
            course_submit_allowed: false,
            custom_fantastic_window: true,
            custom_fantastic_window_ms: 18,
        });

        assert!(!state.valid);
        assert_eq!(
            state.reason_lines,
            vec![
                "GrooveStats does not support dance-solo charts.",
                "GrooveStats QR is unavailable in course mode.",
                "Metrics or preferences are incorrect.",
                "- Custom Fantastic window (18ms)",
                "Music rate must be between 1.0x and 3.0x.",
                "Fail type must be Immediate or ImmediateContinue.",
                "Autoplay or replay is not allowed.",
            ]
        );
    }

    #[test]
    fn arrowcloud_submit_status_policy_matches_shared_retry_rules() {
        assert!(!ArrowCloudSubmitUiStatus::Submitting.can_retry());
        assert!(!ArrowCloudSubmitUiStatus::Submitted.can_retry());
        assert!(ArrowCloudSubmitUiStatus::TimedOut.can_retry());
        assert!(ArrowCloudSubmitUiStatus::NetworkError.can_retry());
        assert!(ArrowCloudSubmitUiStatus::ServerError { http_status: 500 }.can_retry());
        assert!(
            !ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
            .can_retry()
        );
        assert!(ArrowCloudSubmitUiStatus::TimedOut.is_auto_retryable());
        assert!(!ArrowCloudSubmitUiStatus::ServerError { http_status: 500 }.is_auto_retryable());

        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(504),
            ArrowCloudSubmitUiStatus::TimedOut
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(500),
            ArrowCloudSubmitUiStatus::ServerError { http_status: 500 }
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(403),
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(404),
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::NotFound,
            }
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(418),
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
        );
    }

    #[test]
    fn groovestats_submit_banner_prefers_selected_world_record() {
        assert_eq!(
            groovestats_submit_record_banner("improved", false, Some(1), Some(4), true),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord)
        );
        assert_eq!(
            groovestats_submit_record_banner("score-added", true, Some(2), Some(1), true),
            Some(GrooveStatsSubmitRecordBanner::WorldRecordEx)
        );
    }

    #[test]
    fn groovestats_submit_banner_falls_back_to_personal_best() {
        assert_eq!(
            groovestats_submit_record_banner("improved", true, Some(1), None, true),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest)
        );
        assert_eq!(
            groovestats_submit_record_banner("improved", true, Some(1), None, false),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord)
        );
    }

    #[test]
    fn groovestats_submit_banner_ignores_non_improving_results() {
        assert_eq!(
            groovestats_submit_record_banner(
                "score-already-submitted",
                false,
                Some(1),
                None,
                false
            ),
            None
        );
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

    fn ac_score(percent_0_1: f64, is_fail: bool) -> ArrowCloudScore {
        ArrowCloudScore {
            score_percent: percent_0_1,
            server_grade: None,
            played_at: None,
            play_id: None,
            is_fail,
        }
    }

    #[test]
    fn arrowcloud_retrieve_fields_preserve_native_metadata() {
        let score = arrowcloud_score_from_retrieve_fields(
            Some(99.89),
            Some("Tristar"),
            Some("2026-05-03T19:10:17.504Z"),
            Some(12345),
            false,
        )
        .expect("score should decode");

        assert!((score.score_percent - 0.9989).abs() < 1e-6);
        assert_eq!(score.server_grade, Some(ArrowCloudServerGrade::Tristar));
        assert_eq!(score.play_id, Some(12345));
        assert_eq!(
            score
                .played_at
                .expect("played_at parsed")
                .timestamp_millis(),
            1_777_835_417_504
        );
    }

    #[test]
    fn arrowcloud_retrieve_fields_drop_missing_score_and_bad_metadata() {
        assert!(
            arrowcloud_score_from_retrieve_fields(None, Some("Tristar"), None, None, false)
                .is_none()
        );

        let score = arrowcloud_score_from_retrieve_fields(
            Some(125.0),
            Some("Mythic"),
            Some("not-a-date"),
            None,
            true,
        )
        .expect("score should decode");

        assert_eq!(score.score_percent, 1.0);
        assert_eq!(score.server_grade, None);
        assert_eq!(score.played_at, None);
        assert!(score.is_fail);
    }

    #[test]
    fn arrowcloud_submit_percent_clamps_and_rejects_nan() {
        let submitted_at = Utc.timestamp_millis_opt(1_777_835_417_504).unwrap();
        let score = arrowcloud_score_from_submit_percent(120.0, false, submitted_at)
            .expect("finite submit percent should decode");

        assert_eq!(score.score_percent, 1.0);
        assert_eq!(score.server_grade, None);
        assert_eq!(score.played_at, Some(submitted_at));
        assert!(arrowcloud_score_from_submit_percent(f64::NAN, false, submitted_at).is_none());
    }

    #[test]
    fn arrowcloud_leaderboard_slot_assignment_ignores_unknown_ids() {
        let mut scores = ArrowCloudScores::default();

        assert!(set_arrowcloud_score_for_leaderboard(
            &mut scores,
            ArrowCloudLeaderboard::HardEx.id(),
            ac_score(0.9951, false)
        ));
        assert!(set_arrowcloud_score_for_leaderboard(
            &mut scores,
            ArrowCloudLeaderboard::Ex.id(),
            ac_score(0.9810, false)
        ));
        assert!(set_arrowcloud_score_for_leaderboard(
            &mut scores,
            ArrowCloudLeaderboard::Itg.id(),
            ac_score(0.9989, false)
        ));
        assert!(!set_arrowcloud_score_for_leaderboard(
            &mut scores,
            9,
            ac_score(0.9500, false)
        ));

        assert_eq!(scores.itg.unwrap().score_percent, 0.9989);
        assert_eq!(scores.ex.unwrap().score_percent, 0.9810);
        assert_eq!(scores.hard_ex.unwrap().score_percent, 0.9951);
    }

    #[test]
    fn arrowcloud_score_slot_merge_keeps_best_non_failed_score() {
        let mut slot = Some(ac_score(0.85, false));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.95, true)));
        assert!(!slot.as_ref().unwrap().is_fail);

        let mut slot = Some(ac_score(0.85, true));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.50, false)));
        assert_eq!(slot.as_ref().unwrap().score_percent, 0.50);
        assert!(!slot.as_ref().unwrap().is_fail);

        let mut slot = Some(ac_score(0.90, false));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.92, false)));
        assert_eq!(slot.as_ref().unwrap().score_percent, 0.92);
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.91, false)));
        assert_eq!(slot.as_ref().unwrap().score_percent, 0.92);
    }

    fn ac_judgment(grade: judgment::JudgeGrade, offset_ms: f32) -> judgment::Judgment {
        judgment::Judgment {
            time_error_ms: offset_ms,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(offset_ms, 1.0),
            grade,
            window: match grade {
                judgment::JudgeGrade::Fantastic => Some(judgment::TimingWindow::W0),
                judgment::JudgeGrade::Excellent => Some(judgment::TimingWindow::W2),
                judgment::JudgeGrade::Great => Some(judgment::TimingWindow::W3),
                judgment::JudgeGrade::Decent => Some(judgment::TimingWindow::W4),
                judgment::JudgeGrade::WayOff => Some(judgment::TimingWindow::W5),
                judgment::JudgeGrade::Miss => None,
            },
            miss_because_held: false,
        }
    }

    fn ac_note(
        row_index: usize,
        note_type: NoteType,
        result: Option<judgment::Judgment>,
        hold_result: Option<HoldResult>,
        mine_result: Option<MineResult>,
    ) -> Note {
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column: 0,
            note_type,
            row_index,
            result,
            early_result: None,
            hold: hold_result.map(|result| deadsync_rules::note::HoldData {
                end_row_index: row_index,
                end_beat: row_index as f32,
                result: Some(result),
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 1.0,
                last_held_row_index: row_index,
                last_held_beat: row_index as f32,
            }),
            mine_result,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[test]
    fn arrowcloud_submit_stats_caps_failed_runs_at_fail_time() {
        let ns = deadsync_core::song_time::song_time_ns_from_seconds;
        let notes = vec![
            ac_note(
                0,
                NoteType::Tap,
                Some(ac_judgment(judgment::JudgeGrade::Fantastic, 5.0)),
                None,
                None,
            ),
            ac_note(
                1,
                NoteType::Hold,
                Some(ac_judgment(judgment::JudgeGrade::Fantastic, 8.0)),
                Some(HoldResult::Held),
                None,
            ),
            ac_note(
                2,
                NoteType::Roll,
                Some(ac_judgment(judgment::JudgeGrade::Fantastic, 10.0)),
                Some(HoldResult::Held),
                None,
            ),
            ac_note(3, NoteType::Mine, None, None, Some(MineResult::Hit)),
            ac_note(
                4,
                NoteType::Tap,
                Some(ac_judgment(judgment::JudgeGrade::Miss, 180.0)),
                None,
                None,
            ),
            ac_note(
                5,
                NoteType::Tap,
                Some(ac_judgment(judgment::JudgeGrade::Great, 55.0)),
                None,
                None,
            ),
            ac_note(6, NoteType::Mine, None, None, Some(MineResult::Hit)),
        ];
        let note_times = vec![
            ns(1.0),
            ns(1.2),
            ns(1.4),
            ns(1.5),
            ns(2.0),
            ns(3.0),
            ns(3.5),
        ];
        let hold_end_times = vec![None, Some(ns(1.8)), Some(ns(2.4)), None, None, None, None];

        let stats = arrowcloud_submit_stats_from_results(
            &notes,
            &note_times,
            &hold_end_times,
            Some(ns(2.0)),
        );

        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)],
            3
        );
        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Miss)],
            1
        );
        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Great)],
            0
        );
        assert_eq!(stats.window_counts.w0, 3);
        assert_eq!(stats.window_counts.miss, 1);
        assert_eq!(stats.holds_held, 1);
        assert_eq!(stats.rolls_held, 0);
        assert_eq!(stats.mines_hit, 1);
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
    fn groovestats_comment_counts_ignore_ds_prefix() {
        let counts = parse_gs_comment_counts("[DS], 15e, 2g, 2m");
        assert_eq!(counts.e, 15);
        assert_eq!(counts.g, 2);
        assert_eq!(counts.m, 2);
    }

    #[test]
    fn groovestats_comment_ex_percent_accepts_ds_formats() {
        assert_eq!(
            parse_gs_comment_ex_percent("[DS], FA+, 99.78EX"),
            Some(99.78)
        );
        assert_eq!(
            parse_gs_comment_ex_percent("[DS], FA+, 99.78% EX, C650"),
            Some(99.78)
        );
        assert_eq!(parse_gs_comment_ex_percent("[DS], 3 excellents"), None);
    }

    #[test]
    fn groovestats_score_10000_from_counts_uses_itg_score_math() {
        let mut counts = [0u32; deadsync_rules::judgment::JUDGE_GRADE_COUNT];
        counts[deadsync_rules::judgment::judge_grade_ix(
            deadsync_rules::judgment::JudgeGrade::Fantastic,
        )] = 100;
        assert_eq!(
            groovestats_score_10000_from_counts(&counts, 2, 1, 0, 515),
            10_000
        );

        counts[deadsync_rules::judgment::judge_grade_ix(
            deadsync_rules::judgment::JudgeGrade::Miss,
        )] = 1;
        assert_eq!(
            groovestats_score_10000_from_counts(&counts, 2, 1, 0, 520),
            9_673
        );
        assert_eq!(groovestats_score_10000_from_counts(&counts, 0, 0, 0, 0), 0);
    }

    #[test]
    fn groovestats_rate_and_cmod_helpers_normalize_payload_values() {
        assert_eq!(groovestats_rate_hundredths(1.5), 150);
        assert_eq!(groovestats_rate_hundredths(0.0), 100);
        assert_eq!(groovestats_rate_hundredths(f32::NAN), 100);
        assert!(groovestats_used_cmod(ScrollSpeedSetting::CMod(650.0)));
        assert!(!groovestats_used_cmod(ScrollSpeedSetting::MMod(650.0)));
    }

    #[test]
    fn groovestats_ex_evidence_prefers_scoreboard() {
        let comment = Some("[DS], FA+, 100.00EX");
        assert_eq!(
            GsExEvidence::from_sources(None, comment).is_quint(),
            Some(true)
        );
        assert_eq!(
            GsExEvidence::from_sources(Some(9_978.0), comment).is_quint(),
            Some(false)
        );
        assert!(GsExEvidence::from_sources(Some(9_978.0), comment).proves_nonquint());
    }

    #[test]
    fn groovestats_lamp_judge_count_ignores_ds_prefix() {
        assert_eq!(gs_lamp_judge_count(Some(1), Some("[DS], 4w")), Some(4));
        assert_eq!(gs_lamp_judge_count(Some(2), Some("[DS], 5e")), Some(5));
        assert_eq!(gs_lamp_judge_count(Some(3), Some("[DS], 2g")), Some(2));
        assert_eq!(gs_lamp_judge_count(Some(1), Some("[DS], 10w")), None);
    }

    #[test]
    fn groovestats_lamp_index_uses_ex_evidence_for_perfect_scores() {
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                10_000.0,
                Some("[DS], FA+, 99.78EX"),
                GsExEvidence::from_sources(Some(10_000.0), Some("[DS], FA+, 99.78EX")),
                None,
            ),
            Some(0)
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                10_000.0,
                Some("[DS], FA+, 100.00EX, 3w"),
                GsExEvidence::from_sources(Some(9_978.0), Some("[DS], FA+, 100.00EX, 3w")),
                None,
            ),
            Some(1)
        );
    }

    #[test]
    fn groovestats_lamp_index_reconstructs_full_combo_lamps() {
        let stats = Some(GsLampChartStats {
            total_steps: 100,
            holds: 0,
            rolls: 0,
        });

        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_960.0,
                Some("[DS], 2e"),
                GsExEvidence::default(),
                stats
            ),
            Some(2)
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_940.0,
                Some("[DS], 1g"),
                GsExEvidence::default(),
                stats
            ),
            Some(3)
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_900.0,
                Some("[DS], 1d"),
                GsExEvidence::default(),
                stats
            ),
            Some(4)
        );
    }

    #[test]
    fn groovestats_lamp_index_rejects_missing_stats_or_dp_mismatch() {
        let stats = Some(GsLampChartStats {
            total_steps: 100,
            holds: 0,
            rolls: 0,
        });

        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_960.0,
                Some("[DS], 2e"),
                GsExEvidence::default(),
                None
            ),
            None
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_950.0,
                Some("[DS], 2e"),
                GsExEvidence::default(),
                stats
            ),
            None
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_000.0,
                Some("[DS], 1m"),
                GsExEvidence::default(),
                stats
            ),
            None
        );
    }

    #[test]
    fn cached_gs_score_from_lamp_applies_grade_and_judge_count() {
        let quad = cached_gs_score_from_lamp(10_000.0, false, Some(1), Some("[DS], 4w"));
        assert_eq!(quad.grade, Grade::Tier01);
        assert_eq!(quad.lamp_index, Some(1));
        assert_eq!(quad.lamp_judge_count, Some(4));

        let quint = cached_gs_score_from_lamp(10_000.0, false, Some(0), Some("[DS], FA+"));
        assert_eq!(quint.grade, Grade::Quint);
        assert_eq!(quint.lamp_index, Some(0));
        assert_eq!(quint.lamp_judge_count, None);
    }

    #[test]
    fn cached_gs_score_from_chart_stats_reconstructs_lamp_before_caching() {
        let stats = Some(GsLampChartStats {
            total_steps: 100,
            holds: 0,
            rolls: 0,
        });
        let score = cached_gs_score_from_chart_stats(
            9_920.0,
            false,
            Some("[DS], 4e"),
            GsExEvidence::default(),
            stats,
        );
        assert_eq!(score.grade, Grade::Tier02);
        assert_eq!(score.lamp_index, Some(2));
        assert_eq!(score.lamp_judge_count, Some(4));

        let failed = cached_gs_score_from_chart_stats(
            9_920.0,
            true,
            Some("[DS], 4e"),
            GsExEvidence::default(),
            stats,
        );
        assert_eq!(failed.grade, Grade::Failed);
        assert_eq!(failed.score_percent, 0.992);
        assert_eq!(failed.lamp_index, None);
        assert_eq!(failed.lamp_judge_count, None);
    }

    #[test]
    fn imported_gs_scores_use_ex_evidence_and_comments() {
        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+, 100.00EX".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::from_sources(None, Some("[DS], FA+, 100.00EX")),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Quint);
        assert_eq!(score.lamp_index, Some(0));

        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+, 100.00EX, C875".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::from_sources(
                    Some(9_978.0),
                    Some("[DS], FA+, 100.00EX, C875"),
                ),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));

        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+, 99.71EX, 3w".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::from_sources(None, Some("[DS], FA+, 99.71EX, 3w")),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));
        assert_eq!(score.lamp_judge_count, Some(3));

        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::default(),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));
    }

    #[test]
    fn cached_gs_failed_scores_clear_lamp_fields() {
        let failed = cached_gs_score_from_lamp(9_123.0, true, Some(1), Some("[DS], 4w"));
        assert_eq!(failed.grade, Grade::Failed);
        assert_eq!(failed.lamp_index, None);
        assert_eq!(failed.lamp_judge_count, None);
        assert_eq!(cached_missing_gs_score(), cached_failed_gs_score(0.0));
    }

    #[test]
    fn merge_local_fail_marks_matching_imported_score() {
        let imported = ImportedPlayerScore {
            score_10000: 1_358.0,
            comments: None,
            is_fail: false,
            ex_evidence: GsExEvidence::default(),
        };
        let matching_fail = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        assert!(merge_local_fail(imported.clone(), Some(matching_fail)).is_fail);

        let passed = CachedScore {
            grade: Grade::Tier17,
            ..matching_fail
        };
        assert!(!merge_local_fail(imported.clone(), Some(passed)).is_fail);

        let different_fail = CachedScore {
            score_percent: 0.2,
            ..matching_fail
        };
        assert!(!merge_local_fail(imported, Some(different_fail)).is_fail);
    }

    #[test]
    fn score_file_shard_uses_two_hash_chars() {
        assert_eq!(score_file_shard("abcdef"), "ab");
        assert_eq!(score_file_shard("a"), "00");
        assert_eq!(score_file_shard(""), "00");
    }

    #[test]
    fn score_file_name_parses_hash_and_timestamp() {
        assert_eq!(
            parse_score_file_name("dead-beef-1234.bin"),
            Some(("dead-beef", 1234))
        );
        assert_eq!(parse_score_file_name("deadbeef.bin"), None);
        assert_eq!(parse_score_file_name("-1234.bin"), None);
        assert_eq!(parse_score_file_name("deadbeef-abc.bin"), None);
        assert_eq!(parse_score_file_name("deadbeef-1234.txt"), None);
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
    fn imported_player_score_tracks_chart_stats_need() {
        let mut score = ImportedPlayerScore {
            score_10000: 9876.0,
            comments: Some("[DS], 2e".to_string()),
            is_fail: false,
            ex_evidence: GsExEvidence::default(),
        };
        assert!(score.needs_chart_stats());

        score.score_10000 = 10_000.0;
        assert!(!score.needs_chart_stats());

        score.score_10000 = 9876.0;
        score.comments = None;
        assert!(!score.needs_chart_stats());
    }

    #[test]
    fn cached_score_import_result_includes_itl_self_data() {
        let result = cached_score_import_result_from_imported(
            PlayerScoreImportResult {
                score: Some(ImportedPlayerScore {
                    score_10000: 9876.0,
                    comments: Some("[DS], 2e".to_string()),
                    is_fail: false,
                    ex_evidence: GsExEvidence::from_sources(None, Some("[DS], 2e")),
                }),
                score_proves_nonquint_ex: false,
                itl_self_score: Some(9912),
                itl_self_rank: Some(42),
                itl_self_found: true,
            },
            None,
        );

        assert!(result.score.is_some());
        assert!(result.itl_self_found);
        assert_eq!(result.itl_self_score, Some(9912));
        assert_eq!(result.itl_self_rank, Some(42));
    }

    #[test]
    fn cached_score_import_result_uses_ex_leaderboard_for_quint() {
        let result = cached_score_import_result_from_imported(
            PlayerScoreImportResult {
                score: Some(ImportedPlayerScore {
                    score_10000: 10_000.0,
                    comments: Some("[DS], FA+, 99.78EX".to_string()),
                    is_fail: false,
                    ex_evidence: GsExEvidence::from_sources(
                        Some(10_000.0),
                        Some("[DS], FA+, 99.78EX"),
                    ),
                }),
                score_proves_nonquint_ex: false,
                itl_self_score: None,
                itl_self_rank: None,
                itl_self_found: false,
            },
            None,
        );

        let score = result.score.expect("score cached");
        assert_eq!(score.grade, Grade::Quint);
        assert_eq!(score.lamp_index, Some(0));
    }

    #[test]
    fn cached_score_import_result_rejects_lower_ex_quint_comment() {
        let result = cached_score_import_result_from_imported(
            PlayerScoreImportResult {
                score: Some(ImportedPlayerScore {
                    score_10000: 10_000.0,
                    comments: Some("[DS], FA+, 100.00EX, C875".to_string()),
                    is_fail: false,
                    ex_evidence: GsExEvidence::from_sources(
                        Some(9_978.0),
                        Some("[DS], FA+, 100.00EX, C875"),
                    ),
                }),
                score_proves_nonquint_ex: true,
                itl_self_score: None,
                itl_self_rank: None,
                itl_self_found: false,
            },
            None,
        );

        let score = result.score.expect("score cached");
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));
        assert!(result.score_proves_nonquint_ex);
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

    #[test]
    fn scorebox_profile_snapshot_defaults_to_inactive() {
        let snapshot = GameplayScoreboxProfileSnapshot::default();
        assert!(!snapshot.display_scorebox);
        assert!(!snapshot.gs_active);
        assert!(!snapshot.show_ex_score);
        assert!(snapshot.api_key().is_empty());
        assert!(snapshot.arrowcloud_api_key().is_empty());
        assert!(!snapshot.include_arrowcloud());
        assert!(snapshot.gs_username().is_empty());
        assert!(snapshot.persistent_profile_id().is_none());
        assert!(snapshot.auto_profile_id().is_none());
        assert!(!snapshot.should_auto_populate());
    }

    #[test]
    fn scorebox_snapshot_trims_and_gates_services() {
        let snapshot = scorebox_snapshot(
            true,
            true,
            true,
            true,
            true,
            true,
            "  gs-key  ",
            "  ac-key  ",
            "  player  ",
            Some("profile-1".to_string()),
        );

        assert!(snapshot.display_scorebox);
        assert!(snapshot.gs_active);
        assert!(snapshot.show_ex_score);
        assert_eq!(snapshot.api_key(), "gs-key");
        assert_eq!(snapshot.arrowcloud_api_key(), "ac-key");
        assert!(snapshot.include_arrowcloud());
        assert_eq!(snapshot.gs_username(), "player");
        assert_eq!(snapshot.persistent_profile_id(), Some("profile-1"));
        assert_eq!(snapshot.auto_profile_id(), Some("profile-1"));
        assert!(snapshot.should_auto_populate());
    }

    #[test]
    fn scorebox_snapshot_requires_join_and_keys() {
        let not_joined = scorebox_snapshot(
            true,
            false,
            false,
            true,
            true,
            true,
            "gs-key",
            "ac-key",
            "player",
            Some("profile-1".to_string()),
        );
        assert!(!not_joined.gs_active);
        assert!(not_joined.include_arrowcloud());

        let missing_keys = scorebox_snapshot(
            true,
            false,
            true,
            true,
            true,
            true,
            "  ",
            "  ",
            "  ",
            Some("profile-1".to_string()),
        );
        assert!(!missing_keys.gs_active);
        assert!(!missing_keys.include_arrowcloud());
        assert_eq!(missing_keys.auto_profile_id(), Some("profile-1"));
        assert!(!missing_keys.should_auto_populate());
    }

    #[test]
    fn player_leaderboard_cache_key_uses_active_profile_snapshot() {
        let snapshot = scorebox_snapshot(
            true,
            true,
            true,
            true,
            true,
            false,
            "  gs-key  ",
            "  ac-key  ",
            "player",
            None,
        );
        let key =
            player_leaderboard_cache_key("  deadbeef  ", &snapshot).expect("active key expected");

        assert_eq!(key.chart_hash, "deadbeef");
        assert_eq!(key.api_key, "gs-key");
        assert_eq!(key.arrowcloud_api_key, "ac-key");
        assert!(key.include_arrowcloud);
        assert!(key.show_ex_score);
    }

    #[test]
    fn player_leaderboard_cache_key_requires_chart_and_gs() {
        let inactive = scorebox_snapshot(
            true, false, true, true, true, false, " ", "ac-key", "player", None,
        );
        assert!(player_leaderboard_cache_key("deadbeef", &inactive).is_none());

        let active = scorebox_snapshot(
            true, false, true, true, true, false, "gs-key", "ac-key", "player", None,
        );
        assert!(player_leaderboard_cache_key(" ", &active).is_none());
    }

    #[test]
    fn cached_leaderboard_snapshots_set_state_exclusively() {
        let data = PlayerLeaderboardData {
            panes: Vec::new(),
            itl_self_score: Some(9_876),
            itl_self_rank: Some(2),
        };
        let loading = CachedPlayerLeaderboardData::loading();
        assert!(loading.loading);
        assert!(loading.data.is_none());
        assert!(loading.error.is_none());

        let ready = CachedPlayerLeaderboardData::ready(data);
        assert!(!ready.loading);
        assert!(ready.data.is_some());
        assert!(ready.error.is_none());

        let error = CachedPlayerLeaderboardData::error("offline".to_string());
        assert!(!error.loading);
        assert!(error.data.is_none());
        assert_eq!(error.error.as_deref(), Some("offline"));
    }
}
