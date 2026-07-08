use deadsync_online::arrowcloud::{self as arrowcloud_api, ConnectionProbeLog, ConnectionStatus};
use log::{debug, info, warn};

pub fn get_status() -> ConnectionStatus {
    arrowcloud_api::runtime_get_status()
}

pub fn init() {
    refresh_status();
}

/// Re-runs the connectivity probe. Safe to call repeatedly (e.g. after
/// device-login writes a new api key to the active profile).
pub fn refresh_status() {
    let cfg = crate::config::get();
    if cfg.enable_arrowcloud {
        debug!("Initializing ArrowCloud network check...");
    }
    arrowcloud_api::runtime_init(cfg.enable_arrowcloud, log_probe_transition);
}

fn log_probe_transition(log: Option<ConnectionProbeLog>) {
    match log {
        Some(ConnectionProbeLog::Connected) => info!("Connected to ArrowCloud."),
        Some(ConnectionProbeLog::CannotConnect { error }) => {
            warn!("HTTP error to ArrowCloud: {error}");
        }
        None => {}
    }
}
