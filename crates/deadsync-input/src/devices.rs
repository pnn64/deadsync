//! Controller providers used by DeadSync.

use deadlib_platform::input::{GpSystemEvent, PadBackend};
pub use deadsync_core::input::SmxPadPreset;
use std::fmt;

pub type GamepadSystemEvent = GpSystemEvent<GamepadBackend>;

/// Reserved RE:Flex panel codes. Saved bindings use these values.
pub const REFLEX_BUTTON_CODE_BASE: u32 = 0x5246_0000;

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
