use crate::{arrowcloud, groovestats};
use deadsync_profile::Profile;
use deadsync_score::{
    ArrowCloudScores, CachedScore, CachedScoreImportResult, GsLampChartStats, ImportedPlayerScore,
    ScoreBulkImportSummary, ScoreFetchAndCacheInput, ScoreImportEndpoint, ScoreImportProgress,
    ValidatedScoreImportInput, cached_score_import_result_from_imported, fetch_and_cache_score,
    log_score_import_event, run_validated_score_import,
};
use std::collections::HashMap;
use std::error::Error;

pub fn fetch_cached_player_score_import_result<F>(
    endpoint: ScoreImportEndpoint,
    profile: &Profile,
    chart_hash: &str,
    chart_stats: F,
) -> Result<CachedScoreImportResult, Box<dyn Error + Send + Sync>>
where
    F: FnOnce(&ImportedPlayerScore) -> Option<GsLampChartStats>,
{
    let username = profile.score_import_username(endpoint);
    let api_key = profile.score_import_api_key(endpoint);
    let imported = groovestats::fetch_validated_player_score_import_result(
        endpoint, api_key, username, chart_hash,
    )?;
    let stats = imported.score.as_ref().and_then(chart_stats);
    Ok(cached_score_import_result_from_imported(imported, stats))
}

pub fn fetch_and_store_profile_grade<Stats, StoreScore, StoreMissing>(
    endpoint: ScoreImportEndpoint,
    profile: &Profile,
    chart_hash: &str,
    chart_stats: Stats,
    mut store_score: StoreScore,
    store_missing: StoreMissing,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    Stats: Fn(&ImportedPlayerScore) -> Option<GsLampChartStats>,
    StoreScore: FnMut(CachedScore, &str, bool),
    StoreMissing: FnOnce(CachedScore),
{
    let username = profile.score_import_username(endpoint);
    fetch_and_cache_score(
        ScoreFetchAndCacheInput {
            credentials_ready: profile.has_score_import_credentials(endpoint),
            missing_credentials_message: "GrooveStats API key or username is not set in profile.ini.",
            username,
            chart_hash,
        },
        |chart_hash| {
            fetch_cached_player_score_import_result(endpoint, profile, chart_hash, |score| {
                chart_stats(score)
            })
        },
        |score, score_proves_nonquint_ex| {
            store_score(score, username, score_proves_nonquint_ex);
        },
        store_missing,
    )
}

pub fn run_profile_score_import_pack_groups<F, C, GsStore, ItlStore, AcStore, Stats>(
    endpoint: ScoreImportEndpoint,
    profile: &Profile,
    pack_chart_groups: Vec<(String, Vec<String>)>,
    only_missing_scores: bool,
    mut store_gs_score: GsStore,
    store_itl_score: ItlStore,
    cache_ac_scores: AcStore,
    on_progress: F,
    should_cancel: C,
    mut chart_stats: Stats,
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>>
where
    F: FnMut(ScoreImportProgress),
    C: Fn() -> bool,
    GsStore: FnMut(&str, CachedScore, &str, bool),
    ItlStore: FnMut(&str, Option<u32>, Option<u32>),
    AcStore: FnMut(HashMap<String, ArrowCloudScores>),
    Stats: FnMut(&str, &ImportedPlayerScore) -> Option<GsLampChartStats>,
{
    let api_key = profile.score_import_api_key(endpoint);
    let username = profile.score_import_username(endpoint);

    if endpoint == ScoreImportEndpoint::ArrowCloud {
        return arrowcloud::run_validated_bulk_score_import_pack_groups(
            api_key,
            username,
            pack_chart_groups,
            only_missing_scores,
            cache_ac_scores,
            on_progress,
            should_cancel,
        );
    }

    let mut on_progress = on_progress;
    run_validated_score_import(
        ValidatedScoreImportInput {
            endpoint,
            api_key,
            username,
            pack_chart_groups,
            only_missing_scores,
        },
        |chart_hash| {
            fetch_cached_player_score_import_result(endpoint, profile, chart_hash, |score| {
                chart_stats(chart_hash, score)
            })
            .map_err(|error| error.to_string())
        },
        |chart_hash, score, score_proves_nonquint_ex| {
            store_gs_score(chart_hash, score, username, score_proves_nonquint_ex);
        },
        store_itl_score,
        &mut on_progress,
        should_cancel,
        log_score_import_event,
    )
}

pub fn run_profile_score_import<F, C, Collect, GsStore, ItlStore, AcStore, Stats>(
    endpoint: ScoreImportEndpoint,
    profile_id: &str,
    profile: &Profile,
    pack_groups_filter: &[String],
    only_missing_scores: bool,
    collect_chart_hashes: Collect,
    store_gs_score: GsStore,
    store_itl_score: ItlStore,
    cache_ac_scores: AcStore,
    on_progress: F,
    should_cancel: C,
    chart_stats: Stats,
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>>
where
    F: FnMut(ScoreImportProgress),
    C: Fn() -> bool,
    Collect: FnOnce(ScoreImportEndpoint, &[String], &str, bool) -> Vec<(String, Vec<String>)>,
    GsStore: FnMut(&str, CachedScore, &str, bool),
    ItlStore: FnMut(&str, Option<u32>, Option<u32>),
    AcStore: FnMut(HashMap<String, ArrowCloudScores>),
    Stats: FnMut(&str, &ImportedPlayerScore) -> Option<GsLampChartStats>,
{
    let pack_chart_groups = collect_chart_hashes(
        endpoint,
        pack_groups_filter,
        profile_id,
        only_missing_scores,
    );
    run_profile_score_import_pack_groups(
        endpoint,
        profile,
        pack_chart_groups,
        only_missing_scores,
        store_gs_score,
        store_itl_score,
        cache_ac_scores,
        on_progress,
        should_cancel,
        chart_stats,
    )
}

pub fn fetch_and_store_active_service_grade<Stats, StoreScore, StoreMissing>(
    profile: &Profile,
    chart_hash: &str,
    chart_stats: Stats,
    store_score: StoreScore,
    store_missing: StoreMissing,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    Stats: Fn(&ImportedPlayerScore) -> Option<GsLampChartStats>,
    StoreScore: FnMut(CachedScore, &str, bool),
    StoreMissing: FnOnce(CachedScore),
{
    let endpoint = crate::runtime::active_groovestats_service().score_import_endpoint();
    fetch_and_store_profile_grade(
        endpoint,
        profile,
        chart_hash,
        chart_stats,
        store_score,
        store_missing,
    )
}

pub fn import_scores_for_profile_from_app_runtime<F, C>(
    endpoint: ScoreImportEndpoint,
    profile_id: String,
    profile: Profile,
    pack_groups: Vec<String>,
    only_missing_gs_scores: bool,
    on_progress: F,
    should_cancel: C,
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>>
where
    F: FnMut(ScoreImportProgress),
    C: Fn() -> bool,
{
    let api_key = profile.score_import_api_key(endpoint);
    run_profile_score_import(
        endpoint,
        &profile_id,
        &profile,
        &pack_groups,
        only_missing_gs_scores,
        |endpoint, pack_groups_filter, profile_id, only_missing_scores| {
            let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
            deadsync_profile::app_runtime::collect_score_import_chart_hashes(
                endpoint,
                &song_cache,
                pack_groups_filter,
                profile_id,
                only_missing_scores,
            )
        },
        |chart_hash, score, username, score_proves_nonquint_ex| {
            deadsync_profile::app_runtime::cache_logged_gs_score_for_id(
                &profile_id,
                chart_hash,
                score,
                username,
                score_proves_nonquint_ex,
            );
        },
        |chart_hash, itl_self_score, itl_self_rank| {
            deadsync_profile::app_runtime::set_cached_online_itl_self_score(
                Some(&profile_id),
                api_key,
                chart_hash,
                itl_self_score,
            );
            deadsync_profile::app_runtime::set_cached_online_itl_self_rank(
                Some(&profile_id),
                api_key,
                chart_hash,
                itl_self_rank,
            );
        },
        |scores_by_chart| {
            deadsync_profile::app_runtime::write_cached_ac_scores_for_id_bulk(
                &profile_id,
                scores_by_chart,
            );
        },
        on_progress,
        should_cancel,
        |chart_hash, score| {
            let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
            deadsync_score::imported_score_chart_stats(score, &song_cache, chart_hash)
        },
    )
}

pub fn fetch_and_store_grade_from_app_runtime(
    profile_id: String,
    profile: Profile,
    chart_hash: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    fetch_and_store_active_service_grade(
        &profile,
        &chart_hash,
        |score| {
            let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
            deadsync_score::imported_score_chart_stats(score, &song_cache, &chart_hash)
        },
        |score, username, score_proves_nonquint_ex| {
            deadsync_profile::app_runtime::cache_logged_gs_score_for_id(
                &profile_id,
                &chart_hash,
                score,
                username,
                score_proves_nonquint_ex,
            );
        },
        |missing_score| {
            deadsync_profile::app_runtime::write_cached_gs_score_for_id(
                &profile_id,
                chart_hash.clone(),
                missing_score,
            );
        },
    )
}

#[cfg(test)]
#[path = "score_import_perf.rs"]
mod perf_tests;
