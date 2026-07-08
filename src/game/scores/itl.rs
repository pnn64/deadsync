use super::{
    GameplayCoreState, GrooveStatsSubmitPlayerJob, gameplay_run_passed, gameplay_side_for_player,
    get_cached_player_leaderboard_itl_self_rank_for_side,
    get_or_fetch_player_leaderboards_for_side, groovestats_eval_state_from_gameplay,
    groovestats_judgment_counts,
};

use crate::game::online::downloads;
use crate::game::profile;
use crate::game::song::{get_song_cache, song_cache_generation};
use chrono::Local;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::groovestats::{
    GrooveStatsSubmitApiEvent, GrooveStatsSubmitApiPlayer, submit_event_progress_from_api,
};
use deadsync_profile as profile_data;
use log::{debug, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use deadsync_rules::{judgment, scroll::ScrollSpeedSetting};
use deadsync_score::{
    CachedItlScore, ItlChartSaveInput, ItlEvalInput, ItlEvalState, ItlEventProgress, ItlFileData,
    ItlFileReadError, ItlFileWriteError, ItlJudgments, ItlScoreCacheState, OnlineItlSelfCacheMap,
    OnlineItlSelfCacheState, OnlineItlSelfIndexMap, SubmitEventProgressInput,
    event_name_or_unknown, ex_hundredths, itl_chart_no_cmod, itl_data_from_json,
    itl_eval_state_from_parts, itl_group_name_matches, itl_mark_unlock_folders,
    itl_overall_ranks_from_song_cache, itl_rebuild_song_ranks, itl_song_dir,
    itl_song_matches_context, load_online_itl_self_index_file, read_itl_file_from_path,
    save_itl_chart_result, save_online_itl_self_index_file, write_itl_file_to_path,
};
pub use deadsync_score::{is_itl_unlocks_pack, itl_points_for_chart};

#[cfg(test)]
use deadsync_score::{
    ItlHashEntry, ItlPointTotals, itl_judgments_better, itl_point_totals, itl_points_for_song,
    itl_score_for_song, parse_itl_points,
};

const ITL_FILE_NAME: &str = "ITL2026.json";
const ITL_WHEEL_FETCH_ENTRIES: usize = 5;

static ONLINE_ITL_SELF_SCORE_CACHE: std::sync::LazyLock<Mutex<OnlineItlSelfCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(OnlineItlSelfCacheState::default()));
static ONLINE_ITL_SELF_SCORE_GENERATION: AtomicU64 = AtomicU64::new(1);

static ONLINE_ITL_SELF_RANK_CACHE: std::sync::LazyLock<Mutex<OnlineItlSelfCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(OnlineItlSelfCacheState::default()));

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

static ITL_SCORE_CACHE: std::sync::LazyLock<Mutex<ItlScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(ItlScoreCacheState::default()));

struct OnlineItlOverallRankInput {
    api_key: String,
    profile_id: Option<String>,
    self_score_generation: u64,
    by_chart_score: HashMap<String, u32>,
}

fn online_itl_self_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    profile::local_profile_dir_for_id(profile_id)
        .join("scores")
        .join("gs")
        .join("itl_self.bin")
}

fn online_itl_self_rank_index_path_for_profile(profile_id: &str) -> PathBuf {
    profile::local_profile_dir_for_id(profile_id)
        .join("scores")
        .join("gs")
        .join("itl_rank.bin")
}

fn load_online_itl_self_score_index(path: &Path) -> Option<OnlineItlSelfIndexMap> {
    load_online_itl_self_index_file(path)
}

fn save_online_itl_self_score_index(path: &Path, by_key: &OnlineItlSelfCacheMap) {
    if let Err(error) = save_online_itl_self_index_file(path, by_key) {
        warn!("Failed to save ITL self-score cache {path:?}: {error:?}");
    }
}

fn load_online_itl_self_rank_index(path: &Path) -> Option<OnlineItlSelfIndexMap> {
    load_online_itl_self_score_index(path)
}

fn save_online_itl_self_rank_index(path: &Path, by_key: &OnlineItlSelfCacheMap) {
    save_online_itl_self_score_index(path, by_key);
}

#[inline(always)]
fn online_itl_overall_rank_entry_for_side(
    state: &OnlineItlOverallRankCacheState,
    side: profile_data::PlayerSide,
) -> Option<&OnlineItlOverallRankCacheEntry> {
    match side {
        profile_data::PlayerSide::P2 => state.p2.as_ref(),
        _ => state.p1.as_ref(),
    }
}

#[inline(always)]
fn online_itl_overall_rank_entry_for_side_mut(
    state: &mut OnlineItlOverallRankCacheState,
    side: profile_data::PlayerSide,
) -> &mut Option<OnlineItlOverallRankCacheEntry> {
    match side {
        profile_data::PlayerSide::P2 => &mut state.p2,
        _ => &mut state.p1,
    }
}

fn ensure_online_itl_self_score_cache_loaded_for_profile(profile_id: &str) {
    let needs_load = {
        let state = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
        !state.profile_loaded(profile_id)
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
        .insert_loaded_profile(profile_id, by_key);
}

fn ensure_online_itl_self_rank_cache_loaded_for_profile(profile_id: &str) {
    let needs_load = {
        let state = ONLINE_ITL_SELF_RANK_CACHE.lock().unwrap();
        !state.profile_loaded(profile_id)
    };
    if !needs_load {
        return;
    }

    let by_key =
        load_online_itl_self_rank_index(&online_itl_self_rank_index_path_for_profile(profile_id))
            .unwrap_or_default();
    ONLINE_ITL_SELF_RANK_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, by_key);
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
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        ensure_online_itl_self_score_cache_loaded_for_profile(profile_id);
    }

    let update = ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .set_value(profile_id, api_key, chart_hash, score);

    if update.changed {
        ONLINE_ITL_SELF_SCORE_GENERATION.fetch_add(1, AtomicOrdering::Relaxed);
    }

    if let Some((profile_id, by_key)) = update.profile_snapshot {
        save_online_itl_self_score_index(
            &online_itl_self_score_index_path_for_profile(profile_id.as_str()),
            &by_key,
        );
    }
}

pub(super) fn set_cached_online_self_rank(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    rank: Option<u32>,
) {
    let api_key = api_key.trim();
    let chart_hash = chart_hash.trim();
    if api_key.is_empty() || chart_hash.is_empty() {
        return;
    }
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        ensure_online_itl_self_rank_cache_loaded_for_profile(profile_id);
    }

    let update = ONLINE_ITL_SELF_RANK_CACHE
        .lock()
        .unwrap()
        .set_value(profile_id, api_key, chart_hash, rank);

    if let Some((profile_id, by_key)) = update.profile_snapshot {
        save_online_itl_self_rank_index(
            &online_itl_self_rank_index_path_for_profile(profile_id.as_str()),
            &by_key,
        );
    }
}

/// Test/bench helper: seed the *session* online ITL self-score cache directly,
/// keyed by `(chart_hash, api_key)`, without any network fetch or profile file
/// on disk. Lets benchmarks exercise the ITL wheel-score render path. The
/// matching side must be joined and carry a non-empty GrooveStats API key for
/// the wheel lookups to resolve this entry.
pub fn seed_session_online_itl_self_score(api_key: &str, chart_hash: &str, ex_hundredths: u32) {
    set_cached_online_self_score(None, api_key, chart_hash, Some(ex_hundredths));
}

/// Test/bench helper: seed the *session* online ITL self-rank cache directly.
/// See [`seed_session_online_itl_self_score`] for the resolution requirements.
pub fn seed_session_online_itl_self_rank(api_key: &str, chart_hash: &str, rank: u32) {
    set_cached_online_self_rank(None, api_key, chart_hash, Some(rank));
}

/// Test/bench helper: mark song folders as ITL-unlocked for a profile in the
/// in-memory cache without touching disk. Folders not seeded stay locked
/// (matching SL semantics), letting benchmarks exercise the lock-icon path.
pub fn seed_session_itl_unlock_folders(profile_id: &str, folders: &[&str]) {
    ensure_itl_score_cache_loaded(profile_id);
    let mut state = ITL_SCORE_CACHE.lock().unwrap();
    state.mark_unlock_folders(profile_id, folders.iter().copied());
}

pub fn get_cached_itl_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_itl_score_cache_loaded(&profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .chart_score(&profile_id, chart_hash)
}

pub fn get_cached_itl_score_for_song(
    song: &deadsync_chart::SongData,
    side: profile_data::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side);
    get_cached_itl_score_for_song_with_profile(song, profile_id.as_deref())
}

/// Like [`get_cached_itl_score_for_song`] but takes a precomputed profile id so
/// callers iterating many songs in one frame (the song wheel) resolve the
/// active profile once instead of per lookup.
pub fn get_cached_itl_score_for_song_with_profile(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
) -> Option<CachedItlScore> {
    let profile_id = profile_id?;
    ensure_itl_score_cache_loaded(profile_id);
    itl_score_for_song_in_cache(song, profile_id)
}

/// Like [`get_cached_itl_score_for_song_with_profile`] but assumes the profile's
/// ITL score cache was already loaded this frame (see
/// [`ensure_itl_wheel_caches_loaded`]), skipping the per-call ensure-probe lock.
pub fn get_cached_itl_score_for_song_assume_loaded(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
) -> Option<CachedItlScore> {
    let profile_id = profile_id?;
    itl_score_for_song_in_cache(song, profile_id)
}

fn itl_score_for_song_in_cache(
    song: &deadsync_chart::SongData,
    profile_id: &str,
) -> Option<CachedItlScore> {
    ITL_SCORE_CACHE.lock().unwrap().song_score(profile_id, song)
}

/// Load every per-profile ITL cache the song-wheel overlay reads
/// (`ITL_SCORE_CACHE`, `ONLINE_ITL_SELF_SCORE_CACHE`, `ONLINE_ITL_SELF_RANK_CACHE`)
/// once for `profile_id`. Call this once per joined side per frame *before* the
/// per-slot loop so the `*_assume_loaded` accessors can skip their redundant
/// ensure-probe locks.
pub fn ensure_itl_wheel_caches_loaded(profile_id: &str) {
    ensure_itl_score_cache_loaded(profile_id);
    ensure_online_itl_self_score_cache_loaded_for_profile(profile_id);
    ensure_online_itl_self_rank_cache_loaded_for_profile(profile_id);
}

/// Returns true if the song folder is unlocked for this player's ITL profile.
/// Songs not present in the unlock map are treated as locked, matching SL.
pub fn is_itl_song_folder_unlocked_for_side(
    song_folder: &str,
    side: profile_data::PlayerSide,
) -> bool {
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return false;
    };
    ensure_itl_score_cache_loaded(&profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .song_folder_unlocked(&profile_id, song_folder)
}

pub fn is_itl_song_folder_unlocked_with_profile(
    song_folder: &str,
    profile_id: Option<&str>,
) -> bool {
    let Some(profile_id) = profile_id else {
        return false;
    };
    ensure_itl_score_cache_loaded(profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .song_folder_unlocked(profile_id, song_folder)
}

pub fn get_cached_itl_tournament_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    get_cached_player_leaderboard_itl_self_rank_for_side(chart_hash, side)
        .or_else(|| get_cached_online_self_rank_for_side(chart_hash, side))
}

fn get_cached_online_self_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if !profile::is_session_side_joined(side) {
        return None;
    }
    let api_key = profile::groovestats_api_key_for_side(side);
    let profile_id = profile::active_local_profile_id_for_side(side);
    get_cached_online_itl_self_rank_for_key(chart_hash, profile_id.as_deref(), &api_key)
}

/// Cached online ITL self-rank lookup that takes a precomputed profile id and
/// API key instead of re-reading global profile state. Lets the song wheel
/// resolve those frame-invariant values once and reuse them across every slot.
pub fn get_cached_online_itl_self_rank_for_key(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    if let Some(profile_id) = profile_id {
        ensure_online_itl_self_rank_cache_loaded_for_profile(profile_id);
    }
    online_itl_self_rank_in_cache(chart_hash, profile_id, api_key)
}

/// Like [`get_cached_online_itl_self_rank_for_key`] but assumes the profile's
/// rank cache was already loaded this frame (see [`ensure_itl_wheel_caches_loaded`]),
/// skipping the per-call ensure-probe lock.
pub fn get_cached_online_itl_self_rank_for_key_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    online_itl_self_rank_in_cache(chart_hash, profile_id, api_key)
}

fn online_itl_self_rank_in_cache(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    let chart_hash = chart_hash.trim();
    let api_key = api_key.trim();
    if chart_hash.is_empty() || api_key.is_empty() {
        return None;
    }
    ONLINE_ITL_SELF_RANK_CACHE
        .lock()
        .unwrap()
        .get_value(chart_hash, profile_id, api_key)
}

fn online_itl_overall_rank_cache_key_for_side(
    side: profile_data::PlayerSide,
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
    side: profile_data::PlayerSide,
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

    let by_chart = ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .values_by_chart_for_api(profile_id.as_deref(), api_key);
    Some(OnlineItlOverallRankInput {
        api_key: api_key.to_string(),
        profile_id,
        self_score_generation: ONLINE_ITL_SELF_SCORE_GENERATION.load(AtomicOrdering::Relaxed),
        by_chart_score: by_chart,
    })
}

pub fn get_cached_itl_tournament_overall_ranks_for_side(
    side: profile_data::PlayerSide,
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
    let ranks = Arc::new(itl_overall_ranks_from_song_cache(
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
    gs: &GameplayCoreState,
) -> [Option<ItlEventProgress>; MAX_PLAYERS] {
    let mut progress: [Option<ItlEventProgress>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    if gs.autoplay_used() {
        debug!("Skipping ITL save: autoplay or replay was used during this stage.");
        return progress;
    }

    for (player_idx, chart) in gs
        .charts()
        .iter()
        .enumerate()
        .take(gs.num_players().min(MAX_PLAYERS))
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
        let song = gs.song();
        let Some(song_dir) = itl_song_dir(song) else {
            continue;
        };
        let judgments = itl_judgments_from_gameplay(gs, player_idx);
        let (start, end) = gs.note_range_for_player(player_idx);
        let totals = gs.display_totals_for_player(player_idx);
        let ex_percent = judgment::calculate_ex_score_from_notes(
            &gs.notes()[start..end],
            &gs.note_time_cache_ns()[start..end],
            &gs.hold_end_time_cache_ns()[start..end],
            totals.total_steps,
            totals.holds_total,
            totals.rolls_total,
            totals.mines_total,
            gs.players()[player_idx]
                .fail_time
                .map(deadsync_core::song_time::song_time_ns_from_seconds),
            false,
        );
        let save_result = save_itl_chart_result(
            &mut data,
            ItlChartSaveInput {
                song_dir: song_dir.as_str(),
                chart_hash,
                chart_name: gs.charts()[player_idx].chart_name.as_str(),
                chart_type: gs.charts()[player_idx].chart_type.as_str(),
                event_name: itl_group_name(song).as_deref().unwrap_or_default(),
                judgments,
                ex_percent,
                used_cmod: eval.used_cmod,
                chart_no_cmod: eval.chart_no_cmod,
                date: Local::now().format("%Y-%m-%d").to_string(),
            },
        );
        progress[player_idx] = Some(save_result.progress);

        if save_result.needs_write {
            write_itl_file(profile_id.as_str(), &data);
            set_cached_itl_file(profile_id.as_str(), data);
        }
    }

    progress
}

pub(super) fn current_score_hundredths(gs: &GameplayCoreState, player_idx: usize) -> u32 {
    let (start, end) = gs.note_range_for_player(player_idx);
    let totals = gs.display_totals_for_player(player_idx);
    let ex_percent = judgment::calculate_ex_score_from_notes(
        &gs.notes()[start..end],
        &gs.note_time_cache_ns()[start..end],
        &gs.hold_end_time_cache_ns()[start..end],
        totals.total_steps,
        totals.holds_total,
        totals.rolls_total,
        totals.mines_total,
        gs.players()[player_idx]
            .fail_time
            .map(deadsync_core::song_time::song_time_ns_from_seconds),
        false,
    );
    ex_hundredths(ex_percent)
}

pub(super) fn current_score_hundredths_for_submit(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> Option<u32> {
    itl_all_timing_windows_enabled(&gs.profiles()[player_idx])
        .then(|| current_score_hundredths(gs, player_idx))
}

fn itl_file_path(profile_id: &str) -> PathBuf {
    profile::local_profile_dir_for_id(profile_id).join(ITL_FILE_NAME)
}

fn ensure_itl_score_cache_loaded(profile_id: &str) {
    let needs_load = {
        let state = ITL_SCORE_CACHE.lock().unwrap();
        !state.profile_loaded(profile_id)
    };
    if !needs_load {
        return;
    }

    let mut data = read_itl_file(profile_id);
    itl_rebuild_song_ranks(&mut data);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, data);
}

fn set_cached_itl_file(profile_id: &str, data: ItlFileData) {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .set_profile_data(profile_id, data);
}

fn read_itl_file(profile_id: &str) -> ItlFileData {
    let path = itl_file_path(profile_id);
    match read_itl_file_from_path(&path) {
        Ok(data) => data,
        Err(ItlFileReadError::Read { .. }) => ItlFileData::default(),
        Err(ItlFileReadError::Parse { error, .. }) => {
            warn!("Failed to parse ITL data file {path:?}: {error}");
            ItlFileData::default()
        }
    }
}

fn write_itl_file(profile_id: &str, data: &ItlFileData) {
    let path = itl_file_path(profile_id);
    if let Err(error) = write_itl_file_to_path(&path, data) {
        match error {
            ItlFileWriteError::CreateDir { dir, error } => {
                warn!("Failed to create ITL profile dir {dir:?}: {error}");
            }
            ItlFileWriteError::Encode => {
                warn!("Failed to encode ITL data for profile {profile_id}");
            }
            ItlFileWriteError::WriteTemp { path, error } => {
                warn!("Failed to write ITL temp file {path:?}: {error}");
            }
            ItlFileWriteError::Commit { path, error } => {
                warn!("Failed to commit ITL file {path:?}: {error}");
            }
        }
    }
}

/// Imports an ITGmania/Simply Love `ITL2026.json` (raw text) into a
/// freshly-created DeadSync profile, writing it to the profile's ITL file.
/// Returns the number of `hashMap` entries imported (`0` when the file is
/// missing, empty, or unparseable). Song ranks are recomputed lazily the next
/// time the profile's ITL cache is loaded.
pub fn import_itl_json(profile_id: &str, json_text: &str) -> usize {
    let Some(data) = itl_data_from_json(json_text) else {
        return 0;
    };
    let count = data.hash_map.len();
    write_itl_file(profile_id, &data);
    count
}

fn update_unlock_folders(profile_id: &str, folders: &[String]) {
    if folders.is_empty() {
        return;
    }
    let mut data = read_itl_file(profile_id);
    let changed = itl_mark_unlock_folders(&mut data, folders.iter().map(String::as_str));
    if changed {
        write_itl_file(profile_id, &data);
        set_cached_itl_file(profile_id, data);
    }
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
    let accept_itl_response = player.itl_score_hundredths.is_some();
    if accept_itl_response {
        if let Some(itl) = response.itl.as_ref()
            && let Some(profile_id) = player.profile_id.as_deref()
            && let Some(progress) = itl.progress.as_ref()
        {
            for quest in &progress.quests_completed {
                update_unlock_folders(profile_id, quest.song_download_folders.as_slice());
            }
        }
    }
    if let Some(srpg) = response.srpg.as_ref() {
        handle_submit_event_unlocks(player, srpg);
    }
    if accept_itl_response && let Some(itl) = response.itl.as_ref() {
        handle_submit_event_unlocks(player, itl);
    }
}

pub(super) fn event_progress_from_submit(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> Vec<ItlEventProgress> {
    let input = SubmitEventProgressInput {
        result: response.result.clone(),
        score_10000: player.score_10000,
        rate_hundredths: player.rate_hundredths,
        itl_score_hundredths: player.itl_score_hundredths,
        itl: response
            .itl
            .as_ref()
            .map(|event| submit_event_progress_from_api(event, event.itl_leaderboard.clone())),
        srpg: response
            .srpg
            .as_ref()
            .map(|event| submit_event_progress_from_api(event, event.srpg_leaderboard.clone())),
    };
    deadsync_score::event_progress_from_submit(&input)
}

fn itl_group_name(song: &deadsync_chart::SongData) -> Option<String> {
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

fn loaded_chart_no_cmod_for_gameplay(
    gs: &GameplayCoreState,
    player_idx: usize,
    profile_id: &str,
) -> Option<bool> {
    let song = gs.song();
    let song_dir = itl_song_dir(song)?;
    let group_name = itl_group_name(song);
    let state = ITL_SCORE_CACHE.lock().unwrap();
    state.chart_no_cmod_for_song(
        profile_id,
        Some(song_dir.as_str()),
        group_name.as_deref(),
        gs.charts()[player_idx].short_hash.as_str(),
        song.display_subtitle(false),
    )
}

pub fn should_warn_cmod_for_itl_chart(gs: &GameplayCoreState, player_idx: usize) -> bool {
    if player_idx >= gs.num_players().min(MAX_PLAYERS)
        || gs.course_display_is_course_stage()
        || !matches!(
            gs.profiles()[player_idx].scroll_speed,
            ScrollSpeedSetting::CMod(_)
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

    let song = gs.song();
    let Some(group_name) = itl_group_name(song) else {
        return false;
    };
    itl_group_name_matches(group_name.as_str())
        && itl_chart_no_cmod(song.display_subtitle(false), None)
}

fn itl_judgments_from_gameplay(gs: &GameplayCoreState, player_idx: usize) -> ItlJudgments {
    let counts = groovestats_judgment_counts(gs, player_idx);
    ItlJudgments {
        w0: counts.fantastic_plus,
        w1: counts.fantastic,
        w2: counts.excellent,
        w3: counts.great,
        w4: counts.decent_count(),
        w5: counts.way_off_count(),
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

#[inline(always)]
fn itl_all_timing_windows_enabled(profile: &profile_data::Profile) -> bool {
    profile
        .timing_windows
        .disabled_windows()
        .iter()
        .all(|disabled| !*disabled)
}

fn itl_eval_state(gs: &GameplayCoreState, player_idx: usize, data: &ItlFileData) -> ItlEvalState {
    let used_cmod = matches!(
        gs.profiles()[player_idx].scroll_speed,
        ScrollSpeedSetting::CMod(_)
    );
    let song = gs.song();
    let Some(song_dir) = itl_song_dir(song) else {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    };
    let group_name = itl_group_name(song);
    if !itl_song_matches_context(Some(song_dir.as_str()), group_name.as_deref(), data) {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    }

    let chart_hash = gs.charts()[player_idx].short_hash.as_str();
    let prev = data.hash_map.get(chart_hash);
    let chart_no_cmod = itl_chart_no_cmod(song.display_subtitle(false), prev);
    let gs_valid = groovestats_eval_state_from_gameplay(gs, player_idx);
    let remove_mask = gs.profiles()[player_idx].remove_active_mask.bits();
    let mines_enabled = (remove_mask & (1u8 << 1)) == 0;
    let all_timing_windows_enabled = itl_all_timing_windows_enabled(&gs.profiles()[player_idx]);
    let passed = gameplay_run_passed(
        gs.song_completed_naturally(),
        gs.players()[player_idx].is_failing,
        gs.players()[player_idx].life,
        gs.players()[player_idx].fail_time.is_some(),
    );

    itl_eval_state_from_parts(ItlEvalInput {
        chart_no_cmod,
        used_cmod,
        groovestats_valid: gs_valid.valid,
        groovestats_reason_lines: gs_valid.reason_lines.as_slice(),
        music_rate: gs.music_rate(),
        mines_enabled,
        all_timing_windows_enabled,
        passed,
    })
}

pub fn itl_eval_state_from_gameplay(gs: &GameplayCoreState, player_idx: usize) -> ItlEvalState {
    if player_idx >= gs.num_players().min(MAX_PLAYERS) {
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
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if !profile::is_session_side_joined(side) {
        return None;
    }
    let api_key = profile::groovestats_api_key_for_side(side);
    let profile_id = profile::active_local_profile_id_for_side(side);
    get_cached_itl_self_score_for_key(chart_hash, profile_id.as_deref(), &api_key)
}

/// Cached online ITL self-score lookup that takes a precomputed profile id and
/// API key instead of re-reading global profile state. Lets the song wheel
/// resolve those frame-invariant values once and reuse them across every slot.
pub fn get_cached_itl_self_score_for_key(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    if let Some(profile_id) = profile_id {
        ensure_online_itl_self_score_cache_loaded_for_profile(profile_id);
    }
    online_itl_self_score_in_cache(chart_hash, profile_id, api_key)
}

/// Like [`get_cached_itl_self_score_for_key`] but assumes the profile's online
/// self-score cache was already loaded this frame (see
/// [`ensure_itl_wheel_caches_loaded`]), skipping the per-call ensure-probe lock.
pub fn get_cached_itl_self_score_for_key_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    online_itl_self_score_in_cache(chart_hash, profile_id, api_key)
}

fn online_itl_self_score_in_cache(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    let chart_hash = chart_hash.trim();
    let api_key = api_key.trim();
    if chart_hash.is_empty() || api_key.is_empty() {
        return None;
    }
    ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .get_value(chart_hash, profile_id, api_key)
}

pub fn get_or_fetch_itl_self_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if let Some(score) = get_cached_itl_self_score_for_side(chart_hash, side) {
        return Some(score);
    }
    // Keep the wheel's ITL prefetch aligned with the Select Music scorebox cache width.
    // Smaller requests seed the shared leaderboard cache with partial panes, so the
    // scorebox briefly renders a truncated list before refetching the remaining rows.
    let _ = get_or_fetch_player_leaderboards_for_side(chart_hash, side, ITL_WHEEL_FETCH_ENTRIES)?;
    get_cached_itl_self_score_for_side(chart_hash, side)
}

pub fn get_or_fetch_itl_tournament_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if let Some(rank) = get_cached_itl_tournament_rank_for_side(chart_hash, side) {
        return Some(rank);
    }
    let _ = get_or_fetch_player_leaderboards_for_side(chart_hash, side, ITL_WHEEL_FETCH_ENTRIES)?;
    get_cached_itl_tournament_rank_for_side(chart_hash, side)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::SongData;
    use deadsync_chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
    use serde_json::json;
    use std::path::PathBuf;

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
            background_layer2_changes: Vec::new(),
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
            first_second: 0.0,
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
    fn itl_requires_all_timing_windows_enabled() {
        let mut profile = profile_data::Profile::default();
        assert!(itl_all_timing_windows_enabled(&profile));

        for setting in [
            profile_data::TimingWindowsOption::WayOffs,
            profile_data::TimingWindowsOption::DecentsAndWayOffs,
            profile_data::TimingWindowsOption::FantasticsAndExcellents,
        ] {
            profile.timing_windows = setting;
            assert!(!itl_all_timing_windows_enabled(&profile));
        }
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
    fn itl_data_from_json_parses_and_guards() {
        // A Simply Love ITL2026.json with pathMap, hashMap and unlockFolders.
        let text = serde_json::to_string(&json!({
            "pathMap": { "/Songs/ITL Online 2026/Example": "deadbeefcafebabe" },
            "hashMap": {
                "deadbeefcafebabe": { "ex": 94.37, "points": 4200, "clearType": 5 }
            },
            "unlockFolders": { "/Songs/ITL Online 2026/Example": true }
        }))
        .unwrap();
        let data = itl_data_from_json(&text).expect("parses");
        assert_eq!(data.hash_map.len(), 1);
        assert_eq!(data.hash_map["deadbeefcafebabe"].ex, 9437);
        assert_eq!(data.path_map.len(), 1);
        assert!(data.unlock_folders["/Songs/ITL Online 2026/Example"]);

        // Empty and malformed inputs yield None (nothing to import).
        assert!(itl_data_from_json("{}").is_none());
        assert!(itl_data_from_json("not json").is_none());
        assert!(itl_data_from_json(r#"{"hashMap":{}}"#).is_none());
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

        assert!(itl_chart_no_cmod(song.display_subtitle(false), None));
    }

    #[test]
    fn itl_song_matches_context_accepts_cached_path_map() {
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/Custom Pack/Example".to_string(),
            "deadbeefcafebabe".to_string(),
        );

        assert!(itl_song_matches_context(
            Some("/Songs/Custom Pack/Example"),
            None,
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
