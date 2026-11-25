//! Cabinet lighting preferences shared by settings and the driver runtime.

use std::str::FromStr;

const SERIAL_PORT_NAME_CAP: usize = 64;

#[cfg(windows)]
pub const DEFAULT_WIN32_SERIAL_PORT: &str = "COM54";
#[cfg(not(windows))]
pub const DEFAULT_WIN32_SERIAL_PORT: &str = "/dev/ttyUSB0";

pub const DEFAULT_LITBOARD_PORT: &str = DEFAULT_WIN32_SERIAL_PORT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialPortName {
    bytes: [u8; SERIAL_PORT_NAME_CAP],
    len: u8,
}

impl Default for SerialPortName {
    fn default() -> Self {
        Self::parse(DEFAULT_LITBOARD_PORT, Self::empty())
    }
}

impl SerialPortName {
    const fn empty() -> Self {
        Self {
            bytes: [0; SERIAL_PORT_NAME_CAP],
            len: 0,
        }
    }

    #[must_use]
    pub fn parse(raw: &str, default: Self) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.len() > SERIAL_PORT_NAME_CAP {
            return default;
        }
        if trimmed.len() >= 16 {
            let bytes = trimmed.as_bytes();
            if !bytes.iter().all(|byte| (b' '..=b'~').contains(byte)) {
                return default;
            }
            let mut out = Self::empty();
            out.bytes[..bytes.len()].copy_from_slice(bytes);
            out.len = bytes.len() as u8;
            return out;
        }
        let mut out = Self::empty();
        for (i, b) in trimmed.bytes().enumerate() {
            if !b.is_ascii() || b.is_ascii_control() {
                return default;
            }
            out.bytes[i] = b;
            out.len += 1;
        }
        out
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or(DEFAULT_LITBOARD_PORT)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DriverKind {
    #[default]
    Off,
    Snek,
    Litboard,
    Win32Serial,
    Fusion,
    Gpb,
    PacDrive,
    PiuioLeds,
    Itgio,
    HidBlueDot,
    Stac2,
    MinimaidHid,
}

impl DriverKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "None",
            Self::Snek => "Snek",
            Self::Litboard => "Litboard",
            Self::Win32Serial => "Win32Serial",
            Self::Fusion => "Fusion",
            Self::Gpb => "GPB",
            Self::PacDrive => "PacDrive",
            Self::PiuioLeds => "PIUIO_Leds",
            Self::Itgio => "ITGIO",
            Self::HidBlueDot => "HidBlueDot",
            Self::Stac2 => "STAC2",
            Self::MinimaidHid => "MinimaidHID",
        }
    }
}

impl std::fmt::Display for DriverKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DriverKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = [0; b"lightsdriverwin32serial".len()];
        let mut len = 0;
        for byte in s.bytes() {
            if byte.is_ascii_alphanumeric() {
                if len == key.len() {
                    return Err(());
                }
                key[len] = byte.to_ascii_lowercase();
                len += 1;
            }
        }
        match std::str::from_utf8(&key[..len]).expect("ASCII light driver name") {
            "" | "0" | "false" | "off" | "none" | "disabled" => Ok(Self::Off),
            "snek" | "snekboard" => Ok(Self::Snek),
            "lit" | "litboard" | "sextetstream" => Ok(Self::Litboard),
            "win32serial" | "lightsdriverwin32serial" | "sextetserial" => Ok(Self::Win32Serial),
            "fusion" | "icedragonfusion" | "lightsdriverfusion" => Ok(Self::Fusion),
            "gpb" | "icedragongpb" | "lightsdrivergpb" => Ok(Self::Gpb),
            "pac" | "pacdrive" | "ultimarcpacdrive" => Ok(Self::PacDrive),
            "piuio" | "piuioleds" | "linuxpiuioleds" | "lightsdriverpiuioleds" => {
                Ok(Self::PiuioLeds)
            }
            "itgio" | "linuxitgio" | "lightsdriveritgio" => Ok(Self::Itgio),
            "hidbluedot" | "bluedot" => Ok(Self::HidBlueDot),
            "stac2" | "stacv2" | "stac2hid" | "icedragonstac2" => Ok(Self::Stac2),
            "minimaid" | "minimaidhid" | "linuxminimaid" | "win32minimaid" => Ok(Self::MinimaidHid),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PacDriveLightOrdering {
    #[default]
    OpenItg,
    Sm5,
}

impl PacDriveLightOrdering {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenItg => "openitg",
            Self::Sm5 => "sm5",
        }
    }

    /// Match `ITGmania`'s `PacDriveLightOrdering` preference behavior.
    /// `openitg` and the historical `lumenar` name select the `OpenITG` layout;
    /// every other explicit value selects the legacy SM5 layout.
    #[must_use]
    pub const fn from_preference(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("openitg") || raw.eq_ignore_ascii_case("lumenar") {
            Self::OpenItg
        } else {
            Self::Sm5
        }
    }
}

impl std::fmt::Display for PacDriveLightOrdering {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayPadLightMode {
    #[default]
    Input,
    Chart,
}

impl GameplayPadLightMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Chart => "Chart",
        }
    }
}

impl std::fmt::Display for GameplayPadLightMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GameplayPadLightMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = [0; b"notedata".len()];
        let mut len = 0;
        for byte in s.bytes() {
            if byte.is_ascii_alphanumeric() {
                if len == key.len() {
                    return Err(());
                }
                key[len] = byte.to_ascii_lowercase();
                len += 1;
            }
        }
        match std::str::from_utf8(&key[..len]).expect("ASCII gameplay pad light mode") {
            "" | "input" | "buttons" | "press" | "pressed" => Ok(Self::Input),
            "chart" | "notes" | "notedata" | "arrows" => Ok(Self::Chart),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_board_driver_names() {
        assert_eq!(DriverKind::default().as_str(), "None");
        assert_eq!(DriverKind::from_str("None").unwrap(), DriverKind::Off);
        assert_eq!(DriverKind::from_str("Off").unwrap(), DriverKind::Off);
        assert_eq!(DriverKind::from_str("Snekboard").unwrap(), DriverKind::Snek);
        assert_eq!(
            DriverKind::from_str("Litboard").unwrap(),
            DriverKind::Litboard
        );
        assert_eq!(
            DriverKind::from_str("Win32Serial").unwrap(),
            DriverKind::Win32Serial
        );
        assert_eq!(
            DriverKind::from_str("LightsDriver_Win32Serial").unwrap(),
            DriverKind::Win32Serial
        );
        assert_eq!(
            DriverKind::from_str("LightsDriver_fusion").unwrap(),
            DriverKind::Fusion
        );
        assert_eq!(DriverKind::from_str("GPB").unwrap(), DriverKind::Gpb);
        assert_eq!(
            DriverKind::from_str("PacDrive").unwrap(),
            DriverKind::PacDrive
        );
        assert_eq!(
            DriverKind::from_str("PIUIO_Leds").unwrap(),
            DriverKind::PiuioLeds
        );
        assert_eq!(DriverKind::from_str("ITGIO").unwrap(), DriverKind::Itgio);
        assert_eq!(
            DriverKind::from_str("HidBlueDot").unwrap(),
            DriverKind::HidBlueDot
        );
        assert_eq!(DriverKind::from_str("stac2").unwrap(), DriverKind::Stac2);
        assert_eq!(
            DriverKind::from_str("LinuxMinimaid").unwrap(),
            DriverKind::MinimaidHid
        );
    }

    #[test]
    fn parses_gameplay_pad_light_modes() {
        assert_eq!(GameplayPadLightMode::default().as_str(), "Input");
        assert_eq!(
            GameplayPadLightMode::from_str("Input").unwrap(),
            GameplayPadLightMode::Input
        );
        assert_eq!(
            GameplayPadLightMode::from_str("Chart").unwrap(),
            GameplayPadLightMode::Chart
        );
    }

    #[test]
    fn pacdrive_ordering_matches_itgmania_preference_semantics() {
        assert_eq!(
            PacDriveLightOrdering::default(),
            PacDriveLightOrdering::OpenItg
        );
        assert_eq!(
            PacDriveLightOrdering::from_preference("OpEnItG"),
            PacDriveLightOrdering::OpenItg
        );
        assert_eq!(
            PacDriveLightOrdering::from_preference("lumenar"),
            PacDriveLightOrdering::OpenItg
        );
        assert_eq!(
            PacDriveLightOrdering::from_preference(""),
            PacDriveLightOrdering::Sm5
        );
        assert_eq!(
            PacDriveLightOrdering::from_preference("sm5"),
            PacDriveLightOrdering::Sm5
        );
    }

    #[test]
    fn parses_serial_port_names_with_default_fallback() {
        let default = SerialPortName::default();
        assert_eq!(default.as_str(), DEFAULT_WIN32_SERIAL_PORT);
        assert_eq!(SerialPortName::parse(" COM7 ", default).as_str(), "COM7");
        assert_eq!(SerialPortName::parse("", default), default);
        assert_eq!(SerialPortName::parse("COM\u{1b}", default), default);
    }
}
