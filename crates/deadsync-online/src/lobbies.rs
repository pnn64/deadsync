use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use deadsync_profile::PlayerSide;

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
pub const LOBBY_SOCKET_POLL_SLEEP: Duration = Duration::from_millis(16);
pub const LOBBY_SOCKET_PING_INTERVAL: Duration = Duration::from_secs(15);
pub const LOBBY_DISCONNECT_HOLD_SECONDS: f32 = 5.0;
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

#[derive(Debug, Clone, PartialEq)]
pub enum LobbyCommand {
    Search,
    Create {
        password: String,
    },
    Join {
        code: String,
        password: String,
    },
    Leave,
    UpdateMachine {
        screen_name: String,
        p1_ready: bool,
        p2_ready: bool,
        p1_stats: Option<MachinePlayerStats>,
        p2_stats: Option<MachinePlayerStats>,
    },
    SelectSong {
        song_info: LobbySongInfo,
    },
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LobbyCommandAction {
    Send(String),
    Close,
}

pub fn lobby_command_action(
    command: &LobbyCommand,
    mut local_machine_state: impl FnMut(
        &str,
        bool,
        bool,
        Option<&MachinePlayerStats>,
        Option<&MachinePlayerStats>,
    ) -> Value,
) -> LobbyCommandAction {
    match command {
        LobbyCommand::Search => LobbyCommandAction::Send(search_lobby_text()),
        LobbyCommand::Create { password } => LobbyCommandAction::Send(create_lobby_text(
            &local_machine_state("ScreenSelectMusic", true, true, None, None),
            password.as_str(),
        )),
        LobbyCommand::Join { code, password } => LobbyCommandAction::Send(join_lobby_text(
            &local_machine_state("ScreenSelectMusic", true, true, None, None),
            code.as_str(),
            password.as_str(),
        )),
        LobbyCommand::Leave => LobbyCommandAction::Send(leave_lobby_text()),
        LobbyCommand::UpdateMachine {
            screen_name,
            p1_ready,
            p2_ready,
            p1_stats,
            p2_stats,
        } => LobbyCommandAction::Send(update_machine_text(&local_machine_state(
            screen_name.as_str(),
            *p1_ready,
            *p2_ready,
            p1_stats.as_ref(),
            p2_stats.as_ref(),
        ))),
        LobbyCommand::SelectSong { song_info } => {
            LobbyCommandAction::Send(select_song_text(song_info))
        }
        LobbyCommand::Disconnect => LobbyCommandAction::Close,
    }
}

pub fn connection_state_from_lobby_socket_error(error: &LobbySocketError) -> ConnectionState {
    match error {
        LobbySocketError::Closed => ConnectionState::Disconnected,
        _ => ConnectionState::Error(error.to_string()),
    }
}

pub fn run_lobby_socket_worker<M, T, C, L>(
    rx: Receiver<LobbyCommand>,
    mut local_machine_state: M,
    mut handle_text_message: T,
    mut handle_connected: C,
    mut handle_connection_loss: L,
) where
    M: FnMut(&str, bool, bool, Option<&MachinePlayerStats>, Option<&MachinePlayerStats>) -> Value,
    T: FnMut(&str) -> Result<(), String>,
    C: FnMut(),
    L: FnMut(ConnectionState),
{
    let mut socket = match connect_lobby_socket() {
        Ok(socket) => socket,
        Err(error) => {
            handle_connection_loss(ConnectionState::Error(error.to_string()));
            return;
        }
    };

    handle_connected();

    let mut last_ping_at = Instant::now();
    loop {
        while let Ok(command) = rx.try_recv() {
            if let Err(error) = handle_lobby_command(&mut socket, command, &mut local_machine_state)
                && !is_transient_lobby_socket_error(&error)
            {
                handle_connection_loss(connection_state_from_lobby_socket_error(&error));
                return;
            }
        }

        match read_lobby_text(&mut socket) {
            Ok(Some(text)) => {
                if let Err(error) = handle_text_message(text.as_str()) {
                    handle_connection_loss(ConnectionState::Error(error));
                    return;
                }
            }
            Ok(None) => {}
            Err(error) => {
                handle_connection_loss(connection_state_from_lobby_socket_error(&error));
                return;
            }
        }

        if last_ping_at.elapsed() >= LOBBY_SOCKET_PING_INTERVAL {
            if let Err(error) = send_lobby_ping(&mut socket)
                && !is_transient_lobby_socket_error(&error)
            {
                handle_connection_loss(connection_state_from_lobby_socket_error(&error));
                return;
            }
            last_ping_at = Instant::now();
        }

        if let Err(error) = flush_lobby_socket(&mut socket)
            && !is_transient_lobby_socket_error(&error)
        {
            handle_connection_loss(connection_state_from_lobby_socket_error(&error));
            return;
        }

        thread::sleep(LOBBY_SOCKET_POLL_SLEEP);
    }
}

fn handle_lobby_command(
    socket: &mut LobbySocket,
    command: LobbyCommand,
    local_machine_state: &mut impl FnMut(
        &str,
        bool,
        bool,
        Option<&MachinePlayerStats>,
        Option<&MachinePlayerStats>,
    ) -> Value,
) -> Result<(), LobbySocketError> {
    match lobby_command_action(&command, local_machine_state) {
        LobbyCommandAction::Send(text) => send_lobby_text(socket, text),
        LobbyCommandAction::Close => {
            close_lobby_socket(socket);
            Err(LobbySocketError::Closed)
        }
    }
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

pub struct LocalLobbyPlayer<'a> {
    pub side: PlayerSide,
    pub display_name: &'a str,
    pub joined: bool,
    pub screen_name: &'a str,
    pub ready: bool,
    pub stats: Option<&'a MachinePlayerStats>,
}

pub fn local_lobby_machine_state_value(
    p1: LocalLobbyPlayer<'_>,
    p2: LocalLobbyPlayer<'_>,
    session_side: PlayerSide,
) -> Value {
    if !(p1.joined || p2.joined) {
        return lobby_machine_state_value(
            (session_side == PlayerSide::P1).then(|| local_lobby_machine_player(p1)),
            (session_side == PlayerSide::P2).then(|| local_lobby_machine_player(p2)),
        );
    }

    lobby_machine_state_value(
        p1.joined.then(|| local_lobby_machine_player(p1)),
        p2.joined.then(|| local_lobby_machine_player(p2)),
    )
}

pub fn local_lobby_machine_player(player: LocalLobbyPlayer<'_>) -> LobbyMachinePlayer {
    lobby_machine_player(
        lobby_player_id(player.side),
        player.display_name,
        player.screen_name,
        player.ready,
        player.stats,
    )
}

#[inline(always)]
pub const fn lobby_player_id(side: PlayerSide) -> &'static str {
    match side {
        PlayerSide::P1 => "P1",
        PlayerSide::P2 => "P2",
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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

#[inline(always)]
pub fn can_update_machine_state(snapshot: &Snapshot) -> bool {
    matches!(snapshot.connection, ConnectionState::Connected) && snapshot.joined_lobby.is_some()
}

pub fn apply_local_lobby_leave(snapshot: &mut Snapshot) -> bool {
    let connected = matches!(snapshot.connection, ConnectionState::Connected);
    let was_joined = snapshot.joined_lobby.take().is_some();
    if !connected {
        snapshot.last_status = None;
    }
    connected && was_joined
}

pub fn apply_local_lobby_disconnect(snapshot: &mut Snapshot) {
    snapshot.connection = ConnectionState::Disconnected;
    snapshot.joined_lobby = None;
    snapshot.last_status = None;
}

pub fn select_song_command(snapshot: &Snapshot, song_info: LobbySongInfo) -> Option<LobbyCommand> {
    can_update_machine_state(snapshot).then_some(LobbyCommand::SelectSong { song_info })
}

#[derive(Debug, Clone, PartialEq)]
pub struct MachineStateUpdate {
    pub signature: String,
    pub command: LobbyCommand,
}

pub fn machine_state_signature(joined_code: &str, machine_state: &Value) -> String {
    format!("{joined_code}|{machine_state}")
}

#[allow(clippy::too_many_arguments)]
pub fn machine_state_update_command(
    snapshot: &Snapshot,
    last_signature: Option<&str>,
    machine_state: &Value,
    screen_name: &str,
    p1_ready: bool,
    p2_ready: bool,
    p1_stats: Option<MachinePlayerStats>,
    p2_stats: Option<MachinePlayerStats>,
) -> Option<MachineStateUpdate> {
    if !matches!(snapshot.connection, ConnectionState::Connected) {
        return None;
    }
    let joined = snapshot.joined_lobby.as_ref()?;
    let signature = machine_state_signature(joined.code.as_str(), machine_state);
    if last_signature == Some(signature.as_str()) {
        return None;
    }
    Some(MachineStateUpdate {
        signature,
        command: LobbyCommand::UpdateMachine {
            screen_name: screen_name.to_string(),
            p1_ready,
            p2_ready,
            p1_stats,
            p2_stats,
        },
    })
}

pub fn apply_lobby_inbound_effect(
    snapshot: &mut Snapshot,
    reconnect: &mut ReconnectState,
    effect: LobbyInboundEffect,
) -> bool {
    match effect {
        LobbyInboundEffect::Ignore => false,
        LobbyInboundEffect::LobbySearched(data) => {
            snapshot.available_lobbies = public_lobbies_from_search(data);
            false
        }
        LobbyInboundEffect::LobbyState(data) => {
            reconnect.set_joined_lobby(data.code.clone());
            snapshot.joined_lobby = Some(joined_lobby_from_state(data));
            false
        }
        LobbyInboundEffect::LobbyLeft(data) => {
            if !lobby_left_clears_joined(&data) {
                return false;
            }
            reconnect.clear();
            snapshot.joined_lobby = None;
            true
        }
        LobbyInboundEffect::ClientDisconnected => {
            reconnect.clear();
            snapshot.joined_lobby = None;
            true
        }
        LobbyInboundEffect::ResponseStatus(data) => {
            let clears_lobby = response_status_clears_joined(&data);
            let status = response_status_from_data(data);
            if clears_lobby {
                reconnect.clear();
                snapshot.joined_lobby = None;
                snapshot.last_status = Some(status);
                return true;
            }
            snapshot.last_status = Some(status);
            false
        }
    }
}

pub fn apply_lobby_connection_loss(
    snapshot: &mut Snapshot,
    connection: ConnectionState,
    preserve_joined: bool,
) {
    snapshot.connection = connection;
    if !preserve_joined {
        snapshot.joined_lobby = None;
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

pub type LocalMachineStateFn =
    fn(&str, bool, bool, Option<&MachinePlayerStats>, Option<&MachinePlayerStats>) -> Value;
pub type MalformedLobbyPayloadFn = fn(Option<&str>, &str, &str);

#[derive(Clone, Copy)]
pub struct LobbyRuntimeHooks {
    local_machine_state: LocalMachineStateFn,
    malformed_payload: MalformedLobbyPayloadFn,
}

impl LobbyRuntimeHooks {
    #[inline(always)]
    pub const fn new(
        local_machine_state: LocalMachineStateFn,
        malformed_payload: MalformedLobbyPayloadFn,
    ) -> Self {
        Self {
            local_machine_state,
            malformed_payload,
        }
    }
}

static RUNTIME_SNAPSHOT: LazyLock<Mutex<Snapshot>> =
    LazyLock::new(|| Mutex::new(Snapshot::default()));
static RUNTIME_COMMAND_TX: LazyLock<Mutex<Option<Sender<LobbyCommand>>>> =
    LazyLock::new(|| Mutex::new(None));
static RUNTIME_LAST_MACHINE_STATE_SIG: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));
static RUNTIME_RECONNECT_STATE: LazyLock<Mutex<ReconnectState>> =
    LazyLock::new(|| Mutex::new(ReconnectState::default()));
#[cfg(test)]
static RUNTIME_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub fn runtime_snapshot() -> Snapshot {
    RUNTIME_SNAPSHOT.lock().unwrap().clone()
}

#[cfg(test)]
pub fn runtime_with_snapshot_for_test<R>(snapshot: Snapshot, f: impl FnOnce() -> R) -> R {
    let _guard = RUNTIME_TEST_MUTEX.lock().unwrap();
    let prev = std::mem::replace(&mut *RUNTIME_SNAPSHOT.lock().unwrap(), snapshot);
    let result = f();
    *RUNTIME_SNAPSHOT.lock().unwrap() = prev;
    result
}

pub fn runtime_can_update_machine_state() -> bool {
    let snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
    can_update_machine_state(&snapshot)
}

pub fn runtime_search_lobbies(hooks: LobbyRuntimeHooks) {
    let _ = runtime_send_command(hooks, LobbyCommand::Search);
}

pub fn runtime_create_lobby_with_password(hooks: LobbyRuntimeHooks, password: &str) {
    let password = normalize_lobby_password(password);
    {
        let mut reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
        reconnect.set_pending_create(password.clone());
    }
    let _ = runtime_send_command(hooks, LobbyCommand::Create { password });
}

pub fn runtime_join_lobby_with_password(hooks: LobbyRuntimeHooks, code: &str, password: &str) {
    let code = code.trim();
    if code.is_empty() {
        return;
    }
    let password = normalize_lobby_password(password);
    {
        let mut reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
        reconnect.set_join_target(code.to_string(), password.clone());
    }
    let _ = runtime_send_command(
        hooks,
        LobbyCommand::Join {
            code: code.to_string(),
            password,
        },
    );
}

pub fn runtime_leave_lobby(hooks: LobbyRuntimeHooks) {
    runtime_clear_reconnect_target();
    *RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
    let should_send_leave = {
        let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
        apply_local_lobby_leave(&mut snapshot)
    };
    if should_send_leave {
        let _ = runtime_send_command(hooks, LobbyCommand::Leave);
    }
}

pub fn runtime_update_machine_state(hooks: LobbyRuntimeHooks, screen_name: &str, ready: bool) {
    runtime_update_machine_state_sides_with_stats(hooks, screen_name, ready, ready, None, None);
}

pub fn runtime_update_machine_state_sides_with_stats(
    hooks: LobbyRuntimeHooks,
    screen_name: &str,
    p1_ready: bool,
    p2_ready: bool,
    p1_stats: Option<MachinePlayerStats>,
    p2_stats: Option<MachinePlayerStats>,
) {
    let machine_state = (hooks.local_machine_state)(
        screen_name,
        p1_ready,
        p2_ready,
        p1_stats.as_ref(),
        p2_stats.as_ref(),
    );
    let update = {
        let snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
        let last_sig = RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap();
        machine_state_update_command(
            &snapshot,
            last_sig.as_deref(),
            &machine_state,
            screen_name,
            p1_ready,
            p2_ready,
            p1_stats,
            p2_stats,
        )
    };
    if let Some(update) = update
        && runtime_send_command(hooks, update.command)
    {
        *RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap() = Some(update.signature);
    }
}

pub fn runtime_select_song(hooks: LobbyRuntimeHooks, song_info: LobbySongInfo) {
    let snapshot = RUNTIME_SNAPSHOT.lock().unwrap().clone();
    if let Some(command) = select_song_command(&snapshot, song_info) {
        let _ = runtime_send_command(hooks, command);
    }
}

pub fn runtime_disconnect() {
    runtime_clear_reconnect_target();
    if let Some(tx) = RUNTIME_COMMAND_TX.lock().unwrap().take() {
        let _ = tx.send(LobbyCommand::Disconnect);
    }
    *RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
    let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
    apply_local_lobby_disconnect(&mut snapshot);
}

pub fn runtime_poll_reconnect(hooks: LobbyRuntimeHooks) {
    let target = {
        let snapshot = RUNTIME_SNAPSHOT.lock().unwrap().clone();
        if matches!(
            snapshot.connection,
            ConnectionState::Connected | ConnectionState::Connecting
        ) {
            return;
        }
        let mut reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
        let Some(target) = reconnect.ready_target(Instant::now()) else {
            return;
        };
        target
    };

    let _ = runtime_send_command(
        hooks,
        LobbyCommand::Join {
            code: target.code,
            password: target.password,
        },
    );
}

pub fn runtime_reconnect_status_text() -> Option<String> {
    let snapshot = RUNTIME_SNAPSHOT.lock().unwrap().clone();
    let reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
    reconnect.status_text(&snapshot.connection, Instant::now())
}

fn runtime_ensure_worker(hooks: LobbyRuntimeHooks) {
    let mut tx_slot = RUNTIME_COMMAND_TX.lock().unwrap();
    if tx_slot.is_some() {
        return;
    }

    let (tx, rx) = mpsc::channel();
    *tx_slot = Some(tx);
    drop(tx_slot);

    let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
    snapshot.connection = ConnectionState::Connecting;
    snapshot.last_status = None;
    drop(snapshot);

    thread::spawn(move || runtime_worker_main(rx, hooks));
}

fn runtime_send_command(hooks: LobbyRuntimeHooks, command: LobbyCommand) -> bool {
    runtime_ensure_worker(hooks);
    let tx_opt = RUNTIME_COMMAND_TX.lock().unwrap().clone();
    let Some(tx) = tx_opt else {
        return false;
    };
    if tx.send(command).is_ok() {
        return true;
    }

    RUNTIME_COMMAND_TX.lock().unwrap().take();
    false
}

fn runtime_clear_reconnect_target() {
    let mut reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
    reconnect.clear();
}

fn runtime_schedule_reconnect() {
    let mut reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
    reconnect.schedule(Instant::now());
}

fn runtime_should_preserve_joined_lobby() -> bool {
    RUNTIME_RECONNECT_STATE.lock().unwrap().has_target()
}

fn runtime_handle_connection_loss(connection: ConnectionState) {
    let preserve_joined = runtime_should_preserve_joined_lobby();
    {
        let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
        apply_lobby_connection_loss(&mut snapshot, connection, preserve_joined);
    }
    *RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
    RUNTIME_COMMAND_TX.lock().unwrap().take();
    if preserve_joined {
        runtime_schedule_reconnect();
    }
}

fn runtime_worker_main(rx: Receiver<LobbyCommand>, hooks: LobbyRuntimeHooks) {
    run_lobby_socket_worker(
        rx,
        hooks.local_machine_state,
        |text| runtime_handle_text_message(text, hooks.malformed_payload),
        || {
            let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
            snapshot.connection = ConnectionState::Connected;
            snapshot.last_status = None;
        },
        runtime_handle_connection_loss,
    );
}

fn runtime_handle_text_message(
    text: &str,
    malformed_payload: MalformedLobbyPayloadFn,
) -> Result<(), String> {
    let effect = match parse_inbound_text(text) {
        Ok(effect) => effect,
        Err(error) => {
            malformed_payload(error.event.as_deref(), error.error.as_str(), text);
            return Ok(());
        }
    };

    let clear_machine_state_sig = {
        let mut reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
        let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
        apply_lobby_inbound_effect(&mut snapshot, &mut reconnect, effect)
    };
    if clear_machine_state_sig {
        *RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_machine_state(
        _screen_name: &str,
        _p1_ready: bool,
        _p2_ready: bool,
        _p1_stats: Option<&MachinePlayerStats>,
        _p2_stats: Option<&MachinePlayerStats>,
    ) -> Value {
        serde_json::json!({})
    }

    fn ignore_malformed_payload(_event: Option<&str>, _error: &str, _raw_text: &str) {}

    fn runtime_hooks() -> LobbyRuntimeHooks {
        LobbyRuntimeHooks::new(test_machine_state, ignore_malformed_payload)
    }

    fn reset_runtime_test_state() {
        *RUNTIME_SNAPSHOT.lock().unwrap() = Snapshot::default();
        *RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
        *RUNTIME_RECONNECT_STATE.lock().unwrap() = ReconnectState::default();
        RUNTIME_COMMAND_TX.lock().unwrap().take();
    }

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
    fn apply_lobby_inbound_effect_updates_search_and_join_state() {
        let mut snapshot = Snapshot::default();
        let mut reconnect = ReconnectState::default();
        reconnect.set_pending_create("PASS".to_string());

        let clear_sig = apply_lobby_inbound_effect(
            &mut snapshot,
            &mut reconnect,
            LobbyInboundEffect::LobbySearched(LobbySearchedData {
                lobbies: vec![PublicLobbyData {
                    code: "ABCD".to_string(),
                    player_count: 2,
                    is_password_protected: true,
                }],
            }),
        );
        assert!(!clear_sig);
        assert_eq!(
            snapshot.available_lobbies,
            vec![PublicLobby {
                code: "ABCD".to_string(),
                player_count: 2,
                is_password_protected: true,
            }]
        );

        let clear_sig = apply_lobby_inbound_effect(
            &mut snapshot,
            &mut reconnect,
            LobbyInboundEffect::LobbyState(LobbyStateData {
                code: "ROOM".to_string(),
                players: vec![LobbyStatePlayerData {
                    profile_name: Some("Remote".to_string()),
                    screen_name: Some("ScreenGameplay".to_string()),
                    ready: true,
                    score: Some(99.0),
                    ..LobbyStatePlayerData::default()
                }],
                song_info: None,
            }),
        );
        assert!(!clear_sig);
        assert_eq!(
            reconnect.target.as_ref().map(|target| target.code.as_str()),
            Some("ROOM")
        );
        assert_eq!(
            reconnect
                .target
                .as_ref()
                .map(|target| target.password.as_str()),
            Some("PASS")
        );
        assert_eq!(
            snapshot
                .joined_lobby
                .as_ref()
                .map(|lobby| lobby.code.as_str()),
            Some("ROOM")
        );
        assert_eq!(snapshot.joined_lobby.as_ref().unwrap().players.len(), 1);
    }

    #[test]
    fn apply_lobby_inbound_effect_clears_join_state_when_server_leaves() {
        let mut snapshot = Snapshot {
            connection: ConnectionState::Connected,
            available_lobbies: Vec::new(),
            joined_lobby: Some(JoinedLobby {
                code: "ROOM".to_string(),
                players: Vec::new(),
                song_info: None,
            }),
            last_status: None,
        };
        let mut reconnect = ReconnectState::default();
        reconnect.set_join_target("ROOM".to_string(), "PASS".to_string());

        let clear_sig = apply_lobby_inbound_effect(
            &mut snapshot,
            &mut reconnect,
            LobbyInboundEffect::LobbyLeft(LobbyLeftData { left: Some(false) }),
        );
        assert!(!clear_sig);
        assert!(snapshot.joined_lobby.is_some());
        assert!(reconnect.target.is_some());

        let clear_sig = apply_lobby_inbound_effect(
            &mut snapshot,
            &mut reconnect,
            LobbyInboundEffect::ClientDisconnected,
        );
        assert!(clear_sig);
        assert!(snapshot.joined_lobby.is_none());
        assert!(reconnect.target.is_none());
    }

    #[test]
    fn apply_lobby_inbound_effect_failed_join_clears_and_sets_status() {
        let mut snapshot = Snapshot {
            connection: ConnectionState::Connected,
            available_lobbies: Vec::new(),
            joined_lobby: Some(JoinedLobby {
                code: "ROOM".to_string(),
                players: Vec::new(),
                song_info: None,
            }),
            last_status: None,
        };
        let mut reconnect = ReconnectState::default();
        reconnect.set_join_target("ROOM".to_string(), "PASS".to_string());

        let clear_sig = apply_lobby_inbound_effect(
            &mut snapshot,
            &mut reconnect,
            LobbyInboundEffect::ResponseStatus(ResponseStatusData {
                event: EVENT_JOIN_LOBBY.to_string(),
                success: false,
                message: Some("bad password".to_string()),
            }),
        );

        assert!(clear_sig);
        assert!(snapshot.joined_lobby.is_none());
        assert!(reconnect.target.is_none());
        assert_eq!(
            snapshot.last_status,
            Some(ResponseStatus {
                event: EVENT_JOIN_LOBBY.to_string(),
                success: false,
                message: Some("bad password".to_string()),
            })
        );
    }

    #[test]
    fn apply_lobby_connection_loss_preserves_or_clears_joined_lobby() {
        let joined = JoinedLobby {
            code: "ROOM".to_string(),
            players: Vec::new(),
            song_info: None,
        };
        let mut snapshot = Snapshot {
            connection: ConnectionState::Connected,
            available_lobbies: Vec::new(),
            joined_lobby: Some(joined.clone()),
            last_status: None,
        };

        apply_lobby_connection_loss(
            &mut snapshot,
            ConnectionState::Error("lost".to_string()),
            true,
        );
        assert_eq!(snapshot.joined_lobby, Some(joined));

        apply_lobby_connection_loss(&mut snapshot, ConnectionState::Disconnected, false);
        assert_eq!(snapshot.connection, ConnectionState::Disconnected);
        assert!(snapshot.joined_lobby.is_none());
    }

    #[test]
    fn local_lobby_leave_transition_clears_joined_and_reports_send() {
        let mut snapshot = Snapshot {
            connection: ConnectionState::Connected,
            available_lobbies: Vec::new(),
            joined_lobby: Some(JoinedLobby {
                code: "ABCD".to_string(),
                players: Vec::new(),
                song_info: None,
            }),
            last_status: None,
        };

        assert!(can_update_machine_state(&snapshot));
        assert!(apply_local_lobby_leave(&mut snapshot));
        assert!(snapshot.joined_lobby.is_none());
        assert!(!can_update_machine_state(&snapshot));

        let mut disconnected = Snapshot {
            connection: ConnectionState::Disconnected,
            last_status: Some(ResponseStatus {
                event: EVENT_JOIN_LOBBY.to_string(),
                success: false,
                message: Some("bad".to_string()),
            }),
            ..Snapshot::default()
        };
        assert!(!apply_local_lobby_leave(&mut disconnected));
        assert!(disconnected.last_status.is_none());
    }

    #[test]
    fn local_lobby_disconnect_transition_clears_state() {
        let mut snapshot = Snapshot {
            connection: ConnectionState::Connected,
            available_lobbies: Vec::new(),
            joined_lobby: Some(JoinedLobby {
                code: "ABCD".to_string(),
                players: Vec::new(),
                song_info: None,
            }),
            last_status: Some(ResponseStatus {
                event: EVENT_JOIN_LOBBY.to_string(),
                success: true,
                message: None,
            }),
        };

        apply_local_lobby_disconnect(&mut snapshot);

        assert_eq!(snapshot.connection, ConnectionState::Disconnected);
        assert!(snapshot.joined_lobby.is_none());
        assert!(snapshot.last_status.is_none());
    }

    #[test]
    fn runtime_ignores_malformed_payload_without_changing_snapshot() {
        let _guard = RUNTIME_TEST_MUTEX.lock().unwrap();
        reset_runtime_test_state();

        let existing_player = LobbyPlayer {
            label: "Existing".to_string(),
            ready: true,
            screen_name: "ScreenGameplay".to_string(),
            judgments: None,
            score: Some(98.5),
            ex_score: Some(97.25),
        };
        {
            let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
            snapshot.connection = ConnectionState::Connected;
            snapshot.joined_lobby = Some(JoinedLobby {
                code: "ABCD".to_string(),
                players: vec![existing_player.clone()],
                song_info: None,
            });
        }

        let malformed = r#"{
            "event":"lobbyState",
            "data":{
                "code":"ABCD",
                "players":[{
                    "profileName":"Remote",
                    "screenName":"ScreenGameplay",
                    "ready":true,
                    "score":"98.50"
                }]
            }
        }"#;

        assert!(runtime_handle_text_message(malformed, ignore_malformed_payload).is_ok());

        let snapshot = RUNTIME_SNAPSHOT.lock().unwrap().clone();
        assert_eq!(snapshot.connection, ConnectionState::Connected);
        assert_eq!(
            snapshot.joined_lobby,
            Some(JoinedLobby {
                code: "ABCD".to_string(),
                players: vec![existing_player],
                song_info: None,
            })
        );

        reset_runtime_test_state();
    }

    #[test]
    fn runtime_leave_lobby_clears_local_state_before_server_reply() {
        let _guard = RUNTIME_TEST_MUTEX.lock().unwrap();
        reset_runtime_test_state();

        let (tx, rx) = std::sync::mpsc::channel();
        *RUNTIME_COMMAND_TX.lock().unwrap() = Some(tx);
        *RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap() = Some("ABCD|{}".to_string());
        *RUNTIME_RECONNECT_STATE.lock().unwrap() = ReconnectState {
            target: Some(ReconnectTarget {
                code: "ABCD".to_string(),
                password: "PASS".to_string(),
            }),
            pending_create_password: None,
            retry_attempts: 2,
            next_retry_at: Some(std::time::Instant::now()),
        };
        {
            let mut snapshot = RUNTIME_SNAPSHOT.lock().unwrap();
            snapshot.connection = ConnectionState::Connected;
            snapshot.joined_lobby = Some(JoinedLobby {
                code: "ABCD".to_string(),
                players: vec![LobbyPlayer {
                    label: "Local".to_string(),
                    ready: true,
                    screen_name: "ScreenSelectMusic".to_string(),
                    judgments: None,
                    score: None,
                    ex_score: None,
                }],
                song_info: None,
            });
        }

        runtime_leave_lobby(runtime_hooks());

        let snapshot = RUNTIME_SNAPSHOT.lock().unwrap().clone();
        assert!(snapshot.joined_lobby.is_none());
        assert!(RUNTIME_LAST_MACHINE_STATE_SIG.lock().unwrap().is_none());
        let reconnect = RUNTIME_RECONNECT_STATE.lock().unwrap();
        assert!(reconnect.target.is_none());
        assert!(reconnect.pending_create_password.is_none());
        assert_eq!(reconnect.retry_attempts, 0);
        assert!(reconnect.next_retry_at.is_none());
        assert!(matches!(rx.try_recv(), Ok(LobbyCommand::Leave)));

        drop(reconnect);
        reset_runtime_test_state();
    }

    #[test]
    fn local_lobby_select_song_requires_connected_joined_lobby() {
        let song_info = LobbySongInfo {
            song_path: "Songs/Pack/Song".to_string(),
            title: Some("Song".to_string()),
            ..LobbySongInfo::default()
        };
        assert_eq!(
            select_song_command(&Snapshot::default(), song_info.clone()),
            None
        );

        let snapshot = Snapshot {
            connection: ConnectionState::Connected,
            joined_lobby: Some(JoinedLobby {
                code: "ABCD".to_string(),
                players: Vec::new(),
                song_info: None,
            }),
            ..Snapshot::default()
        };
        assert_eq!(
            select_song_command(&snapshot, song_info.clone()),
            Some(LobbyCommand::SelectSong { song_info })
        );
    }

    #[test]
    fn machine_state_update_command_skips_unjoined_and_duplicates() {
        let machine = serde_json::json!({
            "player1": {"profileName": "[DS] Alice"},
            "player2": null
        });
        let snapshot = Snapshot {
            connection: ConnectionState::Connected,
            joined_lobby: Some(JoinedLobby {
                code: "ROOM".to_string(),
                players: Vec::new(),
                song_info: None,
            }),
            ..Snapshot::default()
        };
        let signature = machine_state_signature("ROOM", &machine);

        assert_eq!(
            machine_state_update_command(
                &Snapshot::default(),
                None,
                &machine,
                "ScreenGameplay",
                true,
                false,
                None,
                None,
            ),
            None
        );
        assert_eq!(
            machine_state_update_command(
                &snapshot,
                Some(signature.as_str()),
                &machine,
                "ScreenGameplay",
                true,
                false,
                None,
                None,
            ),
            None
        );

        let update = machine_state_update_command(
            &snapshot,
            None,
            &machine,
            "ScreenGameplay",
            true,
            false,
            None,
            None,
        )
        .expect("update");
        assert_eq!(update.signature, signature);
        assert_eq!(
            update.command,
            LobbyCommand::UpdateMachine {
                screen_name: "ScreenGameplay".to_string(),
                p1_ready: true,
                p2_ready: false,
                p1_stats: None,
                p2_stats: None,
            }
        );
    }

    #[test]
    fn lobby_command_action_builds_update_machine_payload() {
        let command = LobbyCommand::UpdateMachine {
            screen_name: "ScreenGameplay".to_string(),
            p1_ready: true,
            p2_ready: false,
            p1_stats: Some(MachinePlayerStats {
                score: Some(98.5),
                ..MachinePlayerStats::default()
            }),
            p2_stats: None,
        };

        let action = lobby_command_action(&command, |screen, p1_ready, p2_ready, p1, p2| {
            serde_json::json!({
                "screen": screen,
                "p1Ready": p1_ready,
                "p2Ready": p2_ready,
                "p1Score": p1.and_then(|stats| stats.score),
                "p2Present": p2.is_some(),
            })
        });

        let LobbyCommandAction::Send(text) = action else {
            panic!("update machine command should send wire text");
        };
        let payload: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(payload["event"], EVENT_UPDATE_MACHINE);
        assert_eq!(payload["data"]["machine"]["screen"], "ScreenGameplay");
        assert_eq!(payload["data"]["machine"]["p1Ready"], true);
        assert_eq!(payload["data"]["machine"]["p2Ready"], false);
        assert_eq!(payload["data"]["machine"]["p1Score"], 98.5);
        assert_eq!(payload["data"]["machine"]["p2Present"], false);
        assert_eq!(
            lobby_command_action(&LobbyCommand::Disconnect, |_, _, _, _, _| {
                serde_json::json!({})
            }),
            LobbyCommandAction::Close
        );
    }

    #[test]
    fn local_lobby_machine_state_uses_session_side_when_no_sides_joined() {
        let value = local_lobby_machine_state_value(
            LocalLobbyPlayer {
                side: PlayerSide::P1,
                display_name: "Alice",
                joined: false,
                screen_name: "ScreenSelectMusic",
                ready: true,
                stats: None,
            },
            LocalLobbyPlayer {
                side: PlayerSide::P2,
                display_name: "Bob",
                joined: false,
                screen_name: "ScreenGameplay",
                ready: false,
                stats: Some(&MachinePlayerStats {
                    score: Some(98.5),
                    ..MachinePlayerStats::default()
                }),
            },
            PlayerSide::P2,
        );

        assert!(value["player1"].is_null());
        assert_eq!(value["player2"]["playerId"], "P2");
        assert_eq!(value["player2"]["profileName"], "[DS] Bob");
        assert_eq!(value["player2"]["screenName"], "ScreenGameplay");
        assert_eq!(value["player2"]["ready"], false);
        assert_eq!(value["player2"]["score"], 98.5);
    }

    #[test]
    fn local_lobby_machine_state_uses_joined_sides_when_present() {
        let value = local_lobby_machine_state_value(
            LocalLobbyPlayer {
                side: PlayerSide::P1,
                display_name: "[DS] Alice",
                joined: true,
                screen_name: "ScreenGameplay",
                ready: true,
                stats: Some(&MachinePlayerStats {
                    ex_score: Some(97.25),
                    ..MachinePlayerStats::default()
                }),
            },
            LocalLobbyPlayer {
                side: PlayerSide::P2,
                display_name: "Bob",
                joined: false,
                screen_name: "ScreenSelectMusic",
                ready: true,
                stats: None,
            },
            PlayerSide::P2,
        );

        assert_eq!(value["player1"]["playerId"], "P1");
        assert_eq!(value["player1"]["profileName"], "[DS] Alice");
        assert_eq!(value["player1"]["screenName"], "ScreenGameplay");
        assert_eq!(value["player1"]["exScore"], 97.25);
        assert!(value["player2"].is_null());
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
