use std::time::Instant;

pub mod debounce;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Lane {
    Left = 0,
    Down = 1,
    Up = 2,
    Right = 3,
    P2Left = 4,
    P2Down = 5,
    P2Up = 6,
    P2Right = 7,
}

impl Lane {
    #[inline(always)]
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Keyboard,
    Gamepad,
}

pub const INPUT_SLOT_INVALID: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub struct InputEdge {
    pub lane: Lane,
    pub input_slot: u32,
    pub pressed: bool,
    pub source: InputSource,
    pub record_replay: bool,
    // Real-time timestamps for latency tracing. Filled in by gameplay when the
    // edge is accepted for lane processing.
    pub captured_at: Instant,
    pub captured_host_nanos: u64,
    pub stored_at: Instant,
    pub emitted_at: Instant,
    pub queued_at: Instant,
    // Integer song time for this edge, in nanoseconds. Live input may leave this
    // invalid until gameplay resolves the physical timestamp against the frame's
    // song-clock snapshot.
    pub event_music_time_ns: i64,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VirtualAction {
    p1_up,
    p1_down,
    p1_left,
    p1_right,
    p1_start,
    p1_back,
    p1_menu_up,
    p1_menu_down,
    p1_menu_left,
    p1_menu_right,
    p1_select,
    p1_operator,
    p1_restart,
    p2_up,
    p2_down,
    p2_left,
    p2_right,
    p2_start,
    p2_back,
    p2_menu_up,
    p2_menu_down,
    p2_menu_left,
    p2_menu_right,
    p2_select,
    p2_operator,
    p2_restart,
}

impl VirtualAction {
    pub const COUNT: usize = Self::p2_restart as usize + 1;

    #[inline(always)]
    pub const fn ix(self) -> usize {
        self as usize
    }

    #[inline(always)]
    pub const fn is_gameplay_arrow(self) -> bool {
        matches!(
            self,
            Self::p1_up
                | Self::p1_down
                | Self::p1_left
                | Self::p1_right
                | Self::p2_up
                | Self::p2_down
                | Self::p2_left
                | Self::p2_right
        )
    }

    #[inline(always)]
    pub const fn secondary_menu(self) -> Option<Self> {
        match self {
            Self::p1_up => Some(Self::p1_menu_up),
            Self::p1_down => Some(Self::p1_menu_down),
            Self::p1_left => Some(Self::p1_menu_left),
            Self::p1_right => Some(Self::p1_menu_right),
            Self::p2_up => Some(Self::p2_menu_up),
            Self::p2_down => Some(Self::p2_menu_down),
            Self::p2_left => Some(Self::p2_menu_left),
            Self::p2_right => Some(Self::p2_menu_right),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    pub action: VirtualAction,
    pub input_slot: u32,
    pub pressed: bool,
    pub source: InputSource,
    // Timestamp of the raw input edge before debounce filtering.
    pub timestamp: Instant,
    // Host/QPC clock for `timestamp` when the backend can provide one; 0 means
    // the event only has a local `Instant` anchor.
    pub timestamp_host_nanos: u64,
    // Timestamp at which the edge entered the debounce store on the main input path.
    pub stored_at: Instant,
    // Timestamp at which the debounced/normalized input event was emitted.
    pub emitted_at: Instant,
}

#[cfg(test)]
mod tests {
    use super::{Lane, VirtualAction};

    #[test]
    fn lane_indices_are_stable() {
        assert_eq!(Lane::Left.index(), 0);
        assert_eq!(Lane::P2Right.index(), 7);
    }

    #[test]
    fn gameplay_arrow_and_menu_aliases_match() {
        assert!(VirtualAction::p1_left.is_gameplay_arrow());
        assert!(!VirtualAction::p1_start.is_gameplay_arrow());
        assert_eq!(
            VirtualAction::p2_right.secondary_menu(),
            Some(VirtualAction::p2_menu_right)
        );
        assert_eq!(VirtualAction::p2_start.secondary_menu(), None);
    }
}
