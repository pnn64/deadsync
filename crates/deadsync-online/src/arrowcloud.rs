use crate::OnlineRequestError;
use deadsync_net::{self as network, NetworkError};
use deadsync_profile as profile_data;
use deadsync_profile::Profile;
use deadsync_rules::{
    judgment,
    scroll::ScrollSpeedSetting,
    timing::{ScatterPoint, WindowCounts},
};
use deadsync_score::{
    ArrowCloudAutosubmitLog, ArrowCloudAutosubmitLogLevel, ArrowCloudLeaderboard,
    ArrowCloudPaneKind, ArrowCloudScore, ArrowCloudScores, ArrowCloudSubmitStats,
    ArrowCloudSubmitUiStatus, ArrowCloudUserContext, LeaderboardEntry, LeaderboardPane,
    RejectReason, SUBMIT_RETRY_MAX_ATTEMPTS, SubmitRetryState,
    arrowcloud_empty_hard_ex_leaderboard_pane, arrowcloud_entry_flags,
    arrowcloud_hard_ex_leaderboard_pane, arrowcloud_leaderboard_entry,
    arrowcloud_pane_kind_from_type, arrowcloud_score_from_retrieve_fields,
    arrowcloud_submit_ui_status, arrowcloud_target_user_ids, arrowcloud_user_id,
    set_arrowcloud_score_for_leaderboard,
};
use serde::Deserializer;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Instant;

const ARROWCLOUD_API_BASE_URL: &str = "https://api.arrowcloud.dance";
const ARROWCLOUD_USER_URL: &str = "https://api.arrowcloud.dance/user";
const DEVICE_LOGIN_BASE: &str = "https://api.arrowcloud.dance/device-login";
const DEVICE_LOGIN_POLL_INTERVAL_MIN_S: f32 = 1.0;
const DEVICE_LOGIN_POLL_INTERVAL_MAX_S: f32 = 10.0;
const DEVICE_LOGIN_POLL_INTERVAL_DEFAULT_S: f32 = 3.0;
pub const ARROWCLOUD_BULK_MAX_HASHES: usize = 1000;
pub const ARROWCLOUD_LIFEBAR_POINTS: usize = 100;
pub const ARROWCLOUD_BODY_VERSION: &str = "1.4";
pub const ARROWCLOUD_ENGINE_NAME: &str = "DeadSync";
const ARROWCLOUD_RETRY_MAX_ATTEMPTS: u8 = SUBMIT_RETRY_MAX_ATTEMPTS;
const ARROWCLOUD_SUBMIT_RETRY_TRACKED_PER_SIDE: usize = 128;
const ARROWCLOUD_ACCEL_NAMES: [&str; 5] = ["Boost", "Brake", "Wave", "Expand", "Boomerang"];
const ARROWCLOUD_EFFECT_NAMES: [&str; 10] = [
    "Drunk",
    "Dizzy",
    "Confusion",
    "Big",
    "Flip",
    "Invert",
    "Tornado",
    "Tipsy",
    "Bumpy",
    "Beat",
];
const ARROWCLOUD_APPEARANCE_NAMES: [&str; 5] = ["Hidden", "Sudden", "Stealth", "Blink", "R.Vanish"];

#[inline(always)]
pub fn warn_submit_skip(side: profile_data::PlayerSide, chart_hash: &str, reason: &str) {
    log::warn!(
        "Skipping ArrowCloud submit for {:?} ({}): {}.",
        side,
        chart_hash,
        reason
    );
}

#[inline(always)]
pub fn log_global_submit_skip(log: ArrowCloudAutosubmitLog) {
    match log.level {
        ArrowCloudAutosubmitLogLevel::Debug => {
            log::debug!("Skipping ArrowCloud submit: {}.", log.reason)
        }
        ArrowCloudAutosubmitLogLevel::Warn => {
            log::warn!("Skipping ArrowCloud submit: {}.", log.reason)
        }
    }
}

#[inline(always)]
pub fn log_player_submit_skip(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    log: ArrowCloudAutosubmitLog,
) {
    match log.level {
        ArrowCloudAutosubmitLogLevel::Debug => log::debug!(
            "Skipping ArrowCloud submit for {:?} ({}): {}.",
            side,
            chart_hash,
            log.reason
        ),
        ArrowCloudAutosubmitLogLevel::Warn => warn_submit_skip(side, chart_hash, log.reason),
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionProbeLog {
    Connected,
    CannotConnect { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProbeTransition {
    pub status: ConnectionStatus,
    pub log: Option<ConnectionProbeLog>,
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

#[derive(Debug, Clone)]
pub struct ArrowCloudSubmitJob {
    pub side: profile_data::PlayerSide,
    pub api_key: String,
    pub token: u64,
    pub payload: ArrowCloudPayload,
    /// Active local profile id whose AC cache should be updated on submit
    /// success. `None` if the submitting side is in Guest mode.
    pub profile_id: Option<String>,
    /// Gameplay-computed score percents (0..=100) captured at job creation
    /// time so the AC cache can be populated without waiting for a server echo.
    pub itg_percent: f64,
    pub ex_percent: f64,
    pub hard_ex_percent: f64,
    pub is_fail: bool,
}

#[derive(Debug, Clone)]
pub struct ArrowCloudSubmitDraft {
    pub side: profile_data::PlayerSide,
    pub api_key: String,
    pub payload: ArrowCloudPayload,
    /// Active local profile id whose AC cache should be updated on submit
    /// success. `None` if the submitting side is in Guest mode.
    pub profile_id: Option<String>,
    /// Gameplay-computed score percents (0..=100) captured at draft creation
    /// time so retries and cache writes preserve the original result.
    pub itg_percent: f64,
    pub ex_percent: f64,
    pub hard_ex_percent: f64,
    pub is_fail: bool,
}

impl ArrowCloudSubmitDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        side: profile_data::PlayerSide,
        api_key: String,
        payload: ArrowCloudPayload,
        profile_id: Option<String>,
        itg_percent: f64,
        ex_percent: f64,
        hard_ex_percent: f64,
        is_fail: bool,
    ) -> Self {
        Self {
            side,
            api_key,
            payload,
            profile_id,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail,
        }
    }

    pub fn retry_entry(&self) -> ArrowCloudSubmitRetryEntry {
        ArrowCloudSubmitRetryEntry::new(
            self.side,
            self.api_key.clone(),
            self.payload.clone(),
            self.profile_id.clone(),
            self.itg_percent,
            self.ex_percent,
            self.hard_ex_percent,
            self.is_fail,
        )
    }

    pub fn submit_job(self, token: u64) -> ArrowCloudSubmitJob {
        ArrowCloudSubmitJob {
            side: self.side,
            api_key: self.api_key,
            token,
            payload: self.payload,
            profile_id: self.profile_id,
            itg_percent: self.itg_percent,
            ex_percent: self.ex_percent,
            hard_ex_percent: self.hard_ex_percent,
            is_fail: self.is_fail,
        }
    }
}

pub fn begin_submit_jobs_from_drafts(
    drafts: Vec<ArrowCloudSubmitDraft>,
) -> Vec<ArrowCloudSubmitJob> {
    drafts
        .into_iter()
        .map(|draft| {
            store_submit_retry(draft.retry_entry());
            let side = draft.side;
            let chart_hash = draft.payload.hash.clone();
            let token = next_submit_ui_token();
            set_submit_ui_status(
                side,
                chart_hash.as_str(),
                token,
                ArrowCloudSubmitUiStatus::Submitting,
            );
            draft.submit_job(token)
        })
        .collect()
}

impl ArrowCloudSubmitJob {
    pub fn new(
        side: profile_data::PlayerSide,
        api_key: String,
        token: u64,
        payload: ArrowCloudPayload,
        profile_id: Option<String>,
        itg_percent: f64,
        ex_percent: f64,
        hard_ex_percent: f64,
        is_fail: bool,
    ) -> Self {
        Self {
            side,
            api_key,
            token,
            payload,
            profile_id,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail,
        }
    }

    pub fn from_retry_entry(entry: ArrowCloudSubmitRetryEntry, token: u64) -> Self {
        Self {
            side: entry.side,
            api_key: entry.api_key,
            token,
            payload: entry.payload,
            profile_id: entry.profile_id,
            itg_percent: entry.itg_percent,
            ex_percent: entry.ex_percent,
            hard_ex_percent: entry.hard_ex_percent,
            is_fail: entry.is_fail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowCloudSubmitError {
    pub status: ArrowCloudSubmitUiStatus,
    pub message: String,
}

pub fn submit_error_status_and_message(
    error: &ArrowCloudSubmitRequestError,
) -> (ArrowCloudSubmitUiStatus, String) {
    match error {
        ArrowCloudSubmitRequestError::InvalidRequest { message } => {
            let reason = if message.contains("API key") {
                RejectReason::Unauthorized
            } else {
                RejectReason::InvalidScore
            };
            (
                ArrowCloudSubmitUiStatus::Rejected { reason },
                message.clone(),
            )
        }
        ArrowCloudSubmitRequestError::Transport { message, timed_out } => (
            if *timed_out {
                ArrowCloudSubmitUiStatus::TimedOut
            } else {
                ArrowCloudSubmitUiStatus::NetworkError
            },
            message.clone(),
        ),
        ArrowCloudSubmitRequestError::Http {
            status,
            body_snippet,
        } => {
            let status_kind = ArrowCloudSubmitUiStatus::from_http_status(*status);
            let message = if body_snippet.is_empty() {
                format!("HTTP {status}")
            } else {
                format!("HTTP {status}: {body_snippet}")
            };
            (status_kind, message)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArrowCloudSubmitRetryEntry {
    pub side: profile_data::PlayerSide,
    pub api_key: String,
    pub payload: ArrowCloudPayload,
    pub profile_id: Option<String>,
    pub itg_percent: f64,
    pub ex_percent: f64,
    pub hard_ex_percent: f64,
    pub is_fail: bool,
    retry_attempt: u8,
    next_retry_at: Option<Instant>,
}

impl ArrowCloudSubmitRetryEntry {
    pub fn new(
        side: profile_data::PlayerSide,
        api_key: String,
        payload: ArrowCloudPayload,
        profile_id: Option<String>,
        itg_percent: f64,
        ex_percent: f64,
        hard_ex_percent: f64,
        is_fail: bool,
    ) -> Self {
        Self {
            side,
            api_key,
            payload,
            profile_id,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail,
            retry_attempt: 0,
            next_retry_at: None,
        }
    }
}

pub fn arrowcloud_submit_error_from_request(
    error: ArrowCloudSubmitRequestError,
) -> ArrowCloudSubmitError {
    let (status, message) = submit_error_status_and_message(&error);
    ArrowCloudSubmitError { status, message }
}

pub fn submit_job_request(job: &ArrowCloudSubmitJob) -> Result<(), ArrowCloudSubmitError> {
    match submit_score_request(job.api_key.as_str(), &job.payload) {
        Ok(success) => {
            if success.body_snippet.is_empty() {
                log::debug!(
                    "ArrowCloud submit success for {:?} ({}) status={}",
                    job.side,
                    job.payload.hash,
                    success.status
                );
            } else {
                log::debug!(
                    "ArrowCloud submit success for {:?} ({}) status={} body='{}'",
                    job.side,
                    job.payload.hash,
                    success.status,
                    success.body_snippet.as_str()
                );
            }
            Ok(())
        }
        Err(error) => Err(arrowcloud_submit_error_from_request(error)),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArrowCloudSubmitRunSummary {
    pub succeeded: usize,
    pub failed: usize,
}

static ARROWCLOUD_SUBMIT_RETRY: LazyLock<Mutex<SubmitRetryState<ArrowCloudSubmitRetryEntry>>> =
    LazyLock::new(|| Mutex::new(SubmitRetryState::default()));

#[inline(always)]
pub fn reset_submit_ui_status(side: profile_data::PlayerSide, chart_hash: &str) {
    deadsync_score::arrowcloud_reset_submit_ui_status(
        profile_data::player_side_index(side),
        chart_hash,
    );
}

#[inline(always)]
pub fn set_submit_ui_status(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    deadsync_score::arrowcloud_set_submit_ui_status(
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
    status: ArrowCloudSubmitUiStatus,
) -> bool {
    deadsync_score::arrowcloud_update_submit_ui_status_if_token(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        status,
    )
}

#[inline(always)]
pub fn next_submit_ui_token() -> u64 {
    deadsync_score::arrowcloud_next_submit_ui_token()
}

#[inline(always)]
pub fn submit_ui_status_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<ArrowCloudSubmitUiStatus> {
    arrowcloud_submit_ui_status(profile_data::player_side_index(side), chart_hash)
}

#[inline(always)]
pub fn reset_submit_retry(side: profile_data::PlayerSide, chart_hash: &str) {
    ARROWCLOUD_SUBMIT_RETRY.lock().unwrap().reset_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        |entry| entry.payload.hash.as_str(),
    );
}

#[inline(always)]
pub fn store_submit_retry(entry: ArrowCloudSubmitRetryEntry) {
    let side = entry.side;
    ARROWCLOUD_SUBMIT_RETRY.lock().unwrap().upsert_by_key(
        profile_data::player_side_index(side),
        entry,
        |entry| entry.payload.hash.as_str(),
        ARROWCLOUD_SUBMIT_RETRY_TRACKED_PER_SIDE,
    );
}

#[inline(always)]
pub fn take_ready_submit_retry(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
) -> Option<ArrowCloudSubmitRetryEntry> {
    ARROWCLOUD_SUBMIT_RETRY.lock().unwrap().take_ready_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        manual,
        Instant::now(),
        |entry| entry.payload.hash.as_str(),
        |entry| &mut entry.next_retry_at,
    )
}

pub fn take_ready_submit_retry_job(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
    token: u64,
) -> Option<ArrowCloudSubmitJob> {
    take_ready_submit_retry(chart_hash, side, manual)
        .map(|entry| ArrowCloudSubmitJob::from_retry_entry(entry, token))
}

pub fn begin_ready_submit_retry_job(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
) -> Option<ArrowCloudSubmitJob> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    if !submit_ui_status_for_side(hash, side)?.can_retry() {
        return None;
    }
    let token = next_submit_ui_token();
    let job = take_ready_submit_retry_job(hash, side, manual, token)?;
    set_submit_ui_status(side, hash, token, ArrowCloudSubmitUiStatus::Submitting);
    log::debug!("Retrying ArrowCloud submit for {:?} ({}).", side, hash);
    Some(job)
}

pub fn retry_submit_if_enabled(
    enabled: bool,
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
    cache_success: fn(&ArrowCloudSubmitJob),
    after_submit: fn(&ArrowCloudSubmitJob),
) -> bool {
    if !enabled {
        return false;
    }
    let Some(job) = begin_ready_submit_retry_job(chart_hash, side, manual) else {
        return false;
    };
    spawn_submit_jobs(vec![job], cache_success, after_submit);
    true
}

pub fn tick_auto_submit_retries_if_enabled(
    enabled: bool,
    cache_success: fn(&ArrowCloudSubmitJob),
    after_submit: fn(&ArrowCloudSubmitJob),
) -> bool {
    let mut fired = false;
    for (hash, side, _) in due_auto_submit_retries() {
        if retry_submit_if_enabled(
            enabled,
            hash.as_str(),
            side,
            false,
            cache_success,
            after_submit,
        ) {
            fired = true;
        }
    }
    fired
}

pub fn complete_submit_job_success(job: &ArrowCloudSubmitJob) -> bool {
    let accepted = update_submit_ui_status_if_token(
        job.side,
        job.payload.hash.as_str(),
        job.token,
        ArrowCloudSubmitUiStatus::Submitted,
    );
    if accepted {
        reset_submit_retry(job.side, job.payload.hash.as_str());
    }
    accepted
}

pub fn complete_submit_job_failure(
    job: &ArrowCloudSubmitJob,
    status: ArrowCloudSubmitUiStatus,
) -> bool {
    let accepted =
        update_submit_ui_status_if_token(job.side, job.payload.hash.as_str(), job.token, status);
    if accepted {
        record_submit_failure(job.side, job.payload.hash.as_str(), status);
    }
    accepted
}

pub fn run_submit_jobs_with<S, C, A>(
    jobs: Vec<ArrowCloudSubmitJob>,
    mut submit_job: S,
    mut cache_success: C,
    mut after_submit: A,
) -> ArrowCloudSubmitRunSummary
where
    S: FnMut(&ArrowCloudSubmitJob) -> Result<(), ArrowCloudSubmitError>,
    C: FnMut(&ArrowCloudSubmitJob),
    A: FnMut(&ArrowCloudSubmitJob),
{
    let mut summary = ArrowCloudSubmitRunSummary::default();
    for job in jobs {
        match submit_job(&job) {
            Ok(()) => {
                summary.succeeded += 1;
                if complete_submit_job_success(&job) {
                    cache_success(&job);
                }
            }
            Err(err) => {
                summary.failed += 1;
                complete_submit_job_failure(&job, err.status);
                log::warn!(
                    "ArrowCloud submit failed for {:?} ({}) status={:?}: {}",
                    job.side,
                    job.payload.hash,
                    err.status,
                    err.message
                );
            }
        }
        after_submit(&job);
    }
    summary
}

pub fn spawn_submit_jobs(
    jobs: Vec<ArrowCloudSubmitJob>,
    cache_success: fn(&ArrowCloudSubmitJob),
    after_submit: fn(&ArrowCloudSubmitJob),
) {
    thread::spawn(move || {
        run_submit_jobs_with(jobs, submit_job_request, cache_success, after_submit);
    });
}

#[inline(always)]
pub fn record_submit_failure(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    status: ArrowCloudSubmitUiStatus,
) {
    ARROWCLOUD_SUBMIT_RETRY
        .lock()
        .unwrap()
        .record_failure_by_key(
            profile_data::player_side_index(side),
            chart_hash,
            status.can_retry(),
            ARROWCLOUD_RETRY_MAX_ATTEMPTS,
            Instant::now(),
            |entry| entry.payload.hash.as_str(),
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
    ARROWCLOUD_SUBMIT_RETRY
        .lock()
        .unwrap()
        .remaining_secs_by_key(
            profile_data::player_side_index(side),
            hash,
            Instant::now(),
            |entry| entry.payload.hash.as_str(),
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
        let lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
        let Some(attempt) = lock.retry_attempt_by_key(
            profile_data::player_side_index(side),
            hash,
            |entry| entry.payload.hash.as_str(),
            |entry| entry.retry_attempt,
        ) else {
            return false;
        };
        attempt
    };
    if attempt >= ARROWCLOUD_RETRY_MAX_ATTEMPTS {
        return false;
    }
    matches!(
        arrowcloud_submit_ui_status(profile_data::player_side_index(side), hash),
        Some(s) if s.is_auto_retryable()
    )
}

#[inline(always)]
pub fn due_auto_submit_retries() -> Vec<(String, profile_data::PlayerSide, u8)> {
    let due = {
        let lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
        lock.due_retries(
            Instant::now(),
            |entry| entry.payload.hash.as_str(),
            |entry| entry.side,
            |entry| entry.retry_attempt,
            |entry| entry.next_retry_at,
        )
    };
    due.into_iter()
        .filter(|(hash, side, attempt)| {
            *attempt < ARROWCLOUD_RETRY_MAX_ATTEMPTS
                && matches!(
                    arrowcloud_submit_ui_status(profile_data::player_side_index(*side), hash),
                    Some(status) if status.is_auto_retryable()
                )
        })
        .collect()
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

pub fn connection_transition_from_probe_result(
    result: Result<ConnectionStatus, ConnectionProbeError>,
) -> ConnectionProbeTransition {
    match result {
        Ok(ConnectionStatus::Connected) => ConnectionProbeTransition {
            status: ConnectionStatus::Connected,
            log: Some(ConnectionProbeLog::Connected),
        },
        Ok(status) => ConnectionProbeTransition { status, log: None },
        Err(error) => ConnectionProbeTransition {
            status: ConnectionStatus::Error(error.connection_error),
            log: Some(ConnectionProbeLog::CannotConnect {
                error: error.message,
            }),
        },
    }
}

pub fn probe_connection_transition() -> ConnectionProbeTransition {
    connection_transition_from_probe_result(probe_connection())
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

pub fn runtime_init(enabled: bool, log_probe: ConnectionProbeLogFn) {
    if !enabled {
        runtime_set_status(ConnectionStatus::Error(ConnectionError::Disabled));
        return;
    }

    runtime_set_status(ConnectionStatus::Pending);
    thread::spawn(move || runtime_perform_check(log_probe));
}

pub fn runtime_init_with_default_log(enabled: bool) {
    if enabled {
        log::debug!("Initializing ArrowCloud network check...");
    }
    runtime_init(enabled, log_probe_transition);
}

pub fn log_probe_transition(log: Option<ConnectionProbeLog>) {
    match log {
        Some(ConnectionProbeLog::Connected) => log::info!("Connected to ArrowCloud."),
        Some(ConnectionProbeLog::CannotConnect { error }) => {
            log::warn!("HTTP error to ArrowCloud: {error}");
        }
        None => {}
    }
}

fn runtime_perform_check(log_probe: ConnectionProbeLogFn) {
    let transition = probe_connection_transition();
    log_probe(transition.log);
    runtime_set_status(transition.status);
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

/// Convert one bulk-score entry into the shared ArrowCloud score cache type.
///
/// Returns `None` when the response has no usable score field; otherwise it
/// preserves ArrowCloud-native metadata such as server grade, timestamp, and
/// play id.
pub fn score_from_retrieve_entry(entry: &ArrowCloudRetrieveScoreEntry) -> Option<ArrowCloudScore> {
    arrowcloud_score_from_retrieve_fields(
        entry.score,
        entry.grade.as_deref(),
        entry.date.as_deref(),
        entry.play_id,
        entry.is_fail,
    )
}

pub fn scores_from_retrieve_entry_map(
    leaderboards: &HashMap<String, ArrowCloudRetrieveScoreEntry>,
) -> ArrowCloudScores {
    let mut out = ArrowCloudScores::default();
    for (leaderboard_id, entry) in leaderboards {
        let Ok(leaderboard_id) = leaderboard_id.parse::<u32>() else {
            continue;
        };
        let Some(score) = score_from_retrieve_entry(entry) else {
            continue;
        };
        set_arrowcloud_score_for_leaderboard(&mut out, leaderboard_id, score);
    }
    out
}

pub fn score_cache_entries_from_retrieve_response(
    response: ArrowCloudRetrieveScoresResponse,
) -> HashMap<String, ArrowCloudScores> {
    let mut out = HashMap::with_capacity(response.scores.len());
    for (chart_hash, leaderboards) in response.scores {
        let scores = scores_from_retrieve_entry_map(&leaderboards);
        if scores.itg.is_some() || scores.ex.is_some() || scores.hard_ex.is_some() {
            out.insert(chart_hash, scores);
        }
    }
    out
}

pub fn retrieve_score_cache_entries(
    api_key: &str,
    user_id: Option<&str>,
    chart_hashes: &[String],
    leaderboards: &[ArrowCloudLeaderboard],
) -> Result<HashMap<String, ArrowCloudScores>, OnlineRequestError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(OnlineRequestError::Request(
            "ArrowCloud API key is missing.".to_string(),
        ));
    }
    if chart_hashes.is_empty() {
        return Ok(HashMap::new());
    }
    if chart_hashes.len() > ARROWCLOUD_BULK_MAX_HASHES {
        return Err(OnlineRequestError::Request(format!(
            "ArrowCloud bulk request exceeds {ARROWCLOUD_BULK_MAX_HASHES} chart hashes."
        )));
    }

    retrieve_scores(api_key, user_id, chart_hashes, leaderboards)
        .map(score_cache_entries_from_retrieve_response)
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

pub fn user_context_from_api(user: ArrowCloudUserApiUser) -> ArrowCloudUserContext {
    let self_user_id = arrowcloud_user_id(user.id.as_str()).map(str::to_string);
    let rival_user_ids = user
        .rival_user_ids
        .into_iter()
        .map(|user_id| user_id.trim().to_string())
        .filter(|user_id| !user_id.is_empty())
        .collect();
    ArrowCloudUserContext {
        self_user_id,
        rival_user_ids,
    }
}

pub fn fetch_user_context(
    api_key: &str,
) -> Result<Option<ArrowCloudUserContext>, OnlineRequestError> {
    fetch_user(api_key)
        .map(|response| response.map(|response| user_context_from_api(response.user)))
}

pub fn leaderboard_entry_from_api(
    entry: ArrowCloudLeaderboardEntry,
    is_self: bool,
    is_rival: bool,
) -> LeaderboardEntry {
    arrowcloud_leaderboard_entry(
        entry.rank,
        entry.alias,
        entry.score,
        entry.date,
        is_self,
        is_rival,
    )
}

pub fn hard_ex_pane_from_response(
    decoded: ArrowCloudLeaderboardsApiResponse,
) -> Option<ArrowCloudLeaderboardPane> {
    decoded.leaderboards.into_iter().find(|pane| {
        arrowcloud_pane_kind_from_type(pane.r#type.as_str()) == Some(ArrowCloudPaneKind::HardEx)
    })
}

pub fn update_remaining_targets(
    scores: &[ArrowCloudLeaderboardEntry],
    context: Option<&ArrowCloudUserContext>,
    remaining: &mut HashSet<String>,
) {
    if remaining.is_empty() {
        return;
    }
    for entry in scores {
        let Some(user_id) = arrowcloud_user_id(entry.user_id.as_str()) else {
            continue;
        };
        let (is_self, is_rival) =
            arrowcloud_entry_flags(Some(user_id), entry.is_self, entry.is_rival, context);
        if is_self || is_rival {
            remaining.remove(user_id);
            if remaining.is_empty() {
                break;
            }
        }
    }
}

pub fn hard_ex_pane_from_pages(
    first_page: ArrowCloudLeaderboardPane,
    extra_pages: Vec<ArrowCloudLeaderboardPane>,
    context: Option<&ArrowCloudUserContext>,
) -> LeaderboardPane {
    let mut entries = Vec::with_capacity(first_page.scores.len());
    let mut appended_user_ids = HashSet::new();

    for entry in first_page.scores {
        let user_id = arrowcloud_user_id(entry.user_id.as_str()).map(str::to_owned);
        let (is_self, is_rival) =
            arrowcloud_entry_flags(user_id.as_deref(), entry.is_self, entry.is_rival, context);
        if (is_self || is_rival)
            && let Some(user_id) = user_id
        {
            appended_user_ids.insert(user_id);
        }
        entries.push(leaderboard_entry_from_api(entry, is_self, is_rival));
    }

    for page in extra_pages {
        for entry in page.scores {
            let user_id = arrowcloud_user_id(entry.user_id.as_str()).map(str::to_owned);
            let (is_self, is_rival) =
                arrowcloud_entry_flags(user_id.as_deref(), entry.is_self, entry.is_rival, context);
            if !(is_self || is_rival) {
                continue;
            }
            if let Some(user_id) = user_id
                && !appended_user_ids.insert(user_id)
            {
                continue;
            }
            entries.push(leaderboard_entry_from_api(entry, is_self, is_rival));
        }
    }
    let personalized = entries.iter().any(|entry| entry.is_self || entry.is_rival);

    arrowcloud_hard_ex_leaderboard_pane(entries, personalized)
}

pub fn fetch_hard_ex_leaderboard_panes(
    chart_hash: &str,
    api_key: &str,
) -> Result<Vec<LeaderboardPane>, OnlineRequestError> {
    let chart_hash = chart_hash.trim();
    let api_key = api_key.trim();
    if chart_hash.is_empty() || api_key.is_empty() {
        return Ok(Vec::new());
    }

    let user_context = fetch_user_context(api_key).ok().flatten();
    let Some(legacy_api_url) = legacy_leaderboards_url(chart_hash) else {
        return Ok(vec![arrowcloud_empty_hard_ex_leaderboard_pane()]);
    };
    let Some(decoded) = fetch_leaderboards(legacy_api_url.as_str(), Some(1))? else {
        return Ok(vec![arrowcloud_empty_hard_ex_leaderboard_pane()]);
    };
    let Some(first_page) = hard_ex_pane_from_response(decoded) else {
        return Ok(vec![arrowcloud_empty_hard_ex_leaderboard_pane()]);
    };

    let mut extra_pages = Vec::new();
    let mut remaining = arrowcloud_target_user_ids(user_context.as_ref());
    update_remaining_targets(
        first_page.scores.as_slice(),
        user_context.as_ref(),
        &mut remaining,
    );
    let total_pages = first_page
        .total_pages
        .max(first_page.page.max(1) + u32::from(first_page.has_next))
        .max(1);
    let mut page = first_page.page.max(1).saturating_add(1);

    while !remaining.is_empty() && page <= total_pages {
        match fetch_leaderboards(legacy_api_url.as_str(), Some(page)) {
            Ok(Some(decoded)) => {
                let Some(hard_ex_page) = hard_ex_pane_from_response(decoded) else {
                    page += 1;
                    continue;
                };
                update_remaining_targets(
                    hard_ex_page.scores.as_slice(),
                    user_context.as_ref(),
                    &mut remaining,
                );
                extra_pages.push(hard_ex_page);
            }
            Ok(None) | Err(_) => break,
        }
        page += 1;
    }

    Ok(vec![hard_ex_pane_from_pages(
        first_page,
        extra_pages,
        user_context.as_ref(),
    )])
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

#[inline(always)]
fn mask_labels_u8(mask: u8, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u8 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
fn mask_labels_u16(mask: u16, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u16 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
pub const fn turn_label(turn: profile_data::TurnOption) -> &'static str {
    match turn {
        profile_data::TurnOption::None => "None",
        profile_data::TurnOption::Mirror => "Mirror",
        profile_data::TurnOption::Left => "Left",
        profile_data::TurnOption::Right => "Right",
        profile_data::TurnOption::LRMirror => "LR-Mirror",
        profile_data::TurnOption::UDMirror => "UD-Mirror",
        profile_data::TurnOption::Shuffle
        | profile_data::TurnOption::Blender
        | profile_data::TurnOption::Random => "Shuffle",
    }
}

#[inline(always)]
pub fn scroll_label(scroll: profile_data::ScrollOption) -> Option<String> {
    if scroll.contains(profile_data::ScrollOption::Reverse) {
        Some("Reverse".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Split) {
        Some("Split".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Alternate) {
        Some("Alternate".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Cross) {
        Some("Cross".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Centered) {
        Some("Centered".to_string())
    } else {
        None
    }
}

#[inline(always)]
pub fn speed_payload(speed: ScrollSpeedSetting) -> ArrowCloudSpeed {
    match speed {
        ScrollSpeedSetting::CMod(value) => ArrowCloudSpeed {
            value: value as f64,
            speed_type: "C",
        },
        ScrollSpeedSetting::MMod(value) => ArrowCloudSpeed {
            value: value as f64,
            speed_type: "M",
        },
        ScrollSpeedSetting::XMod(value) => ArrowCloudSpeed {
            value: ((value as f64) * 100.0).round() / 100.0,
            speed_type: "X",
        },
    }
}

pub fn modifiers_from_profile(profile: &Profile) -> ArrowCloudModifiers {
    ArrowCloudModifiers {
        visual_delay: profile.visual_delay_ms,
        acceleration: mask_labels_u8(
            profile.accel_effects_active_mask.bits(),
            &ARROWCLOUD_ACCEL_NAMES,
        ),
        appearance: mask_labels_u8(
            profile.appearance_effects_active_mask.bits(),
            &ARROWCLOUD_APPEARANCE_NAMES,
        ),
        effect: mask_labels_u16(
            profile.visual_effects_active_mask.bits(),
            &ARROWCLOUD_EFFECT_NAMES,
        ),
        mini: profile.mini_percent.clamp(-100, 150),
        turn: turn_label(profile.turn_option).to_string(),
        disabled_windows: "None".to_string(),
        speed: speed_payload(profile.scroll_speed),
        perspective: profile.perspective.to_string(),
        noteskin: profile.noteskin.as_str().to_string(),
        scroll: scroll_label(profile.scroll_option),
    }
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

#[inline(always)]
pub fn life_lerp_at(life_history: &[(f32, f32)], sample_time: f32) -> f32 {
    let Some(&(_, first_life)) = life_history.first() else {
        return 0.0;
    };
    if life_history.len() == 1 {
        return first_life.clamp(0.0, 1.0);
    }

    let later_ix = life_history.partition_point(|&(time, _)| time <= sample_time);
    let earlier_ix = later_ix.saturating_sub(1).min(life_history.len() - 1);
    let (earlier_time, earlier_life) = life_history[earlier_ix];
    if later_ix >= life_history.len() {
        return earlier_life.clamp(0.0, 1.0);
    }

    let (later_time, later_life) = life_history[later_ix];
    let dt = later_time - earlier_time;
    if dt.abs() <= f32::EPSILON {
        return earlier_life.clamp(0.0, 1.0);
    }
    let alpha = ((sample_time - earlier_time) / dt).clamp(0.0, 1.0);
    (earlier_life + (later_life - earlier_life) * alpha).clamp(0.0, 1.0)
}

pub fn lifebar_points(
    life_history: &[(f32, f32)],
    chart_start_second: f32,
    first_second: f32,
    last_second: f32,
    point_count: usize,
) -> Vec<ArrowCloudLifePoint> {
    if life_history.is_empty() || point_count == 0 {
        return Vec::new();
    }
    let last_second = last_second.max(first_second);
    let duration = (last_second - first_second).max(0.0);
    let step = duration / point_count as f32;
    let mut out = Vec::with_capacity(point_count);
    for i in 0..point_count {
        let x = chart_start_second + (i as f32 * step);
        out.push(ArrowCloudLifePoint {
            x: x as f64,
            y: life_lerp_at(life_history, x) as f64,
        });
    }
    out
}

pub fn nps_info_from_measure_data(
    max_nps: f64,
    measure_nps: &[f64],
    measure_seconds: &[f32],
    first_second: f32,
    last_second: f32,
) -> ArrowCloudNpsInfo {
    let peak_nps = if max_nps.is_finite() && max_nps > 0.0 {
        max_nps
    } else {
        0.0
    };

    let mut points = Vec::with_capacity(measure_nps.len());
    let mut started = false;
    for (measure, nps) in measure_nps.iter().copied().enumerate() {
        if !nps.is_finite() {
            continue;
        }
        if nps > 0.0 {
            started = true;
        }
        if !started {
            continue;
        }
        let Some(&time) = measure_seconds.get(measure) else {
            continue;
        };
        let x = if last_second > first_second {
            ((time - first_second) / (last_second - first_second)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let y = if peak_nps > 0.0 {
            (nps / peak_nps).clamp(0.0, 1.0)
        } else {
            0.0
        };
        points.push(ArrowCloudNpsPoint {
            x: x as f64,
            y,
            measure: measure as u32,
            nps,
        });
    }

    ArrowCloudNpsInfo { peak_nps, points }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ArrowCloudTimingOffset {
    Seconds(f64),
    Miss(&'static str),
}

pub type ArrowCloudTimingDatum = (f64, ArrowCloudTimingOffset);

#[inline(always)]
pub fn format_length(seconds: f32) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0:00".to_string();
    }
    let total = seconds.floor() as i64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            (total % 3600) / 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

pub fn timing_data_from_scatter(
    scatter: &[ScatterPoint],
    fail_time_s: Option<f32>,
) -> Vec<ArrowCloudTimingDatum> {
    let mut out = Vec::with_capacity(scatter.len());
    for point in scatter {
        if !point.time_sec.is_finite() {
            continue;
        }
        if let Some(fail_time) = fail_time_s
            && point.time_sec > fail_time
        {
            continue;
        }
        let value = if let Some(offset_ms) = point.offset_ms {
            if !offset_ms.is_finite() {
                continue;
            }
            ArrowCloudTimingOffset::Seconds((offset_ms / 1000.0) as f64)
        } else {
            ArrowCloudTimingOffset::Miss("Miss")
        };
        out.push((point.time_sec as f64, value));
    }
    out
}

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

pub fn judgment_counts_from_stats(
    counts: judgment::JudgeCounts,
    windows: WindowCounts,
    holds_held: u32,
    total_holds: u32,
    mines_hit: u32,
    total_mines: u32,
    rolls_held: u32,
    total_rolls: u32,
) -> ArrowCloudJudgmentCounts {
    let fantastic_total = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)];
    let fantastic_plus = windows.w0;
    let fantastic = fantastic_total.saturating_sub(fantastic_plus);
    let excellent = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Excellent)];
    let great = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Great)];
    let decent = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Decent)];
    let way_off = counts[judgment::judge_grade_ix(judgment::JudgeGrade::WayOff)];
    let miss = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Miss)];
    let mut total_steps = 0u32;
    for count in counts {
        total_steps = total_steps.saturating_add(count);
    }

    ArrowCloudJudgmentCounts {
        fantastic_plus,
        fantastic,
        excellent,
        great,
        decent,
        way_off,
        miss,
        total_steps,
        holds_held,
        total_holds,
        mines_hit,
        total_mines,
        rolls_held,
        total_rolls,
    }
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

#[derive(Debug, Clone)]
pub struct ArrowCloudPayloadParts {
    pub song_name: String,
    pub artist: String,
    pub pack: String,
    pub music_length_seconds: f32,
    pub hash: String,
    pub timing_data: Vec<ArrowCloudTimingDatum>,
    pub difficulty: u32,
    pub stepartist: String,
    pub submit_stats: ArrowCloudSubmitStats,
    pub total_holds: u32,
    pub total_mines: u32,
    pub total_rolls: u32,
    pub nps_info: ArrowCloudNpsInfo,
    pub lifebar_info: Vec<ArrowCloudLifePoint>,
    pub modifiers: ArrowCloudModifiers,
    pub music_rate: f32,
    pub used_autoplay: bool,
    pub passed: bool,
}

impl ArrowCloudPayload {
    pub fn fill_metadata(&mut self) {
        self.body_version = ARROWCLOUD_BODY_VERSION;
        self.arrow_cloud_body_version = ARROWCLOUD_BODY_VERSION;
        self.engine_name = ARROWCLOUD_ENGINE_NAME;
        self.engine_version = deadsync_version::current_static();
    }
}

#[inline(always)]
pub fn submit_music_rate(music_rate: f32) -> f64 {
    if music_rate.is_finite() && music_rate > 0.0 {
        music_rate as f64
    } else {
        1.0
    }
}

pub fn payload_from_parts(input: ArrowCloudPayloadParts) -> ArrowCloudPayload {
    let mut payload = ArrowCloudPayload {
        song_name: input.song_name,
        artist: input.artist,
        pack: input.pack.trim().to_string(),
        length: format_length(input.music_length_seconds),
        hash: input.hash,
        timing_data: input.timing_data,
        difficulty: input.difficulty,
        stepartist: input.stepartist,
        radar: ArrowCloudRadar {
            holds: [input.submit_stats.holds_held, input.total_holds],
            mines: [input.submit_stats.mines_avoided, input.total_mines],
            rolls: [input.submit_stats.rolls_held, input.total_rolls],
        },
        judgment_counts: judgment_counts_from_stats(
            input.submit_stats.judgment_counts,
            input.submit_stats.window_counts,
            input.submit_stats.holds_held,
            input.total_holds,
            input.submit_stats.mines_hit,
            input.total_mines,
            input.submit_stats.rolls_held,
            input.total_rolls,
        ),
        nps_info: input.nps_info,
        lifebar_info: input.lifebar_info,
        modifiers: input.modifiers,
        music_rate: submit_music_rate(input.music_rate),
        used_autoplay: input.used_autoplay,
        passed: input.passed,
        body_version: "",
        arrow_cloud_body_version: "",
        engine_name: "",
        engine_version: "",
    };
    payload.fill_metadata();
    payload
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
        client_version: Some(format!("deadsync {}", deadsync_version::current())),
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
    use deadsync_profile::{
        AccelEffectsMask, AppearanceEffectsMask, Perspective, ScrollOption, TurnOption,
        VisualEffectsMask,
    };
    use deadsync_rules::{judgment, scroll::ScrollSpeedSetting, timing::WindowCounts};
    use deadsync_score::{
        ArrowCloudPaneKind, ArrowCloudServerGrade, ArrowCloudSubmitUiStatus, ArrowCloudUserContext,
        RejectReason,
    };
    use std::collections::HashSet;
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
    fn modifiers_from_profile_maps_arrowcloud_labels() {
        let mut profile = Profile::default();
        profile.visual_delay_ms = -12;
        profile.accel_effects_active_mask = AccelEffectsMask::BOOST | AccelEffectsMask::WAVE;
        profile.appearance_effects_active_mask =
            AppearanceEffectsMask::HIDDEN | AppearanceEffectsMask::RANDOM_VANISH;
        profile.visual_effects_active_mask = VisualEffectsMask::DRUNK | VisualEffectsMask::BUMPY;
        profile.mini_percent = 200;
        profile.turn_option = TurnOption::Random;
        profile.scroll_speed = ScrollSpeedSetting::XMod(2.345);
        profile.perspective = Perspective::Space;
        profile.scroll_option = ScrollOption::Alternate;

        let modifiers = modifiers_from_profile(&profile);
        assert_eq!(modifiers.visual_delay, -12);
        assert_eq!(modifiers.acceleration, ["Boost", "Wave"]);
        assert_eq!(modifiers.appearance, ["Hidden", "R.Vanish"]);
        assert_eq!(modifiers.effect, ["Drunk", "Bumpy"]);
        assert_eq!(modifiers.mini, 150);
        assert_eq!(modifiers.turn, "Shuffle");
        assert_eq!(modifiers.disabled_windows, "None");
        assert_eq!(modifiers.speed.value, 2.35);
        assert_eq!(modifiers.speed.speed_type, "X");
        assert_eq!(modifiers.perspective, "Space");
        assert_eq!(modifiers.noteskin, profile.noteskin.as_str());
        assert_eq!(modifiers.scroll.as_deref(), Some("Alternate"));
    }

    #[test]
    fn format_length_matches_arrowcloud_payload_shape() {
        assert_eq!(format_length(f32::NAN), "0:00");
        assert_eq!(format_length(-1.0), "0:00");
        assert_eq!(format_length(83.9), "1:23");
        assert_eq!(format_length(3_661.0), "1:01:01");
    }

    #[test]
    fn timing_data_from_scatter_keeps_misses_and_fail_cutoff() {
        let scatter = [
            ScatterPoint {
                time_sec: 1.0,
                offset_ms: Some(8.0),
                direction_code: 1,
                is_stream: false,
                is_left_foot: false,
                miss_because_held: false,
            },
            ScatterPoint {
                time_sec: 1.5,
                offset_ms: None,
                direction_code: 2,
                is_stream: false,
                is_left_foot: false,
                miss_because_held: false,
            },
            ScatterPoint {
                time_sec: 3.0,
                offset_ms: Some(1.0),
                direction_code: 3,
                is_stream: false,
                is_left_foot: false,
                miss_because_held: false,
            },
        ];

        let timing_data = timing_data_from_scatter(&scatter, Some(2.0));
        let value = serde_json::to_value(&timing_data).expect("serialize timing data");
        assert_eq!(value[0][0], 1.0);
        let first_offset = value[0][1].as_f64().expect("numeric timing offset");
        assert!((first_offset - 0.008).abs() < 1e-6);
        assert_eq!(value[1][0], 1.5);
        assert_eq!(value[1][1], "Miss");
        assert_eq!(timing_data.len(), 2);
    }

    #[test]
    fn lifebar_points_interpolate_life_history() {
        let life_history = [(0.0, 0.0), (10.0, 1.0)];
        let points = lifebar_points(&life_history, 0.0, 0.0, 10.0, 5);

        assert_eq!(points.len(), 5);
        assert!((points[0].x - 0.0).abs() < 1e-6);
        assert!((points[0].y - 0.0).abs() < 1e-6);
        assert!((points[1].x - 2.0).abs() < 1e-6);
        assert!((points[1].y - 0.2).abs() < 1e-6);
        assert!((points[4].x - 8.0).abs() < 1e-6);
        assert!((points[4].y - 0.8).abs() < 1e-6);
    }

    #[test]
    fn nps_info_from_measure_data_skips_leading_zeroes() {
        let info =
            nps_info_from_measure_data(20.0, &[0.0, 10.0, 20.0], &[0.0, 5.0, 10.0], 0.0, 10.0);

        assert_eq!(info.peak_nps, 20.0);
        assert_eq!(info.points.len(), 2);
        assert_eq!(info.points[0].measure, 1);
        assert!((info.points[0].x - 0.5).abs() < 1e-6);
        assert!((info.points[0].y - 0.5).abs() < 1e-6);
        assert_eq!(info.points[0].nps, 10.0);
        assert_eq!(info.points[1].measure, 2);
        assert!((info.points[1].x - 1.0).abs() < 1e-6);
        assert!((info.points[1].y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn judgment_counts_from_stats_splits_fa_plus() {
        let mut counts = [0u32; judgment::JUDGE_GRADE_COUNT];
        counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)] = 7;
        counts[judgment::judge_grade_ix(judgment::JudgeGrade::Excellent)] = 3;
        counts[judgment::judge_grade_ix(judgment::JudgeGrade::Miss)] = 2;

        let out = judgment_counts_from_stats(
            counts,
            WindowCounts {
                w0: 2,
                ..WindowCounts::default()
            },
            4,
            5,
            1,
            6,
            2,
            3,
        );

        assert_eq!(out.fantastic_plus, 2);
        assert_eq!(out.fantastic, 5);
        assert_eq!(out.excellent, 3);
        assert_eq!(out.miss, 2);
        assert_eq!(out.total_steps, 12);
        assert_eq!(out.holds_held, 4);
        assert_eq!(out.total_holds, 5);
        assert_eq!(out.mines_hit, 1);
        assert_eq!(out.total_mines, 6);
        assert_eq!(out.rolls_held, 2);
        assert_eq!(out.total_rolls, 3);
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
    fn retrieve_score_cache_entries_validates_request_before_network() {
        let hashes = vec!["006fb5c4890e98a2".to_string()];
        let missing_key = retrieve_score_cache_entries(
            "   ",
            None,
            hashes.as_slice(),
            &ArrowCloudLeaderboard::ALL_GLOBAL,
        );
        assert!(matches!(missing_key, Err(OnlineRequestError::Request(_))));

        let empty_hashes =
            retrieve_score_cache_entries("key", None, &[], &ArrowCloudLeaderboard::ALL_GLOBAL)
                .expect("empty request");
        assert!(empty_hashes.is_empty());

        let too_many = vec!["deadbeef".to_string(); ARROWCLOUD_BULK_MAX_HASHES + 1];
        let overflow = retrieve_score_cache_entries(
            "key",
            None,
            too_many.as_slice(),
            &ArrowCloudLeaderboard::ALL_GLOBAL,
        );
        assert!(matches!(overflow, Err(OnlineRequestError::Request(_))));
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
    fn hard_ex_pane_from_response_filters_to_hardex() {
        let pane = hard_ex_pane_from_response(ArrowCloudLeaderboardsApiResponse {
            leaderboards: vec![
                ArrowCloudLeaderboardPane {
                    r#type: "EX".to_string(),
                    scores: Vec::new(),
                    page: 1,
                    has_next: false,
                    total_pages: 1,
                },
                ArrowCloudLeaderboardPane {
                    r#type: "HardEX".to_string(),
                    scores: vec![ArrowCloudLeaderboardEntry {
                        rank: 7,
                        score: 98.31,
                        alias: "YOU".to_string(),
                        date: "2026-04-18T12:34:56.000Z".to_string(),
                        user_id: "self".to_string(),
                        is_rival: false,
                        is_self: false,
                    }],
                    page: 1,
                    has_next: true,
                    total_pages: 4,
                },
            ],
        })
        .expect("expected HardEX pane");

        assert_eq!(pane.r#type, "HardEX");
        assert_eq!(pane.page, 1);
        assert!(pane.has_next);
        assert_eq!(pane.total_pages, 4);
        assert_eq!(pane.scores.len(), 1);
        assert_eq!(pane.scores[0].user_id, "self");
    }

    #[test]
    fn hard_ex_pane_from_pages_marks_self_and_rival_from_user_ids() {
        let context = ArrowCloudUserContext {
            self_user_id: Some("self".to_string()),
            rival_user_ids: HashSet::from([String::from("rival")]),
        };
        let pane = hard_ex_pane_from_pages(
            ArrowCloudLeaderboardPane {
                r#type: "HardEX".to_string(),
                scores: vec![ArrowCloudLeaderboardEntry {
                    rank: 1,
                    score: 99.12,
                    alias: "AAA".to_string(),
                    date: String::new(),
                    user_id: "top".to_string(),
                    is_rival: false,
                    is_self: false,
                }],
                page: 1,
                has_next: true,
                total_pages: 4,
            },
            vec![ArrowCloudLeaderboardPane {
                r#type: "HardEX".to_string(),
                scores: vec![
                    ArrowCloudLeaderboardEntry {
                        rank: 81,
                        score: 88.99,
                        alias: "YOU".to_string(),
                        date: String::new(),
                        user_id: "self".to_string(),
                        is_rival: false,
                        is_self: false,
                    },
                    ArrowCloudLeaderboardEntry {
                        rank: 91,
                        score: 87.65,
                        alias: "RIVAL".to_string(),
                        date: String::new(),
                        user_id: "rival".to_string(),
                        is_rival: false,
                        is_self: false,
                    },
                ],
                page: 4,
                has_next: false,
                total_pages: 4,
            }],
            Some(&context),
        );

        assert_eq!(pane.arrowcloud_kind, Some(ArrowCloudPaneKind::HardEx));
        assert!(pane.personalized);
        assert_eq!(pane.entries.len(), 3);
        assert_eq!(pane.entries[1].rank, 81);
        assert!(pane.entries[1].is_self);
        assert_eq!(pane.entries[2].rank, 91);
        assert!(pane.entries[2].is_rival);
    }

    #[test]
    fn fetch_hard_ex_leaderboard_panes_skips_empty_inputs() {
        assert!(
            fetch_hard_ex_leaderboard_panes("", "key")
                .expect("empty chart hash")
                .is_empty()
        );
        assert!(
            fetch_hard_ex_leaderboard_panes("deadbeef", "")
                .expect("empty api key")
                .is_empty()
        );
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

    fn retrieve_entry(score: f64, is_fail: bool) -> ArrowCloudRetrieveScoreEntry {
        ArrowCloudRetrieveScoreEntry {
            score: Some(score),
            grade: None,
            date: None,
            play_id: None,
            is_fail,
        }
    }

    fn retrieve_entry_full(
        score: f64,
        grade: Option<&str>,
        date: Option<&str>,
        play_id: Option<i64>,
        is_fail: bool,
    ) -> ArrowCloudRetrieveScoreEntry {
        ArrowCloudRetrieveScoreEntry {
            score: Some(score),
            grade: grade.map(str::to_string),
            date: date.map(str::to_string),
            play_id,
            is_fail,
        }
    }

    #[test]
    fn scores_from_retrieve_entry_map_assigns_global_leaderboards() {
        let mut map = HashMap::new();
        map.insert("4".to_string(), retrieve_entry(99.51, false));
        map.insert("2".to_string(), retrieve_entry(98.10, false));
        map.insert("3".to_string(), retrieve_entry(99.89, false));
        let scores = scores_from_retrieve_entry_map(&map);
        assert!(scores.itg.is_some());
        assert!(scores.ex.is_some());
        assert!(scores.hard_ex.is_some());
        assert!((scores.itg.unwrap().score_percent - 0.9989).abs() < 1e-6);
        assert!((scores.ex.unwrap().score_percent - 0.9810).abs() < 1e-6);
        assert!((scores.hard_ex.unwrap().score_percent - 0.9951).abs() < 1e-6);
    }

    #[test]
    fn scores_from_retrieve_entry_map_ignores_invalid_leaderboards() {
        let mut map = HashMap::new();
        map.insert("3".to_string(), retrieve_entry(99.0, false));
        map.insert("9".to_string(), retrieve_entry(95.0, false));
        map.insert("itg".to_string(), retrieve_entry(97.0, false));
        let scores = scores_from_retrieve_entry_map(&map);
        assert!(scores.itg.is_some());
        assert!(scores.ex.is_none());
        assert!(scores.hard_ex.is_none());
    }

    #[test]
    fn scores_from_retrieve_entry_map_drops_entries_without_score() {
        let mut map = HashMap::new();
        map.insert(
            "3".to_string(),
            ArrowCloudRetrieveScoreEntry {
                score: None,
                grade: None,
                date: None,
                play_id: None,
                is_fail: false,
            },
        );
        let scores = scores_from_retrieve_entry_map(&map);
        assert!(scores.itg.is_none(), "missing score must not cache as 0%");
    }

    #[test]
    fn score_from_retrieve_entry_preserves_native_metadata() {
        let score = score_from_retrieve_entry(&retrieve_entry_full(
            99.89,
            Some("Tristar"),
            Some("2026-05-03T19:10:17.504Z"),
            Some(12345),
            false,
        ))
        .expect("score");

        assert_eq!(score.server_grade, Some(ArrowCloudServerGrade::Tristar));
        assert_eq!(score.play_id, Some(12345));
        let played_at = score.played_at.expect("played_at parsed");
        assert_eq!(played_at.timestamp_millis(), 1_777_835_417_504);
    }

    #[test]
    fn score_from_retrieve_entry_drops_bad_metadata() {
        let unknown_grade = score_from_retrieve_entry(&retrieve_entry_full(
            98.0,
            Some("Mythic"),
            None,
            None,
            false,
        ))
        .expect("score");
        assert_eq!(unknown_grade.server_grade, None);

        let bad_date = score_from_retrieve_entry(&retrieve_entry_full(
            98.0,
            None,
            Some("not-a-date"),
            None,
            false,
        ))
        .expect("score");
        assert_eq!(bad_date.played_at, None);
    }

    #[test]
    fn score_cache_entries_from_retrieve_response_drops_empty_charts() {
        let response = ArrowCloudRetrieveScoresResponse {
            scores: HashMap::from([
                (
                    "has-score".to_string(),
                    HashMap::from([("3".to_string(), retrieve_entry(99.0, false))]),
                ),
                (
                    "empty".to_string(),
                    HashMap::from([(
                        "3".to_string(),
                        ArrowCloudRetrieveScoreEntry {
                            score: None,
                            grade: None,
                            date: None,
                            play_id: None,
                            is_fail: false,
                        },
                    )]),
                ),
            ]),
        };
        let entries = score_cache_entries_from_retrieve_response(response);
        assert!(entries.contains_key("has-score"));
        assert!(!entries.contains_key("empty"));
    }

    #[test]
    fn submit_payload_serializes_miss_and_counts() {
        let mut payload = ArrowCloudPayload {
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
            body_version: "",
            arrow_cloud_body_version: "",
            engine_name: "",
            engine_version: "",
        };
        payload.fill_metadata();

        let value = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(value["timingData"][0][1], serde_json::json!("Miss"));
        assert_eq!(value["judgmentCounts"]["miss"], serde_json::json!(3));
        assert_eq!(value["judgmentCounts"]["wayOff"], serde_json::json!(60));
        assert_eq!(value["radar"]["Holds"], serde_json::json!([1, 2]));
        assert_eq!(value["modifiers"]["speed"]["type"], serde_json::json!("C"));
        assert_eq!(value["bodyVersion"], serde_json::json!("1.4"));
        assert_eq!(value["_arrowCloudBodyVersion"], serde_json::json!("1.4"));
        assert_eq!(value["_engineName"], serde_json::json!("DeadSync"));
        assert_eq!(
            value["_engineVersion"],
            serde_json::json!(deadsync_version::current_static())
        );
    }

    #[test]
    fn payload_from_parts_builds_submit_shape() {
        let payload = payload_from_parts(ArrowCloudPayloadParts {
            song_name: "Test Song".to_string(),
            artist: "Test Artist".to_string(),
            pack: " Test Pack ".to_string(),
            music_length_seconds: 83.5,
            hash: "deadbeefcafebabe".to_string(),
            timing_data: vec![(24.0, ArrowCloudTimingOffset::Miss("Miss"))],
            difficulty: 12,
            stepartist: "Tester".to_string(),
            submit_stats: ArrowCloudSubmitStats {
                judgment_counts: [10, 20, 30, 40, 50, 60],
                window_counts: WindowCounts {
                    w0: 4,
                    ..WindowCounts::default()
                },
                holds_held: 1,
                mines_hit: 2,
                mines_avoided: 3,
                rolls_held: 5,
            },
            total_holds: 2,
            total_mines: 4,
            total_rolls: 6,
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
            music_rate: f32::NAN,
            used_autoplay: false,
            passed: true,
        });

        assert_eq!(payload.pack, "Test Pack");
        assert_eq!(payload.length, "1:23");
        assert_eq!(payload.music_rate, 1.0);
        assert_eq!(payload.radar.holds, [1, 2]);
        assert_eq!(payload.radar.mines, [3, 4]);
        assert_eq!(payload.radar.rolls, [5, 6]);
        assert_eq!(payload.judgment_counts.fantastic_plus, 4);
        assert_eq!(payload.judgment_counts.fantastic, 6);
        assert_eq!(payload.judgment_counts.mines_hit, 2);
        assert_eq!(payload.body_version, ARROWCLOUD_BODY_VERSION);
        assert_eq!(payload.engine_name, ARROWCLOUD_ENGINE_NAME);
    }

    #[test]
    fn submit_error_maps_transport_errors_to_status() {
        let (status, message) =
            submit_error_status_and_message(&ArrowCloudSubmitRequestError::Transport {
                message: "network error: timed out".to_string(),
                timed_out: true,
            });
        assert_eq!(status, ArrowCloudSubmitUiStatus::TimedOut);
        assert_eq!(message, "network error: timed out");

        let (status, _) =
            submit_error_status_and_message(&ArrowCloudSubmitRequestError::Transport {
                message: "network error: refused".to_string(),
                timed_out: false,
            });
        assert_eq!(status, ArrowCloudSubmitUiStatus::NetworkError);
    }

    #[test]
    fn submit_error_maps_protocol_errors_to_status() {
        let (status, _) =
            submit_error_status_and_message(&ArrowCloudSubmitRequestError::InvalidRequest {
                message: "missing ArrowCloud API key".to_string(),
            });
        assert_eq!(
            status,
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
        );

        let (status, _) =
            submit_error_status_and_message(&ArrowCloudSubmitRequestError::InvalidRequest {
                message: "missing chart hash".to_string(),
            });
        assert_eq!(
            status,
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
        );

        let (status, message) =
            submit_error_status_and_message(&ArrowCloudSubmitRequestError::Http {
                status: 403,
                body_snippet: "bad key".to_string(),
            });
        assert_eq!(
            status,
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
        );
        assert_eq!(message, "HTTP 403: bad key");
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

    #[test]
    fn probe_result_transition_selects_status_and_log_intent() {
        assert_eq!(
            connection_transition_from_probe_result(Ok(ConnectionStatus::Connected)),
            ConnectionProbeTransition {
                status: ConnectionStatus::Connected,
                log: Some(ConnectionProbeLog::Connected),
            }
        );

        assert_eq!(
            connection_transition_from_probe_result(Ok(ConnectionStatus::Pending)),
            ConnectionProbeTransition {
                status: ConnectionStatus::Pending,
                log: None,
            }
        );

        assert_eq!(
            connection_transition_from_probe_result(Ok(ConnectionStatus::Error(
                ConnectionError::Disabled
            ))),
            ConnectionProbeTransition {
                status: ConnectionStatus::Error(ConnectionError::Disabled),
                log: None,
            }
        );
    }

    #[test]
    fn probe_result_transition_maps_errors_to_log_intent() {
        assert_eq!(
            connection_transition_from_probe_result(Err(ConnectionProbeError {
                connection_error: ConnectionError::HostBlocked,
                message: "http status 403".to_string(),
            })),
            ConnectionProbeTransition {
                status: ConnectionStatus::Error(ConnectionError::HostBlocked),
                log: Some(ConnectionProbeLog::CannotConnect {
                    error: "http status 403".to_string()
                }),
            }
        );
    }

    #[test]
    fn submit_ui_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "ac-course-status-first";
        let second = "ac-course-status-second";
        reset_submit_ui_status(side, first);
        reset_submit_ui_status(side, second);

        set_submit_ui_status(side, first, 11, ArrowCloudSubmitUiStatus::Submitting);
        set_submit_ui_status(side, second, 12, ArrowCloudSubmitUiStatus::Submitted);

        assert_eq!(
            submit_ui_status_for_side(first, side),
            Some(ArrowCloudSubmitUiStatus::Submitting)
        );
        assert_eq!(
            submit_ui_status_for_side(second, side),
            Some(ArrowCloudSubmitUiStatus::Submitted)
        );
        assert!(update_submit_ui_status_if_token(
            side,
            first,
            11,
            ArrowCloudSubmitUiStatus::TimedOut,
        ));
        assert!(!update_submit_ui_status_if_token(
            side,
            first,
            12,
            ArrowCloudSubmitUiStatus::Submitted,
        ));
        assert_eq!(
            submit_ui_status_for_side(first, side),
            Some(ArrowCloudSubmitUiStatus::TimedOut)
        );
        assert_eq!(
            submit_ui_status_for_side(second, side),
            Some(ArrowCloudSubmitUiStatus::Submitted)
        );

        reset_submit_ui_status(side, first);
        reset_submit_ui_status(side, second);
    }

    #[test]
    fn submit_retry_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "ac-course-retry-first";
        let second = "ac-course-retry-second";
        reset_submit_ui_status(side, first);
        reset_submit_ui_status(side, second);
        reset_submit_retry(side, first);
        reset_submit_retry(side, second);

        store_submit_retry(sample_submit_draft(first, side).retry_entry());
        store_submit_retry(sample_submit_draft(second, side).retry_entry());
        set_submit_ui_status(side, first, 21, ArrowCloudSubmitUiStatus::TimedOut);
        set_submit_ui_status(side, second, 22, ArrowCloudSubmitUiStatus::NetworkError);

        record_submit_failure(side, first, ArrowCloudSubmitUiStatus::TimedOut);
        record_submit_failure(side, second, ArrowCloudSubmitUiStatus::NetworkError);

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
    fn begin_ready_submit_retry_job_arms_ui_and_consumes_retry() {
        let side = profile_data::PlayerSide::P1;
        let hash = "ac-begin-ready-retry";
        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);

        store_submit_retry(sample_submit_draft(hash, side).retry_entry());
        set_submit_ui_status(side, hash, 31, ArrowCloudSubmitUiStatus::TimedOut);

        let job = begin_ready_submit_retry_job(hash, side, true).expect("ready retry job");
        assert_eq!(job.payload.hash, hash);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(ArrowCloudSubmitUiStatus::Submitting)
        );
        assert!(begin_ready_submit_retry_job(hash, side, true).is_none());

        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn begin_submit_jobs_from_drafts_stores_retry_and_sets_ui() {
        let side = profile_data::PlayerSide::P1;
        let hash = "ac-begin-submit-draft";
        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);

        let jobs = begin_submit_jobs_from_drafts(vec![sample_submit_draft(hash, side)]);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].payload.hash, hash);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(ArrowCloudSubmitUiStatus::Submitting)
        );

        assert!(complete_submit_job_failure(
            &jobs[0],
            ArrowCloudSubmitUiStatus::TimedOut
        ));
        assert!(next_retry_remaining_secs(hash, side).is_some());

        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn complete_submit_job_success_updates_ui_and_resets_retry() {
        let side = profile_data::PlayerSide::P1;
        let hash = "ac-complete-success";
        let job = sample_submit_draft(hash, side).submit_job(41);
        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);

        store_submit_retry(sample_submit_draft(hash, side).retry_entry());
        set_submit_ui_status(side, hash, 9, ArrowCloudSubmitUiStatus::TimedOut);
        record_submit_failure(side, hash, ArrowCloudSubmitUiStatus::TimedOut);
        assert!(next_retry_remaining_secs(hash, side).is_some());
        set_submit_ui_status(side, hash, job.token, ArrowCloudSubmitUiStatus::Submitting);

        assert!(complete_submit_job_success(&job));
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(ArrowCloudSubmitUiStatus::Submitted)
        );
        assert_eq!(next_retry_remaining_secs(hash, side), None);

        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn complete_submit_job_failure_records_retry() {
        let side = profile_data::PlayerSide::P1;
        let hash = "ac-complete-failure";
        let job = sample_submit_draft(hash, side).submit_job(42);
        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);

        store_submit_retry(sample_submit_draft(hash, side).retry_entry());
        set_submit_ui_status(side, hash, job.token, ArrowCloudSubmitUiStatus::Submitting);

        assert!(complete_submit_job_failure(
            &job,
            ArrowCloudSubmitUiStatus::TimedOut
        ));
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(ArrowCloudSubmitUiStatus::TimedOut)
        );
        assert!(next_retry_remaining_secs(hash, side).is_some());

        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn run_submit_jobs_with_caches_success_and_runs_after_hook() {
        let side = profile_data::PlayerSide::P1;
        let hash = "ac-run-submit-success";
        let job = sample_submit_draft(hash, side).submit_job(51);
        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
        set_submit_ui_status(side, hash, job.token, ArrowCloudSubmitUiStatus::Submitting);

        let mut cached = 0;
        let mut after = 0;
        let summary = run_submit_jobs_with(vec![job], |_| Ok(()), |_| cached += 1, |_| after += 1);

        assert_eq!(
            summary,
            ArrowCloudSubmitRunSummary {
                succeeded: 1,
                failed: 0
            }
        );
        assert_eq!(cached, 1);
        assert_eq!(after, 1);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(ArrowCloudSubmitUiStatus::Submitted)
        );

        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
    }

    #[test]
    fn run_submit_jobs_with_records_failure_and_runs_after_hook() {
        let side = profile_data::PlayerSide::P1;
        let hash = "ac-run-submit-failure";
        let job = sample_submit_draft(hash, side).submit_job(52);
        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
        store_submit_retry(sample_submit_draft(hash, side).retry_entry());
        set_submit_ui_status(side, hash, job.token, ArrowCloudSubmitUiStatus::Submitting);

        let mut cached = 0;
        let mut after = 0;
        let summary = run_submit_jobs_with(
            vec![job],
            |_| {
                Err(ArrowCloudSubmitError {
                    status: ArrowCloudSubmitUiStatus::TimedOut,
                    message: "timed out".to_string(),
                })
            },
            |_| cached += 1,
            |_| after += 1,
        );

        assert_eq!(
            summary,
            ArrowCloudSubmitRunSummary {
                succeeded: 0,
                failed: 1
            }
        );
        assert_eq!(cached, 0);
        assert_eq!(after, 1);
        assert_eq!(
            submit_ui_status_for_side(hash, side),
            Some(ArrowCloudSubmitUiStatus::TimedOut)
        );
        assert!(next_retry_remaining_secs(hash, side).is_some());

        reset_submit_ui_status(side, hash);
        reset_submit_retry(side, hash);
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

    fn sample_payload(hash: &str) -> ArrowCloudPayload {
        let mut payload = ArrowCloudPayload {
            song_name: "Test Song".to_string(),
            artist: "Test Artist".to_string(),
            pack: "Test Pack".to_string(),
            length: "1:23".to_string(),
            hash: hash.to_string(),
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
            body_version: "",
            arrow_cloud_body_version: "",
            engine_name: "",
            engine_version: "",
        };
        payload.fill_metadata();
        payload
    }

    fn sample_submit_draft(hash: &str, side: profile_data::PlayerSide) -> ArrowCloudSubmitDraft {
        ArrowCloudSubmitDraft::new(
            side,
            "test-api-key".to_string(),
            sample_payload(hash),
            None,
            99.0,
            98.0,
            97.0,
            false,
        )
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
