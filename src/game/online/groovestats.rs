use deadsync_online::groovestats::{self as groovestats_api, ConnectionProbeLog, ConnectionStatus};
use log::{debug, info, warn};

pub fn get_status() -> ConnectionStatus {
    groovestats_api::runtime_get_status()
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
    let service = active_service();
    let service_name = groovestats_api::service_name(service);
    if cfg.enable_groovestats {
        debug!("Initializing {service_name} network check...");
    }
    groovestats_api::runtime_init(cfg.enable_groovestats, service, log_probe_transition);
}

fn log_probe_transition(log: Option<ConnectionProbeLog>) {
    match log {
        Some(ConnectionProbeLog::Connected { service, services }) => {
            let service_name = groovestats_api::service_name(service);
            info!(
                "Connected to {service_name} (scores={}, leaderboards={}, autosubmit={}).",
                services.get_scores, services.leaderboard, services.auto_submit
            );
        }
        Some(ConnectionProbeLog::MachineOffline { service }) => {
            let service_name = groovestats_api::service_name(service);
            warn!("{service_name} servicesResult != OK.");
        }
        Some(ConnectionProbeLog::Timeout { service }) => {
            let service_name = groovestats_api::service_name(service);
            warn!("{service_name} connectivity check timed out.");
        }
        Some(ConnectionProbeLog::InvalidResponse { service, error }) => {
            let service_name = groovestats_api::service_name(service);
            warn!("Failed to parse {service_name} response: {error}");
        }
        Some(ConnectionProbeLog::CannotConnect { service, error }) => {
            let service_name = groovestats_api::service_name(service);
            warn!("HTTP error to {service_name}: {error}");
        }
        None => {}
    }
}
