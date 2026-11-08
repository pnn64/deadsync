use crate::{DiscoverySnapshot, PlayerReading};

pub fn configure(_enabled: bool, _discover: bool, _device_ids: [Option<&str>; 2]) {}

pub fn player_readings() -> [PlayerReading; 2] {
    [PlayerReading::default(); 2]
}

pub const fn player_readings_generation() -> u64 {
    0
}

pub const fn discovery_generation() -> u64 {
    0
}

pub fn discovery_snapshot() -> DiscoverySnapshot {
    DiscoverySnapshot {
        supported: false,
        scanning: false,
        devices: Vec::new(),
        error: Some("Bluetooth heart-rate monitors are unsupported on this platform".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabling_discovery_with_saved_devices_remains_unsupported() {
        configure(true, true, [Some("polar-h10"), Some("garmin-hrm")]);

        let snapshot = discovery_snapshot();
        assert!(!snapshot.supported);
        assert!(!snapshot.scanning);
        assert!(snapshot.devices.is_empty());
        assert!(snapshot.error.is_some());
        assert_eq!(player_readings(), [PlayerReading::default(); 2]);
        assert_eq!(player_readings_generation(), 0);
        assert_eq!(discovery_generation(), 0);

        configure(false, false, [None, None]);
    }
}
