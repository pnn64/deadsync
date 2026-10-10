use super::*;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "srpg_shop_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn catalog(count: usize, html: bool) -> String {
    let rows: Vec<_> = (0..count)
        .map(|index| {
            serde_json::json!([
                index.to_string(),
                "icon.png",
                format!("Song {index:05}"),
                if html {
                    "<b>Purchase</b> &amp; unlock"
                } else {
                    "Purchase to unlock this song"
                },
                if html {
                    "Difficulty: 14|Speed Tier: 180 BPM"
                } else {
                    "Difficulty: 14"
                },
                2,
                0,
                index % 1234,
                index % 3,
                0,
                0,
                1,
                14,
                180,
                0
            ])
        })
        .collect();
    serde_json::json!({"data": rows}).to_string()
}

#[test]
fn owned_catalog_preserves_alias_precedence_sorting_values_and_censorship() {
    let rows = serde_json::json!([
        [
            "id",
            0,
            " Name  ",
            "<b>Desc</b> &amp;",
            "A|B",
            0,
            0,
            "1,000",
            0,
            0,
            0,
            "1",
            14,
            180
        ],
        [
            true, 0, false, 17, null, 0, 0, "+1,000", 0, 0, 0, "1", 14, 180
        ],
        [42, 0, "同じ", "two  spaces", "no pipe", 0, 0, 9, 0, 0, 0, 0],
        null,
        {},
        [],
        ["short"],
        ["id", 0, "same key", false, 3],
        [
            "bad",
            0,
            "title",
            null,
            [],
            0,
            0,
            -1,
            0,
            0,
            0,
            256,
            4294967296u64,
            "+180"
        ]
    ]);
    for input in [
        rows.clone(),
        serde_json::json!({"data":rows}),
        serde_json::json!({"DaTa":rows}),
        serde_json::json!({"data":null,"aaData":rows}),
        serde_json::json!({"DATA":null,"data":rows,"items":[["fallback"]]}),
        serde_json::json!({"rows":rows,"items":[["ignored"]]}),
        serde_json::json!({}),
        serde_json::json!(17),
    ] {
        let text = input.to_string();
        for shop in [0, 2, 3] {
            for balance in [0, 999, 1000, u64::MAX] {
                assert_eq!(
                    parse_catalog(&text, shop, balance),
                    original::parse_catalog(&text, shop, balance)
                );
            }
        }
    }
    for text in ["", "{", "[]", "{\"data\":[]}"] {
        assert_eq!(
            parse_catalog(text, 0, 0),
            original::parse_catalog(text, 0, 0)
        );
    }
}

fn runtime(items: Vec<SrpgShopItem>) -> RuntimeState {
    RuntimeState {
        generation: u64::MAX,
        snapshot: Arc::new(SrpgShopSnapshot {
            phase: SrpgShopPhase::Ready,
            shops: vec![SrpgShop {
                id: 3,
                balance: 100,
                items,
            }],
            message: Some("Ready".into()),
        }),
        session: Some(ShopSession {
            agent: network::get_agent(),
            entrant_id: "test".into(),
        }),
    }
}

#[test]
fn shared_purchase_snapshot_preserves_state_and_copy_on_write_rollback() {
    let items = parse_catalog(&catalog(20, false), 0, 0).unwrap();
    let mut before = runtime(items.clone());
    let mut after = runtime(items);
    let displayed = Arc::clone(&after.snapshot);
    let expected = original::begin_purchase(&mut before).unwrap();
    let (generation, session, mut previous) = begin_purchase(&mut after).unwrap();
    assert_eq!(generation, expected.0);
    assert_eq!(session.entrant_id, expected.1.entrant_id);
    assert_eq!(*previous, expected.2);
    assert_eq!(before.snapshot, after.snapshot);
    assert_eq!(displayed.phase, SrpgShopPhase::Ready);
    assert!(Arc::ptr_eq(&previous, &after.snapshot));
    let purchasing = Arc::clone(&after.snapshot);
    let snapshot = Arc::make_mut(&mut previous);
    snapshot.phase = SrpgShopPhase::Ready;
    snapshot.message = Some("Purchase failed: fixture".into());
    set_runtime_snapshot(&mut after, previous);
    assert_eq!(purchasing.phase, SrpgShopPhase::Purchasing);
    assert_eq!(after.snapshot.phase, SrpgShopPhase::Ready);
    assert_eq!(after.snapshot.shops, displayed.shops);
    assert_eq!(
        after.snapshot.message.as_deref(),
        Some("Purchase failed: fixture")
    );
    for phase in [
        SrpgShopPhase::Idle,
        SrpgShopPhase::Loading,
        SrpgShopPhase::Purchasing,
        SrpgShopPhase::Error,
    ] {
        Arc::make_mut(&mut after.snapshot).phase = phase;
        let old = Arc::clone(&after.snapshot);
        assert!(begin_purchase(&mut after).is_none());
        assert!(Arc::ptr_eq(&old, &after.snapshot));
        assert_eq!(after.generation, 0);
    }
    after.session = None;
    Arc::make_mut(&mut after.snapshot).phase = SrpgShopPhase::Ready;
    assert!(begin_purchase(&mut after).is_none());
}

#[test]
fn catalog_and_purchase_remove_string_allocation_churn() {
    let body = catalog(100, false);
    let (before, old) = allocations::measure(|| original::parse_catalog(&body, 0, 0).unwrap());
    let (after, new) = allocations::measure(|| parse_catalog(&body, 0, 0).unwrap());
    assert_eq!(before, after);
    assert!(new.allocs + 300 <= old.allocs, "{old:?} -> {new:?}");
    let mut a = runtime(before);
    let mut b = runtime(after);
    let original_address = Arc::as_ptr(&b.snapshot);
    let (_, old) = allocations::measure(|| original::begin_purchase(&mut a).unwrap());
    let (_, new) = allocations::measure(|| begin_purchase(&mut b).unwrap());
    assert_eq!(Arc::as_ptr(&b.snapshot), original_address);
    assert!(new.allocs * 19 < old.allocs * 10, "{old:?} -> {new:?}");
}

#[test]
#[ignore = "paired performance benchmark"]
fn benchmark_owned_catalog() {
    for html in [false, true] {
        let body = catalog(1000, html);
        let (_, old) = allocations::measure(|| original::parse_catalog(&body, 0, 0).unwrap());
        let (_, new) = allocations::measure(|| parse_catalog(&body, 0, 0).unwrap());
        println!("Catalog rows=1000 html={html}: original {old:?}; current {new:?}");
        paired::compare(&format!("Catalog rows=1000 html={html}"), 32, |current| {
            drop(black_box(if current {
                parse_catalog(black_box(&body), 0, 0)
            } else {
                original::parse_catalog(black_box(&body), 0, 0)
            }));
        });
    }
}

#[test]
#[ignore = "paired performance benchmark"]
fn benchmark_shared_purchase() {
    for count in [32, 1000] {
        for shared in [false, true] {
            let items = parse_catalog(&catalog(count, false), 0, 0).unwrap();
            let prepare = || {
                let state = runtime(items.clone());
                let displayed = shared.then(|| Arc::clone(&state.snapshot));
                (state, displayed)
            };
            let (mut a, _old_displayed) = prepare();
            let (mut b, _new_displayed) = prepare();
            let (_, old) = allocations::measure(|| original::begin_purchase(&mut a).unwrap());
            let (_, new) = allocations::measure(|| begin_purchase(&mut b).unwrap());
            println!("Purchase rows={count} shared={shared}: original {old:?}; current {new:?}");
            paired::compare_prepared(
                &format!("Purchase rows={count} shared={shared}"),
                32,
                prepare,
                |(mut state, displayed), current| {
                    if current {
                        drop(black_box(begin_purchase(black_box(&mut state))))
                    } else {
                        drop(black_box(original::begin_purchase(black_box(&mut state))))
                    }
                    drop(displayed);
                },
            );
        }
    }
}
