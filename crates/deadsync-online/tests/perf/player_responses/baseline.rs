// Frozen production implementations from 608e7ae31 (0.5.1224).
use crate::groovestats::*;
use deadsync_score::*;

pub fn leaderboard_entries_from_api(entries: Vec<LeaderboardApiEntry>) -> Vec<LeaderboardEntry> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        out.push(LeaderboardEntry {
            rank: entry.rank,
            name: entry.name,
            machine_tag: entry.machine_tag,
            score: entry.score,
            date: entry.date,
            is_rival: entry.is_rival,
            is_self: entry.is_self,
            is_fail: entry.is_fail,
        });
    }
    out
}

fn push_leaderboard_pane(
    out: &mut Vec<LeaderboardPane>,
    name: &str,
    entries: Vec<LeaderboardApiEntry>,
    is_ex: bool,
) {
    if let Some(pane) = leaderboard_pane_from_api(name, entries, is_ex) {
        out.push(pane);
    }
}

pub fn fetched_player_leaderboards_from_api(
    decoded: LeaderboardsApiResponse,
    username: &str,
    show_ex_score: bool,
) -> FetchedPlayerLeaderboards {
    let mut panes = Vec::with_capacity(5);
    let mut imported_score = None;
    let mut srpg_self_score = None;
    let mut itl_self_score = None;
    let mut itl_self_rank = None;
    let mut itl_self_found = false;
    if let Some(player) = decoded.player1 {
        let LeaderboardApiPlayer {
            is_ranked: _is_ranked,
            gs_leaderboard,
            ex_leaderboard,
            srpg,
            itl,
        } = player;

        imported_score = imported_player_score_from_leaderboard_entries(
            &gs_leaderboard,
            &ex_leaderboard,
            username,
        );
        if show_ex_score {
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
        } else {
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
        }

        if let Some(srpg) = srpg
            && !srpg.srpg_leaderboard.is_empty()
        {
            srpg_self_score = leaderboard_self_score_10000(&srpg.srpg_leaderboard, username);
            let name =
                if srpg.name.trim().is_empty() || srpg.name.trim().eq_ignore_ascii_case("rpg") {
                    "SRPG"
                } else {
                    srpg.name.as_str()
                };
            push_leaderboard_pane(&mut panes, name, srpg.srpg_leaderboard, false);
        }
        if let Some(itl) = itl
            && !itl.itl_leaderboard.is_empty()
        {
            itl_self_found = leaderboard_self_entry(&itl.itl_leaderboard, username).is_some();
            itl_self_score = leaderboard_self_score_10000(&itl.itl_leaderboard, username);
            itl_self_rank = leaderboard_self_rank(&itl.itl_leaderboard, username);
            let name = if itl.name.trim().is_empty() {
                "ITL"
            } else {
                itl.name.as_str()
            };
            push_leaderboard_pane(&mut panes, name, itl.itl_leaderboard, true);
        }
    }

    FetchedPlayerLeaderboards {
        data: PlayerLeaderboardData {
            panes,
            srpg_self_score,
            itl_self_score,
            itl_self_rank,
        },
        imported_score,
        itl_self_found,
    }
}

pub fn player_score_import_result_from_api(
    decoded: LeaderboardsApiResponse,
    endpoint: ScoreImportEndpoint,
    username: &str,
) -> PlayerScoreImportResult {
    let Some(player) = decoded.player1 else {
        return PlayerScoreImportResult::empty();
    };

    let mut result = PlayerScoreImportResult::empty();
    if let Some(entry) = player.gs_leaderboard.iter().find(|entry| {
        score_import_entry_matches_profile(entry.name.as_str(), entry.is_self, endpoint, username)
    }) {
        let ex_score = player
            .ex_leaderboard
            .iter()
            .find(|entry| {
                score_import_entry_matches_profile(
                    entry.name.as_str(),
                    entry.is_self,
                    endpoint,
                    username,
                )
            })
            .and_then(leaderboard_entry_score_10000);
        let ex_evidence = GsExEvidence::from_sources(ex_score, entry.comments.as_deref());
        result.score_proves_nonquint_ex = ex_evidence.proves_nonquint();
        result.score = Some(ImportedPlayerScore {
            score_10000: entry.score,
            comments: entry.comments.clone(),
            is_fail: entry.is_fail,
            ex_evidence,
        });
    }

    if let Some(itl) = player.itl
        && !itl.itl_leaderboard.is_empty()
    {
        result.itl_self_found = leaderboard_self_entry(&itl.itl_leaderboard, username).is_some();
        result.itl_self_score = leaderboard_self_score_10000(&itl.itl_leaderboard, username);
        result.itl_self_rank = leaderboard_self_rank(&itl.itl_leaderboard, username);
    }

    result
}

pub fn leaderboard_pane_from_api(
    name: &str,
    entries: Vec<LeaderboardApiEntry>,
    is_ex: bool,
) -> Option<LeaderboardPane> {
    leaderboard_pane(name, leaderboard_entries_from_api(entries), is_ex)
}

use chrono::{NaiveDateTime, TimeZone};
pub fn parse_itg_datetime_ms(date_time: &str) -> Option<i64> {
    let naive = NaiveDateTime::parse_from_str(date_time.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    // ITGmania writes timestamps in machine-local time. Interpret them the same
    // way; if the local offset is ambiguous (DST fold) take the earliest.
    match chrono::Local.from_local_datetime(&naive) {
        chrono::offset::LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        chrono::offset::LocalResult::Ambiguous(dt, _) => Some(dt.timestamp_millis()),
        chrono::offset::LocalResult::None => None,
    }
}

use deadsync_score::import::{ImportedHighScore, grade_from_itg, music_rate_from_modifiers};
pub fn local_score_from_itg(hs: &ImportedHighScore) -> Option<LocalScoreEntry> {
    let grade = grade_from_itg(&hs.grade)?;

    let counts = [hs.w1, hs.w2, hs.w3, hs.w4, hs.w5, hs.miss];
    let holds_total = hs
        .held
        .saturating_add(hs.let_go)
        .saturating_add(hs.missed_hold);
    let mines_total = hs.hit_mine.saturating_add(hs.avoid_mine);

    let (lamp_index, lamp_judge_count) = compute_local_lamp(counts, grade, None);

    let fail_time = if grade == Grade::Failed {
        Some(hs.survive_seconds.max(0.0))
    } else {
        None
    };

    Some(LocalScoreEntry {
        version: LOCAL_SCORE_VERSION,
        played_at_ms: parse_itg_datetime_ms(&hs.date_time).unwrap_or(0),
        music_rate: music_rate_from_modifiers(&hs.modifiers),
        score_percent: hs.percent_dp.clamp(0.0, 1.0),
        grade_code: grade_to_code(grade),
        lamp_index,
        lamp_judge_count,
        ex_score_percent: 0.0,
        hard_ex_score_percent: 0.0,
        judgment_counts: counts,
        holds_held: hs.held,
        holds_total,
        rolls_held: 0,
        rolls_total: 0,
        mines_avoided: hs.avoid_mine,
        mines_total,
        hands_achieved: 0,
        fail_time,
        beat0_time_ns: 0,
        replay: Vec::new(),
    })
}
