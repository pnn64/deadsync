use std::path::PathBuf;

pub const PLAYER_SLOTS: usize = 2;
pub const DEFAULT_WEIGHT_POUNDS: i32 = 120;
pub const DEFAULT_BIRTH_YEAR: i32 = 1995;
pub const PLAYER_INITIALS_MAX_LEN: usize = 4;
pub const HUD_OFFSET_MIN: i32 = -250;
pub const HUD_OFFSET_MAX: i32 = 250;
pub const SPACING_PERCENT_MIN: i32 = -100;
pub const SPACING_PERCENT_MAX: i32 = 100;
pub const MINI_PERCENT_MIN: i32 = -100;
pub const MINI_PERCENT_MAX: i32 = 150;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveProfile {
    Guest,
    Local { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl PlayStyle {
    #[inline(always)]
    pub const fn chart_type(self) -> &'static str {
        match self {
            Self::Single | Self::Versus => "dance-single",
            Self::Double => "dance-double",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    Regular,
    Marathon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerSide {
    #[default]
    P1,
    P2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingTickMode {
    #[default]
    Off,
    Assist,
    Hit,
}

#[derive(Debug, Clone, Default)]
pub struct GameplayHudPlayerSnapshot {
    pub joined: bool,
    pub guest: bool,
    pub display_name: String,
    pub avatar_texture_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameplayHudSnapshot {
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub p1: GameplayHudPlayerSnapshot,
    pub p2: GameplayHudPlayerSnapshot,
}

pub struct LocalProfileSummary {
    pub id: String,
    pub display_name: String,
    pub avatar_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_style_reports_chart_type() {
        assert_eq!(PlayStyle::Single.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Versus.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Double.chart_type(), "dance-double");
    }

    #[test]
    fn defaults_match_single_player_session() {
        assert_eq!(PLAYER_SLOTS, 2);
        assert_eq!(DEFAULT_WEIGHT_POUNDS, 120);
        assert_eq!(DEFAULT_BIRTH_YEAR, 1995);
        assert_eq!(PLAYER_INITIALS_MAX_LEN, 4);
        assert_eq!((HUD_OFFSET_MIN, HUD_OFFSET_MAX), (-250, 250));
        assert_eq!((SPACING_PERCENT_MIN, SPACING_PERCENT_MAX), (-100, 100));
        assert_eq!((MINI_PERCENT_MIN, MINI_PERCENT_MAX), (-100, 150));
        assert_eq!(PlayStyle::default(), PlayStyle::Single);
        assert_eq!(PlayMode::default(), PlayMode::Regular);
        assert_eq!(PlayerSide::default(), PlayerSide::P1);
        assert_eq!(TimingTickMode::default(), TimingTickMode::Off);
    }

    #[test]
    fn hud_player_snapshot_defaults_to_guestless_unjoined() {
        let snapshot = GameplayHudPlayerSnapshot::default();
        assert!(!snapshot.joined);
        assert!(!snapshot.guest);
        assert_eq!(snapshot.display_name, "");
        assert_eq!(snapshot.avatar_texture_key, None);
    }
}
