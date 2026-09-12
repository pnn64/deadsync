// Frozen from b35f21fb0 (0.5.1155); helper visibility adapted.
use super::*;

pub(super) fn preserve_snapshot_order(
    snapshot: &mut SrpgShopSnapshot,
    previous: &SrpgShopSnapshot,
) {
    for shop in &mut snapshot.shops {
        let Some(old_shop) = previous.shops.iter().find(|old| old.id == shop.id) else {
            continue;
        };
        let mut remaining = std::mem::take(&mut shop.items);
        let mut ordered = Vec::with_capacity(remaining.len());
        for old_item in &old_shop.items {
            if let Some(index) = remaining
                .iter()
                .position(|item| item.item_id == old_item.item_id)
            {
                ordered.push(remaining.remove(index));
            }
        }
        ordered.extend(remaining);
        shop.items = ordered;
    }
}

pub(super) fn merge_downloads(items: &mut Vec<SrpgShopItem>, downloads: Vec<ParsedDownload>) {
    for download in downloads {
        let (difficulty, bpm) = {
            let mut numbers = download
                .details
                .split_whitespace()
                .filter_map(|part| part.parse::<u32>().ok());
            (numbers.next(), numbers.next())
        };
        let effect = song_stats_text(difficulty, bpm);
        if let Some(item) = items
            .iter_mut()
            .find(|item| item.item_id == download.item_id)
        {
            item.owned = true;
            item.site_downloaded = download.site_downloaded;
            item.download_url = Some(download.url);
            item.difficulty = difficulty.or(item.difficulty);
            item.bpm = bpm.or(item.bpm);
            item.effect = effect;
            continue;
        }
        items.push(SrpgShopItem {
            item_id: download.item_id,
            kind: SrpgShopItemKind::Song,
            name: download.name,
            description: "Purchased song unlock".to_string(),
            effect,
            cost: None,
            difficulty,
            bpm,
            type_id: 1,
            owned: true,
            site_downloaded: download.site_downloaded,
            downloaded: false,
            download_url: Some(download.url),
        });
    }
}

// Original runtime_mark_downloaded body adapted to an isolated Arc and explicit
// folder policy. Publication is represented by the returned changed flag.
pub(super) fn mark_downloaded(
    current: &mut Arc<SrpgShopSnapshot>,
    url: &str,
    destination: &str,
    folder: SrpgShopFolder,
) -> bool {
    let mut snapshot = (**current).clone();
    let mut changed = false;
    for shop in &mut snapshot.shops {
        if download_folder(shop.id, folder) != destination {
            continue;
        }
        for item in &mut shop.items {
            if item.download_url.as_deref() == Some(url) && !item.downloaded {
                item.downloaded = true;
                changed = true;
            }
        }
    }
    if changed {
        *current = Arc::new(snapshot);
    }
    changed
}
