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
}
