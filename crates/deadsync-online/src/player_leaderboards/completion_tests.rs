use super::*;
use crate::{paired_bench, perf};
use deadsync_score::{
    PlayerLeaderboardCacheKey, PlayerLeaderboardFetchCompletion, PlayerLeaderboardFetchJobResult,
    QueuedPlayerLeaderboardFetch,
};
use std::cell::RefCell;
use std::hint::black_box;

mod original {
    use super::*;
    include!("completion_original.rs");
    pub(super) fn run(
        handlers: PlayerLeaderboardFetchHandlers,
        result: PlayerLeaderboardFetchJobResult<ImportedPlayerScore>,
    ) -> Option<PlayerLeaderboardFetchRequest> {
        complete(handlers, result)
    }
}

#[derive(Debug, PartialEq)]
enum Event {
    Itl(Option<String>, String, String, Option<u32>, Option<u32>),
    Srpg(Option<String>, String, String, u32),
    Import(String, String, String, ImportedPlayerScore),
}
thread_local! { static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) }; }
fn itl(p: Option<&str>, a: &str, h: &str, s: Option<u32>, r: Option<u32>) {
    EVENTS.with_borrow_mut(|e| e.push(Event::Itl(p.map(str::to_owned), a.into(), h.into(), s, r)));
}
fn srpg(p: Option<&str>, a: &str, h: &str, s: u32) {
    EVENTS.with_borrow_mut(|e| e.push(Event::Srpg(p.map(str::to_owned), a.into(), h.into(), s)));
}
fn import(p: &str, u: &str, h: &str, s: ImportedPlayerScore) {
    EVENTS.with_borrow_mut(|e| e.push(Event::Import(p.into(), u.into(), h.into(), s)));
}
fn handlers() -> PlayerLeaderboardFetchHandlers {
    PlayerLeaderboardFetchHandlers {
        cache_itl_self: itl,
        cache_srpg_self_score: srpg,
        cache_imported_score: import,
    }
}
fn old_handlers() -> original::PlayerLeaderboardFetchHandlers {
    original::PlayerLeaderboardFetchHandlers {
        cache_itl_self: |p, a, h, s, r| itl(p.as_deref(), &a, &h, s, r),
        cache_srpg_self_score: |p, a, h, s| srpg(p.as_deref(), &a, &h, s),
        cache_imported_score: |p, u, h, s| import(&p, &u, &h, s),
    }
}
fn sinks() -> PlayerLeaderboardFetchHandlers {
    PlayerLeaderboardFetchHandlers {
        cache_itl_self: |p, a, h, s, r| {
            black_box((p, a, h, s, r));
        },
        cache_srpg_self_score: |p, a, h, s| {
            black_box((p, a, h, s));
        },
        cache_imported_score: |p, u, h, s| {
            black_box((p, u, h, s));
        },
    }
}
fn old_sinks() -> original::PlayerLeaderboardFetchHandlers {
    original::PlayerLeaderboardFetchHandlers {
        cache_itl_self: |p, a, h, s, r| {
            black_box((p.as_deref(), a.as_str(), h.as_str(), s, r));
        },
        cache_srpg_self_score: |p, a, h, s| {
            black_box((p.as_deref(), a.as_str(), h.as_str(), s));
        },
        cache_imported_score: |p, u, h, s| {
            black_box((p.as_str(), u.as_str(), h.as_str(), s));
        },
    }
}
fn key(hash: &str) -> PlayerLeaderboardCacheKey {
    PlayerLeaderboardCacheKey {
        chart_hash: hash.into(),
        api_key: "test-api-key-0123456789".into(),
        arrowcloud_api_key: "arrowcloud-key".into(),
        include_arrowcloud: true,
        show_ex_score: false,
    }
}
fn result(mask: usize) -> PlayerLeaderboardFetchJobResult<ImportedPlayerScore> {
    PlayerLeaderboardFetchJobResult {
        key: key("current-chart-hash"),
        gs_username: "Player Name".into(),
        persistent_profile_id: (mask & 1 != 0).then(|| "persistent-profile".into()),
        auto_profile_id: (mask & 2 != 0).then(|| "auto-profile".into()),
        should_auto_populate: mask & 4 != 0,
        completion: PlayerLeaderboardFetchCompletion {
            fetched_itl_self: (mask & 8 != 0).then_some((
                (mask & 16 != 0).then_some(12345),
                (mask & 32 != 0).then_some(42),
            )),
            fetched_srpg_self_score: (mask & 64 != 0).then_some(9876),
            fetched_imported_score: (mask & 128 != 0).then(|| ImportedPlayerScore {
                score_10000: 9987.0,
                comments: Some("[DS], test score".into()),
                is_fail: false,
                ex_evidence: Default::default(),
            }),
            queued_fetch: (mask & 256 != 0).then(|| QueuedPlayerLeaderboardFetch {
                key: key("queued-chart-hash"),
                max_entries: 23,
            }),
        },
    }
}
fn request_summary(
    request: Option<PlayerLeaderboardFetchRequest>,
) -> Option<(
    PlayerLeaderboardCacheKey,
    String,
    Option<String>,
    Option<String>,
    bool,
    usize,
)> {
    request.map(|r| {
        (
            r.key,
            r.gs_username,
            r.persistent_profile_id,
            r.auto_profile_id,
            r.should_auto_populate,
            r.max_entries,
        )
    })
}

#[test]
fn completion_preserves_callback_order_payloads_and_queued_request() {
    for mask in 0..512 {
        let old = original::run(old_handlers(), result(mask));
        let old_events = EVENTS.with_borrow_mut(std::mem::take);
        let new = complete(handlers(), result(mask));
        let new_events = EVENTS.with_borrow_mut(std::mem::take);
        assert_eq!(old_events, new_events, "mask {mask}");
        assert_eq!(request_summary(old), request_summary(new), "mask {mask}");
    }
}

#[test]
fn completion_does_not_allocate_callback_strings() {
    let old_input = result(511);
    let new_input = result(511);
    let (_, old) = perf::measure(|| original::run(old_sinks(), old_input));
    let (_, new) = perf::measure(|| complete(sinks(), new_input));
    assert_eq!(old.allocs, 9);
    assert_eq!(new.allocs, 0);
    assert_eq!(new.reallocs, 0);
}

#[test]
#[ignore = "paired performance benchmark"]
fn benchmark_completion_borrows() {
    for (label, mask) in [
        ("completion-all-queued", 511),
        ("completion-all", 255),
        ("completion-itl", 57),
        ("completion-empty", 0),
    ] {
        let work = |input, current| {
            if current {
                black_box(complete(black_box(sinks()), input));
            } else {
                black_box(original::run(black_box(old_sinks()), input));
            }
        };
        let input = result(mask);
        let (_, old) = perf::measure(|| work(input, false));
        let input = result(mask);
        let (_, new) = perf::measure(|| work(input, true));
        println!("{label} allocations: original {old:?}, current {new:?}");
        paired_bench::compare_prepared(label, 10_000, || result(mask), work);
    }
}
