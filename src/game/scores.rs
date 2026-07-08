use super::GameplayCoreState;
use crate::game::profile;
use deadlib_platform::dirs;
use deadsync_profile::Profile;
use deadsync_score::stage_stats;
use deadsync_simfile::runtime_cache::get_song_cache;
use log::{debug, warn};
use std::collections::HashSet;
use std::error::Error;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use deadsync_online::arrowcloud::{self as arrowcloud_api, ARROWCLOUD_BULK_MAX_HASHES};
use deadsync_online::boxed_request_error;
use deadsync_online::groovestats::{self as groovestats_api, GrooveStatsSubmitApiPlayer};
use deadsync_profile as profile_data;
use deadsync_profile_gameplay::score_invalid_reason_lines_for_profile;
use deadsync_rules::judgment;

mod arrowcloud;
mod groovestats;
mod itl;

#[inline(always)]
fn active_groovestats_service() -> groovestats_api::Service {
    crate::game::online::active_groovestats_service()
}

pub use arrowcloud::{
    arrowcloud_next_retry_is_auto, arrowcloud_next_retry_remaining_secs,
    get_arrowcloud_submit_ui_status_for_side, retry_arrowcloud_submit,
    submit_arrowcloud_payloads_from_gameplay, tick_arrowcloud_auto_retries,
};
use deadsync_score::{
    ArrowCloudBulkChunkResult, ArrowCloudLeaderboard, ArrowCloudScores,
    CachedPlayerLeaderboardData, CachedScore, CachedScoreImportResult,
    GameplayScoreboxProfileSnapshot, GrooveStatsSubmitRecordBanner, HeldScoreCaches,
    ImportedPlayerScore, LeaderboardEntry, LocalReplayEdgeInput, LocalScalarScore, LocalScoreEntry,
    LocalScoreGameplayEntryInput, MachineReplayEntry, PlayerLeaderboardFetchRequest,
    PlayerLeaderboardFetchSuccess, ScoreBulkImportSummary, ScoreCacheLoadKind,
    ScoreCacheLoadReport, ScoreCacheRuntimeKind, ScoreCacheRuntimeResult, ScoreImportEndpoint,
    ScoreImportProgress, ScoreIndexWriteError, ScoreProfilePaths, ScoreStoreWriteError,
    ScoreStoreWriteStatus, cached_missing_gs_score, cached_score_from_leaderboard_import,
    cached_score_import_result_from_imported,
    collect_chart_hashes_per_pack_for_import as score_collect_chart_hashes_per_pack_for_import,
    import_local_scores_with_writer, imported_score_chart_stats, local_replay_edges_for_player,
    local_score_entry_from_gameplay_input, local_score_entry_from_stage_summary,
    lua_chart_submit_allowed, machine_leaderboard_local_from_profiles,
    machine_replays_local_from_profiles, personal_leaderboard_local_from_root,
    run_arrowcloud_bulk_import_pack_groups, run_score_import_pack_groups,
    runtime_append_local_score_for_profile, runtime_cache_gs_score_for_profile,
    runtime_cached_player_leaderboard_itl_self_rank,
    runtime_cached_player_leaderboard_srpg_self_score, runtime_complete_player_leaderboard_fetch,
    runtime_ensure_profile_score_caches_loaded,
    runtime_invalidate_player_leaderboard_chart_for_api, runtime_lock_score_caches,
    runtime_machine_record_local, runtime_plan_player_leaderboard_request,
    runtime_played_chart_counts_for_machine, runtime_played_chart_counts_for_profile,
    runtime_prewarm_select_music_score_caches, runtime_read_ac_chart_hashes_with_itg_for_profile,
    runtime_read_ac_scores_for_profile, runtime_read_best_itg_score_for_profile,
    runtime_read_gs_chart_hashes_for_profile, runtime_read_gs_score_for_profile,
    runtime_read_local_itg_score_for_profile, runtime_read_local_pass_rate_for_profile,
    runtime_read_local_scalar_score_for_profile, runtime_recent_played_chart_hashes_for_machine,
    runtime_recent_played_chart_hashes_for_profile, runtime_seed_gs_score_access,
    runtime_seed_local_itg_score_access, runtime_total_songs_played_for_profile,
    runtime_update_machine_cache_if_loaded, runtime_write_ac_scores_for_profile_bulk,
    runtime_write_ac_submit_scores, runtime_write_gs_score_for_profile,
};
use deadsync_score::{ArrowCloudBulkImportRunEvent, ScoreImportRunEvent};
pub use deadsync_score::{Grade, gameplay_run_failed, gameplay_run_passed};
#[cfg(test)]
use deadsync_score::{
    player_leaderboard_cache_key, runtime_lock_player_leaderboard_cache,
    runtime_remove_player_leaderboard_entry, runtime_seed_player_leaderboard_entry,
};
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

fn score_paths_for_profile(profile_id: &str) -> ScoreProfilePaths {
    ScoreProfilePaths::new(profile::local_profile_dir_for_id(profile_id))
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

fn log_score_cache_load(report: ScoreCacheLoadReport) {
    if report.elapsed_ms < 25.0 {
        return;
    }
    match report.kind {
        ScoreCacheLoadKind::GrooveStats => {
            let profile_id = report.profile_id.unwrap_or_default();
            let loaded_entries = report.primary_entries;
            let load_ms = report.elapsed_ms;
            debug!(
                "Loaded GrooveStats score cache for profile {profile_id}: {loaded_entries} chart(s) in {load_ms:.2}ms."
            );
        }
        ScoreCacheLoadKind::Local => {
            let profile_id = report.profile_id.unwrap_or_default();
            let loaded_itg = report.primary_entries;
            let loaded_ex = report.secondary_entries;
            let load_ms = report.elapsed_ms;
            debug!(
                "Loaded local score cache for profile {profile_id}: ITG={loaded_itg}, EX={loaded_ex} in {load_ms:.2}ms."
            );
        }
        ScoreCacheLoadKind::MachineLocal => {
            let total = report.primary_entries;
            let load_ms = report.elapsed_ms;
            debug!("Loaded machine local score cache: {total} chart(s) in {load_ms:.2}ms.");
        }
    }
}

fn handle_score_cache_result(kind: &str, result: ScoreCacheRuntimeResult) {
    for error in result.write_errors {
        warn_score_index_write_error(kind, error);
    }
    if let Some(report) = result.load_report {
        log_score_cache_load(report);
    }
}

fn score_cache_kind_label(kind: ScoreCacheRuntimeKind) -> &'static str {
    match kind {
        ScoreCacheRuntimeKind::GrooveStats => "GS",
        ScoreCacheRuntimeKind::ArrowCloud => "AC",
        ScoreCacheRuntimeKind::Local | ScoreCacheRuntimeKind::MachineLocal => "local",
    }
}

fn handle_score_cache_results(
    results: impl IntoIterator<Item = (ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)>,
) {
    for (kind, result) in results {
        handle_score_cache_result(score_cache_kind_label(kind), result);
    }
}

fn get_cached_local_score_for_profile(profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
    let access =
        runtime_read_local_itg_score_for_profile(profile_id, chart_hash, score_paths_for_profile);
    handle_score_cache_results(access.results);
    access.value
}

fn get_cached_gs_score_for_profile(profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
    let access = runtime_read_gs_score_for_profile(profile_id, chart_hash, score_paths_for_profile);
    handle_score_cache_results(access.results);
    access.value
}

pub fn get_cached_gs_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<CachedScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    get_cached_gs_score_for_profile(&profile_id, chart_hash)
}

fn cached_gs_chart_hashes_for_profile(profile_id: &str) -> HashSet<String> {
    let access = runtime_read_gs_chart_hashes_for_profile(profile_id, score_paths_for_profile);
    handle_score_cache_results(access.results);
    access.value
}

fn set_cached_gs_score_for_profile(profile_id: &str, chart_hash: String, score: CachedScore) {
    debug!("Caching GrooveStats score {score:?} for chart hash {chart_hash}");
    handle_score_cache_results(runtime_write_gs_score_for_profile(
        profile_id,
        chart_hash,
        score,
        score_paths_for_profile,
    ));
}

// --- ArrowCloud score cache (on-disk, all 3 leaderboards per chart) ---

fn get_cached_ac_scores_for_profile(
    profile_id: &str,
    chart_hash: &str,
) -> Option<ArrowCloudScores> {
    let access =
        runtime_read_ac_scores_for_profile(profile_id, chart_hash, score_paths_for_profile);
    handle_score_cache_results(access.results);
    access.value
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
    let access =
        runtime_read_ac_chart_hashes_with_itg_for_profile(profile_id, score_paths_for_profile);
    handle_score_cache_results(access.results);
    access.value
}

/// Bulk-write multiple AC score entries with a single index save.
fn set_cached_ac_scores_for_profile_bulk(
    profile_id: &str,
    entries: impl IntoIterator<Item = (String, ArrowCloudScores)>,
) {
    handle_score_cache_results(runtime_write_ac_scores_for_profile_bulk(
        profile_id,
        entries,
        score_paths_for_profile,
    ));
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
    handle_score_cache_results(runtime_write_ac_submit_scores(
        profile_id,
        chart_hash,
        itg_percent,
        ex_percent,
        hard_ex_percent,
        is_fail,
        submitted_at,
        score_paths_for_profile,
    ));
}

/// Total number of locally-saved plays for this profile (one `.bin` per play).
pub fn total_songs_played_for_profile(profile_id: &str) -> u32 {
    runtime_total_songs_played_for_profile(profile_id, score_paths_for_profile)
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
    runtime_recent_played_chart_hashes_for_machine(&dirs::app_dirs().profiles_root())
}

/// Returns `(chart_hash, play_count)` pairs ordered by play count descending,
/// aggregated across all local profiles.
pub fn played_chart_counts_for_machine() -> Vec<(String, u32)> {
    runtime_played_chart_counts_for_machine(&dirs::app_dirs().profiles_root())
}

/// Returns chart hashes ordered by latest local play time for a single profile.
pub fn recent_played_chart_hashes_for_profile(profile_id: &str) -> Vec<String> {
    runtime_recent_played_chart_hashes_for_profile(profile_id, score_paths_for_profile)
}

/// Returns `(chart_hash, play_count)` pairs for a single profile, ordered by
/// play count descending.
pub fn played_chart_counts_for_profile(profile_id: &str) -> Vec<(String, u32)> {
    runtime_played_chart_counts_for_profile(profile_id, score_paths_for_profile)
}

pub fn prewarm_select_music_score_caches() {
    let p1_profile_id = profile::active_local_profile_id_for_side(profile_data::PlayerSide::P1);
    let p2_profile_id = profile::active_local_profile_id_for_side(profile_data::PlayerSide::P2);
    let warmup = runtime_prewarm_select_music_score_caches(
        p1_profile_id.as_deref(),
        p2_profile_id.as_deref(),
        &profile::local_score_profile_sources(),
        score_paths_for_profile,
    );
    handle_score_cache_results(warmup.results);
    let elapsed_ms = warmup.elapsed_ms;
    debug!("Prewarmed SelectMusic score caches in {elapsed_ms:.2}ms.");
}

fn update_machine_cache_if_loaded(chart_hash: &str, score: CachedScore, initials: &str) {
    runtime_update_machine_cache_if_loaded(chart_hash, score, initials);
}

pub fn get_cached_local_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<CachedScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    get_cached_local_score_for_profile(&profile_id, chart_hash)
}

pub fn get_cached_local_pass_rate_with_profile(chart_hash: &str, profile_id: &str) -> Option<u32> {
    let access =
        runtime_read_local_pass_rate_for_profile(profile_id, chart_hash, score_paths_for_profile);
    handle_score_cache_results(access.results);
    access.value
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
    let access =
        runtime_read_best_itg_score_for_profile(profile_id, chart_hash, score_paths_for_profile);
    handle_score_cache_results(access.results);
    access.value
}

pub fn ensure_score_caches_loaded(profile_id: &str) {
    handle_score_cache_results(runtime_ensure_profile_score_caches_loaded(
        profile_id,
        score_paths_for_profile,
    ));
}

pub fn lock_score_caches() -> HeldScoreCaches {
    runtime_lock_score_caches()
}

/// Test/bench helper: seed the in-memory local ITG score cache for a profile
/// without touching disk. Lets benchmarks exercise the wheel grade/lamp render
/// path deterministically.
pub fn seed_session_local_itg_score(profile_id: &str, chart_hash: &str, score: CachedScore) {
    handle_score_cache_results(runtime_seed_local_itg_score_access(
        profile_id,
        chart_hash,
        score,
        score_paths_for_profile,
    ));
}

/// Test/bench helper: seed the in-memory GrooveStats grade cache for a profile
/// without touching disk. See [`seed_session_local_itg_score`].
pub fn seed_session_gs_score(profile_id: &str, chart_hash: &str, score: CachedScore) {
    handle_score_cache_results(runtime_seed_gs_score_access(
        profile_id,
        chart_hash,
        score,
        score_paths_for_profile,
    ));
}

fn get_cached_local_scalar_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    hard_ex: bool,
) -> Option<LocalScalarScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    let access = runtime_read_local_scalar_score_for_profile(
        &profile_id,
        chart_hash,
        hard_ex,
        score_paths_for_profile,
    );
    handle_score_cache_results(access.results);
    access.value
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
    let (record, result) =
        runtime_machine_record_local(chart_hash, &profile::local_score_profile_sources());
    handle_score_cache_result("local", result);
    record
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

// --- On-disk local score storage (one file per play) ---

fn append_local_score_on_disk(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut LocalScoreEntry,
) -> bool {
    let result = runtime_append_local_score_for_profile(
        profile_id,
        profile_initials,
        chart_hash,
        entry,
        score_paths_for_profile,
    );
    if let Some(error) = result.store_error {
        warn_score_store_write_error("local", chart_hash, error);
    }
    if let Some(error) = result.index_error {
        warn_score_index_write_error("local", error);
    }
    result.append.is_some()
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
    on_progress: F,
    should_cancel: C,
) -> (usize, bool)
where
    F: FnMut(usize, usize),
    C: Fn() -> bool,
{
    import_local_scores_with_writer(scores, on_progress, should_cancel, |chart_hash, entry| {
        append_local_score_on_disk(profile_id, profile_initials, chart_hash, entry)
    })
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
        let side = profile_data::side_for_gameplay_player(
            gs.num_players(),
            player_idx,
            profile::get_session_player_side(),
        );

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
        let replay = local_replay_edges_for_player(
            gs.recorded_replay_edges()
                .iter()
                .map(|e| LocalReplayEdgeInput {
                    event_music_time_ns: e.event_music_time_ns,
                    lane_index: e.lane_index,
                    pressed: e.pressed,
                    source: e.source,
                }),
            player_idx,
            gs.num_players(),
            gs.num_cols(),
            gs.cols_per_player(),
        );

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
            counts: p.judgment_counts,
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
    profile_data::side_for_gameplay_player(
        gs.num_players(),
        player_idx,
        profile::get_session_player_side(),
    )
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
    profile_data::scorebox_profile_snapshot(
        player_profile,
        side_joined,
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
        persistent_profile_id,
    )
}

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

fn cache_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
    proves_nonquint_ex: bool,
) {
    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let result = runtime_cache_gs_score_for_profile(
        profile_id,
        chart_hash,
        score,
        username,
        proves_nonquint_ex,
        fetched_at_ms,
        score_paths_for_profile,
    );
    handle_score_cache_result("GS", result.cache_result);
    if let Some(error) = result.store_error {
        warn_score_store_write_error("GrooveStats", chart_hash, error);
    }
    if let Some(ScoreStoreWriteStatus::Written(path)) = result.store_status {
        debug!("Stored GrooveStats score on disk for chart {chart_hash} at {path:?}");
    }
}

fn cache_gs_score_from_leaderboard(
    profile_id: &str,
    username: &str,
    chart_hash: &str,
    imported: Option<ImportedPlayerScore>,
) {
    let song_cache = get_song_cache();
    let stats = imported
        .as_ref()
        .and_then(|score| imported_score_chart_stats(score, &song_cache, chart_hash));
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

fn spawn_player_leaderboard_fetch(request: PlayerLeaderboardFetchRequest) {
    std::thread::spawn(move || {
        let PlayerLeaderboardFetchRequest {
            key,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
            max_entries: requested_max_entries,
        } = request;
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
        let fetched = fetched
            .map(
                |groovestats_api::FetchedPlayerLeaderboards {
                     data,
                     imported_score,
                     itl_self_found,
                 }| PlayerLeaderboardFetchSuccess {
                    data,
                    imported_score,
                    itl_self_found,
                },
            )
            .map_err(|error| error.to_string());
        let completion = runtime_complete_player_leaderboard_fetch(
            &key,
            requested_max_entries,
            request_started_at,
            refresh_finished_at,
            PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
            fetched,
            should_auto_populate,
            auto_profile_id.is_some(),
        );

        // Keep score/ITL cache writes outside the leaderboard cache. The wheel
        // can hold score-cache guards while probing leaderboard rank state.
        if let Some((itl_self_score, itl_self_rank)) = completion.fetched_itl_self {
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
        if let Some(imported_score) = completion.fetched_imported_score
            && let Some(profile_id) = auto_profile_id.as_deref()
        {
            cache_gs_score_from_leaderboard(
                profile_id,
                gs_username.as_str(),
                key.chart_hash.as_str(),
                Some(imported_score),
            );
        }

        if let Some(queued_fetch) = completion.queued_fetch {
            spawn_player_leaderboard_fetch(PlayerLeaderboardFetchRequest {
                key: queued_fetch.key,
                gs_username,
                persistent_profile_id,
                auto_profile_id,
                should_auto_populate,
                max_entries: queued_fetch.max_entries,
            });
        }
    });
}

#[inline(always)]
fn player_leaderboard_profile_snapshot_for_side(
    side: profile_data::PlayerSide,
) -> GameplayScoreboxProfileSnapshot {
    let cfg = crate::config::get();
    profile_data::runtime_scorebox_profile_snapshot_for_side(
        side,
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
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
    runtime_cached_player_leaderboard_itl_self_rank(chart_hash, profile_snapshot)
}

fn get_cached_player_leaderboard_srpg_self_score_with(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    runtime_cached_player_leaderboard_srpg_self_score(chart_hash, profile_snapshot)
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
    let plan = runtime_plan_player_leaderboard_request(
        chart_hash,
        profile_snapshot,
        max_entries,
        refresh_cached,
        Instant::now(),
    )?;
    if let Some(fetch) = plan.fetch {
        spawn_player_leaderboard_fetch(fetch);
    }

    Some(plan.snapshot)
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

    runtime_invalidate_player_leaderboard_chart_for_api(gs_api_key, chart_hash, Instant::now());
}

pub fn get_machine_leaderboard_local(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    machine_leaderboard_local_from_profiles(
        &profile::local_score_profile_sources(),
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
        &profile::local_score_profile_sources(),
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

    let source = profile::local_score_profile_source_for_id(&profile_id, "");
    personal_leaderboard_local_from_root(&source.root, chart_hash, &source.initials, max_entries)
}

pub fn get_machine_replays_local(chart_hash: &str, max_entries: usize) -> Vec<MachineReplayEntry> {
    machine_replays_local_from_profiles(
        &profile::local_score_profile_sources(),
        chart_hash,
        max_entries,
    )
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
    let song_cache = get_song_cache();
    let stats = imported
        .score
        .as_ref()
        .and_then(|score| imported_score_chart_stats(score, &song_cache, chart_hash));
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
    let summary = run_arrowcloud_bulk_import_pack_groups(
        pack_chart_groups,
        ARROWCLOUD_BULK_MAX_HASHES,
        only_missing_scores,
        |chunk| {
            let scores_by_chart = arrowcloud_api::retrieve_score_cache_entries(
                &api_key,
                user_id.as_deref(),
                chunk,
                &ArrowCloudLeaderboard::ALL_GLOBAL,
            )
            .map_err(|error| boxed_request_error("API", error).to_string())?;
            let hits = scores_by_chart.len();
            let misses = chunk.len().saturating_sub(hits);
            if !scores_by_chart.is_empty() {
                set_cached_ac_scores_for_profile_bulk(&profile_id, scores_by_chart.into_iter());
            }
            Ok(ArrowCloudBulkChunkResult { hits, misses })
        },
        &mut on_progress,
        should_cancel,
        |event| match event {
            ArrowCloudBulkImportRunEvent::Canceled {
                pack_name,
                pack_idx,
                total_packs,
            } => debug!(
                "ArrowCloud bulk import canceled at pack {} ({}/{}).",
                pack_name, pack_idx, total_packs
            ),
            ArrowCloudBulkImportRunEvent::ChunkSucceeded {
                pack_idx,
                total_packs,
                pack_name,
                chunk_len,
                request_elapsed,
                hits,
                misses,
            } => debug!(
                "ArrowCloud /v1/retrieve-scores pack={}/{} pack_name='{}' chunk={} took={:.0}ms hits={} misses={}",
                pack_idx + 1,
                total_packs,
                pack_name,
                chunk_len,
                request_elapsed.as_secs_f32() * 1000.0,
                hits,
                misses,
            ),
            ArrowCloudBulkImportRunEvent::RequestFailed { detail } => warn!("{detail}"),
            ArrowCloudBulkImportRunEvent::ChunkDetail { detail }
            | ArrowCloudBulkImportRunEvent::PackComplete { detail } => debug!("{detail}"),
        },
    );
    Ok(summary)
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
    let summary = run_score_import_pack_groups(
        endpoint,
        username.as_str(),
        pack_chart_groups,
        only_missing_gs_scores,
        |chart_hash| {
            fetch_player_score_from_endpoint(endpoint, &profile, chart_hash)
                .map_err(|error| error.to_string())
        },
        |chart_hash, result| {
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
            }
        },
        &mut on_progress,
        should_cancel,
        |event| match event {
            ScoreImportRunEvent::Canceled {
                import_name,
                pack_idx,
                total_packs,
                processed_charts,
                requested_charts,
            } => debug!(
                "{import_name} score import canceled at pack {}/{} after {processed_charts}/{requested_charts} charts.",
                pack_idx + 1,
                total_packs,
            ),
            ScoreImportRunEvent::RequestFailed {
                import_name,
                chart_hash,
                error,
            } => warn!("{import_name} import request failed for chart {chart_hash}: {error}"),
            ScoreImportRunEvent::ProgressLog {
                import_name,
                username,
                processed_charts,
                requested_charts,
                imported_scores,
                missing_scores,
                failed_requests,
            } => debug!(
                "{import_name} import progress for '{username}': {processed_charts}/{requested_charts} charts (imported={imported_scores}, missing={missing_scores}, failed={failed_requests})",
            ),
            ScoreImportRunEvent::PackComplete { detail } => debug!("{detail}"),
        },
    );
    Ok(summary)
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
    fn player_leaderboard_cache_exposes_srpg_self_score() {
        let snapshot = deadsync_score::scorebox_snapshot(
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
        runtime_seed_player_leaderboard_entry(
            key.clone(),
            deadsync_score::PlayerLeaderboardCacheEntry {
                value: deadsync_score::PlayerLeaderboardCacheValue::Ready(
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

        assert_eq!(
            get_cached_player_leaderboard_srpg_self_score_with("deadbeef", &snapshot),
            Some(9_910)
        );

        runtime_remove_player_leaderboard_entry(&key);
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
        ensure_score_caches_loaded(profile_id);

        let stop = Arc::new(AtomicBool::new(false));

        // Worker: reproduce the pre-fix lock order
        // (leaderboard -> LOCAL -> GS -> AC). If the wheel read
        // ever again held a score cache across the leaderboard lock, this
        // ordering would close the cycle and deadlock.
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let lb = runtime_lock_player_leaderboard_cache();
                let caches = lock_score_caches();
                drop((caches, lb));
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
    }
}
