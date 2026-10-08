//! Background Bluetooth Heart Rate Service integration.
//!
//! The worker thread owns every Bluetooth object and all connection work. The
//! game thread only updates desired device IDs and reads bounded snapshots.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerReading {
    pub configured: bool,
    pub connected: bool,
    pub bpm: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverySnapshot {
    pub supported: bool,
    pub scanning: bool,
    pub devices: Vec<Device>,
    pub error: Option<String>,
}

// Keep the backend selection in sync with the target dependencies in Cargo.toml.
#[cfg_attr(
    not(any(
        all(target_os = "windows", not(target_vendor = "win7")),
        target_os = "linux",
        target_os = "macos"
    )),
    path = "unsupported.rs"
)]
mod platform;

pub use platform::{
    configure, discovery_generation, discovery_snapshot, player_readings,
    player_readings_generation,
};
