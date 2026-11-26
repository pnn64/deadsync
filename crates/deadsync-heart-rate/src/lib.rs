//! Background Bluetooth Heart Rate Service integration.
//!
//! The BLE worker supplies device events. This crate interprets heart-rate
//! notifications and publishes readings for the selected players.

pub use deadlib_ble::{Device, DiscoverySnapshot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerReading {
    pub configured: bool,
    pub connected: bool,
    pub bpm: Option<u16>,
}

mod runtime;

pub use runtime::{
    configure, discovery_generation, discovery_snapshot, player_readings,
    player_readings_generation,
};
