use deadsync_online::groovestats::{
    self as groovestats_api, ConnectionError, ConnectionProbeError, ConnectionStatus,
};
use log::{debug, info, warn};
use std::sync::{LazyLock, Mutex};
use std::thread;

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
    thread::spawn(perform_check);
}

fn perform_check() {
    let service = active_service();
    let service_name = groovestats_api::service_name(service);
    debug!("Performing {service_name} connectivity check...");

    match groovestats_api::probe_connection(service) {
        Ok(status) => {
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
            let connection_error = error.connection_error();
            match error {
                ConnectionProbeError::Timeout => {
                    warn!("{service_name} connectivity check timed out.")
                }
                ConnectionProbeError::InvalidResponse(error) => {
                    warn!("Failed to parse {service_name} response: {error}")
                }
                ConnectionProbeError::CannotConnect(error) => {
                    warn!("HTTP error to {service_name}: {error}")
                }
            }
            set_status(ConnectionStatus::Error(connection_error));
        }
    }
}
