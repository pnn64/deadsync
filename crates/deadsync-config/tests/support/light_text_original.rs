//! Frozen production code from c528f2695.
use deadsync_lights::{DEFAULT_LITBOARD_PORT, DriverKind, GameplayPadLightMode};
const SERIAL_PORT_NAME_CAP: usize = 64;

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

pub fn driver(s: &str) -> Result<DriverKind, ()> {
    let mut key = String::with_capacity(s.len());
    for ch in s.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    match key.as_str() {
        "" | "0" | "false" | "off" | "none" | "disabled" => Ok(DriverKind::Off),
        "snek" | "snekboard" => Ok(DriverKind::Snek),
        "lit" | "litboard" | "sextetstream" => Ok(DriverKind::Litboard),
        "win32serial" | "lightsdriverwin32serial" | "sextetserial" => Ok(DriverKind::Win32Serial),
        "fusion" | "icedragonfusion" | "lightsdriverfusion" => Ok(DriverKind::Fusion),
        "gpb" | "icedragongpb" | "lightsdrivergpb" => Ok(DriverKind::Gpb),
        "pac" | "pacdrive" | "ultimarcpacdrive" => Ok(DriverKind::PacDrive),
        "piuio" | "piuioleds" | "linuxpiuioleds" | "lightsdriverpiuioleds" => {
            Ok(DriverKind::PiuioLeds)
        }
        "itgio" | "linuxitgio" | "lightsdriveritgio" => Ok(DriverKind::Itgio),
        "hidbluedot" | "bluedot" => Ok(DriverKind::HidBlueDot),
        "stac2" | "stacv2" | "stac2hid" | "icedragonstac2" => Ok(DriverKind::Stac2),
        "minimaid" | "minimaidhid" | "linuxminimaid" | "win32minimaid" => {
            Ok(DriverKind::MinimaidHid)
        }
        _ => Err(()),
    }
}

pub fn mode(s: &str) -> Result<GameplayPadLightMode, ()> {
    let mut key = String::with_capacity(s.len());
    for ch in s.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    match key.as_str() {
        "" | "input" | "buttons" | "press" | "pressed" => Ok(GameplayPadLightMode::Input),
        "chart" | "notes" | "notedata" | "arrows" => Ok(GameplayPadLightMode::Chart),
        _ => Err(()),
    }
}
