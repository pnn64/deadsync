//! Read ArrowCloud's golf event board. These are server-calculated scores,
//! distinct from the optional native GolfScoring.lua display.
use crate::{OnlineRequestError, arrowcloud};
use deadsync_net as network;
use deadsync_score::{LeaderboardEntry, LeaderboardPane};
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Board {
    total_players: usize,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    rank: u32,
    user_id: String,
    alias: String,
    total_strokes: u64,
    achieved_at: String,
}

pub fn fetch_pane(
    chart_hash: &str,
    api_key: &str,
    max_entries: usize,
) -> Result<Option<LeaderboardPane>, OnlineRequestError> {
    if chart_hash.is_empty()
        || chart_hash.len() > 64
        || !chart_hash.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(None);
    }
    let agent = network::build_agent(network::AgentConfig {
        timeout: Duration::from_secs(3),
    });
    let fetch = |offset: usize| -> Result<Option<Board>, OnlineRequestError> {
        let url = format!(
            "https://golf.arrowcloud.dance/api/holes/{chart_hash}/leaderboard?limit=100&offset={offset}"
        );
        let response = match agent.get(&url).call() {
            Ok(response) => response,
            Err(error) => {
                let error = OnlineRequestError::from(network::error_from_ureq(error));
                if matches!(error.http_status(), Some(400 | 404)) {
                    return Ok(None);
                }
                return Err(error);
            }
        };
        network::read_json_body(response)
            .map(Some)
            .map_err(OnlineRequestError::from)
    };
    // Most songs aren't golf holes. Check the public board before fetching identity.
    let Some(first_page) = fetch(0)? else {
        return Ok(None);
    };
    let context = arrowcloud::fetch_user_context(api_key).ok().flatten();
    let mut remaining = deadsync_score::arrowcloud_target_user_ids(context.as_ref());
    let started = Instant::now();
    let mut entries = Vec::new();
    let mut first_page = Some(first_page);
    for page in 0..8 {
        let offset = page * 100;
        let board = if let Some(first) = first_page.take() {
            first
        } else {
            match fetch(offset) {
                Ok(Some(board)) => board,
                Ok(None) | Err(_) => break,
            }
        };
        let rows = board.rows.len();
        for row in board.rows {
            let is_self = context.as_ref().and_then(|c| c.self_user_id.as_deref())
                == Some(row.user_id.as_str());
            let is_rival = context
                .as_ref()
                .is_some_and(|c| c.rival_user_ids.contains(&row.user_id));
            remaining.remove(row.user_id.as_str());
            if row.rank as usize <= max_entries.max(5) || is_self || is_rival {
                entries.push(LeaderboardEntry {
                    rank: row.rank,
                    name: row.alias,
                    machine_tag: None,
                    score: row.total_strokes as f64,
                    date: row.achieved_at,
                    is_rival,
                    is_self,
                    is_fail: false,
                });
            }
        }
        if rows < 100
            || offset + rows >= board.total_players
            || remaining.is_empty()
            || started.elapsed() >= Duration::from_secs(5)
        {
            break;
        }
    }
    Ok(Some(LeaderboardPane {
        name: "Golf (ArrowCloud)".to_owned(),
        entries,
        is_ex: false,
        disabled: false,
        personalized: context.is_some(),
        arrowcloud_kind: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_raw_strokes_without_percentage_conversion() {
        let board: Board = serde_json::from_str(r#"{"totalPlayers":1,"rows":[{"rank":1,"userId":"u","alias":"Player","totalStrokes":123456,"achievedAt":"2026-10-24"}]}"#).unwrap();
        assert_eq!(board.rows[0].total_strokes, 123456);
        assert_eq!(board.rows[0].rank, 1);
    }
}
