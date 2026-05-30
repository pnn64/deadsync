use deadsync_net as network;
use deadsync_online::groovestats::{self as groovestats_api, ConnectionError, ConnectionStatus};
use log::{debug, info, warn};
use std::sync::{LazyLock, Mutex};

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
pub fn active_service() -> groovestats_api::Service {
    groovestats_api::Service::from_boogiestats_active(is_boogiestats_active())
}

pub fn init() {
    let cfg = crate::config::get();
    if !cfg.enable_groovestats {
        set_status(ConnectionStatus::Error(ConnectionError::Disabled));
        return;
    }

    let service = active_service();
    let service_name = groovestats_api::service_name(service);
    set_status(ConnectionStatus::Pending);
    debug!("Initializing {service_name} network check...");
    network::spawn_request(perform_check);
}

fn perform_check() {
    let service = active_service();
    let service_name = groovestats_api::service_name(service);
    debug!("Performing {service_name} connectivity check...");

    match network::get_json_with::<groovestats_api::NewSessionResponse>(
        &network::get_groovestats_agent(),
        &groovestats_api::new_session_url(service),
    ) {
        Ok(data) => {
            let status = groovestats_api::connection_status_from_new_session(&data);
            match &status {
                ConnectionStatus::Connected(services) => info!(
                    "Connected to {service_name} (scores={}, leaderboards={}, autosubmit={}).",
                    services.get_scores, services.leaderboard, services.auto_submit
                ),
                ConnectionStatus::Error(ConnectionError::MachineOffline) => {
                    warn!("{service_name} servicesResult != OK.")
                }
                _ => {}
            }
            set_status(status);
        }
        Err(error) => {
            let connection_error = groovestats_api::connection_error_from_network_error(&error);
            match &error {
                network::NetworkError::Timeout => {
                    warn!("{service_name} connectivity check timed out.")
                }
                network::NetworkError::Decode(error) => {
                    warn!("Failed to parse {service_name} response: {error}")
                }
                _ => warn!("HTTP error to {service_name}: {error}"),
            }
            set_status(ConnectionStatus::Error(connection_error));
        }
    }
}
