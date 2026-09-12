// Frozen from c157d3295 (0.5.1153); imports and helper visibility adapted.
use super::*;
use serde_json::{Map as JsonMap, Value as JsonValue};
#[derive(Debug, Clone)]
pub struct GrooveStatsSubmitRequestParts {
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: JsonValue,
}

#[derive(Debug)]
pub struct GrooveStatsSubmitRequest {
    pub players: Vec<GrooveStatsSubmitPlayerJob>,
    pub parts: GrooveStatsSubmitRequestParts,
}

#[must_use]
/// # Panics
///
/// Panics if serialization of the generated value fails.
pub fn submit_request_parts(
    players: &[GrooveStatsSubmitPlayerRequest],
) -> GrooveStatsSubmitRequestParts {
    let mut body = JsonMap::with_capacity(players.len());
    let mut headers = Vec::with_capacity(players.len());
    let mut query = Vec::with_capacity(players.len() + 1);
    query.push((
        "maxLeaderboardResults".to_string(),
        GROOVESTATS_SUBMIT_MAX_ENTRIES.to_string(),
    ));

    for player in players {
        headers.push((
            format!("x-api-key-player-{}", player.slot),
            player.api_key.clone(),
        ));
        query.push((
            format!("chartHashP{}", player.slot),
            player.chart_hash.clone(),
        ));
        body.insert(
            format!("player{}", player.slot),
            serde_json::to_value(&player.payload)
                .expect("serialize GrooveStats submit player payload"),
        );
    }

    GrooveStatsSubmitRequestParts {
        headers,
        query,
        body: JsonValue::Object(body),
    }
}

#[must_use]
pub fn submit_request_from_drafts(
    players: Vec<(GrooveStatsSubmitPlayerDraft, u64)>,
) -> GrooveStatsSubmitRequest {
    let request_players: Vec<_> = players
        .iter()
        .map(|(player, _)| player.player_request())
        .collect();
    GrooveStatsSubmitRequest {
        players: players
            .into_iter()
            .map(|(player, token)| player.player_job(token))
            .collect(),
        parts: submit_request_parts(&request_players),
    }
}

#[must_use]
pub fn retry_submit_request(
    entry: &GrooveStatsSubmitRetryEntry,
    token: u64,
) -> GrooveStatsSubmitRequest {
    let player = GrooveStatsSubmitPlayerJob {
        side: entry.side,
        slot: entry.slot,
        chart_hash: entry.chart_hash.clone(),
        username: entry.username.clone(),
        profile_name: entry.profile_name.clone(),
        profile_id: entry.profile_id.clone(),
        token,
        itl_score_hundredths: entry.itl_score_hundredths,
        show_ex_score: entry.show_ex_score,
        score_10000: entry.payload.score,
        rate_hundredths: entry.payload.rate,
        comment: entry.payload.comment.clone(),
    };
    let request_player = GrooveStatsSubmitPlayerRequest {
        slot: entry.slot,
        chart_hash: entry.chart_hash.clone(),
        api_key: entry.api_key.clone(),
        payload: entry.payload.clone(),
    };
    GrooveStatsSubmitRequest {
        players: vec![player],
        parts: submit_request_parts(&[request_player]),
    }
}

#[must_use]
pub fn itl_unlock_folder_groups_from_submit_response<'a>(
    player: &GrooveStatsSubmitPlayerJob,
    response: &'a GrooveStatsSubmitApiPlayer,
) -> Vec<&'a [String]> {
    if player.itl_score_hundredths.is_none() {
        return Vec::new();
    }
    response
        .itl
        .as_ref()
        .and_then(|event| event.progress.as_ref())
        .map(|progress| {
            progress
                .quests_completed
                .iter()
                .map(|quest| quest.song_download_folders.as_slice())
                .collect()
        })
        .unwrap_or_default()
}

#[must_use]
pub fn unlock_events_from_submit_response<'a>(
    player: &GrooveStatsSubmitPlayerJob,
    response: &'a GrooveStatsSubmitApiPlayer,
) -> Vec<&'a GrooveStatsSubmitApiEvent> {
    let mut events = Vec::with_capacity(2);
    if let Some(srpg) = response.srpg.as_ref() {
        events.push(srpg);
    }
    if player.itl_score_hundredths.is_some()
        && let Some(itl) = response.itl.as_ref()
    {
        events.push(itl);
    }
    events
}

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

#[must_use]
pub fn unlock_downloads_from_submit_event(
    event: &GrooveStatsSubmitApiEvent,
    profile_name: &str,
    separate_by_player: bool,
) -> Vec<GrooveStatsUnlockDownload> {
    let Some(progress) = event.progress.as_ref() else {
        return Vec::new();
    };
    let event_name = event_name_or_unknown(event.name.as_str());
    let profile_name = if profile_name.trim().is_empty() {
        "NoName"
    } else {
        profile_name.trim()
    };

    progress
        .quests_completed
        .iter()
        .filter_map(|quest| {
            let url = quest.song_download_url.trim();
            if url.is_empty() {
                return None;
            }
            let title = quest.title.trim();
            let (download_name, pack_name) = if separate_by_player {
                (
                    format!("[{event_name}] {title} - {profile_name}"),
                    format!("{event_name} Unlocks - {profile_name}"),
                )
            } else {
                (
                    format!("[{event_name}] {title}"),
                    format!("{event_name} Unlocks"),
                )
            };
            Some(GrooveStatsUnlockDownload {
                url: url.to_string(),
                download_name: download_name.trim_end().to_string(),
                pack_name,
            })
        })
        .collect()
}

#[must_use]
pub fn submit_unlock_plan_from_response(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
    auto_download_unlocks: bool,
    separate_unlocks_by_player: bool,
) -> GrooveStatsSubmitUnlockPlan {
    let itl_folder_groups = itl_unlock_folder_groups_from_submit_response(player, response)
        .into_iter()
        .map(<[std::string::String]>::to_vec)
        .collect();
    let downloads = if auto_download_unlocks {
        unlock_events_from_submit_response(player, response)
            .into_iter()
            .flat_map(|event| {
                unlock_downloads_from_submit_event(
                    event,
                    player.profile_name.as_str(),
                    separate_unlocks_by_player,
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    GrooveStatsSubmitUnlockPlan {
        itl_folder_groups,
        downloads,
    }
}
