use crate::bools::{parse_bool_str, parse_loose_bool_str, parse_u8_bool_or_default};
use crate::ini::SimpleIni;
use crate::machine::{canonical_frame_stats_overlay_anchor, canonical_frame_stats_overlay_style};
use crate::numbers::parse_auto_threads_u8;
use crate::theme::{
    AUTO_SS_CLEARS, AUTO_SS_FAILS, AUTO_SS_PBS, AUTO_SS_QUADS, AUTO_SS_QUINTS,
    ArrowCloudQrLoginWhen, BreakdownStyle, DefaultFailType, DefaultSyncOffset, GameFlag,
    GrooveStatsQrLoginWhen, LanguageFlag, LogLevel, MachineBarColor, MachineEvaluationStyle,
    MachineFont, MachinePreferredPlayMode, MachinePreferredPlayStyle, NewPackMode,
    RandomBackgroundMode, SelectMusicItlRankMode, SelectMusicItlWheelMode,
    SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement, SelectMusicSongSelectBgMode,
    SelectMusicStepArtistBoxMode, SelectMusicWheelStyle, SrpgVariant, SyncGraphMode, ThemeFlag,
    VersionOverlaySide, VisualStyle, auto_screenshot_bit, auto_screenshot_mask_from_str,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemOptions {
    pub game_flag: GameFlag,
    pub auto_download_unlocks: bool,
    pub auto_populate_gs_scores: bool,
    pub updater_install_enabled: bool,
    pub enable_groovestats: bool,
    pub enable_arrowcloud: bool,
    pub enable_boogiestats: bool,
    pub submit_arrowcloud_fails: bool,
    pub arrowcloud_qr_login_when: ArrowCloudQrLoginWhen,
    pub groovestats_qr_login_when: GrooveStatsQrLoginWhen,
    pub separate_unlocks_by_player: bool,
    pub mine_hit_sound: bool,
    pub show_stats_mode: u8,
    pub frame_stats_overlay_anchor: &'static str,
    pub frame_stats_overlay_style: &'static str,
    pub translated_titles: bool,
    pub bg_brightness: f32,
    pub center_1player_notefield: bool,
    pub center_image_translate_x: i32,
    pub center_image_translate_y: i32,
    pub center_image_add_width: i32,
    pub center_image_add_height: i32,
    pub autosubmit_course_scores_individually: bool,
    pub show_course_individual_scores: bool,
    pub show_most_played_courses: bool,
    pub show_random_courses: bool,
    pub default_fail_type: DefaultFailType,
    pub banner_cache: bool,
    pub cdtitle_cache: bool,
    pub high_dpi: bool,
    pub hide_mouse_cursor: bool,
    pub allow_shutdown_host: bool,
    pub smx_input: bool,
    pub smx_manages_pad_config: bool,
    pub smx_panel_lights: bool,
    pub smx_underglow_theme: bool,
    pub gfx_debug: bool,
    pub global_offset_seconds: f32,
    pub language_flag: LanguageFlag,
    pub log_level: LogLevel,
    pub log_to_file: bool,
    pub show_console: bool,
}

pub fn load_system_options(conf: &SimpleIni, default: SystemOptions) -> SystemOptions {
    let show_stats_mode = conf.get("Options", "ShowStatsMode");
    let show_stats_legacy = conf.get("Options", "ShowStats");

    SystemOptions {
        game_flag: conf
            .get("Options", "Game")
            .and_then(|value| GameFlag::from_str(&value).ok())
            .unwrap_or(default.game_flag),
        auto_download_unlocks: parse_u8_bool_or_default(
            conf.get("Options", "AutoDownloadUnlocks").as_deref(),
            default.auto_download_unlocks,
        ),
        auto_populate_gs_scores: parse_u8_bool_or_default(
            conf.get("Options", "AutoPopulateGrooveStatsScores")
                .as_deref(),
            default.auto_populate_gs_scores,
        ),
        updater_install_enabled: parse_u8_bool_or_default(
            conf.get("Options", "UpdaterInstallEnabled").as_deref(),
            default.updater_install_enabled,
        ),
        enable_groovestats: parse_u8_bool_or_default(
            conf.get("Options", "EnableGrooveStats").as_deref(),
            default.enable_groovestats,
        ),
        enable_arrowcloud: parse_u8_bool_or_default(
            conf.get("Options", "EnableArrowCloud").as_deref(),
            default.enable_arrowcloud,
        ),
        enable_boogiestats: parse_u8_bool_or_default(
            conf.get("Options", "EnableBoogieStats").as_deref(),
            default.enable_boogiestats,
        ),
        submit_arrowcloud_fails: parse_u8_bool_or_default(
            conf.get("Options", "SubmitArrowCloudFails").as_deref(),
            default.submit_arrowcloud_fails,
        ),
        arrowcloud_qr_login_when: conf
            .get("Options", "ArrowCloudQrLoginWhen")
            .and_then(|value| ArrowCloudQrLoginWhen::from_str(&value).ok())
            .unwrap_or(default.arrowcloud_qr_login_when),
        groovestats_qr_login_when: conf
            .get("Options", "GrooveStatsQrLoginWhen")
            .and_then(|value| GrooveStatsQrLoginWhen::from_str(&value).ok())
            .unwrap_or(default.groovestats_qr_login_when),
        separate_unlocks_by_player: parse_u8_bool_or_default(
            conf.get("Options", "SeparateUnlocksByPlayer").as_deref(),
            default.separate_unlocks_by_player,
        ),
        mine_hit_sound: parse_u8_bool_or_default(
            conf.get("Options", "MineHitSound").as_deref(),
            default.mine_hit_sound,
        ),
        show_stats_mode: parse_show_stats_mode(
            show_stats_mode.as_deref(),
            show_stats_legacy.as_deref(),
            default.show_stats_mode,
        ),
        frame_stats_overlay_anchor: conf
            .get("Options", "FrameStatsOverlayAnchor")
            .map(|value| canonical_frame_stats_overlay_anchor(&value))
            .unwrap_or(default.frame_stats_overlay_anchor),
        frame_stats_overlay_style: conf
            .get("Options", "FrameStatsOverlayStyle")
            .map(|value| canonical_frame_stats_overlay_style(&value))
            .unwrap_or(default.frame_stats_overlay_style),
        translated_titles: conf
            .get("Options", "TranslatedTitles")
            .or_else(|| conf.get("Options", "translatedtitles"))
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.translated_titles),
        bg_brightness: conf
            .get("Options", "BGBrightness")
            .and_then(|value| value.parse::<f32>().ok())
            .map_or(default.bg_brightness, clamp_bg_brightness),
        center_1player_notefield: conf
            .get("Options", "Center1Player")
            .or_else(|| conf.get("Options", "CenteredP1Notefield"))
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.center_1player_notefield),
        center_image_translate_x: conf
            .get("Options", "CenterImageTranslateX")
            .and_then(|value| value.trim().parse::<i32>().ok())
            .unwrap_or(default.center_image_translate_x),
        center_image_translate_y: conf
            .get("Options", "CenterImageTranslateY")
            .and_then(|value| value.trim().parse::<i32>().ok())
            .unwrap_or(default.center_image_translate_y),
        center_image_add_width: conf
            .get("Options", "CenterImageAddWidth")
            .and_then(|value| value.trim().parse::<i32>().ok())
            .unwrap_or(default.center_image_add_width),
        center_image_add_height: conf
            .get("Options", "CenterImageAddHeight")
            .and_then(|value| value.trim().parse::<i32>().ok())
            .unwrap_or(default.center_image_add_height),
        autosubmit_course_scores_individually: parse_u8_bool_or_default(
            conf.get("Options", "CourseAutosubmitScoresIndividually")
                .as_deref(),
            default.autosubmit_course_scores_individually,
        ),
        show_course_individual_scores: parse_u8_bool_or_default(
            conf.get("Options", "CourseShowIndividualScores").as_deref(),
            default.show_course_individual_scores,
        ),
        show_most_played_courses: parse_u8_bool_or_default(
            conf.get("Options", "CourseShowMostPlayed").as_deref(),
            default.show_most_played_courses,
        ),
        show_random_courses: parse_u8_bool_or_default(
            conf.get("Options", "CourseShowRandom").as_deref(),
            default.show_random_courses,
        ),
        default_fail_type: conf
            .get("Options", "DefaultFailType")
            .and_then(|value| DefaultFailType::from_str(&value).ok())
            .unwrap_or(default.default_fail_type),
        banner_cache: parse_u8_bool_or_default(
            conf.get("Options", "BannerCache").as_deref(),
            default.banner_cache,
        ),
        cdtitle_cache: parse_u8_bool_or_default(
            conf.get("Options", "CDTitleCache").as_deref(),
            default.cdtitle_cache,
        ),
        high_dpi: conf
            .get("Options", "HighDPI")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.high_dpi),
        hide_mouse_cursor: conf
            .get("Options", "HideMouseCursor")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.hide_mouse_cursor),
        allow_shutdown_host: conf
            .get("Options", "AllowShutdown")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.allow_shutdown_host),
        smx_input: parse_u8_bool_or_default(
            conf.get("Options", "SmxInput").as_deref(),
            default.smx_input,
        ),
        smx_manages_pad_config: parse_u8_bool_or_default(
            conf.get("Options", "SmxManagesPadConfig").as_deref(),
            default.smx_manages_pad_config,
        ),
        smx_panel_lights: conf
            .get("Options", "SmxPanelLights")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.smx_panel_lights),
        smx_underglow_theme: conf
            .get("Options", "SmxUnderglowTheme")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.smx_underglow_theme),
        gfx_debug: parse_u8_bool_or_default(
            conf.get("Options", "GfxDebug").as_deref(),
            default.gfx_debug,
        ),
        global_offset_seconds: conf
            .get("Options", "GlobalOffsetSeconds")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(default.global_offset_seconds),
        language_flag: conf
            .get("Options", "Language")
            .and_then(|value| LanguageFlag::from_str(&value).ok())
            .unwrap_or(default.language_flag),
        log_level: conf
            .get("Options", "LogLevel")
            .and_then(|value| LogLevel::from_str(&value).ok())
            .unwrap_or(default.log_level),
        log_to_file: conf
            .get("Options", "LogToFile")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.log_to_file),
        show_console: conf
            .get("Options", "ShowConsole")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.show_console),
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectMusicOptions {
    pub breakdown_style: BreakdownStyle,
    pub show_banners: bool,
    pub show_version_overlay: bool,
    pub version_overlay_side: VersionOverlaySide,
    pub show_video_banners: bool,
    pub show_breakdown: bool,
    pub show_stage_display: bool,
    pub show_cdtitles: bool,
    pub show_wheel_grades: bool,
    pub show_wheel_lamps: bool,
    pub itl_rank_mode: SelectMusicItlRankMode,
    pub itl_wheel_mode: SelectMusicItlWheelMode,
    pub wheel_style: SelectMusicWheelStyle,
    pub song_select_bg_mode: SelectMusicSongSelectBgMode,
    pub new_pack_mode: NewPackMode,
    pub show_folder_stats: bool,
    pub show_previews: bool,
    pub show_preview_marker: bool,
    pub preview_loop: bool,
    pub pattern_info_mode: SelectMusicPatternInfoMode,
    pub step_artist_box_mode: SelectMusicStepArtistBoxMode,
    pub show_scorebox: bool,
    pub scorebox_placement: SelectMusicScoreboxPlacement,
    pub scorebox_cycle_itg: bool,
    pub scorebox_cycle_ex: bool,
    pub scorebox_cycle_hard_ex: bool,
    pub scorebox_cycle_tournaments: bool,
    pub chart_info_peak_nps: bool,
    pub chart_info_effective_bpm: bool,
    pub chart_info_matrix_rating: bool,
    pub auto_screenshot_eval: u8,
}

pub fn load_select_music_options(
    conf: &SimpleIni,
    default: SelectMusicOptions,
) -> SelectMusicOptions {
    let itl_rank_mode = conf.get("Options", "SelectMusicWheelITLRank");
    let legacy_itl_chart_rank = conf.get("Options", "SelectMusicShowITLChartRank");
    let song_select_bg = conf.get("Options", "SongSelectBG");
    let legacy_song_select_bg = conf.get("Options", "SelectMusicSongSelectBG");

    SelectMusicOptions {
        breakdown_style: conf
            .get("Options", "SelectMusicBreakdown")
            .and_then(|value| BreakdownStyle::from_str(&value).ok())
            .unwrap_or(default.breakdown_style),
        show_banners: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicShowBanners").as_deref(),
            default.show_banners,
        ),
        show_version_overlay: parse_u8_bool_or_default(
            conf.get("Options", "ShowVersionOverlay").as_deref(),
            default.show_version_overlay,
        ),
        version_overlay_side: conf
            .get("Options", "VersionOverlaySide")
            .and_then(|value| VersionOverlaySide::from_str(&value).ok())
            .unwrap_or(default.version_overlay_side),
        show_video_banners: conf
            .get("Options", "SelectMusicShowVideoBanners")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.show_video_banners),
        show_breakdown: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicShowBreakdown").as_deref(),
            default.show_breakdown,
        ),
        show_stage_display: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicShowStageDisplay")
                .as_deref(),
            default.show_stage_display,
        ),
        show_cdtitles: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicShowCDTitles").as_deref(),
            default.show_cdtitles,
        ),
        show_wheel_grades: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicWheelGrades").as_deref(),
            default.show_wheel_grades,
        ),
        show_wheel_lamps: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicWheelLamps").as_deref(),
            default.show_wheel_lamps,
        ),
        itl_rank_mode: parse_select_music_itl_rank_mode(
            itl_rank_mode.as_deref(),
            legacy_itl_chart_rank.as_deref(),
            default.itl_rank_mode,
        ),
        itl_wheel_mode: conf
            .get("Options", "SelectMusicWheelITL")
            .and_then(|value| SelectMusicItlWheelMode::from_str(&value).ok())
            .unwrap_or(default.itl_wheel_mode),
        wheel_style: conf
            .get("Options", "SelectMusicWheelStyle")
            .and_then(|value| SelectMusicWheelStyle::from_str(&value).ok())
            .unwrap_or(default.wheel_style),
        song_select_bg_mode: parse_select_music_song_select_bg_mode(
            song_select_bg.as_deref(),
            legacy_song_select_bg.as_deref(),
            default.song_select_bg_mode,
        ),
        new_pack_mode: conf
            .get("Options", "SelectMusicNewPackMode")
            .and_then(|value| NewPackMode::from_str(&value).ok())
            .unwrap_or(default.new_pack_mode),
        show_folder_stats: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicFolderStats").as_deref(),
            default.show_folder_stats,
        ),
        show_previews: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicPreviews").as_deref(),
            default.show_previews,
        ),
        show_preview_marker: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicPreviewMarker").as_deref(),
            default.show_preview_marker,
        ),
        preview_loop: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicPreviewLoop").as_deref(),
            default.preview_loop,
        ),
        pattern_info_mode: conf
            .get("Options", "SelectMusicPatternInfo")
            .and_then(|value| SelectMusicPatternInfoMode::from_str(&value).ok())
            .unwrap_or(default.pattern_info_mode),
        step_artist_box_mode: conf
            .get("Options", "SelectMusicStepArtistBox")
            .and_then(|value| SelectMusicStepArtistBoxMode::from_str(&value).ok())
            .unwrap_or(default.step_artist_box_mode),
        show_scorebox: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicScorebox").as_deref(),
            default.show_scorebox,
        ),
        scorebox_placement: conf
            .get("Options", "SelectMusicScoreboxPlacement")
            .and_then(|value| SelectMusicScoreboxPlacement::from_str(&value).ok())
            .unwrap_or(default.scorebox_placement),
        scorebox_cycle_itg: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicScoreboxCycleItg")
                .as_deref(),
            default.scorebox_cycle_itg,
        ),
        scorebox_cycle_ex: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicScoreboxCycleEx").as_deref(),
            default.scorebox_cycle_ex,
        ),
        scorebox_cycle_hard_ex: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicScoreboxCycleHardEx")
                .as_deref(),
            default.scorebox_cycle_hard_ex,
        ),
        scorebox_cycle_tournaments: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicScoreboxCycleTournaments")
                .as_deref(),
            default.scorebox_cycle_tournaments,
        ),
        chart_info_peak_nps: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicChartInfoPeakNps")
                .as_deref(),
            default.chart_info_peak_nps,
        ),
        chart_info_effective_bpm: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicChartInfoEffectiveBpm")
                .as_deref(),
            default.chart_info_effective_bpm,
        ),
        chart_info_matrix_rating: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicChartInfoMatrixRating")
                .as_deref(),
            default.chart_info_matrix_rating,
        ),
        auto_screenshot_eval: conf
            .get("Options", "AutoScreenshotEval")
            .map(|value| auto_screenshot_mask_from_str(&value))
            .unwrap_or(default.auto_screenshot_eval),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeOptions {
    pub fastload: bool,
    pub cachesongs: bool,
    pub song_parsing_threads: u8,
    pub smooth_histogram: bool,
    pub shade_scatterplot_judgments: bool,
    pub arcade_options_navigation: bool,
    pub delayed_back: bool,
    pub three_key_navigation: bool,
    pub use_fsrs: bool,
    pub lights_simplify_bass: bool,
    pub only_dedicated_menu_buttons: bool,
    pub theme_flag: ThemeFlag,
    pub software_renderer_threads: u8,
}

pub fn load_runtime_options(conf: &SimpleIni, default: RuntimeOptions) -> RuntimeOptions {
    RuntimeOptions {
        fastload: parse_u8_bool_or_default(
            conf.get("Options", "FastLoad").as_deref(),
            default.fastload,
        ),
        cachesongs: parse_u8_bool_or_default(
            conf.get("Options", "CacheSongs").as_deref(),
            default.cachesongs,
        ),
        song_parsing_threads: conf
            .get("Options", "SongParsingThreads")
            .and_then(|value| parse_auto_threads_u8(&value))
            .unwrap_or(default.song_parsing_threads),
        smooth_histogram: parse_u8_bool_or_default(
            conf.get("Options", "SmoothHistogram").as_deref(),
            default.smooth_histogram,
        ),
        shade_scatterplot_judgments: parse_u8_bool_or_default(
            conf.get("Options", "ShadeScatterplotJudgments").as_deref(),
            default.shade_scatterplot_judgments,
        ),
        arcade_options_navigation: conf
            .get("Options", "ArcadeOptionsNavigation")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.arcade_options_navigation),
        delayed_back: conf
            .get("Options", "DelayedBack")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.delayed_back),
        three_key_navigation: conf
            .get("Options", "ThreeKeyNavigation")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.three_key_navigation),
        use_fsrs: conf
            .get("Options", "UseFSRs")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.use_fsrs),
        lights_simplify_bass: conf
            .get("Options", "LightsSimplifyBass")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.lights_simplify_bass),
        only_dedicated_menu_buttons: parse_u8_bool_or_default(
            conf.get("Options", "OnlyDedicatedMenuButtons").as_deref(),
            default.only_dedicated_menu_buttons,
        ),
        theme_flag: conf
            .get("Options", "Theme")
            .and_then(|value| ThemeFlag::from_str(&value).ok())
            .unwrap_or(default.theme_flag),
        software_renderer_threads: conf
            .get("Options", "SoftwareRendererThreads")
            .and_then(|value| parse_auto_threads_u8(&value))
            .unwrap_or(default.software_renderer_threads),
    }
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

    fn default_select_music_options() -> SelectMusicOptions {
        SelectMusicOptions {
            breakdown_style: BreakdownStyle::Sl,
            show_banners: true,
            show_version_overlay: false,
            version_overlay_side: VersionOverlaySide::Right,
            show_video_banners: false,
            show_breakdown: true,
            show_stage_display: false,
            show_cdtitles: true,
            show_wheel_grades: true,
            show_wheel_lamps: false,
            itl_rank_mode: SelectMusicItlRankMode::None,
            itl_wheel_mode: SelectMusicItlWheelMode::Off,
            wheel_style: SelectMusicWheelStyle::Itg,
            song_select_bg_mode: SelectMusicSongSelectBgMode::Off,
            new_pack_mode: NewPackMode::Disabled,
            show_folder_stats: false,
            show_previews: true,
            show_preview_marker: true,
            preview_loop: false,
            pattern_info_mode: SelectMusicPatternInfoMode::Auto,
            step_artist_box_mode: SelectMusicStepArtistBoxMode::Default,
            show_scorebox: false,
            scorebox_placement: SelectMusicScoreboxPlacement::Auto,
            scorebox_cycle_itg: true,
            scorebox_cycle_ex: false,
            scorebox_cycle_hard_ex: false,
            scorebox_cycle_tournaments: false,
            chart_info_peak_nps: true,
            chart_info_effective_bpm: true,
            chart_info_matrix_rating: false,
            auto_screenshot_eval: AUTO_SS_PBS,
        }
    }

    fn default_system_options() -> SystemOptions {
        SystemOptions {
            game_flag: GameFlag::Dance,
            auto_download_unlocks: false,
            auto_populate_gs_scores: false,
            updater_install_enabled: true,
            enable_groovestats: false,
            enable_arrowcloud: false,
            enable_boogiestats: false,
            submit_arrowcloud_fails: false,
            arrowcloud_qr_login_when: ArrowCloudQrLoginWhen::Sometimes,
            groovestats_qr_login_when: GrooveStatsQrLoginWhen::Sometimes,
            separate_unlocks_by_player: false,
            mine_hit_sound: true,
            show_stats_mode: 0,
            frame_stats_overlay_anchor: "auto",
            frame_stats_overlay_style: "detailed",
            translated_titles: false,
            bg_brightness: 0.7,
            center_1player_notefield: false,
            center_image_translate_x: 0,
            center_image_translate_y: 0,
            center_image_add_width: 0,
            center_image_add_height: 0,
            autosubmit_course_scores_individually: true,
            show_course_individual_scores: true,
            show_most_played_courses: true,
            show_random_courses: true,
            default_fail_type: DefaultFailType::ImmediateContinue,
            banner_cache: true,
            cdtitle_cache: true,
            high_dpi: false,
            hide_mouse_cursor: true,
            allow_shutdown_host: false,
            smx_input: false,
            smx_manages_pad_config: false,
            smx_panel_lights: false,
            smx_underglow_theme: false,
            gfx_debug: false,
            global_offset_seconds: -0.008,
            language_flag: LanguageFlag::Auto,
            log_level: LogLevel::Warn,
            log_to_file: true,
            show_console: false,
        }
    }

    fn default_runtime_options() -> RuntimeOptions {
        RuntimeOptions {
            fastload: true,
            cachesongs: true,
            song_parsing_threads: 0,
            smooth_histogram: true,
            shade_scatterplot_judgments: false,
            arcade_options_navigation: false,
            delayed_back: true,
            three_key_navigation: false,
            use_fsrs: false,
            lights_simplify_bass: false,
            only_dedicated_menu_buttons: false,
            theme_flag: ThemeFlag::SimplyLove,
            software_renderer_threads: 1,
        }
    }

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
    fn loads_system_options_from_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            Game=dance
            AutoDownloadUnlocks=1
            AutoPopulateGrooveStatsScores=1
            UpdaterInstallEnabled=0
            EnableGrooveStats=1
            EnableArrowCloud=1
            EnableBoogieStats=1
            SubmitArrowCloudFails=1
            ArrowCloudQrLoginWhen=Always
            GrooveStatsQrLoginWhen=Disabled
            SeparateUnlocksByPlayer=1
            MineHitSound=0
            ShowStatsMode=9
            FrameStatsOverlayAnchor=bottom-center
            FrameStatsOverlayStyle=minimal
            TranslatedTitles=false
            BGBrightness=2.0
            Center1Player=1
            CenterImageTranslateX=-12
            CenterImageTranslateY=9
            CenterImageAddWidth=20
            CenterImageAddHeight=-4
            CourseAutosubmitScoresIndividually=0
            CourseShowIndividualScores=0
            CourseShowMostPlayed=0
            CourseShowRandom=0
            DefaultFailType=Immediate
            BannerCache=0
            CDTitleCache=0
            HighDPI=1
            HideMouseCursor=0
            AllowShutdown=1
            SmxInput=1
            SmxManagesPadConfig=1
            SmxPanelLights=1
            SmxUnderglowTheme=1
            GfxDebug=1
            GlobalOffsetSeconds=0.125
            Language=Japanese
            LogLevel=Trace
            LogToFile=false
            ShowConsole=true
            "#,
        );

        let loaded = load_system_options(&conf, default_system_options());

        assert_eq!(loaded.game_flag, GameFlag::Dance);
        assert!(loaded.auto_download_unlocks);
        assert!(loaded.auto_populate_gs_scores);
        assert!(!loaded.updater_install_enabled);
        assert!(loaded.enable_groovestats);
        assert!(loaded.enable_arrowcloud);
        assert!(loaded.enable_boogiestats);
        assert!(loaded.submit_arrowcloud_fails);
        assert_eq!(
            loaded.arrowcloud_qr_login_when,
            ArrowCloudQrLoginWhen::Always
        );
        assert_eq!(
            loaded.groovestats_qr_login_when,
            GrooveStatsQrLoginWhen::Disabled
        );
        assert!(loaded.separate_unlocks_by_player);
        assert!(!loaded.mine_hit_sound);
        assert_eq!(loaded.show_stats_mode, SHOW_STATS_MODE_MAX);
        assert_eq!(loaded.frame_stats_overlay_anchor, "bottom-center");
        assert_eq!(loaded.frame_stats_overlay_style, "minimal");
        assert!(!loaded.translated_titles);
        assert_eq!(loaded.bg_brightness, 1.0);
        assert!(loaded.center_1player_notefield);
        assert_eq!(loaded.center_image_translate_x, -12);
        assert_eq!(loaded.center_image_translate_y, 9);
        assert_eq!(loaded.center_image_add_width, 20);
        assert_eq!(loaded.center_image_add_height, -4);
        assert!(!loaded.autosubmit_course_scores_individually);
        assert!(!loaded.show_course_individual_scores);
        assert!(!loaded.show_most_played_courses);
        assert!(!loaded.show_random_courses);
        assert_eq!(loaded.default_fail_type, DefaultFailType::Immediate);
        assert!(!loaded.banner_cache);
        assert!(!loaded.cdtitle_cache);
        assert!(loaded.high_dpi);
        assert!(!loaded.hide_mouse_cursor);
        assert!(loaded.allow_shutdown_host);
        assert!(loaded.smx_input);
        assert!(loaded.smx_manages_pad_config);
        assert!(loaded.smx_panel_lights);
        assert!(loaded.smx_underglow_theme);
        assert!(loaded.gfx_debug);
        assert_eq!(loaded.global_offset_seconds, 0.125);
        assert_eq!(loaded.language_flag, LanguageFlag::Japanese);
        assert_eq!(loaded.log_level, LogLevel::Trace);
        assert!(!loaded.log_to_file);
        assert!(loaded.show_console);
    }

    #[test]
    fn load_system_options_uses_legacy_keys_and_defaults() {
        let default = default_system_options();
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            ShowStats=1
            translatedtitles=1
            CenteredP1Notefield=1
            FrameStatsOverlayAnchor=middle
            FrameStatsOverlayStyle=compact
            DefaultFailType=bad
            Language=bad
            LogLevel=bad
            LogToFile=bad
            ShowConsole=bad
            "#,
        );

        let loaded = load_system_options(&conf, default);

        assert_eq!(loaded.show_stats_mode, 1);
        assert!(loaded.translated_titles);
        assert!(loaded.center_1player_notefield);
        assert_eq!(loaded.frame_stats_overlay_anchor, "auto");
        assert_eq!(loaded.frame_stats_overlay_style, "detailed");
        assert_eq!(loaded.default_fail_type, default.default_fail_type);
        assert_eq!(loaded.language_flag, default.language_flag);
        assert_eq!(loaded.log_level, default.log_level);
        assert_eq!(loaded.log_to_file, default.log_to_file);
        assert_eq!(loaded.show_console, default.show_console);
    }

    #[test]
    fn loads_runtime_options_from_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            FastLoad=0
            CacheSongs=0
            SongParsingThreads=4
            SmoothHistogram=0
            ShadeScatterplotJudgments=1
            ArcadeOptionsNavigation=1
            DelayedBack=0
            ThreeKeyNavigation=1
            UseFSRs=1
            LightsSimplifyBass=1
            OnlyDedicatedMenuButtons=1
            Theme=Simply Love
            SoftwareRendererThreads=auto
            "#,
        );

        let loaded = load_runtime_options(&conf, default_runtime_options());

        assert!(!loaded.fastload);
        assert!(!loaded.cachesongs);
        assert_eq!(loaded.song_parsing_threads, 4);
        assert!(!loaded.smooth_histogram);
        assert!(loaded.shade_scatterplot_judgments);
        assert!(loaded.arcade_options_navigation);
        assert!(!loaded.delayed_back);
        assert!(loaded.three_key_navigation);
        assert!(loaded.use_fsrs);
        assert!(loaded.lights_simplify_bass);
        assert!(loaded.only_dedicated_menu_buttons);
        assert_eq!(loaded.theme_flag, ThemeFlag::SimplyLove);
        assert_eq!(loaded.software_renderer_threads, 0);
    }

    #[test]
    fn load_runtime_options_keeps_defaults_for_bad_values() {
        let default = default_runtime_options();
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            FastLoad=bad
            CacheSongs=bad
            SongParsingThreads=many
            SmoothHistogram=bad
            ShadeScatterplotJudgments=bad
            ArcadeOptionsNavigation=bad
            DelayedBack=bad
            ThreeKeyNavigation=bad
            UseFSRs=bad
            LightsSimplifyBass=bad
            OnlyDedicatedMenuButtons=bad
            Theme=bad
            SoftwareRendererThreads=many
            "#,
        );

        let loaded = load_runtime_options(&conf, default);

        assert_eq!(loaded, default);
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
    fn loads_select_music_options_from_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            SelectMusicBreakdown=SN
            SelectMusicShowBanners=0
            ShowVersionOverlay=1
            VersionOverlaySide=Left
            SelectMusicShowVideoBanners=true
            SelectMusicShowBreakdown=0
            SelectMusicShowStageDisplay=1
            SelectMusicShowCDTitles=0
            SelectMusicWheelGrades=0
            SelectMusicWheelLamps=1
            SelectMusicWheelITLRank=Overall
            SelectMusicWheelITL=Points
            SelectMusicWheelStyle=IIDX
            SongSelectBG=BG
            SelectMusicNewPackMode=OpenPack
            SelectMusicFolderStats=1
            SelectMusicPreviews=0
            SelectMusicPreviewMarker=0
            SelectMusicPreviewLoop=1
            SelectMusicPatternInfo=Stamina
            SelectMusicStepArtistBox=Expanded
            SelectMusicScorebox=1
            SelectMusicScoreboxPlacement=StepPane
            SelectMusicScoreboxCycleItg=0
            SelectMusicScoreboxCycleEx=1
            SelectMusicScoreboxCycleHardEx=1
            SelectMusicScoreboxCycleTournaments=1
            SelectMusicChartInfoPeakNps=0
            SelectMusicChartInfoEffectiveBpm=0
            SelectMusicChartInfoMatrixRating=1
            AutoScreenshotEval=Fails|Quints
            "#,
        );

        let loaded = load_select_music_options(&conf, default_select_music_options());

        assert_eq!(loaded.breakdown_style, BreakdownStyle::Sn);
        assert!(!loaded.show_banners);
        assert!(loaded.show_version_overlay);
        assert_eq!(loaded.version_overlay_side, VersionOverlaySide::Left);
        assert!(loaded.show_video_banners);
        assert!(!loaded.show_breakdown);
        assert!(loaded.show_stage_display);
        assert!(!loaded.show_cdtitles);
        assert!(!loaded.show_wheel_grades);
        assert!(loaded.show_wheel_lamps);
        assert_eq!(loaded.itl_rank_mode, SelectMusicItlRankMode::Overall);
        assert_eq!(
            loaded.itl_wheel_mode,
            SelectMusicItlWheelMode::PointsAndScore
        );
        assert_eq!(loaded.wheel_style, SelectMusicWheelStyle::Iidx);
        assert_eq!(loaded.song_select_bg_mode, SelectMusicSongSelectBgMode::Bg);
        assert_eq!(loaded.new_pack_mode, NewPackMode::OpenPack);
        assert!(loaded.show_folder_stats);
        assert!(!loaded.show_previews);
        assert!(!loaded.show_preview_marker);
        assert!(loaded.preview_loop);
        assert_eq!(
            loaded.pattern_info_mode,
            SelectMusicPatternInfoMode::Stamina
        );
        assert_eq!(
            loaded.step_artist_box_mode,
            SelectMusicStepArtistBoxMode::Expanded
        );
        assert!(loaded.show_scorebox);
        assert_eq!(
            loaded.scorebox_placement,
            SelectMusicScoreboxPlacement::StepPane
        );
        assert!(!loaded.scorebox_cycle_itg);
        assert!(loaded.scorebox_cycle_ex);
        assert!(loaded.scorebox_cycle_hard_ex);
        assert!(loaded.scorebox_cycle_tournaments);
        assert!(!loaded.chart_info_peak_nps);
        assert!(!loaded.chart_info_effective_bpm);
        assert!(loaded.chart_info_matrix_rating);
        assert_eq!(loaded.auto_screenshot_eval, AUTO_SS_FAILS | AUTO_SS_QUINTS);
    }

    #[test]
    fn load_select_music_options_uses_legacy_keys_and_defaults() {
        let default = default_select_music_options();
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            SelectMusicShowITLChartRank=1
            SelectMusicSongSelectBG=Banner
            SelectMusicBreakdown=bad
            SelectMusicShowVideoBanners=bad
            "#,
        );

        let loaded = load_select_music_options(&conf, default);

        assert_eq!(loaded.itl_rank_mode, SelectMusicItlRankMode::Chart);
        assert_eq!(
            loaded.song_select_bg_mode,
            SelectMusicSongSelectBgMode::Banner
        );
        assert_eq!(loaded.breakdown_style, default.breakdown_style);
        assert_eq!(loaded.show_video_banners, default.show_video_banners);
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
