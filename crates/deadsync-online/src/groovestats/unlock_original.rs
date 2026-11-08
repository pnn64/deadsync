#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrooveStatsUnlockDownload {
    pub url: String,
    pub download_name: String,
    pub pack_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrooveStatsSubmitUnlockPlan {
    pub itl_folder_groups: Vec<Vec<String>>,
    pub downloads: Vec<GrooveStatsUnlockDownload>,
}

fn unlock_events<'a>(
    player: &GrooveStatsSubmitPlayerJob,
    response: &'a GrooveStatsSubmitApiPlayer,
) -> impl Iterator<Item = &'a GrooveStatsSubmitApiEvent> + Clone {
    [
        response.srpg.as_ref(),
        response
            .itl
            .as_ref()
            .filter(|_| player.itl_score_hundredths.is_some()),
    ]
    .into_iter()
    .flatten()
}

fn unlock_quests(event: &GrooveStatsSubmitApiEvent) -> &[GrooveStatsSubmitApiQuest] {
    event
        .progress
        .as_ref()
        .map_or(&[], |progress| progress.quests_completed.as_slice())
}

fn download_count(event: &GrooveStatsSubmitApiEvent) -> usize {
    unlock_quests(event)
        .iter()
        .filter(|quest| !quest.song_download_url.trim().is_empty())
        .count()
}

fn append_unlock_downloads(
    out: &mut Vec<GrooveStatsUnlockDownload>,
    event: &GrooveStatsSubmitApiEvent,
    profile_name: &str,
    separate_by_player: bool,
) {
    let event_name = event_name_or_unknown(event.name.as_str());
    let profile_name = if profile_name.trim().is_empty() {
        "NoName"
    } else {
        profile_name.trim()
    };
    for quest in unlock_quests(event) {
        let url = quest.song_download_url.trim();
        if url.is_empty() {
            continue;
        }
        let title = quest.title.trim();
        let suffix_len = if separate_by_player {
            3 + profile_name.len()
        } else {
            0
        };
        let mut download_name =
            String::with_capacity(3 + event_name.len() + title.len() + suffix_len);
        download_name.push('[');
        download_name.push_str(event_name);
        download_name.push_str("] ");
        download_name.push_str(title);
        let mut pack_name = String::with_capacity(event_name.len() + 8 + suffix_len);
        pack_name.push_str(event_name);
        pack_name.push_str(" Unlocks");
        if separate_by_player {
            download_name.push_str(" - ");
            download_name.push_str(profile_name);
            pack_name.push_str(" - ");
            pack_name.push_str(profile_name);
        }
        download_name.truncate(download_name.trim_end().len());
        out.push(GrooveStatsUnlockDownload {
            url: url.to_string(),
            download_name,
            pack_name,
        });
    }
}

#[must_use]
pub fn unlock_downloads_from_submit_event(
    event: &GrooveStatsSubmitApiEvent,
    profile_name: &str,
    separate_by_player: bool,
) -> Vec<GrooveStatsUnlockDownload> {
    let count = download_count(event);
    if count == 0 {
        return Vec::new();
    }
    let mut downloads = Vec::with_capacity(count);
    append_unlock_downloads(&mut downloads, event, profile_name, separate_by_player);
    downloads
}

#[must_use]
pub fn submit_unlock_plan_from_response(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
    auto_download_unlocks: bool,
    separate_unlocks_by_player: bool,
) -> GrooveStatsSubmitUnlockPlan {
    let itl_quests = response
        .itl
        .as_ref()
        .filter(|_| player.itl_score_hundredths.is_some())
        .map_or(&[][..], unlock_quests);
    let itl_folder_groups = itl_quests
        .iter()
        .map(|quest| quest.song_download_folders.as_slice())
        .map(<[String]>::to_vec)
        .collect();
    let mut downloads = Vec::new();
    if auto_download_unlocks {
        let events = unlock_events(player, response);
        let count = events.clone().map(download_count).sum();
        if count != 0 {
            downloads.reserve_exact(count);
            for event in events {
                append_unlock_downloads(
                    &mut downloads,
                    event,
                    player.profile_name.as_str(),
                    separate_unlocks_by_player,
                );
            }
        }
    }
    GrooveStatsSubmitUnlockPlan {
        itl_folder_groups,
        downloads,
    }
}
