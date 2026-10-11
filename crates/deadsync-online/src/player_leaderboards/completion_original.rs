#[derive(Clone, Copy)]
pub struct PlayerLeaderboardFetchHandlers {
    pub cache_itl_self: fn(Option<String>, String, String, Option<u32>, Option<u32>),
    pub cache_srpg_self_score: fn(Option<String>, String, String, u32),
    pub cache_imported_score: fn(String, String, String, ImportedPlayerScore),
}

fn complete(
    handlers: PlayerLeaderboardFetchHandlers,
    result: deadsync_score::PlayerLeaderboardFetchJobResult<ImportedPlayerScore>,
) -> Option<PlayerLeaderboardFetchRequest> {
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
        return Some(PlayerLeaderboardFetchRequest {
            key: queued_fetch.key,
            gs_username: result.gs_username,
            persistent_profile_id: result.persistent_profile_id,
            auto_profile_id: result.auto_profile_id,
            should_auto_populate: result.should_auto_populate,
            max_entries: queued_fetch.max_entries,
        });
    }
    None
}
