use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/srpg_shop/baseline.rs"
    ));
}

fn item(id: impl Into<String>, tag: usize) -> SrpgShopItem {
    let id = id.into();
    SrpgShopItem {
        download_url: Some(format!("https://example.invalid/{id}.zip")),
        item_id: id,
        kind: if tag.is_multiple_of(2) {
            SrpgShopItemKind::Song
        } else {
            SrpgShopItemKind::Relic
        },
        name: format!("Song 雪 {tag}"),
        description: format!("Description {tag}"),
        effect: format!("Effect {tag}"),
        cost: Some(tag as u64),
        difficulty: Some(tag as u32),
        bpm: Some(180),
        type_id: 3,
        owned: tag.is_multiple_of(2),
        site_downloaded: tag.is_multiple_of(3),
        downloaded: tag.is_multiple_of(5),
    }
}

fn snapshot(items: Vec<SrpgShopItem>) -> SrpgShopSnapshot {
    SrpgShopSnapshot {
        phase: SrpgShopPhase::Ready,
        shops: vec![SrpgShop {
            id: 0,
            balance: 17,
            items,
        }],
        message: Some("fixture 雪".into()),
    }
}

fn order_fixture(count: usize, mode: &str) -> (SrpgShopSnapshot, SrpgShopSnapshot) {
    let unique = if mode == "changed" {
        (count / 4).max(1)
    } else {
        count.max(1)
    };
    let previous = snapshot(
        (0..count)
            .map(|i| item((i % unique).to_string(), i))
            .collect(),
    );
    let mut current = previous.clone();
    for i in &mut current.shops[0].items {
        i.cost = None;
        i.description.push_str(" refreshed");
    }
    match mode {
        "reverse" => current.shops[0].items.reverse(),
        "rotate" => current.shops[0].items.rotate_left(count / 3),
        "changed" => {
            current.shops[0].items.reverse();
            for (i, item) in current.shops[0].items.iter_mut().enumerate() {
                if i % 7 == 0 {
                    item.item_id = format!("new-{i}");
                }
            }
        }
        _ => {}
    }
    (current, previous)
}

#[test]
fn restored_orders_match_legacy_with_duplicates_missing_ids_and_shops() {
    for count in [0, 1, 2, 8, 33, 128] {
        for mode in ["same", "reverse", "rotate", "changed"] {
            let (current, previous) = order_fixture(count, mode);
            let mut old = current.clone();
            let mut new = current;
            baseline::preserve_snapshot_order(&mut old, &previous);
            preserve_snapshot_order(&mut new, &previous);
            assert_eq!(old, new, "{count} {mode}");
        }
    }
    let mut seed = 19u64;
    for turn in 0..512 {
        let mut next_id = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            match (seed >> 32) % 19 {
                0 => String::new(),
                1 => "雪".into(),
                n => n.to_string(),
            }
        };
        let mut previous = snapshot((0..turn % 97).map(|i| item(next_id(), i)).collect());
        previous.shops.push(SrpgShop {
            id: 0,
            balance: 999,
            items: vec![item("ignored duplicate shop", 1)],
        });
        previous.shops.push(SrpgShop {
            id: 3,
            balance: 999,
            items: vec![item("雪", 1), item("", 2)],
        });
        let mut current = snapshot((0..turn % 83).map(|i| item(next_id(), i + 100)).collect());
        current.shops.extend([
            SrpgShop {
                id: 3,
                balance: 7,
                items: vec![item("", 2), item("雪", 1), item("雪", 3)],
            },
            SrpgShop {
                id: 99,
                balance: 8,
                items: vec![item("unmatched shop", 0)],
            },
        ]);
        let mut old = current.clone();
        let mut new = current;
        baseline::preserve_snapshot_order(&mut old, &previous);
        preserve_snapshot_order(&mut new, &previous);
        assert_eq!(old, new, "turn {turn}");
    }
}

fn download(id: String, tag: usize) -> ParsedDownload {
    ParsedDownload {
        item_id: id,
        name: format!("Download 雪 {tag}"),
        details: match tag % 5 {
            0 => "invalid 14 180 extra 9",
            1 => "missing",
            2 => "+15 bad 200",
            3 => "4294967296 16 220",
            _ => "0 0",
        }
        .into(),
        url: format!("https://example.invalid/{tag}.zip"),
        site_downloaded: tag.is_multiple_of(2),
    }
}

fn copy_downloads(downloads: &[ParsedDownload]) -> Vec<ParsedDownload> {
    downloads
        .iter()
        .map(|d| ParsedDownload {
            item_id: d.item_id.clone(),
            name: d.name.clone(),
            details: d.details.clone(),
            url: d.url.clone(),
            site_downloaded: d.site_downloaded,
        })
        .collect()
}

fn merge_fixture(
    items: usize,
    downloads: usize,
    mode: &str,
) -> (Vec<SrpgShopItem>, Vec<ParsedDownload>) {
    let unique = if mode == "duplicates" {
        (items / 4).max(1)
    } else {
        items.max(1)
    };
    let items: Vec<_> = (0..items)
        .map(|i| item((i % unique).to_string(), i))
        .collect();
    let downloads = (0..downloads)
        .map(|i| {
            let id = match mode {
                "new" => format!("new-{i}"),
                "mixed" if i % 2 == 0 => format!("new-{}", i / 4),
                "duplicates" => (i % unique).to_string(),
                _ => (unique - 1 - i % unique).to_string(),
            };
            download(id, i)
        })
        .collect();
    (items, downloads)
}

#[test]
fn merged_downloads_preserve_first_match_updates_and_append_order() {
    for count in [0, 1, 8, 9, 16, 17, 31, 32, 33, 128] {
        for downloads in [0, 1, 8, 9, 16, 17, 32, 33, 129] {
            for mode in ["known", "new", "mixed", "duplicates"] {
                let (mut new, source) = merge_fixture(count, downloads, mode);
                let mut old = new.clone();
                baseline::merge_downloads(&mut old, copy_downloads(&source));
                merge_downloads(&mut new, source);
                assert_eq!(old, new, "{count}, {downloads}, {mode}");
            }
        }
    }
    let mut seed = 31u64;
    for turn in 0..256 {
        let mut id = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            match (seed >> 32) % 13 {
                0 => String::new(),
                1 => "雪".into(),
                n => n.to_string(),
            }
        };
        let mut new: Vec<_> = (0..turn % 71).map(|i| item(id(), i)).collect();
        let mut old = new.clone();
        let downloads: Vec<_> = (0..turn % 89).map(|i| download(id(), i)).collect();
        baseline::merge_downloads(&mut old, copy_downloads(&downloads));
        merge_downloads(&mut new, downloads);
        assert_eq!(old, new, "turn {turn}");
    }
}

#[test]
fn refresh_pipeline_preserves_merged_fields_and_visible_order() {
    for mode in ["same", "reverse", "rotate", "changed"] {
        let (mut new, previous) = order_fixture(256, mode);
        let mut old = new.clone();
        let (_, downloads) = merge_fixture(256, 512, "mixed");
        baseline::merge_downloads(&mut old.shops[0].items, copy_downloads(&downloads));
        baseline::preserve_snapshot_order(&mut old, &previous);
        merge_downloads(&mut new.shops[0].items, downloads);
        preserve_snapshot_order(&mut new, &previous);
        assert_eq!(old, new);
    }
}

#[test]
fn downloaded_flags_preserve_folder_policies_and_snapshot_isolation() {
    let original = SrpgShopSnapshot {
        phase: SrpgShopPhase::Purchasing,
        message: Some("keep message".into()),
        shops: [0, 2, 3, 4, 99, 0]
            .into_iter()
            .map(|id| SrpgShop {
                id,
                balance: id as u64,
                items: vec![item("a", 0), item("a", 1), item("b", 2), item("a", 3)],
            })
            .collect(),
    };
    for folder in [
        SrpgShopFolder::Unlocks,
        SrpgShopFolder::Shops,
        SrpgShopFolder::Faction,
    ] {
        for destination in [
            "wrong",
            download_folder(0, folder),
            download_folder(3, folder),
        ] {
            for url in [
                "https://example.invalid/a.zip",
                "https://example.invalid/b.zip",
                "missing",
                "",
            ] {
                for shared in [false, true] {
                    let mut old = Arc::new(original.clone());
                    let mut new = Arc::new(original.clone());
                    for _ in 0..2 {
                        let readers = shared.then(|| (Arc::clone(&old), Arc::clone(&new)));
                        let saved = (*old).clone();
                        let a = baseline::mark_downloaded(&mut old, url, destination, folder);
                        let b = mark_downloaded(&mut new, url, destination, folder);
                        assert_eq!(a, b);
                        assert_eq!(old, new);
                        if let Some((a, b)) = readers {
                            assert_eq!(a, b);
                            assert_eq!(*a, saved);
                        }
                    }
                }
            }
        }
    }
    let mut unique = Arc::new(original);
    let weak = Arc::downgrade(&unique);
    assert!(!mark_downloaded(
        &mut unique,
        "missing",
        "wrong",
        SrpgShopFolder::Shops
    ));
    assert!(weak.upgrade().is_some());
    assert!(mark_downloaded(
        &mut unique,
        "https://example.invalid/a.zip",
        download_folder(0, SrpgShopFolder::Shops),
        SrpgShopFolder::Shops
    ));
    assert!(weak.upgrade().is_none());
}

#[test]
fn no_op_updates_reuse_storage_and_reordering_has_bounded_scratch() {
    let (mut current, previous) = order_fixture(128, "same");
    let pointer = current.shops[0].items.as_ptr();
    perf::assert_no_churn(|| preserve_snapshot_order(&mut current, &previous));
    current.shops[0].items.reverse();
    let string_pointer = current.shops[0].items[127].name.as_ptr();
    perf::assert_churn_budget(2, 128 * 100 + 128, || {
        preserve_snapshot_order(&mut current, &previous)
    });
    assert_eq!(pointer, current.shops[0].items.as_ptr());
    assert_eq!(string_pointer, current.shops[0].items[0].name.as_ptr());
    perf::assert_no_churn(|| merge_downloads(&mut current.shops[0].items, Vec::new()));
    let mut current = Arc::new(current);
    let pointer = Arc::as_ptr(&current);
    perf::assert_no_churn(|| {
        assert!(mark_downloaded(
            &mut current,
            "https://example.invalid/1.zip",
            download_folder(0, SrpgShopFolder::Shops),
            SrpgShopFolder::Shops
        ));
    });
    assert_eq!(pointer, Arc::as_ptr(&current));
    let reader = Arc::clone(&current);
    perf::assert_no_churn(|| {
        assert!(!mark_downloaded(
            &mut current,
            "https://example.invalid/1.zip",
            download_folder(0, SrpgShopFolder::Shops),
            SrpgShopFolder::Shops
        ));
        assert!(!mark_downloaded(
            &mut current,
            "missing",
            download_folder(0, SrpgShopFolder::Shops),
            SrpgShopFolder::Shops
        ));
    });
    assert!(Arc::ptr_eq(&reader, &current));
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut before =
        || perf::measure_sampled(&format!("{name}_old"), iterations, units.max(1), &mut old);
    let mut after =
        || perf::measure_sampled(&format!("{name}_new"), iterations, units.max(1), &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        after();
        before();
    } else {
        before();
        after();
    }
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation accounting"]
fn srpg_shop_bench() {
    for count in [0, 1, 128, 2048] {
        for mode in ["same", "reverse", "rotate", "changed"] {
            let (current, previous) = order_fixture(count, mode);
            pair(
                &format!("order_{count}_{mode}"),
                if count > 128 { 8 } else { 256 },
                count,
                || {
                    let mut s = black_box(&current).clone();
                    baseline::preserve_snapshot_order(&mut s, black_box(&previous));
                    s
                },
                || {
                    let mut s = black_box(&current).clone();
                    preserve_snapshot_order(&mut s, black_box(&previous));
                    s
                },
            );
        }
    }
    for (name, count, downloads, mode) in [
        ("empty", 0, 0, "known"),
        ("tiny", 4, 4, "known"),
        ("eight", 32, 8, "known"),
        ("nine", 64, 9, "known"),
        ("sixteen", 64, 16, "known"),
        ("seventeen", 64, 17, "known"),
        ("few_in_large", 2048, 17, "known"),
        ("small_existing_new", 8, 1024, "new"),
        ("known", 1024, 1024, "known"),
        ("new", 0, 1024, "new"),
        ("mixed", 512, 1024, "mixed"),
        ("duplicates", 128, 1024, "duplicates"),
    ] {
        let (items, source) = merge_fixture(count, downloads, mode);
        pair(
            &format!("merge_{name}"),
            if downloads > 128 { 16 } else { 256 },
            downloads,
            || {
                let mut i = black_box(&items).clone();
                baseline::merge_downloads(&mut i, copy_downloads(black_box(&source)));
                i
            },
            || {
                let mut i = black_box(&items).clone();
                merge_downloads(&mut i, copy_downloads(black_box(&source)));
                i
            },
        );
    }
    for count in [0, 1, 1024] {
        for mode in [
            "unique",
            "shared",
            "missing",
            "shared_missing",
            "already",
            "wrong_folder",
        ] {
            let mut initial = snapshot((0..count).map(|i| item(i.to_string(), i)).collect());
            if let Some(first) = initial.shops[0].items.first_mut() {
                first.downloaded = mode == "already";
            }
            let mut old = Arc::new(initial.clone());
            let mut new = Arc::new(initial);
            let shared = mode.starts_with("shared");
            let changed = mode == "unique" || mode == "shared";
            let url = if mode.ends_with("missing") {
                "missing"
            } else {
                "https://example.invalid/0.zip"
            };
            let destination = if mode == "wrong_folder" {
                "wrong"
            } else {
                download_folder(0, SrpgShopFolder::Shops)
            };
            pair(
                &format!("mark_{count}_{mode}"),
                if count > 128 { 32 } else { 512 },
                count,
                || {
                    if changed && count > 0 {
                        Arc::get_mut(&mut old).unwrap().shops[0].items[0].downloaded = false;
                    }
                    let reader = shared.then(|| Arc::clone(&old));
                    let result = baseline::mark_downloaded(
                        black_box(&mut old),
                        url,
                        destination,
                        SrpgShopFolder::Shops,
                    );
                    black_box(&old);
                    drop(reader);
                    result
                },
                || {
                    if changed && count > 0 {
                        Arc::get_mut(&mut new).unwrap().shops[0].items[0].downloaded = false;
                    }
                    let reader = shared.then(|| Arc::clone(&new));
                    let result = mark_downloaded(
                        black_box(&mut new),
                        url,
                        destination,
                        SrpgShopFolder::Shops,
                    );
                    black_box(&new);
                    drop(reader);
                    result
                },
            );
        }
    }
}
