//! Controller providers used by DeadSync.

use deadlib_platform::input::{GpSystemEvent, PadBackend};
use std::fmt;
use std::str::FromStr;

pub type GamepadSystemEvent = GpSystemEvent<GamepadBackend>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GamepadBackend {
    Native(PadBackend),
    Smx,
    Reflex,
}

impl GamepadBackend {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            #[cfg(windows)]
            Self::Native(PadBackend::WindowsRawInput) => "Windows Raw Input",
            #[cfg(windows)]
            Self::Native(PadBackend::WindowsWgi) => "Windows Gaming Input",
            #[cfg(target_os = "linux")]
            Self::Native(PadBackend::LinuxEvdev) => "Linux evdev",
            #[cfg(target_os = "freebsd")]
            Self::Native(PadBackend::FreeBsdHidraw) => "FreeBSD hidraw",
            #[cfg(target_os = "freebsd")]
            Self::Native(PadBackend::FreeBsdEvdev) => "FreeBSD evdev",
            #[cfg(target_os = "macos")]
            Self::Native(PadBackend::MacOsIohid) => "macOS IOHID",
            #[cfg(not(any(windows, target_os = "linux", target_os = "freebsd", target_os = "macos")))]
            Self::Native(backend) => match backend {},
            Self::Smx => "StepManiaX",
            Self::Reflex => "RE:Flex",
        }
    }
}

impl fmt::Debug for GamepadBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Preserve the provider names used by connection notices and logs.
        match self {
            Self::Native(backend) => backend.fmt(f),
            Self::Smx => f.write_str("Smx"),
            Self::Reflex => f.write_str("Reflex"),
        }
    }
}

/// Saved StepManiaX pad sensitivity preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmxPadPreset {
    Low,
    Medium,
    High,
}

impl SmxPadPreset {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    /// 0/1/2 index used by the options choice list.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }

    #[must_use]
    pub const fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Medium,
            2 => Self::High,
            _ => Self::Low,
        }
    }
}

impl FromStr for SmxPadPreset {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err(()),
        }
    }
}
