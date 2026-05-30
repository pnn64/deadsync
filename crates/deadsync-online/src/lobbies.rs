use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const LOBBY_SERVICE_URL: &str = "ws://syncservice.groovestats.com:1337";
pub const LOBBY_PASSWORD_MAX_LEN: usize = 4;
pub const EVENT_SEARCH_LOBBY: &str = "searchLobby";
pub const EVENT_CREATE_LOBBY: &str = "createLobby";
pub const EVENT_JOIN_LOBBY: &str = "joinLobby";
pub const EVENT_LEAVE_LOBBY: &str = "leaveLobby";
pub const EVENT_UPDATE_MACHINE: &str = "updateMachine";
pub const EVENT_SELECT_SONG: &str = "selectSong";
pub const EVENT_LOBBY_SEARCHED: &str = "lobbySearched";
pub const EVENT_LOBBY_STATE: &str = "lobbyState";
pub const EVENT_LOBBY_LEFT: &str = "lobbyLeft";
pub const EVENT_CLIENT_DISCONNECTED: &str = "clientDisconnected";
pub const EVENT_RESPONSE_STATUS: &str = "responseStatus";
const LOBBY_PROFILE_PREFIX: &str = "[DS] ";

pub fn normalize_lobby_password(raw: &str) -> String {
    let mut out = String::with_capacity(LOBBY_PASSWORD_MAX_LEN);
    for ch in raw.chars() {
        if out.len() >= LOBBY_PASSWORD_MAX_LEN {
            break;
        }
        if !ch.is_ascii_graphic() {
            continue;
        }
        out.push(ch.to_ascii_uppercase());
    }
    out
}

#[derive(Debug, Deserialize)]
pub struct InboundEnvelope {
    pub event: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Serialize)]
pub struct OutboundEnvelope<'a> {
    pub event: &'a str,
    pub data: &'a Value,
}

pub fn outbound_event_text(event: &str, data: &Value) -> String {
    serde_json::to_string(&OutboundEnvelope { event, data })
        .expect("serialize lobby outbound envelope")
}

pub fn lobby_profile_name(name: &str) -> String {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicLobby {
    pub code: String,
    pub player_count: usize,
    pub is_password_protected: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicLobbyData {
    pub code: String,
    pub player_count: usize,
    #[serde(default)]
    pub is_password_protected: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct LobbySearchedData {
    #[serde(default)]
    pub lobbies: Vec<PublicLobbyData>,
}

pub fn public_lobbies_from_search(data: LobbySearchedData) -> Vec<PublicLobby> {
    data.lobbies
        .into_iter()
        .map(|lobby| PublicLobby {
            code: lobby.code,
            player_count: lobby.player_count,
            is_password_protected: lobby.is_password_protected,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LobbyJudgments {
    #[serde(default)]
    pub fantastic_plus: u32,
    #[serde(default)]
    pub fantastics: u32,
    #[serde(default)]
    pub excellents: u32,
    #[serde(default)]
    pub greats: u32,
    #[serde(default)]
    pub decents: u32,
    #[serde(default)]
    pub way_offs: u32,
    #[serde(default)]
    pub misses: u32,
    #[serde(default)]
    pub total_steps: u32,
    #[serde(default)]
    pub mines_hit: u32,
    #[serde(default)]
    pub total_mines: u32,
    #[serde(default)]
    pub holds_held: u32,
    #[serde(default)]
    pub total_holds: u32,
    #[serde(default)]
    pub rolls_held: u32,
    #[serde(default)]
    pub total_rolls: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MachinePlayerStats {
    pub judgments: Option<LobbyJudgments>,
    pub score: Option<f32>,
    pub ex_score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyMachinePlayer {
    pub player_id: String,
    pub profile_name: String,
    pub screen_name: String,
    pub ready: bool,
    pub judgments: Option<LobbyJudgments>,
    pub score: Option<f32>,
    #[serde(rename = "exScore")]
    pub ex_score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LobbyMachineState {
    pub player1: Option<LobbyMachinePlayer>,
    pub player2: Option<LobbyMachinePlayer>,
}

pub fn lobby_machine_player(
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

pub fn lobby_machine_state_value(
    player1: Option<LobbyMachinePlayer>,
    player2: Option<LobbyMachinePlayer>,
) -> Value {
    serde_json::to_value(LobbyMachineState { player1, player2 })
        .expect("serialize lobby machine state")
}

#[derive(Debug, Clone, PartialEq)]
pub struct LobbyPlayer {
    pub label: String,
    pub ready: bool,
    pub screen_name: String,
    pub judgments: Option<LobbyJudgments>,
    pub score: Option<f32>,
    pub ex_score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbySongInfo {
    #[serde(default)]
    pub song_path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(rename = "songLength", default)]
    pub song_length_seconds: Option<f32>,
    #[serde(default)]
    pub chart_hash: Option<String>,
    #[serde(default)]
    pub chart_type: Option<String>,
    #[serde(default)]
    pub chart_label: Option<String>,
    #[serde(default)]
    pub rate: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyStatePlayerData {
    pub profile_name: Option<String>,
    pub name: Option<String>,
    pub player_id: Option<String>,
    pub screen_name: Option<String>,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub judgments: Option<LobbyJudgments>,
    #[serde(default)]
    pub score: Option<f32>,
    #[serde(rename = "exScore", default)]
    pub ex_score: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyStateData {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub players: Vec<LobbyStatePlayerData>,
    #[serde(default)]
    pub song_info: Option<LobbySongInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JoinedLobby {
    pub code: String,
    pub players: Vec<LobbyPlayer>,
    pub song_info: Option<LobbySongInfo>,
}

pub fn joined_lobby_from_state(data: LobbyStateData) -> JoinedLobby {
    JoinedLobby {
        code: data.code,
        players: data
            .players
            .into_iter()
            .enumerate()
            .map(|(idx, player)| LobbyPlayer {
                label: player
                    .profile_name
                    .or(player.name)
                    .or(player.player_id)
                    .unwrap_or_else(|| format!("Player {}", idx + 1)),
                ready: player.ready,
                screen_name: player.screen_name.unwrap_or_else(|| "NoScreen".to_string()),
                judgments: player.judgments,
                score: player.score,
                ex_score: player.ex_score,
            })
            .collect(),
        song_info: data.song_info,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseStatus {
    pub event: String,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LobbyLeftData {
    pub left: Option<bool>,
}

#[inline(always)]
pub fn lobby_left_clears_joined(data: &LobbyLeftData) -> bool {
    data.left.unwrap_or(true)
}

#[derive(Debug, Deserialize)]
pub struct ResponseStatusData {
    pub event: String,
    pub success: bool,
    pub message: Option<String>,
}

#[inline(always)]
pub fn response_status_clears_joined(data: &ResponseStatusData) -> bool {
    !data.success && matches!(data.event.as_str(), EVENT_JOIN_LOBBY | EVENT_CREATE_LOBBY)
}

pub fn response_status_from_data(data: ResponseStatusData) -> ResponseStatus {
    ResponseStatus {
        event: data.event,
        success: data.success,
        message: data.message,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub connection: ConnectionState,
    pub available_lobbies: Vec<PublicLobby>,
    pub joined_lobby: Option<JoinedLobby>,
    pub last_status: Option<ResponseStatus>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            connection: ConnectionState::Disconnected,
            available_lobbies: Vec::new(),
            joined_lobby: None,
            last_status: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_defaults_to_disconnected_empty_state() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.connection, ConnectionState::Disconnected);
        assert!(snapshot.available_lobbies.is_empty());
        assert!(snapshot.joined_lobby.is_none());
        assert!(snapshot.last_status.is_none());
    }

    #[test]
    fn lobby_judgments_deserialize_camel_case() {
        let raw = r#"{
            "fantasticPlus": 1,
            "fantastics": 2,
            "excellents": 3,
            "greats": 4,
            "decents": 5,
            "wayOffs": 6,
            "misses": 7,
            "totalSteps": 8,
            "minesHit": 9,
            "totalMines": 10,
            "holdsHeld": 11,
            "totalHolds": 12,
            "rollsHeld": 13,
            "totalRolls": 14
        }"#;
        let judgments: LobbyJudgments = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(judgments.fantastic_plus, 1);
        assert_eq!(judgments.way_offs, 6);
        assert_eq!(judgments.total_rolls, 14);
    }

    #[test]
    fn lobby_song_info_deserializes_server_shape() {
        let raw = r#"{
            "songPath": "Songs/Pack/Song",
            "title": "Song",
            "artist": "Artist",
            "songLength": 123.5,
            "chartHash": "deadbeef",
            "chartType": "dance-single",
            "chartLabel": "Hard",
            "rate": 1.25
        }"#;
        let song_info: LobbySongInfo = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(song_info.song_path, "Songs/Pack/Song");
        assert_eq!(song_info.song_length_seconds, Some(123.5));
        assert_eq!(song_info.chart_hash.as_deref(), Some("deadbeef"));
        assert_eq!(song_info.rate, Some(1.25));
    }

    #[test]
    fn normalize_lobby_password_uppercases_and_caps_length() {
        assert_eq!(normalize_lobby_password("ab1!cd"), "AB1!");
        assert_eq!(LOBBY_PASSWORD_MAX_LEN, 4);
    }

    #[test]
    fn normalize_lobby_password_skips_non_graphic_ascii() {
        assert_eq!(normalize_lobby_password(" a b "), "AB");
        assert_eq!(normalize_lobby_password("ab\n\tcd"), "ABCD");
    }

    #[test]
    fn lobby_profile_name_adds_dead_sync_prefix_once() {
        assert_eq!(lobby_profile_name("Alice"), "[DS] Alice");
        assert_eq!(lobby_profile_name("  [ds] Bob  "), "[ds] Bob");
        assert_eq!(lobby_profile_name(" "), "[DS] Player");
    }

    #[test]
    fn inbound_envelope_defaults_missing_data_to_null() {
        let envelope: InboundEnvelope =
            serde_json::from_str(r#"{"event":"lobbySearched"}"#).expect("deserialize");
        assert_eq!(envelope.event, "lobbySearched");
        assert_eq!(envelope.data, serde_json::Value::Null);
    }

    #[test]
    fn outbound_envelope_serializes_event_and_data() {
        let data = serde_json::json!({
            "code": "ABCD",
            "password": "1234",
        });

        let text = outbound_event_text(EVENT_JOIN_LOBBY, &data);
        let value: serde_json::Value = serde_json::from_str(text.as_str()).expect("json");
        assert_eq!(value["event"], EVENT_JOIN_LOBBY);
        assert_eq!(value["data"]["code"], "ABCD");
        assert_eq!(value["data"]["password"], "1234");
    }

    #[test]
    fn lobby_protocol_constants_match_service() {
        assert_eq!(LOBBY_SERVICE_URL, "ws://syncservice.groovestats.com:1337");
        assert_eq!(EVENT_SEARCH_LOBBY, "searchLobby");
        assert_eq!(EVENT_LOBBY_STATE, "lobbyState");
        assert_eq!(EVENT_RESPONSE_STATUS, "responseStatus");
    }

    #[test]
    fn lobby_searched_data_deserializes_public_lobbies() {
        let raw = r#"{
            "lobbies": [
                {
                    "code": "ABCD",
                    "playerCount": 2,
                    "isPasswordProtected": true
                }
            ]
        }"#;
        let searched: LobbySearchedData = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(searched.lobbies.len(), 1);
        assert_eq!(searched.lobbies[0].code, "ABCD");
        assert_eq!(searched.lobbies[0].player_count, 2);
        assert!(searched.lobbies[0].is_password_protected);
    }

    #[test]
    fn lobby_machine_player_serializes_server_shape() {
        let player = lobby_machine_player(
            "P1",
            "Alice",
            "ScreenSelectMusic",
            true,
            Some(&MachinePlayerStats {
                judgments: Some(LobbyJudgments {
                    fantastics: 12,
                    total_steps: 20,
                    ..LobbyJudgments::default()
                }),
                score: Some(98.5),
                ex_score: Some(97.25),
            }),
        );
        let value = lobby_machine_state_value(Some(player), None);

        assert_eq!(value["player1"]["playerId"], "P1");
        assert_eq!(value["player1"]["profileName"], "[DS] Alice");
        assert_eq!(value["player1"]["screenName"], "ScreenSelectMusic");
        assert_eq!(value["player1"]["ready"], true);
        assert_eq!(value["player1"]["judgments"]["fantastics"], 12);
        assert_eq!(value["player1"]["judgments"]["totalSteps"], 20);
        assert_eq!(value["player1"]["score"], 98.5);
        assert_eq!(value["player1"]["exScore"], 97.25);
        assert!(value["player2"].is_null());
    }

    #[test]
    fn public_lobbies_from_search_maps_server_data() {
        let lobbies = public_lobbies_from_search(LobbySearchedData {
            lobbies: vec![PublicLobbyData {
                code: "ABCD".to_string(),
                player_count: 2,
                is_password_protected: true,
            }],
        });

        assert_eq!(
            lobbies,
            vec![PublicLobby {
                code: "ABCD".to_string(),
                player_count: 2,
                is_password_protected: true,
            }]
        );
    }

    #[test]
    fn joined_lobby_from_state_maps_players_with_fallbacks() {
        let joined = joined_lobby_from_state(LobbyStateData {
            code: "ROOM".to_string(),
            players: vec![
                LobbyStatePlayerData {
                    profile_name: Some("Profile".to_string()),
                    ready: true,
                    screen_name: Some("ScreenGameplay".to_string()),
                    score: Some(99.5),
                    ..LobbyStatePlayerData::default()
                },
                LobbyStatePlayerData {
                    player_id: Some("RemoteP2".to_string()),
                    ..LobbyStatePlayerData::default()
                },
                LobbyStatePlayerData::default(),
            ],
            song_info: Some(LobbySongInfo {
                song_path: "Songs/Pack/Song".to_string(),
                title: Some("Song".to_string()),
                artist: None,
                song_length_seconds: None,
                chart_hash: Some("deadbeef".to_string()),
                chart_type: None,
                chart_label: None,
                rate: None,
            }),
        });

        assert_eq!(joined.code, "ROOM");
        assert_eq!(joined.players.len(), 3);
        assert_eq!(joined.players[0].label, "Profile");
        assert!(joined.players[0].ready);
        assert_eq!(joined.players[0].screen_name, "ScreenGameplay");
        assert_eq!(joined.players[0].score, Some(99.5));
        assert_eq!(joined.players[1].label, "RemoteP2");
        assert_eq!(joined.players[1].screen_name, "NoScreen");
        assert_eq!(joined.players[2].label, "Player 3");
        assert_eq!(
            joined
                .song_info
                .as_ref()
                .and_then(|song| song.chart_hash.as_deref()),
            Some("deadbeef")
        );
    }

    #[test]
    fn lobby_left_defaults_to_clearing_joined_lobby() {
        assert!(lobby_left_clears_joined(&LobbyLeftData { left: None }));
        assert!(lobby_left_clears_joined(&LobbyLeftData {
            left: Some(true)
        }));
        assert!(!lobby_left_clears_joined(&LobbyLeftData {
            left: Some(false)
        }));
    }

    #[test]
    fn response_status_mapping_identifies_failed_join_or_create() {
        let failed_join = ResponseStatusData {
            event: EVENT_JOIN_LOBBY.to_string(),
            success: false,
            message: Some("Bad password".to_string()),
        };
        assert!(response_status_clears_joined(&failed_join));

        let failed_search = ResponseStatusData {
            event: EVENT_SEARCH_LOBBY.to_string(),
            success: false,
            message: None,
        };
        assert!(!response_status_clears_joined(&failed_search));

        let status = response_status_from_data(failed_join);
        assert_eq!(status.event, EVENT_JOIN_LOBBY);
        assert!(!status.success);
        assert_eq!(status.message.as_deref(), Some("Bad password"));
    }
}
