pub const MAX_PLAYERS: usize = 2;
pub const MAX_COLS: usize = 8;

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

#[cfg(test)]
mod tests {
    use super::{Lane, MAX_COLS, MAX_PLAYERS};

    #[test]
    fn lane_indices_are_stable() {
        assert_eq!(Lane::Left.index(), 0);
        assert_eq!(Lane::Down.index(), 1);
        assert_eq!(Lane::Up.index(), 2);
        assert_eq!(Lane::Right.index(), 3);
        assert_eq!(Lane::P2Left.index(), 4);
        assert_eq!(Lane::P2Down.index(), 5);
        assert_eq!(Lane::P2Up.index(), 6);
        assert_eq!(Lane::P2Right.index(), 7);
    }

    #[test]
    fn player_and_column_limits_match_lane_model() {
        assert_eq!(MAX_PLAYERS, 2);
        assert_eq!(MAX_COLS, 8);
    }
}
