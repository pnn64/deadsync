use std::time::Instant;

use winit::keyboard::KeyCode;

use crate::{PadDir, PadEvent, PadId};

#[cfg(target_os = "freebsd")]
pub mod devd;
#[cfg(unix)]
pub mod unix_time;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadBackend {
    #[cfg(windows)]
    WindowsRawInput,
    #[cfg(windows)]
    WindowsWgi,
    #[cfg(target_os = "linux")]
    LinuxEvdev,
    #[cfg(target_os = "freebsd")]
    FreeBsdHidraw,
    #[cfg(target_os = "freebsd")]
    FreeBsdEvdev,
    #[cfg(target_os = "macos")]
    MacOsIohid,
    /// StepManiaX pad via the RustManiaX SDK (all platforms).
    Smx,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsPadBackend {
    /// Choose the default Windows backend (currently Raw Input).
    Auto,
    #[default]
    RawInput,
    Wgi,
}

impl WindowsPadBackend {
    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::RawInput => "RawInput",
            Self::Wgi => "WGI",
        }
    }
}

impl std::fmt::Display for WindowsPadBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for WindowsPadBackend {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            return Ok(Self::Auto);
        }
        if s.eq_ignore_ascii_case("rawinput")
            || s.eq_ignore_ascii_case("raw_input")
            || s.eq_ignore_ascii_case("raw")
        {
            return Ok(Self::RawInput);
        }
        if s.eq_ignore_ascii_case("wgi")
            || s.eq_ignore_ascii_case("windowsgaminginput")
            || s.eq_ignore_ascii_case("gaminginput")
        {
            return Ok(Self::Wgi);
        }
        Err(())
    }
}

/// Input backends that persist a stable pad order. SMX is intentionally
/// excluded because it has its own serial-based assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PadOrderBackend {
    RawInput,
    Wgi,
    IoHid,
    Hidraw,
    LinuxEvdev,
    FreeBsdEvdev,
}

pub const PAD_ORDER_BACKENDS: [PadOrderBackend; 6] = [
    PadOrderBackend::RawInput,
    PadOrderBackend::Wgi,
    PadOrderBackend::IoHid,
    PadOrderBackend::Hidraw,
    PadOrderBackend::LinuxEvdev,
    PadOrderBackend::FreeBsdEvdev,
];

#[inline(always)]
pub fn uuid_from_bytes(bytes: &[u8]) -> [u8; 16] {
    // Deterministic, fast, and tiny: two FNV-1a 64-bit passes with different offsets.
    const OFF0: u64 = 0xcbf29ce484222325;
    const OFF1: u64 = 0xaf63dc4c8601ec8c;
    const PRIME: u64 = 0x00000100000001b3;

    #[inline(always)]
    fn fnv64(mut h: u64, bytes: &[u8]) -> u64 {
        let mut i = 0;
        while i < bytes.len() {
            h ^= u64::from(bytes[i]);
            h = h.wrapping_mul(PRIME);
            i += 1;
        }
        h
    }

    let a = fnv64(OFF0, bytes);
    let b = fnv64(OFF1, bytes);
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..].copy_from_slice(&b.to_le_bytes());
    out
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub struct RawKeyboardEvent {
    pub code: KeyCode,
    pub pressed: bool,
    pub repeat: bool,
    pub timestamp: Instant,
    pub host_nanos: u64,
}

#[derive(Clone, Debug)]
pub enum GpSystemEvent {
    Connected {
        name: String,
        id: PadId,
        vendor_id: Option<u16>,
        product_id: Option<u16>,
        backend: PadBackend,
        /// True when this connection is part of startup enumeration.
        initial: bool,
    },
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Disconnected {
        name: String,
        id: PadId,
        backend: PadBackend,
        /// True when this disconnect is part of startup enumeration.
        initial: bool,
    },
    StartupComplete,
}

const DIRS: [PadDir; 4] = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];

#[inline(always)]
pub fn emit_dir_edges(
    emit_pad: &mut impl FnMut(PadEvent),
    id: PadId,
    dir_state: &mut [bool; 4],
    timestamp: Instant,
    host_nanos: u64,
    want: [bool; 4],
) {
    for i in 0..DIRS.len() {
        if dir_state[i] == want[i] {
            continue;
        }
        dir_state[i] = want[i];
        emit_pad(PadEvent::Dir {
            id,
            timestamp,
            host_nanos,
            dir: DIRS[i],
            pressed: want[i],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_dir_edges_updates_only_changed_dirs() {
        let mut events = Vec::new();
        let timestamp = Instant::now();
        let mut dir_state = [false; 4];

        emit_dir_edges(
            &mut |event| events.push(event),
            PadId(7),
            &mut dir_state,
            timestamp,
            42,
            [true, false, true, false],
        );
        assert_eq!(dir_state, [true, false, true, false]);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            PadEvent::Dir {
                id: PadId(7),
                timestamp: ts,
                host_nanos: 42,
                dir: PadDir::Up,
                pressed: true,
            } if ts == timestamp
        ));
        assert!(matches!(
            events[1],
            PadEvent::Dir {
                id: PadId(7),
                timestamp: ts,
                host_nanos: 42,
                dir: PadDir::Left,
                pressed: true,
            } if ts == timestamp
        ));

        events.clear();
        emit_dir_edges(
            &mut |event| events.push(event),
            PadId(7),
            &mut dir_state,
            timestamp,
            42,
            [true, false, true, false],
        );
        assert!(events.is_empty());
    }
}
