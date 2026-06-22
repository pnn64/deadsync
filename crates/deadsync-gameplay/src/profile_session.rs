#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayInputPlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl GameplayInputPlayStyle {
    #[inline(always)]
    pub const fn cols_per_player(self) -> usize {
        match self {
            Self::Single | Self::Versus => 4,
            Self::Double => 8,
        }
    }

    #[inline(always)]
    pub const fn player_count(self) -> usize {
        match self {
            Self::Single | Self::Double => 1,
            Self::Versus => 2,
        }
    }

    #[inline(always)]
    pub const fn total_cols(self) -> usize {
        self.cols_per_player() * self.player_count()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayInputPlayerSide {
    #[default]
    P1,
    P2,
}

#[inline(always)]
pub const fn gameplay_player_side_index(side: GameplayInputPlayerSide) -> usize {
    match side {
        GameplayInputPlayerSide::P1 => 0,
        GameplayInputPlayerSide::P2 => 1,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayErrorBarTrim {
    #[default]
    Off,
    Fantastic,
    Excellent,
    Great,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayErrorBarOptions {
    pub mask_bits: u8,
    pub text_scalable: bool,
    pub text_threshold_ms: u32,
    pub show_fa_plus_window: bool,
    pub trim: GameplayErrorBarTrim,
    pub multi_tick: bool,
    pub error_ms_display: bool,
    pub short_average_enabled: bool,
    pub short_average_intensity: f32,
    pub long_average_enabled: bool,
    pub long_average_threshold_ms: u32,
    pub long_average_intensity: f32,
    pub long_average_min_samples: u32,
    pub average_interval_ms: u32,
}

pub const GAMEPLAY_ERROR_BAR_COLORFUL: u8 = 1 << 0;
pub const GAMEPLAY_ERROR_BAR_MONOCHROME: u8 = 1 << 1;
pub const GAMEPLAY_ERROR_BAR_TEXT: u8 = 1 << 2;
pub const GAMEPLAY_ERROR_BAR_HIGHLIGHT: u8 = 1 << 3;
pub const GAMEPLAY_ERROR_BAR_AVERAGE: u8 = 1 << 4;

pub trait GameplayProfileData: Clone {
    fn insert_mask_bits(&self) -> u8;
    fn remove_mask_bits(&self) -> u8;
    fn holds_mask_bits(&self) -> u8;
    fn appearance_mask_bits(&self) -> u8;
    fn visual_mask_bits(&self) -> u16;
    fn turn_option(&self) -> GameplayTurnOption;
    fn attack_mode(&self) -> GameplayAttackMode;
    fn perspective_effects(&self) -> PerspectiveEffects;
    fn scroll_effects(&self) -> ScrollEffects;
    fn mini_indicator_options(&self) -> GameplayMiniIndicatorOptions;
    fn target_score(&self) -> GameplayTargetScoreSetting;
    fn timing_disabled_windows(&self) -> [bool; 5];
    fn column_flash_options(&self) -> ColumnFlashOptions;
    fn tap_explosion_options(&self) -> TapExplosionOptions;
    fn fantastic_options(&self, base_fa_plus_s: f32) -> FantasticWindowOptions;
    fn fantastic_feedback_options(&self) -> FantasticFeedbackOptions;
    fn error_bar_options(&self) -> GameplayErrorBarOptions;
    fn measure_counter_threshold(&self) -> Option<usize>;
    fn step_statistics_density_graph(&self) -> bool;
    fn note_field_offset_x(&self) -> f32;
    fn noteskin_name(&self) -> String;
    fn mini_percent(&self) -> f32;
    fn global_offset_shift_ms(&self) -> i32;
    fn visual_delay_ms(&self) -> i32;
    fn reverse_scroll(&self) -> bool;
    fn column_cues(&self) -> bool;
    fn crossover_cues(&self) -> bool;
    fn crossover_cue_duration_ms(&self) -> u16;
    fn crossover_cue_quantization(&self) -> u8;
    fn crossover_cue_brackets(&self) -> bool;
    fn nps_graph_at_top(&self) -> bool;
    fn carry_combo_between_songs(&self) -> bool;
    fn calculated_weight_pounds(&self) -> i32;
    fn hide_lifebar(&self) -> bool;
    fn hide_danger(&self) -> bool;
    fn rescore_early_hits(&self) -> bool;
    fn hide_early_dw_judgments(&self) -> bool;
    fn hide_early_dw_flash(&self) -> bool;
    fn hide_early_dw_column_flash(&self) -> bool;

    #[inline(always)]
    fn chart_effects(&self) -> ChartAttackEffects {
        ChartAttackEffects {
            insert_mask: self.insert_mask_bits(),
            remove_mask: self.remove_mask_bits(),
            holds_mask: self.holds_mask_bits(),
            turn_bits: 0,
        }
    }

    #[inline(always)]
    fn appearance_effects(&self) -> AppearanceEffects {
        AppearanceEffects::from_mask_bits(self.appearance_mask_bits())
    }

    #[inline(always)]
    fn visual_effects(&self) -> VisualEffects {
        VisualEffects::from_mask_bits(self.visual_mask_bits())
    }

    #[inline(always)]
    fn effective_mini_value_with_visual_mask(&self, visual_mask: u16, mini_percent: f32) -> f32 {
        mini_value_for_visual_mask(mini_percent, self.mini_percent(), visual_mask)
    }

    #[inline(always)]
    fn draw_scale_for_tilt_with_visual_mask(
        &self,
        tilt: f32,
        visual_mask: u16,
        mini_percent: f32,
    ) -> f32 {
        player_draw_scale_for_visual_mask(tilt, mini_percent, self.mini_percent(), visual_mask)
    }
}

pub const DEFAULT_NOTESKIN_NAME: &str = "cel";

#[inline(always)]
pub const fn gameplay_player_side_for_index(player_idx: usize) -> GameplayInputPlayerSide {
    match player_idx {
        1 => GameplayInputPlayerSide::P2,
        _ => GameplayInputPlayerSide::P1,
    }
}

#[inline(always)]
pub const fn gameplay_runtime_player_is_p2(
    play_style: GameplayInputPlayStyle,
    side: GameplayInputPlayerSide,
) -> bool {
    matches!(
        (play_style, side),
        (
            GameplayInputPlayStyle::Single | GameplayInputPlayStyle::Double,
            GameplayInputPlayerSide::P2
        )
    )
}

#[inline(always)]
pub const fn gameplay_is_single_p2_side(
    play_style: GameplayInputPlayStyle,
    side: GameplayInputPlayerSide,
) -> bool {
    matches!(
        (play_style, side),
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2)
    )
}

#[inline(always)]
pub const fn gameplay_runtime_player_side(
    play_style: GameplayInputPlayStyle,
    session_side: GameplayInputPlayerSide,
    player_idx: usize,
) -> GameplayInputPlayerSide {
    if matches!(play_style, GameplayInputPlayStyle::Versus) {
        gameplay_player_side_for_index(player_idx)
    } else {
        session_side
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplaySession {
    pub play_style: GameplayInputPlayStyle,
    pub player_side: GameplayInputPlayerSide,
    pub joined_sides: [bool; MAX_PLAYERS],
    pub active_profile_ids: [Option<String>; MAX_PLAYERS],
    pub tick_mode: GameplayTimingTickMode,
}

impl GameplaySession {
    pub fn active_profile_id_for_side(&self, side: GameplayInputPlayerSide) -> Option<String> {
        self.active_profile_ids[gameplay_player_side_index(side)].clone()
    }

    #[inline(always)]
    pub const fn side_joined(&self, side: GameplayInputPlayerSide) -> bool {
        self.joined_sides[gameplay_player_side_index(side)]
    }

    #[inline(always)]
    pub const fn p2_runtime_player(&self) -> bool {
        gameplay_runtime_player_is_p2(self.play_style, self.player_side)
    }

    #[inline(always)]
    pub const fn runtime_player_side(&self, player_idx: usize) -> GameplayInputPlayerSide {
        gameplay_runtime_player_side(self.play_style, self.player_side, player_idx)
    }
}

impl Default for GameplaySession {
    fn default() -> Self {
        Self {
            play_style: GameplayInputPlayStyle::Single,
            player_side: GameplayInputPlayerSide::P1,
            joined_sides: [true, false],
            active_profile_ids: [None, None],
            tick_mode: GameplayTimingTickMode::Off,
        }
    }
}

