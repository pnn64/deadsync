use crate::engine::network;
use log::{debug, info, warn};
use std::sync::{LazyLock, Mutex};

const ARROWCLOUD_API_BASE_URL: &str = "https://api.arrowcloud.dance";

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

pub fn init() {
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
