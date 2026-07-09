use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Instant;

use crate::OnlineRequestError;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::GameplayProfileData;
use deadsync_net::{self as network, NetworkError};
use deadsync_profile as profile_data;
use deadsync_profile::{Profile, RemoveMask, TimingWindowsOption};
use deadsync_profile_gameplay::{
    groovestats_eval_state_from_profile, groovestats_submit_invalid_reason_from_profile,
    itl_current_score_hundredths_from_runtime,
};
use deadsync_rules::{judgment, note::Note, scroll::ScrollSpeedSetting, timing::WindowCounts};
use deadsync_score::{
    EventProgress, GrooveStatsAutosubmitLog, GrooveStatsAutosubmitLogLevel,
    GrooveStatsAutosubmitPlayerAction, GrooveStatsAutosubmitPlayerInput,
    GrooveStatsAutosubmitSessionDecision, GrooveStatsAutosubmitSessionInput, GrooveStatsEvalState,
    GrooveStatsGameplayEvalInput, GrooveStatsSubmitRecordBanner, GrooveStatsSubmitUiStatus,
    GsExEvidence, GsLampChartStats, ImportedPlayerScore, ItlEventProgress, LeaderboardEntry,
    LeaderboardPane, PlayerLeaderboardData, PlayerScoreImportResult, RejectReason,
    SUBMIT_RETRY_MAX_ATTEMPTS, ScoreImportEndpoint, SubmitAchievement, SubmitAchievementReward,
    SubmitEventProgressData, SubmitEventProgressInput, SubmitProgress, SubmitQuest,
    SubmitQuestReward, SubmitRetryState, SubmitStatImprovement,
    cached_score_from_imported_player_score, event_name_or_unknown, event_progress_from_submit,
    groovestats_autosubmit_player_decision, groovestats_autosubmit_session_decision,
    groovestats_eval_state_from_gameplay_parts, groovestats_submit_record_banner,
    groovestats_submit_ui_status, leaderboard_nonzero_rank, leaderboard_pane,
    leaderboard_score_10000, leaderboard_username_matches, score_import_entry_matches_profile,
    validate_score_import_credentials,
};
use serde_json::{Map as JsonMap, Value as JsonValue};

const GROOVESTATS_API_BASE_URL: &str = "https://api.groovestats.com";
const GROOVESTATS_API_SERVICE_URL: &str = "https://apiservice.groovestats.com/api/";
const BOOGIESTATS_API_BASE_URL: &str = "https://boogiestats.andr.host";
const GROOVESTATS_QR_BASE_URL: &str = "https://www.groovestats.com";
const GROOVESTATS_QR_LOGIN_WS_URL: &str = "ws://qrlogin.groovestats.com:3000";
const GROOVESTATS_QR_LOGIN_URL: &str = "https://www.groovestats.com/qrlogin.php";
const LEGACY_NEW_SESSION_PATH: &str = "new-session.php?chartHashVersion=3";
pub const GROOVESTATS_QR_LOGIN_WS_READ_TIMEOUT_MS: u64 = 100;
pub const GROOVESTATS_CHART_HASH_VERSION: u8 = 3;
pub const GROOVESTATS_COMMENT_PREFIX: &str = "[DS]";
pub const GROOVESTATS_SUBMIT_MAX_ENTRIES: usize = 10;
const GROOVESTATS_RETRY_MAX_ATTEMPTS: u8 = SUBMIT_RETRY_MAX_ATTEMPTS;
const GROOVESTATS_SUBMIT_RETRY_TRACKED_PER_SIDE: usize = 128;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionProbeLog {
    Connected {
        service: Service,
        services: Services,
    },
    MachineOffline {
        service: Service,
    },
    Timeout {
        service: Service,
    },
    InvalidResponse {
        service: Service,
        error: String,
    },
    CannotConnect {
        service: Service,
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProbeTransition {
    pub status: ConnectionStatus,
    pub log: Option<ConnectionProbeLog>,
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

    #[inline(always)]
    pub const fn score_import_endpoint(self) -> ScoreImportEndpoint {
        match self {
            Self::GrooveStats => ScoreImportEndpoint::GrooveStats,
            Self::BoogieStats => ScoreImportEndpoint::BoogieStats,
        }
    }
}

#[inline(always)]
pub const fn boogiestats_active(enable_groovestats: bool, enable_boogiestats: bool) -> bool {
    enable_groovestats && enable_boogiestats
}

#[inline(always)]
pub const fn active_service(enable_groovestats: bool, enable_boogiestats: bool) -> Service {
    Service::from_boogiestats_active(boogiestats_active(enable_groovestats, enable_boogiestats))
}

#[inline(always)]
pub const fn service_name(service: Service) -> &'static str {
    match service {
        Service::GrooveStats => "GrooveStats",
        Service::BoogieStats => "BoogieStats",
    }
}

#[inline(always)]
pub fn warn_submit_skip(
    service_name: &str,
    side: profile_data::PlayerSide,
    chart_hash: &str,
    reason: &str,
) {
    log::warn!(
        "Skipping {service_name} submit for {:?} ({}): {}.",
        side,
        chart_hash,
        reason
    );
}

#[inline(always)]
pub fn log_global_submit_skip(service_name: &str, log: GrooveStatsAutosubmitLog) {
    match log.level {
        GrooveStatsAutosubmitLogLevel::Debug => {
            log::debug!("Skipping {service_name} submit: {}.", log.reason)
        }
        GrooveStatsAutosubmitLogLevel::Warn => {
            log::warn!("Skipping {service_name} submit: {}.", log.reason)
        }
    }
}

#[inline(always)]
pub fn log_player_submit_skip(
    service_name: &str,
    side: profile_data::PlayerSide,
    chart_hash: &str,
    log: GrooveStatsAutosubmitLog,
) {
    match log.level {
        GrooveStatsAutosubmitLogLevel::Debug => log::debug!(
            "Skipping {service_name} submit for {:?} ({}): {}.",
            side,
            chart_hash,
            log.reason
        ),
        GrooveStatsAutosubmitLogLevel::Warn => {
            warn_submit_skip(service_name, side, chart_hash, log.reason)
        }
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
fn groovestats_action_url(action: &str) -> String {
    format!("{GROOVESTATS_API_SERVICE_URL}?action={action}")
}

#[inline(always)]
pub fn player_leaderboards_url(service: Service) -> String {
    match service {
        Service::GrooveStats => groovestats_action_url("playerLeaderboards"),
        Service::BoogieStats => format!(
            "{}/player-leaderboards.php",
            api_base_url(service).trim_end_matches('/')
        ),
    }
}

#[inline(always)]
pub fn score_submit_url(service: Service) -> String {
    match service {
        Service::GrooveStats => groovestats_action_url("scoreSubmit"),
        Service::BoogieStats => format!(
            "{}/score-submit.php",
            api_base_url(service).trim_end_matches('/')
        ),
    }
}

#[inline(always)]
pub fn new_session_url(service: Service) -> String {
    match service {
        Service::GrooveStats => format!(
            "{}&chartHashVersion={GROOVESTATS_CHART_HASH_VERSION}",
            groovestats_action_url("newSession")
        ),
        Service::BoogieStats => format!(
            "{}/{}",
            api_base_url(service).trim_end_matches('/'),
            LEGACY_NEW_SESSION_PATH
        ),
    }
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

pub fn connection_transition_from_probe_result(
    service: Service,
    result: Result<ConnectionStatus, ConnectionProbeError>,
) -> ConnectionProbeTransition {
    match result {
        Ok(status) => {
            let log = match &status {
                ConnectionStatus::Connected(services) => Some(ConnectionProbeLog::Connected {
                    service,
                    services: services.clone(),
                }),
                ConnectionStatus::Error(ConnectionError::MachineOffline) => {
                    Some(ConnectionProbeLog::MachineOffline { service })
                }
                ConnectionStatus::Pending
                | ConnectionStatus::Error(
                    ConnectionError::Disabled
                    | ConnectionError::CannotConnect
                    | ConnectionError::TimedOut
                    | ConnectionError::InvalidResponse,
                ) => None,
            };
            ConnectionProbeTransition { status, log }
        }
        Err(error) => {
            let status = ConnectionStatus::Error(error.connection_error());
            let log = match error {
                ConnectionProbeError::Timeout => ConnectionProbeLog::Timeout { service },
                ConnectionProbeError::InvalidResponse(error) => {
                    ConnectionProbeLog::InvalidResponse { service, error }
                }
                ConnectionProbeError::CannotConnect(error) => {
                    ConnectionProbeLog::CannotConnect { service, error }
                }
            };
            ConnectionProbeTransition {
                status,
                log: Some(log),
            }
        }
    }
}

pub fn probe_connection_transition(service: Service) -> ConnectionProbeTransition {
    connection_transition_from_probe_result(service, probe_connection(service))
}

static RUNTIME_STATUS: LazyLock<Mutex<ConnectionStatus>> =
    LazyLock::new(|| Mutex::new(ConnectionStatus::Pending));

pub type ConnectionProbeLogFn = fn(Option<ConnectionProbeLog>);

#[inline(always)]
fn runtime_set_status(status: ConnectionStatus) {
    *RUNTIME_STATUS.lock().unwrap() = status;
}

pub fn runtime_get_status() -> ConnectionStatus {
    RUNTIME_STATUS.lock().unwrap().clone()
}

pub fn runtime_init(enabled: bool, service: Service, log_probe: ConnectionProbeLogFn) {
    if !enabled {
        runtime_set_status(ConnectionStatus::Error(ConnectionError::Disabled));
        return;
    }

    runtime_set_status(ConnectionStatus::Pending);
    thread::spawn(move || runtime_perform_check(service, log_probe));
}

pub fn runtime_init_with_default_log(enabled: bool, boogiestats_active: bool) {
    let service = Service::from_boogiestats_active(boogiestats_active);
    if enabled {
        let service_name = service_name(service);
        log::debug!("Initializing {service_name} network check...");
    }
    runtime_init(enabled, service, log_probe_transition);
}

pub fn log_probe_transition(log: Option<ConnectionProbeLog>) {
    match log {
        Some(ConnectionProbeLog::Connected { service, services }) => {
            let service_name = service_name(service);
            log::info!(
                "Connected to {service_name} (scores={}, leaderboards={}, autosubmit={}).",
                services.get_scores,
                services.leaderboard,
                services.auto_submit
            );
        }
        Some(ConnectionProbeLog::MachineOffline { service }) => {
            let service_name = service_name(service);
            log::warn!("{service_name} servicesResult != OK.");
        }
        Some(ConnectionProbeLog::Timeout { service }) => {
            let service_name = service_name(service);
            log::warn!("{service_name} connectivity check timed out.");
        }
        Some(ConnectionProbeLog::InvalidResponse { service, error }) => {
            let service_name = service_name(service);
            log::warn!("Failed to parse {service_name} response: {error}");
        }
        Some(ConnectionProbeLog::CannotConnect { service, error }) => {
            let service_name = service_name(service);
            log::warn!("HTTP error to {service_name}: {error}");
        }
        None => {}
    }
}

fn runtime_perform_check(service: Service, log_probe: ConnectionProbeLogFn) {
    let transition = probe_connection_transition(service);
    log_probe(transition.log);
    runtime_set_status(transition.status);
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
    #[serde(rename = "rpg")]
    pub srpg: Option<LeaderboardEventData>,
    pub itl: Option<LeaderboardEventData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEventData {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "rpgLeaderboard", default)]
    pub srpg_leaderboard: Vec<LeaderboardApiEntry>,
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

pub fn imported_player_score_from_leaderboard_entries(
    gs_entries: &[LeaderboardApiEntry],
    ex_entries: &[LeaderboardApiEntry],
    username: &str,
) -> Option<ImportedPlayerScore> {
    let entry = leaderboard_self_entry(gs_entries, username)?;
    let ex_evidence =
        gs_ex_evidence_from_leaderboard(ex_entries, username, entry.comments.as_deref());
    Some(ImportedPlayerScore {
        score_10000: entry.score,
        comments: entry.comments.clone(),
        is_fail: entry.is_fail,
        ex_evidence,
    })
}

#[derive(Debug)]
pub struct FetchedPlayerLeaderboards {
    pub data: PlayerLeaderboardData,
    pub imported_score: Option<ImportedPlayerScore>,
    pub itl_self_found: bool,
}

#[derive(Debug)]
pub struct CombinedPlayerLeaderboards {
    pub fetched: FetchedPlayerLeaderboards,
    pub arrowcloud_error: Option<OnlineRequestError>,
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

fn insert_arrowcloud_panes(fetched: &mut FetchedPlayerLeaderboards, panes: Vec<LeaderboardPane>) {
    let insert_ix = 2.min(fetched.data.panes.len());
    for pane in panes.into_iter().rev() {
        fetched.data.panes.insert(insert_ix, pane);
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

pub fn fetch_combined_player_leaderboards(
    service: Service,
    api_key: &str,
    username: &str,
    chart_hash: &str,
    arrowcloud_api_key: Option<&str>,
    show_ex_score: bool,
    max_entries: usize,
) -> Result<CombinedPlayerLeaderboards, OnlineRequestError> {
    let max_entries = max_entries.max(1);
    let decoded = fetch_player_leaderboards(service, api_key, chart_hash, Some(max_entries))?;
    let mut fetched = fetched_player_leaderboards_from_api(decoded, username, show_ex_score);
    let mut arrowcloud_error = None;

    if let Some(arrowcloud_api_key) = arrowcloud_api_key {
        match crate::arrowcloud::fetch_hard_ex_leaderboard_panes(chart_hash, arrowcloud_api_key) {
            Ok(arrowcloud_panes) => insert_arrowcloud_panes(&mut fetched, arrowcloud_panes),
            Err(error) => arrowcloud_error = Some(error),
        }
    }

    Ok(CombinedPlayerLeaderboards {
        fetched,
        arrowcloud_error,
    })
}

pub fn fetch_validated_combined_player_leaderboards(
    service: Service,
    api_key: &str,
    username: &str,
    chart_hash: &str,
    arrowcloud_api_key: Option<&str>,
    show_ex_score: bool,
    max_entries: usize,
) -> Result<CombinedPlayerLeaderboards, Box<dyn std::error::Error + Send + Sync>> {
    if chart_hash.trim().is_empty() {
        return Err("Missing chart hash for leaderboard request.".into());
    }
    if api_key.trim().is_empty() {
        return Err("Missing GrooveStats API key for leaderboard request.".into());
    }
    fetch_combined_player_leaderboards(
        service,
        api_key,
        username,
        chart_hash,
        arrowcloud_api_key,
        show_ex_score,
        max_entries,
    )
    .map_err(|error| crate::boxed_request_error("Leaderboard API", error))
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

pub fn fetch_player_score_import_result(
    endpoint: ScoreImportEndpoint,
    api_key: &str,
    username: &str,
    chart_hash: &str,
) -> Result<PlayerScoreImportResult, OnlineRequestError> {
    let decoded = match endpoint {
        ScoreImportEndpoint::GrooveStats => {
            fetch_player_leaderboards(Service::GrooveStats, api_key, chart_hash, None)
        }
        ScoreImportEndpoint::BoogieStats => {
            fetch_player_leaderboards(Service::BoogieStats, api_key, chart_hash, None)
        }
        ScoreImportEndpoint::ArrowCloud => {
            crate::arrowcloud::fetch_player_leaderboards(api_key, chart_hash)
        }
    }?;
    Ok(player_score_import_result_from_api(
        decoded, endpoint, username,
    ))
}

pub fn fetch_validated_player_score_import_result(
    endpoint: ScoreImportEndpoint,
    api_key: &str,
    username: &str,
    chart_hash: &str,
) -> Result<PlayerScoreImportResult, Box<dyn std::error::Error + Send + Sync>> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() {
        return Err("Missing chart hash for score request.".into());
    }
    if let Err(error) = validate_score_import_credentials(endpoint, api_key, username) {
        return Err(error.request_message().into());
    }
    fetch_player_score_import_result(endpoint, api_key, username, chart_hash)
        .map_err(|error| crate::boxed_request_error("API", error))
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

#[inline(always)]
const fn submit_bad_window_count(disabled: bool, count: u32) -> Option<u32> {
    if disabled { None } else { Some(count) }
}

pub fn judgment_counts_from_stats(
    windows: WindowCounts,
    disabled_windows: [bool; 5],
    total_steps: u32,
    holds_held: u32,
    total_holds: u32,
    mines_hit: u32,
    total_mines: u32,
    rolls_held: u32,
    total_rolls: u32,
) -> GrooveStatsJudgmentCounts {
    GrooveStatsJudgmentCounts {
        fantastic_plus: windows.w0,
        fantastic: windows.w1,
        excellent: windows.w2,
        great: windows.w3,
        decent: submit_bad_window_count(disabled_windows[3], windows.w4),
        way_off: submit_bad_window_count(disabled_windows[4], windows.w5),
        miss: windows.miss,
        total_steps,
        holds_held,
        total_holds,
        mines_hit,
        total_mines,
        rolls_held,
        total_rolls,
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

pub fn add_rescore_target(counts: &mut GrooveStatsRescoreCounts, judgment: &judgment::Judgment) {
    if matches!(judgment.window, Some(judgment::TimingWindow::W0)) {
        counts.fantastic_plus = counts.fantastic_plus.saturating_add(1);
        return;
    }
    match judgment.grade {
        judgment::JudgeGrade::Fantastic => counts.fantastic = counts.fantastic.saturating_add(1),
        judgment::JudgeGrade::Excellent => counts.excellent = counts.excellent.saturating_add(1),
        judgment::JudgeGrade::Great => counts.great = counts.great.saturating_add(1),
        judgment::JudgeGrade::Decent => counts.decent = counts.decent.saturating_add(1),
        judgment::JudgeGrade::WayOff => counts.way_off = counts.way_off.saturating_add(1),
        judgment::JudgeGrade::Miss => {}
    }
}

#[inline(always)]
pub const fn final_result_counts_as_rescore_target(judgment: &judgment::Judgment) -> bool {
    !matches!(
        judgment.grade,
        judgment::JudgeGrade::Decent | judgment::JudgeGrade::WayOff | judgment::JudgeGrade::Miss
    )
}

pub fn rescore_counts_from_judgments<'a, I>(judgments: I) -> GrooveStatsRescoreCounts
where
    I: IntoIterator<Item = (&'a judgment::Judgment, &'a judgment::Judgment)>,
{
    let mut counts = GrooveStatsRescoreCounts::default();
    for (final_result, early_result) in judgments {
        if final_result_counts_as_rescore_target(final_result) {
            add_rescore_target(&mut counts, final_result);
        }
        add_rescore_target(&mut counts, early_result);
    }
    counts
}

pub fn submit_comment(
    counts: &GrooveStatsJudgmentCounts,
    fa_plus_ex_score: Option<f64>,
    music_rate: f32,
    timing_windows: TimingWindowsOption,
    scroll_speed: ScrollSpeedSetting,
) -> String {
    let mut parts = Vec::with_capacity(11);

    if let Some(ex_score) = fa_plus_ex_score {
        parts.push("FA+".to_string());
        parts.push(format!("{ex_score:.2}EX"));
    }

    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    if (rate - 1.0).abs() > 0.0001 {
        parts.push(format!("{}x Rate", compact_f32_text(rate)));
    }

    for (count, suffix) in [
        (counts.fantastic, "w"),
        (counts.excellent, "e"),
        (counts.great, "g"),
        (counts.decent_count(), "d"),
        (counts.way_off_count(), "wo"),
        (counts.miss, "m"),
    ] {
        if count != 0 {
            parts.push(format!("{count}{suffix}"));
        }
    }

    if let Some(timing_windows) = timing_windows_comment(timing_windows) {
        parts.push(timing_windows.to_string());
    }

    if let ScrollSpeedSetting::CMod(value) = scroll_speed {
        parts.push(format!("C{}", compact_f32_text(value)));
    }

    if parts.is_empty() {
        GROOVESTATS_COMMENT_PREFIX.to_string()
    } else {
        format!("{GROOVESTATS_COMMENT_PREFIX}, {}", parts.join(", "))
    }
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

pub struct GrooveStatsSubmitPlayerPayloadInput<'a> {
    pub scoring_counts: &'a judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub rolls_held_for_score: u32,
    pub mines_hit_for_score: u32,
    pub possible_grade_points: i32,
    pub music_rate: f32,
    pub judgment_counts: GrooveStatsJudgmentCounts,
    pub rescore_counts: GrooveStatsRescoreCounts,
    pub fa_plus_ex_score: Option<f64>,
    pub timing_windows: profile_data::TimingWindowsOption,
    pub scroll_speed: ScrollSpeedSetting,
    pub player_options: &'a profile_data::Profile,
}

pub struct GrooveStatsGameplayPayloadInput<'a> {
    pub scoring_counts: &'a judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub rolls_held_for_score: u32,
    pub mines_hit_for_score: u32,
    pub possible_grade_points: i32,
    pub music_rate: f32,
    pub window_counts: WindowCounts,
    pub total_steps: u32,
    pub holds_held: u32,
    pub total_holds: u32,
    pub mines_hit: u32,
    pub total_mines: u32,
    pub rolls_held: u32,
    pub total_rolls: u32,
    pub notes: &'a [Note],
    pub note_times: &'a [i64],
    pub hold_end_times: &'a [Option<i64>],
    pub fail_time_ns: Option<i64>,
    pub profile: &'a profile_data::Profile,
}

pub fn submit_player_payload_from_input(
    input: GrooveStatsSubmitPlayerPayloadInput<'_>,
) -> GrooveStatsSubmitPlayerPayload {
    let score = deadsync_score::groovestats_score_10000_from_counts(
        input.scoring_counts,
        input.holds_held_for_score,
        input.rolls_held_for_score,
        input.mines_hit_for_score,
        input.possible_grade_points,
    );
    let comment = submit_comment(
        &input.judgment_counts,
        input.fa_plus_ex_score,
        input.music_rate,
        input.timing_windows,
        input.scroll_speed,
    );
    GrooveStatsSubmitPlayerPayload {
        rate: deadsync_score::groovestats_rate_hundredths(input.music_rate),
        score,
        judgment_counts: input.judgment_counts,
        rescore_counts: input.rescore_counts,
        used_cmod: deadsync_score::groovestats_used_cmod(input.scroll_speed),
        comment,
        player_options: player_options_json(input.player_options),
    }
}

pub fn submit_player_payload_from_gameplay_input(
    input: GrooveStatsGameplayPayloadInput<'_>,
) -> GrooveStatsSubmitPlayerPayload {
    let judgment_counts = judgment_counts_from_stats(
        input.window_counts,
        input.profile.timing_windows.disabled_windows(),
        input.total_steps,
        input.holds_held,
        input.total_holds,
        input.mines_hit,
        input.total_mines,
        input.rolls_held,
        input.total_rolls,
    );
    let rescore_counts = rescore_counts_from_judgments(
        input
            .notes
            .iter()
            .filter_map(|note| Some((note.result.as_ref()?, note.early_result.as_ref()?))),
    );
    let fa_plus_ex_score = if input.profile.show_fa_plus_window {
        Some(judgment::calculate_ex_score_from_notes(
            input.notes,
            input.note_times,
            input.hold_end_times,
            input.total_steps,
            input.total_holds,
            input.total_rolls,
            input.total_mines,
            input.fail_time_ns,
            false,
        ))
    } else {
        None
    };

    submit_player_payload_from_input(GrooveStatsSubmitPlayerPayloadInput {
        scoring_counts: input.scoring_counts,
        holds_held_for_score: input.holds_held_for_score,
        rolls_held_for_score: input.rolls_held_for_score,
        mines_hit_for_score: input.mines_hit_for_score,
        possible_grade_points: input.possible_grade_points,
        music_rate: input.music_rate,
        judgment_counts,
        rescore_counts,
        fa_plus_ex_score,
        timing_windows: input.profile.timing_windows,
        scroll_speed: input.profile.scroll_speed,
        player_options: input.profile,
    })
}

pub fn submit_player_payload_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> Option<GrooveStatsSubmitPlayerPayload>
where
    RuntimeProfile: Deref<Target = profile_data::Profile> + GameplayProfileData,
{
    if player_idx >= gs.num_players() {
        return None;
    }
    let totals = gs.display_totals_for_player(player_idx);
    let player = &gs.players()[player_idx];
    let profile = gs.profiles()[player_idx].deref();
    let (start, end) = gs.note_range_for_player(player_idx);
    Some(submit_player_payload_from_gameplay_input(
        GrooveStatsGameplayPayloadInput {
            scoring_counts: &player.scoring_counts,
            holds_held_for_score: player.holds_held_for_score,
            rolls_held_for_score: player.rolls_held_for_score,
            mines_hit_for_score: player.mines_hit_for_score,
            possible_grade_points: totals.possible_grade_points,
            music_rate: gs.music_rate(),
            window_counts: gs.live_window_counts(player_idx),
            total_steps: totals.total_steps,
            holds_held: player.holds_held,
            total_holds: totals.holds_total,
            mines_hit: player.mines_hit,
            total_mines: totals.mines_total,
            rolls_held: player.rolls_held,
            total_rolls: totals.rolls_total,
            notes: &gs.notes()[start..end],
            note_times: &gs.note_time_cache_ns()[start..end],
            hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
            fail_time_ns: player
                .fail_time
                .map(deadsync_core::song_time::song_time_ns_from_seconds),
            profile,
        },
    ))
}

fn manual_qr_url_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> Option<String>
where
    RuntimeProfile: Deref<Target = profile_data::Profile> + GameplayProfileData,
{
    if player_idx >= gs.num_players() {
        return None;
    }
    let payload = submit_player_payload_from_runtime(gs, player_idx)?;
    manual_qr_url(
        qr_base_url(),
        gs.charts()[player_idx].short_hash.as_str(),
        &payload.judgment_counts,
        &payload.rescore_counts,
        payload.rate,
        payload.used_cmod,
    )
}

pub fn eval_state_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
    lua_submit_allowed: impl Fn(&str) -> bool,
    course_submit_allowed: bool,
    fail_type_ok: bool,
) -> GrooveStatsEvalState
where
    RuntimeProfile: Deref<Target = profile_data::Profile> + GameplayProfileData,
{
    if player_idx >= gs.num_players().min(MAX_PLAYERS) {
        return GrooveStatsEvalState::default();
    }
    let chart = gs.charts()[player_idx].as_ref();
    let profile = gs.profiles()[player_idx].deref();
    let mut result = groovestats_eval_state_from_gameplay_parts(
        groovestats_eval_state_from_profile(
            chart,
            profile,
            gs.music_rate(),
            gs.autoplay_used(),
            gs.course_display_is_course_stage(),
            course_submit_allowed,
            fail_type_ok,
        ),
        GrooveStatsGameplayEvalInput {
            song_has_lua: gs.song().has_lua,
            lua_submit_allowed: lua_submit_allowed(chart.short_hash.as_str()),
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: gs.players()[player_idx].is_failing,
            life: gs.players()[player_idx].life,
            has_fail_time: gs.players()[player_idx].fail_time.is_some(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
        },
    );
    if result.should_set_manual_qr_url {
        result.state.manual_qr_url = manual_qr_url_from_runtime(gs, player_idx);
    }
    result.state
}

pub fn gameplay_submit_player_from_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
    side: profile_data::PlayerSide,
    profile_id: Option<String>,
    itl_score_hundredths: Option<u32>,
    lua_submit_allowed: impl Fn(&str) -> bool,
    fail_type_ok: bool,
) -> GrooveStatsGameplaySubmitPlayer
where
    RuntimeProfile: Deref<Target = profile_data::Profile> + GameplayProfileData,
{
    let profile = gs.profiles()[player_idx].deref();
    let chart = gs.charts()[player_idx].as_ref();
    let chart_hash = chart.short_hash.as_str();
    GrooveStatsGameplaySubmitPlayer {
        side,
        slot: profile_data::player_side_index(side) as u8 + 1,
        chart_hash: chart_hash.to_string(),
        username: profile.groovestats_username.clone(),
        profile_name: profile.display_name.clone(),
        profile_id,
        itl_score_hundredths,
        show_ex_score: profile.show_ex_score,
        api_key: profile.groovestats_api_key.clone(),
        is_pad_player: profile.groovestats_is_pad_player,
        invalid_reason: groovestats_submit_invalid_reason_from_profile(
            chart,
            gs.song().has_lua,
            lua_submit_allowed(chart_hash),
            profile,
            gs.music_rate(),
            fail_type_ok,
        ),
        song_completed_naturally: gs.song_completed_naturally(),
        is_failing: gs.players()[player_idx].is_failing,
        life: gs.players()[player_idx].life,
        has_fail_time: gs.players()[player_idx].fail_time.is_some(),
        course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
        payload: submit_player_payload_from_runtime(gs, player_idx),
    }
}

pub fn submit_gameplay_from_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
    A,
    I,
    L,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    input: GrooveStatsGameplaySubmitInput,
    mut active_profile_id_for_side: A,
    mut itl_score_hundredths_for_player: I,
    lua_submit_allowed: L,
    fail_type_ok: bool,
    cache_success: fn(&GrooveStatsSubmitPlayerJob, &GrooveStatsSubmitApiPlayer),
    after_player: fn(&GrooveStatsSubmitPlayerJob),
) -> bool
where
    RuntimeProfile: Deref<Target = profile_data::Profile> + GameplayProfileData,
    A: FnMut(profile_data::PlayerSide) -> Option<String>,
    I: FnMut(usize) -> Option<u32>,
    L: Fn(&str) -> bool,
{
    let players = (0..gs.num_players().min(MAX_PLAYERS)).map(|player_idx| {
        let side =
            profile_data::app_runtime::gameplay_side_for_player(gs.num_players(), player_idx);
        gameplay_submit_player_from_runtime(
            gs,
            player_idx,
            side,
            active_profile_id_for_side(side),
            itl_score_hundredths_for_player(player_idx),
            &lua_submit_allowed,
            fail_type_ok,
        )
    });

    submit_gameplay_players(input, players, cache_success, after_player)
}

#[inline(always)]
fn submit_fail_type_ok_from_app_runtime() -> bool {
    matches!(
        deadsync_config::runtime::get().default_fail_type,
        deadsync_config::theme::DefaultFailType::Immediate
            | deadsync_config::theme::DefaultFailType::ImmediateContinue
    )
}

fn refresh_submit_leaderboards_from_app_runtime(player: &GrooveStatsSubmitPlayerJob) {
    let runtime = crate::player_leaderboards::PlayerLeaderboardRuntime::from_app_runtime();
    runtime.invalidate_for_side(player.chart_hash.as_str(), player.side);
    runtime.get_or_fetch_for_side(
        player.chart_hash.as_str(),
        player.side,
        GROOVESTATS_SUBMIT_MAX_ENTRIES,
    );
}

fn cache_submit_success_from_app_runtime(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) {
    let cfg = deadsync_config::runtime::get();
    let plan = submit_unlock_plan_from_response(
        player,
        response,
        cfg.auto_download_unlocks,
        cfg.separate_unlocks_by_player,
    );
    if let Some(profile_id) = player.profile_id.as_deref() {
        for folders in &plan.itl_folder_groups {
            deadsync_profile::app_runtime::update_itl_unlock_folders(
                profile_id,
                folders.as_slice(),
            );
        }
    }
    for download in &plan.downloads {
        crate::runtime::queue_event_unlock_download(
            download.url.as_str(),
            download.download_name.as_str(),
            download.pack_name.as_str(),
        );
    }
    if let Some(update) = cache_update_from_submit_success(player, response, |imported| {
        let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
        deadsync_score::imported_score_chart_stats(
            imported,
            &song_cache,
            player.chart_hash.as_str(),
        )
    }) {
        deadsync_profile::app_runtime::cache_logged_gs_score_for_id(
            update.profile_id.as_str(),
            update.chart_hash.as_str(),
            update.score,
            update.username.as_str(),
            update.score_proves_nonquint_ex,
        );
    }
}

pub fn submit_gameplay_from_app_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
) -> bool
where
    RuntimeProfile: Deref<Target = profile_data::Profile> + GameplayProfileData,
{
    let cfg = deadsync_config::runtime::get();
    let service = crate::runtime::active_groovestats_service();
    submit_gameplay_from_runtime(
        gs,
        GrooveStatsGameplaySubmitInput {
            enabled: cfg.enable_groovestats,
            service,
            service_name: service_name(service),
            player_count: gs.num_players(),
            autoplay_used: gs.autoplay_used(),
            is_course_stage: gs.course_display_is_course_stage(),
            autosubmit_course_scores_individually: cfg.autosubmit_course_scores_individually,
        },
        deadsync_profile::runtime_active_local_profile_id_for_side,
        |player_idx| itl_current_score_hundredths_from_runtime(gs, player_idx),
        deadsync_score::lua_chart_submit_allowed,
        submit_fail_type_ok_from_app_runtime(),
        cache_submit_success_from_app_runtime,
        refresh_submit_leaderboards_from_app_runtime,
    )
}

pub fn retry_submit_from_app_runtime(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
) -> bool {
    let service = crate::runtime::active_groovestats_service();
    retry_submit_if_enabled(
        deadsync_config::runtime::get().enable_groovestats,
        chart_hash,
        side,
        manual,
        service,
        service_name(service),
        cache_submit_success_from_app_runtime,
        refresh_submit_leaderboards_from_app_runtime,
    )
}

pub fn retry_manual_submit_from_app_runtime(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> bool {
    retry_submit_from_app_runtime(chart_hash, side, true)
}

pub fn tick_auto_submit_retries_from_app_runtime() -> bool {
    let service = crate::runtime::active_groovestats_service();
    tick_auto_submit_retries_if_enabled(
        deadsync_config::runtime::get().enable_groovestats,
        service,
        service_name(service),
        cache_submit_success_from_app_runtime,
        refresh_submit_leaderboards_from_app_runtime,
    )
}

#[derive(Debug, Clone)]
pub struct GrooveStatsSubmitPlayerRequest {
    pub slot: u8,
    pub chart_hash: String,
    pub api_key: String,
    pub payload: GrooveStatsSubmitPlayerPayload,
}

#[derive(Debug)]
pub struct GrooveStatsSubmitPlayerJob {
    pub side: profile_data::PlayerSide,
    pub slot: u8,
    pub chart_hash: String,
    pub username: String,
    pub profile_name: String,
    pub profile_id: Option<String>,
    pub token: u64,
    pub itl_score_hundredths: Option<u32>,
    pub show_ex_score: bool,
    pub score_10000: u32,
    pub rate_hundredths: u32,
    pub comment: String,
}

#[derive(Debug, Clone)]
pub struct GrooveStatsSubmitPlayerDraft {
    pub side: profile_data::PlayerSide,
    pub slot: u8,
    pub chart_hash: String,
    pub username: String,
    pub profile_name: String,
    pub profile_id: Option<String>,
    pub itl_score_hundredths: Option<u32>,
    pub show_ex_score: bool,
    pub api_key: String,
    pub payload: GrooveStatsSubmitPlayerPayload,
}

#[derive(Debug, Clone, Copy)]
pub struct GrooveStatsGameplaySubmitInput {
    pub enabled: bool,
    pub service: Service,
    pub service_name: &'static str,
    pub player_count: usize,
    pub autoplay_used: bool,
    pub is_course_stage: bool,
    pub autosubmit_course_scores_individually: bool,
}

#[derive(Debug, Clone)]
pub struct GrooveStatsGameplaySubmitPlayer {
    pub side: profile_data::PlayerSide,
    pub slot: u8,
    pub chart_hash: String,
    pub username: String,
    pub profile_name: String,
    pub profile_id: Option<String>,
    pub itl_score_hundredths: Option<u32>,
    pub show_ex_score: bool,
    pub api_key: String,
    pub is_pad_player: bool,
    pub invalid_reason: Option<String>,
    pub song_completed_naturally: bool,
    pub is_failing: bool,
    pub life: f32,
    pub has_fail_time: bool,
    pub course_stage_life_submit_eligible: bool,
    pub payload: Option<GrooveStatsSubmitPlayerPayload>,
}

impl GrooveStatsSubmitPlayerDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        side: profile_data::PlayerSide,
        slot: u8,
        chart_hash: String,
        username: String,
        profile_name: String,
        profile_id: Option<String>,
        itl_score_hundredths: Option<u32>,
        show_ex_score: bool,
        api_key: String,
        payload: GrooveStatsSubmitPlayerPayload,
    ) -> Self {
        Self {
            side,
            slot,
            chart_hash,
            username,
            profile_name,
            profile_id,
            itl_score_hundredths,
            show_ex_score,
            api_key,
            payload,
        }
    }

    pub fn retry_entry(&self) -> GrooveStatsSubmitRetryEntry {
        GrooveStatsSubmitRetryEntry::new(
            self.side,
            self.slot,
            self.chart_hash.clone(),
            self.username.clone(),
            self.profile_name.clone(),
            self.profile_id.clone(),
            self.itl_score_hundredths,
            self.show_ex_score,
            self.api_key.clone(),
            self.payload.clone(),
        )
    }

    pub fn player_job(&self, token: u64) -> GrooveStatsSubmitPlayerJob {
        GrooveStatsSubmitPlayerJob {
            side: self.side,
            slot: self.slot,
            chart_hash: self.chart_hash.clone(),
            username: self.username.clone(),
            profile_name: self.profile_name.clone(),
            profile_id: self.profile_id.clone(),
            token,
            itl_score_hundredths: self.itl_score_hundredths,
            show_ex_score: self.show_ex_score,
            score_10000: self.payload.score,
            rate_hundredths: self.payload.rate,
            comment: self.payload.comment.clone(),
        }
    }

    pub fn player_request(&self) -> GrooveStatsSubmitPlayerRequest {
        GrooveStatsSubmitPlayerRequest {
            slot: self.slot,
            chart_hash: self.chart_hash.clone(),
            api_key: self.api_key.clone(),
            payload: self.payload.clone(),
        }
    }
}

pub fn submit_gameplay_players(
    input: GrooveStatsGameplaySubmitInput,
    players: impl IntoIterator<Item = GrooveStatsGameplaySubmitPlayer>,
    cache_success: fn(&GrooveStatsSubmitPlayerJob, &GrooveStatsSubmitApiPlayer),
    after_player: fn(&GrooveStatsSubmitPlayerJob),
) -> bool {
    let players: Vec<_> = players.into_iter().collect();
    for player in &players {
        reset_submit_ui_status(player.side, player.chart_hash.as_str());
        reset_submit_event_ui(player.side, player.chart_hash.as_str());
        reset_submit_retry(player.side, player.chart_hash.as_str());
    }

    match groovestats_autosubmit_session_decision(GrooveStatsAutosubmitSessionInput {
        enabled: input.enabled,
        player_count: input.player_count,
        autoplay_used: input.autoplay_used,
        is_course_stage: input.is_course_stage,
        autosubmit_course_scores_individually: input.autosubmit_course_scores_individually,
    }) {
        GrooveStatsAutosubmitSessionDecision::Submit => {}
        GrooveStatsAutosubmitSessionDecision::Skip { log } => {
            if let Some(log) = log {
                log_global_submit_skip(input.service_name, log);
            }
            return false;
        }
    }

    let mut drafts = Vec::with_capacity(players.len());
    for player in players {
        let decision = groovestats_autosubmit_player_decision(GrooveStatsAutosubmitPlayerInput {
            has_invalid_reason: player.invalid_reason.is_some(),
            is_pad_player: player.is_pad_player,
            song_completed_naturally: player.song_completed_naturally,
            is_failing: player.is_failing,
            life: player.life,
            has_fail_time: player.has_fail_time,
            course_stage_life_submit_eligible: player.course_stage_life_submit_eligible,
            api_key_present: !player.api_key.trim().is_empty(),
        });
        match decision.action {
            GrooveStatsAutosubmitPlayerAction::BuildPayload => {}
            GrooveStatsAutosubmitPlayerAction::SkipInvalidReason => {
                if let Some(reason) = player.invalid_reason {
                    warn_submit_skip(
                        input.service_name,
                        player.side,
                        player.chart_hash.as_str(),
                        reason.as_str(),
                    );
                }
                continue;
            }
            GrooveStatsAutosubmitPlayerAction::Skip { log } => {
                if let Some(log) = log {
                    log_player_submit_skip(
                        input.service_name,
                        player.side,
                        player.chart_hash.as_str(),
                        log,
                    );
                }
                continue;
            }
        }

        let Some(payload) = player.payload else {
            warn_submit_skip(
                input.service_name,
                player.side,
                player.chart_hash.as_str(),
                "failed to build submit payload",
            );
            continue;
        };
        drafts.push(GrooveStatsSubmitPlayerDraft::new(
            player.side,
            player.slot,
            player.chart_hash,
            player.username.trim().to_string(),
            player.profile_name,
            player.profile_id,
            player.itl_score_hundredths,
            player.show_ex_score,
            player.api_key.trim().to_string(),
            payload,
        ));
    }

    let Some(request) = begin_submit_request_from_drafts(drafts) else {
        return false;
    };
    spawn_submit_request(
        request,
        input.service,
        input.service_name,
        cache_success,
        after_player,
    );
    true
}

#[derive(Debug, Clone)]
pub struct GrooveStatsSubmitRequestParts {
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: JsonValue,
}

#[derive(Debug)]
pub struct GrooveStatsSubmitRequest {
    pub players: Vec<GrooveStatsSubmitPlayerJob>,
    pub parts: GrooveStatsSubmitRequestParts,
}

pub fn submit_request_parts(
    players: &[GrooveStatsSubmitPlayerRequest],
) -> GrooveStatsSubmitRequestParts {
    let mut body = JsonMap::with_capacity(players.len());
    let mut headers = Vec::with_capacity(players.len());
    let mut query = Vec::with_capacity(players.len() + 1);
    query.push((
        "maxLeaderboardResults".to_string(),
        GROOVESTATS_SUBMIT_MAX_ENTRIES.to_string(),
    ));

    for player in players {
        headers.push((
            format!("x-api-key-player-{}", player.slot),
            player.api_key.clone(),
        ));
        query.push((
            format!("chartHashP{}", player.slot),
            player.chart_hash.clone(),
        ));
        body.insert(
            format!("player{}", player.slot),
            serde_json::to_value(&player.payload)
                .expect("serialize GrooveStats submit player payload"),
        );
    }

    GrooveStatsSubmitRequestParts {
        headers,
        query,
        body: JsonValue::Object(body),
    }
}

pub fn submit_request_from_drafts(
    players: Vec<(GrooveStatsSubmitPlayerDraft, u64)>,
) -> GrooveStatsSubmitRequest {
    let request_players: Vec<_> = players
        .iter()
        .map(|(player, _)| player.player_request())
        .collect();
    GrooveStatsSubmitRequest {
        players: players
            .into_iter()
            .map(|(player, token)| player.player_job(token))
            .collect(),
        parts: submit_request_parts(&request_players),
    }
}

pub fn begin_submit_request_from_drafts(
    drafts: Vec<GrooveStatsSubmitPlayerDraft>,
) -> Option<GrooveStatsSubmitRequest> {
    if drafts.is_empty() {
        return None;
    }
    let players = drafts
        .into_iter()
        .map(|draft| {
            store_submit_retry(draft.retry_entry());
            let token = next_submit_ui_token();
            set_submit_ui_status(
                draft.side,
                draft.chart_hash.as_str(),
                token,
                GrooveStatsSubmitUiStatus::Submitting,
            );
            arm_submit_event_ui(draft.side, draft.chart_hash.as_str(), token);
            (draft, token)
        })
        .collect();
    Some(submit_request_from_drafts(players))
}

pub fn retry_submit_request(
    entry: &GrooveStatsSubmitRetryEntry,
    token: u64,
) -> GrooveStatsSubmitRequest {
    let player = GrooveStatsSubmitPlayerJob {
        side: entry.side,
        slot: entry.slot,
        chart_hash: entry.chart_hash.clone(),
        username: entry.username.clone(),
        profile_name: entry.profile_name.clone(),
        profile_id: entry.profile_id.clone(),
        token,
        itl_score_hundredths: entry.itl_score_hundredths,
        show_ex_score: entry.show_ex_score,
        score_10000: entry.payload.score,
        rate_hundredths: entry.payload.rate,
        comment: entry.payload.comment.clone(),
    };
    let request_player = GrooveStatsSubmitPlayerRequest {
        slot: entry.slot,
        chart_hash: entry.chart_hash.clone(),
        api_key: entry.api_key.clone(),
        payload: entry.payload.clone(),
    };
    GrooveStatsSubmitRequest {
        players: vec![player],
        parts: submit_request_parts(&[request_player]),
    }
}

#[derive(Debug, Clone)]
pub struct GrooveStatsSubmitRetryEntry {
    pub side: profile_data::PlayerSide,
    pub slot: u8,
    pub chart_hash: String,
    pub username: String,
    pub profile_name: String,
    pub profile_id: Option<String>,
    pub itl_score_hundredths: Option<u32>,
    pub show_ex_score: bool,
    pub api_key: String,
    pub payload: GrooveStatsSubmitPlayerPayload,
    retry_attempt: u8,
    next_retry_at: Option<Instant>,
}

impl GrooveStatsSubmitRetryEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        side: profile_data::PlayerSide,
        slot: u8,
        chart_hash: String,
        username: String,
        profile_name: String,
        profile_id: Option<String>,
        itl_score_hundredths: Option<u32>,
        show_ex_score: bool,
        api_key: String,
        payload: GrooveStatsSubmitPlayerPayload,
    ) -> Self {
        Self {
            side,
            slot,
            chart_hash,
            username,
            profile_name,
            profile_id,
            itl_score_hundredths,
            show_ex_score,
            api_key,
            payload,
            retry_attempt: 0,
            next_retry_at: None,
        }
    }
}

static GROOVESTATS_SUBMIT_RETRY: LazyLock<Mutex<SubmitRetryState<GrooveStatsSubmitRetryEntry>>> =
    LazyLock::new(|| Mutex::new(SubmitRetryState::default()));

#[inline(always)]
pub fn reset_submit_ui_status(side: profile_data::PlayerSide, chart_hash: &str) {
    deadsync_score::groovestats_reset_submit_ui_status(
        profile_data::player_side_index(side),
        chart_hash,
    );
}

#[inline(always)]
pub fn reset_submit_event_ui(side: profile_data::PlayerSide, chart_hash: &str) {
    deadsync_score::groovestats_reset_submit_event_ui(
        profile_data::player_side_index(side),
        chart_hash,
    );
}

#[inline(always)]
pub fn set_submit_ui_status(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) {
    deadsync_score::groovestats_set_submit_ui_status(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        status,
    );
}

#[inline(always)]
pub fn update_submit_ui_status_if_token(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) -> bool {
    deadsync_score::groovestats_update_submit_ui_status_if_token(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        status,
    )
}

#[inline(always)]
pub fn arm_submit_event_ui(side: profile_data::PlayerSide, chart_hash: &str, token: u64) {
    deadsync_score::groovestats_arm_submit_event_ui(
        profile_data::player_side_index(side),
        chart_hash,
        token,
    );
}

#[inline(always)]
pub fn update_submit_event_ui_if_token(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    event_progress: Vec<EventProgress>,
    record_banner: Option<GrooveStatsSubmitRecordBanner>,
) {
    deadsync_score::groovestats_update_submit_event_ui_if_token(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        event_progress,
        record_banner,
    );
}

#[inline(always)]
pub fn next_submit_ui_token() -> u64 {
    deadsync_score::groovestats_next_submit_ui_token()
}

#[inline(always)]
pub fn submit_ui_status_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<GrooveStatsSubmitUiStatus> {
    groovestats_submit_ui_status(profile_data::player_side_index(side), chart_hash)
}

#[inline(always)]
pub fn submit_event_progress_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Vec<EventProgress> {
    deadsync_score::groovestats_submit_event_progress(
        profile_data::player_side_index(side),
        chart_hash,
    )
}

#[inline(always)]
pub fn submit_record_banner_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<GrooveStatsSubmitRecordBanner> {
    deadsync_score::groovestats_submit_record_banner_ui(
        profile_data::player_side_index(side),
        chart_hash,
    )
}

#[inline(always)]
pub fn reset_submit_retry(side: profile_data::PlayerSide, chart_hash: &str) {
    GROOVESTATS_SUBMIT_RETRY.lock().unwrap().reset_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        |entry| entry.chart_hash.as_str(),
    );
}

#[inline(always)]
pub fn store_submit_retry(entry: GrooveStatsSubmitRetryEntry) {
    let side = entry.side;
    GROOVESTATS_SUBMIT_RETRY.lock().unwrap().upsert_by_key(
        profile_data::player_side_index(side),
        entry,
        |entry| entry.chart_hash.as_str(),
        GROOVESTATS_SUBMIT_RETRY_TRACKED_PER_SIDE,
    );
}

#[inline(always)]
pub fn take_ready_submit_retry(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
) -> Option<GrooveStatsSubmitRetryEntry> {
    GROOVESTATS_SUBMIT_RETRY.lock().unwrap().take_ready_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        manual,
        Instant::now(),
        |entry| entry.chart_hash.as_str(),
        |entry| &mut entry.next_retry_at,
    )
}

pub fn take_ready_submit_retry_request(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
    token: u64,
) -> Option<GrooveStatsSubmitRequest> {
    take_ready_submit_retry(chart_hash, side, manual)
        .map(|entry| retry_submit_request(&entry, token))
}

pub fn begin_ready_submit_retry_request(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
    service_name: &str,
) -> Option<GrooveStatsSubmitRequest> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    if !submit_ui_status_for_side(hash, side)?.can_retry() {
        return None;
    }
    let token = next_submit_ui_token();
    let request = take_ready_submit_retry_request(hash, side, manual, token)?;
    set_submit_ui_status(side, hash, token, GrooveStatsSubmitUiStatus::Submitting);
    arm_submit_event_ui(side, hash, token);
    log::debug!("Retrying {service_name} submit for {:?} ({}).", side, hash);
    Some(request)
}

pub fn retry_submit_if_enabled(
    enabled: bool,
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
    service: Service,
    service_name: &'static str,
    cache_success: fn(&GrooveStatsSubmitPlayerJob, &GrooveStatsSubmitApiPlayer),
    after_player: fn(&GrooveStatsSubmitPlayerJob),
) -> bool {
    if !enabled {
        return false;
    }
    let Some(request) = begin_ready_submit_retry_request(chart_hash, side, manual, service_name)
    else {
        return false;
    };
    spawn_submit_request(request, service, service_name, cache_success, after_player);
    true
}

pub fn tick_auto_submit_retries_if_enabled(
    enabled: bool,
    service: Service,
    service_name: &'static str,
    cache_success: fn(&GrooveStatsSubmitPlayerJob, &GrooveStatsSubmitApiPlayer),
    after_player: fn(&GrooveStatsSubmitPlayerJob),
) -> bool {
    let mut fired = false;
    for (hash, side, _) in due_auto_submit_retries() {
        if retry_submit_if_enabled(
            enabled,
            hash.as_str(),
            side,
            false,
            service,
            service_name,
            cache_success,
            after_player,
        ) {
            fired = true;
        }
    }
    fired
}

pub fn reject_submit_player_response(player: &GrooveStatsSubmitPlayerJob) -> bool {
    update_submit_ui_status_if_token(
        player.side,
        player.chart_hash.as_str(),
        player.token,
        GrooveStatsSubmitUiStatus::Rejected {
            reason: RejectReason::InvalidScore,
        },
    )
}

pub fn complete_submit_player_success(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> bool {
    update_submit_event_ui_if_token(
        player.side,
        player.chart_hash.as_str(),
        player.token,
        event_progress_from_submit_response(player, response),
        submit_record_banner_from_api(response, player.username.as_str(), player.show_ex_score),
    );
    let accepted = update_submit_ui_status_if_token(
        player.side,
        player.chart_hash.as_str(),
        player.token,
        GrooveStatsSubmitUiStatus::Submitted,
    );
    if accepted {
        reset_submit_retry(player.side, player.chart_hash.as_str());
    }
    accepted
}

pub fn complete_submit_player_failure(
    player: &GrooveStatsSubmitPlayerJob,
    status: GrooveStatsSubmitUiStatus,
) -> bool {
    let accepted = update_submit_ui_status_if_token(
        player.side,
        player.chart_hash.as_str(),
        player.token,
        status,
    );
    if accepted {
        record_submit_failure(player.side, player.chart_hash.as_str(), status);
    }
    accepted
}

#[inline(always)]
pub fn record_submit_failure(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    status: GrooveStatsSubmitUiStatus,
) {
    GROOVESTATS_SUBMIT_RETRY
        .lock()
        .unwrap()
        .record_failure_by_key(
            profile_data::player_side_index(side),
            chart_hash,
            status.can_retry(),
            GROOVESTATS_RETRY_MAX_ATTEMPTS,
            Instant::now(),
            |entry| entry.chart_hash.as_str(),
            |entry| &mut entry.retry_attempt,
            |entry| &mut entry.next_retry_at,
        );
}

#[inline(always)]
pub fn next_retry_remaining_secs(chart_hash: &str, side: profile_data::PlayerSide) -> Option<u32> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    GROOVESTATS_SUBMIT_RETRY
        .lock()
        .unwrap()
        .remaining_secs_by_key(
            profile_data::player_side_index(side),
            hash,
            Instant::now(),
            |entry| entry.chart_hash.as_str(),
            |entry| entry.next_retry_at,
        )
}

#[inline(always)]
pub fn next_retry_is_auto(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return false;
    }
    let attempt = {
        let lock = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
        let Some(attempt) = lock.retry_attempt_by_key(
            profile_data::player_side_index(side),
            hash,
            |entry| entry.chart_hash.as_str(),
            |entry| entry.retry_attempt,
        ) else {
            return false;
        };
        attempt
    };
    if attempt >= GROOVESTATS_RETRY_MAX_ATTEMPTS {
        return false;
    }
    matches!(
        groovestats_submit_ui_status(profile_data::player_side_index(side), hash),
        Some(s) if s.is_auto_retryable()
    )
}

#[inline(always)]
pub fn due_auto_submit_retries() -> Vec<(String, profile_data::PlayerSide, u8)> {
    let due = {
        let lock = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
        lock.due_retries(
            Instant::now(),
            |entry| entry.chart_hash.as_str(),
            |entry| entry.side,
            |entry| entry.retry_attempt,
            |entry| entry.next_retry_at,
        )
    };
    due.into_iter()
        .filter(|(hash, side, attempt)| {
            *attempt < GROOVESTATS_RETRY_MAX_ATTEMPTS
                && matches!(
                    groovestats_submit_ui_status(profile_data::player_side_index(*side), hash),
                    Some(status) if status.is_auto_retryable()
                )
        })
        .collect()
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

pub enum GrooveStatsSubmitPlayerResponse<'a> {
    Accepted(&'a GrooveStatsSubmitApiPlayer),
    Missing,
    HashMismatch { actual_chart_hash: &'a str },
}

pub fn submit_player_response_for_job<'a>(
    response: &'a GrooveStatsSubmitApiResponse,
    player: &GrooveStatsSubmitPlayerJob,
) -> GrooveStatsSubmitPlayerResponse<'a> {
    let Some(player_response) = response.player_for_slot(player.slot) else {
        return GrooveStatsSubmitPlayerResponse::Missing;
    };
    if !player_response.chart_hash.trim().is_empty()
        && !player_response
            .chart_hash
            .eq_ignore_ascii_case(player.chart_hash.as_str())
    {
        return GrooveStatsSubmitPlayerResponse::HashMismatch {
            actual_chart_hash: player_response.chart_hash.as_str(),
        };
    }
    GrooveStatsSubmitPlayerResponse::Accepted(player_response)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrooveStatsSubmitError {
    pub status: GrooveStatsSubmitUiStatus,
    pub message: String,
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

pub fn submit_error_from_request(
    service_name: &str,
    error: GrooveStatsSubmitRequestError,
) -> GrooveStatsSubmitError {
    let (status, message) = submit_error_status_and_message(service_name, &error);
    GrooveStatsSubmitError { status, message }
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

pub fn imported_player_score_from_submit_response(
    response: &GrooveStatsSubmitApiPlayer,
    username: &str,
    score_10000: f64,
    comment: &str,
) -> ImportedPlayerScore {
    ImportedPlayerScore {
        score_10000,
        comments: Some(comment.to_string()),
        is_fail: false,
        ex_evidence: gs_ex_evidence_from_leaderboard(
            response.ex_leaderboard.as_slice(),
            username,
            Some(comment),
        ),
    }
}

pub struct GrooveStatsSubmitCacheUpdate {
    pub profile_id: String,
    pub chart_hash: String,
    pub username: String,
    pub score: deadsync_score::CachedScore,
    pub score_proves_nonquint_ex: bool,
}

pub fn cache_update_from_submit_success<F>(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
    chart_stats: F,
) -> Option<GrooveStatsSubmitCacheUpdate>
where
    F: FnOnce(&ImportedPlayerScore) -> Option<GsLampChartStats>,
{
    let profile_id = player.profile_id.as_deref()?;
    let imported = imported_player_score_from_submit_response(
        response,
        player.username.as_str(),
        f64::from(player.score_10000),
        player.comment.as_str(),
    );
    let score_proves_nonquint_ex = imported.ex_evidence.proves_nonquint();
    let stats = chart_stats(&imported);
    Some(GrooveStatsSubmitCacheUpdate {
        profile_id: profile_id.to_string(),
        chart_hash: player.chart_hash.clone(),
        username: player.username.clone(),
        score: cached_score_from_imported_player_score(imported, stats),
        score_proves_nonquint_ex,
    })
}

pub fn event_progress_from_submit_response(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> Vec<ItlEventProgress> {
    let input = SubmitEventProgressInput {
        result: response.result.clone(),
        score_10000: player.score_10000,
        rate_hundredths: player.rate_hundredths,
        itl_score_hundredths: player.itl_score_hundredths,
        itl: response
            .itl
            .as_ref()
            .map(|event| submit_event_progress_from_api(event, event.itl_leaderboard.clone())),
        srpg: response
            .srpg
            .as_ref()
            .map(|event| submit_event_progress_from_api(event, event.srpg_leaderboard.clone())),
    };
    event_progress_from_submit(&input)
}

pub fn itl_unlock_folder_groups_from_submit_response<'a>(
    player: &GrooveStatsSubmitPlayerJob,
    response: &'a GrooveStatsSubmitApiPlayer,
) -> Vec<&'a [String]> {
    if player.itl_score_hundredths.is_none() {
        return Vec::new();
    }
    response
        .itl
        .as_ref()
        .and_then(|event| event.progress.as_ref())
        .map(|progress| {
            progress
                .quests_completed
                .iter()
                .map(|quest| quest.song_download_folders.as_slice())
                .collect()
        })
        .unwrap_or_default()
}

pub fn unlock_events_from_submit_response<'a>(
    player: &GrooveStatsSubmitPlayerJob,
    response: &'a GrooveStatsSubmitApiPlayer,
) -> Vec<&'a GrooveStatsSubmitApiEvent> {
    let mut events = Vec::with_capacity(2);
    if let Some(srpg) = response.srpg.as_ref() {
        events.push(srpg);
    }
    if player.itl_score_hundredths.is_some()
        && let Some(itl) = response.itl.as_ref()
    {
        events.push(itl);
    }
    events
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrooveStatsUnlockDownload {
    pub url: String,
    pub download_name: String,
    pub pack_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrooveStatsSubmitUnlockPlan {
    pub itl_folder_groups: Vec<Vec<String>>,
    pub downloads: Vec<GrooveStatsUnlockDownload>,
}

pub fn unlock_downloads_from_submit_event(
    event: &GrooveStatsSubmitApiEvent,
    profile_name: &str,
    separate_by_player: bool,
) -> Vec<GrooveStatsUnlockDownload> {
    let Some(progress) = event.progress.as_ref() else {
        return Vec::new();
    };
    let event_name = event_name_or_unknown(event.name.as_str());
    let profile_name = if profile_name.trim().is_empty() {
        "NoName"
    } else {
        profile_name.trim()
    };

    progress
        .quests_completed
        .iter()
        .filter_map(|quest| {
            let url = quest.song_download_url.trim();
            if url.is_empty() {
                return None;
            }
            let title = quest.title.trim();
            let (download_name, pack_name) = if separate_by_player {
                (
                    format!("[{event_name}] {title} - {profile_name}"),
                    format!("{event_name} Unlocks - {profile_name}"),
                )
            } else {
                (
                    format!("[{event_name}] {title}"),
                    format!("{event_name} Unlocks"),
                )
            };
            Some(GrooveStatsUnlockDownload {
                url: url.to_string(),
                download_name: download_name.trim_end().to_string(),
                pack_name,
            })
        })
        .collect()
}

pub fn submit_unlock_plan_from_response(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
    auto_download_unlocks: bool,
    separate_unlocks_by_player: bool,
) -> GrooveStatsSubmitUnlockPlan {
    let itl_folder_groups = itl_unlock_folder_groups_from_submit_response(player, response)
        .into_iter()
        .map(|group| group.to_vec())
        .collect();
    let downloads = if auto_download_unlocks {
        unlock_events_from_submit_response(player, response)
            .into_iter()
            .flat_map(|event| {
                unlock_downloads_from_submit_event(
                    event,
                    player.profile_name.as_str(),
                    separate_unlocks_by_player,
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    GrooveStatsSubmitUnlockPlan {
        itl_folder_groups,
        downloads,
    }
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

pub fn submit_request(
    job: &GrooveStatsSubmitRequest,
    service: Service,
    service_name: &str,
) -> Result<GrooveStatsSubmitApiResponse, GrooveStatsSubmitError> {
    match submit_score_request(
        service,
        &job.parts.headers,
        &job.parts.query,
        &job.parts.body,
    ) {
        Ok(success) => {
            if success.body_snippet.is_empty() {
                log::debug!("{service_name} submit success");
            } else {
                log::debug!(
                    "{service_name} submit success body='{}'",
                    success.body_snippet.as_str()
                );
            }
            Ok(success.response)
        }
        Err(error) => Err(submit_error_from_request(service_name, error)),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GrooveStatsSubmitRunSummary {
    pub accepted: usize,
    pub rejected: usize,
    pub failed: usize,
}

pub fn run_submit_request_with<S, A, P>(
    job: GrooveStatsSubmitRequest,
    service_name: &str,
    submit: S,
    mut accepted_player: A,
    mut after_player: P,
) -> GrooveStatsSubmitRunSummary
where
    S: FnOnce(
        &GrooveStatsSubmitRequest,
    ) -> Result<GrooveStatsSubmitApiResponse, GrooveStatsSubmitError>,
    A: FnMut(&GrooveStatsSubmitPlayerJob, &GrooveStatsSubmitApiPlayer),
    P: FnMut(&GrooveStatsSubmitPlayerJob),
{
    let mut summary = GrooveStatsSubmitRunSummary::default();
    match submit(&job) {
        Ok(response) => {
            for player in &job.players {
                let player_response = match submit_player_response_for_job(&response, player) {
                    GrooveStatsSubmitPlayerResponse::Accepted(player_response) => player_response,
                    GrooveStatsSubmitPlayerResponse::Missing => {
                        summary.rejected += 1;
                        reject_submit_player_response(player);
                        log::warn!(
                            "{service_name} submit response omitted player{} for {:?} ({}).",
                            player.slot,
                            player.side,
                            player.chart_hash
                        );
                        after_player(player);
                        continue;
                    }
                    GrooveStatsSubmitPlayerResponse::HashMismatch { actual_chart_hash } => {
                        summary.rejected += 1;
                        reject_submit_player_response(player);
                        log::warn!(
                            "{service_name} submit response hash mismatch for {:?}: expected {}, got {}.",
                            player.side,
                            player.chart_hash,
                            actual_chart_hash
                        );
                        after_player(player);
                        continue;
                    }
                };

                summary.accepted += 1;
                complete_submit_player_success(player, player_response);
                accepted_player(player, player_response);
                log::debug!(
                    "{service_name} submit succeeded for {:?} ({}) result='{}'",
                    player.side,
                    player.chart_hash,
                    player_response.result
                );
                after_player(player);
            }
        }
        Err(err) => {
            let status = err.status;
            for player in &job.players {
                summary.failed += 1;
                complete_submit_player_failure(player, status);
                log::warn!(
                    "{service_name} submit failed for {:?} ({}) status={:?}: {}",
                    player.side,
                    player.chart_hash,
                    status,
                    err.message
                );
                after_player(player);
            }
        }
    }
    summary
}

pub fn spawn_submit_request(
    job: GrooveStatsSubmitRequest,
    service: Service,
    service_name: &'static str,
    accepted_player: fn(&GrooveStatsSubmitPlayerJob, &GrooveStatsSubmitApiPlayer),
    after_player: fn(&GrooveStatsSubmitPlayerJob),
) {
    thread::spawn(move || {
        run_submit_request_with(
            job,
            service_name,
            |job| submit_request(job, service, service_name),
            accepted_player,
            after_player,
        );
    });
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
    #[serde(rename = "rpg")]
    pub srpg: Option<GrooveStatsSubmitApiEvent>,
    pub itl: Option<GrooveStatsSubmitApiEvent>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiEvent {
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    pub score_delta: i32,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    pub rate_delta: i32,
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
    #[serde(rename = "rpgLeaderboard", default)]
    pub srpg_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(default)]
    pub is_doubles: bool,
    pub progress: Option<GrooveStatsSubmitApiProgress>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrooveStatsSubmitApiProgress {
    #[serde(rename = "statImprovements", default)]
    pub stat_improvements: Vec<GrooveStatsSubmitApiStatImprovement>,
    #[serde(rename = "skillImprovements", default)]
    pub skill_improvements: Vec<String>,
    #[serde(rename = "questsCompleted", default)]
    pub quests_completed: Vec<GrooveStatsSubmitApiQuest>,
    #[serde(rename = "achievementsCompleted", default)]
    pub achievements_completed: Vec<GrooveStatsSubmitApiAchievement>,
}

pub fn submit_progress_from_api(progress: &GrooveStatsSubmitApiProgress) -> SubmitProgress {
    SubmitProgress {
        stat_improvements: progress
            .stat_improvements
            .iter()
            .map(|improvement| SubmitStatImprovement {
                name: improvement.name.clone(),
                gained: improvement.gained,
                current: improvement.current,
            })
            .collect(),
        skill_improvements: progress.skill_improvements.clone(),
        quests_completed: progress
            .quests_completed
            .iter()
            .map(|quest| SubmitQuest {
                title: quest.title.clone(),
                rewards: quest
                    .rewards
                    .iter()
                    .map(|reward| SubmitQuestReward {
                        reward_type: reward.reward_type.clone(),
                        description: reward.description.clone(),
                    })
                    .collect(),
            })
            .collect(),
        achievements_completed: progress
            .achievements_completed
            .iter()
            .map(|achievement| SubmitAchievement {
                title: achievement.title.clone(),
                rewards: achievement
                    .rewards
                    .iter()
                    .map(|reward| SubmitAchievementReward {
                        tier: reward.tier.clone(),
                        requirements: reward.requirements.clone(),
                        title_unlocked: reward.title_unlocked.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub fn submit_event_progress_from_api(
    event: &GrooveStatsSubmitApiEvent,
    leaderboard: Vec<LeaderboardApiEntry>,
) -> SubmitEventProgressData {
    SubmitEventProgressData {
        name: event.name.clone(),
        is_doubles: event.is_doubles,
        score_delta: event.score_delta,
        rate_delta: event.rate_delta,
        top_score_points: event.top_score_points,
        prev_top_score_points: event.prev_top_score_points,
        total_passes: event.total_passes,
        current_ranking_point_total: event.current_ranking_point_total,
        previous_ranking_point_total: event.previous_ranking_point_total,
        current_song_point_total: event.current_song_point_total,
        previous_song_point_total: event.previous_song_point_total,
        current_ex_point_total: event.current_ex_point_total,
        previous_ex_point_total: event.previous_ex_point_total,
        current_point_total: event.current_point_total,
        previous_point_total: event.previous_point_total,
        leaderboard: leaderboard_entries_from_api(leaderboard),
        progress: event.progress.as_ref().map(submit_progress_from_api),
    }
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

#[inline(always)]
pub const fn timing_windows_comment(setting: TimingWindowsOption) -> Option<&'static str> {
    match setting {
        TimingWindowsOption::None => None,
        TimingWindowsOption::WayOffs => Some("No WO"),
        TimingWindowsOption::DecentsAndWayOffs => Some("No Dec/WO"),
        TimingWindowsOption::FantasticsAndExcellents => Some("No Fan/Exc"),
    }
}

pub fn player_options_json(profile: &Profile) -> String {
    let (speed_mod_type, speed_mod) = match profile.scroll_speed {
        ScrollSpeedSetting::XMod(value) => (1, value),
        ScrollSpeedSetting::CMod(value) => (2, value),
        ScrollSpeedSetting::MMod(value) => (3, value),
    };
    let mut options = JsonMap::with_capacity(18);
    options.insert("SpeedModType".to_string(), JsonValue::from(speed_mod_type));
    options.insert(
        "SpeedMod".to_string(),
        JsonValue::from(f64::from(speed_mod)),
    );
    options.insert(
        "BackgroundFilter".to_string(),
        JsonValue::from(profile.background_filter.percent()),
    );
    options.insert(
        "HideTargets".to_string(),
        JsonValue::from(profile.hide_targets),
    );
    options.insert(
        "HideSongBG".to_string(),
        JsonValue::from(profile.hide_song_bg),
    );
    options.insert("HideCombo".to_string(), JsonValue::from(profile.hide_combo));
    options.insert(
        "HideLifebar".to_string(),
        JsonValue::from(profile.hide_lifebar),
    );
    options.insert("HideScore".to_string(), JsonValue::from(profile.hide_score));
    options.insert(
        "HideDanger".to_string(),
        JsonValue::from(profile.hide_danger),
    );
    options.insert(
        "HideComboExplosions".to_string(),
        JsonValue::from(profile.hide_combo_explosions),
    );
    options.insert(
        "ColumnFlashOnMiss".to_string(),
        JsonValue::from(profile.column_flash_on_miss),
    );
    options.insert(
        "SubtractiveScoring".to_string(),
        JsonValue::from(profile.subtractive_scoring),
    );
    options.insert("Mini".to_string(), JsonValue::from(profile.mini_percent));
    options.insert(
        "VisualDelay".to_string(),
        JsonValue::from(profile.visual_delay_ms),
    );
    options.insert("Cover".to_string(), JsonValue::from(profile.hide_song_bg));
    options.insert(
        "NoMines".to_string(),
        JsonValue::from(profile.remove_active_mask.contains(RemoveMask::NO_MINES)),
    );
    options.insert(
        "Reverse".to_string(),
        JsonValue::from(
            profile
                .scroll_option
                .contains(profile_data::ScrollOption::Reverse),
        ),
    );
    options.insert(
        "ShowFaPlusWindow".to_string(),
        JsonValue::from(profile.show_fa_plus_window),
    );
    options.insert(
        "ShowExScore".to_string(),
        JsonValue::from(profile.show_ex_score),
    );
    options.insert(
        "ShowFaPlusPane".to_string(),
        JsonValue::from(profile.show_fa_plus_pane),
    );
    serde_json::to_string(&JsonValue::Object(options))
        .expect("serialize GrooveStats playerOptions JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::timing::WindowCounts;
    use deadsync_score::{
        GrooveStatsSubmitRecordBanner, GrooveStatsSubmitUiStatus, RejectReason, ScoreImportEndpoint,
    };

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
    fn boogiestats_requires_both_service_flags() {
        assert!(!boogiestats_active(false, false));
        assert!(!boogiestats_active(false, true));
        assert!(!boogiestats_active(true, false));
        assert!(boogiestats_active(true, true));
    }

    #[test]
    fn active_service_uses_boogiestats_only_when_enabled() {
        assert_eq!(active_service(false, true), Service::GrooveStats);
        assert_eq!(active_service(true, false), Service::GrooveStats);
        assert_eq!(active_service(true, true), Service::BoogieStats);
    }

    #[test]
    fn service_names_match_backend() {
        assert_eq!(service_name(Service::GrooveStats), "GrooveStats");
        assert_eq!(service_name(Service::BoogieStats), "BoogieStats");
    }

    #[test]
    fn service_score_import_endpoints_match_backend() {
        assert_eq!(
            Service::GrooveStats.score_import_endpoint(),
            ScoreImportEndpoint::GrooveStats
        );
        assert_eq!(
            Service::BoogieStats.score_import_endpoint(),
            ScoreImportEndpoint::BoogieStats
        );
    }

    #[test]
    fn compact_f32_text_strips_trailing_decimal_zeroes() {
        assert_eq!(compact_f32_text(1.0), "1");
        assert_eq!(compact_f32_text(1.25), "1.25");
        assert_eq!(compact_f32_text(1.5), "1.5");
    }

    #[test]
    fn timing_windows_comment_matches_submit_labels() {
        assert_eq!(timing_windows_comment(TimingWindowsOption::None), None);
        assert_eq!(
            timing_windows_comment(TimingWindowsOption::WayOffs),
            Some("No WO")
        );
        assert_eq!(
            timing_windows_comment(TimingWindowsOption::DecentsAndWayOffs),
            Some("No Dec/WO")
        );
        assert_eq!(
            timing_windows_comment(TimingWindowsOption::FantasticsAndExcellents),
            Some("No Fan/Exc")
        );
    }

    #[test]
    fn player_options_json_includes_submit_relevant_mods() {
        let mut profile = Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(650.0);
        profile.background_filter = "55".parse().expect("background filter percent");
        profile.hide_targets = true;
        profile.hide_song_bg = true;
        profile.hide_combo = true;
        profile.hide_lifebar = true;
        profile.hide_score = true;
        profile.hide_danger = true;
        profile.hide_combo_explosions = true;
        profile.column_flash_on_miss = true;
        profile.subtractive_scoring = true;
        profile.mini_percent = 37;
        profile.visual_delay_ms = -12;
        profile.remove_active_mask |= RemoveMask::NO_MINES;
        profile.scroll_option = profile
            .scroll_option
            .union(profile_data::ScrollOption::Reverse);
        profile.show_fa_plus_window = true;
        profile.show_ex_score = true;
        profile.show_fa_plus_pane = true;

        let value: serde_json::Value =
            serde_json::from_str(&player_options_json(&profile)).expect("player options json");
        assert_eq!(value["SpeedModType"], 2);
        assert_eq!(value["SpeedMod"], 650.0);
        assert_eq!(value["BackgroundFilter"], 55);
        assert_eq!(value["HideTargets"], true);
        assert_eq!(value["HideSongBG"], true);
        assert_eq!(value["HideCombo"], true);
        assert_eq!(value["HideLifebar"], true);
        assert_eq!(value["HideScore"], true);
        assert_eq!(value["HideDanger"], true);
        assert_eq!(value["HideComboExplosions"], true);
        assert_eq!(value["ColumnFlashOnMiss"], true);
        assert_eq!(value["SubtractiveScoring"], true);
        assert_eq!(value["Mini"], 37);
        assert_eq!(value["VisualDelay"], -12);
        assert_eq!(value["Cover"], true);
        assert_eq!(value["NoMines"], true);
        assert_eq!(value["Reverse"], true);
        assert_eq!(value["ShowFaPlusWindow"], true);
        assert_eq!(value["ShowExScore"], true);
        assert_eq!(value["ShowFaPlusPane"], true);
    }

    #[test]
    fn judgment_counts_from_stats_omits_disabled_bad_windows() {
        let counts = judgment_counts_from_stats(
            WindowCounts {
                w0: 8,
                w1: 17,
                w2: 98,
                w3: 270,
                w4: 12,
                w5: 4,
                miss: 1,
            },
            [false, false, false, true, true],
            394,
            18,
            18,
            0,
            0,
            8,
            8,
        );

        assert_eq!(counts.fantastic_plus, 8);
        assert_eq!(counts.great, 270);
        assert_eq!(counts.decent, None);
        assert_eq!(counts.way_off, None);
        assert_eq!(counts.decent_count(), 0);
        assert_eq!(counts.way_off_count(), 0);
        assert_eq!(counts.total_steps, 394);
        assert_eq!(counts.holds_held, 18);
        assert_eq!(counts.rolls_held, 8);
    }

    #[test]
    fn rescore_target_helpers_count_fa_plus_and_final_windows() {
        let way_off = judgment::Judgment {
            time_error_ms: -18.0,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(-18.0, 1.0),
            grade: judgment::JudgeGrade::WayOff,
            window: Some(judgment::TimingWindow::W5),
            miss_because_held: false,
        };
        let great = judgment::Judgment {
            time_error_ms: -10.0,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(-10.0, 1.0),
            grade: judgment::JudgeGrade::Great,
            window: Some(judgment::TimingWindow::W3),
            miss_because_held: false,
        };
        let fa_plus = judgment::Judgment {
            time_error_ms: 1.0,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(1.0, 1.0),
            grade: judgment::JudgeGrade::Fantastic,
            window: Some(judgment::TimingWindow::W0),
            miss_because_held: false,
        };

        assert!(!final_result_counts_as_rescore_target(&way_off));
        assert!(final_result_counts_as_rescore_target(&great));

        let mut counts = GrooveStatsRescoreCounts::default();
        add_rescore_target(&mut counts, &way_off);
        add_rescore_target(&mut counts, &great);
        add_rescore_target(&mut counts, &fa_plus);

        assert_eq!(counts.fantastic_plus, 1);
        assert_eq!(counts.great, 1);
        assert_eq!(counts.way_off, 1);

        let counts = rescore_counts_from_judgments([(&great, &way_off), (&way_off, &fa_plus)]);
        assert_eq!(counts.fantastic_plus, 1);
        assert_eq!(counts.great, 1);
        assert_eq!(counts.way_off, 1);
    }

    #[test]
    fn submit_comment_formats_groovestats_parts() {
        let counts = GrooveStatsJudgmentCounts {
            fantastic_plus: 1,
            fantastic: 2,
            excellent: 3,
            great: 0,
            decent: Some(4),
            way_off: Some(5),
            miss: 6,
            total_steps: 20,
            holds_held: 1,
            total_holds: 2,
            mines_hit: 0,
            total_mines: 1,
            rolls_held: 0,
            total_rolls: 0,
        };

        assert_eq!(
            submit_comment(
                &counts,
                Some(99.5),
                1.5,
                TimingWindowsOption::DecentsAndWayOffs,
                ScrollSpeedSetting::CMod(650.0),
            ),
            "[DS], FA+, 99.50EX, 1.5x Rate, 2w, 3e, 4d, 5wo, 6m, No Dec/WO, C650"
        );
    }

    #[test]
    fn submit_comment_uses_prefix_when_no_parts() {
        assert_eq!(
            submit_comment(
                &GrooveStatsJudgmentCounts::default(),
                None,
                1.0,
                TimingWindowsOption::None,
                ScrollSpeedSetting::XMod(1.0),
            ),
            "[DS]"
        );
    }

    #[test]
    fn submit_player_payload_from_input_builds_wire_payload() {
        let mut scoring_counts = [0u32; judgment::JUDGE_GRADE_COUNT];
        scoring_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)] = 100;
        let mut profile = Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(650.0);

        let judgment_counts = GrooveStatsJudgmentCounts {
            fantastic_plus: 1,
            fantastic: 2,
            excellent: 3,
            great: 0,
            decent: Some(4),
            way_off: Some(5),
            miss: 6,
            total_steps: 20,
            holds_held: 1,
            total_holds: 2,
            mines_hit: 0,
            total_mines: 1,
            rolls_held: 0,
            total_rolls: 0,
        };
        let payload = submit_player_payload_from_input(GrooveStatsSubmitPlayerPayloadInput {
            scoring_counts: &scoring_counts,
            holds_held_for_score: 0,
            rolls_held_for_score: 0,
            mines_hit_for_score: 0,
            possible_grade_points: 500,
            music_rate: 1.5,
            judgment_counts,
            rescore_counts: GrooveStatsRescoreCounts::default(),
            fa_plus_ex_score: Some(99.5),
            timing_windows: TimingWindowsOption::DecentsAndWayOffs,
            scroll_speed: profile.scroll_speed,
            player_options: &profile,
        });

        assert_eq!(payload.rate, 150);
        assert_eq!(payload.score, 10000);
        assert!(payload.used_cmod);
        assert_eq!(
            payload.comment,
            "[DS], FA+, 99.50EX, 1.5x Rate, 2w, 3e, 4d, 5wo, 6m, No Dec/WO, C650"
        );
        let options: serde_json::Value =
            serde_json::from_str(&payload.player_options).expect("player options json");
        assert_eq!(options["SpeedModType"], 2);
        assert_eq!(options["SpeedMod"], 650.0);
    }

    #[test]
    fn submit_player_payload_from_gameplay_input_builds_wire_payload() {
        let mut scoring_counts = [0u32; judgment::JUDGE_GRADE_COUNT];
        scoring_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)] = 80;
        scoring_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Excellent)] = 20;
        let mut profile = Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(650.0);
        let notes: [Note; 0] = [];
        let note_times: [i64; 0] = [];
        let hold_end_times: [Option<i64>; 0] = [];

        let payload = submit_player_payload_from_gameplay_input(GrooveStatsGameplayPayloadInput {
            scoring_counts: &scoring_counts,
            holds_held_for_score: 0,
            rolls_held_for_score: 0,
            mines_hit_for_score: 0,
            possible_grade_points: 500,
            music_rate: 1.25,
            window_counts: WindowCounts {
                w0: 7,
                w1: 93,
                ..WindowCounts::default()
            },
            total_steps: 100,
            holds_held: 1,
            total_holds: 2,
            mines_hit: 3,
            total_mines: 4,
            rolls_held: 5,
            total_rolls: 6,
            notes: &notes,
            note_times: &note_times,
            hold_end_times: &hold_end_times,
            fail_time_ns: None,
            profile: &profile,
        });

        assert_eq!(payload.rate, 125);
        assert_eq!(payload.score, 9600);
        assert!(payload.used_cmod);
        assert_eq!(payload.judgment_counts.fantastic_plus, 7);
        assert_eq!(payload.judgment_counts.fantastic, 93);
        assert_eq!(payload.judgment_counts.holds_held, 1);
        assert_eq!(payload.judgment_counts.mines_hit, 3);
        assert_eq!(payload.judgment_counts.rolls_held, 5);
        assert_eq!(payload.rescore_counts.fantastic_plus, 0);
        assert_eq!(payload.rescore_counts.fantastic, 0);
        assert_eq!(payload.rescore_counts.excellent, 0);
        assert_eq!(payload.rescore_counts.great, 0);
        assert_eq!(payload.rescore_counts.decent, 0);
        assert_eq!(payload.rescore_counts.way_off, 0);
        assert!(payload.comment.contains("1.25x Rate"));
        assert!(payload.comment.contains("C650"));
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
            serde_json::from_str::<serde_json::Value>(&qr_login_uuid_message("ABC"))
                .expect("uuid message should be valid json"),
            serde_json::json!({"data":{"uuid":"ABC"},"event":"uuid"})
        );
    }

    #[test]
    fn player_leaderboards_url_uses_selected_backend() {
        assert_eq!(
            player_leaderboards_url(Service::GrooveStats),
            "https://apiservice.groovestats.com/api/?action=playerLeaderboards"
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
            "https://apiservice.groovestats.com/api/?action=scoreSubmit"
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
            "https://apiservice.groovestats.com/api/?action=newSession&chartHashVersion=3"
        );
    }

    #[test]
    fn validated_leaderboard_fetch_rejects_missing_inputs_before_network() {
        let missing_hash = fetch_validated_combined_player_leaderboards(
            Service::GrooveStats,
            "api-key",
            "player",
            " ",
            None,
            true,
            10,
        )
        .expect_err("missing hash should fail before request");
        assert_eq!(
            missing_hash.to_string(),
            "Missing chart hash for leaderboard request."
        );

        let missing_key = fetch_validated_combined_player_leaderboards(
            Service::GrooveStats,
            " ",
            "player",
            "deadbeef",
            None,
            true,
            10,
        )
        .expect_err("missing key should fail before request");
        assert_eq!(
            missing_key.to_string(),
            "Missing GrooveStats API key for leaderboard request."
        );
    }

    #[test]
    fn validated_score_fetch_rejects_missing_inputs_before_network() {
        let missing_hash = fetch_validated_player_score_import_result(
            ScoreImportEndpoint::GrooveStats,
            "api-key",
            "player",
            " ",
        )
        .expect_err("missing hash should fail before request");
        assert_eq!(
            missing_hash.to_string(),
            "Missing chart hash for score request."
        );

        let missing_user = fetch_validated_player_score_import_result(
            ScoreImportEndpoint::GrooveStats,
            "api-key",
            " ",
            "deadbeef",
        )
        .expect_err("missing username should fail before request");
        assert_eq!(
            missing_user.to_string(),
            "GrooveStats username is missing in profile configuration."
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
    fn probe_result_transition_selects_status_and_log_intent() {
        let services = Services {
            get_scores: true,
            leaderboard: false,
            auto_submit: true,
        };
        assert_eq!(
            connection_transition_from_probe_result(
                Service::GrooveStats,
                Ok(ConnectionStatus::Connected(services.clone()))
            ),
            ConnectionProbeTransition {
                status: ConnectionStatus::Connected(services.clone()),
                log: Some(ConnectionProbeLog::Connected {
                    service: Service::GrooveStats,
                    services
                }),
            }
        );

        assert_eq!(
            connection_transition_from_probe_result(
                Service::BoogieStats,
                Ok(ConnectionStatus::Error(ConnectionError::MachineOffline))
            ),
            ConnectionProbeTransition {
                status: ConnectionStatus::Error(ConnectionError::MachineOffline),
                log: Some(ConnectionProbeLog::MachineOffline {
                    service: Service::BoogieStats
                }),
            }
        );

        assert_eq!(
            connection_transition_from_probe_result(
                Service::BoogieStats,
                Ok(ConnectionStatus::Pending)
            ),
            ConnectionProbeTransition {
                status: ConnectionStatus::Pending,
                log: None,
            }
        );
    }

    #[test]
    fn probe_result_transition_maps_errors_to_log_intent() {
        assert_eq!(
            connection_transition_from_probe_result(
                Service::GrooveStats,
                Err(ConnectionProbeError::Timeout)
            ),
            ConnectionProbeTransition {
                status: ConnectionStatus::Error(ConnectionError::TimedOut),
                log: Some(ConnectionProbeLog::Timeout {
                    service: Service::GrooveStats
                }),
            }
        );

        assert_eq!(
            connection_transition_from_probe_result(
                Service::BoogieStats,
                Err(ConnectionProbeError::InvalidResponse(
                    "bad json".to_string()
                ))
            ),
            ConnectionProbeTransition {
                status: ConnectionStatus::Error(ConnectionError::InvalidResponse),
                log: Some(ConnectionProbeLog::InvalidResponse {
                    service: Service::BoogieStats,
                    error: "bad json".to_string()
                }),
            }
        );

        assert_eq!(
            connection_transition_from_probe_result(
                Service::GrooveStats,
                Err(ConnectionProbeError::CannotConnect("refused".to_string()))
            ),
            ConnectionProbeTransition {
                status: ConnectionStatus::Error(ConnectionError::CannotConnect),
                log: Some(ConnectionProbeLog::CannotConnect {
                    service: Service::GrooveStats,
                    error: "refused".to_string()
                }),
            }
        );
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

        let srpg = player.srpg.expect("srpg");
        assert_eq!(srpg.name, "RPG");
        assert_eq!(srpg.srpg_leaderboard[0].name, "CASEY");
        assert!(srpg.itl_leaderboard.is_empty());

        let itl = player.itl.expect("itl");
        assert_eq!(itl.name, "ITL");
        assert_eq!(itl.itl_leaderboard[0].name, "DREW");
        assert!(itl.itl_leaderboard[0].is_fail);
        assert!(itl.srpg_leaderboard.is_empty());
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
    fn imported_player_score_from_leaderboard_entries_uses_ex_evidence() {
        let gs_entries = vec![LeaderboardApiEntry {
            rank: 1,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 10_000.0,
            date: String::new(),
            is_rival: false,
            is_self: true,
            is_fail: false,
            comments: Some("[DS], FA+, 100.00EX, C875".to_string()),
        }];
        let ex_entries = vec![LeaderboardApiEntry {
            rank: 1,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 9_978.0,
            date: String::new(),
            is_rival: false,
            is_self: true,
            is_fail: false,
            comments: None,
        }];

        let imported = imported_player_score_from_leaderboard_entries(
            &gs_entries,
            &ex_entries,
            "perfecttaste",
        )
        .expect("self score should map");

        assert_eq!(imported.score_10000, 10_000.0);
        assert_eq!(
            imported.comments.as_deref(),
            Some("[DS], FA+, 100.00EX, C875")
        );
        assert!(!imported.is_fail);
        assert!(imported.ex_evidence.proves_nonquint());
    }

    #[test]
    fn imported_player_score_from_leaderboard_entries_requires_self_row() {
        let entries = vec![LeaderboardApiEntry {
            rank: 1,
            name: "Other".to_string(),
            machine_tag: None,
            score: 10_000.0,
            date: String::new(),
            is_rival: false,
            is_self: false,
            is_fail: false,
            comments: None,
        }];

        assert!(
            imported_player_score_from_leaderboard_entries(&entries, &[], "perfecttaste").is_none()
        );
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
                    srpg: Some(LeaderboardEventData {
                        name: " ".to_string(),
                        srpg_leaderboard: vec![leaderboard_entry(4, "PerfectTaste", 9000.0, true)],
                        itl_leaderboard: Vec::new(),
                    }),
                    itl: Some(LeaderboardEventData {
                        name: String::new(),
                        srpg_leaderboard: Vec::new(),
                        itl_leaderboard: vec![leaderboard_entry(42, "PerfectTaste", 8765.0, true)],
                    }),
                }),
            },
            "perfecttaste",
            true,
        );

        let imported = fetched.imported_score.as_ref().expect("imported score");
        assert_eq!(imported.score_10000, 9876.0);
        assert_eq!(imported.ex_evidence.leaderboard_score_10000, Some(9912.0));
        assert!(fetched.itl_self_found);
        assert_eq!(fetched.data.srpg_self_score, Some(9000));
        assert_eq!(fetched.data.itl_self_score, Some(8765));
        assert_eq!(fetched.data.itl_self_rank, Some(42));
        assert_eq!(fetched.data.panes.len(), 4);
        assert_eq!(fetched.data.panes[0].name, "GrooveStats");
        assert!(fetched.data.panes[0].is_ex);
        assert_eq!(fetched.data.panes[1].name, "GrooveStats");
        assert!(!fetched.data.panes[1].is_ex);
        assert_eq!(fetched.data.panes[2].name, "SRPG");
        assert_eq!(fetched.data.panes[3].name, "ITL");
        assert!(fetched.data.panes[3].is_ex);
    }

    #[test]
    fn insert_arrowcloud_panes_places_them_after_core_gs_panes() {
        let mut fetched = fetched_player_leaderboards_from_api(
            LeaderboardsApiResponse {
                player1: Some(LeaderboardApiPlayer {
                    is_ranked: true,
                    gs_leaderboard: vec![leaderboard_entry(3, "PerfectTaste", 9876.0, true)],
                    ex_leaderboard: vec![leaderboard_entry(2, "PerfectTaste", 9912.0, true)],
                    srpg: Some(LeaderboardEventData {
                        name: "RPG".to_string(),
                        srpg_leaderboard: vec![leaderboard_entry(4, "RPG", 9000.0, false)],
                        itl_leaderboard: Vec::new(),
                    }),
                    itl: None,
                }),
            },
            "perfecttaste",
            false,
        );

        insert_arrowcloud_panes(
            &mut fetched,
            vec![LeaderboardPane {
                name: "ArrowCloud".to_string(),
                entries: Vec::new(),
                is_ex: false,
                disabled: false,
                personalized: false,
                arrowcloud_kind: None,
            }],
        );

        assert_eq!(fetched.data.panes.len(), 4);
        assert_eq!(fetched.data.panes[0].name, "GrooveStats");
        assert_eq!(fetched.data.panes[1].name, "GrooveStats");
        assert_eq!(fetched.data.panes[2].name, "ArrowCloud");
        assert_eq!(fetched.data.panes[3].name, "SRPG");
    }

    #[test]
    fn player_score_import_result_from_api_includes_itl_self_data() {
        let result = player_score_import_result_from_api(
            LeaderboardsApiResponse {
                player1: Some(LeaderboardApiPlayer {
                    is_ranked: true,
                    gs_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 9876.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: Some("[DS], 2e".to_string()),
                    }],
                    ex_leaderboard: Vec::new(),
                    srpg: None,
                    itl: Some(LeaderboardEventData {
                        name: "ITL Online 2026".to_string(),
                        srpg_leaderboard: Vec::new(),
                        itl_leaderboard: vec![LeaderboardApiEntry {
                            rank: 42,
                            name: "PerfectTaste".to_string(),
                            machine_tag: None,
                            score: 9912.0,
                            date: String::new(),
                            is_rival: false,
                            is_self: true,
                            is_fail: false,
                            comments: None,
                        }],
                    }),
                }),
            },
            ScoreImportEndpoint::GrooveStats,
            "perfecttaste",
        );

        let score = result.score.expect("score");
        assert_eq!(score.score_10000, 9876.0);
        assert_eq!(score.comments.as_deref(), Some("[DS], 2e"));
        assert!(result.itl_self_found);
        assert_eq!(result.itl_self_score, Some(9912));
        assert_eq!(result.itl_self_rank, Some(42));
    }

    #[test]
    fn player_score_import_result_from_api_uses_ex_leaderboard_evidence() {
        let result = player_score_import_result_from_api(
            LeaderboardsApiResponse {
                player1: Some(LeaderboardApiPlayer {
                    is_ranked: true,
                    gs_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 10_000.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: Some("[DS], FA+, 100.00EX, C875".to_string()),
                    }],
                    ex_leaderboard: vec![LeaderboardApiEntry {
                        rank: 8,
                        name: "PerfectTaste".to_string(),
                        machine_tag: None,
                        score: 9_978.0,
                        date: String::new(),
                        is_rival: false,
                        is_self: true,
                        is_fail: false,
                        comments: None,
                    }],
                    srpg: None,
                    itl: None,
                }),
            },
            ScoreImportEndpoint::GrooveStats,
            "perfecttaste",
        );

        let score = result.score.expect("score");
        assert_eq!(score.ex_evidence.leaderboard_score_10000, Some(9_978.0));
        assert!(result.score_proves_nonquint_ex);
    }

    #[test]
    fn submit_payload_serializes_score_submit_shape() {
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
    fn submit_request_parts_use_score_submit_shape() {
        let payload = GrooveStatsSubmitPlayerPayload {
            rate: 150,
            score: 9_975,
            judgment_counts: GrooveStatsJudgmentCounts::default(),
            rescore_counts: GrooveStatsRescoreCounts::default(),
            used_cmod: true,
            comment: "[DS]".to_string(),
            player_options: "{}".to_string(),
        };
        let parts = submit_request_parts(&[
            GrooveStatsSubmitPlayerRequest {
                slot: 1,
                chart_hash: "hash-p1".to_string(),
                api_key: "key-p1".to_string(),
                payload: payload.clone(),
            },
            GrooveStatsSubmitPlayerRequest {
                slot: 2,
                chart_hash: "hash-p2".to_string(),
                api_key: "key-p2".to_string(),
                payload,
            },
        ]);

        assert_eq!(
            parts.headers,
            vec![
                ("x-api-key-player-1".to_string(), "key-p1".to_string()),
                ("x-api-key-player-2".to_string(), "key-p2".to_string()),
            ]
        );
        assert_eq!(
            parts.query,
            vec![
                (
                    "maxLeaderboardResults".to_string(),
                    GROOVESTATS_SUBMIT_MAX_ENTRIES.to_string(),
                ),
                ("chartHashP1".to_string(), "hash-p1".to_string()),
                ("chartHashP2".to_string(), "hash-p2".to_string()),
            ]
        );
        assert_eq!(parts.body["player1"]["score"], 9_975);
        assert_eq!(parts.body["player2"]["rate"], 150);
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
            srpg: None,
            itl: None,
        }
    }

    fn submit_player_job(itl_score_hundredths: Option<u32>) -> GrooveStatsSubmitPlayerJob {
        GrooveStatsSubmitPlayerJob {
            side: profile_data::PlayerSide::P1,
            slot: 1,
            chart_hash: "deadbeef".to_string(),
            username: "PerfectTaste".to_string(),
            profile_name: "Perfect Taste".to_string(),
            profile_id: Some("profile-guid".to_string()),
            token: 7,
            itl_score_hundredths,
            show_ex_score: true,
            score_10000: 9876,
            rate_hundredths: 150,
            comment: "[DS], FA+".to_string(),
        }
    }

    fn submit_player_with_unlocks() -> GrooveStatsSubmitApiPlayer {
        serde_json::from_value(serde_json::json!({
            "chartHash": "deadbeef",
            "result": "improved",
            "rpg": {
                "name": "SRPG",
                "progress": {
                    "questsCompleted": [
                        {
                            "title": "SRPG Quest",
                            "songDownloadUrl": "https://example.invalid/srpg.zip",
                            "songDownloadFolders": ["SRPG Pack"]
                        }
                    ]
                }
            },
            "itl": {
                "name": "ITL",
                "progress": {
                    "questsCompleted": [
                        {
                            "title": "ITL Quest",
                            "songDownloadUrl": "https://example.invalid/itl.zip",
                            "songDownloadFolders": ["ITL Pack", "ITL Bonus"]
                        },
                        {
                            "title": "ITL Empty",
                            "songDownloadUrl": "",
                            "songDownloadFolders": []
                        }
                    ]
                }
            }
        }))
        .expect("submit response with unlocks")
    }

    fn sample_player_payload() -> GrooveStatsSubmitPlayerPayload {
        GrooveStatsSubmitPlayerPayload {
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
        }
    }

    fn sample_retry_draft(
        hash: &str,
        side: profile_data::PlayerSide,
    ) -> GrooveStatsSubmitPlayerDraft {
        let slot = if side == profile_data::PlayerSide::P1 {
            1
        } else {
            2
        };
        GrooveStatsSubmitPlayerDraft::new(
            side,
            slot,
            hash.to_string(),
            "PerfectTaste".to_string(),
            "PerfectTaste".to_string(),
            None,
            None,
            true,
            "test-api-key".to_string(),
            sample_player_payload(),
        )
    }

    #[test]
    fn submit_gameplay_players_resets_state_before_disabled_skip() {
        fn ignore_success(_: &GrooveStatsSubmitPlayerJob, _: &GrooveStatsSubmitApiPlayer) {}
        fn ignore_after(_: &GrooveStatsSubmitPlayerJob) {}

        let side = profile_data::PlayerSide::P1;
        let hash = "gs-gameplay-disabled";
        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);
        set_submit_ui_status(side, hash, 77, GrooveStatsSubmitUiStatus::Submitted);
        store_submit_retry(sample_retry_draft(hash, side).retry_entry());

        let fired = submit_gameplay_players(
            GrooveStatsGameplaySubmitInput {
                enabled: false,
                service: Service::GrooveStats,
                service_name: service_name(Service::GrooveStats),
                player_count: 1,
                autoplay_used: false,
                is_course_stage: false,
                autosubmit_course_scores_individually: false,
            },
            vec![GrooveStatsGameplaySubmitPlayer {
                side,
                slot: 1,
                chart_hash: hash.to_string(),
                username: "PerfectTaste".to_string(),
                profile_name: "PerfectTaste".to_string(),
                profile_id: None,
                itl_score_hundredths: None,
                show_ex_score: true,
                api_key: "test-api-key".to_string(),
                is_pad_player: true,
                invalid_reason: None,
                song_completed_naturally: true,
                is_failing: false,
                life: 1.0,
                has_fail_time: false,
                course_stage_life_submit_eligible: true,
                payload: Some(sample_player_payload()),
            }],
            ignore_success,
            ignore_after,
        );

        assert!(!fired);
        assert_eq!(submit_ui_status_for_side(hash, side), None);
        assert!(take_ready_submit_retry(hash, side, true).is_none());
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
    fn imported_player_score_from_submit_response_uses_ex_evidence() {
        let response = submit_player(
            "improved",
            Vec::new(),
            vec![leaderboard_entry(1, "PerfectTaste", 9_978.0, true)],
        );
        let imported = imported_player_score_from_submit_response(
            &response,
            "perfecttaste",
            10_000.0,
            "[DS], FA+, 100.00EX, C875",
        );

        assert_eq!(imported.score_10000, 10_000.0);
        assert_eq!(
            imported.comments.as_deref(),
            Some("[DS], FA+, 100.00EX, C875")
        );
        assert!(!imported.is_fail);
        assert!(imported.ex_evidence.proves_nonquint());
    }

    #[test]
    fn itl_unlock_folder_groups_require_itl_submit_score() {
        let response = submit_player_with_unlocks();
        let groups = itl_unlock_folder_groups_from_submit_response(
            &submit_player_job(Some(12_345)),
            &response,
        );
        assert_eq!(
            groups,
            vec![
                &["ITL Pack".to_string(), "ITL Bonus".to_string()][..],
                &[][..]
            ]
        );

        let groups =
            itl_unlock_folder_groups_from_submit_response(&submit_player_job(None), &response);
        assert!(groups.is_empty());
    }

    #[test]
    fn unlock_events_from_submit_response_keeps_srpg_and_accepted_itl() {
        let response = submit_player_with_unlocks();
        let events =
            unlock_events_from_submit_response(&submit_player_job(Some(12_345)), &response);
        assert_eq!(
            events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["SRPG", "ITL"]
        );

        let events = unlock_events_from_submit_response(&submit_player_job(None), &response);
        assert_eq!(
            events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["SRPG"]
        );
    }

    #[test]
    fn submit_unlock_plan_collects_itl_folders_and_downloads() {
        let response = submit_player_with_unlocks();
        let plan = submit_unlock_plan_from_response(
            &submit_player_job(Some(12_345)),
            &response,
            true,
            true,
        );

        assert_eq!(
            plan.itl_folder_groups,
            vec![
                vec!["ITL Pack".to_string(), "ITL Bonus".to_string()],
                Vec::<String>::new()
            ]
        );
        assert_eq!(
            plan.downloads
                .iter()
                .map(|download| (
                    download.url.as_str(),
                    download.download_name.as_str(),
                    download.pack_name.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "https://example.invalid/srpg.zip",
                    "[SRPG] SRPG Quest - Perfect Taste",
                    "SRPG Unlocks - Perfect Taste"
                ),
                (
                    "https://example.invalid/itl.zip",
                    "[ITL] ITL Quest - Perfect Taste",
                    "ITL Unlocks - Perfect Taste"
                )
            ]
        );
    }

    #[test]
    fn submit_unlock_plan_respects_download_gate_and_itl_acceptance() {
        let response = submit_player_with_unlocks();
        let plan =
            submit_unlock_plan_from_response(&submit_player_job(None), &response, false, false);

        assert!(plan.itl_folder_groups.is_empty());
        assert!(plan.downloads.is_empty());
    }

    #[test]
    fn submit_ui_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "gs-course-status-first";
        let second = "gs-course-status-second";
        reset_submit_ui_status(side, first);
        reset_submit_ui_status(side, second);
        reset_submit_event_ui(side, first);
        reset_submit_event_ui(side, second);

        set_submit_ui_status(side, first, 11, GrooveStatsSubmitUiStatus::Submitting);
        set_submit_ui_status(side, second, 12, GrooveStatsSubmitUiStatus::Submitted);
        arm_submit_event_ui(side, first, 11);
        arm_submit_event_ui(side, second, 12);
        update_submit_event_ui_if_token(
            side,
            first,
            11,
            Vec::new(),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest),
        );
        update_submit_event_ui_if_token(
            side,
            second,
            12,
            Vec::new(),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord),
        );

        assert_eq!(
            submit_ui_status_for_side(first, side),
            Some(GrooveStatsSubmitUiStatus::Submitting)
        );
        assert_eq!(
            submit_ui_status_for_side(second, side),
            Some(GrooveStatsSubmitUiStatus::Submitted)
        );
        assert_eq!(
            submit_record_banner_for_side(first, side),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest)
        );
        assert_eq!(
            submit_record_banner_for_side(second, side),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord)
        );
        assert!(update_submit_ui_status_if_token(
            side,
            first,
            11,
            GrooveStatsSubmitUiStatus::TimedOut,
        ));
        assert!(!update_submit_ui_status_if_token(
            side,
            first,
            12,
            GrooveStatsSubmitUiStatus::Submitted,
        ));
        assert_eq!(
            submit_ui_status_for_side(first, side),
            Some(GrooveStatsSubmitUiStatus::TimedOut)
        );
        assert_eq!(
            submit_ui_status_for_side(second, side),
            Some(GrooveStatsSubmitUiStatus::Submitted)
        );

        reset_submit_ui_status(side, first);
        reset_submit_ui_status(side, second);
        reset_submit_event_ui(side, first);
        reset_submit_event_ui(side, second);
    }

    #[test]
    fn submit_retry_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "gs-course-retry-first";
        let second = "gs-course-retry-second";
        reset_submit_ui_status(side, first);
        reset_submit_ui_status(side, second);
        reset_submit_retry(side, first);
        reset_submit_retry(side, second);

        store_submit_retry(sample_retry_draft(first, side).retry_entry());
        store_submit_retry(sample_retry_draft(second, side).retry_entry());
        set_submit_ui_status(side, first, 21, GrooveStatsSubmitUiStatus::TimedOut);
        set_submit_ui_status(side, second, 22, GrooveStatsSubmitUiStatus::NetworkError);

        record_submit_failure(side, first, GrooveStatsSubmitUiStatus::TimedOut);
        record_submit_failure(side, second, GrooveStatsSubmitUiStatus::NetworkError);

        assert!(next_retry_remaining_secs(first, side).is_some());
        assert!(next_retry_is_auto(first, side));
        assert!(next_retry_remaining_secs(second, side).is_some());
        assert!(!next_retry_is_auto(second, side));

        reset_submit_retry(side, first);
        assert_eq!(next_retry_remaining_secs(first, side), None);
        assert!(next_retry_remaining_secs(second, side).is_some());

        reset_submit_ui_status(side, first);
        reset_submit_ui_status(side, second);
        reset_submit_retry(side, first);
        reset_submit_retry(side, second);
    }

    #[test]
    fn begin_ready_submit_retry_request_arms_ui_and_consumes_retry() {
        let side = profile_data::PlayerSide::P1;
        let hash = "gs-begin-ready-retry";
        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);

        store_submit_retry(sample_retry_draft(hash, side).retry_entry());
        set_submit_ui_status(side, hash, 31, GrooveStatsSubmitUiStatus::TimedOut);

        let request = begin_ready_submit_retry_request(hash, side, true, "GrooveStats")
            .expect("ready retry request");
        assert_eq!(request.players.len(), 1);
        assert_eq!(request.players[0].chart_hash, hash);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(GrooveStatsSubmitUiStatus::Submitting)
        );
        assert!(begin_ready_submit_retry_request(hash, side, true, "GrooveStats").is_none());

        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn begin_submit_request_from_drafts_stores_retry_and_arms_ui() {
        let side = profile_data::PlayerSide::P1;
        let hash = "gs-begin-submit-draft";
        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);

        let request =
            begin_submit_request_from_drafts(vec![sample_retry_draft(hash, side)]).unwrap();
        assert_eq!(request.players.len(), 1);
        assert_eq!(request.players[0].chart_hash, hash);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(GrooveStatsSubmitUiStatus::Submitting)
        );

        assert!(complete_submit_player_failure(
            &request.players[0],
            GrooveStatsSubmitUiStatus::TimedOut,
        ));
        assert!(next_retry_remaining_secs(hash, side).is_some());

        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn run_submit_request_with_accepts_player_and_runs_after_hook() {
        let side = profile_data::PlayerSide::P1;
        let hash = "gs-run-submit-success";
        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);

        let request =
            begin_submit_request_from_drafts(vec![sample_retry_draft(hash, side)]).unwrap();
        let mut response_player = submit_player(
            "improved",
            vec![leaderboard_entry(3, "PerfectTaste", 9876.0, true)],
            Vec::new(),
        );
        response_player.chart_hash = hash.to_string();
        let mut accepted = 0;
        let mut after = 0;
        let summary = run_submit_request_with(
            request,
            "GrooveStats",
            |_| {
                Ok(GrooveStatsSubmitApiResponse {
                    error: String::new(),
                    player1: Some(response_player),
                    player2: None,
                })
            },
            |_, _| accepted += 1,
            |_| after += 1,
        );

        assert_eq!(
            summary,
            GrooveStatsSubmitRunSummary {
                accepted: 1,
                rejected: 0,
                failed: 0
            }
        );
        assert_eq!(accepted, 1);
        assert_eq!(after, 1);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(GrooveStatsSubmitUiStatus::Submitted)
        );
        assert_eq!(
            submit_record_banner_for_side(hash, side),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest)
        );

        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn run_submit_request_with_records_request_failure_and_runs_after_hook() {
        let side = profile_data::PlayerSide::P1;
        let hash = "gs-run-submit-failure";
        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);

        let request =
            begin_submit_request_from_drafts(vec![sample_retry_draft(hash, side)]).unwrap();
        let mut accepted = 0;
        let mut after = 0;
        let summary = run_submit_request_with(
            request,
            "GrooveStats",
            |_| {
                Err(GrooveStatsSubmitError {
                    status: GrooveStatsSubmitUiStatus::TimedOut,
                    message: "timed out".to_string(),
                })
            },
            |_, _| accepted += 1,
            |_| after += 1,
        );

        assert_eq!(
            summary,
            GrooveStatsSubmitRunSummary {
                accepted: 0,
                rejected: 0,
                failed: 1
            }
        );
        assert_eq!(accepted, 0);
        assert_eq!(after, 1);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(GrooveStatsSubmitUiStatus::TimedOut)
        );
        assert!(next_retry_remaining_secs(hash, side).is_some());

        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn complete_submit_player_success_updates_ui_and_resets_retry() {
        let side = profile_data::PlayerSide::P1;
        let hash = "gs-complete-submit-success";
        let mut player = submit_player_job(Some(12_345));
        player.chart_hash = hash.to_string();
        let response = submit_player(
            "improved",
            vec![leaderboard_entry(3, "PerfectTaste", 9876.0, true)],
            Vec::new(),
        );
        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);

        store_submit_retry(sample_retry_draft(hash, side).retry_entry());
        set_submit_ui_status(side, hash, 3, GrooveStatsSubmitUiStatus::TimedOut);
        record_submit_failure(side, hash, GrooveStatsSubmitUiStatus::TimedOut);
        assert!(next_retry_remaining_secs(hash, side).is_some());
        set_submit_ui_status(
            side,
            hash,
            player.token,
            GrooveStatsSubmitUiStatus::Submitting,
        );
        arm_submit_event_ui(side, hash, player.token);

        assert!(complete_submit_player_success(&player, &response));
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(GrooveStatsSubmitUiStatus::Submitted)
        );
        assert_eq!(
            submit_record_banner_for_side(hash, side),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest)
        );
        assert_eq!(next_retry_remaining_secs(hash, side), None);

        reset_submit_ui_status(side, hash);
        reset_submit_event_ui(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn complete_submit_player_failure_records_retry() {
        let side = profile_data::PlayerSide::P1;
        let hash = "gs-complete-submit-failure";
        let mut player = submit_player_job(None);
        player.chart_hash = hash.to_string();
        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);

        store_submit_retry(sample_retry_draft(hash, side).retry_entry());
        set_submit_ui_status(
            side,
            hash,
            player.token,
            GrooveStatsSubmitUiStatus::Submitting,
        );

        assert!(complete_submit_player_failure(
            &player,
            GrooveStatsSubmitUiStatus::TimedOut
        ));
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(GrooveStatsSubmitUiStatus::TimedOut)
        );
        assert!(next_retry_remaining_secs(hash, side).is_some());

        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn reject_submit_player_response_marks_invalid_score() {
        let side = profile_data::PlayerSide::P1;
        let hash = "gs-reject-submit-response";
        let mut player = submit_player_job(None);
        player.chart_hash = hash.to_string();
        reset_submit_ui_status(side, hash);

        set_submit_ui_status(
            side,
            hash,
            player.token,
            GrooveStatsSubmitUiStatus::Submitting,
        );

        assert!(reject_submit_player_response(&player));
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore
            })
        );

        reset_submit_ui_status(side, hash);
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
                        "skillImprovements": [
                            "Gained 150 EXP and reached Level 12"
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
        assert_eq!(
            progress.skill_improvements[0],
            "Gained 150 EXP and reached Level 12"
        );
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

        let progress_data = submit_event_progress_from_api(itl, itl.itl_leaderboard.clone());
        assert_eq!(progress_data.name, "ITL");
        assert!(progress_data.is_doubles);
        assert_eq!(progress_data.score_delta, -123);
        assert_eq!(progress_data.top_score_points, 42);
        assert_eq!(progress_data.leaderboard[0].name, "DREW");
        let score_progress = progress_data.progress.as_ref().expect("score progress");
        assert_eq!(score_progress.stat_improvements[0].name, "clearType");
        assert_eq!(score_progress.stat_improvements[0].gained, 2);
        assert_eq!(
            score_progress.skill_improvements[0],
            "Gained 150 EXP and reached Level 12"
        );
        assert_eq!(score_progress.quests_completed[0].title, "Unlock One");
        assert_eq!(
            score_progress.quests_completed[0].rewards[0].description,
            "Song A"
        );
        assert_eq!(
            score_progress.achievements_completed[0].rewards[0].title_unlocked,
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
