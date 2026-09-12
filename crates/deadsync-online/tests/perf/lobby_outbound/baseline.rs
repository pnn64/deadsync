// Frozen from 04f769207 (0.5.1156); visibility adapted.
use super::*;

pub(super) fn create_lobby_text(machine: &Value, password: &str) -> String {
    outbound_event_text(
        EVENT_CREATE_LOBBY,
        &serde_json::json!({
            "machine": machine,
            "password": password,
        }),
    )
}

pub(super) fn join_lobby_text(machine: &Value, code: &str, password: &str) -> String {
    outbound_event_text(
        EVENT_JOIN_LOBBY,
        &serde_json::json!({
            "machine": machine,
            "code": code,
            "password": password,
        }),
    )
}

pub(super) fn update_machine_text(machine: &Value) -> String {
    outbound_event_text(
        EVENT_UPDATE_MACHINE,
        &serde_json::json!({
            "machine": machine,
        }),
    )
}

pub(super) fn select_song_text(song_info: &LobbySongInfo) -> String {
    outbound_event_text(
        EVENT_SELECT_SONG,
        &serde_json::json!({
            "songInfo": song_info,
        }),
    )
}

pub(super) fn lobby_profile_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return format!("{LOBBY_PROFILE_PREFIX}Player");
    }
    let prefix_tag: String = trimmed.chars().take(4).collect();
    if prefix_tag.eq_ignore_ascii_case("[DS]") {
        trimmed.to_string()
    } else {
        format!("{LOBBY_PROFILE_PREFIX}{trimmed}")
    }
}

pub(super) fn lobby_machine_player(
    player_id: &str,
    profile_name: &str,
    screen_name: &str,
    ready: bool,
    stats: Option<&MachinePlayerStats>,
) -> LobbyMachinePlayer {
    LobbyMachinePlayer {
        player_id: player_id.to_string(),
        profile_name: lobby_profile_name(profile_name),
        screen_name: screen_name.to_string(),
        ready,
        judgments: stats.and_then(|stats| stats.judgments.clone()),
        score: stats.and_then(|stats| stats.score),
        ex_score: stats.and_then(|stats| stats.ex_score),
    }
}

pub(super) fn lobby_machine_state_value(
    player1: Option<LobbyMachinePlayer>,
    player2: Option<LobbyMachinePlayer>,
) -> Value {
    serde_json::to_value(LobbyMachineState { player1, player2 })
        .expect("serialize lobby machine state")
}
