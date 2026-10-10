use super::*;
use crate::buffers_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn states() -> (
    buffers_original::SrpgShopOverlayStateData,
    Box<SrpgShopOverlayStateData>,
) {
    let SrpgShopOverlayState::Visible(current) = show_srpg_shop_overlay(PlayerSide::P1) else {
        unreachable!()
    };
    let original = buffers_original::SrpgShopOverlayStateData {
        side: current.side,
        shop_index: 0,
        item_indices: [0; 4],
        queued: HashSet::new(),
        confirm: None,
        local_message: None,
        presentation_revision: 0,
        presentation: RefCell::new(None),
    };
    (original, current)
}

fn shop_fixture(index: usize, count: usize, mixed: bool) -> SrpgShop {
    SrpgShop {
        id: SRPG_SHOP_IDS[index],
        balance: 2_000,
        items: (0..count)
            .map(|i| SrpgShopItem {
                // The same IDs occur in each shop; colons and Unicode must stay opaque.
                item_id: format!("Å猫:item:{i}"),
                kind: SrpgShopItemKind::Song,
                name: format!("Song {i}"),
                description: "Description\nLine two".into(),
                effect: "Unlock a song".into(),
                cost: Some(1_234),
                difficulty: Some(14),
                bpm: Some(180),
                type_id: 1,
                owned: !mixed || i % 5 != 0,
                site_downloaded: i % 2 == 0,
                downloaded: mixed && i % 7 == 0,
                download_url: (!mixed || i % 11 != 0)
                    .then(|| format!("https://example.test/{i}.zip")),
            })
            .collect(),
    }
}

fn insert_both(
    old: &mut buffers_original::SrpgShopOverlayStateData,
    new: &mut SrpgShopOverlayStateData,
    index: usize,
    id: &str,
) {
    old.queued.insert(format!("{}:{id}", SRPG_SHOP_IDS[index]));
    new.queued[index].insert(id.into());
}

#[test]
fn shop_queues_preserve_cross_shop_identity_duplicate_downloads_and_messages() {
    for mixed in [false, true] {
        let (mut old, mut new) = states();
        for index in [0, 1, 2, 3, 0, 3, 2, 1] {
            old.shop_index = index;
            new.shop_index = index;
            let mut shop = shop_fixture(index, 96, mixed);
            shop.items.push(shop.items[1].clone());
            assert_eq!(
                ready_count(&new, &shop),
                buffers_original::ready_count(&old, &shop)
            );
            assert_eq!(
                download_all(&mut new, &shop),
                buffers_original::download_all(&mut old, &shop)
            );
            assert_eq!(new.local_message, old.local_message);
            assert_eq!(ready_count(&new, &shop), 0);
            let flattened: HashSet<_> = new
                .queued
                .iter()
                .enumerate()
                .flat_map(|(i, ids)| {
                    ids.iter()
                        .map(move |id| format!("{}:{id}", SRPG_SHOP_IDS[i]))
                })
                .collect();
            assert_eq!(flattened, old.queued);
        }
    }
}

#[test]
fn shop_catalog_keeps_every_actor_and_queue_status() {
    for index in 0..SHOPS.len() {
        for count in [0, 1, 12, 96] {
            let (mut old, mut new) = states();
            old.shop_index = index;
            new.shop_index = index;
            let shop = shop_fixture(index, count, true);
            for item in shop.items.iter().step_by(2) {
                insert_both(&mut old, &mut new, index, &item.item_id);
            }
            let snapshot = SrpgShopSnapshot {
                phase: SrpgShopPhase::Ready,
                shops: vec![shop],
                message: None,
            };
            for selected in [0, 1, count, usize::MAX] {
                old.item_indices[index] = selected;
                new.item_indices[index] = selected;
                for local in [None, Some("Queued Å猫".to_owned())] {
                    old.local_message = local.clone();
                    new.local_message = local;
                    let mut before = Vec::new();
                    let mut after = Vec::new();
                    buffers_original::push_catalog(
                        &mut before,
                        &old,
                        &snapshot,
                        SHOPS[index],
                        427.,
                        240.,
                        "wendy",
                    );
                    push_catalog(
                        &mut after,
                        &new,
                        &snapshot,
                        SHOPS[index],
                        427.,
                        240.,
                        "wendy",
                    );
                    assert_eq!(format!("{before:?}"), format!("{after:?}"));
                }
            }
        }
    }
}

#[test]
fn shop_queue_scans_allocate_nothing_and_record_fixed_state_cost() {
    let (mut old, mut new) = states();
    let shop = shop_fixture(0, 128, false);
    for item in shop.items.iter().step_by(2) {
        insert_both(&mut old, &mut new, 0, &item.item_id);
    }
    let (before_value, before) = measure(|| buffers_original::ready_count(&old, &shop));
    let (after_value, after) = measure(|| ready_count(&new, &shop));
    assert_eq!(before_value, after_value);
    assert_eq!(after.allocs + after.reallocs + after.frees, 0);
    assert_eq!(before.allocs, shop.items.len());
    crate::perf::assert_no_churn(|| {
        black_box(ready_count(&new, &shop));
    });
    // Four independent hash sets remove composite key formatting. Their handles
    // add a bounded fixed cost to one overlay, with no empty-set heap allocation.
    assert_eq!(
        std::mem::size_of::<SrpgShopOverlayStateData>(),
        std::mem::size_of::<buffers_original::SrpgShopOverlayStateData>()
            + 3 * std::mem::size_of::<HashSet<String>>()
    );
}

#[test]
fn queue_insertion_reduces_requested_bytes_with_immutable_item_keys() {
    let (mut old, mut new) = states();
    let shop = shop_fixture(0, 128, false);
    let (before_value, before) = measure(|| buffers_original::download_all(&mut old, &shop));
    let (after_value, after) = measure(|| download_all(&mut new, &shop));
    assert_eq!(before_value, after_value);
    assert!(after.allocated_bytes < before.allocated_bytes);
    assert!(after.allocs + after.reallocs < before.allocs + before.reallocs);
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_transient_buffers_shop_queues() {
    println!(
        "shop overlay state bytes: {} -> {}",
        std::mem::size_of::<buffers_original::SrpgShopOverlayStateData>(),
        std::mem::size_of::<SrpgShopOverlayStateData>()
    );
    for (count, mixed, queued) in [
        (1, false, false),
        (64, false, false),
        (1024, false, true),
        (1024, true, true),
    ] {
        let (mut old, mut new) = states();
        let shop = shop_fixture(0, count, mixed);
        if queued {
            for item in shop.items.iter().step_by(2) {
                insert_both(&mut old, &mut new, 0, &item.item_id);
            }
        }
        compare(
            &format!("shop/{count}-{mixed}-{queued}"),
            || {
                black_box(buffers_original::ready_count(
                    black_box(&old),
                    black_box(&shop),
                ));
            },
            || {
                black_box(ready_count(black_box(&new), black_box(&shop)));
            },
        );
    }
    let (mut old, mut new) = states();
    let shop = shop_fixture(0, 1024, true);
    for item in shop.items.iter().step_by(2) {
        insert_both(&mut old, &mut new, 0, &item.item_id);
    }
    let snapshot = SrpgShopSnapshot {
        phase: SrpgShopPhase::Ready,
        shops: vec![shop],
        message: None,
    };
    let mut before = Vec::with_capacity(64);
    let mut after = Vec::with_capacity(64);
    compare(
        "catalog/1024",
        || {
            before.clear();
            buffers_original::push_catalog(
                &mut before,
                black_box(&old),
                black_box(&snapshot),
                SHOPS[0],
                427.,
                240.,
                "wendy",
            );
            black_box(&before);
        },
        || {
            after.clear();
            push_catalog(
                &mut after,
                black_box(&new),
                black_box(&snapshot),
                SHOPS[0],
                427.,
                240.,
                "wendy",
            );
            black_box(&after);
        },
    );
    let (mut old, mut new) = states();
    let shop = shop_fixture(0, 128, false);
    compare(
        "enqueue/128",
        || {
            old.queued.clear();
            black_box(buffers_original::download_all(
                black_box(&mut old),
                black_box(&shop),
            ));
        },
        || {
            new.queued[0].clear();
            black_box(download_all(black_box(&mut new), black_box(&shop)));
        },
    );
}
