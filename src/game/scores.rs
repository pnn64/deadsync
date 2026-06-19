use crate::config::SimpleIni;
use crate::game::gameplay;
use crate::game::online::groovestats as online_groovestats;
use crate::game::profile;
use crate::game::song::get_song_cache;
use crate::game::stage_stats;
use deadlib_platform::dirs;
use deadsync_profile::Profile;
use log::{debug, warn};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use deadsync_online::arrowcloud::{self as arrowcloud_api, ARROWCLOUD_BULK_MAX_HASHES};
use deadsync_online::boxed_request_error;
use deadsync_online::groovestats::{self as groovestats_api, GrooveStatsSubmitApiPlayer};
use deadsync_profile as profile_data;
use deadsync_rules::judgment;

mod arrowcloud;
mod groovestats;
mod itl;

#[inline(always)]
fn active_groovestats_service() -> groovestats_api::Service {
    online_groovestats::active_service()
}

pub use arrowcloud::{
    arrowcloud_next_retry_is_auto, arrowcloud_next_retry_remaining_secs,
    get_arrowcloud_submit_ui_status_for_side, retry_arrowcloud_submit,
    submit_arrowcloud_payloads_from_gameplay, tick_arrowcloud_auto_retries,
};
use deadsync_score::{
    ArrowCloudLeaderboard, ArrowCloudScores, CachedPlayerLeaderboardData, CachedScore,
    CachedScoreImportResult, GameplayScoreboxProfileSnapshot, Grade, GrooveStatsSubmitRecordBanner,
    GsLampChartStats, GsScoreEntry, ImportedPlayerScore, LOCAL_SCORE_VERSION, LeaderboardEntry,
    LocalReplayEdge, LocalScalarScore, LocalScoreEntry, LocalScoreHeader, LocalScoreIndex,
    MachineLeaderboardPlay, MachineReplayEntry, MachineReplayPlay, PlayerLeaderboardCacheKey,
    PlayerLeaderboardData, ScoreBulkImportSummary, ScoreImportEndpoint, ScoreImportProgress,
    arrowcloud_score_from_submit_percent, cached_missing_gs_score, cached_score_from_gs_entry,
    cached_score_from_imported_player_score, cached_score_from_local_header,
    cached_score_import_result_from_imported, compute_local_lamp, decode_gs_score_entry,
    decode_local_score_entry, decode_local_score_header, decode_local_score_index,
    encode_gs_score_entry, encode_local_score_entry, encode_local_score_index,
    failed_score_override, fix_gs_cached_score, gameplay_run_failed, gameplay_run_passed,
    grade_from_code, grade_to_code, gs_score_entry_from_cached, is_better_itg,
    lua_chart_submit_allowed, machine_leaderboard_entries, machine_replay_entries,
    merge_arrowcloud_score_slot, merge_local_fail, parse_score_file_name,
    player_leaderboard_cache_key, promote_quint_grade, score_file_shard, score_to_grade,
    scorebox_snapshot, should_replace_cached_gs_score, update_local_score_index,
};
use groovestats::{GrooveStatsSubmitPlayerJob, groovestats_judgment_counts};
pub use groovestats::{
    get_groovestats_submit_itl_progress_for_side, get_groovestats_submit_record_banner_for_side,
    get_groovestats_submit_ui_status_for_side, groovestats_eval_state_from_gameplay,
    groovestats_next_retry_is_auto, groovestats_next_retry_remaining_secs,
    retry_groovestats_submit, submit_groovestats_payloads_from_gameplay,
    tick_groovestats_auto_retries,
};
pub use itl::{
    ensure_itl_wheel_caches_loaded, get_cached_itl_score_for_side, get_cached_itl_score_for_song,
    get_cached_itl_self_score_for_side, get_cached_itl_tournament_overall_ranks_for_side,
    get_cached_itl_tournament_rank_for_side, get_or_fetch_itl_self_score_for_side,
    get_or_fetch_itl_tournament_rank_for_side, is_itl_song_folder_unlocked_for_side,
    is_itl_song_folder_unlocked_with_profile, is_itl_unlocks_pack, itl_eval_state_from_gameplay,
    itl_points_for_chart, save_itl_data_from_gameplay, seed_session_itl_unlock_folders,
    seed_session_online_itl_self_rank, seed_session_online_itl_self_score,
    should_warn_cmod_for_itl_chart,
};

// --- GrooveStats grade cache (on-disk + network-fetched) ---

#[derive(Default)]
struct GsScoreCacheState {
    loaded_profiles: HashMap<String, HashMap<String, CachedScore>>,
}

static GS_SCORE_CACHE: std::sync::LazyLock<Mutex<GsScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(GsScoreCacheState::default()));

fn gs_scores_dir_for_profile(profile_id: &str) -> PathBuf {
    profile::local_profile_dir_for_id(profile_id)
        .join("scores")
        .join("gs")
}

fn gs_scores_dir_for_profile_and_hash(profile_id: &str, chart_hash: &str) -> PathBuf {
    gs_scores_dir_for_profile(profile_id).join(score_file_shard(chart_hash))
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
    side: profile_data::PlayerSide,
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
    side: profile_data::PlayerSide,
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
    let new_scores = ArrowCloudScores {
        itg: arrowcloud_score_from_submit_percent(itg_percent, is_fail, submitted_at.clone()),
        ex: arrowcloud_score_from_submit_percent(ex_percent, is_fail, submitted_at.clone()),
        hard_ex: arrowcloud_score_from_submit_percent(
            hard_ex_percent,
            is_fail,
            submitted_at.clone(),
        ),
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

// --- Local score cache (on-disk, one file per play) ---

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
    profile::local_profile_dir_for_id(profile_id)
        .join("scores")
        .join("local")
}

fn local_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    local_scores_root_for_profile(profile_id).join("index.bin")
}

fn load_local_score_index_file(path: &Path) -> Option<LocalScoreIndex> {
    let bytes = fs::read(path).ok()?;
    decode_local_score_index(&bytes)
}

fn save_local_score_index_file(path: &Path, index: &LocalScoreIndex) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(parent) {
        warn!("Failed to create local score index dir {parent:?}: {e}");
        return;
    }
    let Some(buf) = encode_local_score_index(index) else {
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

pub fn total_songs_played_for_side(side: profile_data::PlayerSide) -> u32 {
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return 0;
    };
    total_songs_played_for_profile(&profile_id)
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
        let Some((chart_hash, played_at_ms)) = parse_score_file_name(name) else {
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
        let Some((chart_hash, _played_at_ms)) = parse_score_file_name(name) else {
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
    let local_root = local_scores_root_for_profile(profile_id);
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
    let local_root = local_scores_root_for_profile(profile_id);
    if !local_root.is_dir() {
        return Vec::new();
    }

    let mut counts_by_chart: HashMap<String, u32> = HashMap::new();
    collect_play_counts_in_root(&local_root, &mut counts_by_chart);

    let mut ranked: Vec<(String, u32)> = counts_by_chart.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
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
    let ini_path = profile::local_profile_dir_for_id(profile_id).join("profile.ini");
    if !ini_path.is_file() {
        return None;
    }
    let mut ini = SimpleIni::new();
    if ini.load(&ini_path).is_err() {
        return None;
    }
    let s = ini.get("userprofile", "PlayerInitials")?;
    let s = profile_data::sanitize_player_initials(&s);
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

    let p1_profile_id = profile::active_local_profile_id_for_side(profile_data::PlayerSide::P1);
    let p2_profile_id = profile::active_local_profile_id_for_side(profile_data::PlayerSide::P2);

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
    side: profile_data::PlayerSide,
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
    side: profile_data::PlayerSide,
) -> Option<CachedScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    get_cached_score_with_profile(chart_hash, &profile_id)
}

/// Like [`get_cached_score_for_side`] but takes a precomputed profile id so the
/// song wheel can resolve the active profile once per side per frame instead of
/// three times per slot (local + gs + ac each re-resolved the active id and
/// allocated a `String`).
pub fn get_cached_score_with_profile(chart_hash: &str, profile_id: &str) -> Option<CachedScore> {
    let local = get_cached_local_score_for_profile(profile_id, chart_hash);
    let gs = get_cached_gs_score_for_profile(profile_id, chart_hash);
    let ac = get_cached_ac_scores_for_profile(profile_id, chart_hash)
        .and_then(|s| s.itg)
        .map(|ac| ac.to_cached_score());
    // Merge by picking the "best ITG" entry; failed scores win when their
    // numeric percent matches a cached non-failed score (parity with prior
    // local+gs merge semantics).
    [local, gs, ac].into_iter().flatten().reduce(|a, b| {
        failed_score_override(&a, &b).unwrap_or_else(|| if is_better_itg(&a, &b) { a } else { b })
    })
}

pub struct HeldScoreCaches {
    local: std::sync::MutexGuard<'static, LocalScoreCacheState>,
    gs: std::sync::MutexGuard<'static, GsScoreCacheState>,
    ac: std::sync::MutexGuard<'static, AcScoreCacheState>,
}

impl HeldScoreCaches {
    /// Resolve the merged "best ITG" score for `chart_hash` under `profile_id`,
    /// reading the already-held cache maps. Identical merge semantics to
    /// [`get_cached_score_with_profile`].
    pub fn merged(&self, profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
        if profile_id.trim().is_empty() {
            return None;
        }
        let local = self
            .local
            .loaded_profiles
            .get(profile_id)
            .and_then(|idx| idx.best_itg.get(chart_hash).copied());
        let gs = self
            .gs
            .loaded_profiles
            .get(profile_id)
            .and_then(|m| m.get(chart_hash).copied());
        let ac = self
            .ac
            .loaded_profiles
            .get(profile_id)
            .and_then(|m| m.get(chart_hash).copied())
            .and_then(|s| s.itg)
            .map(|ac| ac.to_cached_score());
        [local, gs, ac].into_iter().flatten().reduce(|a, b| {
            failed_score_override(&a, &b)
                .unwrap_or_else(|| if is_better_itg(&a, &b) { a } else { b })
        })
    }
}

pub fn ensure_score_caches_loaded(profile_id: &str) {
    if profile_id.trim().is_empty() {
        return;
    }
    ensure_local_score_cache_loaded(profile_id);
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    ensure_ac_score_cache_loaded_for_profile(profile_id);
}

pub fn lock_score_caches() -> HeldScoreCaches {
    HeldScoreCaches {
        local: LOCAL_SCORE_CACHE.lock().unwrap(),
        gs: GS_SCORE_CACHE.lock().unwrap(),
        ac: AC_SCORE_CACHE.lock().unwrap(),
    }
}

/// Test/bench helper: seed the in-memory local ITG score cache for a profile
/// without touching disk. Lets benchmarks exercise the wheel grade/lamp render
/// path deterministically.
pub fn seed_session_local_itg_score(profile_id: &str, chart_hash: &str, score: CachedScore) {
    ensure_local_score_cache_loaded(profile_id);
    LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_default()
        .best_itg
        .insert(chart_hash.to_string(), score);
}

/// Test/bench helper: seed the in-memory GrooveStats grade cache for a profile
/// without touching disk. See [`seed_session_local_itg_score`].
pub fn seed_session_gs_score(profile_id: &str, chart_hash: &str, score: CachedScore) {
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_default()
        .insert(chart_hash.to_string(), score);
}

fn get_cached_local_scalar_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
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
    side: profile_data::PlayerSide,
) -> Option<LocalScalarScore> {
    get_cached_local_scalar_score_for_side(chart_hash, side, false)
}

pub fn get_cached_local_hard_ex_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<LocalScalarScore> {
    get_cached_local_scalar_score_for_side(chart_hash, side, true)
}

#[inline(always)]
pub fn is_gs_get_scores_service_allowed() -> bool {
    crate::config::get().enable_groovestats
}

#[inline(always)]
pub fn is_gs_active_for_side(side: profile_data::PlayerSide) -> bool {
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
        let cached = cached_score_from_gs_entry(&entry);

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
    let new_entry = gs_score_entry_from_cached(score, username, fetched_at_ms);

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

    match encode_gs_score_entry(&new_entry) {
        Some(buf) => {
            if let Err(e) = fs::write(&path, buf) {
                warn!("Failed to write GrooveStats score file {path:?}: {e}");
            } else {
                debug!("Stored GrooveStats score on disk for chart {chart_hash} at {path:?}");
            }
        }
        None => {
            warn!("Failed to encode GrooveStats score for chart {chart_hash}");
        }
    }
}

// --- On-disk local score storage (one file per play) ---

fn read_local_score_header(path: &Path) -> Option<LocalScoreHeader> {
    // Local score files include replay data; for indexing we only need the prefix.
    // A 1KiB prefix comfortably covers the fixed header fields.
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(1024);
    if file.take(1024).read_to_end(&mut buf).is_err() || buf.is_empty() {
        return None;
    }
    decode_local_score_header(&buf)
}

fn read_local_score_entry(path: &Path) -> Option<LocalScoreEntry> {
    let bytes = fs::read(path).ok()?;
    decode_local_score_entry(&bytes)
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
        let Some((chart_hash, _played_at_ms)) = parse_score_file_name(name) else {
            continue;
        };

        let Some(h) = read_local_score_header(&path) else {
            continue;
        };

        update_local_score_index(index, chart_hash, &h);
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

fn append_local_score_on_disk(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut LocalScoreEntry,
) -> bool {
    let shard = score_file_shard(chart_hash);
    let dir = local_scores_root_for_profile(profile_id).join(shard);
    if let Err(e) = fs::create_dir_all(&dir) {
        warn!("Failed to create local scores dir {dir:?}: {e}");
        return false;
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
    let Some(buf) = encode_local_score_entry(entry) else {
        warn!("Failed to encode local score for chart {chart_hash}");
        return false;
    };
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write local score temp file {tmp_path:?}: {e}");
        return false;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        warn!("Failed to commit local score file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
        return false;
    }

    // Update in-memory cache if it's already loaded for this profile.
    let header = entry.header();
    let loaded_snapshot = {
        let mut state = LOCAL_SCORE_CACHE.lock().unwrap();
        if let Some(idx) = state.loaded_profiles.get_mut(profile_id) {
            update_local_score_index(idx, chart_hash, &header);
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
        update_local_score_index(&mut index, chart_hash, &header);
        save_local_score_index_file(&index_path, &index);
    }

    let cached = cached_score_from_local_header(&header);
    update_machine_cache_if_loaded(chart_hash, cached, profile_initials);
    true
}

/// Write a batch of imported local scores for `profile_id`. Each tuple is the
/// DeadSync chart `short_hash` and the play to record. Returns the number of
/// plays successfully written to disk.
///
/// This reuses the same per-play write path as live gameplay, so the per-profile
/// best index and any loaded in-memory caches stay correct.
pub fn import_local_scores(
    profile_id: &str,
    profile_initials: &str,
    scores: &mut [(String, LocalScoreEntry)],
) -> usize {
    let mut written = 0usize;
    for (chart_hash, entry) in scores.iter_mut() {
        if append_local_score_on_disk(profile_id, profile_initials, chart_hash, entry) {
            written += 1;
        }
    }
    written
}

fn judgment_counts_arr(p: &gameplay::PlayerRuntime) -> [u32; 6] {
    p.judgment_counts
}

fn replay_edges_for_player(gs: &gameplay::State, player: usize) -> Vec<LocalReplayEdge> {
    if player >= gameplay::num_players(gs) {
        return Vec::new();
    }

    let (col_start, col_end) = if gameplay::num_players(gs) <= 1 {
        (0usize, gameplay::num_cols(gs))
    } else {
        let start = player.saturating_mul(gameplay::cols_per_player(gs));
        (start, start.saturating_add(gameplay::cols_per_player(gs)))
    };

    let mut out = Vec::new();
    let replay_edges = gameplay::recorded_replay_edges(gs);
    out.reserve(replay_edges.len().min(4096));
    for e in replay_edges {
        let lane = e.lane_index as usize;
        if lane < col_start
            || lane >= col_end
            || gameplay::song_time_ns_invalid(e.event_music_time_ns)
        {
            continue;
        }
        out.push(LocalReplayEdge::new(
            e.event_music_time_ns,
            (lane - col_start) as u8,
            e.pressed,
            e.source,
        ));
    }
    out
}

pub fn save_local_scores_from_gameplay(gs: &gameplay::State) {
    if gameplay::autoplay_used(gs) {
        debug!("Skipping local score save: autoplay was used during this stage.");
        return;
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // Parameter retained for parity with Simply Love helpers; currently unused.
    let mines_disabled = false;

    for player_idx in 0..gameplay::num_players(gs) {
        let side = if gameplay::num_players(gs) >= 2 {
            if player_idx == 0 {
                profile_data::PlayerSide::P1
            } else {
                profile_data::PlayerSide::P2
            }
        } else {
            profile::get_session_player_side()
        };

        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        if !gameplay::score_valid_for_player(gs, player_idx) {
            let reasons = gameplay::score_invalid_reason_lines_for_chart(
                &gameplay::charts(gs)[player_idx],
                &gameplay::player_profiles(gs)[player_idx],
                gameplay::scroll_speed_for_player(gs, player_idx),
                gameplay::music_rate(gs),
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

        let chart_hash = gameplay::charts(gs)[player_idx].short_hash.as_str();
        let p = &gameplay::players(gs)[player_idx];
        let totals = gameplay::display_totals_for_player(gs, player_idx);

        let score_percent = judgment::calculate_itg_score_percent_from_counts(
            &p.scoring_counts,
            p.holds_held_for_score,
            p.rolls_held_for_score,
            p.mines_hit_for_score,
            totals.possible_grade_points,
        );

        let mut grade = if gameplay_run_passed(
            gameplay::song_completed_naturally(gs),
            p.is_failing,
            p.life,
            p.fail_time.is_some(),
        ) {
            score_to_grade(score_percent * 10000.0)
        } else {
            Grade::Failed
        };

        let (start, end) = gameplay::note_range_for_player(gs, player_idx);
        let notes = &gameplay::notes(gs)[start..end];
        let note_times = &gameplay::note_time_cache_ns(gs)[start..end];
        let hold_end_times = &gameplay::hold_end_time_cache_ns(gs)[start..end];

        let ex_score_percent = judgment::calculate_ex_score_from_notes(
            notes,
            note_times,
            hold_end_times,
            totals.total_steps,
            totals.holds_total,
            totals.rolls_total,
            totals.mines_total,
            p.fail_time.map(gameplay::song_time_ns_from_seconds),
            mines_disabled,
        );
        let hard_ex_score_percent = judgment::calculate_hard_ex_score_from_notes(
            notes,
            note_times,
            hold_end_times,
            totals.total_steps,
            totals.holds_total,
            totals.rolls_total,
            totals.mines_total,
            p.fail_time.map(gameplay::song_time_ns_from_seconds),
            mines_disabled,
        );

        // Quint comes from the achieved result, not the active UI display mode.
        grade = promote_quint_grade(grade, ex_score_percent);

        let counts = judgment_counts_arr(p);
        let white_fantastics = Some(gameplay::live_window_counts(gs, player_idx).w1);
        let (lamp_index, lamp_judge_count) = compute_local_lamp(counts, grade, white_fantastics);
        let replay = replay_edges_for_player(gs, player_idx);

        let mut entry = LocalScoreEntry {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: now_ms,
            music_rate: gameplay::music_rate(gs),
            score_percent,
            grade_code: grade_to_code(grade),
            lamp_index,
            lamp_judge_count,
            ex_score_percent,
            hard_ex_score_percent,
            judgment_counts: counts,
            holds_held: p.holds_held,
            holds_total: totals.holds_total,
            rolls_held: p.rolls_held,
            rolls_total: totals.rolls_total,
            mines_avoided: p.mines_avoided,
            mines_total: totals.mines_total,
            hands_achieved: p.hands_achieved,
            fail_time: p.fail_time,
            beat0_time_ns: gameplay::timing_for_player(gs, player_idx)
                .map(|timing| timing.get_time_for_beat_ns(0.0))
                .unwrap_or(0),
            replay,
        };

        append_local_score_on_disk(
            &profile_id,
            gameplay::player_profiles(gs)[player_idx]
                .player_initials
                .as_str(),
            chart_hash,
            &mut entry,
        );
    }
}

#[inline(always)]
pub(super) fn gameplay_side_for_player(
    gs: &gameplay::State,
    player_idx: usize,
) -> profile_data::PlayerSide {
    if gameplay::num_players(gs) >= 2 {
        profile_data::player_side_for_index(player_idx)
    } else {
        profile::get_session_player_side()
    }
}

#[inline(always)]
pub fn save_local_summary_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
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
    let mut entry = LocalScoreEntry {
        version: LOCAL_SCORE_VERSION,
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

pub fn scorebox_profile_snapshot(
    player_profile: &profile_data::Profile,
    side_joined: bool,
    persistent_profile_id: Option<String>,
) -> GameplayScoreboxProfileSnapshot {
    let cfg = crate::config::get();
    scorebox_snapshot(
        player_profile.display_scorebox,
        player_profile.show_ex_score,
        side_joined,
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
        player_profile.groovestats_api_key.as_str(),
        player_profile.arrowcloud_api_key.as_str(),
        player_profile.groovestats_username.as_str(),
        persistent_profile_id,
    )
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
    by_key: hashbrown::HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheEntry>,
    in_flight: HashMap<PlayerLeaderboardCacheKey, usize>,
    pending_refresh: HashMap<PlayerLeaderboardCacheKey, usize>,
    invalidated_after: HashMap<PlayerLeaderboardCacheKey, Instant>,
}

static PLAYER_LEADERBOARD_CACHE: std::sync::LazyLock<Mutex<PlayerLeaderboardCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(PlayerLeaderboardCacheState::default()));

const PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL: Duration = Duration::from_secs(10);

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
fn submit_record_banner(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> Option<GrooveStatsSubmitRecordBanner> {
    groovestats_api::submit_record_banner_from_api(
        response,
        player.username.as_str(),
        player.show_ex_score,
    )
}

fn chart_stats_for_imported_score(
    score: &ImportedPlayerScore,
    chart_hash: &str,
) -> Option<GsLampChartStats> {
    score
        .needs_chart_stats()
        .then(|| find_chart_stats_for_hash(chart_hash))
        .flatten()
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
        && !should_replace_cached_gs_score(&score, &existing, proves_nonquint_ex)
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
    imported: Option<ImportedPlayerScore>,
) {
    let Some(mut imported) = imported else {
        // Select Music and gameplay scoreboxes fetch shallow leaderboard pages.
        // If the page does not include the player's row, do not clobber an
        // existing cached GS score and make the wheel fall back to local data.
        return;
    };
    imported = merge_local_fail(
        imported,
        get_cached_local_score_for_profile(profile_id, chart_hash),
    );
    let proves_nonquint_ex = imported.ex_evidence.proves_nonquint();
    let stats = chart_stats_for_imported_score(&imported, chart_hash);
    let score = cached_score_from_imported_player_score(imported, stats);
    cache_gs_score_for_profile(profile_id, chart_hash, score, username, proves_nonquint_ex);
}

fn fetch_player_leaderboards_internal(
    chart_hash: &str,
    api_key: &str,
    username: &str,
    arrowcloud_api_key: Option<&str>,
    show_ex_score: bool,
    max_entries: usize,
) -> Result<groovestats_api::FetchedPlayerLeaderboards, Box<dyn Error + Send + Sync>> {
    if chart_hash.trim().is_empty() {
        return Err("Missing chart hash for leaderboard request.".into());
    }
    if api_key.trim().is_empty() {
        return Err("Missing GrooveStats API key for leaderboard request.".into());
    }

    let combined = groovestats_api::fetch_combined_player_leaderboards(
        active_groovestats_service(),
        api_key,
        username,
        chart_hash,
        arrowcloud_api_key,
        show_ex_score,
        max_entries,
    )
    .map_err(|error| boxed_request_error("Leaderboard API", error))?;
    if let Some(error) = combined.arrowcloud_error {
        warn!(
            "ArrowCloud leaderboard fetch failed for chart {}: {}",
            chart_hash, error
        );
    }

    Ok(combined.fetched)
}

#[inline(always)]
const fn loading_player_leaderboard_snapshot() -> CachedPlayerLeaderboardData {
    CachedPlayerLeaderboardData::loading()
}

#[inline(always)]
fn cache_snapshot_from_entry(entry: &PlayerLeaderboardCacheEntry) -> CachedPlayerLeaderboardData {
    match &entry.value {
        PlayerLeaderboardCacheValue::Ready(data) => {
            CachedPlayerLeaderboardData::ready(data.clone())
        }
        PlayerLeaderboardCacheValue::Error(error) => {
            CachedPlayerLeaderboardData::error(error.clone())
        }
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
        let mut fetched_itl_self = None;
        let mut fetched_imported_score = None;

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
                            let groovestats_api::FetchedPlayerLeaderboards {
                                data,
                                imported_score,
                                itl_self_found,
                            } = fetched;
                            if itl_self_found {
                                fetched_itl_self = Some((data.itl_self_score, data.itl_self_rank));
                            }
                            if should_auto_populate && auto_profile_id.is_some() {
                                fetched_imported_score = imported_score;
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

        // Keep score/ITL cache writes outside PLAYER_LEADERBOARD_CACHE. The wheel
        // can hold score-cache guards while probing leaderboard rank state.
        if let Some((itl_self_score, itl_self_rank)) = fetched_itl_self {
            itl::set_cached_online_self_score(
                persistent_profile_id.as_deref(),
                key.api_key.as_str(),
                key.chart_hash.as_str(),
                itl_self_score,
            );
            itl::set_cached_online_self_rank(
                persistent_profile_id.as_deref(),
                key.api_key.as_str(),
                key.chart_hash.as_str(),
                itl_self_rank,
            );
        }
        if let Some(imported_score) = fetched_imported_score
            && let Some(profile_id) = auto_profile_id.as_deref()
        {
            cache_gs_score_from_leaderboard(
                profile_id,
                gs_username.as_str(),
                key.chart_hash.as_str(),
                Some(imported_score),
            );
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
    side: profile_data::PlayerSide,
) -> GameplayScoreboxProfileSnapshot {
    let cfg = crate::config::get();
    let (
        display_scorebox,
        show_ex_score,
        groovestats_api_key,
        arrowcloud_api_key,
        groovestats_username,
    ) = profile::scorebox_fields_for_side(side);
    scorebox_snapshot(
        display_scorebox,
        show_ex_score,
        profile::is_session_side_joined(side),
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
        groovestats_api_key.as_str(),
        arrowcloud_api_key.as_str(),
        groovestats_username.as_str(),
        profile::active_local_profile_id_for_side(side),
    )
}

fn get_cached_player_leaderboard_itl_self_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    let profile_snapshot = player_leaderboard_profile_snapshot_for_side(side);
    get_cached_player_leaderboard_itl_self_rank_with(chart_hash, &profile_snapshot)
}

/// Borrowed view of [`PlayerLeaderboardCacheKey`] for allocation-free cache
/// probes from the per-frame song-wheel rank lookup. Mirrors the field order
/// and the gate of `player_leaderboard_cache_key` so it hashes and compares
/// identically to the owned key without allocating the three key strings.
#[derive(Hash)]
struct PlayerLeaderboardCacheKeyRef<'a> {
    chart_hash: &'a str,
    api_key: &'a str,
    arrowcloud_api_key: &'a str,
    include_arrowcloud: bool,
    show_ex_score: bool,
}

impl<'a> PlayerLeaderboardCacheKeyRef<'a> {
    fn for_lookup(
        chart_hash: &'a str,
        snapshot: &'a GameplayScoreboxProfileSnapshot,
    ) -> Option<Self> {
        let chart_hash = chart_hash.trim();
        if chart_hash.is_empty() || !snapshot.gs_active {
            return None;
        }
        Some(Self {
            chart_hash,
            api_key: snapshot.api_key(),
            arrowcloud_api_key: snapshot.arrowcloud_api_key(),
            include_arrowcloud: snapshot.include_arrowcloud(),
            show_ex_score: snapshot.show_ex_score,
        })
    }
}

impl hashbrown::Equivalent<PlayerLeaderboardCacheKey> for PlayerLeaderboardCacheKeyRef<'_> {
    fn equivalent(&self, key: &PlayerLeaderboardCacheKey) -> bool {
        self.chart_hash == key.chart_hash
            && self.api_key == key.api_key
            && self.arrowcloud_api_key == key.arrowcloud_api_key
            && self.include_arrowcloud == key.include_arrowcloud
            && self.show_ex_score == key.show_ex_score
    }
}

fn get_cached_player_leaderboard_itl_self_rank_with(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    let kref = PlayerLeaderboardCacheKeyRef::for_lookup(chart_hash, profile_snapshot)?;
    let cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
    let entry = cache.by_key.get(&kref)?;
    let PlayerLeaderboardCacheValue::Ready(data) = &entry.value else {
        return None;
    };
    data.itl_self_rank
}

pub struct ItlWheelSideContext {
    profile_id: Option<std::sync::Arc<str>>,
    api_key: String,
    leaderboard_snapshot: GameplayScoreboxProfileSnapshot,
}

impl ItlWheelSideContext {
    pub fn for_side(
        side: profile_data::PlayerSide,
        profile_id: Option<std::sync::Arc<str>>,
    ) -> Self {
        Self {
            profile_id,
            api_key: profile::groovestats_api_key_for_side(side),
            leaderboard_snapshot: player_leaderboard_profile_snapshot_for_side(side),
        }
    }

    /// Cached local ITL score for a song (reads the per-profile ITL file cache).
    /// Assumes [`ensure_itl_wheel_caches_loaded`] ran for this side this frame.
    pub fn cached_local_itl_score(
        &self,
        song: &deadsync_chart::SongData,
    ) -> Option<deadsync_score::CachedItlScore> {
        itl::get_cached_itl_score_for_song_assume_loaded(song, self.profile_id.as_deref())
    }

    /// Cached online ITL self EX score for a chart hash, in integer hundredths
    /// of a percent (e.g. `9912` = 99.12%).
    pub fn cached_self_ex_score(&self, chart_hash: &str) -> Option<u32> {
        itl::get_cached_itl_self_score_for_key_assume_loaded(
            chart_hash,
            self.profile_id.as_deref(),
            &self.api_key,
        )
    }

    /// Cached ITL tournament rank for a chart hash: prefers the player
    /// leaderboard cache, falling back to the online self-rank cache.
    pub fn cached_tournament_rank(&self, chart_hash: &str) -> Option<u32> {
        get_cached_player_leaderboard_itl_self_rank_with(chart_hash, &self.leaderboard_snapshot)
            .or_else(|| {
                itl::get_cached_online_itl_self_rank_for_key_assume_loaded(
                    chart_hash,
                    self.profile_id.as_deref(),
                    &self.api_key,
                )
            })
    }
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
    let key = player_leaderboard_cache_key(chart_hash, profile_snapshot)?;
    let gs_username = profile_snapshot.gs_username().to_string();
    let persistent_profile_id = profile_snapshot.persistent_profile_id().map(str::to_string);
    let auto_profile_id = profile_snapshot.auto_profile_id().map(str::to_string);
    let should_auto_populate = profile_snapshot.should_auto_populate();

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
    side: profile_data::PlayerSide,
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
    side: profile_data::PlayerSide,
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

pub fn invalidate_player_leaderboards_for_side(chart_hash: &str, side: profile_data::PlayerSide) {
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
        let Some((file_hash, played_at_ms)) = parse_score_file_name(file_name) else {
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
        let Some((file_hash, played_at_ms)) = parse_score_file_name(name) else {
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
        let shard_dir = root.join(score_file_shard(chart_hash));
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
        let shard_dir = root.join(score_file_shard(chart_hash));
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
    side: profile_data::PlayerSide,
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
    let shard_dir = root.join(score_file_shard(chart_hash));

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
        let shard_dir = root.join(score_file_shard(chart_hash));
        push_machine_replays_from_dir(&shard_dir, chart_hash, &initials, &mut plays);
    }

    machine_replay_entries(plays, max_entries)
}

fn find_chart_stats_for_hash(chart_hash: &str) -> Option<GsLampChartStats> {
    let cache = get_song_cache();
    for pack in cache.iter() {
        for song in &pack.songs {
            for chart in &song.charts {
                if chart.short_hash == chart_hash {
                    return Some(GsLampChartStats {
                        total_steps: chart.stats.total_steps,
                        holds: chart.stats.holds,
                        rolls: chart.stats.rolls,
                    });
                }
            }
        }
    }
    None
}

// --- Public Fetch Function ---

const SCORE_IMPORT_RATE_LIMIT_PER_SECOND: u32 = 3;
const SCORE_IMPORT_REQUEST_INTERVAL: Duration = Duration::from_millis(334);
const SCORE_IMPORT_PROGRESS_LOG_EVERY: usize = 100;

fn fetch_player_score_from_endpoint(
    endpoint: ScoreImportEndpoint,
    profile: &Profile,
    chart_hash: &str,
) -> Result<CachedScoreImportResult, Box<dyn Error + Send + Sync>> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() {
        return Err("Missing chart hash for score request.".into());
    }

    let api_key = profile.score_import_api_key(endpoint);
    if api_key.is_empty() {
        return Err(format!(
            "{} API key is missing in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }
    let username = profile.score_import_username(endpoint);
    if endpoint.requires_username() && username.is_empty() {
        return Err(format!(
            "{} username is missing in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }

    let imported =
        groovestats_api::fetch_player_score_import_result(endpoint, api_key, username, chart_hash)
            .map_err(|error| boxed_request_error("API", error))?;
    let stats = imported
        .score
        .as_ref()
        .and_then(|score| chart_stats_for_imported_score(score, chart_hash));
    Ok(cached_score_import_result_from_imported(imported, stats))
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
    let api_key = profile
        .score_import_api_key(ScoreImportEndpoint::ArrowCloud)
        .to_string();
    if api_key.is_empty() {
        return Err("ArrowCloud API key is not set in profile configuration.".into());
    }

    // Resolve our user id once up front. The bulk endpoint accepts a missing
    // userId (it'll resolve from the bearer token), but sending it explicitly
    // is preferred and matches the documented contract.
    let user_id = match arrowcloud_api::fetch_user_context(&api_key) {
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
            let detail = match arrowcloud_api::retrieve_score_cache_entries(
                &api_key,
                user_id.as_deref(),
                &chunk_vec,
                &ArrowCloudLeaderboard::ALL_GLOBAL,
            )
            .map_err(|error| boxed_request_error("API", error))
            {
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
    let api_key = profile.score_import_api_key(endpoint);
    if api_key.is_empty() {
        return Err(format!(
            "{} API key is not set in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }
    if endpoint.requires_username() && profile.score_import_username(endpoint).is_empty() {
        return Err(format!(
            "{} username is not set in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }

    let username = profile.score_import_username(endpoint).to_string();
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
    let endpoint = active_groovestats_service().score_import_endpoint();
    if !profile.has_score_import_credentials(endpoint) {
        return Err("GrooveStats API key or username is not set in profile.ini.".into());
    }

    debug!(
        "Requesting scores for '{}' on chart '{}'...",
        profile.score_import_username(endpoint),
        chart_hash
    );

    let result = fetch_player_score_from_endpoint(endpoint, &profile, chart_hash.as_str())?;
    if let Some(cached_score) = result.score {
        cache_gs_score_for_profile(
            &profile_id,
            &chart_hash,
            cached_score,
            profile.score_import_username(endpoint),
            result.score_proves_nonquint_ex,
        );
    } else {
        warn!(
            "No score found for player '{}' on chart '{}'. Caching as Failed.",
            profile.score_import_username(endpoint),
            chart_hash
        );
        set_cached_gs_score_for_profile(&profile_id, chart_hash, cached_missing_gs_score());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

        cache_gs_score_from_leaderboard(profile_id, "PerfectTaste", chart_hash, None);

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
            Some(ImportedPlayerScore {
                score_10000: 1358.0,
                comments: None,
                is_fail: false,
                ex_evidence: deadsync_score::GsExEvidence::default(),
            }),
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
    fn wheel_score_read_does_not_deadlock_with_leaderboard_worker() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;

        let profile_id = "test-deadlock-wheel-profile";
        let chart_hash = "feedface";
        let seeded = CachedScore {
            grade: Grade::Tier01,
            score_percent: 0.9123,
            lamp_index: Some(2),
            lamp_judge_count: Some(7),
        };
        // Seed every cache the read path consults so it never hits disk and the
        // `ensure_*_loaded` helpers become no-ops during the run.
        seed_session_local_itg_score(profile_id, chart_hash, seeded);
        seed_session_gs_score(profile_id, chart_hash, seeded);
        ensure_ac_score_cache_loaded_for_profile(profile_id);

        let stop = Arc::new(AtomicBool::new(false));

        // Worker: reproduce the pre-fix lock order
        // (`PLAYER_LEADERBOARD_CACHE -> LOCAL -> GS -> AC`). If the wheel read
        // ever again held a score cache across the leaderboard lock, this
        // ordering would close the cycle and deadlock.
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let lb = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
                let local = LOCAL_SCORE_CACHE.lock().unwrap();
                let gs = GS_SCORE_CACHE.lock().unwrap();
                let ac = AC_SCORE_CACHE.lock().unwrap();
                drop((ac, gs, local, lb));
            }
        });

        // Wheel: the fixed read path, hammered on a watchdog-guarded thread.
        const ITERS: usize = 20_000;
        let (done_tx, done_rx) = mpsc::channel();
        let wheel_profile = profile_id.to_string();
        let wheel_chart = chart_hash.to_string();
        let wheel = std::thread::spawn(move || {
            let mut last = None;
            for _ in 0..ITERS {
                last = get_cached_score_with_profile(&wheel_chart, &wheel_profile);
            }
            let _ = done_tx.send(last);
        });

        let result = done_rx.recv_timeout(std::time::Duration::from_secs(30));
        stop.store(true, Ordering::Relaxed);

        match result {
            Ok(last) => {
                wheel.join().expect("wheel thread panicked");
                worker.join().expect("worker thread panicked");
                assert_eq!(
                    last,
                    Some(seeded),
                    "wheel read returned an unexpected merged score"
                );
            }
            Err(_) => {
                // The wheel thread is blocked on a lock; joining would hang, so
                // we deliberately leak it and fail loudly. This is the deadlock.
                panic!(
                    "song-wheel score read deadlocked against the leaderboard worker \
                     no progress within 30s"
                );
            }
        }

        // Leave the shared caches clean for other tests in this process.
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
        AC_SCORE_CACHE
            .lock()
            .unwrap()
            .loaded_profiles
            .remove(profile_id);
    }
}
