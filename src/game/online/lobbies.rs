use crate::game::profile;
use deadsync_online::lobbies::{
    ConnectionState, LobbyInboundEffect, LobbyMachinePlayer, LobbySocket, LobbySocketError,
    LobbySongInfo, MachinePlayerStats, Snapshot, close_lobby_socket, connect_lobby_socket,
    create_lobby_text, flush_lobby_socket, is_transient_lobby_socket_error, join_lobby_text,
    joined_lobby_from_state, leave_lobby_text, lobby_left_clears_joined, lobby_machine_player,
    lobby_machine_state_value, normalize_lobby_password, parse_inbound_text,
    public_lobbies_from_search, read_lobby_text, response_status_clears_joined,
    response_status_from_data, search_lobby_text, select_song_text, send_lobby_ping,
    send_lobby_text, update_machine_text,
};
use deadsync_profile as profile_data;
use log::{debug, warn};
use serde_json::Value;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const SOCKET_POLL_SLEEP: Duration = Duration::from_millis(16);
const SOCKET_PING_INTERVAL: Duration = Duration::from_secs(15);
pub const LOBBY_DISCONNECT_HOLD_SECONDS: f32 = 5.0;

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

pub fn can_update_machine_state() -> bool {
    let snapshot = SNAPSHOT.lock().unwrap();
    matches!(snapshot.connection, ConnectionState::Connected) && snapshot.joined_lobby.is_some()
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
    let password = normalize_lobby_password(password);
    {
        let mut reconnect = RECONNECT_STATE.lock().unwrap();
        reconnect.target = None;
        reconnect.pending_create_password = Some(password.clone());
        reconnect.retry_attempts = 0;
        reconnect.next_retry_at = None;
    }
    let _ = send_command(Command::Create { password });
}

pub fn join_lobby_with_password(code: &str, password: &str) {
    let code = code.trim();
    if code.is_empty() {
        return;
    }
    let password = normalize_lobby_password(password);
    {
        let mut reconnect = RECONNECT_STATE.lock().unwrap();
        reconnect.target = Some(ReconnectTarget {
            code: code.to_string(),
            password: password.clone(),
        });
        reconnect.pending_create_password = None;
        reconnect.retry_attempts = 0;
        reconnect.next_retry_at = None;
    }
    let _ = send_command(Command::Join {
        code: code.to_string(),
        password,
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
    let mut socket = match connect_lobby_socket() {
        Ok(socket) => socket,
        Err(error) => {
            handle_connection_loss(ConnectionState::Error(error.to_string()));
            return;
        }
    };

    {
        let mut snapshot = SNAPSHOT.lock().unwrap();
        snapshot.connection = ConnectionState::Connected;
        snapshot.last_status = None;
    }

    let mut last_ping_at = Instant::now();
    loop {
        while let Ok(command) = rx.try_recv() {
            if let Err(error) = handle_command(&mut socket, command) {
                if !is_transient_lobby_socket_error(&error) {
                    handle_connection_loss(connection_state_from_socket_error(&error));
                    return;
                }
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
                handle_connection_loss(connection_state_from_socket_error(&error));
                return;
            }
        }

        if last_ping_at.elapsed() >= SOCKET_PING_INTERVAL {
            if let Err(error) = send_lobby_ping(&mut socket) {
                if !is_transient_lobby_socket_error(&error) {
                    handle_connection_loss(connection_state_from_socket_error(&error));
                    return;
                }
            }
            last_ping_at = Instant::now();
        }

        if let Err(error) = flush_lobby_socket(&mut socket)
            && !is_transient_lobby_socket_error(&error)
        {
            handle_connection_loss(connection_state_from_socket_error(&error));
            return;
        }

        thread::sleep(SOCKET_POLL_SLEEP);
    }
}

fn handle_command(socket: &mut LobbySocket, command: Command) -> Result<(), LobbySocketError> {
    match command {
        Command::Search => send_lobby_text(socket, search_lobby_text()),
        Command::Create { password } => send_lobby_text(
            socket,
            create_lobby_text(
                &local_machine_state_json("ScreenSelectMusic", true, true, None, None),
                password.as_str(),
            ),
        ),
        Command::Join { code, password } => send_lobby_text(
            socket,
            join_lobby_text(
                &local_machine_state_json("ScreenSelectMusic", true, true, None, None),
                code.as_str(),
                password.as_str(),
            ),
        ),
        Command::Leave => send_lobby_text(socket, leave_lobby_text()),
        Command::UpdateMachine {
            screen_name,
            p1_ready,
            p2_ready,
            p1_stats,
            p2_stats,
        } => {
            let machine = local_machine_state_json(
                screen_name.as_str(),
                p1_ready,
                p2_ready,
                p1_stats.as_ref(),
                p2_stats.as_ref(),
            );
            send_lobby_text(socket, update_machine_text(&machine))
        }
        Command::SelectSong { song_info } => send_lobby_text(socket, select_song_text(&song_info)),
        Command::Disconnect => {
            close_lobby_socket(socket);
            Err(LobbySocketError::Closed)
        }
    }
}

fn connection_state_from_socket_error(error: &LobbySocketError) -> ConnectionState {
    match error {
        LobbySocketError::Closed => ConnectionState::Disconnected,
        _ => ConnectionState::Error(error.to_string()),
    }
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
    if !(p1_joined || p2_joined) {
        let side = profile::get_session_player_side();
        return lobby_machine_state_value(
            (side == profile_data::PlayerSide::P1).then(|| {
                local_player(
                    profile_data::PlayerSide::P1,
                    screen_name,
                    p1_ready,
                    p1_stats,
                )
            }),
            (side == profile_data::PlayerSide::P2).then(|| {
                local_player(
                    profile_data::PlayerSide::P2,
                    screen_name,
                    p2_ready,
                    p2_stats,
                )
            }),
        );
    }

    lobby_machine_state_value(
        p1_joined.then(|| {
            local_player(
                profile_data::PlayerSide::P1,
                screen_name,
                p1_ready,
                p1_stats,
            )
        }),
        p2_joined.then(|| {
            local_player(
                profile_data::PlayerSide::P2,
                screen_name,
                p2_ready,
                p2_stats,
            )
        }),
    )
}

fn local_player(
    side: profile_data::PlayerSide,
    screen_name: &str,
    ready: bool,
    stats: Option<&MachinePlayerStats>,
) -> LobbyMachinePlayer {
    let player_id = match side {
        profile_data::PlayerSide::P1 => "P1",
        profile_data::PlayerSide::P2 => "P2",
    };
    let profile = profile::get_for_side(side);
    lobby_machine_player(
        player_id,
        profile.display_name.as_str(),
        screen_name,
        ready,
        stats,
    )
}

fn handle_text_message(text: &str) -> Result<(), String> {
    let effect = match parse_inbound_text(text) {
        Ok(effect) => effect,
        Err(error) => {
            log_malformed_payload(error.event.as_deref(), error.error.as_str(), text);
            return Ok(());
        }
    };

    match effect {
        LobbyInboundEffect::Ignore => {}
        LobbyInboundEffect::LobbySearched(data) => {
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.available_lobbies = public_lobbies_from_search(data);
        }
        LobbyInboundEffect::LobbyState(data) => {
            {
                let mut reconnect = RECONNECT_STATE.lock().unwrap();
                let code = data.code.clone();
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
                reconnect.target = Some(ReconnectTarget { code, password });
                reconnect.retry_attempts = 0;
                reconnect.next_retry_at = None;
            }
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.joined_lobby = Some(joined_lobby_from_state(data));
        }
        LobbyInboundEffect::LobbyLeft(data) => {
            if lobby_left_clears_joined(&data) {
                clear_reconnect_target();
                *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
                let mut snapshot = SNAPSHOT.lock().unwrap();
                snapshot.joined_lobby = None;
            }
        }
        LobbyInboundEffect::ClientDisconnected => {
            clear_reconnect_target();
            *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.joined_lobby = None;
        }
        LobbyInboundEffect::ResponseStatus(data) => {
            let clears_lobby = response_status_clears_joined(&data);
            let status = response_status_from_data(data);
            if clears_lobby {
                clear_reconnect_target();
                *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
                let mut snapshot = SNAPSHOT.lock().unwrap();
                snapshot.joined_lobby = None;
                snapshot.last_status = Some(status);
                return Ok(());
            }
            let mut snapshot = SNAPSHOT.lock().unwrap();
            snapshot.last_status = Some(status);
        }
    }
    Ok(())
}

fn log_malformed_payload(event: Option<&str>, error: &str, raw_text: &str) {
    match event {
        Some(event) => warn!("Ignoring malformed lobby payload for event '{event}': {error}"),
        None => warn!("Ignoring malformed lobby payload: {error}"),
    }
    debug!("Malformed lobby payload: {raw_text}");
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND_TX, ConnectionState, LAST_MACHINE_STATE_SIG, RECONNECT_STATE, ReconnectState,
        SNAPSHOT, Snapshot, handle_text_message,
    };
    use deadsync_online::lobbies::{JoinedLobby, LobbyPlayer};
    use std::sync::{LazyLock, Mutex};

    static TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn reset_test_state() {
        *SNAPSHOT.lock().unwrap() = Snapshot::default();
        *LAST_MACHINE_STATE_SIG.lock().unwrap() = None;
        *RECONNECT_STATE.lock().unwrap() = ReconnectState::default();
        COMMAND_TX.lock().unwrap().take();
    }

    #[test]
    fn malformed_lobby_state_payload_is_ignored() {
        let _guard = TEST_MUTEX.lock().unwrap();
        reset_test_state();

        let existing_player = LobbyPlayer {
            label: "Existing".to_string(),
            ready: true,
            screen_name: "ScreenGameplay".to_string(),
            judgments: None,
            score: Some(98.5),
            ex_score: Some(97.25),
        };
        {
            let mut snapshot = SNAPSHOT.lock().unwrap();
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

        assert!(handle_text_message(malformed).is_ok());

        let snapshot = SNAPSHOT.lock().unwrap().clone();
        assert_eq!(snapshot.connection, ConnectionState::Connected);
        assert_eq!(
            snapshot.joined_lobby,
            Some(JoinedLobby {
                code: "ABCD".to_string(),
                players: vec![existing_player],
                song_info: None,
            })
        );

        reset_test_state();
    }
}
