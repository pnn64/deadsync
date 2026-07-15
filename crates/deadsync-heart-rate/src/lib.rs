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

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
mod platform;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub use platform::{configure, discovery_snapshot, player_readings};

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn configure(_enabled: bool, _discover: bool, _device_ids: [Option<&str>; 2]) {}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn player_readings() -> [PlayerReading; 2] {
    [PlayerReading::default(); 2]
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn discovery_snapshot() -> DiscoverySnapshot {
    DiscoverySnapshot {
        supported: false,
        scanning: false,
        devices: Vec::new(),
        error: Some("Bluetooth heart-rate monitors are unsupported on this platform".to_owned()),
    }
}
