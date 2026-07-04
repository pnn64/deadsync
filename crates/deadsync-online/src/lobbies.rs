use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

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

pub struct LobbySocket {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbySocketError {
    Transient(String),
    Closed,
    Other(String),
}

impl Display for LobbySocketError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(message) | Self::Other(message) => f.write_str(message),
            Self::Closed => f.write_str("Connection closed."),
        }
    }
}

impl Error for LobbySocketError {}

pub fn connect_lobby_socket() -> Result<LobbySocket, LobbySocketError> {
    let (socket, _) =
        tungstenite::connect(LOBBY_SERVICE_URL).map_err(LobbySocketError::from_tungstenite)?;
    let mut socket = LobbySocket { socket };
    set_socket_nonblocking(&mut socket)
        .map_err(|error| LobbySocketError::Other(error.to_string()))?;
    Ok(socket)
}

pub fn send_lobby_text(socket: &mut LobbySocket, text: String) -> Result<(), LobbySocketError> {
    socket
        .socket
        .send(Message::Text(text.into()))
        .map_err(LobbySocketError::from_tungstenite)
}

pub fn read_lobby_text(socket: &mut LobbySocket) -> Result<Option<String>, LobbySocketError> {
    match socket.socket.read() {
        Ok(Message::Text(text)) => Ok(Some(text.to_string())),
        Ok(Message::Close(_)) => Err(LobbySocketError::Closed),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(error)) if error.kind() == ErrorKind::WouldBlock => Ok(None),
        Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
            Err(LobbySocketError::Closed)
        }
        Err(error) => Err(LobbySocketError::from_tungstenite(error)),
    }
}

pub fn send_lobby_ping(socket: &mut LobbySocket) -> Result<(), LobbySocketError> {
    socket
        .socket
        .send(Message::Ping(Vec::new().into()))
        .map_err(LobbySocketError::from_tungstenite)
}

pub fn flush_lobby_socket(socket: &mut LobbySocket) -> Result<(), LobbySocketError> {
    socket
        .socket
        .flush()
        .map_err(LobbySocketError::from_tungstenite)
}

pub fn close_lobby_socket(socket: &mut LobbySocket) {
    let _ = socket.socket.close(None);
}

#[inline(always)]
pub fn is_transient_lobby_socket_error(error: &LobbySocketError) -> bool {
    matches!(error, LobbySocketError::Transient(_))
}

impl LobbySocketError {
    fn from_tungstenite(error: tungstenite::Error) -> Self {
        match error {
            tungstenite::Error::Io(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
            {
                Self::Transient(error.to_string())
            }
            tungstenite::Error::WriteBufferFull(_) => Self::Transient(error.to_string()),
            tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
                Self::Closed
            }
            other => Self::Other(other.to_string()),
        }
    }
}

fn set_socket_nonblocking(socket: &mut LobbySocket) -> Result<(), std::io::Error> {
    match socket.socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_nonblocking(true),
        _ => Ok(()),
    }
}

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

#[inline]
pub fn search_lobby_text() -> String {
    outbound_event_text(EVENT_SEARCH_LOBBY, &serde_json::json!({}))
}

pub fn create_lobby_text(machine: &Value, password: &str) -> String {
    outbound_event_text(
        EVENT_CREATE_LOBBY,
        &serde_json::json!({
            "machine": machine,
            "password": password,
        }),
    )
}

pub fn join_lobby_text(machine: &Value, code: &str, password: &str) -> String {
    outbound_event_text(
        EVENT_JOIN_LOBBY,
        &serde_json::json!({
            "machine": machine,
            "code": code,
            "password": password,
        }),
    )
}

#[inline]
pub fn leave_lobby_text() -> String {
    outbound_event_text(EVENT_LEAVE_LOBBY, &serde_json::json!({}))
}

pub fn update_machine_text(machine: &Value) -> String {
    outbound_event_text(
        EVENT_UPDATE_MACHINE,
        &serde_json::json!({
            "machine": machine,
        }),
    )
}

pub fn select_song_text(song_info: &LobbySongInfo) -> String {
    outbound_event_text(
        EVENT_SELECT_SONG,
        &serde_json::json!({
            "songInfo": song_info,
        }),
    )
}

#[derive(Debug)]
pub enum LobbyInboundEffect {
    Ignore,
    LobbySearched(LobbySearchedData),
    LobbyState(LobbyStateData),
    LobbyLeft(LobbyLeftData),
    ClientDisconnected,
    ResponseStatus(ResponseStatusData),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyInboundParseError {
    pub event: Option<String>,
    pub error: String,
}

pub fn parse_inbound_text(text: &str) -> Result<LobbyInboundEffect, LobbyInboundParseError> {
    let envelope: InboundEnvelope =
        serde_json::from_str(text).map_err(|error| LobbyInboundParseError {
            event: None,
            error: error.to_string(),
        })?;

    match envelope.event.as_str() {
        EVENT_LOBBY_SEARCHED => parse_inbound_data(&envelope.event, envelope.data)
            .map(LobbyInboundEffect::LobbySearched),
        EVENT_LOBBY_STATE => {
            parse_inbound_data(&envelope.event, envelope.data).map(LobbyInboundEffect::LobbyState)
        }
        EVENT_LOBBY_LEFT => {
            parse_inbound_data(&envelope.event, envelope.data).map(LobbyInboundEffect::LobbyLeft)
        }
        EVENT_CLIENT_DISCONNECTED => Ok(LobbyInboundEffect::ClientDisconnected),
        EVENT_RESPONSE_STATUS => parse_inbound_data(&envelope.event, envelope.data)
            .map(LobbyInboundEffect::ResponseStatus),
        _ => Ok(LobbyInboundEffect::Ignore),
    }
}

fn parse_inbound_data<T>(event: &str, data: Value) -> Result<T, LobbyInboundParseError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(data).map_err(|error| LobbyInboundParseError {
        event: Some(event.to_string()),
        error: error.to_string(),
    })
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

#[derive(Debug, Clone)]
pub struct ReconnectTarget {
    pub code: String,
    pub password: String,
}

#[derive(Debug, Default)]
pub struct ReconnectState {
    pub target: Option<ReconnectTarget>,
    pub pending_create_password: Option<String>,
    pub retry_attempts: u32,
    pub next_retry_at: Option<Instant>,
}

impl ReconnectState {
    pub fn clear(&mut self) {
        self.target = None;
        self.pending_create_password = None;
        self.retry_attempts = 0;
        self.next_retry_at = None;
    }

    pub fn set_pending_create(&mut self, password: String) {
        self.target = None;
        self.pending_create_password = Some(password);
        self.retry_attempts = 0;
        self.next_retry_at = None;
    }

    pub fn set_join_target(&mut self, code: String, password: String) {
        self.target = Some(ReconnectTarget { code, password });
        self.pending_create_password = None;
        self.retry_attempts = 0;
        self.next_retry_at = None;
    }

    pub fn set_joined_lobby(&mut self, code: String) {
        let password = self
            .pending_create_password
            .take()
            .or_else(|| self.target.as_ref().map(|target| target.password.clone()))
            .unwrap_or_default();
        self.target = Some(ReconnectTarget { code, password });
        self.retry_attempts = 0;
        self.next_retry_at = None;
    }

    #[inline(always)]
    pub fn has_target(&self) -> bool {
        self.target.is_some()
    }

    pub fn ready_target(&mut self, now: Instant) -> Option<ReconnectTarget> {
        let target = self.target.clone()?;
        if self.next_retry_at.is_some_and(|retry_at| now < retry_at) {
            return None;
        }
        self.next_retry_at = None;
        Some(target)
    }

    pub fn schedule(&mut self, now: Instant) {
        if self.target.is_none() {
            self.retry_attempts = 0;
            self.next_retry_at = None;
            return;
        }
        self.retry_attempts = self.retry_attempts.saturating_add(1);
        self.next_retry_at = Some(now + reconnect_delay(self.retry_attempts));
    }

    pub fn status_text(&self, connection: &ConnectionState, now: Instant) -> Option<String> {
        self.target.as_ref()?;
        match connection {
            ConnectionState::Connected => None,
            ConnectionState::Connecting => Some("Reconnecting to lobby...".to_string()),
            ConnectionState::Disconnected | ConnectionState::Error(_) => {
                if let Some(retry_at) = self.next_retry_at {
                    let remaining = retry_at.saturating_duration_since(now);
                    let seconds = remaining.as_secs().max(1);
                    Some(format!("Connection lost. Retrying in {seconds}s..."))
                } else {
                    Some("Connection lost. Retrying...".to_string())
                }
            }
        }
    }
}

pub fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        0 | 1 => 1,
        2 => 2,
        3 => 3,
        _ => 5,
    })
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
    fn reconnect_delay_caps_after_third_retry() {
        assert_eq!(reconnect_delay(0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(2), Duration::from_secs(2));
        assert_eq!(reconnect_delay(3), Duration::from_secs(3));
        assert_eq!(reconnect_delay(4), Duration::from_secs(5));
        assert_eq!(reconnect_delay(99), Duration::from_secs(5));
    }

    #[test]
    fn reconnect_state_carries_created_lobby_password() {
        let mut reconnect = ReconnectState::default();
        reconnect.set_pending_create("ABCD".to_string());
        reconnect.set_joined_lobby("ROOM".to_string());

        assert_eq!(
            reconnect.target.as_ref().map(|target| target.code.as_str()),
            Some("ROOM")
        );
        assert_eq!(
            reconnect
                .target
                .as_ref()
                .map(|target| target.password.as_str()),
            Some("ABCD")
        );
        assert!(reconnect.pending_create_password.is_none());
        assert_eq!(reconnect.retry_attempts, 0);
        assert!(reconnect.next_retry_at.is_none());
    }

    #[test]
    fn reconnect_state_waits_until_scheduled_retry() {
        let now = Instant::now();
        let mut reconnect = ReconnectState::default();
        reconnect.set_join_target("ROOM".to_string(), "PASS".to_string());
        reconnect.schedule(now);

        assert!(reconnect.ready_target(now).is_none());

        let target = reconnect
            .ready_target(now + Duration::from_secs(2))
            .expect("retry target");
        assert_eq!(target.code, "ROOM");
        assert_eq!(target.password, "PASS");
        assert!(reconnect.next_retry_at.is_none());
    }

    #[test]
    fn reconnect_status_text_reports_connection_phase() {
        let now = Instant::now();
        let mut reconnect = ReconnectState::default();
        assert!(
            reconnect
                .status_text(&ConnectionState::Disconnected, now)
                .is_none()
        );

        reconnect.set_join_target("ROOM".to_string(), "PASS".to_string());
        assert_eq!(
            reconnect.status_text(&ConnectionState::Connecting, now),
            Some("Reconnecting to lobby...".to_string())
        );
        assert_eq!(
            reconnect.status_text(&ConnectionState::Disconnected, now),
            Some("Connection lost. Retrying...".to_string())
        );

        reconnect.next_retry_at = Some(now + Duration::from_secs(3));
        assert_eq!(
            reconnect.status_text(&ConnectionState::Error("closed".to_string()), now),
            Some("Connection lost. Retrying in 3s...".to_string())
        );
    }

    #[test]
    fn lobby_socket_error_display_preserves_connection_text() {
        assert_eq!(LobbySocketError::Closed.to_string(), "Connection closed.");
        assert_eq!(
            LobbySocketError::Other("bad handshake".to_string()).to_string(),
            "bad handshake"
        );
        assert!(is_transient_lobby_socket_error(
            &LobbySocketError::Transient("would block".to_string())
        ));
        assert!(!is_transient_lobby_socket_error(&LobbySocketError::Closed));
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
    fn outbound_lobby_text_helpers_serialize_event_shapes() {
        let machine = serde_json::json!({
            "player1": {
                "playerId": "P1",
                "profileName": "[DS] Alice",
                "screenName": "ScreenSelectMusic",
                "ready": true
            },
            "player2": null
        });

        let search: serde_json::Value =
            serde_json::from_str(search_lobby_text().as_str()).expect("search json");
        assert_eq!(search["event"], EVENT_SEARCH_LOBBY);
        assert_eq!(search["data"], serde_json::json!({}));

        let create: serde_json::Value =
            serde_json::from_str(create_lobby_text(&machine, "ABCD").as_str())
                .expect("create json");
        assert_eq!(create["event"], EVENT_CREATE_LOBBY);
        assert_eq!(create["data"]["machine"], machine);
        assert_eq!(create["data"]["password"], "ABCD");

        let join: serde_json::Value =
            serde_json::from_str(join_lobby_text(&machine, "ROOM", "WXYZ").as_str())
                .expect("join json");
        assert_eq!(join["event"], EVENT_JOIN_LOBBY);
        assert_eq!(join["data"]["machine"], machine);
        assert_eq!(join["data"]["code"], "ROOM");
        assert_eq!(join["data"]["password"], "WXYZ");

        let leave: serde_json::Value =
            serde_json::from_str(leave_lobby_text().as_str()).expect("leave json");
        assert_eq!(leave["event"], EVENT_LEAVE_LOBBY);
        assert_eq!(leave["data"], serde_json::json!({}));

        let update: serde_json::Value =
            serde_json::from_str(update_machine_text(&machine).as_str()).expect("update json");
        assert_eq!(update["event"], EVENT_UPDATE_MACHINE);
        assert_eq!(update["data"]["machine"], machine);
    }

    #[test]
    fn select_song_text_serializes_song_info() {
        let song = LobbySongInfo {
            song_path: "Songs/Pack/Song".to_string(),
            title: Some("Song".to_string()),
            artist: Some("Artist".to_string()),
            song_length_seconds: Some(123.5),
            chart_hash: Some("deadbeef".to_string()),
            chart_type: Some("dance-single".to_string()),
            chart_label: Some("Hard".to_string()),
            rate: Some(1.25),
        };

        let value: serde_json::Value =
            serde_json::from_str(select_song_text(&song).as_str()).expect("song json");
        assert_eq!(value["event"], EVENT_SELECT_SONG);
        assert_eq!(value["data"]["songInfo"]["songPath"], "Songs/Pack/Song");
        assert_eq!(value["data"]["songInfo"]["songLength"], 123.5);
        assert_eq!(value["data"]["songInfo"]["chartHash"], "deadbeef");
    }

    #[test]
    fn parse_inbound_text_routes_lobby_searched() {
        let text = r#"{
            "event":"lobbySearched",
            "data":{
                "lobbies":[{"code":"ABCD","playerCount":2,"isPasswordProtected":true}]
            }
        }"#;

        let effect = parse_inbound_text(text).expect("parse");
        let LobbyInboundEffect::LobbySearched(data) = effect else {
            panic!("expected lobby searched");
        };
        assert_eq!(data.lobbies[0].code, "ABCD");
        assert!(data.lobbies[0].is_password_protected);
    }

    #[test]
    fn parse_inbound_text_routes_lobby_state() {
        let text = r#"{
            "event":"lobbyState",
            "data":{
                "code":"ROOM",
                "players":[{"profileName":"Alice","ready":true}]
            }
        }"#;

        let effect = parse_inbound_text(text).expect("parse");
        let LobbyInboundEffect::LobbyState(data) = effect else {
            panic!("expected lobby state");
        };
        assert_eq!(data.code, "ROOM");
        assert_eq!(data.players[0].profile_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn parse_inbound_text_routes_lobby_left() {
        let effect =
            parse_inbound_text(r#"{"event":"lobbyLeft","data":{"left":false}}"#).expect("parse");
        let LobbyInboundEffect::LobbyLeft(data) = effect else {
            panic!("expected lobby left");
        };
        assert_eq!(data.left, Some(false));
    }

    #[test]
    fn parse_inbound_text_routes_client_disconnected() {
        let effect =
            parse_inbound_text(r#"{"event":"clientDisconnected","data":{}}"#).expect("parse");
        assert!(matches!(effect, LobbyInboundEffect::ClientDisconnected));
    }

    #[test]
    fn parse_inbound_text_routes_response_status() {
        let text = r#"{
            "event":"responseStatus",
            "data":{"event":"joinLobby","success":false,"message":"Bad password"}
        }"#;

        let effect = parse_inbound_text(text).expect("parse");
        let LobbyInboundEffect::ResponseStatus(data) = effect else {
            panic!("expected response status");
        };
        assert_eq!(data.event, EVENT_JOIN_LOBBY);
        assert!(!data.success);
        assert_eq!(data.message.as_deref(), Some("Bad password"));
    }

    #[test]
    fn parse_inbound_text_ignores_unknown_events() {
        let effect = parse_inbound_text(r#"{"event":"unknown","data":{"x":1}}"#).expect("parse");
        assert!(matches!(effect, LobbyInboundEffect::Ignore));
    }

    #[test]
    fn parse_inbound_text_reports_envelope_errors_without_event() {
        let err = parse_inbound_text("not json").expect_err("parse should fail");
        assert_eq!(err.event, None);
        assert!(!err.error.is_empty());
    }

    #[test]
    fn parse_inbound_text_reports_data_errors_with_event() {
        let err = parse_inbound_text(r#"{"event":"lobbyState","data":{"players":"bad"}}"#)
            .expect_err("parse should fail");
        assert_eq!(err.event.as_deref(), Some(EVENT_LOBBY_STATE));
        assert!(!err.error.is_empty());
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
