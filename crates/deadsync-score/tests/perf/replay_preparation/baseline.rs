// Frozen from 07ef01b31 (0.5.1158); unchanged types come from the parent.
use super::*;

pub(crate) fn local_score_date_string(played_at_ms: i64) -> String {
    let Some(dt) = Local.timestamp_millis_opt(played_at_ms).single() else {
        return String::new();
    };
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(crate) fn machine_replay_entry(rank: u32, play: MachineReplayPlay) -> MachineReplayEntry {
    let mut replay = Vec::with_capacity(play.replay.len());
    for edge in play.replay {
        if song_time_ns_invalid(edge.event_music_time_ns) {
            continue;
        }
        replay.push(ReplayEdge {
            event_music_time_ns: edge.event_music_time_ns,
            lane_index: edge.lane,
            pressed: edge.pressed,
            source: edge.input_source(),
        });
    }
    MachineReplayEntry {
        rank,
        name: play.initials,
        score: (play.score_percent * 10000.0).round(),
        date: local_score_date_string(play.played_at_ms),
        is_fail: play.is_fail,
        replay_beat0_time_ns: play.replay_beat0_time_ns,
        replay,
    }
}

pub(crate) fn machine_leaderboard_entry(
    rank: u32,
    play: MachineLeaderboardPlay,
) -> LeaderboardEntry {
    LeaderboardEntry {
        rank,
        name: play.name,
        machine_tag: play.machine_tag,
        score: (play.score_percent * 10000.0).round(),
        date: local_score_date_string(play.played_at_ms),
        is_rival: false,
        is_self: false,
        is_fail: play.is_fail,
    }
}

// The original conversion loop, supplied its owned input independently.
pub(crate) fn replay_edges_from_local(input: Vec<LocalReplayEdge>) -> Vec<ReplayEdge> {
    let mut replay = Vec::with_capacity(input.len());
    for edge in input {
        if song_time_ns_invalid(edge.event_music_time_ns) {
            continue;
        }
        replay.push(ReplayEdge {
            event_music_time_ns: edge.event_music_time_ns,
            lane_index: edge.lane,
            pressed: edge.pressed,
            source: edge.input_source(),
        });
    }
    replay
}
