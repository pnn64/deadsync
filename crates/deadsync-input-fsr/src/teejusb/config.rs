use super::protocol::{MAX_SENSORS, MAX_THRESHOLD, Protocol};
use serde::{Deserialize, Serialize};
use std::path::Path;

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub auto_detect: bool,
    /// Once a USB serial is known to be duplicated, never restore a shared
    /// serial-keyed profile even when only one of those boards is plugged in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ambiguous_serials: Vec<String>,
    pub devices: Vec<DeviceConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_detect: true,
            ambiguous_serials: Vec::new(),
            devices: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeviceConfig {
    /// Generated from USB serial identity, or the port if no unique serial exists.
    pub id: Option<String>,
    /// Explicit ports also support custom USB IDs and serial adapters.
    pub port: Option<String>,
    pub name: Option<String>,
    pub protocol: Protocol,
    /// Zero-based firmware indices. Omitted sensors remain individually editable.
    pub groups: Vec<Vec<usize>>,
    /// Only confirmed threshold writes are recorded. Empty means read the board.
    pub thresholds: Vec<u16>,
    #[serde(default = "yes")]
    pub save_to_board: bool,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            id: None,
            port: None,
            name: None,
            protocol: Protocol::Auto,
            groups: Vec::new(),
            thresholds: Vec::new(),
            save_to_board: true,
        }
    }
}

impl DeviceConfig {
    pub fn discovered(id: String, port: String) -> Self {
        Self {
            id: Some(id),
            port: Some(port),
            save_to_board: true,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.as_ref().is_some_and(|s| s.is_empty())
            || self.port.as_ref().is_some_and(|s| s.is_empty())
            || (self.id.is_none() && self.port.is_none())
        {
            return Err("each teejusb device needs a nonempty id or port".into());
        }
        if self.thresholds.len() > MAX_SENSORS || self.thresholds.iter().any(|&v| v > MAX_THRESHOLD)
        {
            return Err("invalid saved teejusb thresholds".into());
        }
        let mut seen = [false; MAX_SENSORS];
        for group in &self.groups {
            if group.is_empty() {
                return Err("teejusb groups cannot be empty".into());
            }
            for &index in group {
                if index >= MAX_SENSORS || std::mem::replace(&mut seen[index], true) {
                    return Err("teejusb groups contain an invalid or repeated sensor".into());
                }
            }
        }
        Ok(())
    }

    pub fn groups_for(&self, count: usize) -> Result<Vec<Vec<usize>>, String> {
        let mut seen = vec![false; count];
        for group in &self.groups {
            for &index in group {
                let Some(slot) = seen.get_mut(index) else {
                    return Err(
                        "configured group references a sensor absent from this firmware".into(),
                    );
                };
                if std::mem::replace(slot, true) {
                    return Err("sensor appears in multiple groups".into());
                }
            }
        }
        let mut groups = self.groups.clone();
        groups.extend(
            seen.iter()
                .enumerate()
                .filter_map(|(i, &used)| (!used).then_some(vec![i])),
        );
        Ok(groups)
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid {}: {e}", path.display()))?;
        for (i, device) in config.devices.iter().enumerate() {
            device.validate()?;
            if config.devices[..i].iter().any(|other| {
                (device.id.is_some() && device.id == other.id)
                    || (device.port.is_some()
                        && device.port == other.port
                        && (device.id.is_none() || other.id.is_none()))
            }) {
                return Err("duplicate teejusb device id or port".into());
            }
        }
        Ok(config)
    }

    pub fn save(&self, path: Option<&Path>) -> Result<(), String> {
        let Some(path) = path else {
            return Ok(());
        };
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        deadlib_platform::atomic_write::write_atomic(path, &bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_preserve_firmware_indices_and_include_omitted_sensors() {
        let cfg = DeviceConfig {
            groups: vec![vec![4, 0], vec![2, 3]],
            ..DeviceConfig::discovered("id".into(), "port".into())
        };
        assert_eq!(
            cfg.groups_for(6).unwrap(),
            [vec![4, 0], vec![2, 3], vec![1], vec![5]]
        );
        assert!(cfg.groups_for(4).is_err());
        let invalid = DeviceConfig {
            groups: vec![vec![0], vec![0]],
            ..cfg
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn persisted_settings_round_trip_without_losing_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save/teejusb.json");
        let mut cfg = Config::load(Some(&path)).unwrap();
        let mut device = DeviceConfig::discovered("usb:serial".into(), "COM10".into());
        device.protocol = Protocol::Legacy;
        device.groups = vec![vec![2, 0]];
        device.thresholds = vec![100, 200, 300];
        cfg.devices.push(device);
        cfg.save(Some(&path)).unwrap();
        let loaded = Config::load(Some(&path)).unwrap();
        assert_eq!(loaded.devices[0].thresholds, [100, 200, 300]);
        assert_eq!(loaded.devices[0].groups, [vec![2, 0]]);
        assert_eq!(loaded.devices[0].protocol, Protocol::Legacy);
        assert!(loaded.devices[0].save_to_board);
        std::fs::write(&path, b"{broken").unwrap();
        assert!(Config::load(Some(&path)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
    }
}
