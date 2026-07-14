use crate::groovestats;
use deadsync_profile::PlayerSide;
use deadsync_score::{
    CachedItlScore, CachedPlayerLeaderboardData, GameplayScoreboxProfileSnapshot,
    ImportedPlayerScore, PlayerLeaderboardFetchRequest, PlayerLeaderboardFetchSuccess,
    imported_score_chart_stats, runtime_plan_player_leaderboard_request,
    runtime_run_player_leaderboard_fetch,
};
use log::warn;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

const EVENT_WHEEL_FETCH_ENTRIES: usize = 5;
const ITL_WHEEL_FETCH_ENTRIES: usize = 5;

#[derive(Clone, Copy)]
pub struct PlayerLeaderboardFetchHandlers {
    pub cache_itl_self: fn(Option<String>, String, String, Option<u32>, Option<u32>),
    pub cache_srpg_self_score: fn(Option<String>, String, String, u32),
    pub cache_imported_score: fn(String, String, String, ImportedPlayerScore),
}

pub fn fetch_player_leaderboards(
    service: groovestats::Service,
    chart_hash: &str,
    api_key: &str,
    username: &str,
    arrowcloud_api_key: Option<&str>,
    show_ex_score: bool,
    max_entries: usize,
) -> Result<groovestats::FetchedPlayerLeaderboards, Box<dyn Error + Send + Sync>> {
    let combined = groovestats::fetch_validated_combined_player_leaderboards(
        service,
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

pub fn spawn_player_leaderboard_fetch(
    service: groovestats::Service,
    request: PlayerLeaderboardFetchRequest,
    handlers: PlayerLeaderboardFetchHandlers,
) {
    std::thread::spawn(move || {
        let result =
            runtime_run_player_leaderboard_fetch(request, |key, gs_username, max_entries| {
                fetch_player_leaderboards(
                    service,
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
                    |groovestats::FetchedPlayerLeaderboards {
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

        if let Some((itl_self_score, itl_self_rank)) = result.completion.fetched_itl_self {
            (handlers.cache_itl_self)(
                result.persistent_profile_id.clone(),
                result.key.api_key.clone(),
                result.key.chart_hash.clone(),
                itl_self_score,
                itl_self_rank,
            );
        }
        if let Some(srpg_self_score) = result.completion.fetched_srpg_self_score {
            (handlers.cache_srpg_self_score)(
                result.persistent_profile_id.clone(),
                result.key.api_key.clone(),
                result.key.chart_hash.clone(),
                srpg_self_score,
            );
        }
        if let Some(imported_score) = result.completion.fetched_imported_score
            && let Some(profile_id) = result.auto_profile_id.as_deref()
        {
            (handlers.cache_imported_score)(
                profile_id.to_string(),
                result.gs_username.clone(),
                result.key.chart_hash.clone(),
                imported_score,
            );
        }

        if let Some(queued_fetch) = result.completion.queued_fetch {
            spawn_player_leaderboard_fetch(
                service,
                PlayerLeaderboardFetchRequest {
                    key: queued_fetch.key,
                    gs_username: result.gs_username,
                    persistent_profile_id: result.persistent_profile_id,
                    auto_profile_id: result.auto_profile_id,
                    should_auto_populate: result.should_auto_populate,
                    max_entries: queued_fetch.max_entries,
                },
                handlers,
            );
        }
    });
}

pub fn get_or_fetch_player_leaderboards(
    service: groovestats::Service,
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
    max_entries: usize,
    refresh_cached: bool,
    handlers: PlayerLeaderboardFetchHandlers,
) -> Option<CachedPlayerLeaderboardData> {
    let plan = runtime_plan_player_leaderboard_request(
        chart_hash,
        profile_snapshot,
        max_entries,
        refresh_cached,
        Instant::now(),
    )?;
    if let Some(fetch) = plan.fetch {
        spawn_player_leaderboard_fetch(service, fetch, handlers);
    }

    Some(plan.snapshot)
}

#[derive(Clone, Copy)]
pub struct PlayerLeaderboardRuntime {
    service: groovestats::Service,
    handlers: PlayerLeaderboardFetchHandlers,
    profile_snapshot_for_side: fn(PlayerSide) -> GameplayScoreboxProfileSnapshot,
}

impl PlayerLeaderboardRuntime {
    pub fn from_app_runtime() -> Self {
        Self {
            service: crate::runtime::active_groovestats_service(),
            handlers: PlayerLeaderboardFetchHandlers {
                cache_itl_self,
                cache_srpg_self_score,
                cache_imported_score,
            },
            profile_snapshot_for_side,
        }
    }

    pub fn get_or_fetch_for_side(
        &self,
        chart_hash: &str,
        side: PlayerSide,
        max_entries: usize,
    ) -> Option<CachedPlayerLeaderboardData> {
        let profile_snapshot = (self.profile_snapshot_for_side)(side);
        self.get_or_fetch_for_profile(chart_hash, &profile_snapshot, max_entries)
    }

    pub fn get_or_fetch_for_profile(
        &self,
        chart_hash: &str,
        profile_snapshot: &GameplayScoreboxProfileSnapshot,
        max_entries: usize,
    ) -> Option<CachedPlayerLeaderboardData> {
        get_or_fetch_player_leaderboards(
            self.service,
            chart_hash,
            profile_snapshot,
            max_entries,
            false,
            self.handlers,
        )
    }

    pub fn refresh_for_side(
        &self,
        chart_hash: &str,
        side: PlayerSide,
        max_entries: usize,
    ) -> Option<CachedPlayerLeaderboardData> {
        let profile_snapshot = (self.profile_snapshot_for_side)(side);
        get_or_fetch_player_leaderboards(
            self.service,
            chart_hash,
            &profile_snapshot,
            max_entries,
            true,
            self.handlers,
        )
    }

    pub fn invalidate_for_side(&self, chart_hash: &str, side: PlayerSide) {
        deadsync_profile::app_runtime::invalidate_player_leaderboard_chart_for_side(
            chart_hash,
            side,
            Instant::now(),
        );
    }

    pub fn wheel_profile_context<'a>(
        &self,
        leaderboard_snapshot: &'a GameplayScoreboxProfileSnapshot,
    ) -> ItlWheelSideContext<'a> {
        ItlWheelSideContext {
            cache: deadsync_profile::app_runtime::ItlWheelSideCache::new(leaderboard_snapshot),
            runtime: *self,
        }
    }
}

fn cache_itl_self(
    profile_id: Option<String>,
    api_key: String,
    chart_hash: String,
    itl_self_score: Option<u32>,
    itl_self_rank: Option<u32>,
) {
    deadsync_profile::app_runtime::set_cached_online_itl_self_score(
        profile_id.as_deref(),
        api_key.as_str(),
        chart_hash.as_str(),
        itl_self_score,
    );
    deadsync_profile::app_runtime::set_cached_online_itl_self_rank(
        profile_id.as_deref(),
        api_key.as_str(),
        chart_hash.as_str(),
        itl_self_rank,
    );
}

fn cache_srpg_self_score(
    profile_id: Option<String>,
    api_key: String,
    chart_hash: String,
    score: u32,
) {
    deadsync_profile::app_runtime::set_cached_online_srpg_self_score(
        profile_id.as_deref(),
        api_key.as_str(),
        chart_hash.as_str(),
        Some(score),
    );
}

fn cache_imported_score(
    profile_id: String,
    username: String,
    chart_hash: String,
    imported: ImportedPlayerScore,
) {
    let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
    deadsync_profile::app_runtime::cache_gs_score_from_leaderboard_import(
        profile_id.as_str(),
        username.as_str(),
        chart_hash.as_str(),
        imported,
        |score| imported_score_chart_stats(score, &song_cache, chart_hash.as_str()),
    );
}

fn profile_snapshot_for_side(side: PlayerSide) -> GameplayScoreboxProfileSnapshot {
    let cfg = deadsync_config::runtime::get();
    deadsync_profile::runtime_scorebox_profile_snapshot_for_side(
        side,
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
    )
}

pub struct ItlWheelSideContext<'a> {
    cache: deadsync_profile::app_runtime::ItlWheelSideCache<'a>,
    runtime: PlayerLeaderboardRuntime,
}

impl<'a> ItlWheelSideContext<'a> {
    pub fn for_profile(profile: &'a GameplayScoreboxProfileSnapshot) -> Self {
        PlayerLeaderboardRuntime::from_app_runtime().wheel_profile_context(profile)
    }

    pub fn cached_local_itl_score(
        &self,
        song: &deadsync_chart::SongData,
    ) -> Option<CachedItlScore> {
        self.cache.cached_local_itl_score(song)
    }

    pub fn cached_self_ex_score(&self, chart_hash: &str) -> Option<u32> {
        self.cache.cached_self_ex_score(chart_hash)
    }

    pub fn get_or_fetch_self_ex_score(&self, chart_hash: &str) -> Option<u32> {
        if let Some(score) = self.cached_self_ex_score(chart_hash) {
            return Some(score);
        }
        // Keep wheel prefetches aligned with the Select Music scorebox cache
        // width so a smaller request cannot seed a temporarily truncated pane.
        let _ = self.runtime.get_or_fetch_for_profile(
            chart_hash,
            self.cache.leaderboard_snapshot(),
            ITL_WHEEL_FETCH_ENTRIES,
        )?;
        self.cached_self_ex_score(chart_hash)
    }

    pub fn cached_srpg_self_score(&self, chart_hash: &str) -> Option<u32> {
        self.cache.cached_srpg_self_score(chart_hash)
    }

    pub fn get_or_fetch_srpg_self_score(&self, chart_hash: &str) -> Option<u32> {
        if let Some(score) = self.cached_srpg_self_score(chart_hash) {
            return Some(score);
        }
        let _ = self.runtime.get_or_fetch_for_profile(
            chart_hash,
            self.cache.leaderboard_snapshot(),
            EVENT_WHEEL_FETCH_ENTRIES,
        )?;
        self.cached_srpg_self_score(chart_hash)
    }

    pub fn cached_tournament_rank(&self, chart_hash: &str) -> Option<u32> {
        self.cache.cached_tournament_rank(chart_hash)
    }

    pub fn get_or_fetch_tournament_rank(&self, chart_hash: &str) -> Option<u32> {
        if let Some(rank) = self.cached_tournament_rank(chart_hash) {
            return Some(rank);
        }
        let _ = self.runtime.get_or_fetch_for_profile(
            chart_hash,
            self.cache.leaderboard_snapshot(),
            ITL_WHEEL_FETCH_ENTRIES,
        )?;
        self.cached_tournament_rank(chart_hash)
    }
}

pub fn get_or_fetch_player_leaderboards_for_side_from_app_runtime(
    chart_hash: &str,
    side: PlayerSide,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    PlayerLeaderboardRuntime::from_app_runtime().get_or_fetch_for_side(
        chart_hash,
        side,
        max_entries,
    )
}

pub fn get_or_fetch_player_leaderboards_for_profile_from_app_runtime(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    PlayerLeaderboardRuntime::from_app_runtime().get_or_fetch_for_profile(
        chart_hash,
        profile_snapshot,
        max_entries,
    )
}

pub fn refresh_player_leaderboards_for_side_from_app_runtime(
    chart_hash: &str,
    side: PlayerSide,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    PlayerLeaderboardRuntime::from_app_runtime().refresh_for_side(chart_hash, side, max_entries)
}

pub fn invalidate_player_leaderboards_for_side_from_app_runtime(
    chart_hash: &str,
    side: PlayerSide,
) {
    PlayerLeaderboardRuntime::from_app_runtime().invalidate_for_side(chart_hash, side);
}

pub fn cached_itl_tournament_overall_ranks_for_profile_from_app_runtime(
    side_idx: usize,
    joined: bool,
    profile: &GameplayScoreboxProfileSnapshot,
) -> Arc<HashMap<String, u32>> {
    let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
    deadsync_profile::app_runtime::cached_itl_tournament_overall_ranks_for_profile(
        side_idx,
        joined,
        profile.api_key(),
        profile.persistent_profile_id(),
        deadsync_simfile::runtime_cache::song_cache_generation(),
        song_cache.as_slice(),
    )
}
