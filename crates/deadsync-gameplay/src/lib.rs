use deadsync_chart::SyncPref;
use deadsync_core::input::MAX_PLAYERS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayViewport {
    width: f32,
    height: f32,
}

impl GameplayViewport {
    pub const fn design() -> Self {
        Self {
            width: 854.0,
            height: 480.0,
        }
    }

    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: if width.is_finite() && width > 0.0 {
                width
            } else {
                Self::design().width
            },
            height: if height.is_finite() && height > 0.0 {
                height
            } else {
                Self::design().height
            },
        }
    }

    #[inline(always)]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[inline(always)]
    pub const fn height(self) -> f32 {
        self.height
    }

    #[inline(always)]
    pub const fn center_x(self) -> f32 {
        self.width * 0.5
    }

    #[inline(always)]
    pub const fn center_y(self) -> f32 {
        self.height * 0.5
    }

    #[inline(always)]
    pub fn is_wide(self) -> bool {
        self.width / self.height >= 1.6
    }
}

impl Default for GameplayViewport {
    fn default() -> Self {
        Self::design()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameplayFailType {
    Immediate,
    ImmediateContinue,
}

impl Default for GameplayFailType {
    fn default() -> Self {
        Self::ImmediateContinue
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayConfig {
    pub mine_hit_sound: bool,
    pub default_fail_type: GameplayFailType,
    pub global_offset_seconds: f32,
    pub visual_delay_seconds: f32,
    pub machine_pack_ini_offsets: bool,
    pub machine_default_sync_pref: SyncPref,
    pub machine_allow_per_player_global_offsets: bool,
    pub machine_enable_replays: bool,
    pub center_1player_notefield: bool,
    pub delayed_back: bool,
}

impl Default for GameplayConfig {
    fn default() -> Self {
        Self {
            mine_hit_sound: true,
            default_fail_type: GameplayFailType::ImmediateContinue,
            global_offset_seconds: -0.008,
            visual_delay_seconds: 0.0,
            machine_pack_ini_offsets: false,
            machine_default_sync_pref: SyncPref::Null,
            machine_allow_per_player_global_offsets: false,
            machine_enable_replays: true,
            center_1player_notefield: false,
            delayed_back: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameplayMiniIndicatorData {
    pub personal_best_percent: [Option<f64>; MAX_PLAYERS],
    pub machine_best_percent: [Option<f64>; MAX_PLAYERS],
}

impl Default for GameplayMiniIndicatorData {
    fn default() -> Self {
        Self {
            personal_best_percent: [None; MAX_PLAYERS],
            machine_best_percent: [None; MAX_PLAYERS],
        }
    }
}
