pub const MAX_PLAYERS: usize = 2;
/// Routine gives each participant a full double field, so two pump-double
/// fields are the widest layout.
pub const MAX_COLS: usize = 20;
/// The widest field one player owns (pump-double).
pub const MAX_COLS_PER_PLAYER: usize = 10;
pub type LaneMask = u32;

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
    Col8 = 8,
    Col9 = 9,
    Col10 = 10,
    Col11 = 11,
    Col12 = 12,
    Col13 = 13,
    Col14 = 14,
    Col15 = 15,
    Col16 = 16,
    Col17 = 17,
    Col18 = 18,
    Col19 = 19,
}

impl Lane {
    #[inline(always)]
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Keyboard,
    Gamepad,
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

impl std::str::FromStr for SmxPadPreset {
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

#[cfg(test)]
mod tests {
    use super::{Lane, MAX_COLS, MAX_COLS_PER_PLAYER, MAX_PLAYERS};

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
        assert_eq!(Lane::Col8.index(), 8);
        assert_eq!(Lane::Col9.index(), 9);
        assert_eq!(Lane::Col19.index(), 19);
    }

    #[test]
    fn player_and_column_limits_match_lane_model() {
        assert_eq!(MAX_PLAYERS, 2);
        assert_eq!(MAX_COLS, 20);
        assert_eq!(MAX_COLS, MAX_PLAYERS * MAX_COLS_PER_PLAYER);
    }
}
