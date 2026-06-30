pub mod course;
pub mod import;
pub mod online;
pub mod pad_profiles;
pub mod parsing;
pub mod profile;
pub mod random_movies;
pub mod scores;
pub mod song;
pub mod stage_stats;

use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug)]
pub struct GameplayProfile(pub deadsync_profile::Profile);

impl Deref for GameplayProfile {
    type Target = deadsync_profile::Profile;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for GameplayProfile {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<deadsync_profile::Profile> for GameplayProfile {
    #[inline(always)]
    fn from(profile: deadsync_profile::Profile) -> Self {
        Self(profile)
    }
}

impl From<GameplayProfile> for deadsync_profile::Profile {
    #[inline(always)]
    fn from(profile: GameplayProfile) -> Self {
        profile.0
    }
}

pub fn gameplay_play_style_from_profile(
    play_style: deadsync_profile::PlayStyle,
) -> deadsync_gameplay::GameplayInputPlayStyle {
    match play_style {
        deadsync_profile::PlayStyle::Single => deadsync_gameplay::GameplayInputPlayStyle::Single,
        deadsync_profile::PlayStyle::Versus => deadsync_gameplay::GameplayInputPlayStyle::Versus,
        deadsync_profile::PlayStyle::Double => deadsync_gameplay::GameplayInputPlayStyle::Double,
    }
}

pub fn gameplay_player_side_from_profile(
    side: deadsync_profile::PlayerSide,
) -> deadsync_gameplay::GameplayInputPlayerSide {
    match side {
        deadsync_profile::PlayerSide::P1 => deadsync_gameplay::GameplayInputPlayerSide::P1,
        deadsync_profile::PlayerSide::P2 => deadsync_gameplay::GameplayInputPlayerSide::P2,
    }
}

pub fn profile_side_from_gameplay(
    side: deadsync_gameplay::GameplayInputPlayerSide,
) -> deadsync_profile::PlayerSide {
    match side {
        deadsync_gameplay::GameplayInputPlayerSide::P1 => deadsync_profile::PlayerSide::P1,
        deadsync_gameplay::GameplayInputPlayerSide::P2 => deadsync_profile::PlayerSide::P2,
    }
}

pub fn gameplay_tick_mode_from_profile(
    mode: deadsync_profile::TimingTickMode,
) -> deadsync_gameplay::GameplayTimingTickMode {
    match mode {
        deadsync_profile::TimingTickMode::Off => deadsync_gameplay::GameplayTimingTickMode::Off,
        deadsync_profile::TimingTickMode::Assist => {
            deadsync_gameplay::GameplayTimingTickMode::Assist
        }
        deadsync_profile::TimingTickMode::Hit => deadsync_gameplay::GameplayTimingTickMode::Hit,
    }
}

pub fn profile_tick_mode_from_gameplay(
    mode: deadsync_gameplay::GameplayTimingTickMode,
) -> deadsync_profile::TimingTickMode {
    match mode {
        deadsync_gameplay::GameplayTimingTickMode::Off => deadsync_profile::TimingTickMode::Off,
        deadsync_gameplay::GameplayTimingTickMode::Assist => {
            deadsync_profile::TimingTickMode::Assist
        }
        deadsync_gameplay::GameplayTimingTickMode::Hit => deadsync_profile::TimingTickMode::Hit,
    }
}

pub fn score_display_mode_from_profile(
    mode: deadsync_profile::ScoreDisplayMode,
) -> deadsync_gameplay::GameplayScoreDisplayMode {
    match mode {
        deadsync_profile::ScoreDisplayMode::Normal => {
            deadsync_gameplay::GameplayScoreDisplayMode::Normal
        }
        deadsync_profile::ScoreDisplayMode::Predictive => {
            deadsync_gameplay::GameplayScoreDisplayMode::Predictive
        }
    }
}

fn gameplay_target_score_setting(
    setting: deadsync_profile::TargetScoreSetting,
) -> deadsync_gameplay::GameplayTargetScoreSetting {
    match setting {
        deadsync_profile::TargetScoreSetting::CMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::CMinus
        }
        deadsync_profile::TargetScoreSetting::C => deadsync_gameplay::GameplayTargetScoreSetting::C,
        deadsync_profile::TargetScoreSetting::CPlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::CPlus
        }
        deadsync_profile::TargetScoreSetting::BMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::BMinus
        }
        deadsync_profile::TargetScoreSetting::B => deadsync_gameplay::GameplayTargetScoreSetting::B,
        deadsync_profile::TargetScoreSetting::BPlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::BPlus
        }
        deadsync_profile::TargetScoreSetting::AMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::AMinus
        }
        deadsync_profile::TargetScoreSetting::A => deadsync_gameplay::GameplayTargetScoreSetting::A,
        deadsync_profile::TargetScoreSetting::APlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::APlus
        }
        deadsync_profile::TargetScoreSetting::SMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::SMinus
        }
        deadsync_profile::TargetScoreSetting::S => deadsync_gameplay::GameplayTargetScoreSetting::S,
        deadsync_profile::TargetScoreSetting::SPlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::SPlus
        }
        deadsync_profile::TargetScoreSetting::MachineBest => {
            deadsync_gameplay::GameplayTargetScoreSetting::MachineBest
        }
        deadsync_profile::TargetScoreSetting::PersonalBest => {
            deadsync_gameplay::GameplayTargetScoreSetting::PersonalBest
        }
    }
}

pub fn gameplay_attack_mode(
    mode: deadsync_profile::AttackMode,
) -> deadsync_gameplay::GameplayAttackMode {
    match mode {
        deadsync_profile::AttackMode::Off => deadsync_gameplay::GameplayAttackMode::Off,
        deadsync_profile::AttackMode::On => deadsync_gameplay::GameplayAttackMode::On,
        deadsync_profile::AttackMode::Random => deadsync_gameplay::GameplayAttackMode::Random,
    }
}

pub fn chart_effects_from_profile(
    profile: &deadsync_profile::Profile,
) -> deadsync_gameplay::ChartAttackEffects {
    deadsync_gameplay::ChartAttackEffects {
        insert_mask: profile.insert_active_mask.bits(),
        remove_mask: profile.remove_active_mask.bits(),
        holds_mask: profile.holds_active_mask.bits(),
        turn_bits: 0,
    }
}

pub fn scroll_effects_from_option(
    scroll: deadsync_profile::ScrollOption,
) -> deadsync_gameplay::ScrollEffects {
    deadsync_gameplay::scroll_effects_from_flags(
        scroll.contains(deadsync_profile::ScrollOption::Reverse),
        scroll.contains(deadsync_profile::ScrollOption::Split),
        scroll.contains(deadsync_profile::ScrollOption::Alternate),
        scroll.contains(deadsync_profile::ScrollOption::Cross),
        scroll.contains(deadsync_profile::ScrollOption::Centered),
    )
}

pub fn tap_explosion_options_from_profile(
    profile: &deadsync_profile::Profile,
) -> deadsync_gameplay::TapExplosionOptions {
    let mask = profile.tap_explosion_active_mask;
    deadsync_gameplay::TapExplosionOptions {
        fantastic: mask.contains(deadsync_profile::TapExplosionMask::FANTASTIC),
        excellent: mask.contains(deadsync_profile::TapExplosionMask::EXCELLENT),
        great: mask.contains(deadsync_profile::TapExplosionMask::GREAT),
        decent: mask.contains(deadsync_profile::TapExplosionMask::DECENT),
        way_off: mask.contains(deadsync_profile::TapExplosionMask::WAY_OFF),
        miss: mask.contains(deadsync_profile::TapExplosionMask::MISS),
        held: mask.contains(deadsync_profile::TapExplosionMask::HELD),
        holding: mask.contains(deadsync_profile::TapExplosionMask::HOLDING),
    }
}

fn gameplay_turn_option(
    turn: deadsync_profile::TurnOption,
) -> deadsync_gameplay::GameplayTurnOption {
    match turn {
        deadsync_profile::TurnOption::None => deadsync_gameplay::GameplayTurnOption::None,
        deadsync_profile::TurnOption::Mirror => deadsync_gameplay::GameplayTurnOption::Mirror,
        deadsync_profile::TurnOption::LRMirror => deadsync_gameplay::GameplayTurnOption::LRMirror,
        deadsync_profile::TurnOption::UDMirror => deadsync_gameplay::GameplayTurnOption::UDMirror,
        deadsync_profile::TurnOption::Left => deadsync_gameplay::GameplayTurnOption::Left,
        deadsync_profile::TurnOption::Right => deadsync_gameplay::GameplayTurnOption::Right,
        deadsync_profile::TurnOption::Shuffle => deadsync_gameplay::GameplayTurnOption::Shuffle,
        deadsync_profile::TurnOption::Blender => deadsync_gameplay::GameplayTurnOption::Blender,
        deadsync_profile::TurnOption::Random => deadsync_gameplay::GameplayTurnOption::Random,
    }
}

fn mini_indicator_mode(
    mode: deadsync_profile::MiniIndicator,
) -> deadsync_gameplay::GameplayMiniIndicatorMode {
    match mode {
        deadsync_profile::MiniIndicator::None => deadsync_gameplay::GameplayMiniIndicatorMode::None,
        deadsync_profile::MiniIndicator::SubtractiveScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::SubtractiveScoring
        }
        deadsync_profile::MiniIndicator::PredictiveScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::PredictiveScoring
        }
        deadsync_profile::MiniIndicator::PaceScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::PaceScoring
        }
        deadsync_profile::MiniIndicator::RivalScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::RivalScoring
        }
        deadsync_profile::MiniIndicator::Pacemaker => {
            deadsync_gameplay::GameplayMiniIndicatorMode::Pacemaker
        }
        deadsync_profile::MiniIndicator::StreamProg => {
            deadsync_gameplay::GameplayMiniIndicatorMode::StreamProg
        }
    }
}

fn error_bar_trim(trim: deadsync_profile::ErrorBarTrim) -> deadsync_gameplay::GameplayErrorBarTrim {
    match trim {
        deadsync_profile::ErrorBarTrim::Off => deadsync_gameplay::GameplayErrorBarTrim::Off,
        deadsync_profile::ErrorBarTrim::Fantastic => {
            deadsync_gameplay::GameplayErrorBarTrim::Fantastic
        }
        deadsync_profile::ErrorBarTrim::Excellent => {
            deadsync_gameplay::GameplayErrorBarTrim::Excellent
        }
        deadsync_profile::ErrorBarTrim::Great => deadsync_gameplay::GameplayErrorBarTrim::Great,
    }
}

impl deadsync_gameplay::GameplayProfileData for GameplayProfile {
    fn insert_mask_bits(&self) -> u8 {
        self.insert_active_mask.bits()
    }

    fn remove_mask_bits(&self) -> u8 {
        self.remove_active_mask.bits()
    }

    fn holds_mask_bits(&self) -> u8 {
        self.holds_active_mask.bits()
    }

    fn appearance_mask_bits(&self) -> u8 {
        self.appearance_effects_active_mask.bits()
    }

    fn visual_mask_bits(&self) -> u16 {
        self.visual_effects_active_mask.bits()
    }

    fn turn_option(&self) -> deadsync_gameplay::GameplayTurnOption {
        gameplay_turn_option(self.turn_option)
    }

    fn attack_mode(&self) -> deadsync_gameplay::GameplayAttackMode {
        gameplay_attack_mode(self.attack_mode)
    }

    fn perspective_effects(&self) -> deadsync_gameplay::PerspectiveEffects {
        let (tilt, skew) = self.perspective.tilt_skew();
        deadsync_gameplay::PerspectiveEffects { tilt, skew }
    }

    fn scroll_effects(&self) -> deadsync_gameplay::ScrollEffects {
        deadsync_gameplay::scroll_effects_from_flags(
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Reverse),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Split),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Alternate),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Cross),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Centered),
        )
    }

    fn mini_indicator_options(&self) -> deadsync_gameplay::GameplayMiniIndicatorOptions {
        deadsync_gameplay::GameplayMiniIndicatorOptions {
            requested_mode: mini_indicator_mode(self.mini_indicator),
            measure_counter_enabled: self.measure_counter != deadsync_profile::MeasureCounter::None,
            subtractive_scoring: self.subtractive_scoring,
            pacemaker: self.pacemaker,
        }
    }

    fn target_score(&self) -> deadsync_gameplay::GameplayTargetScoreSetting {
        gameplay_target_score_setting(self.target_score)
    }

    fn timing_disabled_windows(&self) -> [bool; 5] {
        self.timing_windows.disabled_windows()
    }

    fn column_flash_options(&self) -> deadsync_gameplay::ColumnFlashOptions {
        let mask = self.column_flash_mask;
        deadsync_gameplay::ColumnFlashOptions {
            enabled: self.column_flash_on_miss,
            blue_fantastic: mask.contains(deadsync_profile::ColumnFlashMask::BLUE_FANTASTIC),
            white_fantastic: mask.contains(deadsync_profile::ColumnFlashMask::WHITE_FANTASTIC),
            excellent: mask.contains(deadsync_profile::ColumnFlashMask::EXCELLENT),
            great: mask.contains(deadsync_profile::ColumnFlashMask::GREAT),
            decent: mask.contains(deadsync_profile::ColumnFlashMask::DECENT),
            way_off: mask.contains(deadsync_profile::ColumnFlashMask::WAY_OFF),
            miss: mask.contains(deadsync_profile::ColumnFlashMask::MISS),
        }
    }

    fn tap_explosion_options(&self) -> deadsync_gameplay::TapExplosionOptions {
        tap_explosion_options_from_profile(self)
    }

    fn fantastic_options(&self, base_fa_plus_s: f32) -> deadsync_gameplay::FantasticWindowOptions {
        deadsync_gameplay::FantasticWindowOptions {
            base_fa_plus_s,
            custom_fantastic_window_s: self.custom_fantastic_window.then_some(
                f32::from(deadsync_profile::clamp_custom_fantastic_window_ms(
                    self.custom_fantastic_window_ms,
                )) / 1000.0,
            ),
            fa_plus_10ms_blue_window: self.fa_plus_10ms_blue_window,
        }
    }

    fn fantastic_feedback_options(&self) -> deadsync_gameplay::FantasticFeedbackOptions {
        deadsync_gameplay::FantasticFeedbackOptions {
            show_fa_plus_window: self.show_fa_plus_window,
            fa_plus_10ms_blue_window: self.fa_plus_10ms_blue_window,
            split_15_10ms: self.split_15_10ms,
            custom_fantastic_window: self.custom_fantastic_window,
        }
    }

    fn error_bar_options(&self) -> deadsync_gameplay::GameplayErrorBarOptions {
        let mut mask = self.error_bar_active_mask;
        if mask.is_empty() {
            mask = deadsync_profile::error_bar_mask_from_style(self.error_bar, self.error_bar_text);
        }
        deadsync_gameplay::GameplayErrorBarOptions {
            mask_bits: mask.bits(),
            text_scalable: self.text_error_bar_scalable,
            text_threshold_ms: deadsync_profile::clamp_text_error_bar_threshold_ms(
                self.text_error_bar_threshold_ms,
            ),
            show_fa_plus_window: self.show_fa_plus_window,
            trim: error_bar_trim(self.error_bar_trim),
            multi_tick: self.error_bar_multi_tick,
            error_ms_display: self.error_ms_display,
            short_average_enabled: self.short_average_error_bar_enabled,
            short_average_intensity: deadsync_profile::clamp_average_error_bar_intensity(
                self.average_error_bar_intensity,
            ),
            long_average_enabled: self.long_error_bar_enabled,
            long_average_threshold_ms: deadsync_profile::clamp_long_error_bar_threshold_ms(
                self.long_error_bar_threshold_ms,
            ),
            long_average_intensity: deadsync_profile::clamp_long_error_bar_intensity(
                self.long_error_bar_intensity,
            ),
            long_average_min_samples: deadsync_profile::clamp_long_error_bar_min_samples(
                self.long_error_bar_min_samples,
            ),
            average_interval_ms: deadsync_profile::clamp_average_error_bar_interval_ms(
                self.average_error_bar_interval_ms,
            ),
        }
    }

    fn measure_counter_threshold(&self) -> Option<usize> {
        self.measure_counter.notes_threshold()
    }

    fn step_statistics_density_graph(&self) -> bool {
        self.step_statistics
            .contains(deadsync_profile::StepStatisticsMask::DENSITY_GRAPH)
    }

    fn note_field_offset_x(&self) -> f32 {
        self.note_field_offset_x as f32
    }

    fn noteskin_name(&self) -> String {
        self.noteskin.to_string()
    }

    fn mini_percent(&self) -> f32 {
        self.mini_percent as f32
    }

    fn global_offset_shift_ms(&self) -> i32 {
        self.global_offset_shift_ms
    }

    fn visual_delay_ms(&self) -> i32 {
        self.visual_delay_ms
    }

    fn reverse_scroll(&self) -> bool {
        self.reverse_scroll
    }

    fn column_cues(&self) -> bool {
        self.column_cues
    }

    fn crossover_cues(&self) -> bool {
        self.crossover_cues
    }

    fn crossover_cue_duration_ms(&self) -> u16 {
        self.crossover_cue_duration_ms
    }

    fn crossover_cue_quantization(&self) -> u8 {
        self.crossover_cue_quantization
    }

    fn crossover_cue_brackets(&self) -> bool {
        self.crossover_cue_brackets
    }

    fn nps_graph_at_top(&self) -> bool {
        self.nps_graph_at_top
    }

    fn carry_combo_between_songs(&self) -> bool {
        self.carry_combo_between_songs
    }

    fn calculated_weight_pounds(&self) -> i32 {
        self.0.calculated_weight_pounds()
    }

    fn hide_lifebar(&self) -> bool {
        self.hide_lifebar
    }

    fn hide_danger(&self) -> bool {
        self.hide_danger
    }

    fn rescore_early_hits(&self) -> bool {
        self.rescore_early_hits
    }

    fn hide_early_dw_judgments(&self) -> bool {
        self.hide_early_dw_judgments
    }

    fn hide_early_dw_flash(&self) -> bool {
        self.hide_early_dw_flash
    }

    fn hide_early_dw_column_flash(&self) -> bool {
        self.hide_early_dw_column_flash
    }
}

pub type GameplayCoreState = deadsync_gameplay::GameplayRuntimeState<
    GameplayProfile,
    parsing::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<deadsync_song_lua::SongLuaOverlayStateDelta>,
>;
