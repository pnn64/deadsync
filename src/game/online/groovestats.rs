use crate::engine::network;
use log::{debug, info, warn};
use serde::Deserialize;
use std::sync::{LazyLock, Mutex};

const GROOVESTATS_API_BASE_URL: &str = "https://api.groovestats.com";
const BOOGIESTATS_API_BASE_URL: &str = "https://boogiestats.andr.host";
const GROOVESTATS_QR_BASE_URL: &str = "https://www.groovestats.com";
const GROOVESTATS_NEW_SESSION_PATH: &str = "new-session.php?chartHashVersion=3";

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ServicesAllowed {
    player_scores: bool,
    player_leaderboards: bool,
    score_submit: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ApiResponse {
    services_allowed: ServicesAllowed,
    services_result: String,
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

static STATUS: LazyLock<Mutex<ConnectionStatus>> =
    LazyLock::new(|| Mutex::new(ConnectionStatus::Pending));

#[inline(always)]
fn set_status(status: ConnectionStatus) {
    *STATUS.lock().unwrap() = status;
}

pub fn get_status() -> ConnectionStatus {
    STATUS.lock().unwrap().clone()
}

pub fn is_boogiestats_active() -> bool {
    let cfg = crate::config::get();
    cfg.enable_groovestats && cfg.enable_boogiestats
}

#[inline(always)]
pub fn service_name() -> &'static str {
    if is_boogiestats_active() {
        "BoogieStats"
    } else {
        "GrooveStats"
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
pub fn api_base_url() -> &'static str {
    if is_boogiestats_active() {
        BOOGIESTATS_API_BASE_URL
    } else {
        GROOVESTATS_API_BASE_URL
    }
}

#[inline(always)]
pub const fn qr_base_url() -> &'static str {
    GROOVESTATS_QR_BASE_URL
}

#[inline(always)]
pub fn player_leaderboards_url() -> String {
    format!(
        "{}/player-leaderboards.php",
        api_base_url().trim_end_matches('/')
    )
}

#[inline(always)]
pub fn score_submit_url() -> String {
    format!("{}/score-submit.php", api_base_url().trim_end_matches('/'))
}

#[inline(always)]
fn new_session_url() -> String {
    format!(
        "{}/{}",
        api_base_url().trim_end_matches('/'),
        GROOVESTATS_NEW_SESSION_PATH
    )
}

pub fn init() {
    let cfg = crate::config::get();
    if !cfg.enable_groovestats {
        set_status(ConnectionStatus::Error(ConnectionError::Disabled));
        return;
    }

    let service_name = service_name();
    set_status(ConnectionStatus::Pending);
    debug!("Initializing {service_name} network check...");
    network::spawn_request(perform_check);
}

fn perform_check() {
    let service_name = service_name();
    debug!("Performing {service_name} connectivity check...");

    match network::get_json_with::<ApiResponse>(&network::get_groovestats_agent(), &new_session_url()) {
        Ok(data) => {
            if !data.services_result.eq_ignore_ascii_case("OK") {
                warn!("{service_name} servicesResult != OK.");
                set_status(ConnectionStatus::Error(ConnectionError::MachineOffline));
                return;
            }

            let services = Services {
                get_scores: data.services_allowed.player_scores,
                leaderboard: data.services_allowed.player_leaderboards,
                auto_submit: data.services_allowed.score_submit,
            };
            info!(
                "Connected to {service_name} (scores={}, leaderboards={}, autosubmit={}).",
                services.get_scores, services.leaderboard, services.auto_submit
            );
            set_status(ConnectionStatus::Connected(services));
        }
        Err(network::NetworkError::Timeout) => {
            warn!("{service_name} connectivity check timed out.");
            set_status(ConnectionStatus::Error(ConnectionError::TimedOut));
        }
        Err(network::NetworkError::Decode(error)) => {
            warn!("Failed to parse {service_name} response: {error}");
            set_status(ConnectionStatus::Error(ConnectionError::InvalidResponse));
        }
        Err(error) => {
            warn!("HTTP error to {service_name}: {error}");
            set_status(ConnectionStatus::Error(ConnectionError::CannotConnect));
        }
    }
}
