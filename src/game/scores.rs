use super::{GameplayCoreState, chart_effects_from_profile, gameplay_attack_mode};
use crate::config::SimpleIni;
use crate::game::online::groovestats as online_groovestats;
use crate::game::profile;
use crate::game::song::get_song_cache;
use deadlib_platform::dirs;
use deadsync_profile::Profile;
use deadsync_score::stage_stats;
use log::{debug, warn};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use deadsync_gameplay::{
    PlayerRuntime, ScoreValidityOptions, score_invalid_reason_lines_for_options,
};
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

fn score_invalid_reason_lines_for_profile(
    chart: &deadsync_chart::ChartData,
    profile: &profile_data::Profile,
    music_rate: f32,
) -> Vec<&'static str> {
    score_invalid_reason_lines_for_options(
        chart,
        ScoreValidityOptions {
            chart_effects: chart_effects_from_profile(profile),
            attack_mode: gameplay_attack_mode(profile.attack_mode),
            music_rate,
        },
    )
}

pub use arrowcloud::{
    arrowcloud_next_retry_is_auto, arrowcloud_next_retry_remaining_secs,
    get_arrowcloud_submit_ui_status_for_side, retry_arrowcloud_submit,
    submit_arrowcloud_payloads_from_gameplay, tick_arrowcloud_auto_retries,
};
use deadsync_score::{
    AcScoreCacheState, ArrowCloudLeaderboard, ArrowCloudScores, CachedPlayerLeaderboardData,
    CachedScore, CachedScoreImportResult, GameplayScoreboxProfileSnapshot,
    GrooveStatsSubmitRecordBanner, GsLampChartStats, GsScoreCacheState, HeldScoreCaches,
    ImportedPlayerScore, LeaderboardEntry, LocalReplayEdge, LocalScalarScore, LocalScoreCacheState,
    LocalScoreEntry, LocalScoreGameplayEntryInput, LocalScoreIndex, LocalScoreProfileSource,
    MachineLocalScoreCacheState, MachineReplayEntry, PlayerLeaderboardCacheEntry,
    PlayerLeaderboardCacheKey, PlayerLeaderboardCacheState, PlayerLeaderboardCacheValue,
    ScoreBulkImportSummary, ScoreImportEndpoint, ScoreImportProgress, ScoreIndexWriteError,
    ScoreStoreWriteError, ScoreStoreWriteStatus, arrowcloud_bulk_failure_detail,
    arrowcloud_bulk_success_detail, best_cached_itg_score, best_gs_scores_from_dir,
    cached_missing_gs_score, cached_player_leaderboard_itl_self_rank,
    cached_player_leaderboard_srpg_self_score, cached_score_from_leaderboard_import,
    cached_score_from_local_header, cached_score_import_result_from_imported,
    collect_chart_hashes_per_pack_for_import as score_collect_chart_hashes_per_pack_for_import,
    empty_score_import_summary, fix_gs_cached_score,
    load_ac_score_index_file as score_load_ac_score_index_file,
    load_gs_score_index_file as score_load_gs_score_index_file,
    load_local_score_index_file as score_load_local_score_index_file,
    load_local_score_index_from_root, local_score_entry_from_gameplay_input,
    local_score_entry_from_stage_summary, local_score_shard_dir, lua_chart_submit_allowed,
    machine_best_itg_from_profiles, machine_leaderboard_local_from_profiles,
    machine_replays_local_from_profiles, personal_leaderboard_local_from_root,
    played_chart_counts_in_profiles_root, played_chart_counts_in_root,
    player_leaderboard_cache_key, player_leaderboard_request_was_invalidated,
    queued_score_import_progress, recent_played_chart_hashes_in_profiles_root,
    recent_played_chart_hashes_in_root, save_ac_score_index_file as score_save_ac_score_index_file,
    save_gs_score_index_file as score_save_gs_score_index_file,
    save_local_score_index_file as score_save_local_score_index_file, score_file_shard,
    score_import_pack_complete_detail, score_import_pack_detail, score_import_summary,
    scorebox_snapshot, should_keep_newer_player_leaderboard_entry,
    should_log_score_import_progress, should_replace_cached_gs_score,
    total_local_score_bins_in_root, update_local_score_index, wait_for_next_score_import_request,
    write_gs_score_entry_file, write_local_score_entry_file,
};
pub use deadsync_score::{Grade, gameplay_run_failed, gameplay_run_passed};
use groovestats::{GrooveStatsSubmitPlayerJob, groovestats_judgment_counts};
pub use groovestats::{
    get_groovestats_submit_event_progress_for_side, get_groovestats_submit_record_banner_for_side,
    get_groovestats_submit_ui_status_for_side, groovestats_eval_state_from_gameplay,
    groovestats_next_retry_is_auto, groovestats_next_retry_remaining_secs,
    retry_groovestats_submit, submit_groovestats_payloads_from_gameplay,
    tick_groovestats_auto_retries,
};
pub use itl::{
    ensure_itl_wheel_caches_loaded, get_cached_itl_score_for_side, get_cached_itl_score_for_song,
    get_cached_itl_self_score_for_side, get_cached_itl_tournament_overall_ranks_for_side,
    get_cached_itl_tournament_rank_for_side, get_or_fetch_itl_self_score_for_side,
    get_or_fetch_itl_tournament_rank_for_side, import_itl_json,
    is_itl_song_folder_unlocked_for_side, is_itl_song_folder_unlocked_with_profile,
    is_itl_unlocks_pack, itl_eval_state_from_gameplay, itl_points_for_chart,
    save_itl_data_from_gameplay, seed_session_itl_unlock_folders,
    seed_session_online_itl_self_rank, seed_session_online_itl_self_score,
    should_warn_cmod_for_itl_chart,
};

// --- GrooveStats grade cache (on-disk + network-fetched) ---

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

fn warn_score_index_write_error(kind: &str, error: ScoreIndexWriteError) {
    match error {
        ScoreIndexWriteError::CreateDir { dir, error } => {
            warn!("Failed to create {kind} score index dir {dir:?}: {error}");
        }
        ScoreIndexWriteError::Encode { path } => {
            warn!("Failed to encode {kind} score index at {path:?}");
        }
        ScoreIndexWriteError::WriteTemp { tmp_path, error } => {
            warn!("Failed to write {kind} score index temp file {tmp_path:?}: {error}");
        }
        ScoreIndexWriteError::Commit {
            path,
            tmp_path: _,
            error,
        } => {
            warn!("Failed to commit {kind} score index file {path:?}: {error}");
        }
    }
}

fn load_gs_score_index(path: &Path) -> Option<HashMap<String, CachedScore>> {
    let (by_chart, changed) = score_load_gs_score_index_file(path)?;
    if changed {
        save_gs_score_index(path, &by_chart);
    }
    Some(by_chart)
}

fn save_gs_score_index(path: &Path, by_chart: &HashMap<String, CachedScore>) {
    if let Err(error) = score_save_gs_score_index_file(path, by_chart) {
        warn_score_index_write_error("GS", error);
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
        let scanned = best_gs_scores_from_dir(&gs_scores_dir_for_profile(profile_id));
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
        .get_profile_itg_score(profile_id, chart_hash)
}

fn get_cached_gs_score_for_profile(profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
    if profile_id.trim().is_empty() {
        return None;
    }
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_score(profile_id, chart_hash)
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
        .get_profile_score(&profile_id, chart_hash)
}

fn cached_gs_chart_hashes_for_profile(profile_id: &str) -> HashSet<String> {
    if profile_id.trim().is_empty() {
        return HashSet::new();
    }
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .profile_chart_hashes(profile_id)
}

fn set_cached_gs_score_for_profile(profile_id: &str, chart_hash: String, score: CachedScore) {
    let score = fix_gs_cached_score(score);
    debug!("Caching GrooveStats score {score:?} for chart hash {chart_hash}");
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    let snapshot = {
        let mut state = GS_SCORE_CACHE.lock().unwrap();
        let Some(snapshot) = state.set_profile_score(profile_id, chart_hash, score) else {
            return;
        };
        snapshot
    };
    save_gs_score_index(&gs_score_index_path_for_profile(profile_id), &snapshot);
}

// --- ArrowCloud score cache (on-disk, all 3 leaderboards per chart) ---

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
    score_load_ac_score_index_file(path)
}

fn save_ac_score_index(path: &Path, by_chart: &HashMap<String, ArrowCloudScores>) {
    if let Err(error) = score_save_ac_score_index_file(path, by_chart) {
        warn_score_index_write_error("AC", error);
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
        .get_profile_scores(profile_id, chart_hash)
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
        .profile_chart_hashes_with_itg(profile_id)
}

/// Bulk-write multiple AC score entries with a single index save.
fn set_cached_ac_scores_for_profile_bulk(
    profile_id: &str,
    entries: impl IntoIterator<Item = (String, ArrowCloudScores)>,
) {
    ensure_ac_score_cache_loaded_for_profile(profile_id);
    let snapshot = {
        let mut state = AC_SCORE_CACHE.lock().unwrap();
        let Some(snapshot) = state.set_profile_scores_bulk(profile_id, entries) else {
            return;
        };
        snapshot
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
    ensure_ac_score_cache_loaded_for_profile(profile_id);
    let merged = {
        let mut state = AC_SCORE_CACHE.lock().unwrap();
        let Some(merged) = state.merge_profile_submit_scores(
            profile_id,
            chart_hash,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail,
            submitted_at,
        ) else {
            return;
        };
        merged
    };
    save_ac_score_index(&ac_score_index_path_for_profile(profile_id), &merged);
}

// --- Local score cache (on-disk, one file per play) ---

static LOCAL_SCORE_CACHE: std::sync::LazyLock<Mutex<LocalScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(LocalScoreCacheState::default()));

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
    score_load_local_score_index_file(path)
}

fn save_local_score_index_file(path: &Path, index: &LocalScoreIndex) {
    if let Err(error) = score_save_local_score_index_file(path, index) {
        warn_score_index_write_error("local", error);
    }
}

/// Total number of locally-saved plays for this profile (one `.bin` per play).
pub fn total_songs_played_for_profile(profile_id: &str) -> u32 {
    total_local_score_bins_in_root(&local_scores_root_for_profile(profile_id))
}

pub fn total_songs_played_for_side(side: profile_data::PlayerSide) -> u32 {
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return 0;
    };
    total_songs_played_for_profile(&profile_id)
}

/// Returns chart hashes ordered by latest local play time (most recent first),
/// aggregated across all local profiles.
pub fn recent_played_chart_hashes_for_machine() -> Vec<String> {
    recent_played_chart_hashes_in_profiles_root(&dirs::app_dirs().profiles_root())
}

/// Returns `(chart_hash, play_count)` pairs ordered by play count descending,
/// aggregated across all local profiles.
pub fn played_chart_counts_for_machine() -> Vec<(String, u32)> {
    played_chart_counts_in_profiles_root(&dirs::app_dirs().profiles_root())
}

/// Returns chart hashes ordered by latest local play time for a single profile.
pub fn recent_played_chart_hashes_for_profile(profile_id: &str) -> Vec<String> {
    recent_played_chart_hashes_in_root(&local_scores_root_for_profile(profile_id))
}

/// Returns `(chart_hash, play_count)` pairs for a single profile, ordered by
/// play count descending.
pub fn played_chart_counts_for_profile(profile_id: &str) -> Vec<(String, u32)> {
    played_chart_counts_in_root(&local_scores_root_for_profile(profile_id))
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
    let loaded = load_local_score_index_from_root(&local_scores_root_for_profile(profile_id));
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
    let best_itg = machine_best_itg_from_profiles(&local_score_profile_sources());

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
    MACHINE_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .update_if_loaded(chart_hash, score, initials);
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
        .get_profile_itg_score(&profile_id, chart_hash)
}

pub fn get_cached_local_pass_rate_with_profile(chart_hash: &str, profile_id: &str) -> Option<u32> {
    if profile_id.trim().is_empty() {
        return None;
    }
    ensure_local_score_cache_loaded(profile_id);
    LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_pass_rate(profile_id, chart_hash)
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
    best_cached_itg_score([local, gs, ac])
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
    HeldScoreCaches::new(
        LOCAL_SCORE_CACHE.lock().unwrap(),
        GS_SCORE_CACHE.lock().unwrap(),
        AC_SCORE_CACHE.lock().unwrap(),
    )
}

/// Test/bench helper: seed the in-memory local ITG score cache for a profile
/// without touching disk. Lets benchmarks exercise the wheel grade/lamp render
/// path deterministically.
pub fn seed_session_local_itg_score(profile_id: &str, chart_hash: &str, score: CachedScore) {
    ensure_local_score_cache_loaded(profile_id);
    LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .seed_profile_itg_score(profile_id, chart_hash, score);
}

/// Test/bench helper: seed the in-memory GrooveStats grade cache for a profile
/// without touching disk. See [`seed_session_local_itg_score`].
pub fn seed_session_gs_score(profile_id: &str, chart_hash: &str, score: CachedScore) {
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .seed_profile_score(profile_id, chart_hash, score);
}

fn get_cached_local_scalar_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    hard_ex: bool,
) -> Option<LocalScalarScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_local_score_cache_loaded(&profile_id);
    LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_scalar_score(&profile_id, chart_hash, hard_ex)
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
    MACHINE_LOCAL_SCORE_CACHE.lock().unwrap().record(chart_hash)
}

fn warn_score_store_write_error(kind: &str, chart_hash: &str, error: ScoreStoreWriteError) {
    match error {
        ScoreStoreWriteError::CreateDir { dir, error } => {
            warn!("Failed to create {kind} score dir {dir:?}: {error}");
        }
        ScoreStoreWriteError::Encode { .. } => {
            warn!("Failed to encode {kind} score for chart {chart_hash}");
        }
        ScoreStoreWriteError::WriteFile { path, error } => {
            warn!("Failed to write {kind} score file {path:?}: {error}");
        }
        ScoreStoreWriteError::CommitFile {
            path,
            tmp_path: _,
            error,
        } => {
            warn!("Failed to commit {kind} score file {path:?}: {error}");
        }
    }
}

// --- On-disk GrooveStats score storage ---

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
    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    match write_gs_score_entry_file(&dir, chart_hash, score, username, fetched_at_ms) {
        Ok(ScoreStoreWriteStatus::SkippedDuplicate) => {}
        Ok(ScoreStoreWriteStatus::Written(path)) => {
            debug!("Stored GrooveStats score on disk for chart {chart_hash} at {path:?}");
        }
        Err(error) => {
            warn_score_store_write_error("GrooveStats", chart_hash, error);
        }
    }
}

// --- On-disk local score storage (one file per play) ---

fn append_local_score_on_disk(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut LocalScoreEntry,
) -> bool {
    let root = local_scores_root_for_profile(profile_id);
    let dir = local_score_shard_dir(&root, chart_hash);
    if let Err(error) = write_local_score_entry_file(&dir, chart_hash, entry) {
        warn_score_store_write_error("local", chart_hash, error);
        return false;
    };

    // Update in-memory cache if it's already loaded for this profile.
    let header = entry.header();
    let loaded_snapshot = {
        let mut state = LOCAL_SCORE_CACHE.lock().unwrap();
        state.update_loaded_profile_index(profile_id, chart_hash, &header)
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
/// DeadSync chart `short_hash` and the play to record. Returns `(written,
/// canceled)`: the number of plays successfully written to disk, and whether the
/// loop stopped early because `should_cancel` returned `true`.
///
/// This reuses the same per-play write path as live gameplay, so the per-profile
/// best index and any loaded in-memory caches stay correct.
///
/// `on_progress(done, total)` is invoked after each play is processed (whether or
/// not it was written), so a caller can drive a progress bar over the disk-write
/// phase, which dominates import time for large histories. `should_cancel()` is
/// polled before each write so a long import can be aborted promptly. Pass no-op
/// closures when progress / cancellation aren't needed.
pub fn import_local_scores<F, C>(
    profile_id: &str,
    profile_initials: &str,
    scores: &mut [(String, LocalScoreEntry)],
    mut on_progress: F,
    should_cancel: C,
) -> (usize, bool)
where
    F: FnMut(usize, usize),
    C: Fn() -> bool,
{
    let total = scores.len();
    let mut written = 0usize;
    for (idx, (chart_hash, entry)) in scores.iter_mut().enumerate() {
        if should_cancel() {
            return (written, true);
        }
        if append_local_score_on_disk(profile_id, profile_initials, chart_hash, entry) {
            written += 1;
        }
        on_progress(idx + 1, total);
    }
    (written, false)
}

fn judgment_counts_arr(p: &PlayerRuntime) -> [u32; 6] {
    p.judgment_counts
}

fn replay_edges_for_player(gs: &GameplayCoreState, player: usize) -> Vec<LocalReplayEdge> {
    if player >= gs.num_players() {
        return Vec::new();
    }

    let (col_start, col_end) = if gs.num_players() <= 1 {
        (0usize, gs.num_cols())
    } else {
        let start = player.saturating_mul(gs.cols_per_player());
        (start, start.saturating_add(gs.cols_per_player()))
    };

    let mut out = Vec::new();
    let replay_edges = gs.recorded_replay_edges();
    out.reserve(replay_edges.len().min(4096));
    for e in replay_edges {
        let lane = e.lane_index as usize;
        if lane < col_start
            || lane >= col_end
            || deadsync_core::song_time::song_time_ns_invalid(e.event_music_time_ns)
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

pub fn save_local_scores_from_gameplay(gs: &GameplayCoreState) {
    if gs.autoplay_used() {
        debug!("Skipping local score save: autoplay was used during this stage.");
        return;
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    for player_idx in 0..gs.num_players() {
        let side = if gs.num_players() >= 2 {
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
        if !gs.score_valid_for_player(player_idx) {
            let reasons = score_invalid_reason_lines_for_profile(
                &gs.charts()[player_idx],
                &gs.profiles()[player_idx],
                gs.music_rate(),
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

        let chart_hash = gs.charts()[player_idx].short_hash.as_str();
        let p = &gs.players()[player_idx];
        let totals = gs.display_totals_for_player(player_idx);

        let score_percent = judgment::calculate_itg_score_percent_from_counts(
            &p.scoring_counts,
            p.holds_held_for_score,
            p.rolls_held_for_score,
            p.mines_hit_for_score,
            totals.possible_grade_points,
        );

        let (start, end) = gs.note_range_for_player(player_idx);
        let counts = judgment_counts_arr(p);
        let replay = replay_edges_for_player(gs, player_idx);

        let mut entry = local_score_entry_from_gameplay_input(LocalScoreGameplayEntryInput {
            played_at_ms: now_ms,
            music_rate: gs.music_rate(),
            score_percent,
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: p.is_failing,
            life: p.life,
            fail_time: p.fail_time,
            notes: &gs.notes()[start..end],
            note_times: &gs.note_time_cache_ns()[start..end],
            hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
            total_steps: totals.total_steps,
            holds_total: totals.holds_total,
            rolls_total: totals.rolls_total,
            mines_total: totals.mines_total,
            counts,
            white_fantastics: Some(gs.live_window_counts(player_idx).w1),
            holds_held: p.holds_held,
            rolls_held: p.rolls_held,
            mines_avoided: p.mines_avoided,
            hands_achieved: p.hands_achieved,
            beat0_time_ns: gs
                .timing_for_player(player_idx)
                .map(|timing| timing.get_time_for_beat_ns(0.0))
                .unwrap_or(0),
            replay,
        });

        append_local_score_on_disk(
            &profile_id,
            gs.profiles()[player_idx].player_initials.as_str(),
            chart_hash,
            &mut entry,
        );
    }
}

#[inline(always)]
pub(super) fn gameplay_side_for_player(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> profile_data::PlayerSide {
    if gs.num_players() >= 2 {
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
    let mut entry = local_score_entry_from_stage_summary(now_ms, music_rate, summary);
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

static PLAYER_LEADERBOARD_CACHE: std::sync::LazyLock<Mutex<PlayerLeaderboardCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(PlayerLeaderboardCacheState::default()));

const PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL: Duration = Duration::from_secs(10);
const EVENT_WHEEL_FETCH_ENTRIES: usize = 5;

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
    let stats = imported
        .as_ref()
        .and_then(|score| chart_stats_for_imported_score(score, chart_hash));
    let Some(cached) = cached_score_from_leaderboard_import(
        imported,
        get_cached_local_score_for_profile(profile_id, chart_hash),
        stats,
    ) else {
        // Select Music and gameplay scoreboxes fetch shallow leaderboard pages.
        // If the page does not include the player's row, do not clobber an
        // existing cached GS score and make the wheel fall back to local data.
        return;
    };
    cache_gs_score_for_profile(
        profile_id,
        chart_hash,
        cached.score,
        username,
        cached.score_proves_nonquint_ex,
    );
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

fn get_cached_player_leaderboard_itl_self_rank_with(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    let cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
    cached_player_leaderboard_itl_self_rank(&cache.by_key, chart_hash, profile_snapshot)
}

fn get_cached_player_leaderboard_srpg_self_score_with(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    let cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
    cached_player_leaderboard_srpg_self_score(&cache.by_key, chart_hash, profile_snapshot)
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

    /// Cached SRPG event score for the active GrooveStats event leaderboard.
    pub fn cached_srpg_self_score(&self, chart_hash: &str) -> Option<u32> {
        get_cached_player_leaderboard_srpg_self_score_with(chart_hash, &self.leaderboard_snapshot)
    }

    pub fn get_or_fetch_srpg_self_score(&self, chart_hash: &str) -> Option<u32> {
        if let Some(score) = self.cached_srpg_self_score(chart_hash) {
            return Some(score);
        }
        let _ = get_or_fetch_player_leaderboards_for_profile_inner(
            chart_hash,
            &self.leaderboard_snapshot,
            EVENT_WHEEL_FETCH_ENTRIES,
            false,
        )?;
        self.cached_srpg_self_score(chart_hash)
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

    let request = {
        let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
        cache.request_leaderboard(&key, max_entries, refresh_cached, Instant::now())
    };

    if request.should_spawn {
        spawn_player_leaderboard_fetch(
            key,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
            request.requested_max_entries,
        );
    }

    Some(request.snapshot)
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

    PLAYER_LEADERBOARD_CACHE
        .lock()
        .unwrap()
        .invalidate_chart_for_api(gs_api_key, chart_hash, Instant::now());
}

fn local_score_profile_sources() -> Vec<LocalScoreProfileSource> {
    profile::scan_local_profiles()
        .into_iter()
        .map(|profile_meta| {
            let initials =
                profile_initials_for_id(&profile_meta.id).unwrap_or_else(|| "----".to_string());
            LocalScoreProfileSource {
                root: local_scores_root_for_profile(&profile_meta.id),
                initials,
                display_name: profile_meta.display_name,
            }
        })
        .collect()
}

pub fn get_machine_leaderboard_local(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    machine_leaderboard_local_from_profiles(
        &local_score_profile_sources(),
        chart_hash,
        max_entries,
        false,
    )
}

pub fn get_machine_leaderboard_local_with_names(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    machine_leaderboard_local_from_profiles(
        &local_score_profile_sources(),
        chart_hash,
        max_entries,
        true,
    )
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
    personal_leaderboard_local_from_root(&root, chart_hash, &initials, max_entries)
}

pub fn get_machine_replays_local(chart_hash: &str, max_entries: usize) -> Vec<MachineReplayEntry> {
    machine_replays_local_from_profiles(&local_score_profile_sources(), chart_hash, max_entries)
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
    let song_cache = get_song_cache();
    score_collect_chart_hashes_per_pack_for_import(
        &song_cache,
        pack_groups_filter,
        &existing_scores,
    )
}

/// AC-specific bulk import orchestrator. Sends one POST per pack to
/// `/v1/retrieve-scores`, splitting any pack with > [`ARROWCLOUD_BULK_MAX_HASHES`]
/// charts into chunks. Throttled to the shared score-import rate limit to match
/// the per-chart paths.
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
    let song_cache = get_song_cache();
    let pack_chart_groups =
        score_collect_chart_hashes_per_pack_for_import(&song_cache, &pack_groups, &existing_scores);
    let requested_charts: usize = pack_chart_groups.iter().map(|(_, h)| h.len()).sum();
    let total_packs = pack_chart_groups.len();
    on_progress(queued_score_import_progress(
        requested_charts,
        total_packs,
        "ArrowCloud bulk",
        only_missing_scores,
    ));
    if requested_charts == 0 {
        return Ok(empty_score_import_summary());
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
            wait_for_next_score_import_request(last_request_started_at);
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
                    arrowcloud_bulk_success_detail(
                        pack_idx,
                        total_packs,
                        &pack_name,
                        hits,
                        misses,
                        request_elapsed,
                    )
                }
                Err(e) => {
                    let request_elapsed = request_started.elapsed();
                    failed_requests += 1;
                    pack_failures += 1;
                    let msg = arrowcloud_bulk_failure_detail(
                        pack_idx,
                        total_packs,
                        &pack_name,
                        chunk_vec.len(),
                        request_elapsed,
                        &e.to_string(),
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
            "{}",
            score_import_pack_complete_detail(
                pack_idx,
                total_packs,
                &pack_name,
                pack_chart_count,
                pack_hits,
                pack_misses,
                pack_failures,
            )
        );
    }

    Ok(score_import_summary(
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        import_started,
        canceled,
    ))
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
    on_progress(queued_score_import_progress(
        requested_charts,
        total_packs,
        endpoint.display_name(),
        only_missing_gs_scores,
    ));
    if requested_charts == 0 {
        return Ok(empty_score_import_summary());
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
            wait_for_next_score_import_request(last_request_started_at);
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
            let detail = score_import_pack_detail(
                pack_idx,
                total_packs,
                &pack_name,
                pack_hits,
                pack_misses,
                pack_failures,
            );
            on_progress(ScoreImportProgress {
                processed_charts,
                total_charts: requested_charts,
                imported_scores,
                missing_scores,
                failed_requests,
                detail,
            });
            if should_log_score_import_progress(processed_charts, requested_charts) {
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
            "{}",
            score_import_pack_complete_detail(
                pack_idx,
                total_packs,
                &pack_name,
                pack_chart_count,
                pack_hits,
                pack_misses,
                pack_failures,
            )
        );
    }

    Ok(score_import_summary(
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        import_started,
        canceled,
    ))
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
            deadsync_score::compute_local_lamp([12, 0, 0, 0, 0, 0], Grade::Tier01, Some(5)),
            (Some(1), Some(5))
        );
        assert_eq!(
            deadsync_score::compute_local_lamp([12, 0, 0, 0, 0, 0], Grade::Quint, Some(0)),
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
    fn player_leaderboard_cache_exposes_srpg_self_score() {
        let snapshot = scorebox_snapshot(
            true,
            false,
            true,
            true,
            false,
            false,
            "gs-key",
            "",
            "PerfectTaste",
            Some("profile-1".to_string()),
        );
        let key = player_leaderboard_cache_key("deadbeef", &snapshot).expect("cache key");
        {
            let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
            cache.by_key.insert(
                key.clone(),
                PlayerLeaderboardCacheEntry {
                    value: PlayerLeaderboardCacheValue::Ready(
                        deadsync_score::PlayerLeaderboardData {
                            panes: Vec::new(),
                            srpg_self_score: Some(9_910),
                            itl_self_score: None,
                            itl_self_rank: None,
                        },
                    ),
                    max_entries: 5,
                    refreshed_at: Instant::now(),
                    retry_after: None,
                },
            );
        }

        assert_eq!(
            get_cached_player_leaderboard_srpg_self_score_with("deadbeef", &snapshot),
            Some(9_910)
        );

        PLAYER_LEADERBOARD_CACHE.lock().unwrap().by_key.remove(&key);
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
