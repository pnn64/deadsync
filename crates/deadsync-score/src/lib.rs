use bincode::{Decode, Encode};
use chrono::{DateTime, TimeZone, Utc};
use deadsync_core::input::InputSource;
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_invalid};
use deadsync_rules::note::{HoldResult, MineResult, Note};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::{judgment, timing};
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

pub mod event_progress;
pub mod import;
pub mod itl;
pub mod leaderboard;
pub mod stage_stats;
pub use event_progress::*;
pub use import::{ImportedHighScore, grade_from_itg, local_score_from_itg, parse_itg_datetime_ms};
pub use itl::{
    ItlFileData, ItlHashEntry, ItlJudgments, ItlPointTotals, ex_hundredths, is_itl_unlocks_pack,
    itl_chart_no_cmod, itl_clear_type, itl_data_from_json, itl_event_name_from_group,
    itl_group_name_matches, itl_judgments_better, itl_mark_unlock_folders,
    itl_overall_ranks_from_song_cache, itl_point_totals, itl_points_for_chart, itl_points_for_song,
    itl_rebuild_song_ranks, itl_score_from_entry, itl_song_folder_unlocked, itl_song_matches,
    itl_steps_type_from_chart_type, parse_itl_points,
};
pub use leaderboard::*;

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
const LUA_SCORE_SUBMIT_ALLOWLIST: &[&str] = &[
    // "d5bd4dd7224f68ff", // Spooky (SM)
    // "c9e45c5e534f058d", // media offline (SM)
    // "596b42ed8317d9b8", // Godspeed (SX)
    // "3926ec3e5f1aaede", // CO5M1C R4ILR0AD (SH)
    // "a147dd828cd08fc7", // Riddle (DX)
    // "f95bc209c6f2cbfe", // Levels (SM)
    // "b50d0c3916e75b84", // Levels (SH)
    // "f41a24722a37758f", // Levels (SX)
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

pub const SCORE_IMPORT_ENDPOINT_CHOICES: [ScoreImportEndpoint; 3] = [
    ScoreImportEndpoint::GrooveStats,
    ScoreImportEndpoint::BoogieStats,
    ScoreImportEndpoint::ArrowCloud,
];

#[inline(always)]
pub const fn score_import_endpoint_choice_index(endpoint: ScoreImportEndpoint) -> usize {
    match endpoint {
        ScoreImportEndpoint::GrooveStats => 0,
        ScoreImportEndpoint::BoogieStats => 1,
        ScoreImportEndpoint::ArrowCloud => 2,
    }
}

#[inline(always)]
pub const fn score_import_endpoint_from_choice_index(idx: usize) -> ScoreImportEndpoint {
    match idx {
        1 => ScoreImportEndpoint::BoogieStats,
        2 => ScoreImportEndpoint::ArrowCloud,
        _ => ScoreImportEndpoint::GrooveStats,
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

#[derive(Debug, Clone, Copy)]
pub struct ItlEvalInput<'a> {
    pub chart_no_cmod: bool,
    pub used_cmod: bool,
    pub groovestats_valid: bool,
    pub groovestats_reason_lines: &'a [String],
    pub music_rate: f32,
    pub mines_enabled: bool,
    pub all_timing_windows_enabled: bool,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct ItlChartSaveInput<'a> {
    pub song_dir: &'a str,
    pub chart_hash: &'a str,
    pub chart_name: &'a str,
    pub chart_type: &'a str,
    pub event_name: &'a str,
    pub judgments: ItlJudgments,
    pub ex_percent: f64,
    pub used_cmod: bool,
    pub chart_no_cmod: bool,
    pub date: String,
}

#[derive(Debug, Clone)]
pub struct ItlChartSaveResult {
    pub needs_write: bool,
    pub progress: ItlEventProgress,
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

pub fn itl_eval_state_from_parts(input: ItlEvalInput<'_>) -> ItlEvalState {
    let rate = if input.music_rate.is_finite() && input.music_rate > 0.0 {
        input.music_rate
    } else {
        1.0
    };

    let mut reason_lines = Vec::with_capacity(5);
    if !input.groovestats_valid {
        if input.groovestats_reason_lines.is_empty() {
            reason_lines.push("Score is not valid for GrooveStats.".to_string());
        } else {
            reason_lines.extend(input.groovestats_reason_lines.iter().cloned());
        }
    }
    if (rate - 1.0).abs() > 0.0001 {
        reason_lines.push("ITL requires 1.00x music rate.".to_string());
    }
    if !input.mines_enabled {
        reason_lines.push("ITL requires mines to be enabled.".to_string());
    }
    if !input.all_timing_windows_enabled {
        reason_lines.push("ITL requires all timing windows to be enabled.".to_string());
    }
    if !input.passed {
        reason_lines.push("ITL only saves passing scores.".to_string());
    }
    if input.chart_no_cmod && input.used_cmod {
        reason_lines.push("This ITL chart does not allow CMod.".to_string());
    }

    ItlEvalState {
        active: true,
        eligible: reason_lines.is_empty(),
        chart_no_cmod: input.chart_no_cmod,
        used_cmod: input.used_cmod,
        reason_lines,
    }
}

pub fn save_itl_chart_result(
    data: &mut ItlFileData,
    input: ItlChartSaveInput<'_>,
) -> ItlChartSaveResult {
    let prev_totals = itl_point_totals(data);
    let path_changed = data
        .path_map
        .get(input.song_dir)
        .is_none_or(|hash| !hash.eq_ignore_ascii_case(input.chart_hash));
    if path_changed {
        data.path_map
            .insert(input.song_dir.to_string(), input.chart_hash.to_string());
    }

    let prev = data.hash_map.get(input.chart_hash).cloned();
    let (passing_points, max_scoring_points) = parse_itl_points(input.chart_name)
        .or_else(|| {
            prev.as_ref()
                .map(|entry| (entry.passing_points, entry.max_scoring_points))
        })
        .unwrap_or((0, 0));
    let max_points = passing_points.saturating_add(max_scoring_points);
    let current_run_ex = ex_hundredths(input.ex_percent);
    let new_entry = ItlHashEntry {
        judgments: input.judgments.clone(),
        ex: current_run_ex,
        clear_type: itl_clear_type(&input.judgments),
        points: itl_points_for_song(passing_points, max_scoring_points, input.ex_percent),
        used_cmod: input.used_cmod,
        date: input.date,
        no_cmod: input.chart_no_cmod,
        passing_points,
        max_scoring_points,
        max_points,
        rank: None,
        steps_type: itl_steps_type_from_chart_type(input.chart_type).to_string(),
        passes: prev
            .as_ref()
            .map_or(1, |entry| entry.passes.saturating_add(1)),
    };

    let mut needs_write = path_changed;
    let mut best_changed = false;
    match data.hash_map.get_mut(input.chart_hash) {
        None => {
            data.hash_map
                .insert(input.chart_hash.to_string(), new_entry.clone());
            needs_write = true;
        }
        Some(existing) => {
            if existing.passes != new_entry.passes {
                existing.passes = new_entry.passes;
                needs_write = true;
            }
            if !existing
                .steps_type
                .eq_ignore_ascii_case(new_entry.steps_type.as_str())
            {
                existing.steps_type.clone_from(&new_entry.steps_type);
                needs_write = true;
            }

            let ex_improved = new_entry.ex > existing.ex;
            let ex_tied = new_entry.ex == existing.ex;
            if ex_improved {
                existing.ex = new_entry.ex;
                existing.points = new_entry.points;
                existing.judgments.clone_from(&new_entry.judgments);
                needs_write = true;
                best_changed = true;
            } else if ex_tied && itl_judgments_better(&new_entry.judgments, &existing.judgments) {
                existing.judgments.clone_from(&new_entry.judgments);
                needs_write = true;
                best_changed = true;
            }
            if new_entry.clear_type > existing.clear_type {
                existing.clear_type = new_entry.clear_type;
                needs_write = true;
                best_changed = true;
            }
            if best_changed {
                existing.used_cmod = new_entry.used_cmod;
                existing.date.clone_from(&new_entry.date);
                existing.no_cmod = new_entry.no_cmod;
                existing.passing_points = new_entry.passing_points;
                existing.max_scoring_points = new_entry.max_scoring_points;
                existing.max_points = new_entry.max_points;
            }
        }
    }

    itl_rebuild_song_ranks(data);
    let current_totals = itl_point_totals(data);
    let current_entry = data
        .hash_map
        .get(input.chart_hash)
        .cloned()
        .unwrap_or(new_entry);
    let prev_entry = prev.unwrap_or_default();
    let mut progress = ItlEventProgress {
        kind: EventProgressKind::Itl,
        name: itl_event_name_from_group(Some(input.event_name)),
        is_doubles: current_entry.steps_type.eq_ignore_ascii_case("double"),
        score_hundredths: current_run_ex,
        score_delta_hundredths: delta_i32(current_run_ex, prev_entry.ex),
        rate_hundredths: None,
        rate_delta_hundredths: None,
        current_points: current_entry.points,
        point_delta: delta_i32(current_entry.points, prev_entry.points),
        current_ranking_points: current_totals.ranking_points,
        ranking_delta: delta_i32(current_totals.ranking_points, prev_totals.ranking_points),
        current_song_points: current_totals.song_points,
        song_delta: delta_i32(current_totals.song_points, prev_totals.song_points),
        current_ex_points: current_totals.ex_points,
        ex_delta: delta_i32(current_totals.ex_points, prev_totals.ex_points),
        current_total_points: current_totals.total_points,
        total_delta: delta_i32(current_totals.total_points, prev_totals.total_points),
        total_passes: current_entry.passes.max(1),
        clear_type_before: Some(prev_entry.clear_type),
        clear_type_after: Some(current_entry.clear_type),
        stat_improvements: Vec::new(),
        skill_improvements: Vec::new(),
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages = event_progress_overlay_pages(&progress, None, &[]);

    ItlChartSaveResult {
        needs_write,
        progress,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
    fn submit_event_progress_builds_srpg_and_itl_pages() {
        let input = SubmitEventProgressInput {
            result: "score-added".to_string(),
            score_10000: 9876,
            rate_hundredths: 150,
            itl_score_hundredths: Some(9912),
            srpg: Some(SubmitEventProgressData {
                name: "SRPG".to_string(),
                is_doubles: false,
                score_delta: 1,
                rate_delta: 1,
                top_score_points: 10,
                prev_top_score_points: 4,
                total_passes: 2,
                current_ranking_point_total: 11,
                previous_ranking_point_total: 5,
                current_song_point_total: 12,
                previous_song_point_total: 6,
                current_ex_point_total: 13,
                previous_ex_point_total: 7,
                current_point_total: 14,
                previous_point_total: 8,
                leaderboard: vec![entry(98.76)],
                progress: Some(SubmitProgress {
                    stat_improvements: vec![
                        SubmitStatImprovement {
                            name: "tp".to_string(),
                            gained: 5,
                            current: 12,
                        },
                        SubmitStatImprovement {
                            name: "exp".to_string(),
                            gained: 150,
                            current: 900,
                        },
                    ],
                    skill_improvements: vec!["Gained 150 EXP and reached Level 12".to_string()],
                    ..SubmitProgress::default()
                }),
            }),
            itl: Some(SubmitEventProgressData {
                name: "ITL Online 2026".to_string(),
                is_doubles: true,
                score_delta: 34,
                top_score_points: 100,
                prev_top_score_points: 90,
                total_passes: 3,
                current_ranking_point_total: 200,
                previous_ranking_point_total: 150,
                current_song_point_total: 300,
                previous_song_point_total: 250,
                current_ex_point_total: 400,
                previous_ex_point_total: 350,
                current_point_total: 900,
                previous_point_total: 750,
                leaderboard: vec![entry(99.12)],
                progress: Some(SubmitProgress {
                    stat_improvements: vec![
                        SubmitStatImprovement {
                            name: "clearType".to_string(),
                            gained: 1,
                            current: 4,
                        },
                        SubmitStatImprovement {
                            name: "grade".to_string(),
                            gained: 1,
                            current: 2,
                        },
                    ],
                    quests_completed: vec![SubmitQuest {
                        title: "Unlock Song".to_string(),
                        rewards: vec![SubmitQuestReward {
                            reward_type: "pack".to_string(),
                            description: "New pack".to_string(),
                        }],
                    }],
                    achievements_completed: vec![SubmitAchievement {
                        title: "Milestone".to_string(),
                        rewards: vec![SubmitAchievementReward {
                            tier: "2".to_string(),
                            requirements: vec!["Play one chart".to_string()],
                            title_unlocked: "Champion".to_string(),
                        }],
                    }],
                    skill_improvements: Vec::new(),
                }),
                ..SubmitEventProgressData::default()
            }),
        };

        let progress = event_progress_from_submit(&input);

        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].kind, EventProgressKind::Srpg);
        assert_eq!(progress[0].score_delta_hundredths, 9876);
        assert_eq!(progress[0].rate_delta_hundredths, Some(150));
        assert!(matches!(
            &progress[0].overlay_pages[0],
            EventOverlayPage::Text(text)
                if text.contains("Skill Improvements")
                    && text.contains("+5 TP")
                    && text.contains("+150 EXP")
                    && text.contains("Gained 150 EXP and reached Level 12")
        ));

        assert_eq!(progress[1].kind, EventProgressKind::Itl);
        assert_eq!(progress[1].clear_type_before, Some(3));
        assert_eq!(progress[1].clear_type_after, Some(4));
        assert!(matches!(
            &progress[1].overlay_pages[0],
            EventOverlayPage::Text(text)
                if text.contains("Clear Type: FEC >>> FFC") && text.contains("New Quint!")
        ));
        assert!(matches!(
            &progress[1].overlay_pages[1],
            EventOverlayPage::Text(text)
                if text.contains("Completed \"Unlock Song\"!") && text.contains("PACK:")
        ));
        assert!(matches!(
            &progress[1].overlay_pages[2],
            EventOverlayPage::Text(text)
                if text.contains("Completed the \"Milestone\" Achievement!")
                    && text.contains("Unlocked the \"Champion\" Title!")
        ));
        assert!(matches!(
            progress[1].overlay_pages.last(),
            Some(EventOverlayPage::Leaderboard(entries)) if entries.len() == 1
        ));
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
    fn itl_eval_state_from_parts_accepts_valid_play() {
        let state = itl_eval_state_from_parts(ItlEvalInput {
            chart_no_cmod: false,
            used_cmod: false,
            groovestats_valid: true,
            groovestats_reason_lines: &[],
            music_rate: 1.0,
            mines_enabled: true,
            all_timing_windows_enabled: true,
            passed: true,
        });

        assert!(state.active);
        assert!(state.eligible);
        assert!(!state.chart_no_cmod);
        assert!(!state.used_cmod);
        assert!(state.reason_lines.is_empty());
    }

    #[test]
    fn itl_eval_state_from_parts_reports_policy_reasons() {
        let gs_reasons = vec!["GrooveStats does not support dance-solo charts.".to_string()];
        let state = itl_eval_state_from_parts(ItlEvalInput {
            chart_no_cmod: true,
            used_cmod: true,
            groovestats_valid: false,
            groovestats_reason_lines: gs_reasons.as_slice(),
            music_rate: 1.5,
            mines_enabled: false,
            all_timing_windows_enabled: false,
            passed: false,
        });

        assert!(state.active);
        assert!(!state.eligible);
        assert_eq!(
            state.reason_lines,
            vec![
                "GrooveStats does not support dance-solo charts.",
                "ITL requires 1.00x music rate.",
                "ITL requires mines to be enabled.",
                "ITL requires all timing windows to be enabled.",
                "ITL only saves passing scores.",
                "This ITL chart does not allow CMod.",
            ]
        );
    }

    #[test]
    fn itl_eval_state_from_parts_falls_back_for_missing_gs_reason() {
        let state = itl_eval_state_from_parts(ItlEvalInput {
            chart_no_cmod: false,
            used_cmod: false,
            groovestats_valid: false,
            groovestats_reason_lines: &[],
            music_rate: 1.0,
            mines_enabled: true,
            all_timing_windows_enabled: true,
            passed: true,
        });

        assert_eq!(
            state.reason_lines,
            vec!["Score is not valid for GrooveStats."]
        );
    }

    #[test]
    fn save_itl_chart_result_adds_new_entry_and_progress() {
        let mut data = ItlFileData::default();
        let result = save_itl_chart_result(
            &mut data,
            ItlChartSaveInput {
                song_dir: "/Songs/ITL Online 2026/Example",
                chart_hash: "deadbeef",
                chart_name: "7500 (P) + 12000 (S)",
                chart_type: "dance-double",
                event_name: "ITL Online 2026",
                judgments: ItlJudgments {
                    w0: 10,
                    total_steps: 10,
                    ..ItlJudgments::default()
                },
                ex_percent: 100.0,
                used_cmod: false,
                chart_no_cmod: true,
                date: "2026-06-23".to_string(),
            },
        );

        assert!(result.needs_write);
        assert_eq!(data.path_map["/Songs/ITL Online 2026/Example"], "deadbeef");
        let entry = &data.hash_map["deadbeef"];
        assert_eq!(entry.ex, 10_000);
        assert_eq!(entry.points, 19_500);
        assert_eq!(entry.passing_points, 7500);
        assert_eq!(entry.max_scoring_points, 12000);
        assert_eq!(entry.passes, 1);
        assert_eq!(entry.steps_type, "double");
        assert!(entry.no_cmod);
        assert_eq!(result.progress.name, "ITL Online 2026");
        assert!(result.progress.is_doubles);
        assert_eq!(result.progress.current_points, 19_500);
        assert_eq!(result.progress.score_delta_hundredths, 10_000);
        assert_eq!(result.progress.clear_type_before, Some(0));
        assert_eq!(result.progress.clear_type_after, Some(5));
        assert!(!result.progress.overlay_pages.is_empty());
    }

    #[test]
    fn save_itl_chart_result_updates_passes_and_better_tied_judgments() {
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/ITL Online 2026/Example".to_string(),
            "deadbeef".to_string(),
        );
        data.hash_map.insert(
            "deadbeef".to_string(),
            ItlHashEntry {
                judgments: ItlJudgments {
                    w0: 9,
                    w1: 1,
                    total_steps: 10,
                    ..ItlJudgments::default()
                },
                ex: 10_000,
                clear_type: 4,
                points: 19_500,
                passing_points: 7500,
                max_scoring_points: 12000,
                max_points: 19_500,
                passes: 1,
                steps_type: "single".to_string(),
                ..ItlHashEntry::default()
            },
        );

        let result = save_itl_chart_result(
            &mut data,
            ItlChartSaveInput {
                song_dir: "/Songs/ITL Online 2026/Example",
                chart_hash: "deadbeef",
                chart_name: "",
                chart_type: "dance-single",
                event_name: "",
                judgments: ItlJudgments {
                    w0: 10,
                    total_steps: 10,
                    ..ItlJudgments::default()
                },
                ex_percent: 100.0,
                used_cmod: true,
                chart_no_cmod: false,
                date: "2026-06-24".to_string(),
            },
        );

        let entry = &data.hash_map["deadbeef"];
        assert!(result.needs_write);
        assert_eq!(entry.passes, 2);
        assert_eq!(entry.judgments.w0, 10);
        assert!(entry.used_cmod);
        assert_eq!(entry.date, "2026-06-24");
        assert_eq!(result.progress.name, "ITL Online 2026");
        assert_eq!(result.progress.total_passes, 2);
        assert_eq!(result.progress.point_delta, 0);
        assert_eq!(result.progress.score_delta_hundredths, 0);
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
    fn lua_submit_allowlist_is_disabled() {
        assert!(!lua_chart_submit_allowed("d5bd4dd7224f68ff"));
        assert!(!lua_chart_submit_allowed(" D5BD4DD7224F68FF "));
        assert!(!lua_chart_submit_allowed("deadbeefcafebabe"));
        assert!(lua_submit_allowed(false, "deadbeefcafebabe"));
        assert!(!lua_submit_allowed(true, "deadbeefcafebabe"));
        assert!(!lua_submit_allowed(true, "d5bd4dd7224f68ff"));
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
    fn score_import_endpoint_choices_match_options_order() {
        assert_eq!(
            SCORE_IMPORT_ENDPOINT_CHOICES,
            [
                ScoreImportEndpoint::GrooveStats,
                ScoreImportEndpoint::BoogieStats,
                ScoreImportEndpoint::ArrowCloud,
            ]
        );
        assert_eq!(
            score_import_endpoint_choice_index(ScoreImportEndpoint::GrooveStats),
            0
        );
        assert_eq!(
            score_import_endpoint_choice_index(ScoreImportEndpoint::BoogieStats),
            1
        );
        assert_eq!(
            score_import_endpoint_choice_index(ScoreImportEndpoint::ArrowCloud),
            2
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(0),
            ScoreImportEndpoint::GrooveStats
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(1),
            ScoreImportEndpoint::BoogieStats
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(2),
            ScoreImportEndpoint::ArrowCloud
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(99),
            ScoreImportEndpoint::GrooveStats
        );
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
