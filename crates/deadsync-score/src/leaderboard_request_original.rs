// Frozen from main 6b440b74a; the state method is renamed to coexist with production.
use super::*;

pub fn runtime_plan_player_leaderboard_request(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
    max_entries: usize,
    refresh_cached: bool,
    now: Instant,
) -> Option<PlayerLeaderboardRequestPlan> {
    if max_entries == 0 {
        return None;
    }
    let key = player_leaderboard_cache_key(chart_hash, profile_snapshot)?;
    let gs_username = profile_snapshot.gs_username().to_string();
    let persistent_profile_id = profile_snapshot.persistent_profile_id().map(str::to_string);
    let auto_profile_id = profile_snapshot.auto_profile_id().map(str::to_string);
    let should_auto_populate = profile_snapshot.should_auto_populate();
    let decision = runtime_request_player_leaderboard(&key, max_entries, refresh_cached, now);
    let fetch = decision
        .should_spawn
        .then_some(PlayerLeaderboardFetchRequest {
            key,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
            max_entries: decision.requested_max_entries,
        });

    Some(PlayerLeaderboardRequestPlan {
        snapshot: decision.snapshot,
        fetch,
    })
}

pub fn runtime_request_player_leaderboard(
    key: &PlayerLeaderboardCacheKey,
    max_entries: usize,
    refresh_cached: bool,
    now: Instant,
) -> PlayerLeaderboardRequestDecision {
    RUNTIME_PLAYER_LEADERBOARD_CACHE
        .lock()
        .unwrap()
        .original_request_leaderboard(key, max_entries, refresh_cached, now)
}

impl PlayerLeaderboardCacheState {
    pub fn original_request_leaderboard(
        &mut self,
        key: &PlayerLeaderboardCacheKey,
        max_entries: usize,
        refresh_cached: bool,
        now: Instant,
    ) -> PlayerLeaderboardRequestDecision {
        let entry = self.by_key.get(key);
        let requested_max_entries =
            entry.map_or(max_entries, |entry| max_entries.max(entry.max_entries));
        let snapshot = entry.map_or_else(
            player_leaderboard_loading_snapshot,
            player_leaderboard_snapshot_from_entry,
        );

        let mut should_spawn = false;
        if should_fetch_player_leaderboard_entry(entry, requested_max_entries, refresh_cached, now)
        {
            if let Some(in_flight_max_entries) = self.in_flight.get(key).copied() {
                if should_rerun_in_flight_player_leaderboard_fetch(
                    in_flight_max_entries,
                    requested_max_entries,
                    refresh_cached,
                ) {
                    queue_player_leaderboard_refresh(
                        &mut self.pending_refresh,
                        key,
                        requested_max_entries,
                    );
                }
            } else {
                self.in_flight.insert(key.clone(), requested_max_entries);
                should_spawn = true;
            }
        }

        PlayerLeaderboardRequestDecision {
            snapshot,
            should_spawn,
            requested_max_entries,
        }
    }
}
