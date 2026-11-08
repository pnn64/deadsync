use crate::protocol::{ADC_MAX, SENSORS};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightMode {
    Off,
    #[default]
    Input,
    Chart,
    Judgment,
}

impl LightMode {
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Input,
            Self::Input => Self::Chart,
            Self::Chart => Self::Judgment,
            Self::Judgment => Self::Off,
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Input => "Input",
            Self::Chart => "Chart",
            Self::Judgment => "Judgment",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeviceConfig {
    /// Zero-based player slot, independent of USB enumeration order.
    pub player: usize,
    pub enabled: bool,
    pub baseline: Option<[u16; SENSORS]>,
    pub press: [u16; SENSORS],
    pub release: [u16; SENSORS],
    pub sensors_enabled: [bool; SENSORS],
    pub brightness: u8,
    pub lights: LightMode,
}
impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            player: 0,
            enabled: true,
            baseline: None,
            press: [30; SENSORS],
            release: [25; SENSORS],
            sensors_enabled: [true; SENSORS],
            brightness: 40,
            lights: LightMode::Input,
        }
    }
}
impl DeviceConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.player > 1 || self.brightness > 100 {
            return Err("invalid RE:Flex player or brightness".into());
        }
        if self
            .baseline
            .is_some_and(|v| v.iter().any(|&v| v > ADC_MAX))
        {
            return Err("invalid RE:Flex baseline".into());
        }
        for i in 0..SENSORS {
            if self.press[i] > ADC_MAX || self.release[i] >= self.press[i] {
                return Err(format!(
                    "RE:Flex sensor {} requires 0 <= release < press <= 4095",
                    i + 1
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub mapping_initialized: [bool; 2],
    pub ambiguous_serials: Vec<String>,
    pub devices: BTreeMap<String, DeviceConfig>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            mapping_initialized: [false; 2],
            ambiguous_serials: Vec::new(),
            devices: BTreeMap::new(),
        }
    }
}
impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
        };
        let config: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        for (serial, device) in &config.devices {
            if serial.trim().is_empty() {
                return Err("empty RE:Flex serial".into());
            }
            device.validate()?;
        }
        Ok(config)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        deadlib_platform::atomic_write::write_atomic(path, &bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_and_invalid_settings_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save/reflex.json");
        let mut c = Config::load(&path).unwrap();
        c.devices.insert(
            "CHIP123".into(),
            DeviceConfig {
                player: 1,
                baseline: Some([1000; 16]),
                ..Default::default()
            },
        );
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().devices, c.devices);
        std::fs::write(&path, "broken").unwrap();
        assert!(Config::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
        let mut d = DeviceConfig::default();
        d.release[1] = d.press[1];
        assert!(d.validate().is_err());
    }
}
