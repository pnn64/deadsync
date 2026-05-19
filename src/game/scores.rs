use crate::config::SimpleIni;
use crate::config::dirs;
use crate::engine::input::InputSource;
use crate::engine::network;
use crate::game::gameplay;
use crate::game::judgment;
use crate::game::online;
use crate::game::profile::{self, Profile};
use crate::game::song::get_song_cache;
use crate::game::stage_stats;
use chrono::{DateTime, Local, TimeZone, Utc};
use log::{debug, warn};
use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bincode::{Decode, Encode};

mod arrowcloud;
mod groovestats;
mod itl;
mod submit_status;

pub use arrowcloud::{
    ArrowCloudSubmitUiStatus, arrowcloud_next_retry_is_auto, arrowcloud_next_retry_remaining_secs,
    get_arrowcloud_submit_ui_status_for_side, retry_arrowcloud_submit,
    submit_arrowcloud_payloads_from_gameplay, tick_arrowcloud_auto_retries,
};
pub use groovestats::{
    GrooveStatsEvalState, GrooveStatsSubmitRecordBanner, GrooveStatsSubmitUiStatus,
    get_groovestats_submit_itl_progress_for_side, get_groovestats_submit_record_banner_for_side,
    get_groovestats_submit_ui_status_for_side, groovestats_eval_state_from_gameplay,
    groovestats_next_retry_is_auto, groovestats_next_retry_remaining_secs,
    retry_groovestats_submit, submit_groovestats_payloads_from_gameplay,
    tick_groovestats_auto_retries,
};
use groovestats::{
    GrooveStatsSubmitApiAchievement, GrooveStatsSubmitApiEvent, GrooveStatsSubmitApiPlayer,
    GrooveStatsSubmitApiProgress, GrooveStatsSubmitApiQuest, GrooveStatsSubmitPlayerJob,
    groovestats_judgment_counts,
};
pub use itl::{
    CachedItlScore, ItlEvalState, ItlEventProgress, ItlOverlayPage, get_cached_itl_score_for_side,
    get_cached_itl_score_for_song, get_cached_itl_self_score_for_side,
    get_cached_itl_tournament_overall_ranks_for_side, get_cached_itl_tournament_rank_for_side,
    get_or_fetch_itl_self_score_for_side, get_or_fetch_itl_tournament_rank_for_side,
    is_itl_song_folder_unlocked_for_side, is_itl_unlocks_pack, itl_eval_state_from_gameplay,
    itl_points_for_chart, save_itl_data_from_gameplay, should_warn_cmod_for_itl_chart,
};
pub use submit_status::RejectReason;
pub(crate) use submit_status::{
    SUBMIT_RETRY_MAX_ATTEMPTS, duration_to_ceil_secs, submit_retry_delay_secs,
};

// Lua charts stay blocked from online submit unless their effects have been
// verified closely enough to match ITGmania for scoring purposes.
const LUA_SCORE_SUBMIT_ALLOWLIST: [&str; 1] = ["d5bd4dd7224f68ff"];

pub fn lua_chart_submit_allowed(chart_hash: &str) -> bool {
    let hash = chart_hash.trim();
    !hash.is_empty()
        && LUA_SCORE_SUBMIT_ALLOWLIST
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(hash))
}

pub fn lua_submit_allowed(song_has_lua: bool, chart_hash: &str) -> bool {
    !song_has_lua || lua_chart_submit_allowed(chart_hash)
}

// --- Grade Definitions ---

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
    /// Converts a grade to the corresponding frame index on the "grades 1x19.png" spritesheet.
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

/// A struct to hold both the calculated grade and the precise score percentage.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct CachedScore {
    pub grade: Grade,
    pub score_percent: f64, // Stored as 0.0 to 1.0
    /// Optional lamp index for UI (e.g., Select Music wheel).
    /// This is intentionally UI-agnostic: the meaning of the index is left
    /// to the presentation layer (colors, effects, etc.).
    pub lamp_index: Option<u8>,
    /// Optional single-digit judge count for the lamp (e.g. 1..=9).
    pub lamp_judge_count: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalScalarScore {
    pub percent: f64,
    pub is_fail: bool,
}

// --- GrooveStats grade cache (on-disk + network-fetched) ---

#[derive(Default)]
struct GsScoreCacheState {
    loaded_profiles: HashMap<String, HashMap<String, CachedScore>>,
}

static GS_SCORE_CACHE: std::sync::LazyLock<Mutex<GsScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(GsScoreCacheState::default()));

fn gs_scores_dir_for_profile(profile_id: &str) -> PathBuf {
    dirs::app_dirs()
        .profiles_root()
        .join(profile_id)
        .join("scores")
        .join("gs")
}

fn gs_scores_dir_for_profile_and_hash(profile_id: &str, chart_hash: &str) -> PathBuf {
    gs_scores_dir_for_profile(profile_id).join(shard2_for_hash(chart_hash))
}

fn gs_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    gs_scores_dir_for_profile(profile_id).join("index.bin")
}

fn load_gs_score_index(path: &Path) -> Option<HashMap<String, CachedScore>> {
    let bytes = fs::read(path).ok()?;
    let (mut by_chart, _) = bincode::decode_from_slice::<HashMap<String, CachedScore>, _>(
        &bytes,
        bincode::config::standard(),
    )
    .ok()?;
    let mut changed = false;
    for score in by_chart.values_mut() {
        let fixed = fix_gs_cached_score(*score);
        changed |= fixed != *score;
        *score = fixed;
    }
    if changed {
        save_gs_score_index(path, &by_chart);
    }
    Some(by_chart)
}

fn save_gs_score_index(path: &Path, by_chart: &HashMap<String, CachedScore>) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(parent) {
        warn!("Failed to create GS score index dir {parent:?}: {e}");
        return;
    }
    let Ok(buf) = bincode::encode_to_vec(by_chart, bincode::config::standard()) else {
        warn!("Failed to encode GS score index at {path:?}");
        return;
    };
    let tmp_path = path.with_extension("tmp");
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write GS score index temp file {tmp_path:?}: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
        warn!("Failed to commit GS score index file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
    }
}

fn ensure_gs_score_cache_loaded_for_profile(profile_id: &str) {
    let needs_load = {
        let state = GS_SCORE_CACHE.lock().unwrap();
        !state.loaded_profiles.contains_key(profile_id)
    };
    if !needs_load {
        return;
    }

    let load_started = Instant::now();
    let index_path = gs_score_index_path_for_profile(profile_id);
    let disk_cache = load_gs_score_index(&index_path).unwrap_or_else(|| {
        let scanned = best_scores_from_disk(&gs_scores_dir_for_profile(profile_id));
        save_gs_score_index(&index_path, &scanned);
        scanned
    });
    let loaded_entries = disk_cache.len();
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    if load_ms >= 25.0 {
        debug!(
            "Loaded GrooveStats score cache for profile {profile_id}: {loaded_entries} chart(s) in {load_ms:.2}ms."
        );
    }
    let mut state = GS_SCORE_CACHE.lock().unwrap();
    state
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(disk_cache);
}

fn get_cached_local_score_for_profile(profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
    if profile_id.trim().is_empty() {
        return None;
    }
    ensure_local_score_cache_loaded(profile_id);
    LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(profile_id)
        .and_then(|idx| idx.best_itg.get(chart_hash).copied())
}

fn get_cached_gs_score_for_profile(profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
    if profile_id.trim().is_empty() {
        return None;
    }
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(profile_id)
        .and_then(|scores| scores.get(chart_hash).copied())
}

pub fn get_cached_gs_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_gs_score_cache_loaded_for_profile(&profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|m| m.get(chart_hash).copied())
}

fn cached_gs_chart_hashes_for_profile(profile_id: &str) -> HashSet<String> {
    if profile_id.trim().is_empty() {
        return HashSet::new();
    }
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(profile_id)
        .map_or_else(HashSet::new, |scores| scores.keys().cloned().collect())
}

fn set_cached_gs_score_for_profile(profile_id: &str, chart_hash: String, score: CachedScore) {
    let score = fix_gs_cached_score(score);
    debug!("Caching GrooveStats score {score:?} for chart hash {chart_hash}");
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    let snapshot = {
        let mut state = GS_SCORE_CACHE.lock().unwrap();
        let Some(map) = state.loaded_profiles.get_mut(profile_id) else {
            return;
        };
        map.insert(chart_hash, score);
        map.clone()
    };
    save_gs_score_index(&gs_score_index_path_for_profile(profile_id), &snapshot);
}

// --- ArrowCloud score cache (on-disk, all 3 leaderboards per chart) ---

/// Server-side grade name returned by ArrowCloud's grading systems
/// (ITG / EX / HardEX). Source of truth:
/// `arrow-cloud/api/src/utils/scoring/index.ts:223-260`.
///
/// `Quint` and `Sex` are the EX and HardEX top-tier grades respectively;
/// `Quad` is the ITG top tier. `Failed` corresponds to AC's `failingGrade: 'F'`.
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

    /// Parse the canonical AC grade string. Case-sensitive (matches the
    /// server's grading-system keys); whitespace is trimmed. Unrecognised
    /// strings (e.g. event-only grades) yield `None`.
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

/// A single ArrowCloud score for one (chart, leaderboard) pair.
///
/// Holds AC-native fields rather than reusing `CachedScore`, because the AC
/// bulk endpoint returns data the GS-style cache cannot represent (e.g. the
/// canonical server grade string for events that don't map to ITG tiers, the
/// play timestamp, the AC `playId`) and lacks data the GS cache requires
/// (judgment counts for FA+/lamp computation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowCloudScore {
    /// 0.0..=1.0 (the percent value from the API divided by 100).
    pub score_percent: f64,
    /// Canonical server grade name. `None` if the API returned an
    /// unrecognised string (e.g. an event-specific grade).
    pub server_grade: Option<ArrowCloudServerGrade>,
    /// UTC timestamp of the play. `None` if the API omitted the field or
    /// returned an unparseable string.
    pub played_at: Option<DateTime<Utc>>,
    /// Opaque AC play id used for linking back to the play detail page.
    pub play_id: Option<i64>,
    /// `true` if the play was marked as a fail by the server.
    pub is_fail: bool,
}

// `chrono::DateTime` does not implement `bincode::Encode/Decode`. Persist the
// timestamp as UTC milliseconds since the unix epoch so the on-disk index
// stays compact and round-trips cleanly across timezones.
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
    /// Adapter used by `get_cached_score_for_side` to merge AC scores with
    /// local + GS scores in the same `CachedScore` slot. Recomputes the
    /// internal `Grade` tier from the score percent because the server's
    /// grade enum (e.g. `Tristar`) does not align with our `Tier01..Tier17`
    /// scheme. AC entries carry no FA+/lamp data, so the lamp fields stay
    /// `None`.
    pub fn to_cached_score(&self) -> CachedScore {
        let percent_0_100 = (self.score_percent * 100.0).clamp(0.0, 100.0);
        let score_10000 = percent_0_100 * 100.0;
        cached_score_from_gs(score_10000, None, "", self.is_fail, GsExEvidence::default())
    }
}

/// Cached AC scores for a single chart, one entry per global leaderboard.
///
/// Fields correspond to AC `Leaderboard.id`:
/// - `itg`     -> `GLOBAL_MONEY_LEADERBOARD_ID = 3`
/// - `ex`      -> `GLOBAL_EX_LEADERBOARD_ID = 2`
/// - `hard_ex` -> `GLOBAL_HARD_EX_LEADERBOARD_ID = 4`
#[derive(Debug, Clone, Copy, Default, PartialEq, Encode, Decode)]
pub struct ArrowCloudScores {
    pub itg: Option<ArrowCloudScore>,
    pub ex: Option<ArrowCloudScore>,
    pub hard_ex: Option<ArrowCloudScore>,
}

#[derive(Default)]
struct AcScoreCacheState {
    loaded_profiles: HashMap<String, HashMap<String, ArrowCloudScores>>,
}

static AC_SCORE_CACHE: std::sync::LazyLock<Mutex<AcScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(AcScoreCacheState::default()));

fn ac_scores_dir_for_profile(profile_id: &str) -> PathBuf {
    profile::local_profile_dir_for_id(profile_id)
        .join("scores")
        .join("ac")
}

fn ac_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    ac_scores_dir_for_profile(profile_id).join("index.bin")
}

fn load_ac_score_index(path: &Path) -> Option<HashMap<String, ArrowCloudScores>> {
    let bytes = fs::read(path).ok()?;
    let (by_chart, _) = bincode::decode_from_slice::<HashMap<String, ArrowCloudScores>, _>(
        &bytes,
        bincode::config::standard(),
    )
    .ok()?;
    Some(by_chart)
}

fn save_ac_score_index(path: &Path, by_chart: &HashMap<String, ArrowCloudScores>) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(parent) {
        warn!("Failed to create AC score index dir {parent:?}: {e}");
        return;
    }
    let Ok(buf) = bincode::encode_to_vec(by_chart, bincode::config::standard()) else {
        warn!("Failed to encode AC score index at {path:?}");
        return;
    };
    let tmp_path = path.with_extension("tmp");
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write AC score index temp file {tmp_path:?}: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
        warn!("Failed to commit AC score index file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
    }
}

fn ensure_ac_score_cache_loaded_for_profile(profile_id: &str) {
    let needs_load = {
        let state = AC_SCORE_CACHE.lock().unwrap();
        !state.loaded_profiles.contains_key(profile_id)
    };
    if !needs_load {
        return;
    }
    let index_path = ac_score_index_path_for_profile(profile_id);
    let disk_cache = load_ac_score_index(&index_path).unwrap_or_default();
    let mut state = AC_SCORE_CACHE.lock().unwrap();
    state
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(disk_cache);
}

fn get_cached_ac_scores_for_profile(
    profile_id: &str,
    chart_hash: &str,
) -> Option<ArrowCloudScores> {
    if profile_id.trim().is_empty() {
        return None;
    }
    ensure_ac_score_cache_loaded_for_profile(profile_id);
    AC_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(profile_id)
        .and_then(|m| m.get(chart_hash).copied())
}

/// Public side-aware accessor for the wheel/gameplay layer to read AC scores.
pub fn get_cached_ac_scores_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<ArrowCloudScores> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    get_cached_ac_scores_for_profile(&profile_id, chart_hash)
}

fn cached_ac_chart_hashes_with_itg_for_profile(profile_id: &str) -> HashSet<String> {
    if profile_id.trim().is_empty() {
        return HashSet::new();
    }
    ensure_ac_score_cache_loaded_for_profile(profile_id);
    AC_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(profile_id)
        .map_or_else(HashSet::new, |scores| {
            scores
                .iter()
                .filter_map(|(hash, ac)| ac.itg.is_some().then(|| hash.clone()))
                .collect()
        })
}

/// Bulk-write multiple AC score entries with a single index save.
fn set_cached_ac_scores_for_profile_bulk(
    profile_id: &str,
    entries: impl IntoIterator<Item = (String, ArrowCloudScores)>,
) {
    ensure_ac_score_cache_loaded_for_profile(profile_id);
    let snapshot = {
        let mut state = AC_SCORE_CACHE.lock().unwrap();
        let Some(map) = state.loaded_profiles.get_mut(profile_id) else {
            return;
        };
        for (hash, scores) in entries {
            map.insert(hash, scores);
        }
        map.clone()
    };
    save_ac_score_index(&ac_score_index_path_for_profile(profile_id), &snapshot);
}

/// Update the AC cache after a successful score submit.
///
/// Builds an `ArrowCloudScores` from the gameplay-computed ITG / EX / HardEX
/// percents and merges it with the existing cached entry, keeping the higher
/// percent per leaderboard so a worse follow-up play doesn't overwrite a
/// better stored score (parity with [`cache_gs_score_for_profile`]).
pub(super) fn cache_arrowcloud_scores_from_submit(
    profile_id: &str,
    chart_hash: &str,
    itg_percent: f64,
    ex_percent: f64,
    hard_ex_percent: f64,
    is_fail: bool,
    submitted_at: chrono::DateTime<chrono::Utc>,
) {
    let make_score = |percent: f64| -> Option<ArrowCloudScore> {
        if !percent.is_finite() {
            return None;
        }
        let clamped = percent.clamp(0.0, 100.0);
        Some(ArrowCloudScore {
            score_percent: clamped / 100.0,
            server_grade: None, // The submit response is best-effort: we
            // don't decode the server's chosen grade here.
            played_at: Some(submitted_at),
            play_id: None,
            is_fail,
        })
    };

    let new_scores = ArrowCloudScores {
        itg: make_score(itg_percent),
        ex: make_score(ex_percent),
        hard_ex: make_score(hard_ex_percent),
    };

    ensure_ac_score_cache_loaded_for_profile(profile_id);
    let merged = {
        let mut state = AC_SCORE_CACHE.lock().unwrap();
        let Some(map) = state.loaded_profiles.get_mut(profile_id) else {
            return;
        };
        let entry = map.entry(chart_hash.to_string()).or_default();
        merge_arrowcloud_score_slot(&mut entry.itg, new_scores.itg);
        merge_arrowcloud_score_slot(&mut entry.ex, new_scores.ex);
        merge_arrowcloud_score_slot(&mut entry.hard_ex, new_scores.hard_ex);
        map.clone()
    };
    save_ac_score_index(&ac_score_index_path_for_profile(profile_id), &merged);
}

#[inline]
fn merge_arrowcloud_score_slot(
    existing: &mut Option<ArrowCloudScore>,
    incoming: Option<ArrowCloudScore>,
) {
    let Some(new_score) = incoming else {
        return;
    };
    match existing {
        None => *existing = Some(new_score),
        Some(prev) => {
            // Failed scores never overwrite a non-failed score with the same
            // or higher percent. Otherwise, keep whichever percent is higher.
            let prev_failed = prev.is_fail;
            let new_failed = new_score.is_fail;
            if prev_failed && !new_failed {
                *prev = new_score;
            } else if !prev_failed && new_failed {
                // Keep prev: the existing non-failed score wins.
            } else if new_score.score_percent > prev.score_percent {
                *prev = new_score;
            }
        }
    }
}

// --- Local score cache (on-disk, one file per play) ---

#[derive(Clone, Copy, Debug, Encode, Decode)]
struct BestScalar {
    grade: Grade,
    percent: f64,
}

#[derive(Debug, Default, Clone, Encode, Decode)]
struct LocalScoreIndex {
    best_itg: HashMap<String, CachedScore>,
    best_ex: HashMap<String, BestScalar>,
    best_hard_ex: HashMap<String, BestScalar>,
}

// Rebuild old indexes so stale grade-only quints are rehydrated from EX-backed play files.
const LOCAL_SCORE_INDEX_VERSION: u16 = 2;

#[derive(Debug, Clone, Encode, Decode)]
struct LocalScoreIndexFile {
    version: u16,
    index: LocalScoreIndex,
}

#[derive(Default)]
struct LocalScoreCacheState {
    loaded_profiles: HashMap<String, LocalScoreIndex>,
}

static LOCAL_SCORE_CACHE: std::sync::LazyLock<Mutex<LocalScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(LocalScoreCacheState::default()));

#[derive(Clone, Debug)]
struct MachineBest {
    score: CachedScore,
    initials: String,
}

#[derive(Default)]
struct MachineLocalScoreCacheState {
    loaded: bool,
    best_itg: HashMap<String, MachineBest>,
}

static MACHINE_LOCAL_SCORE_CACHE: std::sync::LazyLock<Mutex<MachineLocalScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(MachineLocalScoreCacheState::default()));

fn local_scores_root_for_profile(profile_id: &str) -> PathBuf {
    dirs::app_dirs()
        .profiles_root()
        .join(profile_id)
        .join("scores")
        .join("local")
}

fn local_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    local_scores_root_for_profile(profile_id).join("index.bin")
}

fn load_local_score_index_file(path: &Path) -> Option<LocalScoreIndex> {
    let bytes = fs::read(path).ok()?;
    let (file, _) =
        bincode::decode_from_slice::<LocalScoreIndexFile, _>(&bytes, bincode::config::standard())
            .ok()?;
    if file.version != LOCAL_SCORE_INDEX_VERSION {
        return None;
    }
    Some(file.index)
}

fn save_local_score_index_file(path: &Path, index: &LocalScoreIndex) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(parent) {
        warn!("Failed to create local score index dir {parent:?}: {e}");
        return;
    }
    let file = LocalScoreIndexFile {
        version: LOCAL_SCORE_INDEX_VERSION,
        index: index.clone(),
    };
    let Ok(buf) = bincode::encode_to_vec(file, bincode::config::standard()) else {
        warn!("Failed to encode local score index at {path:?}");
        return;
    };
    let tmp_path = path.with_extension("tmp");
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write local score index temp file {tmp_path:?}: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
        warn!("Failed to commit local score index file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
    }
}

fn count_local_score_bins_in_dir(dir: &Path) -> u32 {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return 0;
    };

    let mut total: u32 = 0;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"))
        {
            total = total.saturating_add(1);
        }
    }
    total
}

/// Total number of locally-saved plays for this profile (one `.bin` per play).
pub fn total_songs_played_for_profile(profile_id: &str) -> u32 {
    let root = local_scores_root_for_profile(profile_id);
    if !root.is_dir() {
        return 0;
    }

    // Support both sharded (root/ab/*.bin) and unsharded (root/*.bin) layouts.
    let mut total = count_local_score_bins_in_dir(&root);
    let Ok(read_dir) = fs::read_dir(&root) else {
        return total;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total = total.saturating_add(count_local_score_bins_in_dir(&path));
        }
    }
    total
}

pub fn total_songs_played_for_side(side: profile::PlayerSide) -> u32 {
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return 0;
    };
    total_songs_played_for_profile(&profile_id)
}

#[inline(always)]
fn parse_local_score_filename(name: &str) -> Option<(&str, i64)> {
    if !name.ends_with(".bin") {
        return None;
    }
    let base = &name[..name.len().saturating_sub(4)];
    let idx_dash = base.rfind('-')?;
    if idx_dash == 0 {
        return None;
    }
    let played_at_ms = base[(idx_dash + 1)..].parse::<i64>().ok()?;
    Some((&base[..idx_dash], played_at_ms))
}

fn collect_recent_plays_in_dir(dir: &Path, latest_by_chart: &mut HashMap<String, i64>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((chart_hash, played_at_ms)) = parse_local_score_filename(name) else {
            continue;
        };
        match latest_by_chart.get_mut(chart_hash) {
            Some(existing) => {
                if played_at_ms > *existing {
                    *existing = played_at_ms;
                }
            }
            None => {
                latest_by_chart.insert(chart_hash.to_string(), played_at_ms);
            }
        }
    }
}

fn collect_recent_plays_in_root(root: &Path, latest_by_chart: &mut HashMap<String, i64>) {
    collect_recent_plays_in_dir(root, latest_by_chart);
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recent_plays_in_dir(&path, latest_by_chart);
        }
    }
}

/// Returns chart hashes ordered by latest local play time (most recent first),
/// aggregated across all local profiles.
pub fn recent_played_chart_hashes_for_machine() -> Vec<String> {
    let profiles_root = dirs::app_dirs().profiles_root();
    let Ok(read_dir) = fs::read_dir(&profiles_root) else {
        return Vec::new();
    };

    let mut latest_by_chart: HashMap<String, i64> = HashMap::new();
    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        let local_root = profile_dir.join("scores").join("local");
        if local_root.is_dir() {
            collect_recent_plays_in_root(&local_root, &mut latest_by_chart);
        }
    }

    let mut ranked: Vec<(i64, String)> = latest_by_chart
        .into_iter()
        .map(|(chart_hash, played_at_ms)| (played_at_ms, chart_hash))
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .map(|(_, chart_hash)| chart_hash)
        .collect()
}

fn collect_play_counts_in_dir(dir: &Path, counts_by_chart: &mut HashMap<String, u32>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((chart_hash, _played_at_ms)) = parse_local_score_filename(name) else {
            continue;
        };
        counts_by_chart
            .entry(chart_hash.to_string())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
}

fn collect_play_counts_in_root(root: &Path, counts_by_chart: &mut HashMap<String, u32>) {
    collect_play_counts_in_dir(root, counts_by_chart);
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_play_counts_in_dir(&path, counts_by_chart);
        }
    }
}

/// Returns `(chart_hash, play_count)` pairs ordered by play count descending,
/// aggregated across all local profiles.
pub fn played_chart_counts_for_machine() -> Vec<(String, u32)> {
    let profiles_root = dirs::app_dirs().profiles_root();
    let Ok(read_dir) = fs::read_dir(&profiles_root) else {
        return Vec::new();
    };

    let mut counts_by_chart: HashMap<String, u32> = HashMap::new();
    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        let local_root = profile_dir.join("scores").join("local");
        if local_root.is_dir() {
            collect_play_counts_in_root(&local_root, &mut counts_by_chart);
        }
    }

    let mut ranked: Vec<(String, u32)> = counts_by_chart.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

/// Returns chart hashes ordered by latest local play time for a single profile.
pub fn recent_played_chart_hashes_for_profile(profile_id: &str) -> Vec<String> {
    let profiles_root = dirs::app_dirs().profiles_root();
    let local_root = profiles_root.join(profile_id).join("scores").join("local");
    if !local_root.is_dir() {
        return Vec::new();
    }

    let mut latest_by_chart: HashMap<String, i64> = HashMap::new();
    collect_recent_plays_in_root(&local_root, &mut latest_by_chart);

    let mut ranked: Vec<(i64, String)> = latest_by_chart
        .into_iter()
        .map(|(chart_hash, played_at_ms)| (played_at_ms, chart_hash))
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .map(|(_, chart_hash)| chart_hash)
        .collect()
}

/// Returns `(chart_hash, play_count)` pairs for a single profile, ordered by
/// play count descending.
pub fn played_chart_counts_for_profile(profile_id: &str) -> Vec<(String, u32)> {
    let profiles_root = dirs::app_dirs().profiles_root();
    let local_root = profiles_root.join(profile_id).join("scores").join("local");
    if !local_root.is_dir() {
        return Vec::new();
    }

    let mut counts_by_chart: HashMap<String, u32> = HashMap::new();
    collect_play_counts_in_root(&local_root, &mut counts_by_chart);

    let mut ranked: Vec<(String, u32)> = counts_by_chart.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

#[inline(always)]
fn shard2_for_hash(hash: &str) -> &str {
    if hash.len() >= 2 { &hash[..2] } else { "00" }
}

#[inline(always)]
fn is_better_itg(new: &CachedScore, old: &CachedScore) -> bool {
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
pub(crate) fn same_score_10000(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a.round() - b.round()).abs() <= 1.0
}

#[inline(always)]
fn cached_score_10000(score: &CachedScore) -> f64 {
    score.score_percent * 10000.0
}

#[inline(always)]
fn authoritative_failed_score<'a>(a: &'a CachedScore, b: &'a CachedScore) -> Option<CachedScore> {
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
fn is_better_scalar(new: BestScalar, old: BestScalar) -> bool {
    match (old.grade == Grade::Failed, new.grade == Grade::Failed) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    new.percent > old.percent
}

fn ensure_local_score_cache_loaded(profile_id: &str) {
    if LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .contains_key(profile_id)
    {
        return;
    }

    let load_started = Instant::now();
    let loaded = load_local_score_index(&local_scores_root_for_profile(profile_id));
    let loaded_itg = loaded.best_itg.len();
    let loaded_ex = loaded.best_ex.len();
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    if load_ms >= 25.0 {
        debug!(
            "Loaded local score cache for profile {profile_id}: ITG={loaded_itg}, EX={loaded_ex} in {load_ms:.2}ms."
        );
    }
    let mut state = LOCAL_SCORE_CACHE.lock().unwrap();
    state
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(loaded);
}

fn profile_initials_for_id(profile_id: &str) -> Option<String> {
    let ini_path = dirs::app_dirs()
        .profiles_root()
        .join(profile_id)
        .join("profile.ini");
    if !ini_path.is_file() {
        return None;
    }
    let mut ini = SimpleIni::new();
    if ini.load(&ini_path).is_err() {
        return None;
    }
    let s = ini.get("userprofile", "PlayerInitials")?;
    let s = profile::sanitize_player_initials(&s);
    (!s.is_empty()).then_some(s)
}

fn ensure_machine_local_score_cache_loaded() {
    let needs_load = {
        let state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
        !state.loaded
    };
    if !needs_load {
        return;
    }

    let load_started = Instant::now();
    let mut best_itg: HashMap<String, MachineBest> = HashMap::new();
    for p in profile::scan_local_profiles() {
        let initials = profile_initials_for_id(&p.id).unwrap_or_else(|| "----".to_string());
        let idx = load_local_score_index(&local_scores_root_for_profile(&p.id));
        for (chart_hash, score) in idx.best_itg {
            match best_itg.get_mut(&chart_hash) {
                Some(existing) => {
                    if is_better_itg(&score, &existing.score) {
                        existing.score = score;
                        existing.initials.clone_from(&initials);
                    }
                }
                None => {
                    best_itg.insert(
                        chart_hash,
                        MachineBest {
                            score,
                            initials: initials.clone(),
                        },
                    );
                }
            }
        }
    }

    let mut state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
    if !state.loaded {
        state.loaded = true;
        state.best_itg = best_itg;
        let total = state.best_itg.len();
        let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
        if load_ms >= 25.0 {
            debug!("Loaded machine local score cache: {total} chart(s) in {load_ms:.2}ms.");
        }
    }
}

pub fn prewarm_select_music_score_caches() {
    let started = Instant::now();

    let p1_profile_id = profile::active_local_profile_id_for_side(profile::PlayerSide::P1);
    let p2_profile_id = profile::active_local_profile_id_for_side(profile::PlayerSide::P2);

    if let Some(profile_id) = p1_profile_id.as_deref() {
        ensure_local_score_cache_loaded(profile_id);
        ensure_gs_score_cache_loaded_for_profile(profile_id);
        ensure_ac_score_cache_loaded_for_profile(profile_id);
    }
    if let Some(profile_id) = p2_profile_id.as_deref()
        && p1_profile_id.as_deref() != Some(profile_id)
    {
        ensure_local_score_cache_loaded(profile_id);
        ensure_gs_score_cache_loaded_for_profile(profile_id);
        ensure_ac_score_cache_loaded_for_profile(profile_id);
    }

    ensure_machine_local_score_cache_loaded();

    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    debug!("Prewarmed SelectMusic score caches in {elapsed_ms:.2}ms.");
}

fn update_machine_cache_if_loaded(chart_hash: &str, score: CachedScore, initials: &str) {
    let mut state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
    if !state.loaded {
        return;
    }
    match state.best_itg.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_itg(&score, &existing.score) {
                existing.score = score;
                existing.initials = initials.to_string();
            }
        }
        None => {
            state.best_itg.insert(
                chart_hash.to_string(),
                MachineBest {
                    score,
                    initials: initials.to_string(),
                },
            );
        }
    }
}

pub fn get_cached_local_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_local_score_cache_loaded(&profile_id);
    LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|idx| idx.best_itg.get(chart_hash).copied())
}

pub fn get_cached_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedScore> {
    let local = get_cached_local_score_for_side(chart_hash, side);
    let gs = get_cached_gs_score_for_side(chart_hash, side);
    let ac = get_cached_ac_scores_for_side(chart_hash, side)
        .and_then(|s| s.itg)
        .map(|ac| ac.to_cached_score());
    // Merge by picking the "best ITG" entry; failed scores win when their
    // numeric percent matches a cached non-failed score (parity with prior
    // local+gs merge semantics).
    [local, gs, ac].into_iter().flatten().reduce(|a, b| {
        authoritative_failed_score(&a, &b)
            .unwrap_or_else(|| if is_better_itg(&a, &b) { a } else { b })
    })
}

fn get_cached_local_scalar_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    hard_ex: bool,
) -> Option<LocalScalarScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_local_score_cache_loaded(&profile_id);
    let state = LOCAL_SCORE_CACHE.lock().unwrap();
    let index = state.loaded_profiles.get(&profile_id)?;
    let best = if hard_ex {
        index.best_hard_ex.get(chart_hash)
    } else {
        index.best_ex.get(chart_hash)
    }?;
    Some(LocalScalarScore {
        percent: best.percent,
        is_fail: best.grade == Grade::Failed,
    })
}

pub fn get_cached_local_ex_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<LocalScalarScore> {
    get_cached_local_scalar_score_for_side(chart_hash, side, false)
}

pub fn get_cached_local_hard_ex_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<LocalScalarScore> {
    get_cached_local_scalar_score_for_side(chart_hash, side, true)
}

#[inline(always)]
pub fn is_gs_get_scores_service_allowed() -> bool {
    crate::config::get().enable_groovestats
}

#[inline(always)]
pub fn is_gs_active_for_side(side: profile::PlayerSide) -> bool {
    if !is_gs_get_scores_service_allowed() || !profile::is_session_side_joined(side) {
        return false;
    }
    !profile::get_for_side(side)
        .groovestats_api_key
        .trim()
        .is_empty()
}

pub fn get_machine_record_local(chart_hash: &str) -> Option<(String, CachedScore)> {
    ensure_machine_local_score_cache_loaded();
    let state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
    state
        .best_itg
        .get(chart_hash)
        .map(|m| (m.initials.clone(), m.score))
}

// --- On-disk GrooveStats score storage ---

#[derive(Debug, Clone, Encode, Decode)]
struct GsScoreEntryV1 {
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    username: String,
    fetched_at_ms: i64,
}

#[derive(Debug, Clone, Encode, Decode)]
struct GsScoreEntry {
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
    username: String,
    fetched_at_ms: i64,
}

const fn grade_to_code(g: Grade) -> u8 {
    match g {
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

const fn grade_from_code(code: u8) -> Grade {
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

fn entry_from_cached(score: CachedScore, username: &str, fetched_at_ms: i64) -> GsScoreEntry {
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

fn cached_from_entry(entry: &GsScoreEntry) -> CachedScore {
    fix_gs_cached_score(cached_score(
        grade_from_code(entry.grade_code),
        entry.score_percent,
        entry.lamp_index,
        entry.lamp_judge_count,
    ))
}

fn decode_gs_score_entry(bytes: &[u8]) -> Option<GsScoreEntry> {
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

fn scan_gs_scores_dir(dir: &Path, best_by_chart: &mut HashMap<String, CachedScore>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for item in read_dir.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".bin") {
            continue;
        }
        let base = &name[..name.len().saturating_sub(4)];
        let Some(idx) = base.rfind('-') else {
            continue;
        };
        if idx == 0 {
            continue;
        }
        let chart_hash = &base[..idx];

        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Some(entry) = decode_gs_score_entry(&bytes) else {
            continue;
        };
        let cached = cached_from_entry(&entry);

        match best_by_chart.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_itg(&cached, existing) {
                    *existing = cached;
                }
            }
            None => {
                best_by_chart.insert(chart_hash.to_string(), cached);
            }
        }
    }
}

fn best_scores_from_disk(dir: &Path) -> HashMap<String, CachedScore> {
    let mut best_by_chart: HashMap<String, CachedScore> = HashMap::new();

    if !dir.is_dir() {
        return best_by_chart;
    }

    // Sharded layout only: root/ab/*.bin
    let Ok(read_dir) = fs::read_dir(dir) else {
        return best_by_chart;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_gs_scores_dir(&path, &mut best_by_chart);
        }
    }

    best_by_chart
}

fn load_all_entries_for_chart(chart_hash: &str, dir: &Path) -> Vec<GsScoreEntry> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let prefix = format!("{chart_hash}-");
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".bin") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        if let Some(entry) = decode_gs_score_entry(&bytes) {
            entries.push(entry);
        }
    }
    entries
}

fn append_gs_score_on_disk_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
) {
    if username.trim().is_empty() {
        return;
    }
    let dir = gs_scores_dir_for_profile_and_hash(profile_id, chart_hash);

    let mut entries = load_all_entries_for_chart(chart_hash, &dir);
    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let new_entry = entry_from_cached(score, username, fetched_at_ms);

    let epsilon = 1e-9_f64;
    for existing in &entries {
        if existing.username.eq_ignore_ascii_case(username)
            && (existing.score_percent - new_entry.score_percent).abs() <= epsilon
            && existing.lamp_index == new_entry.lamp_index
            && existing.lamp_judge_count == new_entry.lamp_judge_count
            && existing.grade_code == new_entry.grade_code
        {
            return;
        }
    }

    entries.push(new_entry.clone());

    if let Err(e) = fs::create_dir_all(&dir) {
        warn!("Failed to create GrooveStats scores dir for profile {profile_id} at {dir:?}: {e}");
        return;
    }

    let file_name = format!("{chart_hash}-{fetched_at_ms}.bin");
    let path = dir.join(file_name);

    match bincode::encode_to_vec(&new_entry, bincode::config::standard()) {
        Ok(buf) => {
            if let Err(e) = fs::write(&path, buf) {
                warn!("Failed to write GrooveStats score file {path:?}: {e}");
            } else {
                debug!("Stored GrooveStats score on disk for chart {chart_hash} at {path:?}");
            }
        }
        Err(e) => {
            warn!("Failed to encode GrooveStats score for chart {chart_hash}: {e}");
        }
    }
}

// --- On-disk local score storage (one file per play) ---

const LOCAL_SCORE_VERSION_V1: u16 = 1;

#[derive(Debug, Clone, Copy, Encode, Decode)]
struct LocalReplayEdgeV1 {
    event_music_time_ns: gameplay::SongTimeNs,
    lane: u8,
    pressed: bool,
    // 0 = Keyboard, 1 = Gamepad
    source: u8,
}

#[derive(Debug, Clone, Encode, Decode)]
struct LocalScoreEntryHeaderV1 {
    version: u16,
    played_at_ms: i64,
    music_rate: f32,
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
    ex_score_percent: f64,
    hard_ex_score_percent: f64,
    // Fantastic, Excellent, Great, Decent, WayOff, Miss (row judgments)
    judgment_counts: [u32; 6],
    holds_held: u32,
    holds_total: u32,
    rolls_held: u32,
    rolls_total: u32,
    mines_avoided: u32,
    mines_total: u32,
    hands_achieved: u32,
    fail_time: Option<f32>,
    beat0_time_ns: gameplay::SongTimeNs,
}

#[derive(Debug, Clone, Encode, Decode)]
struct LocalScoreEntryV1 {
    version: u16,
    played_at_ms: i64,
    music_rate: f32,
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
    ex_score_percent: f64,
    hard_ex_score_percent: f64,
    judgment_counts: [u32; 6],
    holds_held: u32,
    holds_total: u32,
    rolls_held: u32,
    rolls_total: u32,
    mines_avoided: u32,
    mines_total: u32,
    hands_achieved: u32,
    fail_time: Option<f32>,
    beat0_time_ns: gameplay::SongTimeNs,
    replay: Vec<LocalReplayEdgeV1>,
}

#[inline(always)]
fn local_lamp_judge_count(count: u32) -> Option<u8> {
    if (1..=9).contains(&count) {
        Some(count as u8)
    } else {
        None
    }
}

fn compute_local_lamp(
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
fn cached_score(
    grade: Grade,
    score_percent: f64,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
) -> CachedScore {
    if grade == Grade::Failed {
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
const fn non_quint_grade(grade: Grade) -> Grade {
    match grade {
        Grade::Quint => Grade::Tier01,
        _ => grade,
    }
}

#[inline(always)]
fn fix_quint_grade_ex(grade: Grade, ex_score_percent: f64) -> Grade {
    if grade == Grade::Failed {
        Grade::Failed
    } else {
        promote_quint_grade(non_quint_grade(grade), ex_score_percent)
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
fn fix_gs_cached_score(score: CachedScore) -> CachedScore {
    cached_score(
        fix_quint_grade_lamp(score.grade, score.score_percent, score.lamp_index),
        score.score_percent,
        score.lamp_index,
        score.lamp_judge_count,
    )
}

#[inline(always)]
fn cached_local_score_from_header(h: &LocalScoreEntryHeaderV1) -> CachedScore {
    let grade = fix_quint_grade_ex(
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

fn decode_local_score_header(bytes: &[u8]) -> Option<LocalScoreEntryHeaderV1> {
    let Ok((h, _)) = bincode::decode_from_slice::<LocalScoreEntryHeaderV1, _>(
        bytes,
        bincode::config::standard(),
    ) else {
        return None;
    };
    if h.version != LOCAL_SCORE_VERSION_V1 {
        return None;
    }
    Some(h)
}

fn read_local_score_header(path: &Path) -> Option<LocalScoreEntryHeaderV1> {
    // Local score files include replay data; for indexing we only need the prefix.
    // A 1KiB prefix comfortably covers the fixed header fields.
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(1024);
    if file.take(1024).read_to_end(&mut buf).is_err() || buf.is_empty() {
        return None;
    }
    decode_local_score_header(&buf)
}

fn read_local_score_entry(path: &Path) -> Option<LocalScoreEntryV1> {
    let bytes = fs::read(path).ok()?;
    let (entry, _) =
        bincode::decode_from_slice::<LocalScoreEntryV1, _>(&bytes, bincode::config::standard())
            .ok()?;
    if entry.version != LOCAL_SCORE_VERSION_V1 {
        return None;
    }
    Some(entry)
}

fn scan_local_scores_dir(dir: &Path, index: &mut LocalScoreIndex) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for item in read_dir.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".bin") {
            continue;
        }
        let base = &name[..name.len().saturating_sub(4)];
        let Some(idx_dash) = base.rfind('-') else {
            continue;
        };
        if idx_dash == 0 {
            continue;
        }
        let chart_hash = &base[..idx_dash];

        let Some(h) = read_local_score_header(&path) else {
            continue;
        };

        let cached = cached_local_score_from_header(&h);
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

        let ex = BestScalar {
            grade,
            percent: h.ex_score_percent,
        };
        match index.best_ex.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_scalar(ex, *existing) {
                    *existing = ex;
                }
            }
            None => {
                index.best_ex.insert(chart_hash.to_string(), ex);
            }
        }

        let hard_ex = BestScalar {
            grade,
            percent: h.hard_ex_score_percent,
        };
        match index.best_hard_ex.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_scalar(hard_ex, *existing) {
                    *existing = hard_ex;
                }
            }
            None => {
                index.best_hard_ex.insert(chart_hash.to_string(), hard_ex);
            }
        }
    }
}

fn load_local_score_index(root: &Path) -> LocalScoreIndex {
    if !root.is_dir() {
        return LocalScoreIndex::default();
    }
    let index_path = root.join("index.bin");
    if let Some(index) = load_local_score_index_file(&index_path) {
        return index;
    }

    let mut index = LocalScoreIndex::default();

    // Support both sharded (root/ab/*.bin) and unsharded (root/*.bin) layouts.
    scan_local_scores_dir(root, &mut index);
    let Ok(read_dir) = fs::read_dir(root) else {
        return index;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_local_scores_dir(&path, &mut index);
        }
    }

    save_local_score_index_file(&index_path, &index);
    index
}

fn update_local_index_with_header(
    idx: &mut LocalScoreIndex,
    chart_hash: &str,
    h: &LocalScoreEntryHeaderV1,
) {
    let cached = cached_local_score_from_header(h);
    let grade = cached.grade;
    match idx.best_itg.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_itg(&cached, existing) {
                *existing = cached;
            }
        }
        None => {
            idx.best_itg.insert(chart_hash.to_string(), cached);
        }
    }

    let ex = BestScalar {
        grade,
        percent: h.ex_score_percent,
    };
    match idx.best_ex.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_scalar(ex, *existing) {
                *existing = ex;
            }
        }
        None => {
            idx.best_ex.insert(chart_hash.to_string(), ex);
        }
    }

    let hard_ex = BestScalar {
        grade,
        percent: h.hard_ex_score_percent,
    };
    match idx.best_hard_ex.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_scalar(hard_ex, *existing) {
                *existing = hard_ex;
            }
        }
        None => {
            idx.best_hard_ex.insert(chart_hash.to_string(), hard_ex);
        }
    }
}

fn append_local_score_on_disk(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut LocalScoreEntryV1,
) {
    let shard = shard2_for_hash(chart_hash);
    let dir = local_scores_root_for_profile(profile_id).join(shard);
    if let Err(e) = fs::create_dir_all(&dir) {
        warn!("Failed to create local scores dir {dir:?}: {e}");
        return;
    }

    // Avoid collisions: keep the GS-like "<hash>-<ms>.bin" filename shape.
    let mut played_at_ms = entry.played_at_ms;
    let mut path = dir.join(format!("{chart_hash}-{played_at_ms}.bin"));
    while path.exists() {
        played_at_ms = played_at_ms.saturating_add(1);
        path = dir.join(format!("{chart_hash}-{played_at_ms}.bin"));
    }
    entry.played_at_ms = played_at_ms;

    let tmp_path = dir.join(format!(".{chart_hash}-{played_at_ms}.tmp"));
    let Ok(buf) = bincode::encode_to_vec(&*entry, bincode::config::standard()) else {
        warn!("Failed to encode local score for chart {chart_hash}");
        return;
    };
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write local score temp file {tmp_path:?}: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        warn!("Failed to commit local score file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
        return;
    }

    // Update in-memory cache if it's already loaded for this profile.
    let header = LocalScoreEntryHeaderV1 {
        version: entry.version,
        played_at_ms: entry.played_at_ms,
        music_rate: entry.music_rate,
        score_percent: entry.score_percent,
        grade_code: entry.grade_code,
        lamp_index: entry.lamp_index,
        lamp_judge_count: entry.lamp_judge_count,
        ex_score_percent: entry.ex_score_percent,
        hard_ex_score_percent: entry.hard_ex_score_percent,
        judgment_counts: entry.judgment_counts,
        holds_held: entry.holds_held,
        holds_total: entry.holds_total,
        rolls_held: entry.rolls_held,
        rolls_total: entry.rolls_total,
        mines_avoided: entry.mines_avoided,
        mines_total: entry.mines_total,
        hands_achieved: entry.hands_achieved,
        fail_time: entry.fail_time,
        beat0_time_ns: entry.beat0_time_ns,
    };
    let loaded_snapshot = {
        let mut state = LOCAL_SCORE_CACHE.lock().unwrap();
        if let Some(idx) = state.loaded_profiles.get_mut(profile_id) {
            update_local_index_with_header(idx, chart_hash, &header);
            Some(idx.clone())
        } else {
            None
        }
    };
    if let Some(index) = loaded_snapshot {
        save_local_score_index_file(&local_score_index_path_for_profile(profile_id), &index);
    } else {
        let index_path = local_score_index_path_for_profile(profile_id);
        let mut index = load_local_score_index_file(&index_path).unwrap_or_default();
        update_local_index_with_header(&mut index, chart_hash, &header);
        save_local_score_index_file(&index_path, &index);
    }

    let cached = cached_local_score_from_header(&header);
    update_machine_cache_if_loaded(chart_hash, cached, profile_initials);
}

fn judgment_counts_arr(p: &gameplay::PlayerRuntime) -> [u32; 6] {
    p.judgment_counts
}

fn replay_edges_for_player(gs: &gameplay::State, player: usize) -> Vec<LocalReplayEdgeV1> {
    if player >= gs.num_players {
        return Vec::new();
    }

    let (col_start, col_end) = if gs.num_players <= 1 {
        (0usize, gs.num_cols)
    } else {
        let start = player.saturating_mul(gs.cols_per_player);
        (start, start.saturating_add(gs.cols_per_player))
    };

    let mut out = Vec::new();
    out.reserve(gs.replay_edges.len().min(4096));
    for e in &gs.replay_edges {
        let lane = e.lane_index as usize;
        if lane < col_start
            || lane >= col_end
            || gameplay::song_time_ns_invalid(e.event_music_time_ns)
        {
            continue;
        }
        let source = match e.source {
            InputSource::Keyboard => 0,
            InputSource::Gamepad => 1,
        };
        out.push(LocalReplayEdgeV1 {
            event_music_time_ns: e.event_music_time_ns,
            lane: (lane - col_start) as u8,
            pressed: e.pressed,
            source,
        });
    }
    out
}

pub fn save_local_scores_from_gameplay(gs: &gameplay::State) {
    if gs.autoplay_used {
        debug!("Skipping local score save: autoplay was used during this stage.");
        return;
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // Parameter retained for parity with Simply Love helpers; currently unused.
    let mines_disabled = false;

    for player_idx in 0..gs.num_players {
        let side = if gs.num_players >= 2 {
            if player_idx == 0 {
                profile::PlayerSide::P1
            } else {
                profile::PlayerSide::P2
            }
        } else {
            profile::get_session_player_side()
        };

        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        if !gs.score_valid[player_idx] {
            let reasons = gameplay::score_invalid_reason_lines_for_chart(
                &gs.charts[player_idx],
                &gs.player_profiles[player_idx],
                gs.scroll_speed[player_idx],
                gs.music_rate,
            );
            let detail = if reasons.is_empty() {
                "ranking-invalid modifiers were used".to_string()
            } else {
                reasons.join("; ")
            };
            debug!(
                "Skipping local score save for player {}: {}.",
                player_idx + 1,
                detail
            );
            continue;
        }

        let chart_hash = gs.charts[player_idx].short_hash.as_str();
        let p = &gs.players[player_idx];

        let score_percent = judgment::calculate_itg_score_percent_from_counts(
            &p.scoring_counts,
            p.holds_held_for_score,
            p.rolls_held_for_score,
            p.mines_hit_for_score,
            gs.possible_grade_points[player_idx],
        );

        let mut grade = if gameplay_run_passed(
            gs.song_completed_naturally,
            p.is_failing,
            p.life,
            p.fail_time.is_some(),
        ) {
            score_to_grade(score_percent * 10000.0)
        } else {
            Grade::Failed
        };

        let (start, end) = gs.note_ranges[player_idx];
        let notes = &gs.notes[start..end];
        let note_times = &gs.note_time_cache_ns[start..end];
        let hold_end_times = &gs.hold_end_time_cache_ns[start..end];

        let ex_score_percent = judgment::calculate_ex_score_from_notes(
            notes,
            note_times,
            hold_end_times,
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            p.fail_time.map(gameplay::song_time_ns_from_seconds),
            mines_disabled,
        );
        let hard_ex_score_percent = judgment::calculate_hard_ex_score_from_notes(
            notes,
            note_times,
            hold_end_times,
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            p.fail_time.map(gameplay::song_time_ns_from_seconds),
            mines_disabled,
        );

        // Quint comes from the achieved result, not the active UI display mode.
        grade = promote_quint_grade(grade, ex_score_percent);

        let counts = judgment_counts_arr(p);
        let white_fantastics = Some(gs.live_window_counts[player_idx].w1);
        let (lamp_index, lamp_judge_count) = compute_local_lamp(counts, grade, white_fantastics);
        let replay = replay_edges_for_player(gs, player_idx);

        let mut entry = LocalScoreEntryV1 {
            version: LOCAL_SCORE_VERSION_V1,
            played_at_ms: now_ms,
            music_rate: gs.music_rate,
            score_percent,
            grade_code: grade_to_code(grade),
            lamp_index,
            lamp_judge_count,
            ex_score_percent,
            hard_ex_score_percent,
            judgment_counts: counts,
            holds_held: p.holds_held,
            holds_total: gs.holds_total[player_idx],
            rolls_held: p.rolls_held,
            rolls_total: gs.rolls_total[player_idx],
            mines_avoided: p.mines_avoided,
            mines_total: gs.mines_total[player_idx],
            hands_achieved: p.hands_achieved,
            fail_time: p.fail_time,
            beat0_time_ns: gs.timing_players[player_idx].get_time_for_beat_ns(0.0),
            replay,
        };

        append_local_score_on_disk(
            &profile_id,
            gs.player_profiles[player_idx].player_initials.as_str(),
            chart_hash,
            &mut entry,
        );
    }
}

const GROOVESTATS_SUBMIT_MAX_ENTRIES: usize = 10;
const GROOVESTATS_COMMENT_PREFIX: &str = "[DS]";
// Mirrors zmod's old-api submit path bit layout from gameplay.rs/player options.
const GS_INVALID_REMOVE_MASK: u8 =
    (1u8 << 0) | (1u8 << 2) | (1u8 << 3) | (1u8 << 4) | (1u8 << 5) | (1u8 << 6) | (1u8 << 7);
const GS_INVALID_INSERT_MASK: u8 = u8::MAX;
const GS_INVALID_HOLDS_MASK: u8 = 1u8 << 3;
const GROOVESTATS_REASON_COUNT: usize = 13;
const GROOVESTATS_CHART_HASH_VERSION: u8 = 3;

#[inline(always)]
pub(super) const fn submit_side_ix(side: profile::PlayerSide) -> usize {
    match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    }
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
pub(super) fn gameplay_side_for_player(
    gs: &gameplay::State,
    player_idx: usize,
) -> profile::PlayerSide {
    if gs.num_players >= 2 {
        if player_idx == 0 {
            profile::PlayerSide::P1
        } else {
            profile::PlayerSide::P2
        }
    } else {
        profile::get_session_player_side()
    }
}

#[inline(always)]
fn compact_f32_text(value: f32) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn log_body_snippet(text: &str) -> String {
    const MAX_LOG_CHARS: usize = 256;
    if text.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(text.len().min(MAX_LOG_CHARS));
    for ch in text.chars().take(MAX_LOG_CHARS) {
        out.push(ch);
    }
    out
}

pub fn save_local_summary_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    music_rate: f32,
    summary: &stage_stats::PlayerStageSummary,
) {
    if chart_hash.trim().is_empty() {
        return;
    }
    if summary.disqualified {
        debug!("Skipping local summary score save: run was disqualified.");
        return;
    }
    if !summary.score_valid {
        debug!("Skipping local summary score save: ranking-invalid modifiers were used.");
        return;
    }
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return;
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let profile_initials = profile::get_for_side(side).player_initials;
    let counts = [
        summary
            .window_counts
            .w0
            .saturating_add(summary.window_counts.w1),
        summary.window_counts.w2,
        summary.window_counts.w3,
        summary.window_counts.w4,
        summary.window_counts.w5,
        summary.window_counts.miss,
    ];
    let (lamp_index, lamp_judge_count) =
        compute_local_lamp(counts, summary.grade, Some(summary.window_counts.w1));
    let mut entry = LocalScoreEntryV1 {
        version: LOCAL_SCORE_VERSION_V1,
        played_at_ms: now_ms,
        music_rate: if music_rate.is_finite() && music_rate > 0.0 {
            music_rate
        } else {
            1.0
        },
        score_percent: summary.score_percent.clamp(0.0, 1.0),
        grade_code: grade_to_code(summary.grade),
        lamp_index,
        lamp_judge_count,
        ex_score_percent: summary.ex_score_percent.clamp(0.0, 100.0),
        hard_ex_score_percent: summary.hard_ex_score_percent.clamp(0.0, 100.0),
        judgment_counts: counts,
        holds_held: 0,
        holds_total: 0,
        rolls_held: 0,
        rolls_total: 0,
        mines_avoided: 0,
        mines_total: 0,
        hands_achieved: 0,
        fail_time: (summary.grade == Grade::Failed).then_some(0.0),
        beat0_time_ns: 0,
        replay: Vec::new(),
    };
    append_local_score_on_disk(
        &profile_id,
        profile_initials.as_str(),
        chart_hash,
        &mut entry,
    );
}

// --- API Response Structs ---

#[derive(Debug, Clone)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub name: String,
    pub machine_tag: Option<String>,
    pub score: f64, // 0..10000
    pub date: String,
    pub is_rival: bool,
    pub is_self: bool,
    pub is_fail: bool,
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

#[derive(Debug, Clone, Copy)]
pub struct ReplayEdge {
    pub event_music_time_ns: gameplay::SongTimeNs,
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
}

#[derive(Debug, Clone)]
pub struct MachineReplayEntry {
    pub rank: u32,
    pub name: String,
    pub score: f64, // 0..10000
    pub date: String,
    pub is_fail: bool,
    pub replay_beat0_time_ns: gameplay::SongTimeNs,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PlayerLeaderboardCacheKey {
    chart_hash: String,
    api_key: String,
    arrowcloud_api_key: String,
    include_arrowcloud: bool,
    show_ex_score: bool,
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
    pub fn from_profile(
        player_profile: &profile::Profile,
        side_joined: bool,
        persistent_profile_id: Option<String>,
    ) -> Self {
        let cfg = crate::config::get();
        let api_key = player_profile.groovestats_api_key.trim().to_string();
        let arrowcloud_api_key = player_profile.arrowcloud_api_key.trim().to_string();
        let include_arrowcloud = cfg.enable_arrowcloud && !arrowcloud_api_key.is_empty();
        let gs_username = player_profile.groovestats_username.trim().to_string();
        let auto_profile_id = if cfg.auto_populate_gs_scores {
            persistent_profile_id.clone()
        } else {
            None
        };
        let should_auto_populate =
            cfg.auto_populate_gs_scores && auto_profile_id.is_some() && !gs_username.is_empty();
        Self {
            display_scorebox: player_profile.display_scorebox,
            gs_active: cfg.enable_groovestats && side_joined && !api_key.is_empty(),
            show_ex_score: player_profile.show_ex_score,
            api_key,
            arrowcloud_api_key,
            include_arrowcloud,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
        }
    }
}

#[derive(Debug, Clone)]
enum PlayerLeaderboardCacheValue {
    Ready(PlayerLeaderboardData),
    Error(String),
}

#[derive(Debug, Clone)]
struct PlayerLeaderboardCacheEntry {
    value: PlayerLeaderboardCacheValue,
    max_entries: usize,
    refreshed_at: Instant,
    retry_after: Option<Instant>,
}

#[derive(Default)]
struct PlayerLeaderboardCacheState {
    by_key: HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheEntry>,
    in_flight: HashMap<PlayerLeaderboardCacheKey, usize>,
    pending_refresh: HashMap<PlayerLeaderboardCacheKey, usize>,
    invalidated_after: HashMap<PlayerLeaderboardCacheKey, Instant>,
}

static PLAYER_LEADERBOARD_CACHE: std::sync::LazyLock<Mutex<PlayerLeaderboardCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(PlayerLeaderboardCacheState::default()));

const PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LeaderboardsApiResponse {
    player1: Option<LeaderboardApiPlayer>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudLeaderboardsApiResponse {
    #[serde(default)]
    leaderboards: Vec<ArrowCloudLeaderboardPane>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudLeaderboardPane {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    scores: Vec<ArrowCloudLeaderboardEntry>,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    page: u32,
    #[serde(default)]
    has_next: bool,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    total_pages: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudLeaderboardEntry {
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    rank: u32,
    #[serde(default, deserialize_with = "de_f64_from_string_or_number")]
    score: f64, // 0..100
    #[serde(default)]
    alias: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    is_rival: bool,
    #[serde(default)]
    is_self: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudUserApiResponse {
    user: ArrowCloudUserApiUser,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudUserApiUser {
    #[serde(default)]
    id: String,
    #[serde(default)]
    rival_user_ids: Vec<String>,
}

#[derive(Debug, Default)]
struct ArrowCloudUserContext {
    self_user_id: Option<String>,
    rival_user_ids: HashSet<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum U32OrString {
    U32(u32),
    F64(f64),
    String(String),
}

fn de_u32_from_string_or_number<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<U32OrString>::deserialize(deserializer)? {
        Some(U32OrString::U32(v)) => Ok(v),
        Some(U32OrString::F64(v)) => Ok(v.max(0.0).floor() as u32),
        Some(U32OrString::String(text)) => Ok(text.trim().parse::<u32>().unwrap_or(0)),
        None => Ok(0),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum I32OrString {
    I32(i32),
    I64(i64),
    F64(f64),
    String(String),
}

fn de_i32_from_string_or_number<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<I32OrString>::deserialize(deserializer)? {
        Some(I32OrString::I32(v)) => Ok(v),
        Some(I32OrString::I64(v)) => Ok(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
        Some(I32OrString::F64(v)) => {
            Ok(v.clamp(f64::from(i32::MIN), f64::from(i32::MAX)).round() as i32)
        }
        Some(I32OrString::String(text)) => Ok(text.trim().parse::<i32>().unwrap_or(0)),
        None => Ok(0),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum F64OrString {
    F64(f64),
    String(String),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum StringOrNumber {
    String(String),
    I64(i64),
    U64(u64),
    F64(f64),
}

fn de_f64_from_string_or_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<F64OrString>::deserialize(deserializer)? {
        Some(F64OrString::F64(v)) => Ok(v),
        Some(F64OrString::String(text)) => Ok(text.trim().parse::<f64>().unwrap_or(0.0)),
        None => Ok(0.0),
    }
}

/// `Option<f64>` variant of [`de_f64_from_string_or_number`]. Returns `None`
/// for missing fields or unparseable strings instead of defaulting to `0.0`,
/// so callers can distinguish "no score" from "actual 0% score".
fn de_optional_f64_from_string_or_number<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<F64OrString>::deserialize(deserializer)? {
        Some(F64OrString::F64(v)) => Ok(Some(v)),
        Some(F64OrString::String(text)) => Ok(text.trim().parse::<f64>().ok()),
        None => Ok(None),
    }
}

/// `Option<i64>` variant accepting either a JSON number or a numeric string.
/// Returns `None` for missing fields or unparseable strings.
fn de_optional_i64_from_string_or_number<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<StringOrNumber>::deserialize(deserializer)? {
        Some(StringOrNumber::I64(v)) => Ok(Some(v)),
        Some(StringOrNumber::U64(v)) => Ok(i64::try_from(v).ok()),
        Some(StringOrNumber::F64(v)) => {
            if v.is_finite() && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                Ok(Some(v as i64))
            } else {
                Ok(None)
            }
        }
        Some(StringOrNumber::String(text)) => Ok(text.trim().parse::<i64>().ok()),
        None => Ok(None),
    }
}

fn de_string_from_string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<StringOrNumber>::deserialize(deserializer)? {
        Some(StringOrNumber::String(text)) => Ok(text),
        Some(StringOrNumber::I64(v)) => Ok(v.to_string()),
        Some(StringOrNumber::U64(v)) => Ok(v.to_string()),
        Some(StringOrNumber::F64(v)) => Ok(compact_f32_text(v as f32)),
        None => Ok(String::new()),
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LeaderboardApiPlayer {
    #[serde(default)]
    is_ranked: bool,
    #[serde(rename = "gsLeaderboard", default)]
    gs_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(rename = "exLeaderboard", default)]
    ex_leaderboard: Vec<LeaderboardApiEntry>,
    rpg: Option<LeaderboardEventData>,
    itl: Option<LeaderboardEventData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LeaderboardEventData {
    #[serde(default)]
    name: String,
    #[serde(rename = "rpgLeaderboard", default)]
    rpg_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(rename = "itlLeaderboard", default)]
    itl_leaderboard: Vec<LeaderboardApiEntry>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct LeaderboardApiEntry {
    #[serde(default)]
    rank: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    machine_tag: Option<String>,
    #[serde(default)]
    score: f64, // 0..10000
    #[serde(default)]
    date: String,
    #[serde(default)]
    is_rival: bool,
    #[serde(default)]
    is_self: bool,
    #[serde(default)]
    is_fail: bool,
    #[serde(default)]
    comments: Option<String>,
}

fn leaderboard_entries_from_api(entries: Vec<LeaderboardApiEntry>) -> Vec<LeaderboardEntry> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        out.push(LeaderboardEntry {
            rank: entry.rank,
            name: entry.name,
            machine_tag: entry.machine_tag,
            score: entry.score,
            date: entry.date,
            is_rival: entry.is_rival,
            is_self: entry.is_self,
            is_fail: entry.is_fail,
        });
    }
    out
}

fn push_leaderboard_pane(
    out: &mut Vec<LeaderboardPane>,
    name: &str,
    entries: Vec<LeaderboardApiEntry>,
    is_ex: bool,
) {
    if entries.is_empty() {
        return;
    }
    out.push(LeaderboardPane {
        name: name.to_string(),
        entries: leaderboard_entries_from_api(entries),
        is_ex,
        disabled: false,
        personalized: true,
        arrowcloud_kind: None,
    });
}

struct FetchedPlayerLeaderboards {
    data: PlayerLeaderboardData,
    gs_entries: Vec<LeaderboardApiEntry>,
    ex_entries: Vec<LeaderboardApiEntry>,
    itl_self_found: bool,
}

#[inline(always)]
fn should_keep_newer_player_leaderboard_entry(
    entry: Option<&PlayerLeaderboardCacheEntry>,
    request_started_at: Instant,
) -> bool {
    entry.is_some_and(|entry| entry.refreshed_at > request_started_at)
}

#[inline(always)]
fn player_leaderboard_request_was_invalidated(
    invalidated_after: Option<Instant>,
    request_started_at: Instant,
) -> bool {
    invalidated_after.is_some_and(|invalidated_after| request_started_at <= invalidated_after)
}

#[inline(always)]
fn leaderboard_self_entry<'a>(
    entries: &'a [LeaderboardApiEntry],
    username: &str,
) -> Option<&'a LeaderboardApiEntry> {
    entries.iter().find(|entry| entry.is_self).or_else(|| {
        (!username.trim().is_empty()).then(|| {
            entries
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(username))
        })?
    })
}

#[inline(always)]
fn entry_score_10000(entry: &LeaderboardApiEntry) -> Option<f64> {
    if entry.is_fail || !entry.score.is_finite() {
        None
    } else {
        Some(entry.score.clamp(0.0, 10000.0))
    }
}

#[inline(always)]
fn leaderboard_self_score_10000(entries: &[LeaderboardApiEntry], username: &str) -> Option<u32> {
    Some(entry_score_10000(leaderboard_self_entry(entries, username)?)?.round() as u32)
}

fn gs_ex_evidence_from_leaderboard(
    ex_entries: &[LeaderboardApiEntry],
    username: &str,
    comment: Option<&str>,
) -> GsExEvidence {
    GsExEvidence::from_sources(
        leaderboard_self_entry(ex_entries, username).and_then(entry_score_10000),
        comment,
    )
}

#[inline(always)]
fn leaderboard_self_rank(entries: &[LeaderboardApiEntry], username: &str) -> Option<u32> {
    let rank = leaderboard_self_entry(entries, username)?.rank;
    (rank != 0).then_some(rank)
}

#[inline(always)]
fn submit_result_improved(result: &str) -> bool {
    result.eq_ignore_ascii_case("score-added") || result.eq_ignore_ascii_case("improved")
}

#[inline(always)]
fn submit_record_banner(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> Option<GrooveStatsSubmitRecordBanner> {
    if !submit_result_improved(response.result.as_str()) {
        return None;
    }
    let use_ex = player.show_ex_score && !response.ex_leaderboard.is_empty();
    let leaderboard = if use_ex {
        response.ex_leaderboard.as_slice()
    } else {
        response.gs_leaderboard.as_slice()
    };
    if leaderboard_self_rank(leaderboard, player.username.as_str()) == Some(1) {
        return Some(if use_ex {
            GrooveStatsSubmitRecordBanner::WorldRecordEx
        } else {
            GrooveStatsSubmitRecordBanner::WorldRecord
        });
    }
    Some(GrooveStatsSubmitRecordBanner::PersonalBest)
}

#[inline(always)]
fn cached_failed_gs_score(score_10000: f64) -> CachedScore {
    cached_score(Grade::Failed, score_10000 / 10000.0, None, None)
}

#[inline(always)]
fn cached_missing_gs_score() -> CachedScore {
    cached_failed_gs_score(0.0)
}

#[inline(always)]
fn cached_score_from_gs(
    score_10000: f64,
    comments: Option<&str>,
    chart_hash: &str,
    is_fail: bool,
    ex_evidence: GsExEvidence,
) -> CachedScore {
    if is_fail {
        return cached_failed_gs_score(score_10000);
    }
    let lamp_index = compute_lamp_index(score_10000, comments, chart_hash, ex_evidence);
    let lamp_judge_count = compute_lamp_judge_count(lamp_index, comments);
    let mut grade = score_to_grade(score_10000);
    if lamp_index == Some(0) {
        grade = promote_quint_grade(grade, 100.0);
    }
    cached_score(grade, score_10000 / 10000.0, lamp_index, lamp_judge_count)
}

#[inline(always)]
fn replaces_stale_inferred_quint(
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

fn cache_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
    proves_nonquint_ex: bool,
) {
    let score = fix_gs_cached_score(score);
    if let Some(existing) = get_cached_gs_score_for_profile(profile_id, chart_hash)
        && !replaces_stale_inferred_quint(&score, &existing, proves_nonquint_ex)
        && !(score.grade == Grade::Failed
            && existing.grade != Grade::Failed
            && same_score_10000(cached_score_10000(&score), cached_score_10000(&existing)))
        && !is_better_itg(&score, &existing)
    {
        return;
    }
    set_cached_gs_score_for_profile(profile_id, chart_hash.to_string(), score);
    if !username.trim().is_empty() {
        append_gs_score_on_disk_for_profile(profile_id, chart_hash, score, username);
    }
}

fn cache_gs_score_from_leaderboard(
    profile_id: &str,
    username: &str,
    chart_hash: &str,
    gs_entries: &[LeaderboardApiEntry],
    ex_entries: &[LeaderboardApiEntry],
) {
    let Some(entry) = leaderboard_self_entry(gs_entries, username) else {
        // Select Music and gameplay scoreboxes fetch shallow leaderboard pages.
        // If the page does not include the player's row, do not clobber an
        // existing cached GS score and make the wheel fall back to local data.
        return;
    };
    let local_failed_match = get_cached_local_score_for_profile(profile_id, chart_hash)
        .filter(|score| {
            score.grade == Grade::Failed && same_score_10000(cached_score_10000(score), entry.score)
        })
        .is_some();
    let ex_evidence =
        gs_ex_evidence_from_leaderboard(ex_entries, username, entry.comments.as_deref());
    let score = cached_score_from_gs(
        entry.score,
        entry.comments.as_deref(),
        chart_hash,
        entry.is_fail || local_failed_match,
        ex_evidence,
    );
    cache_gs_score_for_profile(
        profile_id,
        chart_hash,
        score,
        username,
        ex_evidence.proves_nonquint(),
    );
}

#[inline(always)]
fn arrowcloud_lb_kind(lb_type: &str) -> Option<ArrowCloudPaneKind> {
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

#[inline(always)]
fn arrowcloud_entry_user_id(entry: &ArrowCloudLeaderboardEntry) -> Option<&str> {
    let user_id = entry.user_id.trim();
    (!user_id.is_empty()).then_some(user_id)
}

fn arrowcloud_user_context_from_api(user: ArrowCloudUserApiUser) -> ArrowCloudUserContext {
    let self_user_id = (!user.id.trim().is_empty()).then_some(user.id);
    let rival_user_ids = user
        .rival_user_ids
        .into_iter()
        .map(|user_id| user_id.trim().to_string())
        .filter(|user_id| !user_id.is_empty())
        .collect();
    ArrowCloudUserContext {
        self_user_id,
        rival_user_ids,
    }
}

#[inline(always)]
fn arrowcloud_target_user_ids(context: Option<&ArrowCloudUserContext>) -> HashSet<String> {
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

#[inline(always)]
fn arrowcloud_entry_flags(
    entry: &ArrowCloudLeaderboardEntry,
    context: Option<&ArrowCloudUserContext>,
) -> (bool, bool) {
    let user_id = arrowcloud_entry_user_id(entry);
    let is_self = entry.is_self
        || context
            .and_then(|context| context.self_user_id.as_deref())
            .is_some_and(|self_user_id| user_id == Some(self_user_id));
    let is_rival = entry.is_rival
        || context.is_some_and(|context| {
            user_id.is_some_and(|user_id| context.rival_user_ids.contains(user_id))
        });
    (is_self, is_rival)
}

fn arrowcloud_entry_from_api(
    entry: ArrowCloudLeaderboardEntry,
    is_self: bool,
    is_rival: bool,
) -> LeaderboardEntry {
    let score = if entry.score.is_finite() {
        (entry.score * 100.0).clamp(0.0, 10000.0)
    } else {
        0.0
    };
    LeaderboardEntry {
        rank: entry.rank,
        name: entry.alias,
        machine_tag: None,
        score,
        date: entry.date,
        is_rival,
        is_self,
        is_fail: false,
    }
}

fn arrowcloud_hard_ex_pane_from_response(
    decoded: ArrowCloudLeaderboardsApiResponse,
) -> Option<ArrowCloudLeaderboardPane> {
    decoded
        .leaderboards
        .into_iter()
        .find(|pane| arrowcloud_lb_kind(pane.r#type.as_str()) == Some(ArrowCloudPaneKind::HardEx))
}

fn arrowcloud_update_remaining_targets(
    scores: &[ArrowCloudLeaderboardEntry],
    context: Option<&ArrowCloudUserContext>,
    remaining: &mut HashSet<String>,
) {
    if remaining.is_empty() {
        return;
    }
    for entry in scores {
        let Some(user_id) = arrowcloud_entry_user_id(entry) else {
            continue;
        };
        let (is_self, is_rival) = arrowcloud_entry_flags(entry, context);
        if is_self || is_rival {
            remaining.remove(user_id);
            if remaining.is_empty() {
                break;
            }
        }
    }
}

fn arrowcloud_hard_ex_pane_from_pages(
    first_page: ArrowCloudLeaderboardPane,
    extra_pages: Vec<ArrowCloudLeaderboardPane>,
    context: Option<&ArrowCloudUserContext>,
) -> LeaderboardPane {
    let mut entries = Vec::with_capacity(first_page.scores.len());
    let mut appended_user_ids = HashSet::new();

    for entry in first_page.scores {
        let user_id = arrowcloud_entry_user_id(&entry).map(str::to_owned);
        let (is_self, is_rival) = arrowcloud_entry_flags(&entry, context);
        if (is_self || is_rival)
            && let Some(user_id) = user_id
        {
            appended_user_ids.insert(user_id);
        }
        entries.push(arrowcloud_entry_from_api(entry, is_self, is_rival));
    }

    for page in extra_pages {
        for entry in page.scores {
            let user_id = arrowcloud_entry_user_id(&entry).map(str::to_owned);
            let (is_self, is_rival) = arrowcloud_entry_flags(&entry, context);
            if !(is_self || is_rival) {
                continue;
            }
            if let Some(user_id) = user_id
                && !appended_user_ids.insert(user_id)
            {
                continue;
            }
            entries.push(arrowcloud_entry_from_api(entry, is_self, is_rival));
        }
    }
    let personalized = entries.iter().any(|entry| entry.is_self || entry.is_rival);

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
fn empty_arrowcloud_hard_ex_pane() -> LeaderboardPane {
    LeaderboardPane {
        name: "ArrowCloud".to_string(),
        entries: Vec::new(),
        is_ex: false,
        disabled: false,
        personalized: false,
        arrowcloud_kind: Some(ArrowCloudPaneKind::HardEx),
    }
}

fn fetch_arrowcloud_json<T: DeserializeOwned>(
    api_url: &str,
    api_key: Option<&str>,
    page: Option<u32>,
) -> Result<Option<T>, network::NetworkError> {
    let mut request = network::get_agent().get(api_url);
    if let Some(page) = page.filter(|page| *page > 1) {
        let page = page.to_string();
        request = request.query("page", &page);
    }
    if let Some(api_key) = api_key.map(str::trim).filter(|api_key| !api_key.is_empty()) {
        let bearer = format!("Bearer {api_key}");
        request = request.header("Authorization", &bearer);
    }
    let response = request
        .config()
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| match error {
            ureq::Error::StatusCode(status) => network::NetworkError::HttpStatus(status),
            other => {
                let message = other.to_string();
                let lower = message.to_ascii_lowercase();
                if lower.contains("timeout") || lower.contains("timed out") {
                    network::NetworkError::Timeout
                } else {
                    network::NetworkError::Request(message)
                }
            }
        })?;
    match response.status().as_u16() {
        200 => response
            .into_body()
            .read_json()
            .map(Some)
            .map_err(|error| network::NetworkError::Decode(error.to_string())),
        404 => Ok(None),
        status => Err(network::NetworkError::HttpStatus(status)),
    }
}

fn fetch_arrowcloud_leaderboards(
    api_url: &str,
    page: Option<u32>,
) -> Result<Option<ArrowCloudLeaderboardsApiResponse>, network::NetworkError> {
    fetch_arrowcloud_json(api_url, None, page)
}

fn fetch_arrowcloud_user_context(
    api_key: &str,
) -> Result<Option<ArrowCloudUserContext>, network::NetworkError> {
    fetch_arrowcloud_json::<ArrowCloudUserApiResponse>(
        online::arrowcloud_user_url(),
        Some(api_key),
        None,
    )
    .map(|response| response.map(|response| arrowcloud_user_context_from_api(response.user)))
}

// --- ArrowCloud bulk score retrieval (POST /v1/retrieve-scores) ---

/// Maximum chart hashes per `/v1/retrieve-scores` request, server-enforced.
pub const ARROWCLOUD_BULK_MAX_HASHES: usize = 1000;

/// Global ArrowCloud leaderboard variants. Numeric values match the
/// `Leaderboard.id` rows seeded by ArrowCloud:
/// `2 = EX`, `3 = ITG / Money`, `4 = HardEX`. See
/// `arrow-cloud/api/src/utils/leaderboard/index.ts` (`GLOBAL_*_LEADERBOARD_ID`).
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

impl serde::Serialize for ArrowCloudLeaderboard {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.id())
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudRetrieveScoresRequest<'a> {
    chart_hashes: &'a [String],
    leaderboard_ids: &'a [ArrowCloudLeaderboard],
    #[serde(skip_serializing_if = "Option::is_none")]
    user_id: Option<&'a str>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudRetrieveScoresResponse {
    #[serde(default)]
    scores: HashMap<String, HashMap<String, ArrowCloudRetrieveScoreEntry>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudRetrieveScoreEntry {
    /// Score percent on a 0..100 scale (string in JSON, e.g. `"99.12"`).
    /// `None` means the server returned an entry without a score field; we
    /// treat such rows as "no score" rather than caching a fake 0.00%.
    #[serde(default, deserialize_with = "de_optional_f64_from_string_or_number")]
    score: Option<f64>,
    #[serde(default)]
    grade: Option<String>,
    /// ISO-8601 / RFC-3339 timestamp string, e.g. `"2026-05-03T19:10:17.504Z"`.
    #[serde(default)]
    date: Option<String>,
    #[serde(default, deserialize_with = "de_optional_i64_from_string_or_number")]
    play_id: Option<i64>,
    #[serde(default)]
    is_fail: bool,
}

/// Convert a single bulk-API entry into our internal `ArrowCloudScore`.
///
/// Returns `None` if the entry has no usable `score` field; otherwise
/// preserves AC-native fields (server grade enum, played-at timestamp, play
/// id) rather than collapsing into the GS-shaped `CachedScore`.
fn arrowcloud_score_from_entry(entry: &ArrowCloudRetrieveScoreEntry) -> Option<ArrowCloudScore> {
    let percent_0_100 = entry.score?.clamp(0.0, 100.0);
    let server_grade = entry
        .grade
        .as_deref()
        .and_then(ArrowCloudServerGrade::from_server_str);
    let played_at = entry
        .date
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    Some(ArrowCloudScore {
        score_percent: percent_0_100 / 100.0,
        server_grade,
        played_at,
        play_id: entry.play_id,
        is_fail: entry.is_fail,
    })
}

fn arrowcloud_scores_from_entry_map(
    leaderboards: &HashMap<String, ArrowCloudRetrieveScoreEntry>,
) -> ArrowCloudScores {
    let mut out = ArrowCloudScores::default();
    for (lb_id_raw, entry) in leaderboards {
        let Ok(lb_id) = lb_id_raw.parse::<u32>() else {
            continue;
        };
        let Some(variant) = ArrowCloudLeaderboard::from_id(lb_id) else {
            continue; // Ignore event/non-global leaderboards.
        };
        let Some(score) = arrowcloud_score_from_entry(entry) else {
            continue; // Ignore malformed entries with no score field.
        };
        match variant {
            ArrowCloudLeaderboard::Itg => out.itg = Some(score),
            ArrowCloudLeaderboard::Ex => out.ex = Some(score),
            ArrowCloudLeaderboard::HardEx => out.hard_ex = Some(score),
        }
    }
    out
}

/// POST `chart_hashes` to `/v1/retrieve-scores` and return parsed results.
///
/// `user_id` may be `None` to let the server resolve the bearer-token user.
/// `chart_hashes` MUST contain ≤ `ARROWCLOUD_BULK_MAX_HASHES` entries.
fn fetch_arrowcloud_bulk_scores(
    api_key: &str,
    user_id: Option<&str>,
    chart_hashes: &[String],
    leaderboards: &[ArrowCloudLeaderboard],
) -> Result<HashMap<String, ArrowCloudScores>, Box<dyn Error + Send + Sync>> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("ArrowCloud API key is missing.".into());
    }
    if chart_hashes.is_empty() {
        return Ok(HashMap::new());
    }
    if chart_hashes.len() > ARROWCLOUD_BULK_MAX_HASHES {
        return Err(format!(
            "ArrowCloud bulk request exceeds {ARROWCLOUD_BULK_MAX_HASHES} chart hashes."
        )
        .into());
    }

    let body = ArrowCloudRetrieveScoresRequest {
        chart_hashes,
        leaderboard_ids: leaderboards,
        user_id,
    };
    let bearer = format!("Bearer {api_key}");
    let url = online::arrowcloud_retrieve_scores_url();
    let response = network::get_agent()
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", &bearer)
        .send_json(&body)?;

    if response.status() != 200 {
        return Err(format!(
            "ArrowCloud /v1/retrieve-scores returned status {}",
            response.status()
        )
        .into());
    }

    let decoded: ArrowCloudRetrieveScoresResponse = response.into_body().read_json()?;
    let mut out = HashMap::with_capacity(decoded.scores.len());
    for (chart_hash, leaderboards) in decoded.scores {
        let entries = arrowcloud_scores_from_entry_map(&leaderboards);
        if entries.itg.is_some() || entries.ex.is_some() || entries.hard_ex.is_some() {
            out.insert(chart_hash, entries);
        }
    }
    Ok(out)
}

fn fetch_arrowcloud_panes(
    chart_hash: &str,
    api_key: &str,
    _max_entries: usize,
) -> Result<Vec<LeaderboardPane>, Box<dyn Error + Send + Sync>> {
    let chart_hash = chart_hash.trim();
    let api_key = api_key.trim();
    if chart_hash.is_empty() || api_key.is_empty() {
        return Ok(Vec::new());
    }

    let user_context = match fetch_arrowcloud_user_context(api_key) {
        Ok(context) => context,
        Err(error) => {
            debug!(
                "ArrowCloud user context request failed for chart {}; using public H.EX board only: {}",
                chart_hash, error
            );
            None
        }
    };

    let Some(legacy_api_url) = online::arrowcloud_legacy_leaderboards_url(chart_hash) else {
        return Ok(vec![empty_arrowcloud_hard_ex_pane()]);
    };
    let Some(decoded) = fetch_arrowcloud_leaderboards(legacy_api_url.as_str(), Some(1))? else {
        return Ok(vec![empty_arrowcloud_hard_ex_pane()]);
    };
    let Some(first_page) = arrowcloud_hard_ex_pane_from_response(decoded) else {
        return Ok(vec![empty_arrowcloud_hard_ex_pane()]);
    };

    let mut extra_pages = Vec::new();
    let mut remaining = arrowcloud_target_user_ids(user_context.as_ref());
    arrowcloud_update_remaining_targets(
        first_page.scores.as_slice(),
        user_context.as_ref(),
        &mut remaining,
    );
    let total_pages = first_page
        .total_pages
        .max(first_page.page.max(1) + u32::from(first_page.has_next))
        .max(1);
    let mut page = first_page.page.max(1).saturating_add(1);

    while !remaining.is_empty() && page <= total_pages {
        match fetch_arrowcloud_leaderboards(legacy_api_url.as_str(), Some(page)) {
            Ok(Some(decoded)) => {
                let Some(hard_ex_page) = arrowcloud_hard_ex_pane_from_response(decoded) else {
                    page += 1;
                    continue;
                };
                arrowcloud_update_remaining_targets(
                    hard_ex_page.scores.as_slice(),
                    user_context.as_ref(),
                    &mut remaining,
                );
                extra_pages.push(hard_ex_page);
            }
            Ok(None) => break,
            Err(error) => {
                warn!(
                    "ArrowCloud H.EX personalized scan stopped on chart {} page {}: {}",
                    chart_hash, page, error
                );
                break;
            }
        }
        page += 1;
    }

    Ok(vec![arrowcloud_hard_ex_pane_from_pages(
        first_page,
        extra_pages,
        user_context.as_ref(),
    )])
}

fn fetch_player_leaderboards_internal(
    chart_hash: &str,
    api_key: &str,
    username: &str,
    arrowcloud_api_key: Option<&str>,
    show_ex_score: bool,
    max_entries: usize,
) -> Result<FetchedPlayerLeaderboards, Box<dyn Error + Send + Sync>> {
    if chart_hash.trim().is_empty() {
        return Err("Missing chart hash for leaderboard request.".into());
    }
    if api_key.trim().is_empty() {
        return Err("Missing GrooveStats API key for leaderboard request.".into());
    }

    let max_entries = max_entries.max(1);
    let max_entries_str = max_entries.to_string();
    let agent = network::get_groovestats_agent();
    let api_url = online::groovestats_player_leaderboards_url();
    let response = agent
        .get(&api_url)
        .header("x-api-key-player-1", api_key)
        .query("chartHashP1", chart_hash)
        .query("maxLeaderboardResults", &max_entries_str)
        .call()?;

    if response.status() != 200 {
        return Err(format!("Leaderboard API returned status {}", response.status()).into());
    }

    let decoded: LeaderboardsApiResponse = response.into_body().read_json()?;
    let mut panes = Vec::with_capacity(5);
    let mut gs_entries = Vec::new();
    let mut ex_entries = Vec::new();
    let mut itl_self_score = None;
    let mut itl_self_rank = None;
    let mut itl_self_found = false;
    if let Some(player) = decoded.player1 {
        let LeaderboardApiPlayer {
            is_ranked: _is_ranked,
            gs_leaderboard,
            ex_leaderboard,
            rpg,
            itl,
        } = player;

        gs_entries.clone_from(&gs_leaderboard);
        ex_entries.clone_from(&ex_leaderboard);
        if show_ex_score {
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
        } else {
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
        }

        if let Some(rpg) = rpg
            && !rpg.rpg_leaderboard.is_empty()
        {
            let name = if rpg.name.trim().is_empty() {
                "RPG"
            } else {
                rpg.name.as_str()
            };
            push_leaderboard_pane(&mut panes, name, rpg.rpg_leaderboard, false);
        }
        if let Some(itl) = itl
            && !itl.itl_leaderboard.is_empty()
        {
            itl_self_found = leaderboard_self_entry(&itl.itl_leaderboard, username).is_some();
            itl_self_score = leaderboard_self_score_10000(&itl.itl_leaderboard, username);
            itl_self_rank = leaderboard_self_rank(&itl.itl_leaderboard, username);
            let name = if itl.name.trim().is_empty() {
                "ITL"
            } else {
                itl.name.as_str()
            };
            push_leaderboard_pane(&mut panes, name, itl.itl_leaderboard, true);
        }
    }

    if let Some(arrowcloud_api_key) = arrowcloud_api_key {
        match fetch_arrowcloud_panes(chart_hash, arrowcloud_api_key, max_entries) {
            Ok(arrowcloud_panes) => {
                let insert_ix = 2.min(panes.len());
                for pane in arrowcloud_panes.into_iter().rev() {
                    panes.insert(insert_ix, pane);
                }
            }
            Err(error) => warn!(
                "ArrowCloud leaderboard fetch failed for chart {}: {}",
                chart_hash, error
            ),
        }
    }

    Ok(FetchedPlayerLeaderboards {
        data: PlayerLeaderboardData {
            panes,
            itl_self_score,
            itl_self_rank,
        },
        gs_entries,
        ex_entries,
        itl_self_found,
    })
}

#[inline(always)]
const fn loading_player_leaderboard_snapshot() -> CachedPlayerLeaderboardData {
    CachedPlayerLeaderboardData {
        loading: true,
        data: None,
        error: None,
    }
}

#[inline(always)]
fn cache_snapshot_from_entry(entry: &PlayerLeaderboardCacheEntry) -> CachedPlayerLeaderboardData {
    match &entry.value {
        PlayerLeaderboardCacheValue::Ready(data) => CachedPlayerLeaderboardData {
            loading: false,
            data: Some(data.clone()),
            error: None,
        },
        PlayerLeaderboardCacheValue::Error(error) => CachedPlayerLeaderboardData {
            loading: false,
            data: None,
            error: Some(error.clone()),
        },
    }
}

#[inline(always)]
fn should_fetch_player_leaderboard_entry(
    entry: Option<&PlayerLeaderboardCacheEntry>,
    max_entries: usize,
    refresh_cached: bool,
) -> bool {
    let Some(entry) = entry else {
        return true;
    };
    let now = Instant::now();
    match entry.value {
        PlayerLeaderboardCacheValue::Ready(_) => {
            if refresh_cached {
                return entry
                    .retry_after
                    .is_none_or(|retry_after| now >= retry_after);
            }
            entry.max_entries < max_entries
                && entry
                    .retry_after
                    .is_none_or(|retry_after| now >= retry_after)
        }
        PlayerLeaderboardCacheValue::Error(_) => entry
            .retry_after
            .is_none_or(|retry_after| now >= retry_after),
    }
}

#[inline(always)]
fn should_rerun_in_flight_player_leaderboard_fetch(
    in_flight_max_entries: usize,
    requested_max_entries: usize,
    refresh_cached: bool,
) -> bool {
    refresh_cached || requested_max_entries > in_flight_max_entries
}

#[inline(always)]
fn queue_player_leaderboard_refresh(
    pending_refresh: &mut HashMap<PlayerLeaderboardCacheKey, usize>,
    key: &PlayerLeaderboardCacheKey,
    requested_max_entries: usize,
) {
    pending_refresh
        .entry(key.clone())
        .and_modify(|max_entries| *max_entries = (*max_entries).max(requested_max_entries))
        .or_insert(requested_max_entries);
}

fn spawn_player_leaderboard_fetch(
    key: PlayerLeaderboardCacheKey,
    gs_username: String,
    persistent_profile_id: Option<String>,
    auto_profile_id: Option<String>,
    should_auto_populate: bool,
    requested_max_entries: usize,
) {
    std::thread::spawn(move || {
        let request_started_at = Instant::now();
        let fetched = fetch_player_leaderboards_internal(
            &key.chart_hash,
            &key.api_key,
            gs_username.as_str(),
            if key.include_arrowcloud {
                Some(key.arrowcloud_api_key.as_str())
            } else {
                None
            },
            key.show_ex_score,
            requested_max_entries,
        );
        let refresh_finished_at = Instant::now();
        let mut queued_refresh = None;
        let mut queued_key = None;
        let mut queued_gs_username = None;
        let mut queued_persistent_profile_id = None;
        let mut queued_auto_profile_id = None;

        {
            let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
            cache.in_flight.remove(&key);
            let request_invalidated = player_leaderboard_request_was_invalidated(
                cache.invalidated_after.get(&key).copied(),
                request_started_at,
            );

            if !request_invalidated {
                match fetched {
                    Ok(fetched) => {
                        if !should_keep_newer_player_leaderboard_entry(
                            cache.by_key.get(&key),
                            request_started_at,
                        ) {
                            let FetchedPlayerLeaderboards {
                                data,
                                gs_entries,
                                ex_entries,
                                itl_self_found,
                            } = fetched;
                            if itl_self_found {
                                itl::set_cached_online_self_score(
                                    persistent_profile_id.as_deref(),
                                    key.api_key.as_str(),
                                    key.chart_hash.as_str(),
                                    data.itl_self_score,
                                );
                                itl::set_cached_online_self_rank(
                                    persistent_profile_id.as_deref(),
                                    key.api_key.as_str(),
                                    key.chart_hash.as_str(),
                                    data.itl_self_rank,
                                );
                            }
                            if should_auto_populate
                                && let Some(profile_id) = auto_profile_id.as_deref()
                            {
                                cache_gs_score_from_leaderboard(
                                    profile_id,
                                    gs_username.as_str(),
                                    key.chart_hash.as_str(),
                                    gs_entries.as_slice(),
                                    ex_entries.as_slice(),
                                );
                            }
                            cache.by_key.insert(
                                key.clone(),
                                PlayerLeaderboardCacheEntry {
                                    value: PlayerLeaderboardCacheValue::Ready(data),
                                    max_entries: requested_max_entries,
                                    refreshed_at: refresh_finished_at,
                                    retry_after: None,
                                },
                            );
                            cache.invalidated_after.remove(&key);
                        }
                    }
                    Err(error) => {
                        if !should_keep_newer_player_leaderboard_entry(
                            cache.by_key.get(&key),
                            request_started_at,
                        ) {
                            if let Some(entry) = cache.by_key.get_mut(&key)
                                && matches!(entry.value, PlayerLeaderboardCacheValue::Ready(_))
                            {
                                // Keep stale data visible on refresh failures, but back off retries.
                                entry.refreshed_at = refresh_finished_at;
                                entry.retry_after = Some(
                                    refresh_finished_at + PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
                                );
                            } else {
                                cache.by_key.insert(
                                    key.clone(),
                                    PlayerLeaderboardCacheEntry {
                                        value: PlayerLeaderboardCacheValue::Error(
                                            error.to_string(),
                                        ),
                                        max_entries: requested_max_entries,
                                        refreshed_at: refresh_finished_at,
                                        retry_after: Some(
                                            refresh_finished_at
                                                + PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
                                        ),
                                    },
                                );
                            }
                            cache.invalidated_after.remove(&key);
                        }
                    }
                }
            }

            if let Some(next_max_entries) = cache.pending_refresh.remove(&key) {
                cache.in_flight.insert(key.clone(), next_max_entries);
                queued_refresh = Some(next_max_entries);
                queued_key = Some(key.clone());
                queued_gs_username = Some(gs_username.clone());
                queued_persistent_profile_id = Some(persistent_profile_id.clone());
                queued_auto_profile_id = Some(auto_profile_id.clone());
            }
        }

        if let (
            Some(next_max_entries),
            Some(next_key),
            Some(next_gs_username),
            Some(next_persistent_profile_id),
            Some(next_auto_profile_id),
        ) = (
            queued_refresh,
            queued_key,
            queued_gs_username,
            queued_persistent_profile_id,
            queued_auto_profile_id,
        ) {
            spawn_player_leaderboard_fetch(
                next_key,
                next_gs_username,
                next_persistent_profile_id,
                next_auto_profile_id,
                should_auto_populate,
                next_max_entries,
            );
        }
    });
}

#[inline(always)]
fn player_leaderboard_profile_snapshot_for_side(
    side: profile::PlayerSide,
) -> GameplayScoreboxProfileSnapshot {
    let side_profile = profile::get_for_side(side);
    GameplayScoreboxProfileSnapshot::from_profile(
        &side_profile,
        profile::is_session_side_joined(side),
        profile::active_local_profile_id_for_side(side),
    )
}

#[inline(always)]
fn player_leaderboard_cache_key_for_profile(
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

fn get_cached_player_leaderboard_itl_self_rank_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<u32> {
    let profile_snapshot = player_leaderboard_profile_snapshot_for_side(side);
    let key = player_leaderboard_cache_key_for_profile(chart_hash, &profile_snapshot)?;
    let cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
    let entry = cache.by_key.get(&key)?;
    let PlayerLeaderboardCacheValue::Ready(data) = &entry.value else {
        return None;
    };
    data.itl_self_rank
}

fn get_or_fetch_player_leaderboards_for_profile_inner(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
    max_entries: usize,
    refresh_cached: bool,
) -> Option<CachedPlayerLeaderboardData> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() || max_entries == 0 {
        return None;
    }
    let key = player_leaderboard_cache_key_for_profile(chart_hash, profile_snapshot)?;
    let gs_username = profile_snapshot.gs_username.clone();
    let persistent_profile_id = profile_snapshot.persistent_profile_id.clone();
    let auto_profile_id = profile_snapshot.auto_profile_id.clone();
    let should_auto_populate = profile_snapshot.should_auto_populate;

    let mut should_spawn = false;
    let mut requested_max_entries = max_entries;
    let snapshot = {
        let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
        let entry = cache.by_key.get(&key);
        if let Some(entry) = entry {
            requested_max_entries = requested_max_entries.max(entry.max_entries);
        }
        let snapshot = if let Some(entry) = entry {
            cache_snapshot_from_entry(entry)
        } else {
            loading_player_leaderboard_snapshot()
        };

        if should_fetch_player_leaderboard_entry(entry, requested_max_entries, refresh_cached) {
            if let Some(in_flight_max_entries) = cache.in_flight.get(&key).copied() {
                if should_rerun_in_flight_player_leaderboard_fetch(
                    in_flight_max_entries,
                    requested_max_entries,
                    refresh_cached,
                ) {
                    queue_player_leaderboard_refresh(
                        &mut cache.pending_refresh,
                        &key,
                        requested_max_entries,
                    );
                }
            } else {
                cache.in_flight.insert(key.clone(), requested_max_entries);
                should_spawn = true;
            }
        }
        snapshot
    };

    if should_spawn {
        spawn_player_leaderboard_fetch(
            key,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
            requested_max_entries,
        );
    }

    Some(snapshot)
}

pub fn get_or_fetch_player_leaderboards_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    let profile_snapshot = player_leaderboard_profile_snapshot_for_side(side);
    get_or_fetch_player_leaderboards_for_profile_inner(
        chart_hash,
        &profile_snapshot,
        max_entries,
        false,
    )
}

pub fn get_or_fetch_player_leaderboards_for_profile(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    get_or_fetch_player_leaderboards_for_profile_inner(
        chart_hash,
        profile_snapshot,
        max_entries,
        false,
    )
}

pub fn refresh_player_leaderboards_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    let profile_snapshot = player_leaderboard_profile_snapshot_for_side(side);
    get_or_fetch_player_leaderboards_for_profile_inner(
        chart_hash,
        &profile_snapshot,
        max_entries,
        true,
    )
}

pub fn invalidate_player_leaderboards_for_side(chart_hash: &str, side: profile::PlayerSide) {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() {
        return;
    }
    let side_profile = profile::get_for_side(side);
    let gs_api_key = side_profile.groovestats_api_key.trim();
    if gs_api_key.is_empty() {
        return;
    }

    itl::set_cached_online_self_score(
        profile::active_local_profile_id_for_side(side).as_deref(),
        gs_api_key,
        chart_hash,
        None,
    );
    itl::set_cached_online_self_rank(
        profile::active_local_profile_id_for_side(side).as_deref(),
        gs_api_key,
        chart_hash,
        None,
    );

    let invalidated_at = Instant::now();
    let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
    let matching_keys: HashSet<PlayerLeaderboardCacheKey> = cache
        .by_key
        .keys()
        .chain(cache.in_flight.keys())
        .chain(cache.pending_refresh.keys())
        .chain(cache.invalidated_after.keys())
        .filter(|key| key.api_key == gs_api_key && key.chart_hash.eq_ignore_ascii_case(chart_hash))
        .cloned()
        .collect();
    for key in matching_keys {
        cache.by_key.remove(&key);
        cache.in_flight.remove(&key);
        cache.pending_refresh.remove(&key);
        cache.invalidated_after.insert(key, invalidated_at);
    }
}
#[derive(Debug)]
struct MachineLeaderboardPlay {
    name: String,
    machine_tag: Option<String>,
    score_percent: f64,
    played_at_ms: i64,
    is_fail: bool,
}

#[derive(Debug)]
struct MachineReplayPlay {
    initials: String,
    score_percent: f64,
    played_at_ms: i64,
    is_fail: bool,
    replay_beat0_time_ns: gameplay::SongTimeNs,
    replay: Vec<LocalReplayEdgeV1>,
}

fn push_machine_leaderboard_from_dir(
    dir: &Path,
    chart_hash: &str,
    name: &str,
    machine_tag: Option<&str>,
    out: &mut Vec<MachineLeaderboardPlay>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((file_hash, played_at_ms)) = parse_local_score_filename(file_name) else {
            continue;
        };
        if file_hash != chart_hash {
            continue;
        }
        let Some(h) = read_local_score_header(&path) else {
            continue;
        };
        out.push(MachineLeaderboardPlay {
            name: name.to_string(),
            machine_tag: machine_tag.map(str::to_string),
            score_percent: h.score_percent,
            played_at_ms,
            is_fail: grade_from_code(h.grade_code) == Grade::Failed || h.fail_time.is_some(),
        });
    }
}

fn push_machine_replays_from_dir(
    dir: &Path,
    chart_hash: &str,
    initials: &str,
    out: &mut Vec<MachineReplayPlay>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((file_hash, played_at_ms)) = parse_local_score_filename(name) else {
            continue;
        };
        if file_hash != chart_hash {
            continue;
        }
        let Some(full) = read_local_score_entry(&path) else {
            continue;
        };
        out.push(MachineReplayPlay {
            initials: initials.to_string(),
            score_percent: full.score_percent,
            played_at_ms,
            is_fail: grade_from_code(full.grade_code) == Grade::Failed || full.fail_time.is_some(),
            replay_beat0_time_ns: full.beat0_time_ns,
            replay: full.replay,
        });
    }
}

fn local_score_date_string(played_at_ms: i64) -> String {
    let Some(dt) = Local.timestamp_millis_opt(played_at_ms).single() else {
        return String::new();
    };
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn machine_leaderboard_entries(
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

pub fn get_machine_leaderboard_local(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays: Vec<MachineLeaderboardPlay> = Vec::new();
    for profile_meta in profile::scan_local_profiles() {
        let initials =
            profile_initials_for_id(&profile_meta.id).unwrap_or_else(|| "----".to_string());
        let root = local_scores_root_for_profile(&profile_meta.id);
        push_machine_leaderboard_from_dir(&root, chart_hash, &initials, None, &mut plays);
        let shard_dir = root.join(shard2_for_hash(chart_hash));
        push_machine_leaderboard_from_dir(&shard_dir, chart_hash, &initials, None, &mut plays);
    }

    machine_leaderboard_entries(plays, max_entries)
}

pub fn get_machine_leaderboard_local_with_names(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays: Vec<MachineLeaderboardPlay> = Vec::new();
    for profile_meta in profile::scan_local_profiles() {
        let initials =
            profile_initials_for_id(&profile_meta.id).unwrap_or_else(|| "----".to_string());
        let root = local_scores_root_for_profile(&profile_meta.id);
        push_machine_leaderboard_from_dir(
            &root,
            chart_hash,
            profile_meta.display_name.as_str(),
            Some(initials.as_str()),
            &mut plays,
        );
        let shard_dir = root.join(shard2_for_hash(chart_hash));
        push_machine_leaderboard_from_dir(
            &shard_dir,
            chart_hash,
            profile_meta.display_name.as_str(),
            Some(initials.as_str()),
            &mut plays,
        );
    }

    machine_leaderboard_entries(plays, max_entries)
}

pub fn get_personal_leaderboard_local_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return Vec::new();
    };

    let initials = profile_initials_for_id(&profile_id).unwrap_or_else(|| "----".to_string());
    let root = local_scores_root_for_profile(&profile_id);
    let shard_dir = root.join(shard2_for_hash(chart_hash));

    let mut plays: Vec<MachineLeaderboardPlay> = Vec::new();
    push_machine_leaderboard_from_dir(&root, chart_hash, &initials, None, &mut plays);
    push_machine_leaderboard_from_dir(&shard_dir, chart_hash, &initials, None, &mut plays);

    machine_leaderboard_entries(plays, max_entries)
}

pub fn get_machine_replays_local(chart_hash: &str, max_entries: usize) -> Vec<MachineReplayEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays: Vec<MachineReplayPlay> = Vec::new();
    for profile_meta in profile::scan_local_profiles() {
        let initials =
            profile_initials_for_id(&profile_meta.id).unwrap_or_else(|| "----".to_string());
        let root = local_scores_root_for_profile(&profile_meta.id);
        push_machine_replays_from_dir(&root, chart_hash, &initials, &mut plays);
        let shard_dir = root.join(shard2_for_hash(chart_hash));
        push_machine_replays_from_dir(&shard_dir, chart_hash, &initials, &mut plays);
    }

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
            if gameplay::song_time_ns_invalid(edge.event_music_time_ns) {
                continue;
            }
            let source = if edge.source == 1 {
                InputSource::Gamepad
            } else {
                InputSource::Keyboard
            };
            replay.push(ReplayEdge {
                event_music_time_ns: edge.event_music_time_ns,
                lane_index: edge.lane,
                pressed: edge.pressed,
                source,
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

// --- ITG PercentScore weights (mirror Simply Love SL_Init.lua, ITG mode) ---
const DP_W1: i32 = 5;
const DP_W2: i32 = 4;
const DP_W3: i32 = 2;
const DP_W4: i32 = 0;
const DP_W5: i32 = -6;
const DP_MISS: i32 = -12;
const DP_HELD: i32 = 5;

#[derive(Debug, Default, Clone, Copy)]
struct ParsedCommentCounts {
    w: u32,
    e: u32,
    g: u32,
    d: u32,
    wo: u32,
    m: u32,
}

fn parse_comment_counts(comment: &str) -> ParsedCommentCounts {
    let mut counts = ParsedCommentCounts::default();
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

fn parse_comment_ex_percent(comment: &str) -> Option<f64> {
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
fn ex_scoreboard_is_quint(score_10000: f64) -> bool {
    score_10000.is_finite() && score_10000.round().clamp(0.0, 10000.0) >= 10000.0
}

#[derive(Debug, Clone, Copy, Default)]
struct GsExEvidence {
    leaderboard_score_10000: Option<f64>,
    comment_percent: Option<f64>,
}

impl GsExEvidence {
    fn from_sources(leaderboard_score_10000: Option<f64>, comment: Option<&str>) -> Self {
        Self {
            leaderboard_score_10000,
            comment_percent: comment.and_then(parse_comment_ex_percent),
        }
    }

    #[inline(always)]
    fn is_quint(self) -> Option<bool> {
        if let Some(score) = self.leaderboard_score_10000 {
            return Some(ex_scoreboard_is_quint(score));
        }
        self.comment_percent.map(|ex| ex >= 100.0)
    }

    #[inline(always)]
    fn proves_nonquint(self) -> bool {
        self.is_quint() == Some(false)
    }
}

fn find_chart_stats_for_hash(chart_hash: &str) -> Option<rssp::stats::ArrowStats> {
    let cache = get_song_cache();
    for pack in cache.iter() {
        for song in &pack.songs {
            for chart in &song.charts {
                if chart.short_hash == chart_hash {
                    return Some(chart.stats.clone());
                }
            }
        }
    }
    None
}

fn compute_lamp_index(
    score: f64,
    comment: Option<&str>,
    chart_hash: &str,
    ex_evidence: GsExEvidence,
) -> Option<u8> {
    let score_percent = score / 10000.0;

    // Perfect 100% ITG can still be a quad. Trust the GS/BS EX scoreboard
    // first; only fall back to the EX comment when that board is unavailable.
    if (score_percent - 1.0).abs() <= 1e-9 {
        if let Some(is_quint) = ex_evidence.is_quint() {
            if is_quint {
                debug!(
                    "GrooveStats lamp: hash={} score={:.4}% -> Quint lamp (EX=100, index=0)",
                    chart_hash,
                    score_percent * 100.0
                );
                return Some(0);
            }
            debug!(
                "GrooveStats lamp: hash={} score={:.4}% -> Quad lamp (EX < 100, index=1)",
                chart_hash,
                score_percent * 100.0
            );
            return Some(1);
        }
        if let Some(comment) = comment {
            let counts = parse_comment_counts(comment);
            let explicit_nonquint_counts = counts.w != 0
                || counts.e != 0
                || counts.g != 0
                || counts.d != 0
                || counts.wo != 0
                || counts.m != 0;
            if explicit_nonquint_counts {
                debug!(
                    "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> Quad lamp (W1 FC, index=1)",
                    chart_hash,
                    score_percent * 100.0,
                    comment
                );
                return Some(1);
            }
        }
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% -> Quad lamp (no EX=100 evidence)",
            chart_hash,
            score_percent * 100.0
        );
        return Some(1);
    }

    let comment = if let Some(c) = comment {
        c
    } else {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% -> no lamp (no GrooveStats comment available)",
            chart_hash,
            score_percent * 100.0
        );
        return None;
    };
    let counts = parse_comment_counts(comment);

    // Any explicit Miss or Way Off disqualifies lamps immediately.
    if counts.m > 0 || counts.wo > 0 {
        return None;
    }

    let stats = if let Some(s) = find_chart_stats_for_hash(chart_hash) {
        s
    } else {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> no lamp (chart stats not found for hash)",
            chart_hash,
            score_percent * 100.0,
            comment
        );
        return None;
    };
    let taps_rows = stats.total_steps as i32;
    let holds = stats.holds as i32;
    let rolls = stats.rolls as i32;

    if taps_rows <= 0 {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> no lamp (taps_rows <= 0, taps_rows={})",
            chart_hash,
            score_percent * 100.0,
            comment,
            taps_rows
        );
        return None;
    }

    // Reconstruct W1 count as "everything not explicitly listed".
    let non_w1_from_suffixes = counts.e + counts.g + counts.d + counts.wo + counts.m + counts.w;
    let inferred_w1 = if (non_w1_from_suffixes as i32) > taps_rows {
        0
    } else {
        (taps_rows as u32).saturating_sub(counts.e + counts.g + counts.d + counts.wo + counts.m)
    };
    let w1_total = counts.w.max(inferred_w1);

    // Dance Points from tap judgments (rows) only, per ITG PercentScoreWeight.
    let dp_taps: i32 = (w1_total as i32) * DP_W1
        + (counts.e as i32) * DP_W2
        + (counts.g as i32) * DP_W3
        + (counts.d as i32) * DP_W4
        + (counts.wo as i32) * DP_W5
        + (counts.m as i32) * DP_MISS;

    // Holds + rolls assumed fully held for the "no hidden errors" hypothesis.
    let dp_hold_roll: i32 = (holds + rolls) * DP_HELD;

    // Maximum possible DP if every tap was W1 and all holds/rolls fully held.
    let dp_possible_max: i32 = (taps_rows * DP_W1 + dp_hold_roll).max(1);
    let dp_expect_no_hidden_errors: i32 = dp_taps + dp_hold_roll;

    let dp_expect_frac = f64::from(dp_expect_no_hidden_errors) / f64::from(dp_possible_max);
    let dp_diff = (score_percent - dp_expect_frac).abs();
    let dp_consistent = dp_diff <= 0.0005;

    if !dp_consistent {
        // There must have been extra DP loss (e.g., dropped holds or hit mines).
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP mismatch: score%={:.5} vs no-hidden-errors%={:.5} (Δ={:.6}); \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> no lamp",
            chart_hash,
            score_percent * 100.0,
            comment,
            score_percent * 100.0,
            dp_expect_frac * 100.0,
            dp_diff * 100.0,
            taps_rows,
            holds,
            rolls,
            counts.w,
            counts.e,
            counts.g,
            counts.d,
            counts.wo,
            counts.m
        );
        return None;
    }

    // At this point, we know there were no hidden hold/mine mistakes.
    // Classify the lamp tier, mirroring Simply Love's StageAward semantics.
    if counts.g == 0 && counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // Only W1/W2 present (and W1 reconstructed) => W2 full combo (FEC).
        if counts.e > 0 || w1_total > 0 {
            debug!(
                "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=FEC (index=2)",
                chart_hash,
                score_percent * 100.0,
                comment,
                taps_rows,
                holds,
                rolls,
                w1_total,
                counts.e,
                counts.g,
                counts.d,
                counts.wo,
                counts.m
            );
            return Some(2);
        }
    }

    if counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // At least one Great, but no Decents/WayOff/Miss => W3 full combo.
        if counts.g > 0 {
            debug!(
                "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=W3 FC (index=3)",
                chart_hash,
                score_percent * 100.0,
                comment,
                taps_rows,
                holds,
                rolls,
                w1_total,
                counts.e,
                counts.g,
                counts.d,
                counts.wo,
                counts.m
            );
            return Some(3);
        }
    }

    // No WayOff/Miss and DP-consistent => at worst a W4 full combo.
    if counts.wo == 0 && counts.m == 0 {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=W4 FC (index=4)",
            chart_hash,
            score_percent * 100.0,
            comment,
            taps_rows,
            holds,
            rolls,
            w1_total,
            counts.e,
            counts.g,
            counts.d,
            counts.wo,
            counts.m
        );
        return Some(4);
    }

    None
}

fn compute_lamp_judge_count(lamp_index: Option<u8>, comment: Option<&str>) -> Option<u8> {
    let lamp_index = lamp_index?;
    let comment = comment?;
    let counts = parse_comment_counts(comment);

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

// --- Grade Calculation ---

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
    }
    // Note: We don't have enough info to detect Quints (W0) yet.
    else if percent >= 0.99 {
        Grade::Tier02
    }
    // three-stars
    else if percent >= 0.98 {
        Grade::Tier03
    }
    // two-stars
    else if percent >= 0.96 {
        Grade::Tier04
    }
    // one-star
    else if percent >= 0.94 {
        Grade::Tier05
    }
    // s-plus
    else if percent >= 0.92 {
        Grade::Tier06
    }
    // s
    else if percent >= 0.89 {
        Grade::Tier07
    }
    // s-minus
    else if percent >= 0.86 {
        Grade::Tier08
    }
    // a-plus
    else if percent >= 0.83 {
        Grade::Tier09
    }
    // a
    else if percent >= 0.80 {
        Grade::Tier10
    }
    // a-minus
    else if percent >= 0.76 {
        Grade::Tier11
    }
    // b-plus
    else if percent >= 0.72 {
        Grade::Tier12
    }
    // b
    else if percent >= 0.68 {
        Grade::Tier13
    }
    // b-minus
    else if percent >= 0.64 {
        Grade::Tier14
    }
    // c-plus
    else if percent >= 0.60 {
        Grade::Tier15
    }
    // c
    else if percent >= 0.55 {
        Grade::Tier16
    }
    // c-minus
    else {
        Grade::Tier17
    } // d
    // Grade::Failed is not score-based; it's determined by gameplay failure (e.g., lifebar empty),
    // which is not yet implemented. This function will never return Grade::Failed.
}

// --- Public Fetch Function ---

const SCORE_IMPORT_RATE_LIMIT_PER_SECOND: u32 = 3;
const SCORE_IMPORT_REQUEST_INTERVAL: Duration = Duration::from_millis(334);
const SCORE_IMPORT_PROGRESS_LOG_EVERY: usize = 100;

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

    fn player_leaderboards_url(self) -> String {
        let base = match self {
            Self::GrooveStats => online::groovestats_primary_api_base_url(),
            Self::BoogieStats => online::boogiestats_api_base_url(),
            Self::ArrowCloud => online::arrowcloud_api_base_url(),
        };
        format!("{}/player-leaderboards.php", base.trim_end_matches('/'))
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

struct ScoreImportFetchResult {
    score: Option<CachedScore>,
    score_proves_nonquint_ex: bool,
    itl_self_score: Option<u32>,
    itl_self_rank: Option<u32>,
    itl_self_found: bool,
}

fn score_import_api_key_for_endpoint(endpoint: ScoreImportEndpoint, profile: &Profile) -> &str {
    match endpoint {
        ScoreImportEndpoint::GrooveStats | ScoreImportEndpoint::BoogieStats => {
            profile.groovestats_api_key.trim()
        }
        ScoreImportEndpoint::ArrowCloud => profile.arrowcloud_api_key.trim(),
    }
}

fn score_entry_matches_profile(
    entry: &LeaderboardApiEntry,
    endpoint: ScoreImportEndpoint,
    username: &str,
) -> bool {
    if entry.is_self {
        return true;
    }
    if endpoint.requires_username() {
        return entry.name.eq_ignore_ascii_case(username);
    }
    false
}

fn score_import_result_from_response(
    decoded: LeaderboardsApiResponse,
    endpoint: ScoreImportEndpoint,
    username: &str,
    chart_hash: &str,
) -> ScoreImportFetchResult {
    let mut result = ScoreImportFetchResult {
        score: None,
        score_proves_nonquint_ex: false,
        itl_self_score: None,
        itl_self_rank: None,
        itl_self_found: false,
    };
    let Some(player) = decoded.player1 else {
        return result;
    };

    if let Some(entry) = player
        .gs_leaderboard
        .iter()
        .find(|entry| score_entry_matches_profile(entry, endpoint, username))
    {
        let ex_score = player
            .ex_leaderboard
            .iter()
            .find(|entry| score_entry_matches_profile(entry, endpoint, username))
            .and_then(entry_score_10000);
        let ex_evidence = GsExEvidence::from_sources(ex_score, entry.comments.as_deref());
        result.score_proves_nonquint_ex = ex_evidence.proves_nonquint();
        result.score = Some(cached_score_from_gs(
            entry.score,
            entry.comments.as_deref(),
            chart_hash,
            entry.is_fail,
            ex_evidence,
        ));
    }

    if let Some(itl) = player.itl
        && !itl.itl_leaderboard.is_empty()
    {
        result.itl_self_found = leaderboard_self_entry(&itl.itl_leaderboard, username).is_some();
        result.itl_self_score = leaderboard_self_score_10000(&itl.itl_leaderboard, username);
        result.itl_self_rank = leaderboard_self_rank(&itl.itl_leaderboard, username);
    }

    result
}

fn fetch_player_score_from_endpoint(
    endpoint: ScoreImportEndpoint,
    profile: &Profile,
    chart_hash: &str,
) -> Result<ScoreImportFetchResult, Box<dyn Error + Send + Sync>> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() {
        return Err("Missing chart hash for score request.".into());
    }

    let api_key = score_import_api_key_for_endpoint(endpoint, profile);
    if api_key.is_empty() {
        return Err(format!(
            "{} API key is missing in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }
    let username = profile.groovestats_username.trim();
    if endpoint.requires_username() && username.is_empty() {
        return Err(format!(
            "{} username is missing in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }

    let agent = match endpoint {
        ScoreImportEndpoint::GrooveStats | ScoreImportEndpoint::BoogieStats => {
            network::get_groovestats_agent()
        }
        ScoreImportEndpoint::ArrowCloud => network::get_agent(),
    };
    let api_url = endpoint.player_leaderboards_url();
    let response = agent
        .get(&api_url)
        .header("x-api-key-player-1", api_key)
        .query("chartHashP1", chart_hash)
        .call()?;

    if response.status() != 200 {
        return Err(format!("API returned status {}", response.status()).into());
    }

    let decoded: LeaderboardsApiResponse = response.into_body().read_json()?;
    Ok(score_import_result_from_response(
        decoded, endpoint, username, chart_hash,
    ))
}

fn collect_chart_hashes_for_import(
    pack_groups_filter: &[String],
    profile_id: &str,
    only_missing_gs_scores: bool,
) -> Vec<(String, Vec<String>)> {
    let existing_scores = if only_missing_gs_scores {
        cached_gs_chart_hashes_for_profile(profile_id)
    } else {
        HashSet::new()
    };
    collect_chart_hashes_per_pack_for_import(pack_groups_filter, &existing_scores)
}

/// Per-pack chart-hash collection for score import.
///
/// Returns a vector of `(pack_display_name, chart_hashes)` pairs preserving
/// song-cache iteration order. Globally deduplicates chart hashes across packs
/// (first pack wins). Optionally filters by pack group / display name and
/// skips charts present in `existing_scores` (caller supplies the right cache
/// for the endpoint they're importing into).
fn collect_chart_hashes_per_pack_for_import(
    pack_groups_filter: &[String],
    existing_scores: &HashSet<String>,
) -> Vec<(String, Vec<String>)> {
    let filter_set: HashSet<String> = pack_groups_filter
        .iter()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .collect();

    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let song_cache = get_song_cache();
    for pack in song_cache.iter() {
        let group_name = pack.group_name.trim();
        let display_name = if pack.name.trim().is_empty() {
            group_name
        } else {
            pack.name.trim()
        };
        if !filter_set.is_empty() {
            let group_lc = group_name.to_ascii_lowercase();
            let display_lc = display_name.to_ascii_lowercase();
            if !filter_set.contains(&group_lc) && !filter_set.contains(&display_lc) {
                continue;
            }
        }

        let mut hashes = Vec::new();
        for song in &pack.songs {
            for chart in &song.charts {
                let chart_hash = chart.short_hash.trim();
                if chart_hash.is_empty() {
                    continue;
                }
                if existing_scores.contains(chart_hash) {
                    continue;
                }
                if seen.insert(chart_hash.to_string()) {
                    hashes.push(chart_hash.to_string());
                }
            }
        }
        if !hashes.is_empty() {
            out.push((display_name.to_string(), hashes));
        }
    }
    out
}

#[inline(always)]
fn wait_for_next_import_request(last_request_started_at: Option<Instant>) {
    let Some(last_started) = last_request_started_at else {
        return;
    };
    let elapsed = last_started.elapsed();
    if elapsed < SCORE_IMPORT_REQUEST_INTERVAL {
        std::thread::sleep(SCORE_IMPORT_REQUEST_INTERVAL - elapsed);
    }
}

/// AC-specific bulk import orchestrator. Sends one POST per pack to
/// `/v1/retrieve-scores`, splitting any pack with > [`ARROWCLOUD_BULK_MAX_HASHES`]
/// charts into chunks. Throttled to [`SCORE_IMPORT_RATE_LIMIT_PER_SECOND`]
/// requests per second to match the per-chart paths.
fn import_scores_for_profile_arrowcloud_bulk<F>(
    profile_id: String,
    profile: Profile,
    pack_groups: Vec<String>,
    only_missing_scores: bool,
    on_progress: F,
    should_cancel: impl Fn() -> bool,
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>>
where
    F: FnMut(ScoreImportProgress),
{
    let mut on_progress = on_progress;
    let api_key = profile.arrowcloud_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("ArrowCloud API key is not set in profile configuration.".into());
    }

    // Resolve our user id once up front. The bulk endpoint accepts a missing
    // userId (it'll resolve from the bearer token), but sending it explicitly
    // is preferred and matches the documented contract.
    let user_id = match fetch_arrowcloud_user_context(&api_key) {
        Ok(Some(ctx)) => ctx.self_user_id,
        Ok(None) => None,
        Err(e) => {
            warn!("Could not resolve ArrowCloud user id, sending without it: {e}");
            None
        }
    };

    let existing_scores = if only_missing_scores {
        cached_ac_chart_hashes_with_itg_for_profile(&profile_id)
    } else {
        HashSet::new()
    };
    let pack_chart_groups =
        collect_chart_hashes_per_pack_for_import(&pack_groups, &existing_scores);
    let requested_charts: usize = pack_chart_groups.iter().map(|(_, h)| h.len()).sum();
    let total_packs = pack_chart_groups.len();
    let filter_note = if only_missing_scores {
        " (missing only)"
    } else {
        ""
    };
    on_progress(ScoreImportProgress {
        processed_charts: 0,
        total_charts: requested_charts,
        imported_scores: 0,
        missing_scores: 0,
        failed_requests: 0,
        detail: format!(
            "Queued {requested_charts} chart hashes across {total_packs} pack(s) for ArrowCloud bulk import{filter_note}."
        ),
    });
    if requested_charts == 0 {
        return Ok(ScoreBulkImportSummary {
            requested_charts: 0,
            imported_scores: 0,
            missing_scores: 0,
            failed_requests: 0,
            rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
            elapsed_seconds: 0.0,
            canceled: false,
        });
    }

    let import_started = Instant::now();
    let mut last_request_started_at: Option<Instant> = None;
    let mut imported_scores = 0usize;
    let mut missing_scores = 0usize;
    let mut failed_requests = 0usize;
    let mut processed_charts = 0usize;
    let mut canceled = false;

    'packs: for (pack_idx, (pack_name, hashes)) in pack_chart_groups.into_iter().enumerate() {
        let pack_chart_count = hashes.len();
        let mut pack_hits = 0usize;
        let mut pack_misses = 0usize;
        let mut pack_failures = 0usize;

        for chunk in hashes.chunks(ARROWCLOUD_BULK_MAX_HASHES) {
            if should_cancel() {
                canceled = true;
                debug!(
                    "ArrowCloud bulk import canceled at pack {} ({}/{}).",
                    pack_name, pack_idx, total_packs
                );
                break 'packs;
            }
            wait_for_next_import_request(last_request_started_at);
            if should_cancel() {
                canceled = true;
                break 'packs;
            }
            last_request_started_at = Some(Instant::now());

            let chunk_vec = chunk.to_vec();
            let request_started = Instant::now();
            let detail = match fetch_arrowcloud_bulk_scores(
                &api_key,
                user_id.as_deref(),
                &chunk_vec,
                &ArrowCloudLeaderboard::ALL_GLOBAL,
            ) {
                Ok(scores_by_chart) => {
                    let request_elapsed = request_started.elapsed();
                    let hits = scores_by_chart.len();
                    let misses = chunk_vec.len().saturating_sub(hits);
                    if !scores_by_chart.is_empty() {
                        set_cached_ac_scores_for_profile_bulk(
                            &profile_id,
                            scores_by_chart.into_iter(),
                        );
                    }
                    imported_scores += hits;
                    missing_scores += misses;
                    pack_hits += hits;
                    pack_misses += misses;
                    debug!(
                        "ArrowCloud /v1/retrieve-scores pack={}/{} pack_name='{}' chunk={} took={:.0}ms hits={} misses={}",
                        pack_idx + 1,
                        total_packs,
                        pack_name,
                        chunk_vec.len(),
                        request_elapsed.as_secs_f32() * 1000.0,
                        hits,
                        misses,
                    );
                    format!(
                        "Pack {}/{}: {pack_name} -> {hits} hit, {misses} missing ({:.0}ms)",
                        pack_idx + 1,
                        total_packs,
                        request_elapsed.as_secs_f32() * 1000.0,
                    )
                }
                Err(e) => {
                    let request_elapsed = request_started.elapsed();
                    failed_requests += 1;
                    pack_failures += 1;
                    let msg = format!(
                        "Pack {}/{}: {pack_name} request failed ({} charts, {:.0}ms): {e}",
                        pack_idx + 1,
                        total_packs,
                        chunk_vec.len(),
                        request_elapsed.as_secs_f32() * 1000.0,
                    );
                    warn!("{msg}");
                    msg
                }
            };

            processed_charts += chunk_vec.len();
            on_progress(ScoreImportProgress {
                processed_charts,
                total_charts: requested_charts,
                imported_scores,
                missing_scores,
                failed_requests,
                detail: detail.clone(),
            });
            debug!("{detail}");
        }

        debug!(
            "Pack {}/{} complete: {pack_name} ({pack_chart_count} charts -> {pack_hits} hit, {pack_misses} missing{}).",
            pack_idx + 1,
            total_packs,
            if pack_failures > 0 {
                format!(", {pack_failures} failed")
            } else {
                String::new()
            },
        );
    }

    Ok(ScoreBulkImportSummary {
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
        elapsed_seconds: import_started.elapsed().as_secs_f32(),
        canceled,
    })
}

pub fn import_scores_for_profile<F>(
    endpoint: ScoreImportEndpoint,
    profile_id: String,
    profile: Profile,
    pack_groups: Vec<String>,
    only_missing_gs_scores: bool,
    on_progress: F,
    should_cancel: impl Fn() -> bool,
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>>
where
    F: FnMut(ScoreImportProgress),
{
    if endpoint == ScoreImportEndpoint::ArrowCloud {
        return import_scores_for_profile_arrowcloud_bulk(
            profile_id,
            profile,
            pack_groups,
            only_missing_gs_scores,
            on_progress,
            should_cancel,
        );
    }

    let mut on_progress = on_progress;
    let api_key = score_import_api_key_for_endpoint(endpoint, &profile);
    if api_key.is_empty() {
        return Err(format!(
            "{} API key is not set in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }
    if endpoint.requires_username() && profile.groovestats_username.trim().is_empty() {
        return Err(format!(
            "{} username is not set in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }

    let username = profile.groovestats_username.trim().to_string();
    let pack_chart_groups =
        collect_chart_hashes_for_import(&pack_groups, &profile_id, only_missing_gs_scores);
    let requested_charts: usize = pack_chart_groups.iter().map(|(_, h)| h.len()).sum();
    let total_packs = pack_chart_groups.len();
    let filter_note = if only_missing_gs_scores {
        " (missing only)"
    } else {
        ""
    };
    on_progress(ScoreImportProgress {
        processed_charts: 0,
        total_charts: requested_charts,
        imported_scores: 0,
        missing_scores: 0,
        failed_requests: 0,
        detail: format!(
            "Queued {requested_charts} chart hashes across {total_packs} pack(s) for {} import{filter_note}.",
            endpoint.display_name(),
        ),
    });
    if requested_charts == 0 {
        return Ok(ScoreBulkImportSummary {
            requested_charts: 0,
            imported_scores: 0,
            missing_scores: 0,
            failed_requests: 0,
            rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
            elapsed_seconds: 0.0,
            canceled: false,
        });
    }

    let import_started = Instant::now();
    let mut last_request_started_at: Option<Instant> = None;
    let mut imported_scores = 0usize;
    let mut missing_scores = 0usize;
    let mut failed_requests = 0usize;
    let mut processed_charts = 0usize;
    let mut canceled = false;

    'packs: for (pack_idx, (pack_name, hashes)) in pack_chart_groups.into_iter().enumerate() {
        let pack_chart_count = hashes.len();
        let mut pack_hits = 0usize;
        let mut pack_misses = 0usize;
        let mut pack_failures = 0usize;

        for chart_hash in &hashes {
            if should_cancel() {
                canceled = true;
                debug!(
                    "{} score import canceled at pack {}/{} after {processed_charts}/{requested_charts} charts.",
                    endpoint.display_name(),
                    pack_idx + 1,
                    total_packs,
                );
                break 'packs;
            }
            wait_for_next_import_request(last_request_started_at);
            if should_cancel() {
                canceled = true;
                break 'packs;
            }
            last_request_started_at = Some(Instant::now());
            match fetch_player_score_from_endpoint(endpoint, &profile, chart_hash) {
                Ok(result) => {
                    if result.itl_self_found {
                        itl::set_cached_online_self_score(
                            Some(profile_id.as_str()),
                            api_key,
                            chart_hash,
                            result.itl_self_score,
                        );
                        itl::set_cached_online_self_rank(
                            Some(profile_id.as_str()),
                            api_key,
                            chart_hash,
                            result.itl_self_rank,
                        );
                    }
                    if let Some(score) = result.score {
                        cache_gs_score_for_profile(
                            &profile_id,
                            chart_hash,
                            score,
                            username.as_str(),
                            result.score_proves_nonquint_ex,
                        );
                        imported_scores += 1;
                        pack_hits += 1;
                    } else {
                        missing_scores += 1;
                        pack_misses += 1;
                    }
                }
                Err(e) => {
                    failed_requests += 1;
                    pack_failures += 1;
                    warn!(
                        "{} import request failed for chart {}: {}",
                        endpoint.display_name(),
                        chart_hash,
                        e,
                    );
                }
            }

            processed_charts += 1;
            let detail = format!(
                "Pack {}/{}: {pack_name} -> {pack_hits} hit, {pack_misses} missing{}",
                pack_idx + 1,
                total_packs,
                if pack_failures > 0 {
                    format!(", {pack_failures} failed")
                } else {
                    String::new()
                },
            );
            on_progress(ScoreImportProgress {
                processed_charts,
                total_charts: requested_charts,
                imported_scores,
                missing_scores,
                failed_requests,
                detail,
            });
            if processed_charts == requested_charts
                || processed_charts % SCORE_IMPORT_PROGRESS_LOG_EVERY == 0
                || processed_charts == 1
            {
                debug!(
                    "{} import progress for '{}': {processed_charts}/{requested_charts} charts (imported={}, missing={}, failed={})",
                    endpoint.display_name(),
                    username,
                    imported_scores,
                    missing_scores,
                    failed_requests,
                );
            }
        }

        debug!(
            "Pack {}/{} complete: {pack_name} ({pack_chart_count} charts -> {pack_hits} hit, {pack_misses} missing{}).",
            pack_idx + 1,
            total_packs,
            if pack_failures > 0 {
                format!(", {pack_failures} failed")
            } else {
                String::new()
            },
        );
    }

    Ok(ScoreBulkImportSummary {
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
        elapsed_seconds: import_started.elapsed().as_secs_f32(),
        canceled,
    })
}

pub fn fetch_and_store_grade(
    profile_id: String,
    profile: Profile,
    chart_hash: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if profile.groovestats_api_key.trim().is_empty()
        || profile.groovestats_username.trim().is_empty()
    {
        return Err("GrooveStats API key or username is not set in profile.ini.".into());
    }

    debug!(
        "Requesting scores for '{}' on chart '{}'...",
        profile.groovestats_username, chart_hash
    );

    let endpoint = if crate::game::online::is_boogiestats_active() {
        ScoreImportEndpoint::BoogieStats
    } else {
        ScoreImportEndpoint::GrooveStats
    };
    let result = fetch_player_score_from_endpoint(endpoint, &profile, chart_hash.as_str())?;
    if let Some(cached_score) = result.score {
        cache_gs_score_for_profile(
            &profile_id,
            &chart_hash,
            cached_score,
            profile.groovestats_username.trim(),
            result.score_proves_nonquint_ex,
        );
    } else {
        warn!(
            "No score found for player '{}' on chart '{}'. Caching as Failed.",
            profile.groovestats_username, chart_hash
        );
        set_cached_gs_score_for_profile(&profile_id, chart_hash, cached_missing_gs_score());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_chart_submit_allowlist_matches_spooky_hash() {
        assert!(lua_chart_submit_allowed("d5bd4dd7224f68ff"));
        assert!(lua_chart_submit_allowed(" D5BD4DD7224F68FF "));
        assert!(!lua_chart_submit_allowed("deadbeefcafebabe"));
        assert!(lua_submit_allowed(false, "deadbeefcafebabe"));
        assert!(!lua_submit_allowed(true, "deadbeefcafebabe"));
        assert!(lua_submit_allowed(true, "d5bd4dd7224f68ff"));
    }

    #[test]
    fn groovestats_comment_counts_ignore_ds_prefix() {
        let counts = parse_comment_counts("[DS], 15e, 2g, 2m");
        assert_eq!(counts.e, 15);
        assert_eq!(counts.g, 2);
        assert_eq!(counts.m, 2);
    }

    #[test]
    fn groovestats_comment_ex_percent_accepts_ds_formats() {
        assert_eq!(parse_comment_ex_percent("[DS], FA+, 99.78EX"), Some(99.78));
        assert_eq!(
            parse_comment_ex_percent("[DS], FA+, 99.78% EX, C650"),
            Some(99.78)
        );
        assert_eq!(parse_comment_ex_percent("[DS], 3 excellents"), None);
    }

    #[test]
    fn groovestats_lamp_judge_count_ignores_ds_prefix() {
        assert_eq!(compute_lamp_judge_count(Some(1), Some("[DS], 4w")), Some(4));
        assert_eq!(compute_lamp_judge_count(Some(2), Some("[DS], 5e")), Some(5));
        assert_eq!(compute_lamp_judge_count(Some(3), Some("[DS], 2g")), Some(2));
    }

    #[test]
    fn cached_score_from_gs_uses_quint_comment_as_fallback() {
        let comment = "[DS], FA+, 100.00EX";
        let cached = cached_score_from_gs(
            10_000.0,
            Some(comment),
            "deadbeef",
            false,
            GsExEvidence::from_sources(None, Some(comment)),
        );

        assert_eq!(cached.grade, Grade::Quint);
        assert_eq!(cached.lamp_index, Some(0));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_gs_trusts_ex_leaderboard_for_quint() {
        let cached = cached_score_from_gs(
            10_000.0,
            Some("[DS], FA+, 99.78EX"),
            "deadbeef",
            false,
            GsExEvidence::from_sources(Some(10_000.0), Some("[DS], FA+, 99.78EX")),
        );

        assert_eq!(cached.grade, Grade::Quint);
        assert_eq!(cached.lamp_index, Some(0));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_gs_rejects_quint_comment_when_ex_leaderboard_is_lower() {
        let cached = cached_score_from_gs(
            10_000.0,
            Some("[DS], FA+, 100.00EX, C875"),
            "deadbeef",
            false,
            GsExEvidence::from_sources(Some(9_978.0), Some("[DS], FA+, 100.00EX, C875")),
        );

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_gs_keeps_quad_white_count() {
        let comment = "[DS], FA+, 99.71EX, 3w";
        let cached = cached_score_from_gs(
            10_000.0,
            Some(comment),
            "deadbeef",
            false,
            GsExEvidence::from_sources(None, Some(comment)),
        );

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, Some(3));
    }

    #[test]
    fn cached_score_from_gs_keeps_quad_from_nonperfect_ex_comment() {
        let comment = "[DS], FA+, 99.78EX";
        let cached = cached_score_from_gs(
            10_000.0,
            Some(comment),
            "deadbeef",
            false,
            GsExEvidence::from_sources(None, Some(comment)),
        );

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_gs_does_not_infer_quint_without_ex_evidence() {
        let cached = cached_score_from_gs(
            10_000.0,
            Some("[DS], FA+"),
            "deadbeef",
            false,
            GsExEvidence::default(),
        );

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_from_entry_downgrades_stale_quint_without_quint_lamp() {
        let cached = cached_from_entry(&GsScoreEntry {
            score_percent: 1.0,
            grade_code: grade_to_code(Grade::Quint),
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
            username: "Player".to_string(),
            fetched_at_ms: 0,
        });

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, Some(3));
    }

    #[test]
    fn cached_from_entry_promotes_quint_from_quint_lamp() {
        let cached = cached_from_entry(&GsScoreEntry {
            score_percent: 1.0,
            grade_code: grade_to_code(Grade::Tier01),
            lamp_index: Some(0),
            lamp_judge_count: None,
            username: "Player".to_string(),
            fetched_at_ms: 0,
        });

        assert_eq!(cached.grade, Grade::Quint);
        assert_eq!(cached.lamp_index, Some(0));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn promote_quint_grade_ignores_display_mode() {
        assert_eq!(promote_quint_grade(Grade::Tier01, 100.0), Grade::Quint);
        assert_eq!(promote_quint_grade(Grade::Tier01, 99.99), Grade::Tier01);
        assert_eq!(promote_quint_grade(Grade::Failed, 100.0), Grade::Failed);
    }

    #[test]
    fn local_lamp_uses_single_digit_white_fantastics() {
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
    fn is_better_itg_prefers_richer_tied_score() {
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
    fn stale_inferred_quint_replacement_requires_nonperfect_ex() {
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

        assert!(replaces_stale_inferred_quint(
            &corrected_quad,
            &stale_quint,
            true
        ));
        assert!(!replaces_stale_inferred_quint(
            &corrected_quad,
            &stale_quint,
            false
        ));
        assert!(!is_better_itg(&corrected_quad, &stale_quint));
    }

    #[test]
    fn leaderboard_self_score_prefers_self_flag_for_itl() {
        let entries = vec![
            LeaderboardApiEntry {
                rank: 10,
                name: "Other".to_string(),
                machine_tag: None,
                score: 9321.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
                comments: None,
            },
            LeaderboardApiEntry {
                rank: 25,
                name: "Player".to_string(),
                machine_tag: None,
                score: 9789.0,
                date: String::new(),
                is_rival: false,
                is_self: true,
                is_fail: false,
                comments: None,
            },
        ];

        assert_eq!(
            leaderboard_self_score_10000(&entries, "ignored"),
            Some(9789)
        );
        assert_eq!(leaderboard_self_rank(&entries, "ignored"), Some(25));
    }

    #[test]
    fn leaderboard_self_score_falls_back_to_username_match() {
        let entries = vec![LeaderboardApiEntry {
            rank: 25,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 9712.0,
            date: String::new(),
            is_rival: false,
            is_self: false,
            is_fail: false,
            comments: None,
        }];

        assert_eq!(
            leaderboard_self_score_10000(&entries, "perfecttaste"),
            Some(9712)
        );
        assert_eq!(leaderboard_self_rank(&entries, "perfecttaste"), Some(25));
    }

    #[test]
    fn leaderboard_self_rank_ignores_zero_rank() {
        let entries = vec![LeaderboardApiEntry {
            rank: 0,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 9712.0,
            date: String::new(),
            is_rival: false,
            is_self: true,
            is_fail: false,
            comments: None,
        }];

        assert_eq!(leaderboard_self_rank(&entries, "perfecttaste"), None);
    }

    #[test]
    fn score_import_result_includes_itl_self_data() {
        let result = score_import_result_from_response(
            LeaderboardsApiResponse {
                player1: Some(LeaderboardApiPlayer {
                    is_ranked: true,
                    gs_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 9876.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: Some("[DS], 2e".to_string()),
                    }],
                    ex_leaderboard: Vec::new(),
                    rpg: None,
                    itl: Some(LeaderboardEventData {
                        name: "ITL Online 2026".to_string(),
                        rpg_leaderboard: Vec::new(),
                        itl_leaderboard: vec![LeaderboardApiEntry {
                            rank: 42,
                            name: "PerfectTaste".to_string(),
                            machine_tag: None,
                            score: 9912.0,
                            date: String::new(),
                            is_rival: false,
                            is_self: true,
                            is_fail: false,
                            comments: None,
                        }],
                    }),
                }),
            },
            ScoreImportEndpoint::GrooveStats,
            "perfecttaste",
            "deadbeef",
        );

        assert!(result.score.is_some());
        assert!(result.itl_self_found);
        assert_eq!(result.itl_self_score, Some(9912));
        assert_eq!(result.itl_self_rank, Some(42));
    }

    #[test]
    fn score_import_result_uses_ex_leaderboard_for_quint() {
        let result = score_import_result_from_response(
            LeaderboardsApiResponse {
                player1: Some(LeaderboardApiPlayer {
                    is_ranked: true,
                    gs_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 10_000.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: Some("[DS], FA+, 99.78EX".to_string()),
                    }],
                    ex_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 10_000.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: None,
                    }],
                    rpg: None,
                    itl: None,
                }),
            },
            ScoreImportEndpoint::GrooveStats,
            "perfecttaste",
            "deadbeef",
        );

        let score = result.score.expect("score cached");
        assert_eq!(score.grade, Grade::Quint);
        assert_eq!(score.lamp_index, Some(0));
    }

    #[test]
    fn score_import_result_rejects_quint_comment_when_ex_leaderboard_is_lower() {
        let result = score_import_result_from_response(
            LeaderboardsApiResponse {
                player1: Some(LeaderboardApiPlayer {
                    is_ranked: true,
                    gs_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 10_000.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: Some("[DS], FA+, 100.00EX, C875".to_string()),
                    }],
                    ex_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 9_978.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: None,
                    }],
                    rpg: None,
                    itl: None,
                }),
            },
            ScoreImportEndpoint::GrooveStats,
            "perfecttaste",
            "deadbeef",
        );

        let score = result.score.expect("score cached");
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));
        assert!(result.score_proves_nonquint_ex);
    }

    #[test]
    fn cache_gs_score_from_leaderboard_keeps_existing_score_when_self_missing() {
        let profile_id = "test-profile-missing-self";
        let chart_hash = "deadbeef";
        let existing = CachedScore {
            grade: Grade::Tier01,
            score_percent: 0.9934,
            lamp_index: Some(1),
            lamp_judge_count: Some(2),
        };
        {
            let mut state = GS_SCORE_CACHE.lock().unwrap();
            state.loaded_profiles.insert(
                profile_id.to_string(),
                HashMap::from([(chart_hash.to_string(), existing)]),
            );
        }

        cache_gs_score_from_leaderboard(
            profile_id,
            "PerfectTaste",
            chart_hash,
            &[LeaderboardApiEntry {
                rank: 1,
                name: "Other".to_string(),
                machine_tag: None,
                score: 9999.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
                comments: None,
            }],
            &[],
        );

        let cached = GS_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .get(profile_id)
            .and_then(|scores| scores.get(chart_hash))
            .copied();
        assert_eq!(cached, Some(existing));

        GS_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .remove(profile_id);
    }

    #[test]
    fn cached_score_from_gs_preserves_failed_percent() {
        let cached = cached_score_from_gs(
            9482.0,
            Some("[DS], 3e"),
            "deadbeef",
            true,
            GsExEvidence::default(),
        );

        assert_eq!(cached.grade, Grade::Failed);
        assert_eq!(cached.score_percent, 0.9482);
        assert_eq!(cached.lamp_index, None);
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_local_score_from_header_treats_fail_time_as_failed() {
        let header = LocalScoreEntryHeaderV1 {
            version: LOCAL_SCORE_VERSION_V1,
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

        let cached = cached_local_score_from_header(&header);

        assert_eq!(cached.grade, Grade::Failed);
        assert_eq!(cached.score_percent, 0.9482);
        assert_eq!(cached.lamp_index, None);
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_local_score_from_header_downgrades_stale_quint() {
        let header = LocalScoreEntryHeaderV1 {
            version: LOCAL_SCORE_VERSION_V1,
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

        let cached = cached_local_score_from_header(&header);

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_local_score_from_header_promotes_perfect_ex_to_quint() {
        let header = LocalScoreEntryHeaderV1 {
            version: LOCAL_SCORE_VERSION_V1,
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

        let cached = cached_local_score_from_header(&header);

        assert_eq!(cached.grade, Grade::Quint);
        assert_eq!(cached.lamp_index, Some(0));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cache_gs_score_for_profile_overwrites_same_score_pass_with_fail() {
        let profile_id = "test-profile-force-fail";
        let chart_hash = "deadbeef";
        {
            let mut state = GS_SCORE_CACHE.lock().unwrap();
            state.loaded_profiles.insert(
                profile_id.to_string(),
                HashMap::from([(
                    chart_hash.to_string(),
                    CachedScore {
                        grade: Grade::Tier17,
                        score_percent: 0.1358,
                        lamp_index: None,
                        lamp_judge_count: None,
                    },
                )]),
            );
        }

        cache_gs_score_for_profile(
            profile_id,
            chart_hash,
            CachedScore {
                grade: Grade::Failed,
                score_percent: 0.1358,
                lamp_index: None,
                lamp_judge_count: None,
            },
            "PerfectTaste",
            false,
        );

        let cached = GS_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .get(profile_id)
            .and_then(|scores| scores.get(chart_hash))
            .copied();
        assert_eq!(cached.map(|score| score.grade), Some(Grade::Failed));

        GS_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .remove(profile_id);
    }

    #[test]
    fn cache_gs_score_from_leaderboard_uses_matching_local_fail() {
        let profile_id = "test-profile-local-fail";
        let chart_hash = "deadbeef";
        {
            let mut state = LOCAL_SCORE_CACHE.lock().unwrap();
            state.loaded_profiles.insert(
                profile_id.to_string(),
                LocalScoreIndex {
                    best_itg: HashMap::from([(
                        chart_hash.to_string(),
                        CachedScore {
                            grade: Grade::Failed,
                            score_percent: 0.1358,
                            lamp_index: None,
                            lamp_judge_count: None,
                        },
                    )]),
                    ..LocalScoreIndex::default()
                },
            );
        }

        cache_gs_score_from_leaderboard(
            profile_id,
            "PerfectTaste",
            chart_hash,
            &[LeaderboardApiEntry {
                rank: 498,
                name: "PerfectTaste".to_string(),
                machine_tag: None,
                score: 1358.0,
                date: String::new(),
                is_rival: false,
                is_self: true,
                is_fail: false,
                comments: None,
            }],
            &[],
        );

        let cached = GS_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .get(profile_id)
            .and_then(|scores| scores.get(chart_hash))
            .copied();
        assert_eq!(cached.map(|score| score.grade), Some(Grade::Failed));

        LOCAL_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .remove(profile_id);
        GS_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .remove(profile_id);
    }

    #[test]
    fn player_leaderboard_cache_reuses_success_until_more_rows_are_needed() {
        let ready = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(PlayerLeaderboardData {
                panes: Vec::new(),
                itl_self_score: None,
                itl_self_rank: None,
            }),
            max_entries: 5,
            refreshed_at: Instant::now(),
            retry_after: None,
        };
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&ready),
            5,
            false
        ));
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&ready),
            3,
            false
        ));
        assert!(should_fetch_player_leaderboard_entry(
            Some(&ready),
            10,
            false
        ));
        assert!(should_fetch_player_leaderboard_entry(Some(&ready), 5, true));

        let cooled_down_ready = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(PlayerLeaderboardData {
                panes: Vec::new(),
                itl_self_score: None,
                itl_self_rank: None,
            }),
            max_entries: 5,
            refreshed_at: Instant::now(),
            retry_after: Some(Instant::now() + PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL),
        };
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&cooled_down_ready),
            10,
            false
        ));
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&cooled_down_ready),
            5,
            true
        ));

        let stale_error = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Error("boom".to_string()),
            max_entries: 5,
            refreshed_at: Instant::now() - PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
            retry_after: Some(Instant::now() - Duration::from_millis(1)),
        };
        assert!(should_fetch_player_leaderboard_entry(
            Some(&stale_error),
            5,
            false
        ));
    }

    #[test]
    fn in_flight_leaderboard_fetch_reruns_for_submit_refresh() {
        assert!(!should_rerun_in_flight_player_leaderboard_fetch(
            5, 5, false
        ));
        assert!(should_rerun_in_flight_player_leaderboard_fetch(
            5, 10, false
        ));
        assert!(should_rerun_in_flight_player_leaderboard_fetch(5, 5, true));
    }

    #[test]
    fn queued_leaderboard_refresh_keeps_largest_request() {
        let key = PlayerLeaderboardCacheKey {
            chart_hash: "deadbeef".to_string(),
            api_key: "gs".to_string(),
            arrowcloud_api_key: "ac".to_string(),
            include_arrowcloud: true,
            show_ex_score: false,
        };
        let mut pending_refresh = HashMap::new();

        queue_player_leaderboard_refresh(&mut pending_refresh, &key, 5);
        queue_player_leaderboard_refresh(&mut pending_refresh, &key, 10);
        queue_player_leaderboard_refresh(&mut pending_refresh, &key, 3);

        assert_eq!(pending_refresh.get(&key), Some(&10));
    }

    #[test]
    fn arrowcloud_hard_ex_pane_from_response_filters_to_hardex() {
        let pane = arrowcloud_hard_ex_pane_from_response(ArrowCloudLeaderboardsApiResponse {
            leaderboards: vec![
                ArrowCloudLeaderboardPane {
                    r#type: "EX".to_string(),
                    scores: Vec::new(),
                    page: 1,
                    has_next: false,
                    total_pages: 1,
                },
                ArrowCloudLeaderboardPane {
                    r#type: "HardEX".to_string(),
                    scores: vec![ArrowCloudLeaderboardEntry {
                        rank: 7,
                        score: 98.31,
                        alias: "YOU".to_string(),
                        date: "2026-04-18T12:34:56.000Z".to_string(),
                        user_id: "self".to_string(),
                        is_rival: false,
                        is_self: false,
                    }],
                    page: 1,
                    has_next: true,
                    total_pages: 4,
                },
            ],
        })
        .expect("expected HardEX pane");

        assert_eq!(pane.r#type, "HardEX");
        assert_eq!(pane.page, 1);
        assert!(pane.has_next);
        assert_eq!(pane.total_pages, 4);
        assert_eq!(pane.scores.len(), 1);
        assert_eq!(pane.scores[0].user_id, "self");
    }

    #[test]
    fn arrowcloud_hard_ex_pane_from_pages_marks_self_and_rival_from_user_ids() {
        let context = ArrowCloudUserContext {
            self_user_id: Some("self".to_string()),
            rival_user_ids: HashSet::from([String::from("rival")]),
        };
        let pane = arrowcloud_hard_ex_pane_from_pages(
            ArrowCloudLeaderboardPane {
                r#type: "HardEX".to_string(),
                scores: vec![ArrowCloudLeaderboardEntry {
                    rank: 1,
                    score: 99.12,
                    alias: "AAA".to_string(),
                    date: String::new(),
                    user_id: "top".to_string(),
                    is_rival: false,
                    is_self: false,
                }],
                page: 1,
                has_next: true,
                total_pages: 4,
            },
            vec![ArrowCloudLeaderboardPane {
                r#type: "HardEX".to_string(),
                scores: vec![
                    ArrowCloudLeaderboardEntry {
                        rank: 81,
                        score: 88.99,
                        alias: "YOU".to_string(),
                        date: String::new(),
                        user_id: "self".to_string(),
                        is_rival: false,
                        is_self: false,
                    },
                    ArrowCloudLeaderboardEntry {
                        rank: 91,
                        score: 87.65,
                        alias: "RIVAL".to_string(),
                        date: String::new(),
                        user_id: "rival".to_string(),
                        is_rival: false,
                        is_self: false,
                    },
                ],
                page: 4,
                has_next: false,
                total_pages: 4,
            }],
            Some(&context),
        );

        assert_eq!(pane.arrowcloud_kind, Some(ArrowCloudPaneKind::HardEx));
        assert!(pane.personalized);
        assert_eq!(pane.entries.len(), 3);
        assert_eq!(pane.entries[1].rank, 81);
        assert!(pane.entries[1].is_self);
        assert_eq!(pane.entries[2].rank, 91);
        assert!(pane.entries[2].is_rival);
    }

    #[test]
    fn empty_arrowcloud_hard_ex_pane_stays_visible_as_hard_ex() {
        let pane = empty_arrowcloud_hard_ex_pane();

        assert!(pane.entries.is_empty());
        assert!(!pane.personalized);
        assert!(pane.is_arrowcloud());
        assert!(pane.is_hard_ex());
    }

    #[test]
    fn newer_player_leaderboard_entry_blocks_older_fetch_result() {
        let newer_entry = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(PlayerLeaderboardData {
                panes: Vec::new(),
                itl_self_score: None,
                itl_self_rank: None,
            }),
            max_entries: 0,
            refreshed_at: Instant::now(),
            retry_after: None,
        };
        let older_request_started_at = Instant::now() - Duration::from_millis(1);
        assert!(should_keep_newer_player_leaderboard_entry(
            Some(&newer_entry),
            older_request_started_at,
        ));

        let older_entry = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(PlayerLeaderboardData {
                panes: Vec::new(),
                itl_self_score: None,
                itl_self_rank: None,
            }),
            max_entries: 0,
            refreshed_at: Instant::now() - Duration::from_secs(1),
            retry_after: None,
        };
        assert!(!should_keep_newer_player_leaderboard_entry(
            Some(&older_entry),
            Instant::now(),
        ));
    }

    #[test]
    fn leaderboard_rank_for_score_does_not_require_name_match() {
        let entries = vec![
            LeaderboardEntry {
                rank: 1,
                name: "AAA".to_string(),
                machine_tag: None,
                score: 9999.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
            },
            LeaderboardEntry {
                rank: 2,
                name: "BBB".to_string(),
                machine_tag: None,
                score: 9750.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
            },
        ];
        assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
    }

    #[test]
    fn leaderboard_rank_for_score_places_current_run_ahead_of_equal_scores() {
        let entries = vec![
            LeaderboardEntry {
                rank: 1,
                name: "AAA".to_string(),
                machine_tag: None,
                score: 9999.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
            },
            LeaderboardEntry {
                rank: 2,
                name: "BBB".to_string(),
                machine_tag: None,
                score: 9750.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
            },
            LeaderboardEntry {
                rank: 3,
                name: "CCC".to_string(),
                machine_tag: None,
                score: 9750.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
            },
        ];
        assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
    }

    #[test]
    fn leaderboard_rank_for_score_rejects_non_finite_scores() {
        assert_eq!(leaderboard_rank_for_score(&[], f64::NAN), None);
    }

    // --- ArrowCloud bulk score import tests ---

    fn ac_entry(score: f64, is_fail: bool) -> ArrowCloudRetrieveScoreEntry {
        ArrowCloudRetrieveScoreEntry {
            score: Some(score),
            grade: None,
            date: None,
            play_id: None,
            is_fail,
        }
    }

    fn ac_entry_full(
        score: f64,
        grade: Option<&str>,
        date: Option<&str>,
        play_id: Option<i64>,
        is_fail: bool,
    ) -> ArrowCloudRetrieveScoreEntry {
        ArrowCloudRetrieveScoreEntry {
            score: Some(score),
            grade: grade.map(str::to_string),
            date: date.map(str::to_string),
            play_id,
            is_fail,
        }
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
    fn arrowcloud_scores_from_entry_map_assigns_global_leaderboards() {
        let mut map = HashMap::new();
        map.insert("4".to_string(), ac_entry(99.51, false));
        map.insert("2".to_string(), ac_entry(98.10, false));
        map.insert("3".to_string(), ac_entry(99.89, false));
        let scores = arrowcloud_scores_from_entry_map(&map);
        assert!(scores.itg.is_some());
        assert!(scores.ex.is_some());
        assert!(scores.hard_ex.is_some());
        assert!((scores.itg.unwrap().score_percent - 0.9989).abs() < 1e-6);
        assert!((scores.ex.unwrap().score_percent - 0.9810).abs() < 1e-6);
        assert!((scores.hard_ex.unwrap().score_percent - 0.9951).abs() < 1e-6);
    }

    #[test]
    fn arrowcloud_scores_from_entry_map_ignores_unknown_leaderboard_ids() {
        let mut map = HashMap::new();
        map.insert("3".to_string(), ac_entry(99.0, false));
        // Event-specific leaderboard (e.g. BlueShift) – should be dropped.
        map.insert("9".to_string(), ac_entry(95.0, false));
        let scores = arrowcloud_scores_from_entry_map(&map);
        assert!(scores.itg.is_some());
        assert!(scores.ex.is_none());
        assert!(scores.hard_ex.is_none());
    }

    #[test]
    fn arrowcloud_scores_from_entry_map_ignores_unparseable_keys() {
        let mut map = HashMap::new();
        map.insert("itg".to_string(), ac_entry(99.0, false));
        let scores = arrowcloud_scores_from_entry_map(&map);
        assert!(scores.itg.is_none());
    }

    #[test]
    fn arrowcloud_scores_from_entry_map_drops_entries_without_score_field() {
        let mut map = HashMap::new();
        map.insert(
            "3".to_string(),
            ArrowCloudRetrieveScoreEntry {
                score: None,
                grade: None,
                date: None,
                play_id: None,
                is_fail: false,
            },
        );
        let scores = arrowcloud_scores_from_entry_map(&map);
        assert!(scores.itg.is_none(), "missing score must not cache as 0%");
    }

    #[test]
    fn arrowcloud_scores_from_entry_preserves_grade_and_date() {
        let mut map = HashMap::new();
        map.insert(
            "3".to_string(),
            ac_entry_full(
                99.89,
                Some("Tristar"),
                Some("2026-05-03T19:10:17.504Z"),
                Some(12345),
                false,
            ),
        );
        let scores = arrowcloud_scores_from_entry_map(&map);
        let itg = scores.itg.unwrap();
        assert_eq!(itg.server_grade, Some(ArrowCloudServerGrade::Tristar));
        assert_eq!(itg.play_id, Some(12345));
        let played_at = itg.played_at.expect("played_at parsed");
        assert_eq!(played_at.timestamp_millis(), 1_777_835_417_504);
    }

    #[test]
    fn arrowcloud_scores_from_entry_drops_unknown_grade() {
        let mut map = HashMap::new();
        map.insert(
            "3".to_string(),
            ac_entry_full(98.0, Some("Mythic"), None, None, false),
        );
        let scores = arrowcloud_scores_from_entry_map(&map);
        assert_eq!(scores.itg.unwrap().server_grade, None);
    }

    #[test]
    fn arrowcloud_scores_from_entry_drops_unparseable_date() {
        let mut map = HashMap::new();
        map.insert(
            "3".to_string(),
            ac_entry_full(98.0, None, Some("not-a-date"), None, false),
        );
        let scores = arrowcloud_scores_from_entry_map(&map);
        assert_eq!(scores.itg.unwrap().played_at, None);
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
    fn merge_arrowcloud_score_slot_inserts_when_empty() {
        let mut slot: Option<ArrowCloudScore> = None;
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.95, false)));
        assert!(slot.is_some());
        assert!((slot.unwrap().score_percent - 0.95).abs() < 1e-9);
    }

    #[test]
    fn merge_arrowcloud_score_slot_keeps_higher_percent() {
        let mut slot = Some(ac_score(0.99, false));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.97, false)));
        assert!((slot.unwrap().score_percent - 0.99).abs() < 1e-9);
    }

    #[test]
    fn merge_arrowcloud_score_slot_overwrites_lower_percent() {
        let mut slot = Some(ac_score(0.90, false));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.92, false)));
        assert!((slot.unwrap().score_percent - 0.92).abs() < 1e-9);
    }

    #[test]
    fn merge_arrowcloud_score_slot_failed_does_not_overwrite_passed() {
        let mut slot = Some(ac_score(0.85, false));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.95, true)));
        assert!(
            !slot.unwrap().is_fail,
            "failed score must not overwrite passed"
        );
    }

    #[test]
    fn merge_arrowcloud_score_slot_passed_overwrites_failed() {
        let mut slot = Some(ac_score(0.85, true));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.50, false)));
        assert!(
            !slot.unwrap().is_fail,
            "non-failed score must replace failed"
        );
    }

    #[test]
    fn merge_arrowcloud_score_slot_ignores_none() {
        let mut slot = Some(ac_score(0.85, false));
        merge_arrowcloud_score_slot(&mut slot, None);
        assert!((slot.unwrap().score_percent - 0.85).abs() < 1e-9);
    }

    #[test]
    fn arrowcloud_retrieve_response_decodes_full_shape() {
        // Mirrors the documented bulk response shape.
        let raw = r#"{
            "scores": {
                "006fb5c4890e98a2": {
                    "2": { "score": "99.12", "grade": "Tristar", "date": "2026-05-03T19:10:17.504Z" },
                    "3": { "score": "99.89", "grade": "Tristar", "date": "2026-05-03T19:10:17.504Z" }
                },
                "0092bb246527b2ec": {
                    "2": { "score": "97.44", "grade": "Twostar", "date": "2026-05-02T11:03:42.000Z" }
                }
            }
        }"#;
        let decoded: ArrowCloudRetrieveScoresResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(decoded.scores.len(), 2);
        assert!(decoded.scores["006fb5c4890e98a2"].contains_key("2"));
        assert!(decoded.scores["006fb5c4890e98a2"].contains_key("3"));
        assert_eq!(decoded.scores["0092bb246527b2ec"]["2"].score, Some(97.44));
    }

    #[test]
    fn arrowcloud_retrieve_response_ignores_unknown_top_level_fields() {
        let raw = r#"{ "scores": {}, "extra": 42, "meta": { "x": 1 } }"#;
        let decoded: ArrowCloudRetrieveScoresResponse = serde_json::from_str(raw).unwrap();
        assert!(decoded.scores.is_empty());
    }

    #[test]
    fn arrowcloud_retrieve_response_treats_missing_score_field_as_none() {
        let raw = r#"{
            "scores": {
                "abc": { "3": { "grade": "n/a" } }
            }
        }"#;
        let decoded: ArrowCloudRetrieveScoresResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(decoded.scores["abc"]["3"].score, None);
    }

    #[test]
    fn arrowcloud_global_leaderboard_set_is_hardex_ex_itg() {
        // Order matters for the request body payload; the server is
        // order-insensitive but we send HardEX first to mirror display priority.
        let ids: Vec<u32> = ArrowCloudLeaderboard::ALL_GLOBAL
            .iter()
            .map(|v| v.id())
            .collect();
        assert_eq!(ids, vec![4, 2, 3]);
    }
}
