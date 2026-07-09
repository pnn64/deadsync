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
use std::sync::Arc;

type CollectChartHashes =
    dyn Fn(ScoreImportEndpoint, &[String], &str, bool) -> Vec<(String, Vec<String>)> + Send + Sync;
type StoreGsScore = dyn Fn(&str, &str, CachedScore, &str, bool) + Send + Sync;
type StoreItlSelf = dyn Fn(&str, &str, &str, Option<u32>, Option<u32>) + Send + Sync;
type CacheAcScores = dyn Fn(&str, HashMap<String, ArrowCloudScores>) + Send + Sync;
type StoreMissingGsScore = dyn Fn(&str, &str, CachedScore) + Send + Sync;
type ChartStats = dyn Fn(&str, &ImportedPlayerScore) -> Option<GsLampChartStats> + Send + Sync;

#[derive(Clone)]
pub struct ScoreImportRuntime {
    collect_chart_hashes: Arc<CollectChartHashes>,
    store_gs_score: Arc<StoreGsScore>,
    store_itl_self: Arc<StoreItlSelf>,
    cache_ac_scores: Arc<CacheAcScores>,
    store_missing_gs_score: Arc<StoreMissingGsScore>,
    chart_stats: Arc<ChartStats>,
}

impl ScoreImportRuntime {
    pub fn new<Collect, StoreGs, StoreItl, CacheAc, StoreMissing, Stats>(
        collect_chart_hashes: Collect,
        store_gs_score: StoreGs,
        store_itl_self: StoreItl,
        cache_ac_scores: CacheAc,
        store_missing_gs_score: StoreMissing,
        chart_stats: Stats,
    ) -> Self
    where
        Collect: Fn(ScoreImportEndpoint, &[String], &str, bool) -> Vec<(String, Vec<String>)>
            + Send
            + Sync
            + 'static,
        StoreGs: Fn(&str, &str, CachedScore, &str, bool) + Send + Sync + 'static,
        StoreItl: Fn(&str, &str, &str, Option<u32>, Option<u32>) + Send + Sync + 'static,
        CacheAc: Fn(&str, HashMap<String, ArrowCloudScores>) + Send + Sync + 'static,
        StoreMissing: Fn(&str, &str, CachedScore) + Send + Sync + 'static,
        Stats: Fn(&str, &ImportedPlayerScore) -> Option<GsLampChartStats> + Send + Sync + 'static,
    {
        Self {
            collect_chart_hashes: Arc::new(collect_chart_hashes),
            store_gs_score: Arc::new(store_gs_score),
            store_itl_self: Arc::new(store_itl_self),
            cache_ac_scores: Arc::new(cache_ac_scores),
            store_missing_gs_score: Arc::new(store_missing_gs_score),
            chart_stats: Arc::new(chart_stats),
        }
    }

    pub fn from_app_runtime() -> Self {
        Self::new(
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
            |profile_id, chart_hash, score, username, score_proves_nonquint_ex| {
                deadsync_profile::app_runtime::cache_logged_gs_score_for_id(
                    profile_id,
                    chart_hash,
                    score,
                    username,
                    score_proves_nonquint_ex,
                );
            },
            |profile_id, api_key, chart_hash, itl_self_score, itl_self_rank| {
                deadsync_profile::app_runtime::set_cached_online_itl_self_score(
                    Some(profile_id),
                    api_key,
                    chart_hash,
                    itl_self_score,
                );
                deadsync_profile::app_runtime::set_cached_online_itl_self_rank(
                    Some(profile_id),
                    api_key,
                    chart_hash,
                    itl_self_rank,
                );
            },
            |profile_id, scores_by_chart| {
                deadsync_profile::app_runtime::write_cached_ac_scores_for_id_bulk(
                    profile_id,
                    scores_by_chart,
                );
            },
            |profile_id, chart_hash, missing_score| {
                deadsync_profile::app_runtime::write_cached_gs_score_for_id(
                    profile_id,
                    chart_hash.to_string(),
                    missing_score,
                );
            },
            |chart_hash, score| {
                let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
                deadsync_score::imported_score_chart_stats(score, &song_cache, chart_hash)
            },
        )
    }

    pub fn import_scores_for_profile<F, C>(
        &self,
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
        let collect = Arc::clone(&self.collect_chart_hashes);
        let store_gs = Arc::clone(&self.store_gs_score);
        let store_itl = Arc::clone(&self.store_itl_self);
        let cache_ac = Arc::clone(&self.cache_ac_scores);
        let chart_stats = Arc::clone(&self.chart_stats);
        let gs_profile_id = profile_id.clone();
        let itl_profile_id = profile_id.clone();
        let ac_profile_id = profile_id.clone();
        let api_key = profile.score_import_api_key(endpoint).to_string();
        run_profile_score_import(
            endpoint,
            &profile_id,
            &profile,
            &pack_groups,
            only_missing_gs_scores,
            move |endpoint, pack_groups_filter, profile_id, only_missing_scores| {
                collect(
                    endpoint,
                    pack_groups_filter,
                    profile_id,
                    only_missing_scores,
                )
            },
            move |chart_hash, score, username, score_proves_nonquint_ex| {
                store_gs(
                    gs_profile_id.as_str(),
                    chart_hash,
                    score,
                    username,
                    score_proves_nonquint_ex,
                );
            },
            move |chart_hash, itl_self_score, itl_self_rank| {
                store_itl(
                    itl_profile_id.as_str(),
                    api_key.as_str(),
                    chart_hash,
                    itl_self_score,
                    itl_self_rank,
                );
            },
            move |scores_by_chart| {
                cache_ac(ac_profile_id.as_str(), scores_by_chart);
            },
            on_progress,
            should_cancel,
            move |chart_hash, score| chart_stats(chart_hash, score),
        )
    }

    pub fn fetch_and_store_grade(
        &self,
        profile_id: String,
        profile: Profile,
        chart_hash: String,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let chart_stats = Arc::clone(&self.chart_stats);
        let store_gs = Arc::clone(&self.store_gs_score);
        let store_missing = Arc::clone(&self.store_missing_gs_score);
        let stats_chart_hash = chart_hash.clone();
        let score_profile_id = profile_id.clone();
        let score_chart_hash = chart_hash.clone();
        let missing_profile_id = profile_id.clone();
        let missing_chart_hash = chart_hash.clone();
        fetch_and_store_active_service_grade(
            &profile,
            chart_hash.as_str(),
            move |score| chart_stats(stats_chart_hash.as_str(), score),
            move |cached_score, username, score_proves_nonquint_ex| {
                store_gs(
                    score_profile_id.as_str(),
                    score_chart_hash.as_str(),
                    cached_score,
                    username,
                    score_proves_nonquint_ex,
                );
            },
            move |missing_score| {
                store_missing(
                    missing_profile_id.as_str(),
                    missing_chart_hash.as_str(),
                    missing_score,
                );
            },
        )
    }
}

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
    ScoreImportRuntime::from_app_runtime().import_scores_for_profile(
        endpoint,
        profile_id,
        profile,
        pack_groups,
        only_missing_gs_scores,
        on_progress,
        should_cancel,
    )
}

pub fn fetch_and_store_grade_from_app_runtime(
    profile_id: String,
    profile: Profile,
    chart_hash: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    ScoreImportRuntime::from_app_runtime().fetch_and_store_grade(profile_id, profile, chart_hash)
}
