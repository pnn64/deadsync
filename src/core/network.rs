use log::{info, warn};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const GROOVESTATS_API_URL: &str = "https://api.groovestats.com/new-session.php?chartHashVersion=3";
const ARROWCLOUD_API_URL: &str = "https://api.arrowcloud.dance/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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
    services_result: String, // "OK" when healthy
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Services {
    pub get_scores: bool,
    pub leaderboard: bool,
    pub auto_submit: bool,
}

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Pending,
    Connected(Services),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ArrowCloudConnectionStatus {
    Pending,
    Connected,
    Error(String),
}

static CONNECTION_STATUS: std::sync::LazyLock<Arc<Mutex<ConnectionStatus>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(ConnectionStatus::Pending)));
static ARROWCLOUD_CONNECTION_STATUS: std::sync::LazyLock<Arc<Mutex<ArrowCloudConnectionStatus>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(ArrowCloudConnectionStatus::Pending)));

pub fn get_status() -> ConnectionStatus {
    CONNECTION_STATUS.lock().unwrap().clone()
}

pub fn get_arrowcloud_status() -> ArrowCloudConnectionStatus {
    ARROWCLOUD_CONNECTION_STATUS.lock().unwrap().clone()
}

fn set_status(new_status: ConnectionStatus) {
    *CONNECTION_STATUS.lock().unwrap() = new_status;
}

fn set_arrowcloud_status(new_status: ArrowCloudConnectionStatus) {
    *ARROWCLOUD_CONNECTION_STATUS.lock().unwrap() = new_status;
}

/// Exposes the globally configured ureq Agent for other network requests.
pub fn get_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into()
}

pub fn init() {
    let cfg = crate::config::get();

    if cfg.enable_groovestats {
        set_status(ConnectionStatus::Pending);
        info!("Initializing GrooveStats network check...");
        thread::spawn(perform_check);
    } else {
        set_status(ConnectionStatus::Error("Disabled".into()));
    }

    if cfg.enable_arrowcloud {
        set_arrowcloud_status(ArrowCloudConnectionStatus::Pending);
        info!("Initializing ArrowCloud network check...");
        thread::spawn(perform_arrowcloud_check);
    } else {
        set_arrowcloud_status(ArrowCloudConnectionStatus::Error("Disabled".into()));
    }
}

fn perform_check() {
    info!("Performing GrooveStats connectivity check...");

    let agent = get_agent();
    match agent.get(GROOVESTATS_API_URL).call() {
        Ok(resp) => {
            let mut body = resp.into_body();
            match body.read_json::<ApiResponse>() {
                Ok(data) => {
                    if data.services_result == "OK" {
                        println!("Connected to GrooveStats!"); // per your requirement
                        info!("Successfully connected to GrooveStats.");
                        let services = Services {
                            get_scores: data.services_allowed.player_scores,
                            leaderboard: data.services_allowed.player_leaderboards,
                            auto_submit: data.services_allowed.score_submit,
                        };
                        set_status(ConnectionStatus::Connected(services));
                    } else {
                        warn!("servicesResult != OK");
                        set_status(ConnectionStatus::Error("Service not OK".into()));
                    }
                }
                Err(e) => {
                    warn!("Failed to parse GrooveStats response: {e}");
                    set_status(ConnectionStatus::Error("Failed to Parse".into()));
                }
            }
        }
        Err(e) => {
            warn!("HTTP error to GrooveStats: {e}");
            set_status(ConnectionStatus::Error(format!("HTTP error: {e}")));
        }
    }
}

fn perform_arrowcloud_check() {
    info!("Performing ArrowCloud connectivity check...");

    let agent = get_agent();
    match agent.get(ARROWCLOUD_API_URL).call() {
        Ok(_) => {
            info!("Successfully connected to ArrowCloud.");
            set_arrowcloud_status(ArrowCloudConnectionStatus::Connected);
        }
        Err(e) => {
            warn!("HTTP error to ArrowCloud: {e}");
            set_arrowcloud_status(ArrowCloudConnectionStatus::Error(format!(
                "HTTP error: {e}"
            )));
        }
    }
}
