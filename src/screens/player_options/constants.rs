use super::*;

pub(super) const TRANSITION_IN_DURATION: f32 = 0.4;

pub(super) const TRANSITION_OUT_DURATION: f32 = 0.4;

pub(super) const SL_OPTION_ROW_TWEEN_SECONDS: f32 = 0.1;

pub(super) const CURSOR_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;

pub(super) const ROW_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;

pub(super) const PANE_FADE_SECONDS: f32 = 0.2;

pub(super) const TAP_EXPLOSION_PREVIEW_SPEED: f32 = 0.7;

pub(super) const INLINE_SPACING: f32 = 15.75;

pub(super) const TILT_INTENSITY_MIN: f32 = 0.05;

pub(super) const TILT_INTENSITY_MAX: f32 = 10.00;

pub(super) const TILT_INTENSITY_STEP: f32 = 0.05;

pub(super) const LONG_ERROR_BAR_INTENSITY_MIN: f32 = deadsync_profile::LONG_ERROR_BAR_INTENSITY_MIN;

pub(super) const LONG_ERROR_BAR_INTENSITY_MAX: f32 = deadsync_profile::LONG_ERROR_BAR_INTENSITY_MAX;

pub(super) const LONG_ERROR_BAR_INTENSITY_STEP: f32 =
    deadsync_profile::LONG_ERROR_BAR_INTENSITY_STEP;

pub(super) const AVERAGE_ERROR_BAR_INTENSITY_MIN: f32 =
    deadsync_profile::AVERAGE_ERROR_BAR_INTENSITY_MIN;

pub(super) const AVERAGE_ERROR_BAR_INTENSITY_MAX: f32 =
    deadsync_profile::AVERAGE_ERROR_BAR_INTENSITY_MAX;

pub(super) const AVERAGE_ERROR_BAR_INTENSITY_STEP: f32 =
    deadsync_profile::AVERAGE_ERROR_BAR_INTENSITY_STEP;

pub(super) const AVERAGE_ERROR_BAR_INTERVAL_MS_MIN: u32 =
    deadsync_profile::AVERAGE_ERROR_BAR_INTERVAL_MS_MIN;

pub(super) const AVERAGE_ERROR_BAR_INTERVAL_MS_MAX: u32 =
    deadsync_profile::AVERAGE_ERROR_BAR_INTERVAL_MS_MAX;

pub(super) const AVERAGE_ERROR_BAR_INTERVAL_MS_STEP: u32 =
    deadsync_profile::AVERAGE_ERROR_BAR_INTERVAL_MS_STEP;

pub(super) const TEXT_ERROR_BAR_THRESHOLD_MS_MIN: u32 =
    deadsync_profile::TEXT_ERROR_BAR_THRESHOLD_MS_MIN;

pub(super) const TEXT_ERROR_BAR_THRESHOLD_MS_MAX: u32 =
    deadsync_profile::TEXT_ERROR_BAR_THRESHOLD_MS_MAX;

pub(super) const LONG_ERROR_BAR_THRESHOLD_MS_MIN: u32 =
    deadsync_profile::LONG_ERROR_BAR_THRESHOLD_MS_MIN;

pub(super) const LONG_ERROR_BAR_THRESHOLD_MS_MAX: u32 =
    deadsync_profile::LONG_ERROR_BAR_THRESHOLD_MS_MAX;

pub(super) const LONG_ERROR_BAR_MIN_SAMPLES_MIN: u32 =
    deadsync_profile::LONG_ERROR_BAR_MIN_SAMPLES_MIN;

pub(super) const LONG_ERROR_BAR_MIN_SAMPLES_MAX: u32 =
    deadsync_profile::LONG_ERROR_BAR_MIN_SAMPLES_MAX;

pub(super) const TILT_THRESHOLD_MIN_MS: u32 = deadsync_profile::TILT_THRESHOLD_MIN_MS;

pub(super) const TILT_THRESHOLD_MAX_MS: u32 = deadsync_profile::TILT_THRESHOLD_MAX_MS;

pub(super) const HUD_OFFSET_MIN: i32 = deadsync_profile::HUD_OFFSET_MIN;

pub(super) const HUD_OFFSET_MAX: i32 = deadsync_profile::HUD_OFFSET_MAX;

pub(super) const HUD_OFFSET_ZERO_INDEX: usize = (-HUD_OFFSET_MIN) as usize;

pub(super) const SPACING_PERCENT_MIN: i32 = deadsync_profile::SPACING_PERCENT_MIN;

pub(super) const SPACING_PERCENT_MAX: i32 = deadsync_profile::SPACING_PERCENT_MAX;

pub(super) const VISIBLE_ROWS: usize = 10;

pub(super) const ROW_START_OFFSET: f32 = -164.0;

pub(super) const ROW_HEIGHT: f32 = 33.0;

pub(super) const TITLE_BG_WIDTH: f32 = 127.0;

pub(super) fn hud_offset_choices() -> Vec<String> {
    (HUD_OFFSET_MIN..=HUD_OFFSET_MAX)
        .map(|v| v.to_string())
        .collect()
}

pub(super) const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);

pub(super) const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

pub(super) const PLAYER_SLOTS: usize = 2;

pub(super) const P1: usize = 0;

pub(super) const P2: usize = 1;

pub(super) const MATCH_NOTESKIN_LABEL: &str = "MatchNoteSkinLabel";

pub(super) const NO_TAP_EXPLOSION_LABEL: &str = "NoTapExplosionLabel";

use deadsync_profile::{
    AttackMode, ColumnFlashBrightness, ColumnFlashSize, ComboColors, ComboFont, ComboMode,
    ErrorBarTrim, HideLightType, LifeMeterType, MeasureCounter, MeasureLines, MiniIndicator,
    MiniIndicatorColor, MiniIndicatorPosition, MiniIndicatorScoreType, MiniIndicatorSize,
    MiniIndicatorSubtractiveDisplay, NoCmodAlternative, Perspective, ScatterplotMaxWindow,
    ScoreDisplayMode, ScorePosition, TargetScoreSetting, TimingWindowsOption, TurnOption,
};

/// `NoCmodAlternative` variants in row-choice order (index ↔ enum).
pub(super) const NO_CMOD_ALTERNATIVE_VARIANTS: [NoCmodAlternative; 3] = [
    NoCmodAlternative::None,
    NoCmodAlternative::XMod,
    NoCmodAlternative::MMod,
];

/// MiniIndicator variants in row-choice order (index ↔ enum).
pub(super) const MINI_INDICATOR_VARIANTS: [MiniIndicator; 7] = [
    MiniIndicator::None,
    MiniIndicator::SubtractiveScoring,
    MiniIndicator::PredictiveScoring,
    MiniIndicator::PaceScoring,
    MiniIndicator::RivalScoring,
    MiniIndicator::Pacemaker,
    MiniIndicator::StreamProg,
];

pub(super) const COLUMN_FLASH_BRIGHTNESS_VARIANTS: [ColumnFlashBrightness; 2] =
    [ColumnFlashBrightness::Normal, ColumnFlashBrightness::Dimmed];

pub(super) const COLUMN_FLASH_SIZE_VARIANTS: [ColumnFlashSize; 2] =
    [ColumnFlashSize::Default, ColumnFlashSize::Compact];

pub(super) const TURN_OPTION_VARIANTS: [TurnOption; 9] = [
    TurnOption::None,
    TurnOption::Mirror,
    TurnOption::Left,
    TurnOption::Right,
    TurnOption::LRMirror,
    TurnOption::UDMirror,
    TurnOption::Shuffle,
    TurnOption::Blender,
    TurnOption::Random,
];

pub(super) const PERSPECTIVE_VARIANTS: [Perspective; 5] = [
    Perspective::Overhead,
    Perspective::Hallway,
    Perspective::Distant,
    Perspective::Incoming,
    Perspective::Space,
];

pub(super) const COMBO_FONT_VARIANTS: [ComboFont; 9] = [
    ComboFont::Wendy,
    ComboFont::ArialRounded,
    ComboFont::Asap,
    ComboFont::BebasNeue,
    ComboFont::SourceCode,
    ComboFont::Work,
    ComboFont::WendyCursed,
    ComboFont::Mega,
    ComboFont::None,
];

pub(super) const COMBO_COLORS_VARIANTS: [ComboColors; 5] = [
    ComboColors::Glow,
    ComboColors::Solid,
    ComboColors::Rainbow,
    ComboColors::RainbowScroll,
    ComboColors::None,
];

pub(super) const COMBO_MODE_VARIANTS: [ComboMode; 2] =
    [ComboMode::FullCombo, ComboMode::CurrentCombo];

pub(super) const SCATTERPLOT_MAX_WINDOW_VARIANTS: [ScatterplotMaxWindow; 4] = [
    ScatterplotMaxWindow::Off,
    ScatterplotMaxWindow::Fantastic,
    ScatterplotMaxWindow::Excellent,
    ScatterplotMaxWindow::Great,
];

pub(super) const SCORE_POSITION_VARIANTS: [ScorePosition; 2] =
    [ScorePosition::Normal, ScorePosition::StepStatistics];

pub(super) const SCORE_DISPLAY_MODE_VARIANTS: [ScoreDisplayMode; 2] =
    [ScoreDisplayMode::Normal, ScoreDisplayMode::Predictive];

pub(super) const STEP_STATS_EXTRA_VARIANTS: [deadsync_profile::StepStatsExtra; 13] = [
    deadsync_profile::StepStatsExtra::None,
    deadsync_profile::StepStatsExtra::ErrorStats,
    deadsync_profile::StepStatsExtra::AmongUs,
    deadsync_profile::StepStatsExtra::BrodyQuest,
    deadsync_profile::StepStatsExtra::CatJAM,
    deadsync_profile::StepStatsExtra::CrabPls,
    deadsync_profile::StepStatsExtra::DancingDuck,
    deadsync_profile::StepStatsExtra::DonChan,
    deadsync_profile::StepStatsExtra::NyanCat,
    deadsync_profile::StepStatsExtra::Randomizer,
    deadsync_profile::StepStatsExtra::RinCat,
    deadsync_profile::StepStatsExtra::Snoop,
    deadsync_profile::StepStatsExtra::Sonic,
];

pub(super) const TARGET_SCORE_VARIANTS: [TargetScoreSetting; 14] = [
    TargetScoreSetting::CMinus,
    TargetScoreSetting::C,
    TargetScoreSetting::CPlus,
    TargetScoreSetting::BMinus,
    TargetScoreSetting::B,
    TargetScoreSetting::BPlus,
    TargetScoreSetting::AMinus,
    TargetScoreSetting::A,
    TargetScoreSetting::APlus,
    TargetScoreSetting::SMinus,
    TargetScoreSetting::S,
    TargetScoreSetting::SPlus,
    TargetScoreSetting::MachineBest,
    TargetScoreSetting::PersonalBest,
];

pub(super) const LIFE_METER_TYPE_VARIANTS: [LifeMeterType; 3] = [
    LifeMeterType::Standard,
    LifeMeterType::Surround,
    LifeMeterType::Vertical,
];

pub(super) const ERROR_BAR_TRIM_VARIANTS: [ErrorBarTrim; 4] = [
    ErrorBarTrim::Off,
    ErrorBarTrim::Fantastic,
    ErrorBarTrim::Excellent,
    ErrorBarTrim::Great,
];

pub(super) const MEASURE_COUNTER_VARIANTS: [MeasureCounter; 6] = [
    MeasureCounter::None,
    MeasureCounter::Eighth,
    MeasureCounter::Twelfth,
    MeasureCounter::Sixteenth,
    MeasureCounter::TwentyFourth,
    MeasureCounter::ThirtySecond,
];

pub(super) const MEASURE_LINES_VARIANTS: [MeasureLines; 4] = [
    MeasureLines::Off,
    MeasureLines::Measure,
    MeasureLines::Quarter,
    MeasureLines::Eighth,
];

pub(super) const TIMING_WINDOWS_VARIANTS: [TimingWindowsOption; 4] = [
    TimingWindowsOption::None,
    TimingWindowsOption::WayOffs,
    TimingWindowsOption::DecentsAndWayOffs,
    TimingWindowsOption::FantasticsAndExcellents,
];

pub(super) const MINI_INDICATOR_SCORE_TYPE_VARIANTS: [MiniIndicatorScoreType; 3] = [
    MiniIndicatorScoreType::Itg,
    MiniIndicatorScoreType::Ex,
    MiniIndicatorScoreType::HardEx,
];

pub(super) const MINI_INDICATOR_SUBTRACTIVE_DISPLAY_VARIANTS: [MiniIndicatorSubtractiveDisplay; 2] = [
    MiniIndicatorSubtractiveDisplay::Percent,
    MiniIndicatorSubtractiveDisplay::Points,
];

pub(super) const MINI_INDICATOR_SIZE_VARIANTS: [MiniIndicatorSize; 2] =
    [MiniIndicatorSize::Default, MiniIndicatorSize::Large];

pub(super) const MINI_INDICATOR_COLOR_VARIANTS: [MiniIndicatorColor; 3] = [
    MiniIndicatorColor::Default,
    MiniIndicatorColor::Detailed,
    MiniIndicatorColor::Combo,
];

pub(super) const MINI_INDICATOR_POSITION_VARIANTS: [MiniIndicatorPosition; 2] = [
    MiniIndicatorPosition::Default,
    MiniIndicatorPosition::UnderUpArrow,
];

pub(super) const ATTACK_MODE_VARIANTS: [AttackMode; 3] =
    [AttackMode::On, AttackMode::Random, AttackMode::Off];

pub(super) const HIDE_LIGHT_TYPE_VARIANTS: [HideLightType; 4] = [
    HideLightType::NoHideLights,
    HideLightType::HideAllLights,
    HideLightType::HideMarqueeLights,
    HideLightType::HideBassLights,
];

pub(super) const ARCADE_NEXT_ROW_TEXT: &str = "▼";
