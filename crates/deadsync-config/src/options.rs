use crate::theme::{
    AUTO_SS_CLEARS, AUTO_SS_FAILS, AUTO_SS_PBS, AUTO_SS_QUADS, AUTO_SS_QUINTS,
    ArrowCloudQrLoginWhen, BreakdownStyle, DefaultFailType, DefaultSyncOffset,
    GrooveStatsQrLoginWhen, LanguageFlag, LogLevel, MachineBarColor, MachineEvaluationStyle,
    MachineFont, MachinePreferredPlayMode, MachinePreferredPlayStyle, NewPackMode,
    RandomBackgroundMode, SelectMusicItlRankMode, SelectMusicItlWheelMode,
    SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement, SelectMusicSongSelectBgMode,
    SelectMusicStepArtistBoxMode, SelectMusicWheelStyle, SrpgVariant, SyncGraphMode,
    VersionOverlaySide, VisualStyle, auto_screenshot_bit,
};
use std::str::FromStr;
use std::time::Duration;

pub const SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES: usize = 4;
pub const SELECT_MUSIC_CHART_INFO_NUM_CHOICES: usize = 3;
pub const MUSIC_WHEEL_SCROLL_SPEED_VALUES: [u8; 7] = [5, 10, 15, 25, 30, 45, 100];
pub const SHOW_STATS_MODE_MAX: u8 = 3;
pub const MAX_FPS_MIN: u16 = 5;
pub const MAX_FPS_MAX: u16 = 1000;
pub const MAX_FPS_STEP: u16 = 1;
pub const MAX_FPS_DEFAULT: u16 = 60;
pub const MAX_FPS_HOLD_FAST_AFTER: Duration = Duration::from_millis(700);
pub const MAX_FPS_HOLD_FASTER_AFTER: Duration = Duration::from_millis(1200);
pub const MAX_FPS_HOLD_FASTEST_AFTER: Duration = Duration::from_millis(1800);

pub fn bg_brightness_choice_index(brightness: f32) -> usize {
    ((clamp_bg_brightness(brightness) * 10.0).round() as i32).clamp(0, 10) as usize
}

pub fn bg_brightness_from_choice(idx: usize) -> f32 {
    idx.min(10) as f32 / 10.0
}

pub fn clamp_bg_brightness(brightness: f32) -> f32 {
    brightness.clamp(0.0, 1.0)
}

pub const fn clamp_show_stats_mode(mode: u8) -> u8 {
    if mode > SHOW_STATS_MODE_MAX {
        SHOW_STATS_MODE_MAX
    } else {
        mode
    }
}

pub fn parse_show_stats_mode(raw_mode: Option<&str>, raw_legacy: Option<&str>, default: u8) -> u8 {
    raw_mode
        .and_then(|v| v.parse::<u8>().ok())
        .map(clamp_show_stats_mode)
        .or_else(|| {
            raw_legacy
                .and_then(|v| v.parse::<u8>().ok())
                .map(|v| if v != 0 { 1 } else { 0 })
        })
        .unwrap_or(default)
}

pub fn parse_select_music_itl_rank_mode(
    raw_mode: Option<&str>,
    raw_legacy_chart_rank: Option<&str>,
    default: SelectMusicItlRankMode,
) -> SelectMusicItlRankMode {
    raw_mode
        .and_then(|v| SelectMusicItlRankMode::from_str(v).ok())
        .or_else(|| {
            raw_legacy_chart_rank
                .and_then(|v| v.parse::<u8>().ok())
                .map(|v| {
                    if v != 0 {
                        SelectMusicItlRankMode::Chart
                    } else {
                        SelectMusicItlRankMode::None
                    }
                })
        })
        .unwrap_or(default)
}

pub fn parse_select_music_song_select_bg_mode(
    raw_mode: Option<&str>,
    raw_legacy_mode: Option<&str>,
    default: SelectMusicSongSelectBgMode,
) -> SelectMusicSongSelectBgMode {
    raw_mode
        .or(raw_legacy_mode)
        .and_then(|v| SelectMusicSongSelectBgMode::from_str(v).ok())
        .unwrap_or(default)
}

pub fn music_wheel_scroll_speed_choice_index(speed: u8) -> usize {
    let mut best_idx = 0usize;
    let mut best_diff = u8::MAX;
    for (idx, value) in MUSIC_WHEEL_SCROLL_SPEED_VALUES.iter().enumerate() {
        let diff = speed.abs_diff(*value);
        if diff < best_diff {
            best_diff = diff;
            best_idx = idx;
        }
    }
    best_idx
}

pub fn music_wheel_scroll_speed_from_choice(idx: usize) -> u8 {
    MUSIC_WHEEL_SCROLL_SPEED_VALUES
        .get(idx)
        .copied()
        .unwrap_or(15)
}

#[inline(always)]
pub const fn scorebox_cycle_mask(itg: bool, ex: bool, hard_ex: bool, tournaments: bool) -> u8 {
    (itg as u8) | ((ex as u8) << 1) | ((hard_ex as u8) << 2) | ((tournaments as u8) << 3)
}

#[inline(always)]
pub const fn scorebox_cycle_cursor_index(
    itg: bool,
    ex: bool,
    hard_ex: bool,
    tournaments: bool,
) -> usize {
    if itg {
        0
    } else if ex {
        1
    } else if hard_ex {
        2
    } else if tournaments {
        3
    } else {
        0
    }
}

#[inline(always)]
pub const fn scorebox_cycle_bit_from_choice(idx: usize) -> u8 {
    if idx < SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES {
        1u8 << (idx as u8)
    } else {
        0
    }
}

#[inline(always)]
pub const fn auto_screenshot_cursor_index(mask: u8) -> usize {
    if (mask & AUTO_SS_PBS) != 0 {
        0
    } else if (mask & AUTO_SS_FAILS) != 0 {
        1
    } else if (mask & AUTO_SS_CLEARS) != 0 {
        2
    } else if (mask & AUTO_SS_QUADS) != 0 {
        3
    } else if (mask & AUTO_SS_QUINTS) != 0 {
        4
    } else {
        0
    }
}

#[inline(always)]
pub const fn auto_screenshot_bit_from_choice(idx: usize) -> u8 {
    auto_screenshot_bit(idx)
}

#[inline(always)]
pub const fn select_music_chart_info_mask(
    peak_nps: bool,
    effective_bpm: bool,
    matrix_rating: bool,
) -> u8 {
    (peak_nps as u8) | ((effective_bpm as u8) << 1) | ((matrix_rating as u8) << 2)
}

#[inline(always)]
pub const fn select_music_chart_info_cursor_index(
    peak_nps: bool,
    effective_bpm: bool,
    matrix_rating: bool,
) -> usize {
    if peak_nps {
        0
    } else if effective_bpm {
        1
    } else if matrix_rating {
        2
    } else {
        0
    }
}

#[inline(always)]
pub const fn select_music_chart_info_bit_from_choice(idx: usize) -> u8 {
    if idx < SELECT_MUSIC_CHART_INFO_NUM_CHOICES {
        1u8 << (idx as u8)
    } else {
        0
    }
}

#[inline(always)]
pub const fn select_music_chart_info_enabled_mask(mask: u8) -> u8 {
    if mask == 0 { 1 } else { mask }
}

pub fn build_max_fps_choices() -> Vec<u16> {
    let mut out = Vec::with_capacity(
        1 + usize::from(MAX_FPS_MAX.saturating_sub(MAX_FPS_MIN)) / usize::from(MAX_FPS_STEP),
    );
    let mut fps = MAX_FPS_MIN;
    while fps <= MAX_FPS_MAX {
        out.push(fps);
        fps = fps.saturating_add(MAX_FPS_STEP);
    }
    out
}

pub fn max_fps_hold_delta(delta: isize, held_for: Duration) -> isize {
    let multiplier = if held_for >= MAX_FPS_HOLD_FASTEST_AFTER {
        50
    } else if held_for >= MAX_FPS_HOLD_FASTER_AFTER {
        25
    } else if held_for >= MAX_FPS_HOLD_FAST_AFTER {
        10
    } else {
        5
    };
    delta * multiplier
}

#[inline(always)]
pub const fn clamped_max_fps(max_fps: u16) -> u16 {
    if max_fps < MAX_FPS_MIN {
        MAX_FPS_MIN
    } else if max_fps > MAX_FPS_MAX {
        MAX_FPS_MAX
    } else {
        max_fps
    }
}

pub fn max_fps_choice_index(values: &[u16], max_fps: u16) -> usize {
    let target = clamped_max_fps(max_fps);
    values.iter().position(|&v| v == target).unwrap_or_else(|| {
        values
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.abs_diff(target))
            .map_or(0, |(idx, _)| idx)
    })
}

pub fn max_fps_from_choice(values: &[u16], idx: usize) -> u16 {
    values.get(idx).copied().unwrap_or(MAX_FPS_DEFAULT)
}

pub const fn sync_confidence_choice_index(percent: u8) -> usize {
    let capped = if percent > 100 { 100 } else { percent };
    ((capped as usize) + 2) / 5
}

pub const fn sync_confidence_from_choice(idx: usize) -> u8 {
    let capped = if idx > 20 { 20 } else { idx };
    capped as u8 * 5
}

pub const fn translated_titles_choice_index(translated_titles: bool) -> usize {
    if translated_titles { 0 } else { 1 }
}

pub const fn translated_titles_from_choice(idx: usize) -> bool {
    idx == 0
}

pub const fn language_choice_index(flag: LanguageFlag) -> usize {
    match flag {
        LanguageFlag::Auto | LanguageFlag::English => 0,
        LanguageFlag::German => 1,
        LanguageFlag::Spanish => 2,
        LanguageFlag::French => 3,
        LanguageFlag::Italian => 4,
        LanguageFlag::Japanese => 5,
        LanguageFlag::Polish => 6,
        LanguageFlag::PortugueseBrazil => 7,
        LanguageFlag::Russian => 8,
        LanguageFlag::Swedish => 9,
        LanguageFlag::Pseudo => 10,
    }
}

pub const fn language_flag_from_choice(idx: usize) -> LanguageFlag {
    match idx {
        1 => LanguageFlag::German,
        2 => LanguageFlag::Spanish,
        3 => LanguageFlag::French,
        4 => LanguageFlag::Italian,
        5 => LanguageFlag::Japanese,
        6 => LanguageFlag::Polish,
        7 => LanguageFlag::PortugueseBrazil,
        8 => LanguageFlag::Russian,
        9 => LanguageFlag::Swedish,
        10 => LanguageFlag::Pseudo,
        _ => LanguageFlag::English,
    }
}

pub const fn breakdown_style_choice_index(style: BreakdownStyle) -> usize {
    match style {
        BreakdownStyle::Sl => 0,
        BreakdownStyle::Sn => 1,
    }
}

pub const fn breakdown_style_from_choice(idx: usize) -> BreakdownStyle {
    match idx {
        1 => BreakdownStyle::Sn,
        _ => BreakdownStyle::Sl,
    }
}

pub const fn select_music_pattern_info_mode_choice_index(
    mode: SelectMusicPatternInfoMode,
) -> usize {
    match mode {
        SelectMusicPatternInfoMode::Auto => 0,
        SelectMusicPatternInfoMode::Tech => 1,
        SelectMusicPatternInfoMode::Stamina => 2,
    }
}

pub const fn select_music_pattern_info_mode_from_choice(idx: usize) -> SelectMusicPatternInfoMode {
    match idx {
        1 => SelectMusicPatternInfoMode::Tech,
        2 => SelectMusicPatternInfoMode::Stamina,
        _ => SelectMusicPatternInfoMode::Auto,
    }
}

pub const fn select_music_step_artist_box_mode_choice_index(
    mode: SelectMusicStepArtistBoxMode,
) -> usize {
    match mode {
        SelectMusicStepArtistBoxMode::Default => 0,
        SelectMusicStepArtistBoxMode::Legacy => 1,
        SelectMusicStepArtistBoxMode::Expanded => 2,
    }
}

pub const fn select_music_step_artist_box_mode_from_choice(
    idx: usize,
) -> SelectMusicStepArtistBoxMode {
    match idx {
        1 => SelectMusicStepArtistBoxMode::Legacy,
        2 => SelectMusicStepArtistBoxMode::Expanded,
        _ => SelectMusicStepArtistBoxMode::Default,
    }
}

pub const fn select_music_itl_wheel_mode_choice_index(mode: SelectMusicItlWheelMode) -> usize {
    match mode {
        SelectMusicItlWheelMode::Off => 0,
        SelectMusicItlWheelMode::Score => 1,
        SelectMusicItlWheelMode::PointsAndScore => 2,
    }
}

pub const fn select_music_itl_wheel_mode_from_choice(idx: usize) -> SelectMusicItlWheelMode {
    match idx {
        1 => SelectMusicItlWheelMode::Score,
        2 => SelectMusicItlWheelMode::PointsAndScore,
        _ => SelectMusicItlWheelMode::Off,
    }
}

pub const fn select_music_itl_rank_mode_choice_index(mode: SelectMusicItlRankMode) -> usize {
    match mode {
        SelectMusicItlRankMode::None => 0,
        SelectMusicItlRankMode::Chart => 1,
        SelectMusicItlRankMode::Overall => 2,
    }
}

pub const fn select_music_itl_rank_mode_from_choice(idx: usize) -> SelectMusicItlRankMode {
    match idx {
        1 => SelectMusicItlRankMode::Chart,
        2 => SelectMusicItlRankMode::Overall,
        _ => SelectMusicItlRankMode::None,
    }
}

pub const fn select_music_wheel_style_choice_index(style: SelectMusicWheelStyle) -> usize {
    match style {
        SelectMusicWheelStyle::Itg => 0,
        SelectMusicWheelStyle::Iidx => 1,
    }
}

pub const fn select_music_wheel_style_from_choice(idx: usize) -> SelectMusicWheelStyle {
    match idx {
        1 => SelectMusicWheelStyle::Iidx,
        _ => SelectMusicWheelStyle::Itg,
    }
}

pub const fn select_music_song_select_bg_mode_choice_index(
    mode: SelectMusicSongSelectBgMode,
) -> usize {
    match mode {
        SelectMusicSongSelectBgMode::Off => 0,
        SelectMusicSongSelectBgMode::Banner => 1,
        SelectMusicSongSelectBgMode::Bg => 2,
    }
}

pub const fn select_music_song_select_bg_mode_from_choice(
    idx: usize,
) -> SelectMusicSongSelectBgMode {
    match idx {
        1 => SelectMusicSongSelectBgMode::Banner,
        2 => SelectMusicSongSelectBgMode::Bg,
        _ => SelectMusicSongSelectBgMode::Off,
    }
}

pub const fn select_music_new_pack_mode_choice_index(mode: NewPackMode) -> usize {
    match mode {
        NewPackMode::Disabled => 0,
        NewPackMode::OpenPack => 1,
        NewPackMode::HasScore => 2,
    }
}

pub const fn select_music_new_pack_mode_from_choice(idx: usize) -> NewPackMode {
    match idx {
        1 => NewPackMode::OpenPack,
        2 => NewPackMode::HasScore,
        _ => NewPackMode::Disabled,
    }
}

pub const fn select_music_scorebox_placement_choice_index(
    placement: SelectMusicScoreboxPlacement,
) -> usize {
    match placement {
        SelectMusicScoreboxPlacement::Auto => 0,
        SelectMusicScoreboxPlacement::StepPane => 1,
    }
}

pub const fn select_music_scorebox_placement_from_choice(
    idx: usize,
) -> SelectMusicScoreboxPlacement {
    match idx {
        1 => SelectMusicScoreboxPlacement::StepPane,
        _ => SelectMusicScoreboxPlacement::Auto,
    }
}

pub const fn log_level_choice_index(level: LogLevel) -> usize {
    match level {
        LogLevel::Error => 0,
        LogLevel::Warn => 1,
        LogLevel::Info => 2,
        LogLevel::Debug => 3,
        LogLevel::Trace => 4,
    }
}

pub const fn log_level_from_choice(idx: usize) -> LogLevel {
    match idx {
        0 => LogLevel::Error,
        1 => LogLevel::Warn,
        2 => LogLevel::Info,
        3 => LogLevel::Debug,
        _ => LogLevel::Trace,
    }
}

pub const fn default_fail_type_choice_index(fail_type: DefaultFailType) -> usize {
    match fail_type {
        DefaultFailType::Immediate => 0,
        DefaultFailType::ImmediateContinue => 1,
    }
}

pub const fn default_fail_type_from_choice(idx: usize) -> DefaultFailType {
    match idx {
        0 => DefaultFailType::Immediate,
        _ => DefaultFailType::ImmediateContinue,
    }
}

pub const fn sync_graph_mode_choice_index(mode: SyncGraphMode) -> usize {
    match mode {
        SyncGraphMode::Frequency => 0,
        SyncGraphMode::BeatIndex => 1,
        SyncGraphMode::PostKernelFingerprint => 2,
    }
}

pub const fn sync_graph_mode_from_choice(idx: usize) -> SyncGraphMode {
    match idx {
        0 => SyncGraphMode::Frequency,
        1 => SyncGraphMode::BeatIndex,
        _ => SyncGraphMode::PostKernelFingerprint,
    }
}

pub const fn machine_preferred_play_style_choice_index(style: MachinePreferredPlayStyle) -> usize {
    match style {
        MachinePreferredPlayStyle::Single => 0,
        MachinePreferredPlayStyle::Versus => 1,
        MachinePreferredPlayStyle::Double => 2,
    }
}

pub const fn machine_preferred_play_style_from_choice(idx: usize) -> MachinePreferredPlayStyle {
    match idx {
        1 => MachinePreferredPlayStyle::Versus,
        2 => MachinePreferredPlayStyle::Double,
        _ => MachinePreferredPlayStyle::Single,
    }
}

pub const fn machine_preferred_play_mode_choice_index(mode: MachinePreferredPlayMode) -> usize {
    match mode {
        MachinePreferredPlayMode::Regular => 0,
        MachinePreferredPlayMode::Marathon => 1,
    }
}

pub const fn machine_preferred_play_mode_from_choice(idx: usize) -> MachinePreferredPlayMode {
    match idx {
        1 => MachinePreferredPlayMode::Marathon,
        _ => MachinePreferredPlayMode::Regular,
    }
}

pub const fn machine_font_choice_index(font: MachineFont) -> usize {
    match font {
        MachineFont::Wendy => 0,
        MachineFont::Mega => 1,
    }
}

pub const fn machine_font_from_choice(idx: usize) -> MachineFont {
    match idx {
        1 => MachineFont::Mega,
        _ => MachineFont::Wendy,
    }
}

pub const fn machine_bar_color_choice_index(color: MachineBarColor) -> usize {
    match color {
        MachineBarColor::Default => 0,
        MachineBarColor::Colored => 1,
        MachineBarColor::Transparent => 2,
    }
}

pub const fn machine_bar_color_from_choice(idx: usize) -> MachineBarColor {
    match idx {
        1 => MachineBarColor::Colored,
        2 => MachineBarColor::Transparent,
        _ => MachineBarColor::Default,
    }
}

pub const fn machine_evaluation_style_choice_index(style: MachineEvaluationStyle) -> usize {
    match style {
        MachineEvaluationStyle::Default => 0,
        MachineEvaluationStyle::Opaque => 1,
        MachineEvaluationStyle::Transparent => 2,
    }
}

pub const fn machine_evaluation_style_from_choice(idx: usize) -> MachineEvaluationStyle {
    match idx {
        1 => MachineEvaluationStyle::Opaque,
        2 => MachineEvaluationStyle::Transparent,
        _ => MachineEvaluationStyle::Default,
    }
}

pub const fn random_background_mode_choice_index(mode: RandomBackgroundMode) -> usize {
    match mode {
        RandomBackgroundMode::Off => 0,
        RandomBackgroundMode::RandomMovies => 1,
    }
}

pub const fn random_background_mode_from_choice(idx: usize) -> RandomBackgroundMode {
    match idx {
        1 => RandomBackgroundMode::RandomMovies,
        _ => RandomBackgroundMode::Off,
    }
}

pub const fn default_sync_offset_choice_index(offset: DefaultSyncOffset) -> usize {
    match offset {
        DefaultSyncOffset::Null => 0,
        DefaultSyncOffset::Itg => 1,
    }
}

pub const fn default_sync_offset_from_choice(idx: usize) -> DefaultSyncOffset {
    match idx {
        1 => DefaultSyncOffset::Itg,
        _ => DefaultSyncOffset::Null,
    }
}

pub const fn version_overlay_side_choice_index(side: VersionOverlaySide) -> usize {
    match side {
        VersionOverlaySide::Left => 0,
        VersionOverlaySide::Right => 1,
    }
}

pub const fn version_overlay_side_from_choice(idx: usize) -> VersionOverlaySide {
    match idx {
        0 => VersionOverlaySide::Left,
        _ => VersionOverlaySide::Right,
    }
}

pub const fn visual_style_choice_index(style: VisualStyle) -> usize {
    match style {
        VisualStyle::Hearts => 0,
        VisualStyle::Arrows => 1,
        VisualStyle::Bears => 2,
        VisualStyle::Ducks => 3,
        VisualStyle::Cats => 4,
        VisualStyle::Spooky => 5,
        VisualStyle::Gay => 6,
        VisualStyle::Stars => 7,
        VisualStyle::Thonk => 8,
        VisualStyle::Technique => 9,
        VisualStyle::Srpg9 => 10,
    }
}

pub const fn visual_style_from_choice(idx: usize) -> VisualStyle {
    match idx {
        1 => VisualStyle::Arrows,
        2 => VisualStyle::Bears,
        3 => VisualStyle::Ducks,
        4 => VisualStyle::Cats,
        5 => VisualStyle::Spooky,
        6 => VisualStyle::Gay,
        7 => VisualStyle::Stars,
        8 => VisualStyle::Thonk,
        9 => VisualStyle::Technique,
        10 => VisualStyle::Srpg9,
        _ => VisualStyle::Hearts,
    }
}

pub const fn srpg_variant_choice_index(variant: SrpgVariant) -> usize {
    match variant {
        SrpgVariant::Srpg9 => 0,
        SrpgVariant::Srpg10 => 1,
    }
}

pub const fn srpg_variant_from_choice(idx: usize) -> SrpgVariant {
    match idx {
        1 => SrpgVariant::Srpg10,
        _ => SrpgVariant::Srpg9,
    }
}

pub const fn arrowcloud_qr_login_when_choice_index(when: ArrowCloudQrLoginWhen) -> usize {
    match when {
        ArrowCloudQrLoginWhen::Always => 0,
        ArrowCloudQrLoginWhen::Sometimes => 1,
        ArrowCloudQrLoginWhen::Disabled => 2,
    }
}

pub const fn arrowcloud_qr_login_when_from_choice(idx: usize) -> ArrowCloudQrLoginWhen {
    match idx {
        0 => ArrowCloudQrLoginWhen::Always,
        2 => ArrowCloudQrLoginWhen::Disabled,
        _ => ArrowCloudQrLoginWhen::Sometimes,
    }
}

pub const fn groovestats_qr_login_when_choice_index(when: GrooveStatsQrLoginWhen) -> usize {
    match when {
        GrooveStatsQrLoginWhen::Always => 0,
        GrooveStatsQrLoginWhen::Sometimes => 1,
        GrooveStatsQrLoginWhen::Disabled => 2,
    }
}

pub const fn groovestats_qr_login_when_from_choice(idx: usize) -> GrooveStatsQrLoginWhen {
    match idx {
        0 => GrooveStatsQrLoginWhen::Always,
        2 => GrooveStatsQrLoginWhen::Disabled,
        _ => GrooveStatsQrLoginWhen::Sometimes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bg_brightness_choices_round_and_clamp() {
        assert_eq!(bg_brightness_choice_index(-0.5), 0);
        assert_eq!(bg_brightness_choice_index(0.04), 0);
        assert_eq!(bg_brightness_choice_index(0.05), 1);
        assert_eq!(bg_brightness_choice_index(0.74), 7);
        assert_eq!(bg_brightness_choice_index(1.5), 10);
        assert_eq!(bg_brightness_from_choice(7), 0.7);
        assert_eq!(bg_brightness_from_choice(99), 1.0);
    }

    #[test]
    fn bg_brightness_clamps_to_unit_range() {
        assert_eq!(clamp_bg_brightness(-1.0), 0.0);
        assert_eq!(clamp_bg_brightness(0.4), 0.4);
        assert_eq!(clamp_bg_brightness(2.0), 1.0);
    }

    #[test]
    fn show_stats_mode_parses_current_key_first() {
        assert_eq!(parse_show_stats_mode(Some("2"), Some("0"), 0), 2);
        assert_eq!(parse_show_stats_mode(Some("8"), Some("0"), 0), 3);
        assert_eq!(parse_show_stats_mode(Some("bad"), Some("1"), 0), 1);
    }

    #[test]
    fn show_stats_mode_supports_legacy_bool_key() {
        assert_eq!(parse_show_stats_mode(None, Some("0"), 2), 0);
        assert_eq!(parse_show_stats_mode(None, Some("1"), 0), 1);
        assert_eq!(parse_show_stats_mode(None, Some("2"), 0), 1);
        assert_eq!(parse_show_stats_mode(None, Some("bad"), 2), 2);
    }

    #[test]
    fn show_stats_mode_clamps_for_save_and_update() {
        assert_eq!(clamp_show_stats_mode(0), 0);
        assert_eq!(clamp_show_stats_mode(3), 3);
        assert_eq!(clamp_show_stats_mode(4), 3);
    }

    #[test]
    fn select_music_itl_rank_parses_current_and_legacy_keys() {
        assert_eq!(
            parse_select_music_itl_rank_mode(
                Some("Overall"),
                Some("1"),
                SelectMusicItlRankMode::None
            ),
            SelectMusicItlRankMode::Overall
        );
        assert_eq!(
            parse_select_music_itl_rank_mode(Some("bad"), Some("1"), SelectMusicItlRankMode::None),
            SelectMusicItlRankMode::Chart
        );
        assert_eq!(
            parse_select_music_itl_rank_mode(None, Some("0"), SelectMusicItlRankMode::Overall),
            SelectMusicItlRankMode::None
        );
        assert_eq!(
            parse_select_music_itl_rank_mode(None, Some("bad"), SelectMusicItlRankMode::Overall),
            SelectMusicItlRankMode::Overall
        );
    }

    #[test]
    fn song_select_bg_parses_primary_before_legacy_key() {
        assert_eq!(
            parse_select_music_song_select_bg_mode(
                Some("BG"),
                Some("Banner"),
                SelectMusicSongSelectBgMode::Off,
            ),
            SelectMusicSongSelectBgMode::Bg
        );
        assert_eq!(
            parse_select_music_song_select_bg_mode(
                None,
                Some("Banner"),
                SelectMusicSongSelectBgMode::Off,
            ),
            SelectMusicSongSelectBgMode::Banner
        );
        assert_eq!(
            parse_select_music_song_select_bg_mode(
                Some("bad"),
                Some("Banner"),
                SelectMusicSongSelectBgMode::Bg,
            ),
            SelectMusicSongSelectBgMode::Bg
        );
    }

    #[test]
    fn music_wheel_speed_uses_nearest_choice() {
        assert_eq!(music_wheel_scroll_speed_choice_index(4), 0);
        assert_eq!(music_wheel_scroll_speed_choice_index(12), 1);
        assert_eq!(music_wheel_scroll_speed_choice_index(13), 2);
        assert_eq!(music_wheel_scroll_speed_choice_index(14), 2);
        assert_eq!(music_wheel_scroll_speed_choice_index(99), 6);
        assert_eq!(music_wheel_scroll_speed_from_choice(3), 25);
        assert_eq!(music_wheel_scroll_speed_from_choice(99), 15);
    }

    #[test]
    fn scorebox_cycle_bits_follow_choice_order() {
        assert_eq!(scorebox_cycle_mask(true, false, true, false), 0b0101);
        assert_eq!(scorebox_cycle_cursor_index(false, false, true, true), 2);
        assert_eq!(scorebox_cycle_bit_from_choice(0), 0b0001);
        assert_eq!(scorebox_cycle_bit_from_choice(3), 0b1000);
        assert_eq!(scorebox_cycle_bit_from_choice(4), 0);
    }

    #[test]
    fn auto_screenshot_cursor_uses_first_enabled_flag() {
        assert_eq!(auto_screenshot_cursor_index(0), 0);
        assert_eq!(auto_screenshot_cursor_index(AUTO_SS_CLEARS), 2);
        assert_eq!(
            auto_screenshot_cursor_index(AUTO_SS_FAILS | AUTO_SS_QUINTS),
            1
        );
        assert_eq!(auto_screenshot_bit_from_choice(4), AUTO_SS_QUINTS);
        assert_eq!(auto_screenshot_bit_from_choice(5), 0);
    }

    #[test]
    fn chart_info_bits_keep_one_visible_default() {
        assert_eq!(select_music_chart_info_mask(true, false, true), 0b101);
        assert_eq!(select_music_chart_info_cursor_index(false, true, true), 1);
        assert_eq!(select_music_chart_info_bit_from_choice(2), 0b100);
        assert_eq!(select_music_chart_info_bit_from_choice(3), 0);
        assert_eq!(select_music_chart_info_enabled_mask(0), 1);
        assert_eq!(select_music_chart_info_enabled_mask(0b110), 0b110);
    }

    #[test]
    fn max_fps_choices_are_single_fps_steps() {
        let choices = build_max_fps_choices();
        assert_eq!(choices.first().copied(), Some(MAX_FPS_MIN));
        assert_eq!(choices.get(1).copied(), Some(MAX_FPS_MIN + MAX_FPS_STEP));
        assert_eq!(choices.last().copied(), Some(MAX_FPS_MAX));
        assert_eq!(
            choices.len(),
            1 + usize::from(MAX_FPS_MAX - MAX_FPS_MIN) / usize::from(MAX_FPS_STEP)
        );
    }

    #[test]
    fn max_fps_choice_helpers_clamp_and_fallback() {
        let choices = build_max_fps_choices();
        assert_eq!(clamped_max_fps(0), MAX_FPS_MIN);
        assert_eq!(clamped_max_fps(10_000), MAX_FPS_MAX);
        assert_eq!(max_fps_choice_index(&choices, 0), 0);
        assert_eq!(
            max_fps_choice_index(&choices, 60),
            usize::from(60 - MAX_FPS_MIN)
        );
        assert_eq!(max_fps_from_choice(&choices, usize::MAX), MAX_FPS_DEFAULT);
    }

    #[test]
    fn max_fps_hold_delta_accelerates() {
        assert_eq!(max_fps_hold_delta(1, Duration::from_millis(300)), 5);
        assert_eq!(max_fps_hold_delta(1, Duration::from_millis(700)), 10);
        assert_eq!(max_fps_hold_delta(1, Duration::from_millis(1200)), 25);
        assert_eq!(max_fps_hold_delta(-1, Duration::from_millis(1800)), -50);
    }

    #[test]
    fn sync_confidence_choice_uses_five_percent_steps() {
        assert_eq!(sync_confidence_choice_index(0), 0);
        assert_eq!(sync_confidence_choice_index(2), 0);
        assert_eq!(sync_confidence_choice_index(3), 1);
        assert_eq!(sync_confidence_choice_index(98), 20);
        assert_eq!(sync_confidence_choice_index(255), 20);
        assert_eq!(sync_confidence_from_choice(0), 0);
        assert_eq!(sync_confidence_from_choice(7), 35);
        assert_eq!(sync_confidence_from_choice(99), 100);
    }

    #[test]
    fn translated_titles_choice_roundtrips() {
        assert_eq!(translated_titles_choice_index(true), 0);
        assert_eq!(translated_titles_choice_index(false), 1);
        assert!(translated_titles_from_choice(0));
        assert!(!translated_titles_from_choice(1));
        assert!(!translated_titles_from_choice(99));
    }

    #[test]
    fn language_choices_match_system_order() {
        assert_eq!(language_choice_index(LanguageFlag::Auto), 0);
        assert_eq!(language_choice_index(LanguageFlag::English), 0);
        assert_eq!(language_choice_index(LanguageFlag::German), 1);
        assert_eq!(language_choice_index(LanguageFlag::Pseudo), 10);
        assert_eq!(language_flag_from_choice(0), LanguageFlag::English);
        assert_eq!(language_flag_from_choice(7), LanguageFlag::PortugueseBrazil);
        assert_eq!(language_flag_from_choice(99), LanguageFlag::English);
    }

    #[test]
    fn select_music_choice_helpers_match_screen_order() {
        assert_eq!(breakdown_style_choice_index(BreakdownStyle::Sl), 0);
        assert_eq!(breakdown_style_choice_index(BreakdownStyle::Sn), 1);
        assert_eq!(breakdown_style_from_choice(99), BreakdownStyle::Sl);

        assert_eq!(
            select_music_pattern_info_mode_choice_index(SelectMusicPatternInfoMode::Auto),
            0
        );
        assert_eq!(
            select_music_pattern_info_mode_choice_index(SelectMusicPatternInfoMode::Tech),
            1
        );
        assert_eq!(
            select_music_pattern_info_mode_from_choice(2),
            SelectMusicPatternInfoMode::Stamina
        );
        assert_eq!(
            select_music_pattern_info_mode_from_choice(99),
            SelectMusicPatternInfoMode::Auto
        );

        assert_eq!(
            select_music_step_artist_box_mode_choice_index(SelectMusicStepArtistBoxMode::Default),
            0
        );
        assert_eq!(
            select_music_step_artist_box_mode_from_choice(1),
            SelectMusicStepArtistBoxMode::Legacy
        );
        assert_eq!(
            select_music_step_artist_box_mode_from_choice(99),
            SelectMusicStepArtistBoxMode::Default
        );

        assert_eq!(
            select_music_itl_wheel_mode_choice_index(SelectMusicItlWheelMode::Off),
            0
        );
        assert_eq!(
            select_music_itl_wheel_mode_from_choice(2),
            SelectMusicItlWheelMode::PointsAndScore
        );
        assert_eq!(
            select_music_itl_wheel_mode_from_choice(99),
            SelectMusicItlWheelMode::Off
        );

        assert_eq!(
            select_music_itl_rank_mode_choice_index(SelectMusicItlRankMode::None),
            0
        );
        assert_eq!(
            select_music_itl_rank_mode_from_choice(2),
            SelectMusicItlRankMode::Overall
        );
        assert_eq!(
            select_music_itl_rank_mode_from_choice(99),
            SelectMusicItlRankMode::None
        );

        assert_eq!(
            select_music_wheel_style_choice_index(SelectMusicWheelStyle::Itg),
            0
        );
        assert_eq!(
            select_music_wheel_style_from_choice(1),
            SelectMusicWheelStyle::Iidx
        );
        assert_eq!(
            select_music_wheel_style_from_choice(99),
            SelectMusicWheelStyle::Itg
        );

        assert_eq!(
            select_music_song_select_bg_mode_choice_index(SelectMusicSongSelectBgMode::Off),
            0
        );
        assert_eq!(
            select_music_song_select_bg_mode_from_choice(2),
            SelectMusicSongSelectBgMode::Bg
        );
        assert_eq!(
            select_music_song_select_bg_mode_from_choice(99),
            SelectMusicSongSelectBgMode::Off
        );

        assert_eq!(
            select_music_new_pack_mode_choice_index(NewPackMode::Disabled),
            0
        );
        assert_eq!(
            select_music_new_pack_mode_from_choice(2),
            NewPackMode::HasScore
        );
        assert_eq!(
            select_music_new_pack_mode_from_choice(99),
            NewPackMode::Disabled
        );

        assert_eq!(
            select_music_scorebox_placement_choice_index(SelectMusicScoreboxPlacement::Auto),
            0
        );
        assert_eq!(
            select_music_scorebox_placement_from_choice(1),
            SelectMusicScoreboxPlacement::StepPane
        );
        assert_eq!(
            select_music_scorebox_placement_from_choice(99),
            SelectMusicScoreboxPlacement::Auto
        );
    }

    #[test]
    fn machine_choice_helpers_match_screen_order() {
        assert_eq!(
            machine_preferred_play_style_choice_index(MachinePreferredPlayStyle::Single),
            0
        );
        assert_eq!(
            machine_preferred_play_style_from_choice(2),
            MachinePreferredPlayStyle::Double
        );
        assert_eq!(
            machine_preferred_play_style_from_choice(99),
            MachinePreferredPlayStyle::Single
        );

        assert_eq!(
            machine_preferred_play_mode_choice_index(MachinePreferredPlayMode::Regular),
            0
        );
        assert_eq!(
            machine_preferred_play_mode_from_choice(1),
            MachinePreferredPlayMode::Marathon
        );
        assert_eq!(
            machine_preferred_play_mode_from_choice(99),
            MachinePreferredPlayMode::Regular
        );

        assert_eq!(machine_font_choice_index(MachineFont::Wendy), 0);
        assert_eq!(machine_font_from_choice(1), MachineFont::Mega);
        assert_eq!(machine_font_from_choice(99), MachineFont::Wendy);

        assert_eq!(machine_bar_color_choice_index(MachineBarColor::Default), 0);
        assert_eq!(
            machine_bar_color_from_choice(2),
            MachineBarColor::Transparent
        );
        assert_eq!(machine_bar_color_from_choice(99), MachineBarColor::Default);

        assert_eq!(
            machine_evaluation_style_choice_index(MachineEvaluationStyle::Default),
            0
        );
        assert_eq!(
            machine_evaluation_style_from_choice(2),
            MachineEvaluationStyle::Transparent
        );
        assert_eq!(
            machine_evaluation_style_from_choice(99),
            MachineEvaluationStyle::Default
        );

        assert_eq!(
            random_background_mode_choice_index(RandomBackgroundMode::Off),
            0
        );
        assert_eq!(
            random_background_mode_from_choice(1),
            RandomBackgroundMode::RandomMovies
        );
        assert_eq!(
            random_background_mode_from_choice(99),
            RandomBackgroundMode::Off
        );

        assert_eq!(default_sync_offset_choice_index(DefaultSyncOffset::Null), 0);
        assert_eq!(default_sync_offset_from_choice(1), DefaultSyncOffset::Itg);
        assert_eq!(default_sync_offset_from_choice(99), DefaultSyncOffset::Null);

        assert_eq!(
            version_overlay_side_choice_index(VersionOverlaySide::Left),
            0
        );
        assert_eq!(
            version_overlay_side_from_choice(1),
            VersionOverlaySide::Right
        );
        assert_eq!(
            version_overlay_side_from_choice(99),
            VersionOverlaySide::Right
        );

        assert_eq!(visual_style_choice_index(VisualStyle::Hearts), 0);
        assert_eq!(visual_style_choice_index(VisualStyle::Srpg9), 10);
        assert_eq!(visual_style_from_choice(9), VisualStyle::Technique);
        assert_eq!(visual_style_from_choice(99), VisualStyle::Hearts);

        assert_eq!(srpg_variant_choice_index(SrpgVariant::Srpg9), 0);
        assert_eq!(srpg_variant_from_choice(1), SrpgVariant::Srpg10);
        assert_eq!(srpg_variant_from_choice(99), SrpgVariant::Srpg9);
    }

    #[test]
    fn system_advanced_and_online_choice_helpers_match_screen_order() {
        assert_eq!(log_level_choice_index(LogLevel::Error), 0);
        assert_eq!(log_level_choice_index(LogLevel::Trace), 4);
        assert_eq!(log_level_from_choice(0), LogLevel::Error);
        assert_eq!(log_level_from_choice(99), LogLevel::Trace);

        assert_eq!(
            default_fail_type_choice_index(DefaultFailType::Immediate),
            0
        );
        assert_eq!(
            default_fail_type_from_choice(1),
            DefaultFailType::ImmediateContinue
        );
        assert_eq!(
            default_fail_type_from_choice(99),
            DefaultFailType::ImmediateContinue
        );

        assert_eq!(sync_graph_mode_choice_index(SyncGraphMode::Frequency), 0);
        assert_eq!(sync_graph_mode_from_choice(1), SyncGraphMode::BeatIndex);
        assert_eq!(
            sync_graph_mode_from_choice(99),
            SyncGraphMode::PostKernelFingerprint
        );

        assert_eq!(
            arrowcloud_qr_login_when_choice_index(ArrowCloudQrLoginWhen::Always),
            0
        );
        assert_eq!(
            arrowcloud_qr_login_when_from_choice(2),
            ArrowCloudQrLoginWhen::Disabled
        );
        assert_eq!(
            arrowcloud_qr_login_when_from_choice(99),
            ArrowCloudQrLoginWhen::Sometimes
        );

        assert_eq!(
            groovestats_qr_login_when_choice_index(GrooveStatsQrLoginWhen::Always),
            0
        );
        assert_eq!(
            groovestats_qr_login_when_from_choice(2),
            GrooveStatsQrLoginWhen::Disabled
        );
        assert_eq!(
            groovestats_qr_login_when_from_choice(99),
            GrooveStatsQrLoginWhen::Sometimes
        );
    }
}
