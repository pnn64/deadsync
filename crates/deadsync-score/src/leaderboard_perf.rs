// Frozen from starting main c28a030a8 for differential tests and paired benchmarks.
use super::*;
use std::hint::black_box;

fn key() -> PlayerLeaderboardCacheKey {
    PlayerLeaderboardCacheKey {
        chart_hash: "0123456789abcdef".into(),
        api_key: "g".repeat(64),
        arrowcloud_api_key: "a".repeat(64),
        include_arrowcloud: true,
        show_ex_score: true,
    }
}

fn data(score: u32) -> PlayerLeaderboardData {
    PlayerLeaderboardData {
        panes: Vec::new(),
        srpg_self_score: Some(score),
        itl_self_score: Some(score - 1),
        itl_self_rank: Some(7),
    }
}

fn fetched(
    success: bool,
    itl_self_found: bool,
) -> Result<PlayerLeaderboardFetchSuccess<u32>, String> {
    if success {
        Ok(PlayerLeaderboardFetchSuccess {
            data: data(9900),
            imported_score: Some(42),
            itl_self_found,
        })
    } else {
        Err("network".into())
    }
}

fn cache(
    key: &PlayerLeaderboardCacheKey,
    state: u8,
    stamp: Instant,
) -> PlayerLeaderboardCacheState {
    let mut cache = PlayerLeaderboardCacheState::default();
    if state != 0 {
        cache.by_key.insert(
            key.clone(),
            PlayerLeaderboardCacheEntry {
                value: if state == 1 {
                    PlayerLeaderboardCacheValue::Ready(Arc::new(data(9800)))
                } else {
                    PlayerLeaderboardCacheValue::Error("old error".into())
                },
                max_entries: 5,
                refreshed_at: stamp,
                retry_after: Some(stamp + Duration::from_secs(10)),
            },
        );
    }
    cache
}

fn effects(
    completion: PlayerLeaderboardFetchCompletion<u32>,
) -> (
    Option<(Option<u32>, Option<u32>)>,
    Option<u32>,
    Option<u32>,
    Option<(PlayerLeaderboardCacheKey, usize)>,
) {
    (
        completion.fetched_itl_self,
        completion.fetched_srpg_self_score,
        completion.fetched_imported_score,
        completion
            .queued_fetch
            .map(|queued| (queued.key, queued.max_entries)),
    )
}

#[test]
fn refreshing_in_place_preserves_staleness_invalidation_and_queued_effects() {
    let key = key();
    let start = Instant::now();
    let finish = start + Duration::from_millis(5);
    for state in 0..3 {
        for stamp in [start - Duration::from_secs(1), start, finish] {
            for invalidated in [false, true] {
                for queued in [false, true] {
                    for success in [false, true] {
                        for flags in 0..8 {
                            let mut original = cache(&key, state, stamp);
                            let mut current = cache(&key, state, stamp);
                            for cache in [&mut original, &mut current] {
                                cache.in_flight.insert(key.clone(), 5);
                                if invalidated {
                                    cache.invalidated_after.insert(key.clone(), finish);
                                }
                                if queued {
                                    cache.pending_refresh.insert(key.clone(), 10);
                                }
                            }
                            let expected = original_complete(
                                &mut original,
                                &key,
                                8,
                                start,
                                finish,
                                Duration::from_secs(10),
                                fetched(success, flags & 1 != 0),
                                flags & 2 != 0,
                                flags & 4 != 0,
                            );
                            let actual = current.complete_fetch(
                                &key,
                                8,
                                start,
                                finish,
                                Duration::from_secs(10),
                                fetched(success, flags & 1 != 0),
                                flags & 2 != 0,
                                flags & 4 != 0,
                            );
                            assert_eq!(effects(actual), effects(expected));
                            assert_eq!(
                                format!("{:?}", current.by_key),
                                format!("{:?}", original.by_key)
                            );
                            assert_eq!(current.in_flight, original.in_flight);
                            assert_eq!(current.pending_refresh, original.pending_refresh);
                            assert_eq!(current.invalidated_after, original.invalidated_after);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn refresh_reuses_keys_and_keeps_previous_snapshots_stable() {
    let key = key();
    let now = Instant::now();
    let mut original = cache(&key, 1, now);
    let mut current = cache(&key, 1, now);
    let snapshot = player_leaderboard_snapshot_from_entry(current.by_key.get(&key).unwrap());
    let result = fetched(true, true);
    let (_, before) = crate::perf::measure(|| {
        original_complete(
            &mut original,
            &key,
            8,
            now,
            now,
            Duration::from_secs(10),
            result,
            true,
            true,
        )
    });
    let result = fetched(true, true);
    let (_, after) = crate::perf::measure(|| {
        current.complete_fetch(
            &key,
            8,
            now,
            now,
            Duration::from_secs(10),
            result,
            true,
            true,
        )
    });
    assert_eq!(before.allocs - after.allocs, 3);
    assert_eq!(before.allocated_bytes - after.allocated_bytes, 144);
    assert_eq!(after.reallocs, 0);
    assert_eq!(snapshot.data.as_ref().unwrap().srpg_self_score, Some(9800));
    assert!(
        matches!(&current.by_key[&key].value, PlayerLeaderboardCacheValue::Ready(data) if data.srpg_self_score == Some(9900))
    );
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_leaderboard_refresh() {
    let key = key();
    let now = Instant::now();
    for (state, label) in [
        (1, "leaderboard ready refresh"),
        (2, "leaderboard error recovery"),
        (0, "leaderboard first fetch control"),
    ] {
        let mut prepared = cache(&key, state, now);
        let result = fetched(true, true);
        let (_, original) = crate::perf::measure(|| {
            original_complete(
                &mut prepared,
                &key,
                8,
                now,
                now,
                Duration::from_secs(10),
                result,
                true,
                true,
            )
        });
        let mut prepared = cache(&key, state, now);
        let result = fetched(true, true);
        let (_, current) = crate::perf::measure(|| {
            prepared.complete_fetch(
                &key,
                8,
                now,
                now,
                Duration::from_secs(10),
                result,
                true,
                true,
            )
        });
        println!("{label}: original {original:?}, current {current:?}");
        crate::paired_bench::compare_prepared(
            label,
            4_000,
            || (cache(&key, state, now), fetched(true, true)),
            |(mut prepared, result), current| {
                black_box(if current {
                    prepared.complete_fetch(
                        black_box(&key),
                        8,
                        now,
                        now,
                        Duration::from_secs(10),
                        result,
                        true,
                        true,
                    )
                } else {
                    original_complete(
                        &mut prepared,
                        black_box(&key),
                        8,
                        now,
                        now,
                        Duration::from_secs(10),
                        result,
                        true,
                        true,
                    )
                });
                black_box(&prepared);
            },
        );
    }
}

fn original_complete<T>(
    state: &mut PlayerLeaderboardCacheState,
    key: &PlayerLeaderboardCacheKey,
    requested_max_entries: usize,
    request_started_at: Instant,
    refresh_finished_at: Instant,
    error_retry_interval: std::time::Duration,
    fetched: Result<PlayerLeaderboardFetchSuccess<T>, String>,
    should_auto_populate: bool,
    auto_profile_id_exists: bool,
) -> PlayerLeaderboardFetchCompletion<T> {
    state.in_flight.remove(key);
    let request_invalidated = player_leaderboard_request_was_invalidated(
        state.invalidated_after.get(key).copied(),
        request_started_at,
    );

    let mut fetched_itl_self = None;
    let mut fetched_srpg_self_score = None;
    let mut fetched_imported_score = None;
    if !request_invalidated {
        match fetched {
            Ok(fetched) => {
                if !should_keep_newer_player_leaderboard_entry(
                    state.by_key.get(key),
                    request_started_at,
                ) {
                    let PlayerLeaderboardFetchSuccess {
                        data,
                        imported_score,
                        itl_self_found,
                    } = fetched;
                    if itl_self_found {
                        fetched_itl_self = Some((data.itl_self_score, data.itl_self_rank));
                    }
                    fetched_srpg_self_score = data.srpg_self_score;
                    if should_auto_populate && auto_profile_id_exists {
                        fetched_imported_score = imported_score;
                    }
                    state.by_key.insert(
                        key.clone(),
                        PlayerLeaderboardCacheEntry {
                            value: PlayerLeaderboardCacheValue::Ready(Arc::new(data)),
                            max_entries: requested_max_entries,
                            refreshed_at: refresh_finished_at,
                            retry_after: None,
                        },
                    );
                    state.invalidated_after.remove(key);
                }
            }
            Err(error) => {
                if !should_keep_newer_player_leaderboard_entry(
                    state.by_key.get(key),
                    request_started_at,
                ) {
                    let retry_after = Some(refresh_finished_at + error_retry_interval);
                    if let Some(entry) = state.by_key.get_mut(key)
                        && matches!(entry.value, PlayerLeaderboardCacheValue::Ready(_))
                    {
                        entry.refreshed_at = refresh_finished_at;
                        entry.retry_after = retry_after;
                    } else {
                        state.by_key.insert(
                            key.clone(),
                            PlayerLeaderboardCacheEntry {
                                value: PlayerLeaderboardCacheValue::Error(error.into()),
                                max_entries: requested_max_entries,
                                refreshed_at: refresh_finished_at,
                                retry_after,
                            },
                        );
                    }
                    state.invalidated_after.remove(key);
                }
            }
        }
    }

    let queued_fetch = state
        .pending_refresh
        .remove_entry(key)
        .map(|(key, max_entries)| {
            state.in_flight.insert(key.clone(), max_entries);
            QueuedPlayerLeaderboardFetch { key, max_entries }
        });

    PlayerLeaderboardFetchCompletion {
        fetched_itl_self,
        fetched_srpg_self_score,
        fetched_imported_score,
        queued_fetch,
    }
}
