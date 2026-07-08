use crate::game::profile;
use deadsync_online::lobbies::{
    LobbyRuntimeHooks, LocalLobbyPlayer, MachinePlayerStats, local_lobby_machine_state_value,
};
use deadsync_profile as profile_data;
use log::{debug, warn};
use serde_json::Value;

#[cfg(test)]
pub use deadsync_online::lobbies::runtime_with_snapshot_for_test as with_snapshot_for_test;
pub use deadsync_online::lobbies::{
    ConnectionState, LOBBY_DISCONNECT_HOLD_SECONDS, LobbyCommand, LobbySongInfo, Snapshot,
};

const RUNTIME_HOOKS: LobbyRuntimeHooks =
    LobbyRuntimeHooks::new(local_machine_state_json, log_malformed_payload);

pub fn snapshot() -> Snapshot {
    deadsync_online::lobbies::runtime_snapshot()
}

pub fn can_update_machine_state() -> bool {
    deadsync_online::lobbies::runtime_can_update_machine_state()
}

pub fn search_lobbies() {
    deadsync_online::lobbies::runtime_search_lobbies(RUNTIME_HOOKS);
}

pub fn create_lobby_with_password(password: &str) {
    deadsync_online::lobbies::runtime_create_lobby_with_password(RUNTIME_HOOKS, password);
}

pub fn join_lobby_with_password(code: &str, password: &str) {
    deadsync_online::lobbies::runtime_join_lobby_with_password(RUNTIME_HOOKS, code, password);
}

pub fn leave_lobby() {
    deadsync_online::lobbies::runtime_leave_lobby(RUNTIME_HOOKS);
}

pub fn update_machine_state(screen_name: &str, ready: bool) {
    deadsync_online::lobbies::runtime_update_machine_state(RUNTIME_HOOKS, screen_name, ready);
}

pub fn update_machine_state_sides_with_stats(
    screen_name: &str,
    p1_ready: bool,
    p2_ready: bool,
    p1_stats: Option<MachinePlayerStats>,
    p2_stats: Option<MachinePlayerStats>,
) {
    deadsync_online::lobbies::runtime_update_machine_state_sides_with_stats(
        RUNTIME_HOOKS,
        screen_name,
        p1_ready,
        p2_ready,
        p1_stats,
        p2_stats,
    );
}

pub fn select_song(song_info: LobbySongInfo) {
    deadsync_online::lobbies::runtime_select_song(RUNTIME_HOOKS, song_info);
}

pub fn disconnect() {
    deadsync_online::lobbies::runtime_disconnect();
}

pub fn poll_reconnect() {
    deadsync_online::lobbies::runtime_poll_reconnect(RUNTIME_HOOKS);
}

pub fn reconnect_status_text() -> Option<String> {
    deadsync_online::lobbies::runtime_reconnect_status_text()
}

fn local_machine_state_json(
    screen_name: &str,
    p1_ready: bool,
    p2_ready: bool,
    p1_stats: Option<&MachinePlayerStats>,
    p2_stats: Option<&MachinePlayerStats>,
) -> Value {
    let p1_joined = profile::is_session_side_joined(profile_data::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile_data::PlayerSide::P2);
    let p1_profile = profile::get_for_side(profile_data::PlayerSide::P1);
    let p2_profile = profile::get_for_side(profile_data::PlayerSide::P2);
    local_lobby_machine_state_value(
        LocalLobbyPlayer {
            side: profile_data::PlayerSide::P1,
            display_name: p1_profile.display_name.as_str(),
            joined: p1_joined,
            screen_name,
            ready: p1_ready,
            stats: p1_stats,
        },
        LocalLobbyPlayer {
            side: profile_data::PlayerSide::P2,
            display_name: p2_profile.display_name.as_str(),
            joined: p2_joined,
            screen_name,
            ready: p2_ready,
            stats: p2_stats,
        },
        profile::get_session_player_side(),
    )
}

fn log_malformed_payload(event: Option<&str>, error: &str, raw_text: &str) {
    match event {
        Some(event) => warn!("Ignoring malformed lobby payload for event '{event}': {error}"),
        None => warn!("Ignoring malformed lobby payload: {error}"),
    }
    debug!("Malformed lobby payload: {raw_text}");
}
