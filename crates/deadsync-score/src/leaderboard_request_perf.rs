use super::*;
use crate::perf;
use std::hint::black_box;
use std::time::Duration;

mod original {
    include!("leaderboard_request_original.rs");
}
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn profile() -> GameplayScoreboxProfileSnapshot {
    scorebox_snapshot(
        true,
        true,
        true,
        true,
        true,
        true,
        "gs-request-key",
        "ac-request-key",
        "Player",
        Some("request-profile".into()),
    )
}

fn entry(now: Instant, error: bool, cooldown: i8) -> PlayerLeaderboardCacheEntry {
    PlayerLeaderboardCacheEntry {
        value: if error {
            PlayerLeaderboardCacheValue::Error(Arc::from("offline"))
        } else {
            PlayerLeaderboardCacheValue::Ready(Arc::new(PlayerLeaderboardData {
                panes: Vec::new(),
                srpg_self_score: Some(9876),
                itl_self_score: Some(9912),
                itl_self_rank: Some(7),
            }))
        },
        max_entries: 5,
        refreshed_at: now - Duration::from_secs(20),
        retry_after: match cooldown {
            -1 => Some(now - Duration::from_secs(1)),
            1 => Some(now + Duration::from_secs(1)),
            _ => None,
        },
    }
}

fn clear(key: &PlayerLeaderboardCacheKey) {
    let mut cache = runtime_lock_player_leaderboard_cache();
    cache.by_key.remove(key);
    cache.in_flight.remove(key);
    cache.pending_refresh.remove(key);
    cache.invalidated_after.remove(key);
}

fn result_text(plan: Option<PlayerLeaderboardRequestPlan>) -> String {
    plan.map_or_else(
        || "none".into(),
        |plan| {
            let fetch = plan.fetch.map(|f| {
                format!(
                    "{:?} {:?} {:?} {:?} {} {}",
                    f.key,
                    f.gs_username,
                    f.persistent_profile_id,
                    f.auto_profile_id,
                    f.should_auto_populate,
                    f.max_entries
                )
            });
            format!(
                "{} {:?} {:?} {fetch:?}",
                plan.snapshot.loading, plan.snapshot.data, plan.snapshot.error
            )
        },
    )
}

#[test]
fn cached_request_plans_preserve_fetch_payloads_cooldowns_and_queues() {
    let profile = profile();
    let hash = " request-plan-regression ";
    let key = player_leaderboard_cache_key(hash, &profile).unwrap();
    let now = Instant::now();
    for kind in 0..3 {
        for cooldown in [-1, 0, 1] {
            for in_flight in [None, Some(3), Some(10)] {
                for refresh in [false, true] {
                    for max_entries in [0, 3, 5, 10] {
                        let mut observed = Vec::new();
                        for current in [false, true] {
                            clear(&key);
                            {
                                let mut cache = runtime_lock_player_leaderboard_cache();
                                if kind != 0 {
                                    cache
                                        .by_key
                                        .insert(key.clone(), entry(now, kind == 2, cooldown));
                                }
                                if let Some(rows) = in_flight {
                                    cache.in_flight.insert(key.clone(), rows);
                                }
                                cache
                                    .invalidated_after
                                    .insert(key.clone(), now - Duration::from_secs(30));
                            }
                            let plan = if current {
                                runtime_plan_player_leaderboard_request
                            } else {
                                original::runtime_plan_player_leaderboard_request
                            };
                            let result =
                                result_text(plan(hash, &profile, max_entries, refresh, now));
                            let cache = runtime_lock_player_leaderboard_cache();
                            observed.push((
                                result,
                                format!(
                                    "{:?} {:?} {:?} {:?}",
                                    cache.by_key.get(&key),
                                    cache.in_flight.get(&key),
                                    cache.pending_refresh.get(&key),
                                    cache.invalidated_after.get(&key)
                                ),
                            ));
                        }
                        assert_eq!(
                            observed[0], observed[1],
                            "kind={kind} cooldown={cooldown} in_flight={in_flight:?} refresh={refresh} rows={max_entries}"
                        );
                    }
                }
            }
        }
    }
    clear(&key);
    for hash in ["", " \t\u{2003}", "valid"] {
        let inactive =
            scorebox_snapshot(true, true, false, true, true, true, "gs", "ac", "P", None);
        assert_eq!(
            result_text(original::runtime_plan_player_leaderboard_request(
                hash, &inactive, 5, false, now
            )),
            result_text(runtime_plan_player_leaderboard_request(
                hash, &inactive, 5, false, now
            ))
        );
    }
}

#[test]
fn cached_ready_and_cooling_error_requests_do_not_allocate() {
    let profile = profile();
    let key = player_leaderboard_cache_key("request-allocation-regression", &profile).unwrap();
    let now = Instant::now();
    for error in [false, true] {
        clear(&key);
        runtime_lock_player_leaderboard_cache()
            .by_key
            .insert(key.clone(), entry(now, error, 1));
        let (before, old) = perf::measure(|| {
            original::runtime_plan_player_leaderboard_request(
                &key.chart_hash,
                &profile,
                10,
                true,
                now,
            )
        });
        drop(before);
        let (after, new) = perf::measure(|| {
            runtime_plan_player_leaderboard_request(&key.chart_hash, &profile, 10, true, now)
        });
        assert_eq!(new.allocs + new.reallocs, 0);
        assert_eq!(old.allocs, 6);
        assert!(after.unwrap().fetch.is_none());
    }
    clear(&key);
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_leaderboard_request_plans() {
    let profile = profile();
    let key = player_leaderboard_cache_key("request-benchmark", &profile).unwrap();
    let now = Instant::now();
    for (label, kind, refresh) in [
        ("ready-hit", 1, false),
        ("error-cooldown", 2, true),
        ("in-flight", 0, false),
        ("refresh-control", 1, true),
    ] {
        clear(&key);
        {
            let mut cache = runtime_lock_player_leaderboard_cache();
            if kind != 0 {
                cache.by_key.insert(
                    key.clone(),
                    entry(now, kind == 2, if kind == 2 { 1 } else { 0 }),
                );
            }
            // Reserve the table in every case, excluding table growth from the control.
            cache.in_flight.insert(key.clone(), 5);
            if kind != 0 {
                cache.in_flight.remove(&key);
            }
        }
        let mut work = |current| {
            let plan = if current {
                runtime_plan_player_leaderboard_request
            } else {
                original::runtime_plan_player_leaderboard_request
            };
            let result = plan(
                black_box(&key.chart_hash),
                black_box(&profile),
                5,
                refresh,
                now,
            );
            if refresh && kind == 1 {
                runtime_lock_player_leaderboard_cache()
                    .in_flight
                    .remove(&key);
            }
            let _ = black_box(result);
        };
        for current in [false, true] {
            let (_, churn) = perf::measure(|| work(current));
            println!("leaderboard {label} current={current}: {churn:?}");
        }
        paired::compare(&format!("leaderboard {label}"), 50_000, &mut work);
        clear(&key);
    }
}
