use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::OnlineRequestError;
use deadsync_net::{self as network, NetworkError};
use deadsync_score::{
    GrooveStatsSubmitRecordBanner, GrooveStatsSubmitUiStatus, GsExEvidence, LeaderboardEntry,
    LeaderboardPane, PlayerLeaderboardData, RejectReason, groovestats_submit_record_banner,
    leaderboard_nonzero_rank, leaderboard_pane, leaderboard_score_10000,
    leaderboard_username_matches,
};

const GROOVESTATS_API_BASE_URL: &str = "https://api.groovestats.com";
const BOOGIESTATS_API_BASE_URL: &str = "https://boogiestats.andr.host";
const GROOVESTATS_QR_BASE_URL: &str = "https://www.groovestats.com";
const GROOVESTATS_QR_LOGIN_WS_URL: &str = "ws://qrlogin.groovestats.com:3000";
const GROOVESTATS_QR_LOGIN_URL: &str = "https://www.groovestats.com/qrlogin.php";
const GROOVESTATS_NEW_SESSION_PATH: &str = "new-session.php?chartHashVersion=3";
pub const GROOVESTATS_QR_LOGIN_WS_READ_TIMEOUT_MS: u64 = 100;
pub const GROOVESTATS_CHART_HASH_VERSION: u8 = 3;
pub const GROOVESTATS_COMMENT_PREFIX: &str = "[DS]";
pub const GROOVESTATS_SUBMIT_MAX_ENTRIES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    GrooveStats,
    BoogieStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Services {
    pub get_scores: bool,
    pub leaderboard: bool,
    pub auto_submit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionError {
    Disabled,
    MachineOffline,
    CannotConnect,
    TimedOut,
    InvalidResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Pending,
    Connected(Services),
    Error(ConnectionError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionProbeError {
    Timeout,
    InvalidResponse(String),
    CannotConnect(String),
}

impl std::fmt::Display for ConnectionProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => f.write_str("request timed out"),
            Self::InvalidResponse(message) | Self::CannotConnect(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConnectionProbeError {}

impl ConnectionProbeError {
    #[inline(always)]
    pub const fn connection_error(&self) -> ConnectionError {
        match self {
            Self::Timeout => ConnectionError::TimedOut,
            Self::InvalidResponse(_) => ConnectionError::InvalidResponse,
            Self::CannotConnect(_) => ConnectionError::CannotConnect,
        }
    }
}

impl Service {
    #[inline(always)]
    pub const fn from_boogiestats_active(active: bool) -> Self {
        if active {
            Self::BoogieStats
        } else {
            Self::GrooveStats
        }
    }
}

#[inline(always)]
pub const fn service_name(service: Service) -> &'static str {
    match service {
        Service::GrooveStats => "GrooveStats",
        Service::BoogieStats => "BoogieStats",
    }
}

#[inline(always)]
pub const fn primary_api_base_url() -> &'static str {
    GROOVESTATS_API_BASE_URL
}

#[inline(always)]
pub const fn boogiestats_api_base_url() -> &'static str {
    BOOGIESTATS_API_BASE_URL
}

#[inline(always)]
pub const fn api_base_url(service: Service) -> &'static str {
    match service {
        Service::GrooveStats => GROOVESTATS_API_BASE_URL,
        Service::BoogieStats => BOOGIESTATS_API_BASE_URL,
    }
}

#[inline(always)]
pub const fn qr_base_url() -> &'static str {
    GROOVESTATS_QR_BASE_URL
}

#[inline(always)]
pub const fn qr_login_ws_url() -> &'static str {
    GROOVESTATS_QR_LOGIN_WS_URL
}

/// 32-character uppercase hex string mirroring Simply Love's
/// `CRYPTMAN:GenerateRandomUUID():gsub("-",""):upper()`.
pub fn generate_qr_login_uuid() -> String {
    use rand::Rng;

    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let mut out = String::with_capacity(32);
    for b in bytes {
        out.push_str(&format!("{b:02X}"));
    }
    out
}

pub fn qr_login_url(uuid: &str, side: u8) -> String {
    format!("{GROOVESTATS_QR_LOGIN_URL}?UUID={uuid}&SIDE={side}")
}

pub fn qr_login_uuid_message(uuid: &str) -> String {
    serde_json::json!({ "event": "uuid", "data": { "uuid": uuid } }).to_string()
}

#[inline(always)]
pub fn player_leaderboards_url(service: Service) -> String {
    format!(
        "{}/player-leaderboards.php",
        api_base_url(service).trim_end_matches('/')
    )
}

#[inline(always)]
pub fn score_submit_url(service: Service) -> String {
    format!(
        "{}/score-submit.php",
        api_base_url(service).trim_end_matches('/')
    )
}

#[inline(always)]
pub fn new_session_url(service: Service) -> String {
    format!(
        "{}/{}",
        api_base_url(service).trim_end_matches('/'),
        GROOVESTATS_NEW_SESSION_PATH
    )
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionServices {
    pub player_scores: bool,
    pub player_leaderboards: bool,
    pub score_submit: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionResponse {
    pub services_allowed: NewSessionServices,
    pub services_result: String,
}

#[inline(always)]
pub const fn services_from_new_session(data: &NewSessionResponse) -> Services {
    Services {
        get_scores: data.services_allowed.player_scores,
        leaderboard: data.services_allowed.player_leaderboards,
        auto_submit: data.services_allowed.score_submit,
    }
}

pub fn connection_status_from_new_session(data: &NewSessionResponse) -> ConnectionStatus {
    if !data.services_result.eq_ignore_ascii_case("OK") {
        return ConnectionStatus::Error(ConnectionError::MachineOffline);
    }
    ConnectionStatus::Connected(services_from_new_session(data))
}

#[inline(always)]
pub const fn connection_error_from_network_error(error: &NetworkError) -> ConnectionError {
    match error {
        NetworkError::Timeout => ConnectionError::TimedOut,
        NetworkError::Decode(_) => ConnectionError::InvalidResponse,
        NetworkError::HttpStatus(_) | NetworkError::Request(_) => ConnectionError::CannotConnect,
    }
}

pub fn check_connection(service: Service) -> Result<ConnectionStatus, NetworkError> {
    let data = network::get_json_with::<NewSessionResponse>(
        &network::get_groovestats_agent(),
        &new_session_url(service),
    )?;
    Ok(connection_status_from_new_session(&data))
}

pub fn probe_connection(service: Service) -> Result<ConnectionStatus, ConnectionProbeError> {
    check_connection(service).map_err(ConnectionProbeError::from)
}

impl From<NetworkError> for ConnectionProbeError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::Timeout => Self::Timeout,
            NetworkError::Decode(message) => Self::InvalidResponse(message),
            other => Self::CannotConnect(other.to_string()),
        }
    }
}

pub fn fetch_player_leaderboards(
    service: Service,
    api_key: &str,
    chart_hash: &str,
    max_entries: Option<usize>,
) -> Result<LeaderboardsApiResponse, OnlineRequestError> {
    let api_url = player_leaderboards_url(service);
    let mut request = network::get_groovestats_agent()
        .get(&api_url)
        .header("x-api-key-player-1", api_key)
        .query("chartHashP1", chart_hash);
    if let Some(max_entries) = max_entries {
        let max_entries_str = max_entries.max(1).to_string();
        request = request.query("maxLeaderboardResults", &max_entries_str);
    }

    let response = request
        .call()
        .map_err(network::error_from_ureq)
        .map_err(OnlineRequestError::from)?;
    if response.status().as_u16() != 200 {
        return Err(OnlineRequestError::HttpStatus(response.status().as_u16()));
    }
    network::read_json_body(response).map_err(OnlineRequestError::from)
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardsApiResponse {
    pub player1: Option<LeaderboardApiPlayer>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardApiPlayer {
    #[serde(default)]
    pub is_ranked: bool,
    #[serde(rename = "gsLeaderboard", default)]
    pub gs_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(rename = "exLeaderboard", default)]
    pub ex_leaderboard: Vec<LeaderboardApiEntry>,
    pub rpg: Option<LeaderboardEventData>,
    pub itl: Option<LeaderboardEventData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEventData {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "rpgLeaderboard", default)]
    pub rpg_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(rename = "itlLeaderboard", default)]
    pub itl_leaderboard: Vec<LeaderboardApiEntry>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardApiEntry {
    #[serde(default)]
    pub rank: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub machine_tag: Option<String>,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub is_rival: bool,
    #[serde(default)]
    pub is_self: bool,
    #[serde(default)]
    pub is_fail: bool,
    #[serde(default)]
    pub comments: Option<String>,
}

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

pub fn leaderboard_pane_from_api(
    name: &str,
    entries: Vec<LeaderboardApiEntry>,
    is_ex: bool,
) -> Option<LeaderboardPane> {
    leaderboard_pane(name, leaderboard_entries_from_api(entries), is_ex)
}

pub fn leaderboard_self_entry<'a>(
    entries: &'a [LeaderboardApiEntry],
    username: &str,
) -> Option<&'a LeaderboardApiEntry> {
    entries.iter().find(|entry| entry.is_self).or_else(|| {
        entries
            .iter()
            .find(|entry| leaderboard_username_matches(entry.name.as_str(), username))
    })
}

#[inline(always)]
pub fn leaderboard_entry_score_10000(entry: &LeaderboardApiEntry) -> Option<f64> {
    leaderboard_score_10000(entry.score, entry.is_fail)
}

pub fn leaderboard_self_score_10000(
    entries: &[LeaderboardApiEntry],
    username: &str,
) -> Option<u32> {
    Some(leaderboard_entry_score_10000(leaderboard_self_entry(entries, username)?)?.round() as u32)
}

pub fn leaderboard_self_rank(entries: &[LeaderboardApiEntry], username: &str) -> Option<u32> {
    leaderboard_nonzero_rank(leaderboard_self_entry(entries, username)?.rank)
}

pub fn gs_ex_evidence_from_leaderboard(
    ex_entries: &[LeaderboardApiEntry],
    username: &str,
    comment: Option<&str>,
) -> GsExEvidence {
    GsExEvidence::from_sources(
        leaderboard_self_entry(ex_entries, username).and_then(leaderboard_entry_score_10000),
        comment,
    )
}

#[derive(Debug)]
pub struct FetchedPlayerLeaderboards {
    pub data: PlayerLeaderboardData,
    pub gs_entries: Vec<LeaderboardApiEntry>,
    pub ex_entries: Vec<LeaderboardApiEntry>,
    pub itl_self_found: bool,
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
    let mut gs_entries = Vec::new();
    let mut ex_entries = Vec::new();
    let mut itl_self_score = None;
    let mut itl_self_rank = None;
    let mut itl_self_found = false;
    if let Some(player) = decoded.player1 {
        let LeaderboardApiPlayer {
            is_ranked: _is_ranked,
            gs_leaderboard,
            ex_leaderboard,
            rpg,
            itl,
        } = player;

        gs_entries.clone_from(&gs_leaderboard);
        ex_entries.clone_from(&ex_leaderboard);
        if show_ex_score {
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
        } else {
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
        }

        if let Some(rpg) = rpg
            && !rpg.rpg_leaderboard.is_empty()
        {
            let name = if rpg.name.trim().is_empty() {
                "RPG"
            } else {
                rpg.name.as_str()
            };
            push_leaderboard_pane(&mut panes, name, rpg.rpg_leaderboard, false);
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
            itl_self_score,
            itl_self_rank,
        },
        gs_entries,
        ex_entries,
        itl_self_found,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsJudgmentCounts {
    pub fantastic_plus: u32,
    pub fantastic: u32,
    pub excellent: u32,
    pub great: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub way_off: Option<u32>,
    pub miss: u32,
    pub total_steps: u32,
    pub holds_held: u32,
    pub total_holds: u32,
    pub mines_hit: u32,
    pub total_mines: u32,
    pub rolls_held: u32,
    pub total_rolls: u32,
}

impl GrooveStatsJudgmentCounts {
    #[inline(always)]
    const fn optional_count(count: Option<u32>) -> u32 {
        match count {
            Some(count) => count,
            None => 0,
        }
    }

    #[inline(always)]
    pub const fn decent_count(&self) -> u32 {
        Self::optional_count(self.decent)
    }

    #[inline(always)]
    pub const fn way_off_count(&self) -> u32 {
        Self::optional_count(self.way_off)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsRescoreCounts {
    pub fantastic_plus: u32,
    pub fantastic: u32,
    pub excellent: u32,
    pub great: u32,
    pub decent: u32,
    pub way_off: u32,
}

#[inline(always)]
fn qr_append_rescore(out: &mut String, label: char, value: u32) {
    if value == 0 {
        return;
    }
    out.push(label);
    out.push_str(format!("{value:x}").as_str());
}

pub fn manual_qr_url(
    base_url: &str,
    chart_hash: &str,
    counts: &GrooveStatsJudgmentCounts,
    rescored: &GrooveStatsRescoreCounts,
    rate: u32,
    used_cmod: bool,
) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }

    let mut rescored_str = String::with_capacity(24);
    for (label, value) in [
        ('G', rescored.fantastic_plus),
        ('H', rescored.fantastic),
        ('I', rescored.excellent),
        ('J', rescored.great),
        ('K', rescored.decent),
        ('L', rescored.way_off),
    ] {
        qr_append_rescore(&mut rescored_str, label, value);
    }

    Some(format!(
        "{}/QR/{hash}/T{:x}G{:x}H{:x}I{:x}J{:x}K{:x}L{:x}M{:x}H{:x}T{:x}R{:x}T{:x}M{:x}T{:x}{rescored_str}/F0R{:x}C{}V{:x}",
        base_url.trim_end_matches('/'),
        counts.total_steps,
        counts.fantastic_plus,
        counts.fantastic,
        counts.excellent,
        counts.great,
        counts.decent_count(),
        counts.way_off_count(),
        counts.miss,
        counts.holds_held,
        counts.total_holds,
        counts.rolls_held,
        counts.total_rolls,
        counts.mines_hit,
        counts.total_mines,
        rate,
        if used_cmod { '1' } else { '0' },
        GROOVESTATS_CHART_HASH_VERSION,
    ))
}

/// Effect to dispatch when a raw GrooveStats QR-login WebSocket text
/// frame arrives.  The caller owns transport and channel routing.
#[derive(Debug, PartialEq, Eq)]
pub enum GrooveStatsQrLoginWsEffect {
    Ignore,
    DeliverApiKey {
        side: u8,
        api_key: String,
        username: String,
    },
}

#[derive(Deserialize, Debug)]
struct GrooveStatsWsEnvelope {
    event: String,
    #[serde(default)]
    data: Option<GrooveStatsApiKeyPayload>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsApiKeyPayload {
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    side: Option<u8>,
}

pub fn classify_qr_login_ws_message(text: &str, expected_uuid: &str) -> GrooveStatsQrLoginWsEffect {
    let Ok(env) = serde_json::from_str::<GrooveStatsWsEnvelope>(text) else {
        return GrooveStatsQrLoginWsEffect::Ignore;
    };
    if env.event != "apiKey" {
        return GrooveStatsQrLoginWsEffect::Ignore;
    }
    let Some(data) = env.data else {
        return GrooveStatsQrLoginWsEffect::Ignore;
    };
    if data.uuid.as_deref() != Some(expected_uuid) {
        return GrooveStatsQrLoginWsEffect::Ignore;
    }
    let api_key = data.api_key.unwrap_or_default();
    if api_key.trim().is_empty() {
        return GrooveStatsQrLoginWsEffect::Ignore;
    }
    let Some(side @ (1 | 2)) = data.side else {
        return GrooveStatsQrLoginWsEffect::Ignore;
    };
    GrooveStatsQrLoginWsEffect::DeliverApiKey {
        side,
        api_key,
        username: data.username.unwrap_or_default(),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GrooveStatsQrLoginEvent {
    Failed {
        reason: String,
    },
    Consumed {
        side: u8,
        api_key: String,
        username: String,
    },
}

/// WebSocket worker body for GrooveStats QR-login. The caller owns the
/// thread, cancellation flag, and mapping transport events to UI state.
pub fn run_qr_login_session<F>(uuid: String, cancel: Arc<AtomicBool>, mut dispatch: F)
where
    F: FnMut(GrooveStatsQrLoginEvent),
{
    use tungstenite::Message;
    use tungstenite::stream::MaybeTlsStream;

    if cancel.load(Ordering::Relaxed) {
        return;
    }

    let mut socket = match tungstenite::connect(qr_login_ws_url()) {
        Ok((sock, _resp)) => sock,
        Err(err) => {
            dispatch(GrooveStatsQrLoginEvent::Failed {
                reason: format!("{err}"),
            });
            return;
        }
    };

    // Plaintext ws://; the maybe-tls stream is the plain branch. Set a
    // short read timeout so the loop can poll the cancel flag.
    if let MaybeTlsStream::Plain(tcp) = socket.get_mut() {
        let _ = tcp.set_read_timeout(Some(std::time::Duration::from_millis(
            GROOVESTATS_QR_LOGIN_WS_READ_TIMEOUT_MS,
        )));
    }

    if socket
        .send(Message::Text(qr_login_uuid_message(&uuid).into()))
        .is_err()
    {
        return;
    }

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = socket.close(None);
            return;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let GrooveStatsQrLoginWsEffect::DeliverApiKey {
                    side,
                    api_key,
                    username,
                } = classify_qr_login_ws_message(&text, &uuid)
                {
                    dispatch(GrooveStatsQrLoginEvent::Consumed {
                        side,
                        api_key,
                        username,
                    });
                }
            }
            Ok(Message::Close(_)) => {
                let _ = socket.close(None);
                return;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(io))
                if matches!(
                    io.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut,
                ) => {}
            Err(_) => {
                let _ = socket.close(None);
                return;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitPlayerPayload {
    pub rate: u32,
    pub score: u32,
    pub judgment_counts: GrooveStatsJudgmentCounts,
    pub rescore_counts: GrooveStatsRescoreCounts,
    pub used_cmod: bool,
    pub comment: String,
    pub player_options: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiResponse {
    #[serde(default)]
    pub error: String,
    pub player1: Option<GrooveStatsSubmitApiPlayer>,
    pub player2: Option<GrooveStatsSubmitApiPlayer>,
}

impl GrooveStatsSubmitApiResponse {
    #[inline(always)]
    pub fn player_for_slot(&self, slot: u8) -> Option<&GrooveStatsSubmitApiPlayer> {
        match slot {
            1 => self.player1.as_ref(),
            2 => self.player2.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct GrooveStatsSubmitRequestSuccess {
    pub response: GrooveStatsSubmitApiResponse,
    pub body_snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrooveStatsSubmitRequestError {
    Transport { message: String, timed_out: bool },
    Http { status: u16, body_snippet: String },
    Decode { message: String },
    Api { message: String },
}

pub fn submit_error_status_and_message(
    service_name: &str,
    error: &GrooveStatsSubmitRequestError,
) -> (GrooveStatsSubmitUiStatus, String) {
    match error {
        GrooveStatsSubmitRequestError::Transport { message, timed_out } => (
            if *timed_out {
                GrooveStatsSubmitUiStatus::TimedOut
            } else {
                GrooveStatsSubmitUiStatus::NetworkError
            },
            message.clone(),
        ),
        GrooveStatsSubmitRequestError::Http {
            status,
            body_snippet,
        } => {
            let message = if body_snippet.is_empty() {
                format!("{service_name} submit returned HTTP {status}")
            } else {
                format!("{service_name} submit returned HTTP {status}: {body_snippet}")
            };
            (
                GrooveStatsSubmitUiStatus::from_http_status(*status),
                message,
            )
        }
        GrooveStatsSubmitRequestError::Decode { message } => (
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            },
            format!("failed to parse {service_name} submit response: {message}"),
        ),
        GrooveStatsSubmitRequestError::Api { message } => (
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            },
            format!("{service_name} submit error: {message}"),
        ),
    }
}

pub fn submit_record_banner_from_api(
    response: &GrooveStatsSubmitApiPlayer,
    username: &str,
    show_ex_score: bool,
) -> Option<GrooveStatsSubmitRecordBanner> {
    groovestats_submit_record_banner(
        response.result.as_str(),
        show_ex_score,
        leaderboard_self_rank(response.gs_leaderboard.as_slice(), username),
        leaderboard_self_rank(response.ex_leaderboard.as_slice(), username),
        !response.ex_leaderboard.is_empty(),
    )
}

pub fn submit_score_request(
    service: Service,
    headers: &[(String, String)],
    query: &[(String, String)],
    body: &serde_json::Value,
) -> Result<GrooveStatsSubmitRequestSuccess, GrooveStatsSubmitRequestError> {
    let mut request = network::get_groovestats_agent()
        .post(&score_submit_url(service))
        .header("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.header(name, value);
    }
    for (name, value) in query {
        request = request.query(name, value);
    }

    let response = request.send_json(body).map_err(|error| {
        let message = format!("network error: {error}");
        GrooveStatsSubmitRequestError::Transport {
            timed_out: network::is_timeout_message(message.as_str()),
            message,
        }
    })?;

    let status = response.status();
    let status_code = status.as_u16();
    let body_text = network::read_text_body_or_empty(response);
    let body_snippet = network::log_body_snippet(body_text.as_str());
    if !status.is_success() {
        return Err(GrooveStatsSubmitRequestError::Http {
            status: status_code,
            body_snippet,
        });
    }

    let response: GrooveStatsSubmitApiResponse =
        serde_json::from_str(body_text.as_str()).map_err(|error| {
            GrooveStatsSubmitRequestError::Decode {
                message: network::log_body_snippet(error.to_string().as_str()),
            }
        })?;
    if !response.error.trim().is_empty() {
        return Err(GrooveStatsSubmitRequestError::Api {
            message: response.error.trim().to_string(),
        });
    }

    Ok(GrooveStatsSubmitRequestSuccess {
        response,
        body_snippet,
    })
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiPlayer {
    #[serde(default)]
    pub chart_hash: String,
    #[serde(default)]
    pub result: String,
    #[serde(rename = "gsLeaderboard", default)]
    pub gs_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(rename = "exLeaderboard", default)]
    pub ex_leaderboard: Vec<LeaderboardApiEntry>,
    pub rpg: Option<GrooveStatsSubmitApiEvent>,
    pub itl: Option<GrooveStatsSubmitApiEvent>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiEvent {
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    pub score_delta: i32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub top_score_points: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub prev_top_score_points: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub total_passes: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub current_ranking_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub previous_ranking_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub current_song_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub previous_song_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub current_ex_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub previous_ex_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub current_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub previous_point_total: u32,
    #[serde(rename = "itlLeaderboard", default)]
    pub itl_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(default)]
    pub is_doubles: bool,
    pub progress: Option<GrooveStatsSubmitApiProgress>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiProgress {
    #[serde(rename = "statImprovements", default)]
    pub stat_improvements: Vec<GrooveStatsSubmitApiStatImprovement>,
    #[serde(rename = "questsCompleted", default)]
    pub quests_completed: Vec<GrooveStatsSubmitApiQuest>,
    #[serde(rename = "achievementsCompleted", default)]
    pub achievements_completed: Vec<GrooveStatsSubmitApiAchievement>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiStatImprovement {
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub gained: u32,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    pub current: i32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiQuest {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub rewards: Vec<GrooveStatsSubmitApiQuestReward>,
    #[serde(default)]
    pub song_download_url: String,
    #[serde(rename = "songDownloadFolders", default)]
    pub song_download_folders: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiQuestReward {
    #[serde(rename = "type", default)]
    pub reward_type: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiAchievement {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub rewards: Vec<GrooveStatsSubmitApiAchievementReward>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiAchievementReward {
    #[serde(default, deserialize_with = "de_string_from_string_or_number")]
    pub tier: String,
    #[serde(default)]
    pub requirements: Vec<String>,
    #[serde(rename = "titleUnlocked", default)]
    pub title_unlocked: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum U32OrString {
    U32(u32),
    F64(f64),
    String(String),
}

fn de_u32_from_string_or_number<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<U32OrString>::deserialize(deserializer)? {
        Some(U32OrString::U32(v)) => Ok(v),
        Some(U32OrString::F64(v)) => Ok(v.max(0.0).floor() as u32),
        Some(U32OrString::String(text)) => Ok(text.trim().parse::<u32>().unwrap_or(0)),
        None => Ok(0),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum I32OrString {
    I32(i32),
    I64(i64),
    F64(f64),
    String(String),
}

fn de_i32_from_string_or_number<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<I32OrString>::deserialize(deserializer)? {
        Some(I32OrString::I32(v)) => Ok(v),
        Some(I32OrString::I64(v)) => Ok(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
        Some(I32OrString::F64(v)) => {
            Ok(v.clamp(f64::from(i32::MIN), f64::from(i32::MAX)).round() as i32)
        }
        Some(I32OrString::String(text)) => Ok(text.trim().parse::<i32>().unwrap_or(0)),
        None => Ok(0),
    }
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum StringOrNumber {
    String(String),
    I64(i64),
    U64(u64),
    F64(f64),
}

fn de_string_from_string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<StringOrNumber>::deserialize(deserializer)? {
        Some(StringOrNumber::String(text)) => Ok(text),
        Some(StringOrNumber::I64(v)) => Ok(v.to_string()),
        Some(StringOrNumber::U64(v)) => Ok(v.to_string()),
        Some(StringOrNumber::F64(v)) => Ok(compact_f32_text(v as f32)),
        None => Ok(String::new()),
    }
}

pub fn compact_f32_text(value: f32) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_score::{GrooveStatsSubmitRecordBanner, GrooveStatsSubmitUiStatus, RejectReason};

    fn leaderboard_entry(rank: u32, name: &str, score: f64, is_self: bool) -> LeaderboardApiEntry {
        LeaderboardApiEntry {
            rank,
            name: name.to_string(),
            machine_tag: None,
            score,
            date: String::new(),
            is_rival: false,
            is_self,
            is_fail: false,
            comments: None,
        }
    }

    #[test]
    fn service_from_boogiestats_flag_selects_backend() {
        assert_eq!(
            Service::from_boogiestats_active(false),
            Service::GrooveStats
        );
        assert_eq!(Service::from_boogiestats_active(true), Service::BoogieStats);
    }

    #[test]
    fn service_names_match_backend() {
        assert_eq!(service_name(Service::GrooveStats), "GrooveStats");
        assert_eq!(service_name(Service::BoogieStats), "BoogieStats");
    }

    #[test]
    fn compact_f32_text_strips_trailing_decimal_zeroes() {
        assert_eq!(compact_f32_text(1.0), "1");
        assert_eq!(compact_f32_text(1.25), "1.25");
        assert_eq!(compact_f32_text(1.5), "1.5");
    }

    #[test]
    fn submit_protocol_constants_match_legacy_api_shape() {
        assert_eq!(GROOVESTATS_CHART_HASH_VERSION, 3);
        assert_eq!(GROOVESTATS_COMMENT_PREFIX, "[DS]");
        assert_eq!(GROOVESTATS_SUBMIT_MAX_ENTRIES, 10);
    }

    #[test]
    fn api_base_urls_match_backend() {
        assert_eq!(api_base_url(Service::GrooveStats), GROOVESTATS_API_BASE_URL);
        assert_eq!(api_base_url(Service::BoogieStats), BOOGIESTATS_API_BASE_URL);
    }

    #[test]
    fn qr_login_ws_url_matches_legacy_socket() {
        assert_eq!(qr_login_ws_url(), "ws://qrlogin.groovestats.com:3000");
        assert_eq!(GROOVESTATS_QR_LOGIN_WS_READ_TIMEOUT_MS, 100);
    }

    #[test]
    fn generate_qr_login_uuid_is_32_uppercase_hex() {
        let id = generate_qr_login_uuid();
        assert_eq!(id.len(), 32);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)),
            "uuid contained a non-hex-uppercase char: {id}"
        );
    }

    #[test]
    fn generate_qr_login_uuid_returns_distinct_values() {
        let a = generate_qr_login_uuid();
        let b = generate_qr_login_uuid();
        assert_ne!(a, b);
    }

    #[test]
    fn qr_login_url_format_matches_simply_love() {
        assert_eq!(
            qr_login_url("ABCDEF", 1),
            "https://www.groovestats.com/qrlogin.php?UUID=ABCDEF&SIDE=1"
        );
        assert_eq!(
            qr_login_url("DEADBEEF", 2),
            "https://www.groovestats.com/qrlogin.php?UUID=DEADBEEF&SIDE=2"
        );
    }

    #[test]
    fn qr_login_uuid_message_announces_uuid() {
        assert_eq!(
            qr_login_uuid_message("ABC"),
            r#"{"data":{"uuid":"ABC"},"event":"uuid"}"#
        );
    }

    #[test]
    fn player_leaderboards_url_uses_selected_backend() {
        assert_eq!(
            player_leaderboards_url(Service::GrooveStats),
            "https://api.groovestats.com/player-leaderboards.php"
        );
        assert_eq!(
            player_leaderboards_url(Service::BoogieStats),
            "https://boogiestats.andr.host/player-leaderboards.php"
        );
    }

    #[test]
    fn score_submit_url_uses_selected_backend() {
        assert_eq!(
            score_submit_url(Service::GrooveStats),
            "https://api.groovestats.com/score-submit.php"
        );
        assert_eq!(
            score_submit_url(Service::BoogieStats),
            "https://boogiestats.andr.host/score-submit.php"
        );
    }

    #[test]
    fn new_session_url_uses_chart_hash_version() {
        assert_eq!(
            new_session_url(Service::GrooveStats),
            "https://api.groovestats.com/new-session.php?chartHashVersion=3"
        );
    }

    #[test]
    fn new_session_response_deserializes_camel_case() {
        let raw = r#"{
            "servicesAllowed": {
                "playerScores": true,
                "playerLeaderboards": false,
                "scoreSubmit": true
            },
            "servicesResult": "OK"
        }"#;
        let response: NewSessionResponse = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(response.services_result, "OK");
        assert!(response.services_allowed.player_scores);
        assert!(!response.services_allowed.player_leaderboards);
        assert!(response.services_allowed.score_submit);
    }

    #[test]
    fn new_session_response_maps_to_connection_status() {
        let response = NewSessionResponse {
            services_allowed: NewSessionServices {
                player_scores: true,
                player_leaderboards: false,
                score_submit: true,
            },
            services_result: "OK".to_string(),
        };

        assert_eq!(
            connection_status_from_new_session(&response),
            ConnectionStatus::Connected(Services {
                get_scores: true,
                leaderboard: false,
                auto_submit: true,
            })
        );

        let offline = NewSessionResponse {
            services_result: "Machine Offline".to_string(),
            ..response
        };
        assert_eq!(
            connection_status_from_new_session(&offline),
            ConnectionStatus::Error(ConnectionError::MachineOffline)
        );
    }

    #[test]
    fn network_errors_map_to_connection_errors() {
        assert_eq!(
            connection_error_from_network_error(&NetworkError::Timeout),
            ConnectionError::TimedOut
        );
        assert_eq!(
            connection_error_from_network_error(&NetworkError::Decode("bad json".to_string())),
            ConnectionError::InvalidResponse
        );
        assert_eq!(
            connection_error_from_network_error(&NetworkError::HttpStatus(500)),
            ConnectionError::CannotConnect
        );
        assert_eq!(
            connection_error_from_network_error(&NetworkError::Request("refused".to_string())),
            ConnectionError::CannotConnect
        );
    }

    #[test]
    fn network_errors_map_to_probe_errors() {
        assert_eq!(
            ConnectionProbeError::from(NetworkError::Timeout),
            ConnectionProbeError::Timeout
        );
        assert_eq!(
            ConnectionProbeError::from(NetworkError::Decode("bad json".to_string())),
            ConnectionProbeError::InvalidResponse("bad json".to_string())
        );
        let error = ConnectionProbeError::from(NetworkError::HttpStatus(500));
        assert_eq!(error.connection_error(), ConnectionError::CannotConnect);
        assert_eq!(error.to_string(), "http status 500");
    }

    #[test]
    fn leaderboards_response_deserializes_panes_and_events() {
        let raw = r#"{
            "player1": {
                "isRanked": true,
                "gsLeaderboard": [
                    {
                        "rank": 1,
                        "name": "ALEX",
                        "machineTag": "CAB",
                        "score": 9876.5,
                        "date": "2026-05-01",
                        "isRival": true,
                        "isSelf": false,
                        "isFail": false,
                        "comments": "solid"
                    }
                ],
                "exLeaderboard": [
                    {
                        "rank": 2,
                        "name": "BLAKE",
                        "score": 1234.0,
                        "isSelf": true
                    }
                ],
                "rpg": {
                    "name": "RPG",
                    "rpgLeaderboard": [
                        {
                            "rank": 3,
                            "name": "CASEY",
                            "score": 8765.0
                        }
                    ]
                },
                "itl": {
                    "name": "ITL",
                    "itlLeaderboard": [
                        {
                            "rank": 4,
                            "name": "DREW",
                            "score": 7654.0,
                            "isFail": true
                        }
                    ]
                }
            }
        }"#;

        let response: LeaderboardsApiResponse = serde_json::from_str(raw).expect("deserialize");
        let player = response.player1.expect("player1");
        assert!(player.is_ranked);
        assert_eq!(player.gs_leaderboard.len(), 1);
        assert_eq!(player.gs_leaderboard[0].rank, 1);
        assert_eq!(player.gs_leaderboard[0].name, "ALEX");
        assert_eq!(player.gs_leaderboard[0].machine_tag.as_deref(), Some("CAB"));
        assert_eq!(player.gs_leaderboard[0].score, 9876.5);
        assert!(player.gs_leaderboard[0].is_rival);
        assert!(!player.gs_leaderboard[0].is_self);
        assert!(!player.gs_leaderboard[0].is_fail);
        assert_eq!(player.gs_leaderboard[0].comments.as_deref(), Some("solid"));

        assert_eq!(player.ex_leaderboard[0].name, "BLAKE");
        assert!(player.ex_leaderboard[0].is_self);

        let rpg = player.rpg.expect("rpg");
        assert_eq!(rpg.name, "RPG");
        assert_eq!(rpg.rpg_leaderboard[0].name, "CASEY");
        assert!(rpg.itl_leaderboard.is_empty());

        let itl = player.itl.expect("itl");
        assert_eq!(itl.name, "ITL");
        assert_eq!(itl.itl_leaderboard[0].name, "DREW");
        assert!(itl.itl_leaderboard[0].is_fail);
        assert!(itl.rpg_leaderboard.is_empty());
    }

    #[test]
    fn leaderboard_self_score_prefers_self_flag() {
        let entries = vec![
            LeaderboardApiEntry {
                rank: 10,
                name: "Other".to_string(),
                machine_tag: None,
                score: 9321.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
                comments: None,
            },
            LeaderboardApiEntry {
                rank: 25,
                name: "Player".to_string(),
                machine_tag: None,
                score: 9789.0,
                date: String::new(),
                is_rival: false,
                is_self: true,
                is_fail: false,
                comments: None,
            },
        ];

        assert_eq!(
            leaderboard_self_score_10000(&entries, "ignored"),
            Some(9789)
        );
        assert_eq!(leaderboard_self_rank(&entries, "ignored"), Some(25));
    }

    #[test]
    fn leaderboard_self_score_falls_back_to_username_match() {
        let entries = vec![LeaderboardApiEntry {
            rank: 25,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 9712.0,
            date: String::new(),
            is_rival: false,
            is_self: false,
            is_fail: false,
            comments: None,
        }];

        assert_eq!(
            leaderboard_self_score_10000(&entries, "perfecttaste"),
            Some(9712)
        );
        assert_eq!(leaderboard_self_rank(&entries, "perfecttaste"), Some(25));
    }

    #[test]
    fn leaderboard_self_rank_ignores_zero_rank() {
        let entries = vec![LeaderboardApiEntry {
            rank: 0,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 9712.0,
            date: String::new(),
            is_rival: false,
            is_self: true,
            is_fail: false,
            comments: None,
        }];

        assert_eq!(leaderboard_self_rank(&entries, "perfecttaste"), None);
    }

    #[test]
    fn leaderboard_pane_from_api_maps_entries() {
        let pane = leaderboard_pane_from_api(
            "GrooveStats",
            vec![LeaderboardApiEntry {
                rank: 2,
                name: "Player".to_string(),
                machine_tag: Some("CAB".to_string()),
                score: 9876.0,
                date: "2026-05-03".to_string(),
                is_rival: true,
                is_self: false,
                is_fail: false,
                comments: Some("not displayed".to_string()),
            }],
            false,
        )
        .expect("pane");

        assert_eq!(pane.name, "GrooveStats");
        assert_eq!(pane.entries.len(), 1);
        assert_eq!(pane.entries[0].machine_tag.as_deref(), Some("CAB"));
        assert!(pane.entries[0].is_rival);
    }

    #[test]
    fn fetched_player_leaderboards_from_api_builds_panes_and_self_data() {
        let fetched = fetched_player_leaderboards_from_api(
            LeaderboardsApiResponse {
                player1: Some(LeaderboardApiPlayer {
                    is_ranked: true,
                    gs_leaderboard: vec![leaderboard_entry(3, "PerfectTaste", 9876.0, true)],
                    ex_leaderboard: vec![leaderboard_entry(2, "PerfectTaste", 9912.0, true)],
                    rpg: Some(LeaderboardEventData {
                        name: " ".to_string(),
                        rpg_leaderboard: vec![leaderboard_entry(4, "RPG", 9000.0, false)],
                        itl_leaderboard: Vec::new(),
                    }),
                    itl: Some(LeaderboardEventData {
                        name: String::new(),
                        rpg_leaderboard: Vec::new(),
                        itl_leaderboard: vec![leaderboard_entry(42, "PerfectTaste", 8765.0, true)],
                    }),
                }),
            },
            "perfecttaste",
            true,
        );

        assert_eq!(fetched.gs_entries.len(), 1);
        assert_eq!(fetched.ex_entries.len(), 1);
        assert!(fetched.itl_self_found);
        assert_eq!(fetched.data.itl_self_score, Some(8765));
        assert_eq!(fetched.data.itl_self_rank, Some(42));
        assert_eq!(fetched.data.panes.len(), 4);
        assert_eq!(fetched.data.panes[0].name, "GrooveStats");
        assert!(fetched.data.panes[0].is_ex);
        assert_eq!(fetched.data.panes[1].name, "GrooveStats");
        assert!(!fetched.data.panes[1].is_ex);
        assert_eq!(fetched.data.panes[2].name, "RPG");
        assert_eq!(fetched.data.panes[3].name, "ITL");
        assert!(fetched.data.panes[3].is_ex);
    }

    #[test]
    fn submit_payload_serializes_old_api_shape() {
        let payload = GrooveStatsSubmitPlayerPayload {
            rate: 150,
            score: 9_975,
            judgment_counts: GrooveStatsJudgmentCounts {
                fantastic_plus: 7,
                fantastic: 12,
                excellent: 18,
                great: 4,
                decent: Some(1),
                way_off: Some(0),
                miss: 2,
                total_steps: 213,
                holds_held: 5,
                total_holds: 6,
                mines_hit: 1,
                total_mines: 8,
                rolls_held: 2,
                total_rolls: 3,
            },
            rescore_counts: GrooveStatsRescoreCounts {
                fantastic_plus: 1,
                fantastic: 2,
                excellent: 3,
                great: 4,
                decent: 5,
                way_off: 6,
            },
            used_cmod: true,
            comment: "[DS], FA+, 99.50EX, 2w, 1m, C650".to_string(),
            player_options: "{\"SpeedModType\":2,\"SpeedMod\":650}".to_string(),
        };

        let value = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(value["rate"], 150);
        assert_eq!(value["score"], 9_975);
        assert!(value.get("isFail").is_none());
        assert_eq!(value["judgmentCounts"]["fantasticPlus"], 7);
        assert_eq!(value["judgmentCounts"]["decent"], 1);
        assert_eq!(value["judgmentCounts"]["wayOff"], 0);
        assert_eq!(value["judgmentCounts"]["totalMines"], 8);
        assert_eq!(value["rescoreCounts"]["wayOff"], 6);
        assert_eq!(value["usedCmod"], true);
        assert_eq!(value["comment"], "[DS], FA+, 99.50EX, 2w, 1m, C650");
        assert_eq!(
            value["playerOptions"],
            "{\"SpeedModType\":2,\"SpeedMod\":650}"
        );
    }

    #[test]
    fn submit_payload_omits_disabled_bad_windows() {
        let counts = GrooveStatsJudgmentCounts {
            fantastic_plus: 8,
            fantastic: 17,
            excellent: 98,
            great: 270,
            decent: None,
            way_off: None,
            miss: 1,
            total_steps: 394,
            holds_held: 18,
            total_holds: 18,
            mines_hit: 0,
            total_mines: 0,
            rolls_held: 8,
            total_rolls: 8,
        };

        let value = serde_json::to_value(&counts).expect("serialize");
        assert_eq!(value["fantasticPlus"], 8);
        assert_eq!(value["great"], 270);
        assert_eq!(value["totalSteps"], 394);
        assert!(value.get("decent").is_none());
        assert!(value.get("wayOff").is_none());
        assert_eq!(counts.decent_count(), 0);
        assert_eq!(counts.way_off_count(), 0);
    }

    #[test]
    fn submit_error_maps_transport_errors_to_status() {
        let (status, message) = submit_error_status_and_message(
            "GrooveStats",
            &GrooveStatsSubmitRequestError::Transport {
                message: "network error: timed out".to_string(),
                timed_out: true,
            },
        );
        assert_eq!(status, GrooveStatsSubmitUiStatus::TimedOut);
        assert_eq!(message, "network error: timed out");

        let (status, _) = submit_error_status_and_message(
            "GrooveStats",
            &GrooveStatsSubmitRequestError::Transport {
                message: "network error: refused".to_string(),
                timed_out: false,
            },
        );
        assert_eq!(status, GrooveStatsSubmitUiStatus::NetworkError);
    }

    #[test]
    fn submit_error_maps_protocol_errors_to_status() {
        let (status, message) = submit_error_status_and_message(
            "GrooveStats",
            &GrooveStatsSubmitRequestError::Http {
                status: 403,
                body_snippet: "bad key".to_string(),
            },
        );
        assert_eq!(
            status,
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
        );
        assert_eq!(message, "GrooveStats submit returned HTTP 403: bad key");

        let (status, message) = submit_error_status_and_message(
            "GrooveStats",
            &GrooveStatsSubmitRequestError::Api {
                message: "score-already-submitted".to_string(),
            },
        );
        assert_eq!(
            status,
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
        );
        assert_eq!(message, "GrooveStats submit error: score-already-submitted");
    }

    fn submit_player(
        result: &str,
        gs_leaderboard: Vec<LeaderboardApiEntry>,
        ex_leaderboard: Vec<LeaderboardApiEntry>,
    ) -> GrooveStatsSubmitApiPlayer {
        GrooveStatsSubmitApiPlayer {
            chart_hash: "deadbeef".to_string(),
            result: result.to_string(),
            gs_leaderboard,
            ex_leaderboard,
            rpg: None,
            itl: None,
        }
    }

    #[test]
    fn submit_record_banner_from_api_maps_response_rank() {
        let banner = submit_record_banner_from_api(
            &submit_player(
                "improved",
                vec![leaderboard_entry(1, "PerfectTaste", 9999.0, true)],
                Vec::new(),
            ),
            "PerfectTaste",
            false,
        );
        assert_eq!(banner, Some(GrooveStatsSubmitRecordBanner::WorldRecord));

        let banner = submit_record_banner_from_api(
            &submit_player(
                "score-added",
                vec![leaderboard_entry(2, "PerfectTaste", 9999.0, true)],
                vec![leaderboard_entry(1, "PerfectTaste", 9999.0, true)],
            ),
            "PerfectTaste",
            true,
        );
        assert_eq!(banner, Some(GrooveStatsSubmitRecordBanner::WorldRecordEx));

        let banner = submit_record_banner_from_api(
            &submit_player(
                "improved",
                vec![leaderboard_entry(3, "PerfectTaste", 9999.0, true)],
                vec![leaderboard_entry(4, "PerfectTaste", 9999.0, true)],
            ),
            "PerfectTaste",
            true,
        );
        assert_eq!(banner, Some(GrooveStatsSubmitRecordBanner::PersonalBest));

        let banner = submit_record_banner_from_api(
            &submit_player(
                "score-already-submitted",
                vec![leaderboard_entry(1, "PerfectTaste", 9999.0, true)],
                Vec::new(),
            ),
            "PerfectTaste",
            false,
        );
        assert_eq!(banner, None);
    }

    #[test]
    fn manual_qr_url_preserves_base_url_case_and_encodes_rescore() {
        let counts = GrooveStatsJudgmentCounts {
            fantastic_plus: 0x0a,
            fantastic: 0x0b,
            excellent: 0x0c,
            great: 0x0d,
            decent: Some(0x0e),
            way_off: Some(0x0f),
            miss: 0x10,
            total_steps: 0x1d,
            holds_held: 0x11,
            total_holds: 0x12,
            mines_hit: 0x15,
            total_mines: 0x16,
            rolls_held: 0x13,
            total_rolls: 0x14,
        };
        let rescored = GrooveStatsRescoreCounts {
            fantastic_plus: 0x01,
            fantastic: 0x02,
            excellent: 0x03,
            great: 0x04,
            decent: 0x05,
            way_off: 0x06,
        };

        let url = manual_qr_url(
            "https://www.groovestats.com",
            "deadbeef",
            &counts,
            &rescored,
            150,
            true,
        )
        .expect("manual qr url");

        assert_eq!(
            url,
            "https://www.groovestats.com/QR/deadbeef/T1dGaHbIcJdKeLfM10H11T12R13T14M15T16G1H2I3J4K5L6/F0R96C1V3"
        );
        assert!(
            manual_qr_url(
                "https://www.groovestats.com",
                " ",
                &counts,
                &rescored,
                150,
                true,
            )
            .is_none()
        );
    }

    #[test]
    fn classify_qr_login_ws_message_routes_matching_uuid() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"GS-1","username":"alice","side":1}}"#;
        assert_eq!(
            classify_qr_login_ws_message(payload, "ABC"),
            GrooveStatsQrLoginWsEffect::DeliverApiKey {
                side: 1,
                api_key: "GS-1".into(),
                username: "alice".into(),
            }
        );
    }

    #[test]
    fn classify_qr_login_ws_message_ignores_mismatched_uuid() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"OTHER","apiKey":"GS-1","side":1}}"#;
        assert_eq!(
            classify_qr_login_ws_message(payload, "ABC"),
            GrooveStatsQrLoginWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_qr_login_ws_message_ignores_non_apikey_events() {
        let payload = r#"{"event":"hello","data":{"uuid":"ABC"}}"#;
        assert_eq!(
            classify_qr_login_ws_message(payload, "ABC"),
            GrooveStatsQrLoginWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_qr_login_ws_message_ignores_empty_api_key() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"   ","side":1}}"#;
        assert_eq!(
            classify_qr_login_ws_message(payload, "ABC"),
            GrooveStatsQrLoginWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_qr_login_ws_message_ignores_unknown_side() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"k","side":7}}"#;
        assert_eq!(
            classify_qr_login_ws_message(payload, "ABC"),
            GrooveStatsQrLoginWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_qr_login_ws_message_defaults_missing_username_to_empty() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"k","side":2}}"#;
        assert_eq!(
            classify_qr_login_ws_message(payload, "ABC"),
            GrooveStatsQrLoginWsEffect::DeliverApiKey {
                side: 2,
                api_key: "k".into(),
                username: String::new(),
            }
        );
    }

    #[test]
    fn classify_qr_login_ws_message_ignores_malformed_json() {
        assert_eq!(
            classify_qr_login_ws_message("not json", "ABC"),
            GrooveStatsQrLoginWsEffect::Ignore,
        );
    }

    #[test]
    fn submit_response_deserializes_players_events_and_progress() {
        let raw = r#"{
            "player1": {
                "chartHash": "deadbeef",
                "result": "improved",
                "gsLeaderboard": [
                    { "rank": 1, "name": "ALEX", "score": 9999.0, "isSelf": true }
                ],
                "exLeaderboard": [
                    { "rank": 2, "name": "BLAKE", "score": 9876.0 }
                ],
                "itl": {
                    "name": "ITL",
                    "scoreDelta": "-123",
                    "topScorePoints": "42",
                    "prevTopScorePoints": 40.9,
                    "totalPasses": "3",
                    "currentRankingPointTotal": 100,
                    "previousRankingPointTotal": "80",
                    "currentSongPointTotal": 50,
                    "previousSongPointTotal": "45",
                    "currentExPointTotal": 70,
                    "previousExPointTotal": "65",
                    "currentPointTotal": 120,
                    "previousPointTotal": "110",
                    "itlLeaderboard": [
                        { "rank": 4, "name": "DREW", "score": 7654.0 }
                    ],
                    "isDoubles": true,
                    "progress": {
                        "statImprovements": [
                            { "name": "clearType", "gained": "2", "current": "4" }
                        ],
                        "questsCompleted": [
                            {
                                "title": "Unlock One",
                                "songDownloadUrl": "https://example.invalid/song.zip",
                                "songDownloadFolders": ["Pack A"],
                                "rewards": [
                                    { "type": "song", "description": "Song A" }
                                ]
                            }
                        ],
                        "achievementsCompleted": [
                            {
                                "title": "Achievement One",
                                "rewards": [
                                    {
                                        "tier": 2.0,
                                        "requirements": ["Req A"],
                                        "titleUnlocked": "Title A"
                                    }
                                ]
                            }
                        ]
                    }
                }
            },
            "player2": {
                "chartHash": "feedface",
                "result": "score-already-submitted"
            }
        }"#;

        let response: GrooveStatsSubmitApiResponse =
            serde_json::from_str(raw).expect("deserialize");
        assert!(response.error.is_empty());

        let player1 = response.player_for_slot(1).expect("player1");
        assert_eq!(player1.chart_hash, "deadbeef");
        assert_eq!(player1.result, "improved");
        assert_eq!(player1.gs_leaderboard[0].rank, 1);
        assert!(player1.gs_leaderboard[0].is_self);
        assert_eq!(player1.ex_leaderboard[0].name, "BLAKE");

        let itl = player1.itl.as_ref().expect("itl");
        assert_eq!(itl.name, "ITL");
        assert_eq!(itl.score_delta, -123);
        assert_eq!(itl.top_score_points, 42);
        assert_eq!(itl.prev_top_score_points, 40);
        assert_eq!(itl.total_passes, 3);
        assert_eq!(itl.current_ranking_point_total, 100);
        assert_eq!(itl.previous_ranking_point_total, 80);
        assert_eq!(itl.current_song_point_total, 50);
        assert_eq!(itl.previous_song_point_total, 45);
        assert_eq!(itl.current_ex_point_total, 70);
        assert_eq!(itl.previous_ex_point_total, 65);
        assert_eq!(itl.current_point_total, 120);
        assert_eq!(itl.previous_point_total, 110);
        assert!(itl.is_doubles);
        assert_eq!(itl.itl_leaderboard[0].name, "DREW");

        let progress = itl.progress.as_ref().expect("progress");
        assert_eq!(progress.stat_improvements[0].name, "clearType");
        assert_eq!(progress.stat_improvements[0].gained, 2);
        assert_eq!(progress.stat_improvements[0].current, 4);
        assert_eq!(progress.quests_completed[0].title, "Unlock One");
        assert_eq!(progress.quests_completed[0].rewards[0].reward_type, "song");
        assert_eq!(
            progress.quests_completed[0].song_download_folders[0],
            "Pack A"
        );
        assert_eq!(progress.achievements_completed[0].title, "Achievement One");
        assert_eq!(progress.achievements_completed[0].rewards[0].tier, "2");
        assert_eq!(
            progress.achievements_completed[0].rewards[0].requirements[0],
            "Req A"
        );
        assert_eq!(
            progress.achievements_completed[0].rewards[0].title_unlocked,
            "Title A"
        );

        let player2 = response.player_for_slot(2).expect("player2");
        assert_eq!(player2.chart_hash, "feedface");
        assert_eq!(player2.result, "score-already-submitted");
        assert!(response.player_for_slot(3).is_none());
    }

    #[test]
    fn submit_response_exposes_server_error_string() {
        let response: GrooveStatsSubmitApiResponse =
            serde_json::from_str(r#"{ "error": "bad api key" }"#).expect("deserialize");

        assert_eq!(response.error, "bad api key");
        assert!(response.player_for_slot(1).is_none());
    }
}
