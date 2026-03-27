use crate::engine::network;
use log::{debug, info, warn};
use std::sync::{LazyLock, Mutex};

const ARROWCLOUD_API_BASE_URL: &str = "https://api.arrowcloud.dance";

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Pending,
    Connected,
    Error(String),
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
        set_status(ConnectionStatus::Error("Disabled".to_string()));
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
            let message = error.to_string();
            let lower = message.to_ascii_lowercase();
            let status = if lower.contains("timeout") || lower.contains("timed out") {
                ConnectionStatus::Error("Timed Out".to_string())
            } else {
                ConnectionStatus::Error(format!("HTTP error: {error}"))
            };
            warn!("HTTP error to ArrowCloud: {error}");
            set_status(status);
        }
    }
}
