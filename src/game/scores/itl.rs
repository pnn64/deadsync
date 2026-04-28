use super::{
    GrooveStatsSubmitApiAchievement, GrooveStatsSubmitApiEvent, GrooveStatsSubmitApiPlayer,
    GrooveStatsSubmitApiProgress, GrooveStatsSubmitPlayerJob, LeaderboardApiEntry,
    LeaderboardEntry, gameplay_run_passed, gameplay_side_for_player,
    get_cached_player_leaderboard_itl_self_rank_for_side,
    get_or_fetch_player_leaderboards_for_side_inner, groovestats_eval_state_from_gameplay,
    groovestats_judgment_counts, leaderboard_entries_from_api,
};
use crate::config::dirs;
use crate::game::gameplay;
use crate::game::judgment;
use crate::game::online::downloads;
use crate::game::profile;
use crate::game::song::{get_song_cache, song_cache_generation};
use chrono::Local;
use log::{debug, warn};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use bincode::{Decode, Encode};

const ITL_FILE_NAME: &str = "ITL2026.json";
const ITL_WHEEL_FETCH_ENTRIES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachedItlScore {
    pub ex_hundredths: u32,
    pub clear_type: u8,
    pub points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
struct OnlineItlSelfScoreKey {
    chart_hash: String,
    api_key: String,
}

#[derive(Default)]
struct OnlineItlSelfScoreCacheState {
    session_by_key: HashMap<OnlineItlSelfScoreKey, u32>,
    loaded_profiles: HashMap<String, HashMap<OnlineItlSelfScoreKey, u32>>,
}

static ONLINE_ITL_SELF_SCORE_CACHE: std::sync::LazyLock<Mutex<OnlineItlSelfScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(OnlineItlSelfScoreCacheState::default()));
static ONLINE_ITL_SELF_SCORE_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
struct OnlineItlOverallRankCacheKey {
    api_key: String,
    profile_id: Option<String>,
    song_cache_generation: u64,
    self_score_generation: u64,
}

#[derive(Clone)]
struct OnlineItlOverallRankCacheEntry {
    key: OnlineItlOverallRankCacheKey,
    ranks: Arc<HashMap<String, u32>>,
}

#[derive(Default)]
struct OnlineItlOverallRankCacheState {
    p1: Option<OnlineItlOverallRankCacheEntry>,
    p2: Option<OnlineItlOverallRankCacheEntry>,
}

static ONLINE_ITL_OVERALL_RANK_CACHE: std::sync::LazyLock<Mutex<OnlineItlOverallRankCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(OnlineItlOverallRankCacheState::default()));
static EMPTY_ONLINE_ITL_OVERALL_RANKS: std::sync::LazyLock<Arc<HashMap<String, u32>>> =
    std::sync::LazyLock::new(|| Arc::new(HashMap::new()));

#[derive(Default)]
struct ItlScoreCacheState {
    loaded_profiles: HashMap<String, ItlFileData>,
}

static ITL_SCORE_CACHE: std::sync::LazyLock<Mutex<ItlScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(ItlScoreCacheState::default()));

struct OnlineItlOverallRankInput {
    api_key: String,
    profile_id: Option<String>,
    self_score_generation: u64,
    by_chart_score: HashMap<String, u32>,
}

#[derive(Clone, Debug, Default)]
pub struct ItlEvalState {
    pub active: bool,
    pub eligible: bool,
    pub chart_no_cmod: bool,
    pub used_cmod: bool,
    pub reason_lines: Vec<String>,
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ItlFileData {
    #[serde(rename = "pathMap", default)]
    path_map: HashMap<String, String>,
    #[serde(rename = "hashMap", default)]
    hash_map: HashMap<String, ItlHashEntry>,
    #[serde(default)]
    points: Vec<u32>,
    #[serde(rename = "pointsSingle", default)]
    points_single: Vec<u32>,
    #[serde(rename = "pointsDouble", default)]
    points_double: Vec<u32>,
    #[serde(rename = "unlockFolders", default)]
    unlock_folders: HashMap<String, bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ItlHashEntry {
    #[serde(default)]
    judgments: ItlJudgments,
    #[serde(default, deserialize_with = "deserialize_itl_ex")]
    ex: u32,
    #[serde(rename = "clearType", default)]
    clear_type: u8,
    #[serde(default)]
    points: u32,
    #[serde(rename = "usedCmod", default)]
    used_cmod: bool,
    #[serde(default)]
    date: String,
    #[serde(rename = "noCmod", default)]
    no_cmod: bool,
    #[serde(rename = "passingPoints", default)]
    passing_points: u32,
    #[serde(rename = "maxScoringPoints", default)]
    max_scoring_points: u32,
    #[serde(rename = "maxPoints", default)]
    max_points: u32,
    #[serde(default)]
    rank: Option<u32>,
    #[serde(rename = "stepsType", default)]
    steps_type: String,
    #[serde(default)]
    passes: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ItlJudgments {
    #[serde(rename = "W0", default)]
    w0: u32,
    #[serde(rename = "W1", default)]
    w1: u32,
    #[serde(rename = "W2", default)]
    w2: u32,
    #[serde(rename = "W3", default)]
    w3: u32,
    #[serde(rename = "W4", default)]
    w4: u32,
    #[serde(rename = "W5", default)]
    w5: u32,
    #[serde(rename = "Miss", default)]
    miss: u32,
    #[serde(rename = "totalSteps", default)]
    total_steps: u32,
    #[serde(rename = "Holds", default)]
    holds: u32,
    #[serde(rename = "totalHolds", default)]
    total_holds: u32,
    #[serde(rename = "Mines", default)]
    mines: u32,
    #[serde(rename = "totalMines", default)]
    total_mines: u32,
    #[serde(rename = "Rolls", default)]
    rolls: u32,
    #[serde(rename = "totalRolls", default)]
    total_rolls: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ItlPointTotals {
    ranking_points: u32,
    song_points: u32,
    ex_points: u32,
    total_points: u32,
}

fn online_itl_self_score_key_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<OnlineItlSelfScoreKey> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() || !profile::is_session_side_joined(side) {
        return None;
    }
    let side_profile = profile::get_for_side(side);
    let api_key = side_profile.groovestats_api_key.trim();
    if api_key.is_empty() {
        return None;
    }
    Some(OnlineItlSelfScoreKey {
        chart_hash: chart_hash.to_string(),
        api_key: api_key.to_string(),
    })
}

fn online_itl_self_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    dirs::app_dirs()
        .profiles_root()
        .join(profile_id)
        .join("scores")
        .join("gs")
        .join("itl_self.bin")
}

fn load_online_itl_self_score_index(path: &Path) -> Option<HashMap<OnlineItlSelfScoreKey, u32>> {
    let bytes = fs::read(path).ok()?;
    let (by_key, _) = bincode::decode_from_slice::<HashMap<OnlineItlSelfScoreKey, u32>, _>(
        &bytes,
        bincode::config::standard(),
    )
    .ok()?;
    Some(by_key)
}

fn save_online_itl_self_score_index(path: &Path, by_key: &HashMap<OnlineItlSelfScoreKey, u32>) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(error) = fs::create_dir_all(parent) {
        warn!("Failed to create ITL self-score cache dir {parent:?}: {error}");
        return;
    }
    let Ok(buf) = bincode::encode_to_vec(by_key, bincode::config::standard()) else {
        warn!("Failed to encode ITL self-score cache at {path:?}");
        return;
    };
    let tmp_path = path.with_extension("tmp");
    if let Err(error) = fs::write(&tmp_path, buf) {
        warn!("Failed to write ITL self-score temp file {tmp_path:?}: {error}");
        return;
    }
    if let Err(error) = fs::rename(&tmp_path, path) {
        warn!("Failed to commit ITL self-score cache {path:?}: {error}");
        let _ = fs::remove_file(&tmp_path);
    }
}

#[inline(always)]
fn online_itl_overall_rank_entry_for_side(
    state: &OnlineItlOverallRankCacheState,
    side: profile::PlayerSide,
) -> Option<&OnlineItlOverallRankCacheEntry> {
    match side {
        profile::PlayerSide::P2 => state.p2.as_ref(),
        _ => state.p1.as_ref(),
    }
}

#[inline(always)]
fn online_itl_overall_rank_entry_for_side_mut(
    state: &mut OnlineItlOverallRankCacheState,
    side: profile::PlayerSide,
) -> &mut Option<OnlineItlOverallRankCacheEntry> {
    match side {
        profile::PlayerSide::P2 => &mut state.p2,
        _ => &mut state.p1,
    }
}

fn ensure_online_itl_self_score_cache_loaded_for_profile(profile_id: &str) {
    let needs_load = {
        let state = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
        !state.loaded_profiles.contains_key(profile_id)
    };
    if !needs_load {
        return;
    }

    let by_key =
        load_online_itl_self_score_index(&online_itl_self_score_index_path_for_profile(profile_id))
            .unwrap_or_default();
    ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(by_key);
}

pub(super) fn set_cached_online_self_score(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    score: Option<u32>,
) {
    let api_key = api_key.trim();
    let chart_hash = chart_hash.trim();
    if api_key.is_empty() || chart_hash.is_empty() {
        return;
    }
    let key = OnlineItlSelfScoreKey {
        chart_hash: chart_hash.to_string(),
        api_key: api_key.to_string(),
    };
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    let (changed, snapshot) = if let Some(profile_id) = profile_id {
        ensure_online_itl_self_score_cache_loaded_for_profile(profile_id);
        {
            let mut state = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
            let session_changed = if let Some(score) = score {
                state.session_by_key.insert(key.clone(), score) != Some(score)
            } else {
                state.session_by_key.remove(&key).is_some()
            };
            let Some(profile_scores) = state.loaded_profiles.get_mut(profile_id) else {
                return;
            };
            let profile_changed = if let Some(score) = score {
                profile_scores.insert(key.clone(), score) != Some(score)
            } else {
                profile_scores.remove(&key).is_some()
            };
            (
                session_changed || profile_changed,
                profile_changed.then(|| (profile_id.to_string(), profile_scores.clone())),
            )
        }
    } else {
        let mut state = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
        (
            if let Some(score) = score {
                state.session_by_key.insert(key, score) != Some(score)
            } else {
                state.session_by_key.remove(&key).is_some()
            },
            None,
        )
    };

    if changed {
        ONLINE_ITL_SELF_SCORE_GENERATION.fetch_add(1, AtomicOrdering::Relaxed);
    }

    if let Some((profile_id, by_key)) = snapshot {
        save_online_itl_self_score_index(
            &online_itl_self_score_index_path_for_profile(profile_id.as_str()),
            &by_key,
        );
    }
}

pub fn get_cached_itl_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_itl_score_cache_loaded(&profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|data| data.hash_map.get(chart_hash))
        .map(itl_score_from_entry)
}

pub fn get_cached_itl_score_for_song(
    song: &crate::game::song::SongData,
    side: profile::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_itl_score_cache_loaded(&profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|data| itl_score_for_song(song, data))
}

/// Returns true if the song folder is unlocked for this player's ITL profile.
/// Songs not present in the unlock map are treated as locked, matching SL.
pub fn is_itl_song_folder_unlocked_for_side(song_folder: &str, side: profile::PlayerSide) -> bool {
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return false;
    };
    ensure_itl_score_cache_loaded(&profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .map(|data| data.unlock_folders.get(song_folder).copied().unwrap_or(false))
        .unwrap_or(false)
}

/// True when `pack_dir` matches the SL-style pattern `ITL Online <year> Unlocks`
/// (case-insensitive, any 4-digit year).
pub fn is_itl_unlocks_pack(pack_dir: &str) -> bool {
    let trimmed = pack_dir.trim();
    let lower = trimmed.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("itl online ") else {
        return false;
    };
    let Some(year_part) = rest.strip_suffix(" unlocks") else {
        return false;
    };
    year_part.len() == 4 && year_part.chars().all(|c| c.is_ascii_digit())
}

pub fn get_cached_itl_tournament_rank_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<u32> {
    get_cached_player_leaderboard_itl_self_rank_for_side(chart_hash, side)
}

fn online_itl_overall_rank_cache_key_for_side(
    side: profile::PlayerSide,
) -> Option<OnlineItlOverallRankCacheKey> {
    if !profile::is_session_side_joined(side) {
        return None;
    }
    let side_profile = profile::get_for_side(side);
    let api_key = side_profile.groovestats_api_key.trim();
    if api_key.is_empty() {
        return None;
    }

    let profile_id = profile::active_local_profile_id_for_side(side);
    if let Some(profile_id) = profile_id.as_deref() {
        ensure_online_itl_self_score_cache_loaded_for_profile(profile_id);
    }

    let self_score_generation = {
        let _cache = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
        ONLINE_ITL_SELF_SCORE_GENERATION.load(AtomicOrdering::Relaxed)
    };
    let song_cache = get_song_cache();
    let key = OnlineItlOverallRankCacheKey {
        api_key: api_key.to_string(),
        profile_id,
        song_cache_generation: song_cache_generation(),
        self_score_generation,
    };
    drop(song_cache);
    Some(key)
}

fn cached_online_itl_scores_by_chart_for_side(
    side: profile::PlayerSide,
) -> Option<OnlineItlOverallRankInput> {
    if !profile::is_session_side_joined(side) {
        return None;
    }
    let side_profile = profile::get_for_side(side);
    let api_key = side_profile.groovestats_api_key.trim();
    if api_key.is_empty() {
        return None;
    }

    let profile_id = profile::active_local_profile_id_for_side(side);
    if let Some(profile_id) = profile_id.as_deref() {
        ensure_online_itl_self_score_cache_loaded_for_profile(profile_id);
    }

    let cache = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
    let loaded_count = profile_id
        .as_deref()
        .and_then(|profile_id| cache.loaded_profiles.get(profile_id))
        .map_or(0, HashMap::len);
    let mut by_chart = HashMap::with_capacity(loaded_count + cache.session_by_key.len());
    if let Some(profile_id) = profile_id.as_deref()
        && let Some(scores) = cache.loaded_profiles.get(profile_id)
    {
        for (key, score) in scores {
            if key.api_key == api_key {
                by_chart.insert(key.chart_hash.clone(), *score);
            }
        }
    }
    for (key, score) in &cache.session_by_key {
        if key.api_key == api_key {
            by_chart.insert(key.chart_hash.clone(), *score);
        }
    }
    Some(OnlineItlOverallRankInput {
        api_key: api_key.to_string(),
        profile_id,
        self_score_generation: ONLINE_ITL_SELF_SCORE_GENERATION.load(AtomicOrdering::Relaxed),
        by_chart_score: by_chart,
    })
}

fn apply_online_itl_overall_ranks(
    out: &mut HashMap<String, u32>,
    mut by_chart_points: Vec<(String, u32)>,
) {
    by_chart_points.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut prev_points = None;
    let mut prev_rank = 0u32;
    for (idx, (chart_hash, points)) in by_chart_points.into_iter().enumerate() {
        let rank = if prev_points == Some(points) {
            prev_rank
        } else {
            idx.saturating_add(1) as u32
        };
        out.insert(chart_hash, rank);
        prev_points = Some(points);
        prev_rank = rank;
    }
}

fn build_online_itl_overall_ranks(
    song_cache: &[crate::game::song::SongPack],
    by_chart_score: &HashMap<String, u32>,
) -> HashMap<String, u32> {
    if by_chart_score.is_empty() {
        return HashMap::new();
    }

    let mut single_points = Vec::new();
    let mut double_points = Vec::new();
    for pack in song_cache {
        if !group_name_matches(pack.group_name.as_str()) {
            continue;
        }
        for song in &pack.songs {
            for chart in &song.charts {
                if !chart.has_note_data {
                    continue;
                }
                let Some(ex_hundredths) = by_chart_score.get(chart.short_hash.as_str()).copied()
                else {
                    continue;
                };
                let Some(points) = itl_points_for_chart(chart, ex_hundredths) else {
                    continue;
                };
                if itl_steps_type(chart).eq_ignore_ascii_case("double") {
                    double_points.push((chart.short_hash.clone(), points));
                } else {
                    single_points.push((chart.short_hash.clone(), points));
                }
            }
        }
    }

    let mut ranks = HashMap::with_capacity(single_points.len() + double_points.len());
    apply_online_itl_overall_ranks(&mut ranks, single_points);
    apply_online_itl_overall_ranks(&mut ranks, double_points);
    ranks
}

pub fn get_cached_itl_tournament_overall_ranks_for_side(
    side: profile::PlayerSide,
) -> Arc<HashMap<String, u32>> {
    let Some(cache_key) = online_itl_overall_rank_cache_key_for_side(side) else {
        return EMPTY_ONLINE_ITL_OVERALL_RANKS.clone();
    };
    {
        let cache = ONLINE_ITL_OVERALL_RANK_CACHE.lock().unwrap();
        if let Some(entry) = online_itl_overall_rank_entry_for_side(&cache, side)
            && entry.key == cache_key
        {
            return entry.ranks.clone();
        }
    }

    let Some(input) = cached_online_itl_scores_by_chart_for_side(side) else {
        return EMPTY_ONLINE_ITL_OVERALL_RANKS.clone();
    };
    let song_cache = get_song_cache();
    let key = OnlineItlOverallRankCacheKey {
        api_key: input.api_key,
        profile_id: input.profile_id,
        song_cache_generation: song_cache_generation(),
        self_score_generation: input.self_score_generation,
    };
    let ranks = Arc::new(build_online_itl_overall_ranks(
        song_cache.as_slice(),
        &input.by_chart_score,
    ));
    drop(song_cache);

    let mut cache = ONLINE_ITL_OVERALL_RANK_CACHE.lock().unwrap();
    *online_itl_overall_rank_entry_for_side_mut(&mut cache, side) =
        Some(OnlineItlOverallRankCacheEntry {
            key,
            ranks: ranks.clone(),
        });
    ranks
}

pub fn save_itl_data_from_gameplay(
    gs: &gameplay::State,
) -> [Option<ItlEventProgress>; gameplay::MAX_PLAYERS] {
    let mut progress: [Option<ItlEventProgress>; gameplay::MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    if gs.autoplay_used {
        debug!("Skipping ITL save: autoplay or replay was used during this stage.");
        return progress;
    }

    for (player_idx, chart) in gs
        .charts
        .iter()
        .enumerate()
        .take(gs.num_players.min(gameplay::MAX_PLAYERS))
    {
        let side = gameplay_side_for_player(gs, player_idx);
        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        let chart_hash = chart.short_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }

        let mut data = read_itl_file(profile_id.as_str());
        itl_rebuild_song_ranks(&mut data);
        let eval = itl_eval_state(gs, player_idx, &data);
        if !eval.active {
            continue;
        }
        if !eval.eligible {
            debug!(
                "Skipping ITL save for {:?} ({}): {}",
                side,
                chart_hash,
                eval.reason_lines.join("; ")
            );
            continue;
        }
        let prev_totals = itl_point_totals(&data);

        let Some(song_dir) = itl_song_dir(gs.song.as_ref()) else {
            continue;
        };
        let path_changed = data
            .path_map
            .get(song_dir.as_str())
            .is_none_or(|hash| !hash.eq_ignore_ascii_case(chart_hash));
        if path_changed {
            data.path_map
                .insert(song_dir.clone(), chart_hash.to_string());
        }

        let prev = data.hash_map.get(chart_hash).cloned();
        let (passing_points, max_scoring_points) =
            parse_itl_points(gs.charts[player_idx].chart_name.as_str())
                .or_else(|| {
                    prev.as_ref()
                        .map(|entry| (entry.passing_points, entry.max_scoring_points))
                })
                .unwrap_or((0, 0));
        let max_points = passing_points.saturating_add(max_scoring_points);
        let judgments = itl_judgments_from_gameplay(gs, player_idx);
        let (start, end) = gs.note_ranges[player_idx];
        let ex_percent = judgment::calculate_ex_score_from_notes(
            &gs.notes[start..end],
            &gs.note_time_cache_ns[start..end],
            &gs.hold_end_time_cache_ns[start..end],
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            gs.players[player_idx]
                .fail_time
                .map(gameplay::song_time_ns_from_seconds),
            false,
        );
        let current_run_ex = ex_hundredths(ex_percent);
        let new_entry = ItlHashEntry {
            judgments: judgments.clone(),
            ex: current_run_ex,
            clear_type: itl_clear_type(&judgments),
            points: itl_points_for_song(passing_points, max_scoring_points, ex_percent),
            used_cmod: eval.used_cmod,
            date: Local::now().format("%Y-%m-%d").to_string(),
            no_cmod: eval.chart_no_cmod,
            passing_points,
            max_scoring_points,
            max_points,
            rank: None,
            steps_type: itl_steps_type(gs.charts[player_idx].as_ref()).to_string(),
            passes: prev
                .as_ref()
                .map_or(1, |entry| entry.passes.saturating_add(1)),
        };

        let mut needs_write = path_changed;
        let mut best_changed = false;
        match data.hash_map.get_mut(chart_hash) {
            None => {
                data.hash_map
                    .insert(chart_hash.to_string(), new_entry.clone());
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
                } else if ex_tied && itl_judgments_better(&new_entry.judgments, &existing.judgments)
                {
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

        itl_rebuild_song_ranks(&mut data);
        let current_totals = itl_point_totals(&data);
        let current_entry = data
            .hash_map
            .get(chart_hash)
            .cloned()
            .unwrap_or(new_entry.clone());
        let prev_entry = prev.unwrap_or_default();
        let mut event_progress = ItlEventProgress {
            name: itl_event_name(gs.song.as_ref()),
            is_doubles: current_entry.steps_type.eq_ignore_ascii_case("double"),
            score_hundredths: current_run_ex,
            score_delta_hundredths: delta_i32(current_run_ex, prev_entry.ex),
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
            overlay_pages: Vec::new(),
        };
        event_progress.overlay_pages = overlay_pages(&event_progress, None, &[]);
        progress[player_idx] = Some(event_progress);

        if needs_write {
            write_itl_file(profile_id.as_str(), &data);
            set_cached_itl_file(profile_id.as_str(), data);
        }
    }

    progress
}

pub(super) fn current_score_hundredths(gs: &gameplay::State, player_idx: usize) -> u32 {
    let (start, end) = gs.note_ranges[player_idx];
    let ex_percent = judgment::calculate_ex_score_from_notes(
        &gs.notes[start..end],
        &gs.note_time_cache_ns[start..end],
        &gs.hold_end_time_cache_ns[start..end],
        gs.total_steps[player_idx],
        gs.holds_total[player_idx],
        gs.rolls_total[player_idx],
        gs.mines_total[player_idx],
        gs.players[player_idx]
            .fail_time
            .map(gameplay::song_time_ns_from_seconds),
        false,
    );
    ex_hundredths(ex_percent)
}

fn itl_file_path(profile_id: &str) -> PathBuf {
    profile::local_profile_dir_for_id(profile_id).join(ITL_FILE_NAME)
}

fn ensure_itl_score_cache_loaded(profile_id: &str) {
    let needs_load = {
        let state = ITL_SCORE_CACHE.lock().unwrap();
        !state.loaded_profiles.contains_key(profile_id)
    };
    if !needs_load {
        return;
    }

    let mut data = read_itl_file(profile_id);
    itl_rebuild_song_ranks(&mut data);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(data);
}

fn set_cached_itl_file(profile_id: &str, data: ItlFileData) {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .insert(profile_id.to_string(), data);
}

fn deserialize_itl_ex<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<f64>::deserialize(deserializer)?.unwrap_or(0.0);
    if !raw.is_finite() || raw <= 0.0 {
        return Ok(0);
    }
    let scaled = if raw <= 100.0001 { raw * 100.0 } else { raw };
    Ok(scaled.round().clamp(0.0, 10_000.0) as u32)
}

#[inline(always)]
fn ex_hundredths(ex_percent: f64) -> u32 {
    let ex = if ex_percent.is_finite() {
        ex_percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    (ex * 100.0).round() as u32
}

fn read_itl_file(profile_id: &str) -> ItlFileData {
    let path = itl_file_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return ItlFileData::default();
    };
    serde_json::from_str(text.as_str()).unwrap_or_else(|error| {
        warn!("Failed to parse ITL data file {path:?}: {error}");
        ItlFileData::default()
    })
}

fn write_itl_file(profile_id: &str, data: &ItlFileData) {
    if data.path_map.is_empty() && data.hash_map.is_empty() && data.unlock_folders.is_empty() {
        return;
    }
    let path = itl_file_path(profile_id);
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(error) = fs::create_dir_all(parent) {
        warn!("Failed to create ITL profile dir {parent:?}: {error}");
        return;
    }
    let Ok(text) = serde_json::to_string(data) else {
        warn!("Failed to encode ITL data for profile {profile_id}");
        return;
    };
    let tmp = path.with_extension("tmp");
    if let Err(error) = fs::write(&tmp, text) {
        warn!("Failed to write ITL temp file {tmp:?}: {error}");
        return;
    }
    if let Err(error) = fs::rename(&tmp, &path) {
        warn!("Failed to commit ITL file {path:?}: {error}");
        let _ = fs::remove_file(&tmp);
    }
}

fn update_unlock_folders(profile_id: &str, folders: &[String]) {
    if folders.is_empty() {
        return;
    }
    let mut data = read_itl_file(profile_id);
    let mut changed = false;
    for folder in folders {
        let folder = folder.trim();
        if folder.is_empty() {
            continue;
        }
        changed |= data.unlock_folders.insert(folder.to_string(), true) != Some(true);
    }
    if changed {
        write_itl_file(profile_id, &data);
        set_cached_itl_file(profile_id, data);
    }
}

fn event_name_or_unknown(name: &str) -> &str {
    if name.trim().is_empty() {
        "Unknown Event"
    } else {
        name.trim()
    }
}

#[inline(always)]
fn clear_type_name(clear_type: u8) -> &'static str {
    match clear_type {
        0 => "No Play",
        1 => "Clear",
        2 => "FC",
        3 => "FEC",
        4 => "FFC",
        5 => "FBFC",
        _ => "Clear",
    }
}

fn trim_blank_lines(text: String) -> String {
    text.trim_end_matches(['\n', '\r']).to_string()
}

fn capitalize_ascii_first(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.extend(chars);
    out
}

fn stat_improvement_lines(progress: Option<&GrooveStatsSubmitApiProgress>) -> Vec<String> {
    let Some(progress) = progress else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 {
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("clearType") {
            let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
            let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
            lines.push(format!(
                "Clear Type: {} >>> {}",
                clear_type_name(before),
                clear_type_name(after)
            ));
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("grade") {
            let curr = improvement.current;
            let prev = curr - improvement.gained as i32;
            if curr != 0 && prev != curr {
                let grade = match curr {
                    1 => Some("Quad"),
                    2 => Some("Quint"),
                    _ => None,
                };
                if let Some(grade) = grade {
                    lines.push(format!("New {grade}!"));
                }
            }
            continue;
        }
        let stat_name = capitalize_ascii_first(improvement.name.trim_end_matches("Level"));
        lines.push(format!(
            "{stat_name} Lvl: {} (+{})",
            improvement.current, improvement.gained
        ));
    }
    lines
}

fn summary_page_text(
    progress: &ItlEventProgress,
    submit_progress: Option<&GrooveStatsSubmitApiProgress>,
) -> String {
    let mut text = format!(
        "EX Score: {:.2}% ({:+.2}%)\n\
         Points: {} ({:+})\n\n\
         Ranking Points: {} ({:+})\n\
         Song Points: {} ({:+})\n\
         EX Points: {} ({:+})\n\
         Total Points: {} ({:+})\n\n\
         You've passed the chart {} times",
        progress.score_hundredths as f64 / 100.0,
        progress.score_delta_hundredths as f64 / 100.0,
        progress.current_points,
        progress.point_delta,
        progress.current_ranking_points,
        progress.ranking_delta,
        progress.current_song_points,
        progress.song_delta,
        progress.current_ex_points,
        progress.ex_delta,
        progress.current_total_points,
        progress.total_delta,
        progress.total_passes,
    );
    let lines = stat_improvement_lines(submit_progress);
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(lines.join("\n").as_str());
    }
    trim_blank_lines(text)
}

fn append_grouped_reward_text(out: &mut String, reward_type: &str, descriptions: &[String]) {
    if descriptions.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    if !reward_type.eq_ignore_ascii_case("ad-hoc") {
        out.push_str(reward_type.trim().to_ascii_uppercase().as_str());
        out.push_str(":\n");
    }
    out.push_str(descriptions.join("\n").as_str());
}

fn quest_page_text(quest: &super::GrooveStatsSubmitApiQuest) -> String {
    let mut body = format!("Completed \"{}\"!", quest.title.trim());
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for reward in &quest.rewards {
        let reward_type = reward.reward_type.trim();
        let description = reward.description.trim();
        if description.is_empty() {
            continue;
        }
        if let Some((_, descriptions)) = grouped
            .iter_mut()
            .find(|(kind, _)| kind.eq_ignore_ascii_case(reward_type))
        {
            descriptions.push(description.to_string());
        } else {
            grouped.push((reward_type.to_string(), vec![description.to_string()]));
        }
    }
    for (reward_type, descriptions) in &grouped {
        append_grouped_reward_text(&mut body, reward_type.as_str(), descriptions.as_slice());
    }
    trim_blank_lines(body)
}

fn achievement_page_text(achievement: &GrooveStatsSubmitApiAchievement) -> String {
    let mut lines = vec![format!(
        "Completed the \"{}\" Achievement!",
        achievement.title.trim()
    )];
    for reward in &achievement.rewards {
        let tier = reward.tier.trim();
        if !tier.is_empty() && tier != "0" {
            lines.push(format!("Tier {tier}"));
        }
        for requirement in &reward.requirements {
            let requirement = requirement.trim();
            if !requirement.is_empty() {
                lines.push(requirement.to_string());
            }
        }
        let title = reward.title_unlocked.trim();
        if !title.is_empty() {
            lines.push(format!("Unlocked the \"{}\" Title!", title));
        }
        lines.push(String::new());
    }
    trim_blank_lines(lines.join("\n"))
}

fn overlay_pages(
    progress: &ItlEventProgress,
    submit_progress: Option<&GrooveStatsSubmitApiProgress>,
    submit_leaderboard: &[LeaderboardApiEntry],
) -> Vec<ItlOverlayPage> {
    let mut pages = vec![ItlOverlayPage::Text(summary_page_text(
        progress,
        submit_progress,
    ))];
    let Some(submit_progress) = submit_progress else {
        pages.push(ItlOverlayPage::Leaderboard(leaderboard_entries_from_api(
            submit_leaderboard.to_vec(),
        )));
        return pages;
    };
    for quest in &submit_progress.quests_completed {
        pages.push(ItlOverlayPage::Text(quest_page_text(quest)));
    }
    for achievement in &submit_progress.achievements_completed {
        pages.push(ItlOverlayPage::Text(achievement_page_text(achievement)));
    }
    pages.push(ItlOverlayPage::Leaderboard(leaderboard_entries_from_api(
        submit_leaderboard.to_vec(),
    )));
    pages
}

fn handle_submit_event_unlocks(
    player: &GrooveStatsSubmitPlayerJob,
    event: &GrooveStatsSubmitApiEvent,
) {
    let cfg = crate::config::get();
    if !cfg.auto_download_unlocks {
        return;
    }
    let Some(progress) = event.progress.as_ref() else {
        return;
    };
    let event_name = event_name_or_unknown(event.name.as_str());
    let profile_name = if player.profile_name.trim().is_empty() {
        "NoName"
    } else {
        player.profile_name.trim()
    };

    for quest in &progress.quests_completed {
        let url = quest.song_download_url.trim();
        if url.is_empty() {
            continue;
        }
        let title = quest.title.trim();
        let (download_name, pack_name) = if cfg.separate_unlocks_by_player {
            (
                format!("[{event_name}] {title} - {profile_name}"),
                format!("{event_name} Unlocks - {profile_name}"),
            )
        } else {
            (
                format!("[{event_name}] {title}"),
                format!("{event_name} Unlocks"),
            )
        };
        downloads::queue_event_unlock_download(url, download_name.trim_end(), pack_name.as_str());
    }
}

pub(super) fn handle_submit_player_unlocks(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) {
    if let Some(itl) = response.itl.as_ref()
        && let Some(profile_id) = player.profile_id.as_deref()
        && let Some(progress) = itl.progress.as_ref()
    {
        for quest in &progress.quests_completed {
            update_unlock_folders(profile_id, quest.song_download_folders.as_slice());
        }
    }
    if let Some(rpg) = response.rpg.as_ref() {
        handle_submit_event_unlocks(player, rpg);
    }
    if let Some(itl) = response.itl.as_ref() {
        handle_submit_event_unlocks(player, itl);
    }
}

fn clear_type_change(progress: Option<&GrooveStatsSubmitApiProgress>) -> (Option<u8>, Option<u8>) {
    let Some(progress) = progress else {
        return (None, None);
    };
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 || !improvement.name.eq_ignore_ascii_case("clearType") {
            continue;
        }
        let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
        let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
        return (Some(before), Some(after));
    }
    (None, None)
}

pub(super) fn progress_from_submit(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> Option<ItlEventProgress> {
    let itl = response.itl.as_ref()?;
    let score_hundredths = player.itl_score_hundredths?;
    let (clear_type_before, clear_type_after) = clear_type_change(itl.progress.as_ref());
    let mut progress = ItlEventProgress {
        name: event_name_or_unknown(itl.name.as_str()).to_string(),
        is_doubles: itl.is_doubles,
        score_hundredths,
        score_delta_hundredths: itl.score_delta,
        current_points: itl.top_score_points,
        point_delta: delta_i32(itl.top_score_points, itl.prev_top_score_points),
        current_ranking_points: itl.current_ranking_point_total,
        ranking_delta: delta_i32(
            itl.current_ranking_point_total,
            itl.previous_ranking_point_total,
        ),
        current_song_points: itl.current_song_point_total,
        song_delta: delta_i32(itl.current_song_point_total, itl.previous_song_point_total),
        current_ex_points: itl.current_ex_point_total,
        ex_delta: delta_i32(itl.current_ex_point_total, itl.previous_ex_point_total),
        current_total_points: itl.current_point_total,
        total_delta: delta_i32(itl.current_point_total, itl.previous_point_total),
        total_passes: itl.total_passes,
        clear_type_before,
        clear_type_after,
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages = overlay_pages(
        &progress,
        itl.progress.as_ref(),
        itl.itl_leaderboard.as_slice(),
    );
    Some(progress)
}

#[inline(always)]
fn itl_score_from_entry(entry: &ItlHashEntry) -> CachedItlScore {
    CachedItlScore {
        ex_hundredths: entry.ex,
        clear_type: entry.clear_type,
        points: entry.points,
    }
}

fn itl_score_for_song(
    song: &crate::game::song::SongData,
    data: &ItlFileData,
) -> Option<CachedItlScore> {
    itl_entry_for_song(song, data).map(itl_score_from_entry)
}

fn itl_entry_for_song<'a>(
    song: &crate::game::song::SongData,
    data: &'a ItlFileData,
) -> Option<&'a ItlHashEntry> {
    let song_dir = itl_song_dir(song)?;
    let chart_hash = data.path_map.get(song_dir.as_str())?;
    data.hash_map.get(chart_hash)
}

fn itl_song_dir(song: &crate::game::song::SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
}

fn itl_group_name(song: &crate::game::song::SongData) -> Option<String> {
    let song_cache = get_song_cache();
    for pack in song_cache.iter() {
        if pack
            .songs
            .iter()
            .any(|candidate| candidate.simfile_path == song.simfile_path)
        {
            return Some(pack.group_name.clone());
        }
    }
    None
}

#[inline(always)]
fn group_name_matches(group_name: &str) -> bool {
    let group = group_name.to_ascii_lowercase();
    group.contains("itl online 2026") || group.contains("itl 2026")
}

fn itl_is_song(
    song: &crate::game::song::SongData,
    song_dir: Option<&str>,
    data: &ItlFileData,
) -> bool {
    let song_dir_known = song_dir.is_some_and(|dir| data.path_map.contains_key(dir));
    if song_dir_known {
        return true;
    }
    let Some(group_name) = itl_group_name(song) else {
        return false;
    };
    group_name_matches(group_name.as_str())
}

fn chart_no_cmod(song: &crate::game::song::SongData, prev: Option<&ItlHashEntry>) -> bool {
    prev.map_or_else(
        || {
            song.display_subtitle(false)
                .to_ascii_lowercase()
                .contains("no cmod")
        },
        |data| data.no_cmod,
    )
}

#[inline(always)]
fn itl_event_name(song: &crate::game::song::SongData) -> String {
    itl_group_name(song).unwrap_or_else(|| "ITL Online 2026".to_string())
}

#[inline(always)]
fn itl_steps_type(chart: &crate::game::chart::ChartData) -> &'static str {
    if chart.chart_type.to_ascii_lowercase().contains("double") {
        "double"
    } else {
        "single"
    }
}

#[inline(always)]
fn rank_for_points(sorted_points: &[u32], points: u32) -> Option<u32> {
    sorted_points
        .iter()
        .position(|value| *value == points)
        .map(|idx| idx.saturating_add(1) as u32)
}

fn itl_rebuild_song_ranks(data: &mut ItlFileData) {
    let mut points: Vec<u32> = data.hash_map.values().map(|entry| entry.points).collect();
    points.sort_unstable_by(|a, b| b.cmp(a));

    let mut points_single = Vec::with_capacity(points.len());
    let mut points_double = Vec::with_capacity(points.len());
    let mut unknown_points = Vec::new();
    let mut plays_single = 0usize;
    let mut plays_double = 0usize;

    for entry in data.hash_map.values_mut() {
        entry.rank = rank_for_points(points.as_slice(), entry.points);
        if entry.steps_type.eq_ignore_ascii_case("single") {
            points_single.push(entry.points);
            plays_single = plays_single.saturating_add(1);
        } else if entry.steps_type.eq_ignore_ascii_case("double") {
            points_double.push(entry.points);
            plays_double = plays_double.saturating_add(1);
        } else {
            unknown_points.push(entry.points);
        }
    }

    if plays_single > plays_double {
        points_single.extend(unknown_points);
    } else {
        points_double.extend(unknown_points);
    }

    points_single.sort_unstable_by(|a, b| b.cmp(a));
    points_double.sort_unstable_by(|a, b| b.cmp(a));

    for entry in data.hash_map.values_mut() {
        if entry.steps_type.eq_ignore_ascii_case("single") {
            entry.rank = rank_for_points(points_single.as_slice(), entry.points);
        } else if entry.steps_type.eq_ignore_ascii_case("double") {
            entry.rank = rank_for_points(points_double.as_slice(), entry.points);
        }
    }

    data.points = points;
    data.points_single = points_single;
    data.points_double = points_double;
}

fn itl_point_totals(data: &ItlFileData) -> ItlPointTotals {
    let ranking_points = data.points.iter().take(75).copied().sum();
    let mut song_points = 0u32;
    let mut ex_points = 0u32;
    let mut total_points = 0u32;
    for entry in data.hash_map.values() {
        song_points = song_points.saturating_add(entry.passing_points);
        ex_points = ex_points.saturating_add(entry.points.saturating_sub(entry.passing_points));
        total_points = total_points.saturating_add(entry.points);
    }
    ItlPointTotals {
        ranking_points,
        song_points,
        ex_points,
        total_points,
    }
}

#[inline(always)]
fn delta_i32(current: u32, previous: u32) -> i32 {
    (i64::from(current) - i64::from(previous)).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
        as i32
}

fn loaded_chart_no_cmod_for_gameplay(
    gs: &gameplay::State,
    player_idx: usize,
    profile_id: &str,
) -> Option<bool> {
    let song_dir = itl_song_dir(gs.song.as_ref())?;
    let state = ITL_SCORE_CACHE.lock().unwrap();
    let data = state.loaded_profiles.get(profile_id)?;
    if !itl_is_song(gs.song.as_ref(), Some(song_dir.as_str()), data) {
        return Some(false);
    }
    let prev = data.hash_map.get(gs.charts[player_idx].short_hash.as_str());
    Some(chart_no_cmod(gs.song.as_ref(), prev))
}

pub fn should_warn_cmod_for_itl_chart(gs: &gameplay::State, player_idx: usize) -> bool {
    if player_idx >= gs.num_players.min(gameplay::MAX_PLAYERS)
        || gs.course_display_totals.is_some()
        || !matches!(
            gs.player_profiles[player_idx].scroll_speed,
            crate::game::scroll::ScrollSpeedSetting::CMod(_)
        )
    {
        return false;
    }

    let side = gameplay_side_for_player(gs, player_idx);
    if let Some(profile_id) = profile::active_local_profile_id_for_side(side)
        && let Some(no_cmod) =
            loaded_chart_no_cmod_for_gameplay(gs, player_idx, profile_id.as_str())
    {
        return no_cmod;
    }

    let Some(group_name) = itl_group_name(gs.song.as_ref()) else {
        return false;
    };
    group_name_matches(group_name.as_str()) && chart_no_cmod(gs.song.as_ref(), None)
}

fn parse_itl_points(chart_name: &str) -> Option<(u32, u32)> {
    let mut nums = chart_name
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok());
    Some((nums.next()?, nums.next()?))
}

pub fn itl_points_for_chart(
    chart: &crate::game::chart::ChartData,
    ex_hundredths: u32,
) -> Option<u32> {
    let (passing_points, max_scoring_points) = parse_itl_points(chart.chart_name.as_str())?;
    Some(itl_points_for_song(
        passing_points,
        max_scoring_points,
        f64::from(ex_hundredths) / 100.0,
    ))
}

fn itl_points_for_song(passing_points: u32, max_scoring_points: u32, ex_score: f64) -> u32 {
    let scalar = 40.0_f64;
    let curve = (scalar.powf(ex_score.max(0.0) / scalar) - 1.0)
        * (100.0 / (scalar.powf(100.0 / scalar) - 1.0));
    let percent = ((curve / 100.0) * 1_000_000.0).round() / 1_000_000.0;
    passing_points.saturating_add((f64::from(max_scoring_points) * percent).floor() as u32)
}

fn itl_judgments_better(cur: &ItlJudgments, prev: &ItlJudgments) -> bool {
    for (cur_value, prev_value) in [
        (cur.w0, prev.w0),
        (cur.w1, prev.w1),
        (cur.w2, prev.w2),
        (cur.w3, prev.w3),
        (cur.w4, prev.w4),
        (cur.w5, prev.w5),
        (cur.miss, prev.miss),
    ] {
        match cur_value.cmp(&prev_value) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
    }
    false
}

fn itl_clear_type(judgments: &ItlJudgments) -> u8 {
    if judgments.total_rolls.saturating_sub(judgments.rolls) > 0
        || judgments.total_holds.saturating_sub(judgments.holds) > 0
    {
        return 1;
    }

    let mut clear_type = 1;
    let mut taps = judgments
        .miss
        .saturating_add(judgments.w5)
        .saturating_add(judgments.w4);
    if taps == 0 {
        clear_type = 2;
    }
    taps = taps.saturating_add(judgments.w3);
    if taps == 0 {
        clear_type = 3;
    }
    taps = taps.saturating_add(judgments.w2);
    if taps == 0 {
        clear_type = 4;
    }
    taps = taps.saturating_add(judgments.w1);
    if taps == 0 {
        clear_type = 5;
    }
    clear_type
}

fn itl_judgments_from_gameplay(gs: &gameplay::State, player_idx: usize) -> ItlJudgments {
    let counts = groovestats_judgment_counts(gs, player_idx);
    ItlJudgments {
        w0: counts.fantastic_plus,
        w1: counts.fantastic,
        w2: counts.excellent,
        w3: counts.great,
        w4: counts.decent,
        w5: counts.way_off,
        miss: counts.miss,
        total_steps: counts.total_steps,
        holds: counts.holds_held,
        total_holds: counts.total_holds,
        mines: counts.mines_hit,
        total_mines: counts.total_mines,
        rolls: counts.rolls_held,
        total_rolls: counts.total_rolls,
    }
}

fn itl_eval_state(gs: &gameplay::State, player_idx: usize, data: &ItlFileData) -> ItlEvalState {
    let used_cmod = matches!(
        gs.player_profiles[player_idx].scroll_speed,
        crate::game::scroll::ScrollSpeedSetting::CMod(_)
    );
    let Some(song_dir) = itl_song_dir(gs.song.as_ref()) else {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    };
    if !itl_is_song(gs.song.as_ref(), Some(song_dir.as_str()), data) {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    }

    let chart_hash = gs.charts[player_idx].short_hash.as_str();
    let prev = data.hash_map.get(chart_hash);
    let chart_no_cmod = chart_no_cmod(gs.song.as_ref(), prev);
    let gs_valid = groovestats_eval_state_from_gameplay(gs, player_idx);
    let rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        gs.music_rate
    } else {
        1.0
    };
    let remove_mask = gs.player_profiles[player_idx].remove_active_mask.bits();
    let mines_enabled = (remove_mask & (1u8 << 1)) == 0;
    let passed = gameplay_run_passed(
        gs.song_completed_naturally,
        gs.players[player_idx].is_failing,
        gs.players[player_idx].life,
        gs.players[player_idx].fail_time.is_some(),
    );

    let mut reason_lines = Vec::with_capacity(4);
    if !gs_valid.valid {
        if gs_valid.reason_lines.is_empty() {
            reason_lines.push("Score is not valid for GrooveStats.".to_string());
        } else {
            reason_lines.extend(gs_valid.reason_lines);
        }
    }
    if (rate - 1.0).abs() > 0.0001 {
        reason_lines.push("ITL requires 1.00x music rate.".to_string());
    }
    if !mines_enabled {
        reason_lines.push("ITL requires mines to be enabled.".to_string());
    }
    if !passed {
        reason_lines.push("ITL only saves passing scores.".to_string());
    }
    if chart_no_cmod && used_cmod {
        reason_lines.push("This ITL chart does not allow CMod.".to_string());
    }

    ItlEvalState {
        active: true,
        eligible: reason_lines.is_empty(),
        chart_no_cmod,
        used_cmod,
        reason_lines,
    }
}

pub fn itl_eval_state_from_gameplay(gs: &gameplay::State, player_idx: usize) -> ItlEvalState {
    if player_idx >= gs.num_players.min(gameplay::MAX_PLAYERS) {
        return ItlEvalState::default();
    }
    let side = gameplay_side_for_player(gs, player_idx);
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return ItlEvalState::default();
    };
    let data = read_itl_file(profile_id.as_str());
    itl_eval_state(gs, player_idx, &data)
}

pub fn get_cached_itl_self_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<u32> {
    let key = online_itl_self_score_key_for_side(chart_hash, side)?;
    let profile_id = profile::active_local_profile_id_for_side(side);
    if let Some(profile_id) = profile_id.as_deref() {
        ensure_online_itl_self_score_cache_loaded_for_profile(profile_id);
    }
    let cache = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
    profile_id
        .as_deref()
        .and_then(|profile_id| cache.loaded_profiles.get(profile_id))
        .and_then(|scores| scores.get(&key).copied())
        .or_else(|| cache.session_by_key.get(&key).copied())
}

pub fn get_or_fetch_itl_self_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<u32> {
    if let Some(score) = get_cached_itl_self_score_for_side(chart_hash, side) {
        return Some(score);
    }
    // Keep the wheel's ITL prefetch aligned with the Select Music scorebox cache width.
    // Smaller requests seed the shared leaderboard cache with partial panes, so the
    // scorebox briefly renders a truncated list before refetching the remaining rows.
    let _ = get_or_fetch_player_leaderboards_for_side_inner(
        chart_hash,
        side,
        ITL_WHEEL_FETCH_ENTRIES,
        false,
    )?;
    get_cached_itl_self_score_for_side(chart_hash, side)
}

pub fn get_or_fetch_itl_tournament_rank_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<u32> {
    if let Some(rank) = get_cached_itl_tournament_rank_for_side(chart_hash, side) {
        return Some(rank);
    }
    let _ = get_or_fetch_player_leaderboards_for_side_inner(
        chart_hash,
        side,
        ITL_WHEEL_FETCH_ENTRIES,
        false,
    )?;
    get_cached_itl_tournament_rank_for_side(chart_hash, side)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::chart::{ChartData, StaminaCounts};
    use crate::game::song::SongData;
    use rssp::{TechCounts, stats::ArrowStats};
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(1);

    fn sample_chart(chart_type: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: "deadbeefcafebabe".to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 12,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 12,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let id = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("deadsync-{name}-{}-{id}", std::process::id()))
    }

    fn sample_song(dir: &str) -> SongData {
        SongData {
            simfile_path: PathBuf::from(dir).join("song.ssc"),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }

    #[test]
    fn parse_itl_points_reads_chart_name_values() {
        assert_eq!(
            parse_itl_points("7500 (P) + 12000 (S)"),
            Some((7500, 12000))
        );
        assert_eq!(parse_itl_points("No points here"), None);
    }

    #[test]
    fn itl_points_for_chart_uses_chart_name_curve() {
        let mut chart = sample_chart("dance-single");
        chart.chart_name = "7500 (P) + 12000 (S)".to_string();

        assert_eq!(itl_points_for_chart(&chart, 10_000), Some(19_500));
    }

    #[test]
    fn itl_points_curve_keeps_full_ex_exact() {
        assert_eq!(itl_points_for_song(7500, 12000, 100.0), 19_500);
    }

    #[test]
    fn itl_judgments_compare_from_top_window() {
        let prev = ItlJudgments {
            w0: 10,
            w1: 20,
            ..ItlJudgments::default()
        };
        let better = ItlJudgments {
            w0: 11,
            w1: 19,
            ..ItlJudgments::default()
        };
        let worse = ItlJudgments {
            w0: 9,
            w1: 25,
            ..ItlJudgments::default()
        };

        assert!(itl_judgments_better(&better, &prev));
        assert!(!itl_judgments_better(&worse, &prev));
    }

    #[test]
    fn itl_file_reads_simply_love_and_legacy_ex_values() {
        let sl: ItlFileData = serde_json::from_value(json!({
            "hashMap": {
                "sl": { "ex": 9437 }
            }
        }))
        .unwrap();
        let legacy: ItlFileData = serde_json::from_value(json!({
            "hashMap": {
                "legacy": { "ex": 94.37 }
            }
        }))
        .unwrap();

        assert_eq!(sl.hash_map["sl"].ex, 9437);
        assert_eq!(legacy.hash_map["legacy"].ex, 9437);
    }

    #[test]
    fn online_itl_self_score_index_round_trips() {
        let dir = temp_test_dir("itl-self-score");
        let path = dir.join("itl_self.bin");
        let key = OnlineItlSelfScoreKey {
            chart_hash: "deadbeefcafebabe".to_string(),
            api_key: "api-key".to_string(),
        };
        let mut expected = HashMap::new();
        expected.insert(key, 9912);

        save_online_itl_self_score_index(&path, &expected);

        assert_eq!(load_online_itl_self_score_index(&path), Some(expected));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn online_itl_overall_ranks_share_tied_points() {
        let mut ranks = HashMap::new();
        apply_online_itl_overall_ranks(
            &mut ranks,
            vec![
                ("a".to_string(), 19_500),
                ("b".to_string(), 19_500),
                ("c".to_string(), 18_000),
            ],
        );

        assert_eq!(ranks.get("a"), Some(&1));
        assert_eq!(ranks.get("b"), Some(&1));
        assert_eq!(ranks.get("c"), Some(&3));
    }

    #[test]
    fn itl_score_lookup_uses_song_path_map() {
        let song = sample_song("/Songs/ITL Online 2026/Example");
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/ITL Online 2026/Example".to_string(),
            "deadbeefcafebabe".to_string(),
        );
        data.hash_map.insert(
            "deadbeefcafebabe".to_string(),
            ItlHashEntry {
                ex: 9754,
                clear_type: 4,
                points: 12345,
                ..ItlHashEntry::default()
            },
        );

        assert_eq!(
            itl_score_for_song(&song, &data),
            Some(CachedItlScore {
                ex_hundredths: 9754,
                clear_type: 4,
                points: 12345,
            })
        );
    }

    #[test]
    fn itl_chart_no_cmod_uses_subtitle_fallback() {
        let mut song = sample_song("/Songs/ITL Online 2026/Example");
        song.subtitle = "(NO CMOD)".to_string();

        assert!(chart_no_cmod(&song, None));
    }

    #[test]
    fn itl_is_song_accepts_cached_path_map_without_group_lookup() {
        let song = sample_song("/Songs/Custom Pack/Example");
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/Custom Pack/Example".to_string(),
            "deadbeefcafebabe".to_string(),
        );

        assert!(itl_is_song(
            &song,
            Some("/Songs/Custom Pack/Example"),
            &data
        ));
    }

    #[test]
    fn itl_totals_split_song_and_ex_points() {
        let mut data = ItlFileData::default();
        data.hash_map.insert(
            "a".to_string(),
            ItlHashEntry {
                points: 100,
                passing_points: 60,
                steps_type: "single".to_string(),
                ..ItlHashEntry::default()
            },
        );
        data.hash_map.insert(
            "b".to_string(),
            ItlHashEntry {
                points: 50,
                passing_points: 20,
                steps_type: "double".to_string(),
                ..ItlHashEntry::default()
            },
        );

        itl_rebuild_song_ranks(&mut data);

        assert_eq!(data.points, vec![100, 50]);
        assert_eq!(data.hash_map["a"].rank, Some(1));
        assert_eq!(data.hash_map["b"].rank, Some(1));
        assert_eq!(
            itl_point_totals(&data),
            ItlPointTotals {
                ranking_points: 150,
                song_points: 80,
                ex_points: 70,
                total_points: 150,
            }
        );
    }

    #[test]
    fn itl_run_passed_rejects_failed_runs() {
        assert!(gameplay_run_passed(true, false, 1.0, false));
        assert!(!gameplay_run_passed(false, false, 1.0, false));
        assert!(!gameplay_run_passed(true, true, 1.0, false));
        assert!(!gameplay_run_passed(true, false, 1.0, true));
        assert!(!gameplay_run_passed(true, false, 0.0, false));
    }
}
