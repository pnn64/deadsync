mod unlock_original {
    use super::*;
    include!("unlock_original.rs");
}

fn unlock_fixture(quests: usize, folders: usize) -> GrooveStatsSubmitApiPlayer {
    let quests: Vec<_> = (0..quests)
        .map(|q| {
            serde_json::json!({
                "title": format!("  Quest {q} \u{2003}"),
                "songDownloadUrl": if q % 3 == 1 { String::from(" \u{2003}") }
                    else { format!(" \u{2003}https://example.invalid/quest-{q}.zip \n") },
                "songDownloadFolders": (0..folders).map(|f| format!("Folder {q}-{f}"))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "chartHash": "deadbeef", "result": "improved",
        "rpg": { "name": "  SRPG \u{2003}", "progress": { "questsCompleted": quests } },
        "itl": { "name": "  ITL \u{2003}", "progress": { "questsCompleted": quests } }
    }))
    .unwrap()
}

#[test]
fn borrowed_unlock_plan_preserves_gates_order_and_labels() {
    for profile_name in ["", " \u{2003}", " Player \u{00e9} "] {
        for accepted in [None, Some(12345)] {
            let mut player = submit_player_job(accepted);
            player.profile_name = profile_name.into();
            for event_mask in 0..4 {
                for with_progress in [false, true] {
                    let mut response = unlock_fixture(6, 3);
                    if !with_progress {
                        response.srpg.as_mut().unwrap().progress = None;
                        response.itl.as_mut().unwrap().progress = None;
                    }
                    if event_mask & 1 == 0 {
                        response.srpg = None;
                    }
                    if event_mask & 2 == 0 {
                        response.itl = None;
                    }
                    for auto in [false, true] {
                        for separate in [false, true] {
                            let old = unlock_original::submit_unlock_plan_from_response(
                                &player, &response, auto, separate,
                            );
                            let new = submit_unlock_plan_from_response(
                                &player, &response, auto, separate,
                            );
                            assert_eq!(old.itl_folder_groups, new.itl_folder_groups);
                            let old_downloads: Vec<_> = old
                                .downloads
                                .iter()
                                .map(|d| {
                                    (
                                        d.url.as_str(),
                                        d.download_name.as_str(),
                                        d.pack_name.as_str(),
                                    )
                                })
                                .collect();
                            let new_downloads: Vec<_> = new
                                .downloads
                                .iter()
                                .map(|d| (d.url, d.download_name.as_str(), d.pack_name.as_str()))
                                .collect();
                            assert_eq!(old_downloads, new_downloads);
                            for event in unlock_events(&player, &response) {
                                let old = unlock_original::unlock_downloads_from_submit_event(
                                    event,
                                    profile_name,
                                    separate,
                                );
                                let new = unlock_downloads_from_submit_event(
                                    event,
                                    profile_name,
                                    separate,
                                );
                                assert_eq!(old.len(), new.len());
                                for (old, new) in old.iter().zip(&new) {
                                    assert_eq!(
                                        (&old.url[..], &old.download_name, &old.pack_name),
                                        (new.url, &new.download_name, &new.pack_name)
                                    );
                                    assert!(unlock_quests(event).iter().any(|q| std::ptr::eq(
                                        new.url,
                                        q.song_download_url.trim()
                                    )));
                                }
                            }
                            if accepted.is_some() {
                                if let Some(event) = &response.itl {
                                    for (group, quest) in
                                        new.itl_folder_groups.iter().zip(unlock_quests(event))
                                    {
                                        assert!(std::ptr::eq(
                                            *group,
                                            quest.song_download_folders.as_slice()
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn borrowed_unlock_folders_allocate_only_the_outer_list() {
    let response = unlock_fixture(32, 8);
    let player = submit_player_job(Some(12345));
    let (_, old) = crate::perf::measure(|| {
        unlock_original::submit_unlock_plan_from_response(&player, &response, false, true)
    });
    let (_, new) =
        crate::perf::measure(|| submit_unlock_plan_from_response(&player, &response, false, true));
    assert_eq!(old.allocs, 289);
    assert_eq!(new.allocs, 1);
    assert_eq!(new.reallocs, 0);
    assert!(new.allocated_bytes < old.allocated_bytes);
}

#[test]
fn queued_unlocks_outlive_the_borrowed_response_and_keep_deduplication() {
    use crate::downloads::{DownloadState, UnlockCache, queue_event_unlock_download};
    let mut old_state = DownloadState::default();
    let mut new_state = DownloadState::default();
    let (old_results, new_results) = {
        let response = submit_player_with_unlocks();
        let player = submit_player_job(Some(12345));
        let old = unlock_original::submit_unlock_plan_from_response(&player, &response, true, true);
        let new = submit_unlock_plan_from_response(&player, &response, true, true);
        let mut old_results = Vec::new();
        let mut new_results = Vec::new();
        for _ in 0..2 {
            for (i, download) in old.downloads.iter().enumerate() {
                old_results.push(queue_event_unlock_download(
                    &mut old_state,
                    &download.url,
                    &download.download_name,
                    &download.pack_name,
                    UnlockCache::new,
                    || i as u64,
                ));
            }
            for (i, download) in new.downloads.iter().enumerate() {
                new_results.push(queue_event_unlock_download(
                    &mut new_state,
                    download.url,
                    &download.download_name,
                    &download.pack_name,
                    UnlockCache::new,
                    || i as u64,
                ));
            }
        }
        (old_results, new_results)
    };
    assert_eq!(old_results, new_results);
    assert_eq!(
        format!("{:?}", old_state.snapshots()),
        format!("{:?}", new_state.snapshots())
    );
    assert!(
        matches!(&new_results[0], crate::downloads::QueueEventUnlockDownloadResult::Queued(d)
        if d.url == "https://example.invalid/srpg.zip")
    );
    assert!(matches!(
        &new_results[2],
        crate::downloads::QueueEventUnlockDownloadResult::Duplicate { .. }
    ));
}

#[test]
#[ignore = "paired performance benchmark"]
fn benchmark_borrowed_unlock_plan() {
    use std::hint::black_box;
    for (quests, folders) in [(0, 0), (2, 2), (32, 8)] {
        let response = unlock_fixture(quests, folders);
        let player = submit_player_job(Some(12345));
        for auto in [false, true] {
            let label = format!("unlocks-{quests}x{folders}-downloads-{auto}");
            let work = |current| {
                if current {
                    black_box(submit_unlock_plan_from_response(
                        black_box(&player),
                        black_box(&response),
                        auto,
                        true,
                    ));
                } else {
                    black_box(unlock_original::submit_unlock_plan_from_response(
                        black_box(&player),
                        black_box(&response),
                        auto,
                        true,
                    ));
                }
            };
            let (_, old) = crate::perf::measure(|| work(false));
            let (_, new) = crate::perf::measure(|| work(true));
            println!("{label} allocations: original {old:?}, current {new:?}");
            crate::paired_bench::compare(&label, if quests == 0 { 100_000 } else { 5_000 }, work);
        }
    }
}
