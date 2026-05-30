use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

use deadsync_net::NetworkError;

const GROOVESTATS_API_BASE_URL: &str = "https://api.groovestats.com";
const BOOGIESTATS_API_BASE_URL: &str = "https://boogiestats.andr.host";
const GROOVESTATS_QR_BASE_URL: &str = "https://www.groovestats.com";
const GROOVESTATS_NEW_SESSION_PATH: &str = "new-session.php?chartHashVersion=3";

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

fn compact_f32_text(value: f32) -> String {
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
    fn api_base_urls_match_backend() {
        assert_eq!(api_base_url(Service::GrooveStats), GROOVESTATS_API_BASE_URL);
        assert_eq!(api_base_url(Service::BoogieStats), BOOGIESTATS_API_BASE_URL);
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
