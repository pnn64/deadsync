//! Physical input events and device identifiers shared with native backends.
//!
//! Events retain the backend's capture timestamps and element codes. Consumers
//! interpret bindings, players, and song time above this transport contract.

use std::time::Instant;
#[doc(inline)]
pub use winit::keyboard::KeyCode;

/// Number of supported native pad slots, including the shared overflow slot.
/// Slots `0..64` identify devices without growing hot-path input state.
pub const PAD_ID_COUNT_CAP: usize = 65;

/// Physical key transition with its original capture timestamps.
#[derive(Clone, Copy, Debug)]
pub struct RawKeyboardEvent {
    pub code: KeyCode,
    pub pressed: bool,
    pub repeat: bool,
    pub timestamp: Instant,
    pub host_nanos: u64,
}

/// Runtime device slot assigned by a native input backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PadId(pub u32);

impl From<PadId> for usize {
    #[inline(always)]
    fn from(value: PadId) -> Self {
        value.0 as Self
    }
}

/// Platform-specific button or axis element code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PadCode(pub u32);

impl PadCode {
    #[inline(always)]
    #[must_use]
    pub const fn into_u32(self) -> u32 {
        self.0
    }
}

/// Physical directional input from a controller hat or directional control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadDir {
    Up,
    Down,
    Left,
    Right,
}

impl PadDir {
    #[inline(always)]
    #[must_use]
    pub const fn ix(self) -> usize {
        match self {
            Self::Up => 0,
            Self::Down => 1,
            Self::Left => 2,
            Self::Right => 3,
        }
    }
}

/// Physical controller transition carrying the backend's device identity and timestamps.
#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    Dir {
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
        dir: PadDir,
        pressed: bool,
    },
    /// Raw low-level button event with platform-specific code and device UUID.
    RawButton {
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
        pressed: bool,
    },
    /// Raw low-level axis event with platform-specific code and device UUID.
    RawAxis {
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_dir_indices_are_stable() {
        assert_eq!(PadDir::Up.ix(), 0);
        assert_eq!(PadDir::Down.ix(), 1);
        assert_eq!(PadDir::Left.ix(), 2);
        assert_eq!(PadDir::Right.ix(), 3);
    }

    #[test]
    fn pad_physical_ids_are_plain_numeric_wrappers() {
        assert_eq!(usize::from(PadId(7)), 7);
        assert_eq!(PadCode(0xDEAD_BEEF).into_u32(), 0xDEAD_BEEF);
    }
}
