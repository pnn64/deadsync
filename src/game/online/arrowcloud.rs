use crate::engine::network;
use log::{debug, info, warn};
use std::sync::{LazyLock, Mutex};

const ARROWCLOUD_API_BASE_URL: &str = "https://api.arrowcloud.dance";
const ARROWCLOUD_USER_URL: &str = "https://api.arrowcloud.dance/user";

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

#[cfg(test)]
mod tests {
    use super::{leaderboards_url, legacy_leaderboards_url, user_url};

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
}
