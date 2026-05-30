use deadsync_net as network;
use deadsync_online::arrowcloud::{
    self as arrowcloud_api, ConnectionError, ConnectionStatus, classify_connection_error,
};
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

pub fn init() {
    refresh_status();
}

/// Re-runs the connectivity probe.  Safe to call repeatedly (e.g. after
/// device-login writes a new api key to the active profile).
pub fn refresh_status() {
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

    match network::get_agent()
        .get(arrowcloud_api::api_base_url())
        .call()
    {
        Ok(_) => {
            info!("Connected to ArrowCloud.");
            set_status(ConnectionStatus::Connected);
        }
        Err(error) => {
            warn!("HTTP error to ArrowCloud: {error}");
            let message = error.to_string();
            set_status(ConnectionStatus::Error(classify_connection_error(&message)));
        }
    }
}
