use crate::game::profile;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket, connect};

const LOBBY_SERVICE_URL: &str = "ws://syncservice.groovestats.com:1337";
const SOCKET_POLL_SLEEP: Duration = Duration::from_millis(16);
const SOCKET_PING_INTERVAL: Duration = Duration::from_secs(15);
pub const LOBBY_DISCONNECT_HOLD_SECONDS: f32 = 5.0;
const LOBBY_PROFILE_PREFIX: &str = "[DS] ";

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

#[derive(Debug, Clone, PartialEq)]
pub struct JoinedLobby {
    pub code: String,
    pub players: Vec<LobbyPlayer>,
    pub song_info: Option<LobbySongInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseStatus {
    pub event: String,
    pub success: bool,
    pub message: Option<String>,
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

#[derive(Debug)]
enum Command {
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

static SNAPSHOT: LazyLock<Mutex<Snapshot>> = LazyLock::new(|| Mutex::new(Snapshot::default()));
static COMMAND_TX: LazyLock<Mutex<Option<Sender<Command>>>> = LazyLock::new(|| Mutex::new(None));
static LAST_MACHINE_STATE_SIG: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static RECONNECT_STATE: LazyLock<Mutex<ReconnectState>> =
    LazyLock::new(|| Mutex::new(ReconnectState::default()));

#[derive(Debug, Clone)]
struct ReconnectTarget {
    code: String,
    password: String,
}

#[derive(Debug, Default)]
struct ReconnectState {
    target: Option<ReconnectTarget>,
    pending_create_password: Option<String>,
    retry_attempts: u32,
    next_retry_at: Option<Instant>,
}

pub fn init() {}

pub fn snapshot() -> Snapshot {
    SNAPSHOT.lock().unwrap().clone()
}

pub fn search_lobbies() {
    let _ = send_command(Command::Search);
}

pub fn create_lobby() {
    create_lobby_with_password("");
}

pub fn join_lobby(code: &str) {
    join_lobby_with_password(code, "");
}

pub fn create_lobby_with_password(password: &str) {
    {
        let mut reconnect = RECONNECT_STATE.lock().unwrap();
        reconnect.target = None;
        reconnect.pending_create_password = Some(password.to_string());
        reconnect.retry_attempts = 0;
        reconnect.next_retry_at = None;
    }
    let _ = send_command(Command::Create {
        password: password.to_string(),
    });
}

pub fn join_lobby_with_password(code: &str, password: &str) {
    let code = code.trim();
    if code.is_empty() {
        return;
    }
    {
        let mut reconnect = RECONNECT_STATE.lock().unwrap();
        reconnect.target = Some(ReconnectTarget {
            code: code.to_string(),
            password: password.to_string(),
        });
        reconnect.pending_create_password = None;
        reconnect.retry_attempts = 0;
        reconnect.next_retry_at = None;
    }
    let _ = send_command(Command::Join {
        code: code.to_string(),
        password: password.to_string(),
    });
}

pub fn leave_lobby() {
    clear_reconnect_target();
    if !matches!(snapshot().connection, ConnectionState::Connected) {
        *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
        let mut snapshot = SNAPSHOT.lock().unwrap();
        snapshot.joined_lobby = None;
        snapshot.last_status = None;
        return;
    }
    let _ = send_command(Command::Leave);
}

pub fn update_machine_state(screen_name: &str, ready: bool) {
    update_machine_state_sides(screen_name, ready, ready);
}

pub fn update_machine_state_sides(screen_name: &str, p1_ready: bool, p2_ready: bool) {
    update_machine_state_sides_with_stats(screen_name, p1_ready, p2_ready, None, None);
}

pub fn update_machine_state_sides_with_stats(
    screen_name: &str,
    p1_ready: bool,
    p2_ready: bool,
    p1_stats: Option<MachinePlayerStats>,
    p2_stats: Option<MachinePlayerStats>,
) {
    let joined_code = {
        let snapshot = SNAPSHOT.lock().unwrap();
        if !matches!(snapshot.connection, ConnectionState::Connected) {
            return;
        }
        let Some(joined) = snapshot.joined_lobby.as_ref() else {
            return;
        };
        joined.code.clone()
    };
    let machine_state = local_machine_state_json(
        screen_name,
        p1_ready,
        p2_ready,
        p1_stats.as_ref(),
        p2_stats.as_ref(),
    );
    let sig = format!("{joined_code}|{}", machine_state);
    {
        let last_sig = LAST_MACHINE_STATE_SIG.lock().unwrap();
        if last_sig.as_deref() == Some(sig.as_str()) {
            return;
        }
    }
    if send_command(Command::UpdateMachine {
        screen_name: screen_name.to_string(),
        p1_ready,
        p2_ready,
        p1_stats,
        p2_stats,
    }) {
        *LAST_MACHINE_STATE_SIG.lock().unwrap() = Some(sig);
    }
}

pub fn select_song(song_info: LobbySongInfo) {
    let snapshot = SNAPSHOT.lock().unwrap().clone();
    if !matches!(snapshot.connection, ConnectionState::Connected) || snapshot.joined_lobby.is_none()
    {
        return;
    }
    let _ = send_command(Command::SelectSong { song_info });
}

pub fn disconnect() {
    clear_reconnect_target();
    if let Some(tx) = COMMAND_TX.lock().unwrap().take() {
        let _ = tx.send(Command::Disconnect);
    }
    *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
    let mut snapshot = SNAPSHOT.lock().unwrap();
    snapshot.connection = ConnectionState::Disconnected;
    snapshot.joined_lobby = None;
    snapshot.last_status = None;
}

pub fn poll_reconnect() {
    let target = {
        let snapshot = SNAPSHOT.lock().unwrap().clone();
        if matches!(
            snapshot.connection,
            ConnectionState::Connected | ConnectionState::Connecting
        ) {
            return;
        }
        let mut reconnect = RECONNECT_STATE.lock().unwrap();
        let Some(target) = reconnect.target.clone() else {
            return;
        };
        if reconnect
            .next_retry_at
            .is_some_and(|retry_at| Instant::now() < retry_at)
        {
            return;
        }
        reconnect.next_retry_at = None;
        target
    };

    let _ = send_command(Command::Join {
        code: target.code,
        password: target.password,
    });
}

pub fn reconnect_status_text() -> Option<String> {
    let snapshot = SNAPSHOT.lock().unwrap().clone();
    let reconnect = RECONNECT_STATE.lock().unwrap();
    reconnect.target.as_ref()?;
    match snapshot.connection {
        ConnectionState::Connected => None,
        ConnectionState::Connecting => Some("Reconnecting to lobby...".to_string()),
        ConnectionState::Disconnected | ConnectionState::Error(_) => {
            if let Some(retry_at) = reconnect.next_retry_at {
                let remaining = retry_at.saturating_duration_since(Instant::now());
                let seconds = remaining.as_secs().max(1);
                Some(format!("Connection lost. Retrying in {seconds}s..."))
            } else {
                Some("Connection lost. Retrying...".to_string())
            }
        }
    }
}

fn ensure_worker() {
    let mut tx_slot = COMMAND_TX.lock().unwrap();
    if tx_slot.is_some() {
        return;
    }

    let (tx, rx) = mpsc::channel();
    *tx_slot = Some(tx);
    drop(tx_slot);

    let mut snapshot = SNAPSHOT.lock().unwrap();
    snapshot.connection = ConnectionState::Connecting;
    snapshot.last_status = None;
    drop(snapshot);

    thread::spawn(move || worker_main(rx));
}

fn send_command(command: Command) -> bool {
    ensure_worker();
    let tx_opt = COMMAND_TX.lock().unwrap().clone();
    let Some(tx) = tx_opt else {
        return false;
    };
    if tx.send(command).is_ok() {
        return true;
    }

    COMMAND_TX.lock().unwrap().take();
    false
}

fn clear_reconnect_target() {
    let mut reconnect = RECONNECT_STATE.lock().unwrap();
    reconnect.target = None;
    reconnect.pending_create_password = None;
    reconnect.retry_attempts = 0;
    reconnect.next_retry_at = None;
}

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        0 | 1 => 1,
        2 => 2,
        3 => 3,
        _ => 5,
    })
}

fn schedule_reconnect() {
    let mut reconnect = RECONNECT_STATE.lock().unwrap();
    if reconnect.target.is_none() {
        reconnect.retry_attempts = 0;
        reconnect.next_retry_at = None;
        return;
    }
    reconnect.retry_attempts = reconnect.retry_attempts.saturating_add(1);
    reconnect.next_retry_at = Some(Instant::now() + reconnect_delay(reconnect.retry_attempts));
}

fn should_preserve_joined_lobby() -> bool {
    RECONNECT_STATE.lock().unwrap().target.is_some()
}

fn handle_connection_loss(connection: ConnectionState) {
    let preserve_joined = should_preserve_joined_lobby();
    let mut snapshot = SNAPSHOT.lock().unwrap();
    snapshot.connection = connection;
    if !preserve_joined {
        snapshot.joined_lobby = None;
    }
    *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
    COMMAND_TX.lock().unwrap().take();
    drop(snapshot);
    if preserve_joined {
        schedule_reconnect();
    }
}

fn worker_main(rx: Receiver<Command>) {
    let (mut socket, _) = match connect(LOBBY_SERVICE_URL) {
        Ok(parts) => parts,
        Err(error) => {
            handle_connection_loss(ConnectionState::Error(error.to_string()));
            return;
        }
    };

    if let Err(error) = set_socket_nonblocking(&mut socket) {
        handle_connection_loss(ConnectionState::Error(error.to_string()));
        return;
    }

    {
        let mut snapshot = SNAPSHOT.lock().unwrap();
        snapshot.connection = ConnectionState::Connected;
        snapshot.last_status = None;
    }

    let mut last_ping_at = Instant::now();
    loop {
        while let Ok(command) = rx.try_recv() {
            if handle_command(&mut socket, command).is_err() {
                handle_connection_loss(ConnectionState::Disconnected);
                return;
            }
        }

        match socket.read() {
            Ok(message) => {
                if let Err(error) = handle_message(message) {
                    handle_connection_loss(ConnectionState::Error(error));
                    return;
                }
            }
            Err(tungstenite::Error::Io(error)) if error.kind() == ErrorKind::WouldBlock => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                handle_connection_loss(ConnectionState::Disconnected);
                return;
            }
            Err(error) => {
                handle_connection_loss(ConnectionState::Error(error.to_string()));
                return;
            }
        }

        if last_ping_at.elapsed() >= SOCKET_PING_INTERVAL {
            if let Err(error) = socket.send(Message::Ping(Vec::new().into())) {
                handle_connection_loss(ConnectionState::Error(error.to_string()));
                return;
            }
            last_ping_at = Instant::now();
        }

        thread::sleep(SOCKET_POLL_SLEEP);
    }
}

fn handle_command(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    command: Command,
) -> Result<(), tungstenite::Error> {
    match command {
        Command::Search => send_event(socket, "searchLobby", json!({})),
        Command::Create { password } => send_event(
            socket,
            "createLobby",
            json!({
                "machine": local_machine_state_json("ScreenSelectMusic", true, true, None, None),
                "password": password,
            }),
        ),
        Command::Join { code, password } => send_event(
            socket,
            "joinLobby",
            json!({
                "machine": local_machine_state_json("ScreenSelectMusic", true, true, None, None),
                "code": code,
                "password": password,
            }),
        ),
        Command::Leave => send_event(socket, "leaveLobby", json!({})),
        Command::UpdateMachine {
            screen_name,
            p1_ready,
            p2_ready,
            p1_stats,
            p2_stats,
        } => send_event(
            socket,
            "updateMachine",
            json!({
                "machine": local_machine_state_json(
                    screen_name.as_str(),
                    p1_ready,
                    p2_ready,
                    p1_stats.as_ref(),
                    p2_stats.as_ref(),
                ),
            }),
        ),
        Command::SelectSong { song_info } => send_event(
            socket,
            "selectSong",
            json!({
                "songInfo": song_info,
            }),
        ),
        Command::Disconnect => {
            let _ = socket.close(None);
            Err(tungstenite::Error::ConnectionClosed)
        }
    }
}

fn send_event(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    event: &str,
    data: Value,
) -> Result<(), tungstenite::Error> {
    socket.send(Message::Text(
        json!({
            "event": event,
            "data": data,
        })
        .to_string()
        .into(),
    ))
}

fn local_machine_state_json(
    screen_name: &str,
    p1_ready: bool,
    p2_ready: bool,
    p1_stats: Option<&MachinePlayerStats>,
    p2_stats: Option<&MachinePlayerStats>,
) -> Value {
    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    if !(p1_joined || p2_joined) {
        let side = profile::get_session_player_side();
        return json!({
            "player1": if side == profile::PlayerSide::P1 { local_player_json(profile::PlayerSide::P1, screen_name, p1_ready, p1_stats) } else { Value::Null },
            "player2": if side == profile::PlayerSide::P2 { local_player_json(profile::PlayerSide::P2, screen_name, p2_ready, p2_stats) } else { Value::Null },
        });
    }

    json!({
        "player1": if p1_joined { local_player_json(profile::PlayerSide::P1, screen_name, p1_ready, p1_stats) } else { Value::Null },
        "player2": if p2_joined { local_player_json(profile::PlayerSide::P2, screen_name, p2_ready, p2_stats) } else { Value::Null },
    })
}

fn local_player_json(
    side: profile::PlayerSide,
    screen_name: &str,
    ready: bool,
    stats: Option<&MachinePlayerStats>,
) -> Value {
    let player_id = match side {
        profile::PlayerSide::P1 => "P1",
        profile::PlayerSide::P2 => "P2",
    };
    let profile = profile::get_for_side(side);
    let profile_name = decorate_lobby_profile_name(profile.display_name.as_str());
    json!({
        "playerId": player_id,
        "profileName": profile_name,
        "screenName": screen_name,
        "ready": ready,
        "judgments": stats.and_then(|stats| stats.judgments.clone()),
        "score": stats.and_then(|stats| stats.score),
        "exScore": stats.and_then(|stats| stats.ex_score),
    })
}

fn decorate_lobby_profile_name(name: &str) -> String {
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

fn handle_message(message: Message) -> Result<(), String> {
    let text = match message {
        Message::Text(text) => text,
        Message::Close(_) => return Err("Connection closed.".to_string()),
        _ => return Ok(()),
    };

    let envelope: InboundEnvelope =
        serde_json::from_str(text.as_str()).map_err(|error| error.to_string())?;
    match envelope.event.as_str() {
        "lobbySearched" => {
            let data: LobbySearchedData =
                serde_json::from_value(envelope.data).map_err(|error| error.to_string())?;
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.available_lobbies = data
                .lobbies
                .into_iter()
                .map(|lobby| PublicLobby {
                    code: lobby.code,
                    player_count: lobby.player_count,
                    is_password_protected: lobby.is_password_protected,
                })
                .collect();
        }
        "lobbyState" => {
            let data: LobbyStateData =
                serde_json::from_value(envelope.data).map_err(|error| error.to_string())?;
            {
                let mut reconnect = RECONNECT_STATE.lock().unwrap();
                let password = reconnect
                    .pending_create_password
                    .take()
                    .or_else(|| {
                        reconnect
                            .target
                            .as_ref()
                            .map(|target| target.password.clone())
                    })
                    .unwrap_or_default();
                reconnect.target = Some(ReconnectTarget {
                    code: data.code.clone(),
                    password,
                });
                reconnect.retry_attempts = 0;
                reconnect.next_retry_at = None;
            }
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.joined_lobby = Some(JoinedLobby {
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
            });
        }
        "lobbyLeft" => {
            let data: LobbyLeftData =
                serde_json::from_value(envelope.data).map_err(|error| error.to_string())?;
            if data.left.unwrap_or(true) {
                clear_reconnect_target();
                *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
                let mut snapshot = SNAPSHOT.lock().unwrap();
                snapshot.joined_lobby = None;
            }
        }
        "clientDisconnected" => {
            clear_reconnect_target();
            *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.joined_lobby = None;
        }
        "responseStatus" => {
            let data: ResponseStatusData =
                serde_json::from_value(envelope.data).map_err(|error| error.to_string())?;
            if !data.success && matches!(data.event.as_str(), "joinLobby" | "createLobby") {
                clear_reconnect_target();
                *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
                let mut snapshot = SNAPSHOT.lock().unwrap();
                snapshot.joined_lobby = None;
                snapshot.last_status = Some(ResponseStatus {
                    event: data.event,
                    success: data.success,
                    message: data.message,
                });
                return Ok(());
            }
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.last_status = Some(ResponseStatus {
                event: data.event,
                success: data.success,
                message: data.message,
            });
        }
        _ => {}
    }
    Ok(())
}

fn set_socket_nonblocking(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> Result<(), std::io::Error> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_nonblocking(true),
        _ => Ok(()),
    }
}

#[derive(Debug, Deserialize)]
struct InboundEnvelope {
    event: String,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicLobbyData {
    code: String,
    player_count: usize,
    #[serde(default)]
    is_password_protected: bool,
}

#[derive(Debug, Default, Deserialize)]
struct LobbySearchedData {
    #[serde(default)]
    lobbies: Vec<PublicLobbyData>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LobbyStatePlayerData {
    profile_name: Option<String>,
    name: Option<String>,
    player_id: Option<String>,
    screen_name: Option<String>,
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    judgments: Option<LobbyJudgments>,
    #[serde(default)]
    score: Option<f32>,
    #[serde(rename = "exScore", default)]
    ex_score: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct LobbyStateData {
    #[serde(default)]
    code: String,
    #[serde(default)]
    players: Vec<LobbyStatePlayerData>,
    #[serde(default)]
    song_info: Option<LobbySongInfo>,
}

#[derive(Debug, Default, Deserialize)]
struct LobbyLeftData {
    left: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ResponseStatusData {
    event: String,
    success: bool,
    message: Option<String>,
}
