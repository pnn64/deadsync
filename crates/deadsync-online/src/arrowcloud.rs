use crate::OnlineRequestError;
use deadsync_net::{self as network, NetworkError};
use deadsync_score::ArrowCloudLeaderboard;
use serde::Deserializer;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const ARROWCLOUD_API_BASE_URL: &str = "https://api.arrowcloud.dance";
const ARROWCLOUD_USER_URL: &str = "https://api.arrowcloud.dance/user";
const DEVICE_LOGIN_BASE: &str = "https://api.arrowcloud.dance/device-login";
const DEVICE_LOGIN_POLL_INTERVAL_MIN_S: f32 = 1.0;
const DEVICE_LOGIN_POLL_INTERVAL_MAX_S: f32 = 10.0;
const DEVICE_LOGIN_POLL_INTERVAL_DEFAULT_S: f32 = 3.0;
pub const ARROWCLOUD_BULK_MAX_HASHES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionError {
    Disabled,
    TimedOut,
    HostBlocked,
    CannotConnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Pending,
    Connected,
    Error(ConnectionError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProbeError {
    pub connection_error: ConnectionError,
    pub message: String,
}

impl std::fmt::Display for ConnectionProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message.as_str())
    }
}

impl std::error::Error for ConnectionProbeError {}

pub fn classify_connection_error(message: &str) -> ConnectionError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        return ConnectionError::TimedOut;
    }
    if lower.contains("blocked") || lower.contains("forbidden") || lower.contains("403") {
        return ConnectionError::HostBlocked;
    }
    ConnectionError::CannotConnect
}

pub fn connection_error_from_network_error(error: &NetworkError) -> ConnectionError {
    match error {
        NetworkError::Timeout => ConnectionError::TimedOut,
        NetworkError::HttpStatus(403) => ConnectionError::HostBlocked,
        NetworkError::HttpStatus(_) | NetworkError::Decode(_) => ConnectionError::CannotConnect,
        NetworkError::Request(message) => classify_connection_error(message),
    }
}

#[inline(always)]
pub const fn api_base_url() -> &'static str {
    ARROWCLOUD_API_BASE_URL
}

#[inline(always)]
pub const fn user_url() -> &'static str {
    ARROWCLOUD_USER_URL
}

pub fn submit_url(chart_hash: &str) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    Some(format!(
        "{}/v1/chart/{hash}/play",
        ARROWCLOUD_API_BASE_URL.trim_end_matches('/')
    ))
}

pub fn leaderboards_url(chart_hash: &str) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    Some(format!(
        "{}/v1/chart/{hash}/leaderboards",
        ARROWCLOUD_API_BASE_URL.trim_end_matches('/')
    ))
}

pub fn player_leaderboards_url() -> String {
    format!(
        "{}/player-leaderboards.php",
        ARROWCLOUD_API_BASE_URL.trim_end_matches('/')
    )
}

pub fn fetch_player_leaderboards(
    api_key: &str,
    chart_hash: &str,
) -> Result<crate::groovestats::LeaderboardsApiResponse, OnlineRequestError> {
    let api_url = player_leaderboards_url();
    let response = network::get_agent()
        .get(&api_url)
        .header("x-api-key-player-1", api_key)
        .query("chartHashP1", chart_hash)
        .call()
        .map_err(network::error_from_ureq)
        .map_err(OnlineRequestError::from)?;
    if response.status().as_u16() != 200 {
        return Err(OnlineRequestError::HttpStatus(response.status().as_u16()));
    }
    network::read_json_body(response).map_err(OnlineRequestError::from)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowCloudSubmitRequestSuccess {
    pub status: u16,
    pub body_snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrowCloudSubmitRequestError {
    InvalidRequest { message: String },
    Transport { message: String, timed_out: bool },
    Http { status: u16, body_snippet: String },
}

pub fn submit_score_request(
    api_key: &str,
    payload: &ArrowCloudPayload,
) -> Result<ArrowCloudSubmitRequestSuccess, ArrowCloudSubmitRequestError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(ArrowCloudSubmitRequestError::InvalidRequest {
            message: "missing ArrowCloud API key".to_string(),
        });
    }
    let Some(url) = submit_url(payload.hash.as_str()) else {
        return Err(ArrowCloudSubmitRequestError::InvalidRequest {
            message: "missing chart hash".to_string(),
        });
    };

    let bearer = format!("Bearer {api_key}");
    let response = network::get_agent()
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", &bearer)
        .send_json(payload)
        .map_err(|error| {
            let message = format!("network error: {error}");
            ArrowCloudSubmitRequestError::Transport {
                timed_out: network::is_timeout_message(message.as_str()),
                message,
            }
        })?;
    let status = response.status();
    let status_code = status.as_u16();
    let body = network::read_text_body_or_empty(response);
    let body_snippet = network::log_body_snippet(body.as_str());
    if status.is_success() {
        return Ok(ArrowCloudSubmitRequestSuccess {
            status: status_code,
            body_snippet,
        });
    }

    Err(ArrowCloudSubmitRequestError::Http {
        status: status_code,
        body_snippet,
    })
}

pub fn legacy_leaderboards_url(chart_hash: &str) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    Some(format!(
        "{}/chart/{hash}/leaderboards",
        ARROWCLOUD_API_BASE_URL.trim_end_matches('/')
    ))
}

#[inline(always)]
pub fn retrieve_scores_url() -> String {
    format!(
        "{}/v1/retrieve-scores",
        ARROWCLOUD_API_BASE_URL.trim_end_matches('/')
    )
}

pub fn check_connection() -> Result<ConnectionStatus, NetworkError> {
    network::get_agent()
        .get(api_base_url())
        .call()
        .map_err(network::error_from_ureq)?;
    Ok(ConnectionStatus::Connected)
}

pub fn probe_connection() -> Result<ConnectionStatus, ConnectionProbeError> {
    check_connection().map_err(ConnectionProbeError::from)
}

impl From<NetworkError> for ConnectionProbeError {
    fn from(error: NetworkError) -> Self {
        Self {
            connection_error: connection_error_from_network_error(&error),
            message: error.to_string(),
        }
    }
}

fn get_arrowcloud_json<T: DeserializeOwned>(
    api_url: &str,
    api_key: Option<&str>,
    page: Option<u32>,
) -> Result<Option<T>, OnlineRequestError> {
    let mut request = network::get_agent().get(api_url);
    if let Some(page) = page.filter(|page| *page > 1) {
        let page = page.to_string();
        request = request.query("page", &page);
    }
    if let Some(api_key) = api_key.map(str::trim).filter(|api_key| !api_key.is_empty()) {
        let bearer = format!("Bearer {api_key}");
        request = request.header("Authorization", &bearer);
    }
    let response = request
        .config()
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(network::error_from_ureq)
        .map_err(OnlineRequestError::from)?;
    match response.status().as_u16() {
        200 => network::read_json_body(response)
            .map(Some)
            .map_err(OnlineRequestError::from),
        404 => Ok(None),
        status => Err(OnlineRequestError::HttpStatus(status)),
    }
}

pub fn fetch_leaderboards(
    api_url: &str,
    page: Option<u32>,
) -> Result<Option<ArrowCloudLeaderboardsApiResponse>, OnlineRequestError> {
    get_arrowcloud_json(api_url, None, page)
}

pub fn fetch_user(api_key: &str) -> Result<Option<ArrowCloudUserApiResponse>, OnlineRequestError> {
    get_arrowcloud_json(user_url(), Some(api_key), None)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudRetrieveScoresRequest<'a> {
    pub chart_hashes: &'a [String],
    pub leaderboard_ids: &'a [ArrowCloudLeaderboard],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<&'a str>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudRetrieveScoresResponse {
    #[serde(default)]
    pub scores: HashMap<String, HashMap<String, ArrowCloudRetrieveScoreEntry>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudRetrieveScoreEntry {
    /// Score percent on a 0..100 scale (string in JSON, e.g. `"99.12"`).
    /// `None` means the server returned an entry without a score field.
    #[serde(default, deserialize_with = "de_optional_f64_from_string_or_number")]
    pub score: Option<f64>,
    #[serde(default)]
    pub grade: Option<String>,
    /// ISO-8601 / RFC-3339 timestamp string, e.g. `"2026-05-03T19:10:17.504Z"`.
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default, deserialize_with = "de_optional_i64_from_string_or_number")]
    pub play_id: Option<i64>,
    #[serde(default)]
    pub is_fail: bool,
}

pub fn retrieve_scores(
    api_key: &str,
    user_id: Option<&str>,
    chart_hashes: &[String],
    leaderboards: &[ArrowCloudLeaderboard],
) -> Result<ArrowCloudRetrieveScoresResponse, OnlineRequestError> {
    let body = ArrowCloudRetrieveScoresRequest {
        chart_hashes,
        leaderboard_ids: leaderboards,
        user_id,
    };
    let bearer = format!("Bearer {}", api_key.trim());
    let response = network::get_agent()
        .post(&retrieve_scores_url())
        .header("Content-Type", "application/json")
        .header("Authorization", &bearer)
        .send_json(&body)
        .map_err(network::error_from_ureq)
        .map_err(OnlineRequestError::from)?;
    if response.status().as_u16() != 200 {
        return Err(OnlineRequestError::HttpStatus(response.status().as_u16()));
    }
    network::read_json_body(response).map_err(OnlineRequestError::from)
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudLeaderboardsApiResponse {
    #[serde(default)]
    pub leaderboards: Vec<ArrowCloudLeaderboardPane>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudLeaderboardPane {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub scores: Vec<ArrowCloudLeaderboardEntry>,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub page: u32,
    #[serde(default)]
    pub has_next: bool,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub total_pages: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudLeaderboardEntry {
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub rank: u32,
    #[serde(default, deserialize_with = "de_f64_from_string_or_number")]
    pub score: f64, // 0..100
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub is_rival: bool,
    #[serde(default)]
    pub is_self: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudUserApiResponse {
    pub user: ArrowCloudUserApiUser,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudUserApiUser {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub rival_user_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudSpeed {
    pub value: f64,
    #[serde(rename = "type")]
    pub speed_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudModifiers {
    #[serde(rename = "visualDelay")]
    pub visual_delay: i32,
    pub acceleration: Vec<String>,
    pub appearance: Vec<String>,
    pub effect: Vec<String>,
    pub mini: i32,
    pub turn: String,
    #[serde(rename = "disabledWindows")]
    pub disabled_windows: String,
    pub speed: ArrowCloudSpeed,
    pub perspective: String,
    pub noteskin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudRadar {
    #[serde(rename = "Holds")]
    pub holds: [u32; 2],
    #[serde(rename = "Mines")]
    pub mines: [u32; 2],
    #[serde(rename = "Rolls")]
    pub rolls: [u32; 2],
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudLifePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudNpsPoint {
    pub x: f64,
    pub y: f64,
    pub measure: u32,
    pub nps: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudNpsInfo {
    #[serde(rename = "peakNPS")]
    pub peak_nps: f64,
    pub points: Vec<ArrowCloudNpsPoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ArrowCloudTimingOffset {
    Seconds(f64),
    Miss(&'static str),
}

pub type ArrowCloudTimingDatum = (f64, ArrowCloudTimingOffset);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowCloudJudgmentCounts {
    pub fantastic_plus: u32,
    pub fantastic: u32,
    pub excellent: u32,
    pub great: u32,
    pub decent: u32,
    pub way_off: u32,
    pub miss: u32,
    pub total_steps: u32,
    pub holds_held: u32,
    pub total_holds: u32,
    pub mines_hit: u32,
    pub total_mines: u32,
    pub rolls_held: u32,
    pub total_rolls: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudPayload {
    #[serde(rename = "songName")]
    pub song_name: String,
    pub artist: String,
    pub pack: String,
    pub length: String,
    pub hash: String,
    #[serde(rename = "timingData")]
    pub timing_data: Vec<ArrowCloudTimingDatum>,
    pub difficulty: u32,
    pub stepartist: String,
    pub radar: ArrowCloudRadar,
    #[serde(rename = "judgmentCounts")]
    pub judgment_counts: ArrowCloudJudgmentCounts,
    #[serde(rename = "npsInfo")]
    pub nps_info: ArrowCloudNpsInfo,
    #[serde(rename = "lifebarInfo")]
    pub lifebar_info: Vec<ArrowCloudLifePoint>,
    pub modifiers: ArrowCloudModifiers,
    #[serde(rename = "musicRate")]
    pub music_rate: f64,
    #[serde(rename = "usedAutoplay")]
    pub used_autoplay: bool,
    pub passed: bool,
    #[serde(rename = "bodyVersion")]
    pub body_version: &'static str,
    #[serde(rename = "_arrowCloudBodyVersion")]
    pub arrow_cloud_body_version: &'static str,
    #[serde(rename = "_engineName")]
    pub engine_name: &'static str,
    #[serde(rename = "_engineVersion")]
    pub engine_version: &'static str,
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
enum F64OrString {
    F64(f64),
    String(String),
}

fn de_f64_from_string_or_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<F64OrString>::deserialize(deserializer)? {
        Some(F64OrString::F64(v)) => Ok(v),
        Some(F64OrString::String(text)) => Ok(text.trim().parse::<f64>().unwrap_or(0.0)),
        None => Ok(0.0),
    }
}

fn de_optional_f64_from_string_or_number<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<F64OrString>::deserialize(deserializer)? {
        Some(F64OrString::F64(v)) => Ok(Some(v)),
        Some(F64OrString::String(text)) => Ok(text.trim().parse::<f64>().ok()),
        None => Ok(None),
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

fn de_optional_i64_from_string_or_number<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<StringOrNumber>::deserialize(deserializer)? {
        Some(StringOrNumber::I64(v)) => Ok(Some(v)),
        Some(StringOrNumber::U64(v)) => Ok(i64::try_from(v).ok()),
        Some(StringOrNumber::F64(v)) => {
            if v.is_finite() && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                Ok(Some(v as i64))
            } else {
                Ok(None)
            }
        }
        Some(StringOrNumber::String(text)) => Ok(text.trim().parse::<i64>().ok()),
        None => Ok(None),
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLoginStartReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLoginStartResp {
    pub session_id: String,
    pub short_code: String,
    pub poll_token: String,
    pub poll_interval_seconds: Option<u64>,
    pub verification_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLoginPollReq {
    pub session_id: String,
    pub poll_token: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLoginPollResp {
    pub status: DeviceLoginStatus,
    pub poll_interval_seconds: Option<u64>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceLoginStatus {
    Pending,
    Approved,
    Consumed,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceLoginEvent {
    Started {
        short_code: String,
        verification_url: String,
    },
    StatusUpdate,
    Consumed {
        api_key: String,
    },
    Failed {
        reason: String,
    },
}

/// `POST /device-login/start`. Asks ArrowCloud to mint a fresh
/// device-login session and returns the short code plus poll token.
pub fn device_login_start(
    body: &DeviceLoginStartReq,
) -> Result<DeviceLoginStartResp, NetworkError> {
    network::post_json(&format!("{DEVICE_LOGIN_BASE}/start"), body)
}

/// `POST /device-login/poll`. Asks ArrowCloud for the current status of
/// a device-login session. When `status == "consumed"`, the response
/// carries the new API key.
pub fn device_login_poll(body: &DeviceLoginPollReq) -> Result<DeviceLoginPollResp, NetworkError> {
    network::post_json(&format!("{DEVICE_LOGIN_BASE}/poll"), body)
}

pub fn run_device_login_session<F>(cancel: Arc<AtomicBool>, dispatch: F)
where
    F: FnMut(DeviceLoginEvent) -> bool,
{
    run_device_login_session_with(
        cancel,
        device_login_start,
        device_login_poll,
        dispatch,
        sleep_device_login_with_cancel,
    );
}

fn run_device_login_session_with<S, P, F, W>(
    cancel: Arc<AtomicBool>,
    start_fn: S,
    poll_fn: P,
    mut dispatch: F,
    mut wait: W,
) where
    S: Fn(&DeviceLoginStartReq) -> Result<DeviceLoginStartResp, NetworkError>,
    P: Fn(&DeviceLoginPollReq) -> Result<DeviceLoginPollResp, NetworkError>,
    F: FnMut(DeviceLoginEvent) -> bool,
    W: FnMut(f32, &Arc<AtomicBool>) -> bool,
{
    if cancel.load(Ordering::Relaxed) {
        return;
    }

    let req = DeviceLoginStartReq {
        machine_label: None,
        client_version: Some(format!("deadsync {}", env!("CARGO_PKG_VERSION"))),
        theme_version: None,
    };
    let start = match start_fn(&req) {
        Ok(resp) => resp,
        Err(err) => {
            dispatch(DeviceLoginEvent::Failed {
                reason: format!("{err}"),
            });
            return;
        }
    };

    let mut interval_s = clamp_device_login_poll_interval(start.poll_interval_seconds);
    let poll_req = DeviceLoginPollReq {
        session_id: start.session_id.clone(),
        poll_token: start.poll_token.clone(),
    };

    if !dispatch(DeviceLoginEvent::Started {
        short_code: start.short_code.clone(),
        verification_url: start.verification_url.clone(),
    }) {
        return;
    }

    loop {
        if !wait(interval_s, &cancel) {
            return;
        }
        match poll_fn(&poll_req) {
            Ok(resp) => {
                interval_s = clamp_device_login_poll_interval(resp.poll_interval_seconds);
                match resp.status {
                    DeviceLoginStatus::Consumed => {
                        let api_key = resp.api_key.unwrap_or_default();
                        let event = if api_key.trim().is_empty() {
                            DeviceLoginEvent::Failed {
                                reason: "server returned empty api key".to_string(),
                            }
                        } else {
                            DeviceLoginEvent::Consumed { api_key }
                        };
                        dispatch(event);
                        return;
                    }
                    DeviceLoginStatus::Cancelled | DeviceLoginStatus::Expired => {
                        dispatch(DeviceLoginEvent::Failed {
                            reason: format!("{:?}", resp.status).to_lowercase(),
                        });
                        return;
                    }
                    DeviceLoginStatus::Pending | DeviceLoginStatus::Approved => {
                        if !dispatch(DeviceLoginEvent::StatusUpdate) {
                            return;
                        }
                    }
                }
            }
            Err(err) => {
                dispatch(DeviceLoginEvent::Failed {
                    reason: format!("{err}"),
                });
                return;
            }
        }
    }
}

fn clamp_device_login_poll_interval(seconds: Option<u64>) -> f32 {
    let raw = seconds
        .map(|seconds| seconds as f32)
        .unwrap_or(DEVICE_LOGIN_POLL_INTERVAL_DEFAULT_S);
    raw.clamp(
        DEVICE_LOGIN_POLL_INTERVAL_MIN_S,
        DEVICE_LOGIN_POLL_INTERVAL_MAX_S,
    )
}

fn sleep_device_login_with_cancel(seconds: f32, cancel: &Arc<AtomicBool>) -> bool {
    let total = std::time::Duration::from_millis((seconds * 1000.0).max(50.0) as u64);
    let mut elapsed = std::time::Duration::ZERO;
    let tick = std::time::Duration::from_millis(100);
    while elapsed < total {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let chunk = tick.min(total - elapsed);
        std::thread::sleep(chunk);
        elapsed += chunk;
    }
    !cancel.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn leaderboards_url_uses_v1_chart_route() {
        assert_eq!(
            leaderboards_url("deadbeef").as_deref(),
            Some("https://api.arrowcloud.dance/v1/chart/deadbeef/leaderboards")
        );
    }

    #[test]
    fn leaderboards_url_rejects_empty_hash() {
        assert_eq!(leaderboards_url("   "), None);
    }

    #[test]
    fn legacy_leaderboards_url_uses_chart_route() {
        assert_eq!(
            legacy_leaderboards_url("deadbeef").as_deref(),
            Some("https://api.arrowcloud.dance/chart/deadbeef/leaderboards")
        );
    }

    #[test]
    fn user_url_uses_user_route() {
        assert_eq!(user_url(), "https://api.arrowcloud.dance/user");
    }

    #[test]
    fn retrieve_request_serializes_leaderboard_ids() {
        let hashes = vec!["006fb5c4890e98a2".to_string()];
        let body = ArrowCloudRetrieveScoresRequest {
            chart_hashes: hashes.as_slice(),
            leaderboard_ids: &ArrowCloudLeaderboard::ALL_GLOBAL,
            user_id: Some("user-1"),
        };
        let raw = serde_json::to_string(&body).expect("serialize");
        assert!(raw.contains("\"chartHashes\":[\"006fb5c4890e98a2\"]"));
        assert!(raw.contains("\"leaderboardIds\":[4,2,3]"));
        assert!(raw.contains("\"userId\":\"user-1\""));
    }

    #[test]
    fn leaderboards_response_deserializes_numeric_strings() {
        let raw = r#"{
            "leaderboards": [{
                "type": "HardEX",
                "page": "2",
                "totalPages": "4",
                "hasNext": true,
                "scores": [{
                    "rank": "7",
                    "score": "98.31",
                    "alias": "YOU",
                    "date": "2026-04-18T12:34:56.000Z",
                    "userId": "self",
                    "isSelf": true
                }]
            }]
        }"#;
        let decoded: ArrowCloudLeaderboardsApiResponse =
            serde_json::from_str(raw).expect("deserialize");
        let pane = &decoded.leaderboards[0];
        assert_eq!(pane.r#type, "HardEX");
        assert_eq!(pane.page, 2);
        assert_eq!(pane.total_pages, 4);
        assert!(pane.has_next);
        assert_eq!(pane.scores[0].rank, 7);
        assert_eq!(pane.scores[0].score, 98.31);
    }

    #[test]
    fn user_response_deserializes_rival_ids() {
        let raw = r#"{"user":{"id":"self","rivalUserIds":["rival"]}}"#;
        let decoded: ArrowCloudUserApiResponse = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(decoded.user.id, "self");
        assert_eq!(decoded.user.rival_user_ids, vec!["rival"]);
    }

    #[test]
    fn retrieve_response_decodes_full_shape() {
        let raw = r#"{
            "scores": {
                "006fb5c4890e98a2": {
                    "2": { "score": "99.12", "grade": "Tristar", "date": "2026-05-03T19:10:17.504Z" },
                    "3": { "score": "99.89", "grade": "Tristar", "date": "2026-05-03T19:10:17.504Z" }
                },
                "0092bb246527b2ec": {
                    "2": { "score": "97.44", "grade": "Twostar", "date": "2026-05-02T11:03:42.000Z" }
                }
            }
        }"#;
        let decoded: ArrowCloudRetrieveScoresResponse =
            serde_json::from_str(raw).expect("deserialize");
        assert_eq!(decoded.scores.len(), 2);
        assert!(decoded.scores["006fb5c4890e98a2"].contains_key("2"));
        assert!(decoded.scores["006fb5c4890e98a2"].contains_key("3"));
        assert_eq!(decoded.scores["0092bb246527b2ec"]["2"].score, Some(97.44));
    }

    #[test]
    fn retrieve_response_ignores_unknown_top_level_fields() {
        let raw = r#"{ "scores": {}, "extra": 42, "meta": { "x": 1 } }"#;
        let decoded: ArrowCloudRetrieveScoresResponse =
            serde_json::from_str(raw).expect("deserialize");
        assert!(decoded.scores.is_empty());
    }

    #[test]
    fn retrieve_response_treats_missing_score_field_as_none() {
        let raw = r#"{
            "scores": {
                "abc": { "3": { "grade": "n/a" } }
            }
        }"#;
        let decoded: ArrowCloudRetrieveScoresResponse =
            serde_json::from_str(raw).expect("deserialize");
        assert_eq!(decoded.scores["abc"]["3"].score, None);
    }

    #[test]
    fn submit_payload_serializes_miss_and_counts() {
        let payload = ArrowCloudPayload {
            song_name: "Test Song".to_string(),
            artist: "Test Artist".to_string(),
            pack: "Test Pack".to_string(),
            length: "1:23".to_string(),
            hash: "deadbeefcafebabe".to_string(),
            timing_data: vec![(24.488_208_770_752, ArrowCloudTimingOffset::Miss("Miss"))],
            difficulty: 12,
            stepartist: "Tester".to_string(),
            radar: ArrowCloudRadar {
                holds: [1, 2],
                mines: [3, 4],
                rolls: [5, 6],
            },
            judgment_counts: ArrowCloudJudgmentCounts {
                fantastic_plus: 10,
                fantastic: 20,
                excellent: 30,
                great: 40,
                decent: 50,
                way_off: 60,
                miss: 3,
                total_steps: 213,
                holds_held: 1,
                total_holds: 2,
                mines_hit: 3,
                total_mines: 4,
                rolls_held: 5,
                total_rolls: 6,
            },
            nps_info: ArrowCloudNpsInfo {
                peak_nps: 0.0,
                points: Vec::new(),
            },
            lifebar_info: Vec::new(),
            modifiers: ArrowCloudModifiers {
                visual_delay: 0,
                acceleration: Vec::new(),
                appearance: Vec::new(),
                effect: Vec::new(),
                mini: 0,
                turn: "None".to_string(),
                disabled_windows: "None".to_string(),
                speed: ArrowCloudSpeed {
                    value: 600.0,
                    speed_type: "C",
                },
                perspective: "Overhead".to_string(),
                noteskin: "cel".to_string(),
                scroll: None,
            },
            music_rate: 1.0,
            used_autoplay: false,
            passed: true,
            body_version: "1.4",
            arrow_cloud_body_version: "1.4",
            engine_name: "DeadSync",
            engine_version: "0.0.0",
        };

        let value = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(value["timingData"][0][1], serde_json::json!("Miss"));
        assert_eq!(value["judgmentCounts"]["miss"], serde_json::json!(3));
        assert_eq!(value["judgmentCounts"]["wayOff"], serde_json::json!(60));
        assert_eq!(value["radar"]["Holds"], serde_json::json!([1, 2]));
        assert_eq!(value["modifiers"]["speed"]["type"], serde_json::json!("C"));
        assert_eq!(value["bodyVersion"], serde_json::json!("1.4"));
        assert_eq!(value["_arrowCloudBodyVersion"], serde_json::json!("1.4"));
    }

    #[test]
    fn classify_connection_error_detects_timeout() {
        assert_eq!(
            classify_connection_error("request timed out"),
            ConnectionError::TimedOut
        );
        assert_eq!(
            classify_connection_error("Timeout reading body"),
            ConnectionError::TimedOut
        );
    }

    #[test]
    fn classify_connection_error_detects_host_blocked() {
        assert_eq!(
            classify_connection_error("403 forbidden"),
            ConnectionError::HostBlocked
        );
        assert_eq!(
            classify_connection_error("connection blocked by firewall"),
            ConnectionError::HostBlocked
        );
    }

    #[test]
    fn classify_connection_error_falls_back_to_cannot_connect() {
        assert_eq!(
            classify_connection_error("connection refused"),
            ConnectionError::CannotConnect
        );
    }

    #[test]
    fn network_errors_map_to_connection_errors() {
        assert_eq!(
            connection_error_from_network_error(&NetworkError::Timeout),
            ConnectionError::TimedOut
        );
        assert_eq!(
            connection_error_from_network_error(&NetworkError::HttpStatus(403)),
            ConnectionError::HostBlocked
        );
        assert_eq!(
            connection_error_from_network_error(&NetworkError::HttpStatus(500)),
            ConnectionError::CannotConnect
        );
        assert_eq!(
            connection_error_from_network_error(&NetworkError::Request(
                "connection blocked by firewall".to_string()
            )),
            ConnectionError::HostBlocked
        );
    }

    #[test]
    fn network_errors_map_to_probe_errors() {
        let timeout = ConnectionProbeError::from(NetworkError::Timeout);
        assert_eq!(timeout.connection_error, ConnectionError::TimedOut);
        assert_eq!(timeout.to_string(), "request timed out");

        let blocked = ConnectionProbeError::from(NetworkError::HttpStatus(403));
        assert_eq!(blocked.connection_error, ConnectionError::HostBlocked);
        assert_eq!(blocked.to_string(), "http status 403");
    }

    fn make_start_ok() -> DeviceLoginStartResp {
        DeviceLoginStartResp {
            session_id: "sess-1".into(),
            short_code: "ABCD2345".into(),
            poll_token: "tok-1".into(),
            poll_interval_seconds: Some(0),
            verification_url: "https://arrowcloud.dance/device-login/sess-1".into(),
        }
    }

    fn run_test_device_login<S, P>(start_fn: S, poll_fn: P) -> Vec<DeviceLoginEvent>
    where
        S: Fn(&DeviceLoginStartReq) -> Result<DeviceLoginStartResp, NetworkError>,
        P: Fn(&DeviceLoginPollReq) -> Result<DeviceLoginPollResp, NetworkError>,
    {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        run_device_login_session_with(
            cancel,
            start_fn,
            poll_fn,
            |event| {
                events.push(event);
                true
            },
            |_, _| true,
        );
        events
    }

    #[test]
    fn clamp_device_login_poll_interval_uses_default_when_missing() {
        assert!(
            (clamp_device_login_poll_interval(None) - DEVICE_LOGIN_POLL_INTERVAL_DEFAULT_S).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn clamp_device_login_poll_interval_clamps_to_min() {
        assert!(
            (clamp_device_login_poll_interval(Some(0)) - DEVICE_LOGIN_POLL_INTERVAL_MIN_S).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn clamp_device_login_poll_interval_clamps_to_max() {
        assert!(
            (clamp_device_login_poll_interval(Some(9999)) - DEVICE_LOGIN_POLL_INTERVAL_MAX_S).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn device_login_worker_emits_started_then_consumed() {
        let polls = Arc::new(Mutex::new(0u32));
        let polls_clone = Arc::clone(&polls);
        let start = make_start_ok();
        let start_fn =
            move |_req: &DeviceLoginStartReq| -> Result<_, NetworkError> { Ok(start.clone()) };
        let poll_fn = move |_req: &DeviceLoginPollReq| -> Result<_, NetworkError> {
            let mut n = polls_clone.lock().unwrap();
            *n += 1;
            if *n == 1 {
                Ok(DeviceLoginPollResp {
                    status: DeviceLoginStatus::Pending,
                    poll_interval_seconds: Some(0),
                    api_key: None,
                })
            } else {
                Ok(DeviceLoginPollResp {
                    status: DeviceLoginStatus::Consumed,
                    poll_interval_seconds: None,
                    api_key: Some("AC-KEY-7".into()),
                })
            }
        };

        let events = run_test_device_login(start_fn, poll_fn);

        assert!(matches!(
            events.first(),
            Some(DeviceLoginEvent::Started { .. })
        ));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, DeviceLoginEvent::StatusUpdate))
        );
        assert!(matches!(
            events.last(),
            Some(DeviceLoginEvent::Consumed { api_key }) if api_key == "AC-KEY-7"
        ));
        assert_eq!(*polls.lock().unwrap(), 2);
    }

    #[test]
    fn device_login_worker_reports_failure_on_expired() {
        let start = make_start_ok();
        let start_fn =
            move |_req: &DeviceLoginStartReq| -> Result<_, NetworkError> { Ok(start.clone()) };
        let poll_fn = move |_req: &DeviceLoginPollReq| -> Result<_, NetworkError> {
            Ok(DeviceLoginPollResp {
                status: DeviceLoginStatus::Expired,
                poll_interval_seconds: None,
                api_key: None,
            })
        };

        let events = run_test_device_login(start_fn, poll_fn);

        assert!(matches!(
            events.last(),
            Some(DeviceLoginEvent::Failed { reason }) if reason == "expired"
        ));
    }

    #[test]
    fn device_login_worker_reports_failure_when_start_errors() {
        let start_fn = |_req: &DeviceLoginStartReq| -> Result<_, NetworkError> {
            Err(NetworkError::Request("boom".into()))
        };
        let poll_fn = |_req: &DeviceLoginPollReq| -> Result<_, NetworkError> {
            unreachable!("poll should not be called when start fails")
        };

        let events = run_test_device_login(start_fn, poll_fn);

        assert!(matches!(
            events.first(),
            Some(DeviceLoginEvent::Failed { .. })
        ));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn device_login_worker_consumed_with_empty_key_is_failure() {
        let start = make_start_ok();
        let start_fn =
            move |_req: &DeviceLoginStartReq| -> Result<_, NetworkError> { Ok(start.clone()) };
        let poll_fn = move |_req: &DeviceLoginPollReq| -> Result<_, NetworkError> {
            Ok(DeviceLoginPollResp {
                status: DeviceLoginStatus::Consumed,
                poll_interval_seconds: None,
                api_key: Some("   ".into()),
            })
        };

        let events = run_test_device_login(start_fn, poll_fn);

        assert!(matches!(
            events.last(),
            Some(DeviceLoginEvent::Failed { .. })
        ));
    }

    #[test]
    fn sleep_device_login_with_cancel_returns_false_when_cancelled_mid_wait() {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let handle =
            std::thread::spawn(move || sleep_device_login_with_cancel(5.0, &cancel_for_thread));
        std::thread::sleep(std::time::Duration::from_millis(150));
        cancel.store(true, Ordering::Relaxed);
        assert!(!handle.join().unwrap());
    }

    #[test]
    fn start_resp_deserializes_camel_case() {
        let json = r#"{
            "sessionId": "11111111-2222-3333-4444-555555555555",
            "shortCode": "ABCD2345",
            "pollToken": "tok-xyz",
            "pollIntervalSeconds": 3,
            "verificationUrl": "https://arrowcloud.dance/device-login/11111111-2222-3333-4444-555555555555",
            "expiresAt": "2030-01-01T00:00:00.000Z"
        }"#;
        let resp: DeviceLoginStartResp = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.short_code, "ABCD2345");
        assert_eq!(resp.poll_token, "tok-xyz");
        assert_eq!(resp.poll_interval_seconds, Some(3));
        assert!(
            resp.verification_url
                .starts_with("https://arrowcloud.dance/device-login/")
        );
    }

    #[test]
    fn poll_resp_pending_omits_api_key() {
        let json = r#"{"status":"pending","pollIntervalSeconds":3}"#;
        let resp: DeviceLoginPollResp = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.status, DeviceLoginStatus::Pending);
        assert_eq!(resp.poll_interval_seconds, Some(3));
        assert!(resp.api_key.is_none());
    }

    #[test]
    fn poll_resp_consumed_carries_api_key() {
        let json = r#"{"status":"consumed","apiKey":"AC-KEY-123"}"#;
        let resp: DeviceLoginPollResp = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.status, DeviceLoginStatus::Consumed);
        assert_eq!(resp.api_key.as_deref(), Some("AC-KEY-123"));
    }

    #[test]
    fn poll_resp_terminal_states_parse() {
        for (raw, expected) in [
            (r#"{"status":"approved"}"#, DeviceLoginStatus::Approved),
            (r#"{"status":"cancelled"}"#, DeviceLoginStatus::Cancelled),
            (r#"{"status":"expired"}"#, DeviceLoginStatus::Expired),
        ] {
            let resp: DeviceLoginPollResp = serde_json::from_str(raw).expect("deserialize");
            assert_eq!(resp.status, expected);
        }
    }

    #[test]
    fn start_req_skips_none_optional_fields() {
        let body = DeviceLoginStartReq::default();
        let s = serde_json::to_string(&body).unwrap();
        assert_eq!(s, "{}");
    }

    #[test]
    fn start_req_serializes_camel_case_when_present() {
        let body = DeviceLoginStartReq {
            machine_label: Some("cab-1".into()),
            client_version: Some("deadsync 0.1".into()),
            theme_version: None,
        };
        let s = serde_json::to_string(&body).unwrap();
        assert!(s.contains("\"machineLabel\":\"cab-1\""));
        assert!(s.contains("\"clientVersion\":\"deadsync 0.1\""));
        assert!(!s.contains("themeVersion"));
    }
}
