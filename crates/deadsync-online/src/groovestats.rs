use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::OnlineRequestError;
use deadsync_net::{self as network, NetworkError};
use deadsync_profile as profile_data;
use deadsync_profile::{Profile, RemoveMask, TimingWindowsOption};
use deadsync_rules::{judgment, scroll::ScrollSpeedSetting, timing::WindowCounts};
use deadsync_score::{
    GrooveStatsSubmitRecordBanner, GrooveStatsSubmitUiStatus, GsExEvidence, ImportedPlayerScore,
    LeaderboardEntry, LeaderboardPane, PlayerLeaderboardData, PlayerScoreImportResult,
    RejectReason, ScoreImportEndpoint, SubmitAchievement, SubmitAchievementReward,
    SubmitEventProgressData, SubmitProgress, SubmitQuest, SubmitQuestReward, SubmitStatImprovement,
    groovestats_submit_record_banner, leaderboard_nonzero_rank, leaderboard_pane,
    leaderboard_score_10000, leaderboard_username_matches, score_import_entry_matches_profile,
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

    #[inline(always)]
    pub const fn score_import_endpoint(self) -> ScoreImportEndpoint {
        match self {
            Self::GrooveStats => ScoreImportEndpoint::GrooveStats,
            Self::BoogieStats => ScoreImportEndpoint::BoogieStats,
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

#[derive(Debug, Clone)]
pub struct GrooveStatsSubmitPlayerRequest {
    pub slot: u8,
    pub chart_hash: String,
    pub api_key: String,
    pub payload: GrooveStatsSubmitPlayerPayload,
}

#[derive(Debug, Clone)]
pub struct GrooveStatsSubmitRequestParts {
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: JsonValue,
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
                        srpg_leaderboard: vec![leaderboard_entry(4, "RPG", 9000.0, false)],
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

        let progress_data = submit_event_progress_from_api(itl, itl.itl_leaderboard.clone());
        assert_eq!(progress_data.name, "ITL");
        assert!(progress_data.is_doubles);
        assert_eq!(progress_data.score_delta, -123);
        assert_eq!(progress_data.top_score_points, 42);
        assert_eq!(progress_data.leaderboard[0].name, "DREW");
        let score_progress = progress_data.progress.as_ref().expect("score progress");
        assert_eq!(score_progress.stat_improvements[0].name, "clearType");
        assert_eq!(score_progress.stat_improvements[0].gained, 2);
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
