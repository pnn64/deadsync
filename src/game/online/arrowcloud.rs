use crate::engine::network::{self, NetworkError};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};

const ARROWCLOUD_API_BASE_URL: &str = "https://api.arrowcloud.dance";
const ARROWCLOUD_USER_URL: &str = "https://api.arrowcloud.dance/user";
const DEVICE_LOGIN_BASE: &str = "https://api.arrowcloud.dance/device-login";

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

static STATUS: LazyLock<Mutex<ConnectionStatus>> =
    LazyLock::new(|| Mutex::new(ConnectionStatus::Pending));

#[inline(always)]
fn set_status(status: ConnectionStatus) {
    *STATUS.lock().unwrap() = status;
}

pub fn get_status() -> ConnectionStatus {
    STATUS.lock().unwrap().clone()
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

pub fn init() {
    refresh_status();
}

/// Re-runs the connectivity probe.  Safe to call repeatedly (e.g. after
/// device-login writes a new api key to the active profile).
pub fn refresh_status() {
    let cfg = crate::config::get();
    if !cfg.enable_arrowcloud {
        set_status(ConnectionStatus::Error(ConnectionError::Disabled));
        return;
    }

    set_status(ConnectionStatus::Pending);
    debug!("Initializing ArrowCloud network check...");
    network::spawn_request(perform_check);
}

fn perform_check() {
    debug!("Performing ArrowCloud connectivity check...");

    match network::get_agent().get(ARROWCLOUD_API_BASE_URL).call() {
        Ok(_) => {
            info!("Connected to ArrowCloud.");
            set_status(ConnectionStatus::Connected);
        }
        Err(error) => {
            warn!("HTTP error to ArrowCloud: {error}");
            let message = error.to_string();
            set_status(ConnectionStatus::Error(classify_error(&message)));
        }
    }
}

fn classify_error(message: &str) -> ConnectionError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        return ConnectionError::TimedOut;
    }
    if lower.contains("blocked") || lower.contains("forbidden") || lower.contains("403") {
        return ConnectionError::HostBlocked;
    }
    ConnectionError::CannotConnect
}

// ---------------------------------------------------------------------------
// Device-login HTTP endpoints
//
// Wire types + free functions for the AC `/device-login/{start,poll}`
// API.  The state machine that drives a session through these endpoints
// lives in a separate module added in a follow-up commit.
// ---------------------------------------------------------------------------

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

/// `POST /device-login/start`.  Asks AC to mint a fresh device-login
/// session and returns the short code + poll token.
pub fn device_login_start(
    body: &DeviceLoginStartReq,
) -> Result<DeviceLoginStartResp, NetworkError> {
    network::post_json(&format!("{DEVICE_LOGIN_BASE}/start"), body)
}

/// `POST /device-login/poll`.  Asks AC for the current status of a
/// device-login session.  When `status == "consumed"`, the response
/// carries the new api key.
pub fn device_login_poll(
    body: &DeviceLoginPollReq,
) -> Result<DeviceLoginPollResp, NetworkError> {
    network::post_json(&format!("{DEVICE_LOGIN_BASE}/poll"), body)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn classify_error_detects_timeout() {
        assert_eq!(classify_error("request timed out"), ConnectionError::TimedOut);
        assert_eq!(classify_error("Timeout reading body"), ConnectionError::TimedOut);
    }

    #[test]
    fn classify_error_detects_host_blocked() {
        assert_eq!(classify_error("403 forbidden"), ConnectionError::HostBlocked);
        assert_eq!(classify_error("connection blocked by firewall"), ConnectionError::HostBlocked);
    }

    #[test]
    fn classify_error_falls_back_to_cannot_connect() {
        assert_eq!(classify_error("connection refused"), ConnectionError::CannotConnect);
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
        assert!(resp.verification_url.starts_with("https://arrowcloud.dance/device-login/"));
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
