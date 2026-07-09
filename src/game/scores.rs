use super::GameplayCoreState;
use crate::game::profile;
use deadsync_profile::Profile;
use deadsync_simfile::runtime_cache::get_song_cache;
use log::{debug, warn};
use std::collections::HashSet;
use std::error::Error;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use deadsync_online::arrowcloud as arrowcloud_api;
use deadsync_online::groovestats as groovestats_api;
use deadsync_profile as profile_data;
use deadsync_profile_gameplay::score_invalid_reason_lines_for_profile;

mod arrowcloud;
mod groovestats;
mod itl;

#[inline(always)]
fn active_groovestats_service() -> groovestats_api::Service {
    deadsync_online::runtime::active_groovestats_service()
}

pub use arrowcloud::{
    retry_arrowcloud_submit, submit_arrowcloud_payloads_from_gameplay, tick_arrowcloud_auto_retries,
};
pub use deadsync_online::arrowcloud::{
    next_retry_is_auto as arrowcloud_next_retry_is_auto,
    next_retry_remaining_secs as arrowcloud_next_retry_remaining_secs,
    submit_ui_status_for_side as get_arrowcloud_submit_ui_status_for_side,
};
pub use deadsync_online::groovestats::{
    next_retry_is_auto as groovestats_next_retry_is_auto,
    next_retry_remaining_secs as groovestats_next_retry_remaining_secs,
    submit_event_progress_for_side as get_groovestats_submit_event_progress_for_side,
    submit_record_banner_for_side as get_groovestats_submit_record_banner_for_side,
    submit_ui_status_for_side as get_groovestats_submit_ui_status_for_side,
};
use deadsync_score::{
    CachedPlayerLeaderboardData, CachedScoreImportResult, GameplayScoreboxProfileSnapshot,
    ImportedPlayerScore, LocalReplayEdgeInput, LocalScoreGameplayPlayer,
    LocalScoreGameplaySaveSkip, PlayerLeaderboardFetchRequest, PlayerLeaderboardFetchSuccess,
    ScoreBulkImportSummary, ScoreFetchAndCacheInput, ScoreImportEndpoint, ScoreImportProgress,
    ValidatedScoreImportInput, cached_score_from_leaderboard_import,
    cached_score_import_result_from_imported,
    collect_chart_hashes_per_pack_for_import as score_collect_chart_hashes_per_pack_for_import,
    fetch_and_cache_score, imported_score_chart_stats, local_replay_edges_for_player,
    log_score_import_event, lua_chart_submit_allowed, run_validated_score_import,
    runtime_cached_player_leaderboard_itl_self_rank,
    runtime_cached_player_leaderboard_srpg_self_score, runtime_plan_player_leaderboard_request,
    runtime_run_player_leaderboard_fetch, save_local_gameplay_scores,
};
pub use deadsync_score::{
    Grade, gameplay_run_failed, gameplay_run_passed, runtime_lock_score_caches as lock_score_caches,
};
#[cfg(test)]
use deadsync_score::{
    player_leaderboard_cache_key, runtime_lock_player_leaderboard_cache,
    runtime_remove_player_leaderboard_entry, runtime_seed_player_leaderboard_entry,
};
pub use groovestats::{
    groovestats_eval_state_from_gameplay, retry_groovestats_submit,
    submit_groovestats_payloads_from_gameplay, tick_groovestats_auto_retries,
};
pub use itl::{
    get_cached_itl_tournament_overall_ranks_for_side, get_cached_itl_tournament_rank_for_side,
    get_or_fetch_itl_self_score_for_side, get_or_fetch_itl_tournament_rank_for_side,
    import_itl_json, is_itl_unlocks_pack, itl_eval_state_from_gameplay, itl_points_for_chart,
    save_itl_data_from_gameplay, should_warn_cmod_for_itl_chart,
};
pub use profile::{
    cached_ac_scores_for_side as get_cached_ac_scores_for_side,
    cached_best_itg_score_for_side as get_cached_score_for_side,
    cached_best_itg_score_with_profile as get_cached_score_with_profile,
    cached_gs_score_for_side as get_cached_gs_score_for_side,
    cached_itl_score_for_side as get_cached_itl_score_for_side,
    cached_itl_score_for_song as get_cached_itl_score_for_song,
    cached_local_ex_score_for_side as get_cached_local_ex_score_for_side,
    cached_local_hard_ex_score_for_side as get_cached_local_hard_ex_score_for_side,
    cached_local_pass_rate_with_profile as get_cached_local_pass_rate_with_profile,
    cached_local_score_for_side as get_cached_local_score_for_side,
    cached_online_itl_self_score_for_side as get_cached_itl_self_score_for_side,
    ensure_itl_wheel_caches_loaded_for_id as ensure_itl_wheel_caches_loaded,
    ensure_score_caches_loaded_for_id as ensure_score_caches_loaded,
    groovestats_score_service_allowed as is_gs_get_scores_service_allowed,
    import_local_scores_for_id as import_local_scores,
    is_groovestats_active_for_side as is_gs_active_for_side,
    itl_song_folder_unlocked_for_side as is_itl_song_folder_unlocked_for_side,
    itl_song_folder_unlocked_with_profile as is_itl_song_folder_unlocked_with_profile,
    machine_leaderboard_local_with_names as get_machine_leaderboard_local_with_names,
    machine_leaderboard_local_without_names as get_machine_leaderboard_local,
    machine_record_local as get_machine_record_local,
    machine_replays_local as get_machine_replays_local,
    personal_leaderboard_local_for_side as get_personal_leaderboard_local_for_side,
    played_chart_counts_for_id as played_chart_counts_for_profile, played_chart_counts_for_machine,
    prewarm_select_music_score_caches,
    recent_played_chart_hashes_for_id as recent_played_chart_hashes_for_profile,
    recent_played_chart_hashes_for_machine, save_local_summary_score_for_side,
    scorebox_profile_snapshot, seed_session_gs_score_for_id as seed_session_gs_score,
    seed_session_itl_unlock_folders,
    seed_session_local_itg_score_for_id as seed_session_local_itg_score,
    seed_session_online_itl_self_rank, seed_session_online_itl_self_score,
    total_songs_played_for_id as total_songs_played_for_profile, total_songs_played_for_side,
};

pub fn save_local_scores_from_gameplay(gs: &GameplayCoreState) {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let players = (0..gs.num_players()).filter_map(|player_idx| {
        let (_side, profile_id) =
            profile::active_local_profile_id_for_gameplay_player(gs.num_players(), player_idx)?;
        let p = &gs.players()[player_idx];
        let totals = gs.display_totals_for_player(player_idx);
        let invalid_reasons = if gs.score_valid_for_player(player_idx) {
            Vec::new()
        } else {
            score_invalid_reason_lines_for_profile(
                &gs.charts()[player_idx],
                &gs.profiles()[player_idx],
                gs.music_rate(),
            )
        };
        let chart_hash = gs.charts()[player_idx].short_hash.as_str();
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

        Some(LocalScoreGameplayPlayer {
            player_idx,
            profile_id,
            profile_initials: gs.profiles()[player_idx].player_initials.as_str(),
            chart_hash,
            invalid_reasons,
            score_valid: gs.score_valid_for_player(player_idx),
            scoring_counts: &p.scoring_counts,
            holds_held_for_score: p.holds_held_for_score,
            rolls_held_for_score: p.rolls_held_for_score,
            mines_hit_for_score: p.mines_hit_for_score,
            possible_grade_points: totals.possible_grade_points,
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
        })
    });

    save_local_gameplay_scores(
        now_ms,
        gs.music_rate(),
        gs.autoplay_used(),
        players,
        profile::append_local_score_for_id,
        |skip| match skip {
            LocalScoreGameplaySaveSkip::Autoplay => {
                debug!("Skipping local score save: autoplay was used during this stage.");
            }
            LocalScoreGameplaySaveSkip::Invalid { player_idx, detail } => {
                debug!(
                    "Skipping local score save for player {}: {}.",
                    player_idx + 1,
                    detail
                );
            }
        },
    );
}

const EVENT_WHEEL_FETCH_ENTRIES: usize = 5;

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
        profile::cached_local_itg_score_for_id(profile_id, chart_hash),
        stats,
    ) else {
        // Select Music and gameplay scoreboxes fetch shallow leaderboard pages.
        // If the page does not include the player's row, do not clobber an
        // existing cached GS score and make the wheel fall back to local data.
        return;
    };
    profile::cache_logged_gs_score_for_id(
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

    let combined = groovestats_api::fetch_validated_combined_player_leaderboards(
        active_groovestats_service(),
        api_key,
        username,
        chart_hash,
        arrowcloud_api_key,
        show_ex_score,
        max_entries,
    )?;
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
        let result =
            runtime_run_player_leaderboard_fetch(request, |key, gs_username, max_entries| {
                fetch_player_leaderboards_internal(
                    &key.chart_hash,
                    &key.api_key,
                    gs_username,
                    if key.include_arrowcloud {
                        Some(key.arrowcloud_api_key.as_str())
                    } else {
                        None
                    },
                    key.show_ex_score,
                    max_entries,
                )
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
                .map_err(|error| error.to_string())
            });

        // Keep score/ITL cache writes outside the leaderboard cache. The wheel
        // can hold score-cache guards while probing leaderboard rank state.
        if let Some((itl_self_score, itl_self_rank)) = result.completion.fetched_itl_self {
            itl::set_cached_online_self_score(
                result.persistent_profile_id.as_deref(),
                result.key.api_key.as_str(),
                result.key.chart_hash.as_str(),
                itl_self_score,
            );
            itl::set_cached_online_self_rank(
                result.persistent_profile_id.as_deref(),
                result.key.api_key.as_str(),
                result.key.chart_hash.as_str(),
                itl_self_rank,
            );
        }
        if let Some(imported_score) = result.completion.fetched_imported_score
            && let Some(profile_id) = result.auto_profile_id.as_deref()
        {
            cache_gs_score_from_leaderboard(
                profile_id,
                result.gs_username.as_str(),
                result.key.chart_hash.as_str(),
                Some(imported_score),
            );
        }

        if let Some(queued_fetch) = result.completion.queued_fetch {
            spawn_player_leaderboard_fetch(PlayerLeaderboardFetchRequest {
                key: queued_fetch.key,
                gs_username: result.gs_username,
                persistent_profile_id: result.persistent_profile_id,
                auto_profile_id: result.auto_profile_id,
                should_auto_populate: result.should_auto_populate,
                max_entries: queued_fetch.max_entries,
            });
        }
    });
}

#[inline(always)]
fn player_leaderboard_profile_snapshot_for_side(
    side: profile_data::PlayerSide,
) -> GameplayScoreboxProfileSnapshot {
    profile::player_leaderboard_profile_snapshot_for_side(side)
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
        profile::cached_itl_score_for_song_assume_loaded(song, self.profile_id.as_deref())
    }

    /// Cached online ITL self EX score for a chart hash, in integer hundredths
    /// of a percent (e.g. `9912` = 99.12%).
    pub fn cached_self_ex_score(&self, chart_hash: &str) -> Option<u32> {
        profile::cached_online_itl_self_score_for_key_assume_loaded(
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
                profile::cached_online_itl_self_rank_for_key_assume_loaded(
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
    profile::invalidate_player_leaderboard_chart_for_side(chart_hash, side, Instant::now());
}

// --- Public Fetch Function ---

fn fetch_player_score_from_endpoint(
    endpoint: ScoreImportEndpoint,
    profile: &Profile,
    chart_hash: &str,
) -> Result<CachedScoreImportResult, Box<dyn Error + Send + Sync>> {
    let username = profile.score_import_username(endpoint);
    let api_key = profile.score_import_api_key(endpoint);

    let imported = groovestats_api::fetch_validated_player_score_import_result(
        endpoint, api_key, username, chart_hash,
    )?;
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
        profile::cached_gs_chart_hashes_for_id(profile_id)
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
/// `/v1/retrieve-scores`, splitting large packs into chunks. Throttled to the
/// shared score-import rate limit to match the per-chart paths.
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
    let api_key = profile.score_import_api_key(ScoreImportEndpoint::ArrowCloud);
    let username = profile.score_import_username(ScoreImportEndpoint::ArrowCloud);
    let existing_scores = if only_missing_scores {
        profile::cached_ac_chart_hashes_with_itg_for_id(&profile_id)
    } else {
        HashSet::new()
    };
    let song_cache = get_song_cache();
    let pack_chart_groups =
        score_collect_chart_hashes_per_pack_for_import(&song_cache, &pack_groups, &existing_scores);
    arrowcloud_api::run_validated_bulk_score_import_pack_groups(
        api_key,
        username,
        pack_chart_groups,
        only_missing_scores,
        |scores_by_chart| {
            profile::write_cached_ac_scores_for_id_bulk(&profile_id, scores_by_chart.into_iter());
        },
        on_progress,
        should_cancel,
    )
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
    let username = profile.score_import_username(endpoint);
    let pack_chart_groups =
        collect_chart_hashes_for_import(&pack_groups, &profile_id, only_missing_gs_scores);
    run_validated_score_import(
        ValidatedScoreImportInput {
            endpoint,
            api_key,
            username,
            pack_chart_groups,
            only_missing_scores: only_missing_gs_scores,
        },
        |chart_hash| {
            fetch_player_score_from_endpoint(endpoint, &profile, chart_hash)
                .map_err(|error| error.to_string())
        },
        |chart_hash, score, score_proves_nonquint_ex| {
            profile::cache_logged_gs_score_for_id(
                &profile_id,
                chart_hash,
                score,
                username,
                score_proves_nonquint_ex,
            );
        },
        |chart_hash, itl_self_score, itl_self_rank| {
            itl::set_cached_online_self_score(
                Some(profile_id.as_str()),
                api_key,
                chart_hash,
                itl_self_score,
            );
            itl::set_cached_online_self_rank(
                Some(profile_id.as_str()),
                api_key,
                chart_hash,
                itl_self_rank,
            );
        },
        &mut on_progress,
        should_cancel,
        log_score_import_event,
    )
}

pub fn fetch_and_store_grade(
    profile_id: String,
    profile: Profile,
    chart_hash: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let endpoint = active_groovestats_service().score_import_endpoint();
    let username = profile.score_import_username(endpoint);
    let missing_chart_hash = chart_hash.clone();
    fetch_and_cache_score(
        ScoreFetchAndCacheInput {
            credentials_ready: profile.has_score_import_credentials(endpoint),
            missing_credentials_message: "GrooveStats API key or username is not set in profile.ini.",
            username,
            chart_hash: chart_hash.as_str(),
        },
        |chart_hash| fetch_player_score_from_endpoint(endpoint, &profile, chart_hash),
        |cached_score, score_proves_nonquint_ex| {
            profile::cache_logged_gs_score_for_id(
                &profile_id,
                chart_hash.as_str(),
                cached_score,
                username,
                score_proves_nonquint_ex,
            );
        },
        |missing_score| {
            profile::write_cached_gs_score_for_id(&profile_id, missing_chart_hash, missing_score);
        },
    )
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
