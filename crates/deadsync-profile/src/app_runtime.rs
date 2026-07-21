//! App-facing profile path and sidecar-file runtime helpers.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use deadlib_platform::dirs;
use deadsync_config::prelude as config;
use log::{debug, info, warn};

use crate::pad_config::{self, PadConfigProfile};
use crate::{
    ActiveProfile, ImportProfileData, LocalProfileSummary, NoteSkin, PLAYER_SLOTS,
    PlayerOptionsData, PlayerSide, Profile, ProfileStatsDecodeError, ProfileStatsLoadError,
    ProfileStatsWriteError, RuntimeProfileStatsWriteError, default_profile_ids_after_side_update,
    is_local_profile_id, player_side_index,
};

#[inline(always)]
pub fn profiles_root() -> PathBuf {
    dirs::app_dirs().profiles_root()
}

pub fn warn_duplicate_profile_guid(guid: &str, left: &Path, right: &Path, kept: &Path) {
    warn!(
        "Duplicate profile GUID {} in '{}' and '{}'; using '{}'.",
        guid,
        left.display(),
        right.display(),
        kept.display()
    );
}

/// Folder for a profile id. Ids that aren't valid GUIDs (the guest/default seed,
/// legacy folder-name ids) skip the cache and fall back to a literal folder,
/// which also covers freshly created or not-yet-migrated profiles.
#[inline(always)]
pub fn local_profile_dir(id: &str) -> PathBuf {
    crate::runtime_profile_dir_for_id(&profiles_root(), id, warn_duplicate_profile_guid)
}

#[inline(always)]
pub fn local_profile_dir_for_id(id: &str) -> PathBuf {
    local_profile_dir(id)
}

pub fn local_score_profile_source_for_id(
    profile_id: &str,
    display_name: &str,
) -> deadsync_score::LocalScoreProfileSource {
    crate::runtime_local_score_profile_source(
        &profiles_root(),
        profile_id,
        display_name,
        warn_duplicate_profile_guid,
    )
}

pub fn local_score_profile_sources() -> Vec<deadsync_score::LocalScoreProfileSource> {
    crate::runtime_local_score_profile_sources(&profiles_root(), warn_duplicate_profile_guid)
}

pub fn score_profile_paths_for_id(profile_id: &str) -> deadsync_score::ScoreProfilePaths {
    deadsync_score::ScoreProfilePaths::new(local_profile_dir(profile_id))
}

pub fn cached_local_itg_score_for_id(
    profile_id: &str,
    chart_hash: &str,
) -> Option<deadsync_score::CachedScore> {
    deadsync_score::runtime_read_logged_local_itg_score_for_profile(
        profile_id,
        chart_hash,
        score_profile_paths_for_id,
    )
}

pub fn cached_gs_score_for_id(
    profile_id: &str,
    chart_hash: &str,
) -> Option<deadsync_score::CachedScore> {
    deadsync_score::runtime_read_logged_gs_score_for_profile(
        profile_id,
        chart_hash,
        score_profile_paths_for_id,
    )
}

pub fn cached_gs_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
) -> Option<deadsync_score::CachedScore> {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side)?;
    cached_gs_score_for_id(&profile_id, chart_hash)
}

pub fn cached_gs_chart_hashes_for_id(profile_id: &str) -> HashSet<String> {
    deadsync_score::runtime_read_logged_gs_chart_hashes_for_profile(
        profile_id,
        score_profile_paths_for_id,
    )
}

pub fn write_cached_gs_score_for_id(
    profile_id: &str,
    chart_hash: String,
    score: deadsync_score::CachedScore,
) {
    deadsync_score::runtime_write_logged_gs_score_for_profile(
        profile_id,
        chart_hash,
        score,
        score_profile_paths_for_id,
    );
}

pub fn cache_logged_gs_score_for_id(
    profile_id: &str,
    chart_hash: &str,
    score: deadsync_score::CachedScore,
    username: &str,
    proves_nonquint_ex: bool,
) {
    deadsync_score::runtime_cache_logged_gs_score_for_profile(
        profile_id,
        chart_hash,
        score,
        username,
        proves_nonquint_ex,
        score_profile_paths_for_id,
    );
}

pub fn cache_gs_score_from_leaderboard_import<F>(
    profile_id: &str,
    username: &str,
    chart_hash: &str,
    imported: deadsync_score::ImportedPlayerScore,
    chart_stats: F,
) where
    F: FnOnce(&deadsync_score::ImportedPlayerScore) -> Option<deadsync_score::GsLampChartStats>,
{
    let stats = chart_stats(&imported);
    let Some(cached) = deadsync_score::cached_score_from_leaderboard_import(
        Some(imported),
        cached_local_itg_score_for_id(profile_id, chart_hash),
        stats,
    ) else {
        return;
    };
    cache_logged_gs_score_for_id(
        profile_id,
        chart_hash,
        cached.score,
        username,
        cached.score_proves_nonquint_ex,
    );
}

pub fn cached_ac_scores_for_id(
    profile_id: &str,
    chart_hash: &str,
) -> Option<deadsync_score::ArrowCloudScores> {
    deadsync_score::runtime_read_logged_ac_scores_for_profile(
        profile_id,
        chart_hash,
        score_profile_paths_for_id,
    )
}

pub fn cached_ac_scores_for_side(
    chart_hash: &str,
    side: PlayerSide,
) -> Option<deadsync_score::ArrowCloudScores> {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side)?;
    cached_ac_scores_for_id(&profile_id, chart_hash)
}

pub fn cached_ac_chart_hashes_with_itg_for_id(profile_id: &str) -> HashSet<String> {
    deadsync_score::runtime_read_logged_ac_chart_hashes_with_itg_for_profile(
        profile_id,
        score_profile_paths_for_id,
    )
}

pub fn collect_score_import_chart_hashes(
    endpoint: deadsync_score::ScoreImportEndpoint,
    song_cache: &[deadsync_chart::SongPack],
    pack_groups_filter: &[String],
    profile_id: &str,
    only_missing_scores: bool,
) -> Vec<(String, Vec<String>)> {
    let existing_scores = match (endpoint, only_missing_scores) {
        (deadsync_score::ScoreImportEndpoint::ArrowCloud, true) => {
            cached_ac_chart_hashes_with_itg_for_id(profile_id)
        }
        (_, true) => cached_gs_chart_hashes_for_id(profile_id),
        (_, false) => HashSet::new(),
    };
    deadsync_score::collect_chart_hashes_per_pack_for_import(
        song_cache,
        pack_groups_filter,
        &existing_scores,
    )
}

pub fn write_cached_ac_scores_for_id_bulk(
    profile_id: &str,
    entries: impl IntoIterator<Item = (String, deadsync_score::ArrowCloudScores)>,
) {
    deadsync_score::runtime_write_logged_ac_scores_for_profile_bulk(
        profile_id,
        entries,
        score_profile_paths_for_id,
    );
}

pub fn write_ac_submit_scores_for_id(
    profile_id: &str,
    chart_hash: &str,
    itg_percent: f64,
    ex_percent: f64,
    hard_ex_percent: f64,
    is_fail: bool,
    submitted_at: chrono::DateTime<chrono::Utc>,
) {
    deadsync_score::runtime_write_logged_ac_submit_scores(
        profile_id,
        chart_hash,
        itg_percent,
        ex_percent,
        hard_ex_percent,
        is_fail,
        submitted_at,
        score_profile_paths_for_id,
    );
}

pub fn total_songs_played_for_id(profile_id: &str) -> u32 {
    deadsync_score::runtime_total_songs_played_for_profile(profile_id, score_profile_paths_for_id)
}

pub fn total_songs_played_for_side(side: PlayerSide) -> u32 {
    let Some(profile_id) = crate::runtime_active_local_profile_id_for_side(side) else {
        return 0;
    };
    total_songs_played_for_id(&profile_id)
}

pub fn recent_played_chart_hashes_for_machine() -> Vec<String> {
    deadsync_score::runtime_recent_played_chart_hashes_for_machine(&profiles_root())
}

pub fn played_chart_counts_for_machine() -> Vec<(String, u32)> {
    deadsync_score::runtime_played_chart_counts_for_machine(&profiles_root())
}

pub fn recent_played_chart_hashes_for_id(profile_id: &str) -> Vec<String> {
    deadsync_score::runtime_recent_played_chart_hashes_for_profile(
        profile_id,
        score_profile_paths_for_id,
    )
}

pub fn played_chart_counts_for_id(profile_id: &str) -> Vec<(String, u32)> {
    deadsync_score::runtime_played_chart_counts_for_profile(profile_id, score_profile_paths_for_id)
}

pub fn prewarm_select_music_score_caches() {
    let p1_profile_id = crate::runtime_active_local_profile_id_for_side(PlayerSide::P1);
    let p2_profile_id = crate::runtime_active_local_profile_id_for_side(PlayerSide::P2);
    deadsync_score::runtime_prewarm_logged_select_music_score_caches(
        p1_profile_id.as_deref(),
        p2_profile_id.as_deref(),
        &local_score_profile_sources(),
        score_profile_paths_for_id,
    );
}

pub fn cached_local_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
) -> Option<deadsync_score::CachedScore> {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side)?;
    cached_local_itg_score_for_id(&profile_id, chart_hash)
}

pub fn cached_local_pass_rate_for_id(profile_id: &str, chart_hash: &str) -> Option<u32> {
    deadsync_score::runtime_read_logged_local_pass_rate_for_profile(
        profile_id,
        chart_hash,
        score_profile_paths_for_id,
    )
}

pub fn cached_local_pass_rate_with_profile(chart_hash: &str, profile_id: &str) -> Option<u32> {
    cached_local_pass_rate_for_id(profile_id, chart_hash)
}

pub fn cached_best_itg_score_for_id(
    profile_id: &str,
    chart_hash: &str,
) -> Option<deadsync_score::CachedScore> {
    deadsync_score::runtime_read_logged_best_itg_score_for_profile(
        profile_id,
        chart_hash,
        score_profile_paths_for_id,
    )
}

pub fn cached_best_itg_score_with_profile(
    chart_hash: &str,
    profile_id: &str,
) -> Option<deadsync_score::CachedScore> {
    cached_best_itg_score_for_id(profile_id, chart_hash)
}

pub fn cached_best_itg_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
) -> Option<deadsync_score::CachedScore> {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side)?;
    cached_best_itg_score_for_id(&profile_id, chart_hash)
}

pub fn ensure_score_caches_loaded_for_id(profile_id: &str) {
    deadsync_score::runtime_ensure_logged_profile_score_caches_loaded(
        profile_id,
        score_profile_paths_for_id,
    );
}

pub fn seed_session_local_itg_score_for_id(
    profile_id: &str,
    chart_hash: &str,
    score: deadsync_score::CachedScore,
) {
    deadsync_score::runtime_seed_logged_local_itg_score(
        profile_id,
        chart_hash,
        score,
        score_profile_paths_for_id,
    );
}

pub fn seed_session_gs_score_for_id(
    profile_id: &str,
    chart_hash: &str,
    score: deadsync_score::CachedScore,
) {
    deadsync_score::runtime_seed_logged_gs_score(
        profile_id,
        chart_hash,
        score,
        score_profile_paths_for_id,
    );
}

pub fn cached_local_scalar_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
    hard_ex: bool,
) -> Option<deadsync_score::LocalScalarScore> {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side)?;
    cached_local_scalar_score_for_id(&profile_id, chart_hash, hard_ex)
}

pub fn cached_local_scalar_score_for_id(
    profile_id: &str,
    chart_hash: &str,
    hard_ex: bool,
) -> Option<deadsync_score::LocalScalarScore> {
    deadsync_score::runtime_read_logged_local_scalar_score_for_profile(
        profile_id,
        chart_hash,
        hard_ex,
        score_profile_paths_for_id,
    )
}

pub fn cached_local_ex_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
) -> Option<deadsync_score::LocalScalarScore> {
    cached_local_scalar_score_for_side(chart_hash, side, false)
}

pub fn cached_local_ex_score_for_id(
    profile_id: &str,
    chart_hash: &str,
) -> Option<deadsync_score::LocalScalarScore> {
    cached_local_scalar_score_for_id(profile_id, chart_hash, false)
}

pub fn cached_local_hard_ex_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
) -> Option<deadsync_score::LocalScalarScore> {
    cached_local_scalar_score_for_side(chart_hash, side, true)
}

pub fn cached_local_hard_ex_score_for_id(
    profile_id: &str,
    chart_hash: &str,
) -> Option<deadsync_score::LocalScalarScore> {
    cached_local_scalar_score_for_id(profile_id, chart_hash, true)
}

pub fn machine_record_local(chart_hash: &str) -> Option<(String, deadsync_score::CachedScore)> {
    deadsync_score::runtime_machine_record_logged_local_lazy(
        chart_hash,
        local_score_profile_sources,
    )
}

pub fn machine_leaderboard_local(
    chart_hash: &str,
    max_entries: usize,
    include_profile_names: bool,
) -> Vec<deadsync_score::LeaderboardEntry> {
    deadsync_score::machine_leaderboard_local_from_profiles(
        &local_score_profile_sources(),
        chart_hash,
        max_entries,
        include_profile_names,
    )
}

pub fn machine_leaderboard_local_without_names(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<deadsync_score::LeaderboardEntry> {
    machine_leaderboard_local(chart_hash, max_entries, false)
}

pub fn machine_leaderboard_local_with_names(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<deadsync_score::LeaderboardEntry> {
    machine_leaderboard_local(chart_hash, max_entries, true)
}

pub fn personal_leaderboard_local_for_side(
    chart_hash: &str,
    side: PlayerSide,
    max_entries: usize,
) -> Vec<deadsync_score::LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }
    let Some(profile_id) = crate::runtime_active_local_profile_id_for_side(side) else {
        return Vec::new();
    };

    let source = local_score_profile_source_for_id(&profile_id, "");
    deadsync_score::personal_leaderboard_local_from_root(
        &source.root,
        chart_hash,
        &source.initials,
        max_entries,
    )
}

pub fn machine_replays_local(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<deadsync_score::MachineReplayEntry> {
    deadsync_score::machine_replays_local_from_profiles(
        &local_score_profile_sources(),
        chart_hash,
        max_entries,
    )
}

pub fn append_local_score_for_id(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut deadsync_score::LocalScoreEntry,
) -> bool {
    deadsync_score::runtime_append_logged_local_score_for_profile(
        profile_id,
        profile_initials,
        chart_hash,
        entry,
        score_profile_paths_for_id,
    )
}

pub fn import_local_scores_for_id<F, C>(
    profile_id: &str,
    profile_initials: &str,
    scores: &mut [(String, deadsync_score::LocalScoreEntry)],
    on_progress: F,
    should_cancel: C,
) -> (usize, bool)
where
    F: FnMut(usize, usize),
    C: Fn() -> bool,
{
    deadsync_score::import_local_scores_with_writer(
        scores,
        on_progress,
        should_cancel,
        |chart_hash, entry| {
            append_local_score_for_id(profile_id, profile_initials, chart_hash, entry)
        },
    )
}

pub fn save_local_summary_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
    music_rate: f32,
    summary: &deadsync_score::stage_stats::PlayerStageSummary,
) {
    match deadsync_score::local_summary_score_save_decision(
        deadsync_score::LocalSummaryScoreSaveInput {
            chart_hash,
            disqualified: summary.disqualified,
            score_valid: summary.score_valid,
        },
    ) {
        deadsync_score::LocalSummaryScoreSaveDecision::SkipEmptyChartHash => return,
        deadsync_score::LocalSummaryScoreSaveDecision::SkipDisqualified => {
            debug!("Skipping local summary score save: run was disqualified.");
            return;
        }
        deadsync_score::LocalSummaryScoreSaveDecision::SkipInvalid => {
            debug!("Skipping local summary score save: ranking-invalid modifiers were used.");
            return;
        }
        deadsync_score::LocalSummaryScoreSaveDecision::Save => {}
    }
    let Some(profile_id) = crate::runtime_active_local_profile_id_for_side(side) else {
        return;
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let profile_initials = crate::runtime_profile_for_side(side).player_initials;
    let mut entry =
        deadsync_score::local_score_entry_from_stage_summary(now_ms, music_rate, summary);
    append_local_score_for_id(
        &profile_id,
        profile_initials.as_str(),
        chart_hash,
        &mut entry,
    );
}

pub fn set_cached_online_itl_self_score(
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
    deadsync_score::runtime_set_online_itl_self_score_for_profile_dirs(
        profile_id,
        api_key,
        chart_hash,
        score,
        local_profile_dir_for_id,
    );
}

pub fn set_cached_online_itl_self_rank(
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
    deadsync_score::runtime_set_online_itl_self_rank_for_profile_dirs(
        profile_id,
        api_key,
        chart_hash,
        rank,
        local_profile_dir_for_id,
    );
}

pub fn set_cached_online_srpg_self_score(
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
    deadsync_score::runtime_set_online_srpg_self_score_for_profile_dirs(
        profile_id,
        api_key,
        chart_hash,
        score,
        local_profile_dir_for_id,
    );
}

pub fn invalidate_player_leaderboard_chart_for_side(
    chart_hash: &str,
    side: PlayerSide,
    invalidated_at: Instant,
) {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() {
        return;
    }
    let api_key = crate::runtime_groovestats_api_key_for_side(side);
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return;
    }

    let profile_id = crate::runtime_active_local_profile_id_for_side(side);
    set_cached_online_itl_self_score(profile_id.as_deref(), api_key, chart_hash, None);
    set_cached_online_itl_self_rank(profile_id.as_deref(), api_key, chart_hash, None);
    set_cached_online_srpg_self_score(profile_id.as_deref(), api_key, chart_hash, None);
    deadsync_score::runtime_invalidate_player_leaderboard_chart_for_api(
        api_key,
        chart_hash,
        invalidated_at,
    );
}

pub fn seed_session_online_itl_self_score(api_key: &str, chart_hash: &str, ex_hundredths: u32) {
    set_cached_online_itl_self_score(None, api_key, chart_hash, Some(ex_hundredths));
}

pub fn seed_session_online_itl_self_rank(api_key: &str, chart_hash: &str, rank: u32) {
    set_cached_online_itl_self_rank(None, api_key, chart_hash, Some(rank));
}

pub fn read_itl_file_for_id(profile_id: &str) -> deadsync_score::ItlFileData {
    deadsync_score::runtime_read_itl_file_for_profile(profile_id, local_profile_dir_for_id)
}

pub fn write_itl_file_for_id(profile_id: &str, data: &deadsync_score::ItlFileData) {
    deadsync_score::runtime_write_itl_file_for_profile(profile_id, data, local_profile_dir_for_id);
}

pub fn set_cached_itl_file_for_id(profile_id: &str, data: deadsync_score::ItlFileData) {
    deadsync_score::runtime_set_itl_score_file(profile_id, data);
}

pub fn save_itl_gameplay_players<L>(
    players: impl IntoIterator<Item = deadsync_score::ItlGameplaySavePlayer>,
    log_skip: L,
) -> Vec<deadsync_score::ItlGameplaySaveProgress>
where
    L: FnMut(deadsync_score::ItlGameplaySaveSkip<'_>),
{
    deadsync_score::save_itl_gameplay_players(
        players,
        read_itl_file_for_id,
        write_itl_file_for_id,
        set_cached_itl_file_for_id,
        log_skip,
    )
}

pub fn ensure_itl_score_cache_loaded_for_id(profile_id: &str) {
    deadsync_score::runtime_ensure_itl_score_profile_loaded(profile_id, read_itl_file_for_id);
}

pub fn seed_session_itl_unlock_folders(profile_id: &str, folders: &[&str]) {
    ensure_itl_score_cache_loaded_for_id(profile_id);
    deadsync_score::mark_itl_unlock_folders(profile_id, folders.iter().copied());
}

pub fn cached_itl_score_for_side(
    chart_hash: &str,
    side: PlayerSide,
) -> Option<deadsync_score::CachedItlScore> {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side);
    cached_itl_score_for_id(chart_hash, profile_id.as_deref())
}

pub fn cached_itl_score_for_id(
    chart_hash: &str,
    profile_id: Option<&str>,
) -> Option<deadsync_score::CachedItlScore> {
    deadsync_score::runtime_cached_itl_chart_score(profile_id, chart_hash, read_itl_file_for_id)
}

pub fn cached_itl_score_for_song(
    song: &deadsync_chart::SongData,
    side: PlayerSide,
) -> Option<deadsync_score::CachedItlScore> {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side);
    cached_itl_score_for_song_with_profile(song, profile_id.as_deref())
}

pub fn cached_itl_score_for_song_with_profile(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
) -> Option<deadsync_score::CachedItlScore> {
    deadsync_score::runtime_cached_itl_song_score(song, profile_id, read_itl_file_for_id)
}

pub fn cached_itl_score_for_song_assume_loaded(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
) -> Option<deadsync_score::CachedItlScore> {
    deadsync_score::runtime_cached_itl_song_score_assume_loaded(song, profile_id)
}

fn cached_itl_chart_no_cmod_for_song(
    profile_id: &str,
    song_dir: Option<&str>,
    group_name: Option<&str>,
    chart_hash: &str,
    subtitle: &str,
) -> Option<bool> {
    deadsync_score::cached_itl_chart_no_cmod_for_song(
        profile_id, song_dir, group_name, chart_hash, subtitle,
    )
}

pub fn should_warn_itl_cmod(
    profile_id: Option<&str>,
    song_dir: Option<&str>,
    group_name: Option<&str>,
    chart_hash: &str,
    subtitle: &str,
) -> bool {
    let cached_no_cmod = profile_id.and_then(|profile_id| {
        cached_itl_chart_no_cmod_for_song(profile_id, song_dir, group_name, chart_hash, subtitle)
    });
    deadsync_score::itl_should_warn_cmod_context(cached_no_cmod, group_name, subtitle)
}

pub fn ensure_itl_wheel_caches_loaded_for_id(profile_id: &str) {
    deadsync_score::runtime_ensure_itl_wheel_caches_loaded_for_profile_dirs(
        profile_id,
        local_profile_dir_for_id,
    );
}

pub fn itl_song_folder_unlocked_for_side(song_folder: &str, side: PlayerSide) -> bool {
    let profile_id = crate::runtime_active_local_profile_id_for_side(side);
    deadsync_score::runtime_cached_itl_song_folder_unlocked(
        song_folder,
        profile_id.as_deref(),
        read_itl_file_for_id,
    )
}

pub fn itl_song_folder_unlocked_with_profile(song_folder: &str, profile_id: Option<&str>) -> bool {
    deadsync_score::runtime_cached_itl_song_folder_unlocked(
        song_folder,
        profile_id,
        read_itl_file_for_id,
    )
}

pub fn cached_online_itl_self_rank_for_key(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    deadsync_score::runtime_cached_online_itl_self_rank_for_profile_dirs(
        chart_hash,
        profile_id,
        api_key,
        local_profile_dir_for_id,
    )
}

pub fn cached_online_itl_self_rank_for_key_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    deadsync_score::runtime_cached_online_itl_self_rank_assume_loaded(
        chart_hash, profile_id, api_key,
    )
}

pub fn cached_online_itl_self_score_for_key(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    deadsync_score::runtime_cached_online_itl_self_score_for_profile_dirs(
        chart_hash,
        profile_id,
        api_key,
        local_profile_dir_for_id,
    )
}

pub fn cached_online_itl_self_score_for_key_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    deadsync_score::runtime_cached_online_itl_self_score_assume_loaded(
        chart_hash, profile_id, api_key,
    )
}

pub fn cached_online_srpg_self_score_for_key(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    deadsync_score::runtime_cached_online_srpg_self_score_for_profile_dirs(
        chart_hash,
        profile_id,
        api_key,
        local_profile_dir_for_id,
    )
}

pub fn cached_online_srpg_self_score_for_key_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    deadsync_score::runtime_cached_online_srpg_self_score_assume_loaded(
        chart_hash, profile_id, api_key,
    )
}

pub struct ItlWheelSideCache<'a> {
    leaderboard_snapshot: &'a deadsync_score::GameplayScoreboxProfileSnapshot,
}

impl<'a> ItlWheelSideCache<'a> {
    pub const fn new(
        leaderboard_snapshot: &'a deadsync_score::GameplayScoreboxProfileSnapshot,
    ) -> Self {
        Self {
            leaderboard_snapshot,
        }
    }

    pub fn leaderboard_snapshot(&self) -> &deadsync_score::GameplayScoreboxProfileSnapshot {
        &self.leaderboard_snapshot
    }

    pub fn cached_local_itl_score(
        &self,
        song: &deadsync_chart::SongData,
    ) -> Option<deadsync_score::CachedItlScore> {
        cached_itl_score_for_song_assume_loaded(
            song,
            self.leaderboard_snapshot.persistent_profile_id(),
        )
    }

    pub fn cached_self_ex_score(&self, chart_hash: &str) -> Option<u32> {
        cached_online_itl_self_score_for_key_assume_loaded(
            chart_hash,
            self.leaderboard_snapshot.persistent_profile_id(),
            self.leaderboard_snapshot.api_key(),
        )
    }

    pub fn cached_srpg_self_score(&self, chart_hash: &str) -> Option<u32> {
        deadsync_score::runtime_cached_player_leaderboard_srpg_self_score(
            chart_hash,
            &self.leaderboard_snapshot,
        )
        .or_else(|| {
            cached_online_srpg_self_score_for_key_assume_loaded(
                chart_hash,
                self.leaderboard_snapshot.persistent_profile_id(),
                self.leaderboard_snapshot.api_key(),
            )
        })
    }

    pub fn cached_tournament_rank(&self, chart_hash: &str) -> Option<u32> {
        deadsync_score::runtime_cached_player_leaderboard_itl_self_rank(
            chart_hash,
            &self.leaderboard_snapshot,
        )
        .or_else(|| {
            cached_online_itl_self_rank_for_key_assume_loaded(
                chart_hash,
                self.leaderboard_snapshot.persistent_profile_id(),
                self.leaderboard_snapshot.api_key(),
            )
        })
    }
}

pub fn cached_itl_tournament_overall_ranks_for_profile(
    side_idx: usize,
    joined: bool,
    api_key: &str,
    profile_id: Option<&str>,
    song_cache_generation: u64,
    song_cache: &[deadsync_chart::SongPack],
) -> Arc<HashMap<String, u32>> {
    deadsync_score::runtime_online_itl_overall_ranks_for_side(
        side_idx,
        joined,
        api_key,
        profile_id,
        song_cache_generation,
        song_cache,
        |profile_id| {
            deadsync_score::runtime_load_online_itl_self_index_for_profile(
                profile_id,
                deadsync_score::OnlineItlSelfIndexKind::Score,
                local_profile_dir_for_id,
            )
        },
    )
}

pub fn import_itl_json(profile_id: &str, json_text: &str) -> usize {
    deadsync_score::runtime_import_itl_json(profile_id, json_text, write_itl_file_for_id)
}

pub fn update_itl_unlock_folders(profile_id: &str, folders: &[String]) {
    deadsync_score::runtime_update_itl_unlock_folders(
        profile_id,
        folders.iter().map(String::as_str),
        read_itl_file_for_id,
        write_itl_file_for_id,
    );
}

/// Idempotent startup migration to embedded-GUID identity. In a single pass over
/// the profiles directory it backfills GUIDs into legacy profiles, rewrites
/// stored default ids that still hold a folder name, renames legacy folders to
/// match their display name (best-effort), and seeds the resolver cache.
pub fn migrate_local_profiles(
    default_profiles: [Option<String>; PLAYER_SLOTS],
    update_default_profiles: impl FnOnce(Option<String>, Option<String>),
) {
    let migration = crate::migrate_local_profile_dirs(&profiles_root());
    for backfill in &migration.guid_backfills {
        match &backfill.error {
            None => info!(
                "Assigned profile GUID {} to '{}'.",
                backfill.guid,
                backfill.path.display()
            ),
            Some(e) => warn!(
                "Failed to backfill GUID for '{}': {e}",
                backfill.path.display()
            ),
        }
    }

    // Self-heal stored default ids that still reference a legacy folder name.
    let folder_to_guid: HashMap<&str, &str> = migration
        .entries
        .iter()
        .map(|e| (e.original_folder.as_str(), e.guid.as_str()))
        .collect();
    let new_profiles = [
        crate::heal_default_profile_id(default_profiles[0].clone(), &folder_to_guid),
        crate::heal_default_profile_id(default_profiles[1].clone(), &folder_to_guid),
    ];
    if new_profiles != default_profiles {
        info!("Migrated default profile ids to embedded GUIDs.");
        update_default_profiles(new_profiles[0].clone(), new_profiles[1].clone());
    }

    for rename in &migration.folder_renames {
        match &rename.error {
            None => info!(
                "Renamed profile folder '{}' -> '{}'.",
                rename.current_folder, rename.desired_folder
            ),
            Some(e) => warn!(
                "Failed to rename profile folder '{}' -> '{}': {e}",
                rename.current_folder, rename.desired_folder
            ),
        }
    }

    // Seed the resolver cache from the post-migration snapshot (dedup duplicate
    // GUIDs by smallest folder name) so the first lookup needn't rescan.
    crate::runtime_set_profile_dir_cache(&profiles_root(), migration.cache_map);
}

/// Seeds the session's active profiles from the configured default local
/// profiles. Only applies a saved id when it still refers to an existing local
/// profile; otherwise that side starts as Guest.
pub fn restore_default_profiles(default_profiles: [Option<String>; PLAYER_SLOTS]) {
    crate::runtime_restore_default_profiles(&default_profiles, |id| local_profile_dir(id).is_dir());
}

pub fn update_machine_default_noteskin(
    current_noteskin: &str,
    setting: NoteSkin,
    update_noteskin: impl FnOnce(&str),
) {
    if current_noteskin.eq_ignore_ascii_case(setting.as_str()) {
        return;
    }
    update_noteskin(setting.as_str());
    crate::runtime_update_guest_profile_noteskin(setting);
}

#[inline(always)]
pub fn machine_default_noteskin_value() -> NoteSkin {
    NoteSkin::new(&config::machine_default_noteskin())
}

/// Machine-default pad-light brightness used to seed a new profile, mirroring
/// `machine_default_noteskin_value`. Players adjust their own value afterwards.
#[inline(always)]
pub fn machine_default_light_brightness() -> u8 {
    config::get().smx_default_light_brightness
}

pub fn machine_default_noteskin() -> NoteSkin {
    machine_default_noteskin_value()
}

pub fn update_machine_default_noteskin_from_config(setting: NoteSkin) {
    update_machine_default_noteskin(
        &config::machine_default_noteskin(),
        setting,
        config::update_machine_default_noteskin,
    );
}

pub fn load_profile_for_side(
    side: PlayerSide,
    machine_default_noteskin: NoteSkin,
    machine_default_light_brightness: u8,
) {
    let today = chrono::Local::now().date_naive().to_string();
    let mut default_profile = crate::default_profile_with_machine_settings(
        machine_default_noteskin.clone(),
        machine_default_light_brightness,
    );
    default_profile.calories_burned_day = today.clone();
    let load = crate::runtime_load_profile_for_side(
        &profiles_root(),
        side,
        &default_profile,
        today.as_str(),
        machine_default_noteskin,
        machine_default_light_brightness,
        |id| local_profile_dir(id).is_dir(),
        || scan_local_profiles().into_iter().next().map(|p| p.id),
        warn_duplicate_profile_guid,
    );

    match &load.selection {
        crate::ActiveProfileLoadSelection::MissingFallbackLocal {
            missing_id,
            fallback_id,
        } => {
            info!("Profile folder '{missing_id}' not found; falling back to '{fallback_id}'.");
        }
        crate::ActiveProfileLoadSelection::MissingFallbackGuest { missing_id } => {
            info!(
                "Profile folder '{missing_id}' not found and no other profiles exist; using Guest."
            );
        }
        crate::ActiveProfileLoadSelection::Guest
        | crate::ActiveProfileLoadSelection::Local { .. } => {}
    }

    if let Some(dir) = load.default_files_dir.as_deref() {
        info!(
            "Profile files not found, creating defaults in '{}'.",
            dir.display()
        );
        // A new folder may have appeared; let the resolver pick it up.
        crate::runtime_invalidate_profile_dir_cache();
    }
    if let Some(e) = load.default_files_error {
        warn!("Failed to create default profile files: {e}");
        // Proceed with default struct values and attempt to save them.
    }
    let Some(load_report) = load.load_report else {
        return;
    };
    if !load_report.profile_ini_loaded {
        warn!(
            "Failed to load '{}', using default profile settings.",
            load_report.profile_ini_path.display()
        );
    }
    if !load_report.groovestats_ini_loaded {
        warn!(
            "Failed to load '{}', using default GrooveStats info.",
            load_report.groovestats_ini_path.display()
        );
    }
    if !load_report.arrowcloud_ini_loaded {
        warn!(
            "Failed to load '{}', using default ArrowCloud info.",
            load_report.arrowcloud_ini_path.display()
        );
    }
    if let Some(error) = load_report.stats_error {
        log_profile_stats_load_error(&load_report.stats_path, error);
    }
    if let Some(error) = load.profile_ini_save_error {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
    if let Some(error) = load.stats_save_error {
        log_profile_stats_write_error(error.profile_id.as_str(), error.error);
    }
    if let Some(error) = load.groovestats_save_error {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
    if let Some(error) = load.arrowcloud_save_error {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
    info!("Profile configuration files updated with default values for any missing fields.");
}

pub fn load_profiles(
    default_profiles: [Option<String>; PLAYER_SLOTS],
    update_default_profiles: impl FnOnce(Option<String>, Option<String>),
    machine_default_noteskin: NoteSkin,
    machine_default_light_brightness: u8,
) {
    migrate_local_profiles(default_profiles.clone(), update_default_profiles);
    restore_default_profiles(default_profiles);
    load_profile_for_side(
        PlayerSide::P1,
        machine_default_noteskin.clone(),
        machine_default_light_brightness,
    );
    load_profile_for_side(
        PlayerSide::P2,
        machine_default_noteskin,
        machine_default_light_brightness,
    );
}

pub fn load_profiles_from_config() {
    let (p1, p2) = config::default_profiles();
    load_profiles(
        [p1, p2],
        config::update_default_profiles,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    );
}

pub fn scan_local_profiles() -> Vec<LocalProfileSummary> {
    crate::scan_local_profile_summaries(&profiles_root())
}

pub fn default_local_profile_options(
    noteskin: NoteSkin,
    pad_light_brightness: u8,
) -> (PlayerOptionsData, PlayerOptionsData) {
    crate::default_player_options_with_machine_settings(noteskin, pad_light_brightness)
}

pub fn default_local_profile_options_from_config() -> (PlayerOptionsData, PlayerOptionsData) {
    default_local_profile_options(
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    )
}

pub fn default_profile_for_side(
    default_profiles: [Option<String>; PLAYER_SLOTS],
    side: PlayerSide,
) -> ActiveProfile {
    crate::default_active_profile_for_side(&default_profiles, side, |id| {
        local_profile_dir(id).is_dir()
    })
}

pub fn default_local_profile_id_for_side(
    default_profiles: [Option<String>; PLAYER_SLOTS],
    side: PlayerSide,
) -> Option<String> {
    match default_profile_for_side(default_profiles, side) {
        ActiveProfile::Local { id } => Some(id),
        ActiveProfile::Guest => None,
    }
}

pub fn default_profile_for_side_from_config(side: PlayerSide) -> ActiveProfile {
    let (p1, p2) = config::default_profiles();
    default_profile_for_side([p1, p2], side)
}

pub fn default_local_profile_id_for_side_from_config(side: PlayerSide) -> Option<String> {
    let (p1, p2) = config::default_profiles();
    default_local_profile_id_for_side([p1, p2], side)
}

pub fn update_default_profile_for_side(
    default_profiles: [Option<String>; PLAYER_SLOTS],
    side: PlayerSide,
    profile: &ActiveProfile,
    update_default_profiles: impl FnOnce(Option<String>, Option<String>),
) {
    let defaults = default_profile_ids_after_side_update(default_profiles, side, profile, |id| {
        is_local_profile_id(id) && local_profile_dir(id).is_dir()
    });
    update_default_profiles(defaults[0].clone(), defaults[1].clone());
}

pub fn update_default_profile_for_side_from_config(side: PlayerSide, profile: ActiveProfile) {
    let (p1, p2) = config::default_profiles();
    update_default_profile_for_side([p1, p2], side, &profile, config::update_default_profiles);
}

#[inline(always)]
pub fn gameplay_side_for_player(num_players: usize, player_idx: usize) -> PlayerSide {
    crate::side_for_gameplay_player(
        num_players,
        player_idx,
        crate::runtime_session_player_side(),
    )
}

#[inline(always)]
pub fn active_local_profile_id_for_gameplay_player(
    num_players: usize,
    player_idx: usize,
) -> Option<(PlayerSide, String)> {
    let side = gameplay_side_for_player(num_players, player_idx);
    crate::runtime_active_local_profile_id_for_side(side).map(|profile_id| (side, profile_id))
}

pub fn update_default_profiles_from_selection(
    default_profiles: [Option<String>; PLAYER_SLOTS],
    update_default_profiles: impl FnOnce(Option<String>, Option<String>),
) {
    let defaults = crate::runtime_default_profile_ids_after_current_selection(default_profiles);
    update_default_profiles(defaults[0].clone(), defaults[1].clone());
}

pub fn smx_gif_packs<T: Copy>(
    machine_bg: T,
    machine_judge: T,
    parse: impl FnMut(&str) -> T,
) -> ([T; PLAYER_SLOTS], [T; PLAYER_SLOTS]) {
    crate::runtime_smx_pack_names_for_profiles(machine_bg, machine_judge, parse)
}

pub fn smx_gif_packs_from_config(
    machine_bg: config::SmxPackName,
    machine_judge: config::SmxPackName,
) -> (
    [config::SmxPackName; PLAYER_SLOTS],
    [config::SmxPackName; PLAYER_SLOTS],
) {
    smx_gif_packs(machine_bg, machine_judge, config::SmxPackName::parse)
}

pub fn scorebox_profile_snapshot_from_config(
    player_profile: &Profile,
    side_joined: bool,
    persistent_profile_id: Option<String>,
) -> deadsync_score::GameplayScoreboxProfileSnapshot {
    let cfg = config::get();
    crate::scorebox_profile_snapshot(
        player_profile,
        side_joined,
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
        persistent_profile_id,
    )
}

#[inline(always)]
pub fn groovestats_score_service_allowed() -> bool {
    config::get().enable_groovestats
}

pub fn toggle_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    crate::runtime_toggle_favorite_for_side(
        &profiles_root(),
        side,
        chart_hash,
        warn_duplicate_profile_guid,
    )
}

pub fn toggle_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    crate::runtime_toggle_favorited_pack_for_side(
        &profiles_root(),
        side,
        pack_name,
        warn_duplicate_profile_guid,
    )
}

pub fn set_active_profile_for_side_with_defaults(
    side: PlayerSide,
    profile: ActiveProfile,
    machine_default_noteskin: NoteSkin,
    machine_default_light_brightness: u8,
) -> Profile {
    if !crate::runtime_set_active_profile_for_side(side, profile) {
        return crate::runtime_profile_for_side(side);
    }
    load_profile_for_side(
        side,
        machine_default_noteskin,
        machine_default_light_brightness,
    );
    crate::runtime_profile_for_side(side)
}

pub fn set_active_profile_for_side_from_config(
    side: PlayerSide,
    profile: ActiveProfile,
) -> Profile {
    set_active_profile_for_side_with_defaults(
        side,
        profile,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    )
}

pub fn set_active_profiles_with_defaults(
    profiles: [ActiveProfile; PLAYER_SLOTS],
    default_profiles: [Option<String>; PLAYER_SLOTS],
    update_default_profiles: impl FnOnce(Option<String>, Option<String>),
    machine_default_noteskin: NoteSkin,
    machine_default_light_brightness: u8,
) -> [Profile; PLAYER_SLOTS] {
    let changed = crate::runtime_set_active_profiles(profiles);
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if changed[player_side_index(side)] {
            load_profile_for_side(
                side,
                machine_default_noteskin.clone(),
                machine_default_light_brightness,
            );
        }
    }
    update_default_profiles_from_selection(default_profiles, update_default_profiles);
    [
        crate::runtime_profile_for_side(PlayerSide::P1),
        crate::runtime_profile_for_side(PlayerSide::P2),
    ]
}

pub fn set_active_profiles_from_config(
    p1: ActiveProfile,
    p2: ActiveProfile,
) -> [Profile; PLAYER_SLOTS] {
    let (p1_default, p2_default) = config::default_profiles();
    set_active_profiles_with_defaults(
        [p1, p2],
        [p1_default, p2_default],
        config::update_default_profiles,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    )
}

pub fn load_default_profiles_for_joined_sides_with_defaults(
    default_profiles: [Option<String>; PLAYER_SLOTS],
    machine_default_noteskin: NoteSkin,
    machine_default_light_brightness: u8,
) -> [Profile; PLAYER_SLOTS] {
    let changed = crate::runtime_restore_joined_default_profiles(&default_profiles, |id| {
        local_profile_dir(id).is_dir()
    });
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if changed[player_side_index(side)] {
            load_profile_for_side(
                side,
                machine_default_noteskin.clone(),
                machine_default_light_brightness,
            );
        }
    }
    [
        crate::runtime_profile_for_side(PlayerSide::P1),
        crate::runtime_profile_for_side(PlayerSide::P2),
    ]
}

pub fn load_default_profiles_for_joined_sides_from_config() -> [Profile; PLAYER_SLOTS] {
    let (p1, p2) = config::default_profiles();
    load_default_profiles_for_joined_sides_with_defaults(
        [p1, p2],
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    )
}

pub fn create_local_profile(
    display_name: &str,
    noteskin: NoteSkin,
    pad_light_brightness: u8,
    default_profiles: [Option<String>; PLAYER_SLOTS],
    update_default_profiles: impl FnOnce(Option<String>, Option<String>),
) -> Result<String, std::io::Error> {
    let result = crate::runtime_create_local_profile(
        &profiles_root(),
        display_name,
        noteskin,
        pad_light_brightness,
        default_profiles,
    )?;
    update_default_profiles(
        result.default_profiles[0].clone(),
        result.default_profiles[1].clone(),
    );
    Ok(result.id)
}

pub fn create_local_profile_from_config(display_name: &str) -> Result<String, std::io::Error> {
    let (p1_default, p2_default) = config::default_profiles();
    create_local_profile(
        display_name,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
        [p1_default, p2_default],
        config::update_default_profiles,
    )
}

pub fn create_local_profile_from_import(
    data: &ImportProfileData<'_>,
) -> Result<String, std::io::Error> {
    let result = crate::runtime_create_local_profile_from_import(&profiles_root(), data)?;
    if let Some(e) = result.avatar_copy_error
        && let Some(src) = data.avatar_src
    {
        warn!("Failed to copy imported avatar {src:?}: {e}");
    }

    Ok(result.id)
}

pub fn rename_local_profile(id: &str, display_name: &str) -> Result<(), std::io::Error> {
    let result = crate::runtime_rename_local_profile(
        &profiles_root(),
        &local_profile_dir(id),
        id,
        display_name,
    )?;
    if let Some(folder) = result.folder_rename {
        match folder.error {
            None => info!(
                "Renamed profile folder '{}' -> '{}'.",
                folder.current_folder, folder.desired_folder
            ),
            Some(e) => warn!(
                "Failed to rename profile folder '{}' -> '{}': {e}",
                folder.current_folder, folder.desired_folder
            ),
        }
    }

    Ok(())
}

pub fn delete_local_profile_with_defaults(
    id: &str,
    default_profiles: [Option<String>; PLAYER_SLOTS],
    update_default_profiles: impl FnOnce(Option<String>, Option<String>),
    machine_default_noteskin: NoteSkin,
    machine_default_light_brightness: u8,
) -> Result<(), std::io::Error> {
    let result = crate::runtime_delete_local_profile(&local_profile_dir(id), id, default_profiles)?;
    update_default_profiles(
        result.default_profiles[0].clone(),
        result.default_profiles[1].clone(),
    );
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if result.changed_sides[player_side_index(side)] {
            load_profile_for_side(
                side,
                machine_default_noteskin.clone(),
                machine_default_light_brightness,
            );
        }
    }

    Ok(())
}

pub fn delete_local_profile_from_config(id: &str) -> Result<(), std::io::Error> {
    let (p1_default, p2_default) = config::default_profiles();
    delete_local_profile_with_defaults(
        id,
        [p1_default, p2_default],
        config::update_default_profiles,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    )
}

pub fn load_pad_configs(profile_id: &str) -> Vec<PadConfigProfile> {
    pad_config::load_profile_id(&profiles_root(), profile_id, warn_duplicate_profile_guid)
}

pub fn save_pad_configs(profile_id: &str, profiles: &[PadConfigProfile]) {
    if let Err(error) = pad_config::save_profile_id_report(
        &profiles_root(),
        profile_id,
        profiles,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_pad_config(
    profile_id: &str,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
) {
    if let Err(error) = pad_config::upsert_profile_id_report(
        &profiles_root(),
        profile_id,
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn set_default_pad_config(profile_id: &str, serial: &str, name: &str) {
    if let Err(error) = pad_config::set_default_profile_id_report(
        &profiles_root(),
        profile_id,
        serial,
        name,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn rename_pad_config(profile_id: &str, old: &str, new: &str) {
    if let Err(error) = pad_config::rename_profile_id_report(
        &profiles_root(),
        profile_id,
        old,
        new,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn delete_pad_config(profile_id: &str, name: &str) {
    if let Err(error) = pad_config::delete_profile_id_report(
        &profiles_root(),
        profile_id,
        name,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn log_profile_stats_load_error(path: &Path, error: ProfileStatsLoadError) {
    match error {
        ProfileStatsLoadError::Read(e) => {
            warn!("Failed to read {}: {}", path.display(), e);
        }
        ProfileStatsLoadError::Decode(ProfileStatsDecodeError::UnsupportedVersion(version)) => {
            warn!(
                "Unsupported profile stats version {} in '{}'.",
                version,
                path.display()
            );
        }
        ProfileStatsLoadError::Decode(ProfileStatsDecodeError::InvalidPayload) => {
            warn!("Failed to decode profile stats '{}'.", path.display());
        }
    }
}

pub fn log_profile_stats_write_error(profile_id: &str, error: ProfileStatsWriteError) {
    match error {
        ProfileStatsWriteError::Encode => {
            warn!("Failed to encode profile stats for '{}'.", profile_id);
        }
        ProfileStatsWriteError::CreateDir { path, error } => {
            warn!(
                "Failed to create profile stats directory '{}': {}",
                path.display(),
                error
            );
        }
        ProfileStatsWriteError::WriteTmp { path, error } => {
            warn!("Failed to write {}: {}", path.display(), error);
        }
        ProfileStatsWriteError::Rename { path, error, .. } => {
            warn!("Failed to save {}: {}", path.display(), error);
        }
    }
}

fn log_profile_stats_runtime_write_error(error: RuntimeProfileStatsWriteError) {
    log_profile_stats_write_error(error.profile_id.as_str(), error.error);
}

pub fn save_profile_ini_for_side(side: PlayerSide) {
    if let Some(error) = crate::runtime_save_profile_ini_for_side(
        &profiles_root(),
        side,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn save_profile_stats_for_side(side: PlayerSide) {
    if let Some(error) = crate::runtime_write_profile_stats_for_side(
        &profiles_root(),
        side,
        warn_duplicate_profile_guid,
    ) {
        log_profile_stats_runtime_write_error(error);
    }
}

pub fn write_imported_profile_stats(profile_id: &str, current_combo: u32) {
    if let Err(e) =
        crate::write_imported_profile_stats_dir(&local_profile_dir(profile_id), current_combo)
    {
        log_profile_stats_write_error(profile_id, e);
    }
}

pub fn set_arrowcloud_api_key_for_side(side: PlayerSide, api_key: &str) {
    if let Some(error) = crate::runtime_set_arrowcloud_api_key_for_side(
        &profiles_root(),
        side,
        api_key,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn set_arrowcloud_api_key_for_id(profile_id: &str, api_key: &str) {
    if let Some(error) = crate::runtime_set_arrowcloud_api_key_for_id(
        &profiles_root(),
        profile_id,
        api_key,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn get_arrowcloud_api_key_for_id(profile_id: &str) -> String {
    crate::runtime_read_arrowcloud_api_key_for_id(
        &profiles_root(),
        profile_id,
        warn_duplicate_profile_guid,
    )
}

pub fn set_groovestats_credentials_for_side(side: PlayerSide, api_key: &str, username: &str) {
    if let Some(error) = crate::runtime_set_groovestats_credentials_for_side(
        &profiles_root(),
        side,
        api_key,
        username,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn set_groovestats_credentials_for_id(profile_id: &str, api_key: &str, username: &str) {
    if let Some(error) = crate::runtime_set_groovestats_credentials_for_id(
        &profiles_root(),
        profile_id,
        api_key,
        username,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

pub fn get_groovestats_api_key_for_id(profile_id: &str) -> Option<String> {
    crate::runtime_read_groovestats_api_key_for_id(
        &profiles_root(),
        profile_id,
        warn_duplicate_profile_guid,
    )
}

pub fn mark_known_pack_names_for_local_profile<'a>(
    profile_id: &str,
    pack_names: impl IntoIterator<Item = &'a str>,
) {
    if let Some(error) = crate::runtime_mark_known_pack_names_for_local_profile(
        &profiles_root(),
        profile_id,
        pack_names,
        warn_duplicate_profile_guid,
    ) {
        log_profile_stats_runtime_write_error(error);
    }
}

pub fn sync_known_packs(profile_ids: &[String], scanned_pack_names: &[String]) -> HashSet<String> {
    let result = crate::runtime_sync_known_packs(
        &profiles_root(),
        profile_ids,
        scanned_pack_names,
        warn_duplicate_profile_guid,
    );
    for error in result.write_errors {
        log_profile_stats_runtime_write_error(error);
    }
    result.unknown_pack_names
}

pub fn mark_pack_known(profile_ids: &[String], name: &str) {
    mark_packs_known(profile_ids, std::iter::once(name));
}

pub fn mark_packs_known<'a>(profile_ids: &[String], pack_names: impl IntoIterator<Item = &'a str>) {
    for error in crate::runtime_mark_packs_known(
        &profiles_root(),
        profile_ids,
        pack_names,
        warn_duplicate_profile_guid,
    ) {
        log_profile_stats_runtime_write_error(error);
    }
}

pub fn write_imported_favorites(profile_id: &str, hashes: &HashSet<String>) {
    crate::merge_imported_favorites_dir(&local_profile_dir(profile_id), hashes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Instant;

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
        let key =
            deadsync_score::player_leaderboard_cache_key("deadbeef", &snapshot).expect("cache key");
        deadsync_score::runtime_seed_player_leaderboard_entry(
            key.clone(),
            deadsync_score::PlayerLeaderboardCacheEntry {
                value: deadsync_score::PlayerLeaderboardCacheValue::Ready(std::sync::Arc::new(
                    deadsync_score::PlayerLeaderboardData {
                        panes: Vec::new(),
                        srpg_self_score: Some(9_910),
                        itl_self_score: None,
                        itl_self_rank: None,
                    },
                )),
                max_entries: 5,
                refreshed_at: Instant::now(),
                retry_after: None,
            },
        );

        assert_eq!(
            deadsync_score::runtime_cached_player_leaderboard_srpg_self_score(
                "deadbeef", &snapshot
            ),
            Some(9_910)
        );

        deadsync_score::runtime_remove_player_leaderboard_entry(&key);
    }

    #[test]
    fn wheel_cache_exposes_profile_backed_srpg_self_score() {
        let profile_id = "profile-backed-srpg-wheel";
        let api_key = "profile-backed-srpg-key";
        let chart_hash = "profile-backed-srpg-chart";
        let snapshot = deadsync_score::scorebox_snapshot(
            true,
            false,
            true,
            true,
            false,
            false,
            api_key,
            "",
            "PerfectTaste",
            Some(profile_id.to_string()),
        );
        assert_eq!(
            deadsync_score::runtime_cached_online_srpg_self_score(
                chart_hash,
                Some(profile_id),
                api_key,
                |_| {
                    HashMap::from([(
                        deadsync_score::OnlineItlSelfScoreKey {
                            chart_hash: chart_hash.to_string(),
                            api_key: api_key.to_string(),
                        },
                        9_876,
                    )])
                },
            ),
            Some(9_876)
        );

        let cache = ItlWheelSideCache::new(&snapshot);
        assert_eq!(cache.cached_srpg_self_score(chart_hash), Some(9_876));
    }

    #[test]
    fn wheel_score_read_does_not_deadlock_with_leaderboard_worker() {
        let profile_id = "test-deadlock-wheel-profile";
        let chart_hash = "feedface";
        let seeded = deadsync_score::CachedScore {
            grade: deadsync_score::Grade::Tier01,
            score_percent: 0.9123,
            lamp_index: Some(2),
            lamp_judge_count: Some(7),
        };
        // Seed every cache the read path consults so it never hits disk and the
        // `ensure_*_loaded` helpers become no-ops during the run.
        seed_session_local_itg_score_for_id(profile_id, chart_hash, seeded);
        seed_session_gs_score_for_id(profile_id, chart_hash, seeded);
        ensure_score_caches_loaded_for_id(profile_id);

        let stop = Arc::new(AtomicBool::new(false));

        // Worker: reproduce the pre-fix lock order
        // (leaderboard -> LOCAL -> GS -> AC). If the wheel read
        // ever again held a score cache across the leaderboard lock, this
        // ordering would close the cycle and deadlock.
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let lb = deadsync_score::runtime_lock_player_leaderboard_cache();
                let caches = deadsync_score::runtime_lock_score_caches();
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
                last = cached_best_itg_score_with_profile(&wheel_chart, &wheel_profile);
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
