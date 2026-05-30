use deadsync_net::{self as network, NetworkError};
use deadsync_score::ArrowCloudLeaderboard;
use serde::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ARROWCLOUD_API_BASE_URL: &str = "https://api.arrowcloud.dance";
const ARROWCLOUD_USER_URL: &str = "https://api.arrowcloud.dance/user";
const DEVICE_LOGIN_BASE: &str = "https://api.arrowcloud.dance/device-login";
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

#[inline(always)]
pub fn retrieve_scores_url() -> String {
    format!(
        "{}/v1/retrieve-scores",
        ARROWCLOUD_API_BASE_URL.trim_end_matches('/')
    )
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
