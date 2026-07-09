use crate::bools::{parse_bool_str, parse_loose_bool_str, parse_u8_bool_or_default};
use crate::defaults::{
    DEFAULT_ALLOW_SHUTDOWN_HOST, DEFAULT_ARCADE_OPTIONS_NAVIGATION, DEFAULT_AUTO_DOWNLOAD_UNLOCKS,
    DEFAULT_AUTO_POPULATE_GS_SCORES, DEFAULT_AUTO_SCREENSHOT_EVAL,
    DEFAULT_AUTOSUBMIT_COURSE_SCORES_INDIVIDUALLY, DEFAULT_BANNER_CACHE, DEFAULT_BG_BRIGHTNESS,
    DEFAULT_CACHE_SONGS, DEFAULT_CDTITLE_CACHE, DEFAULT_CENTER_1PLAYER_NOTEFIELD,
    DEFAULT_CENTER_IMAGE_ADD_HEIGHT, DEFAULT_CENTER_IMAGE_ADD_WIDTH,
    DEFAULT_CENTER_IMAGE_TRANSLATE_X, DEFAULT_CENTER_IMAGE_TRANSLATE_Y, DEFAULT_DELAYED_BACK,
    DEFAULT_ENABLE_ARROWCLOUD, DEFAULT_ENABLE_BOOGIESTATS, DEFAULT_ENABLE_GROOVESTATS,
    DEFAULT_FASTLOAD, DEFAULT_GFX_DEBUG, DEFAULT_GLOBAL_OFFSET_SECONDS, DEFAULT_HIDE_MOUSE_CURSOR,
    DEFAULT_HIGH_DPI, DEFAULT_LIGHTS_SIMPLIFY_BASS, DEFAULT_LOG_TO_FILE, DEFAULT_MINE_HIT_SOUND,
    DEFAULT_ONLY_DEDICATED_MENU_BUTTONS, DEFAULT_SELECT_MUSIC_CHART_INFO_EFFECTIVE_BPM,
    DEFAULT_SELECT_MUSIC_CHART_INFO_MATRIX_RATING, DEFAULT_SELECT_MUSIC_CHART_INFO_PEAK_NPS,
    DEFAULT_SELECT_MUSIC_PREVIEW_LOOP, DEFAULT_SELECT_MUSIC_PREVIEW_STARTS_IMMEDIATELY,
    DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_EX, DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_HARD_EX,
    DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_ITG, DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_TOURNAMENTS,
    DEFAULT_SEPARATE_UNLOCKS_BY_PLAYER, DEFAULT_SHADE_SCATTERPLOT_JUDGMENTS, DEFAULT_SHOW_CONSOLE,
    DEFAULT_SHOW_COURSE_INDIVIDUAL_SCORES, DEFAULT_SHOW_MOST_PLAYED_COURSES,
    DEFAULT_SHOW_MUSIC_WHEEL_GRADES, DEFAULT_SHOW_MUSIC_WHEEL_LAMPS, DEFAULT_SHOW_RANDOM_COURSES,
    DEFAULT_SHOW_SELECT_MUSIC_BANNERS, DEFAULT_SHOW_SELECT_MUSIC_BREAKDOWN,
    DEFAULT_SHOW_SELECT_MUSIC_CDTITLES, DEFAULT_SHOW_SELECT_MUSIC_FOLDER_STATS,
    DEFAULT_SHOW_SELECT_MUSIC_PREVIEW_MARKER, DEFAULT_SHOW_SELECT_MUSIC_PREVIEWS,
    DEFAULT_SHOW_SELECT_MUSIC_SCOREBOX, DEFAULT_SHOW_SELECT_MUSIC_STAGE_DISPLAY,
    DEFAULT_SHOW_SELECT_MUSIC_VIDEO_BANNERS, DEFAULT_SHOW_SRPG_SHOP, DEFAULT_SHOW_STATS_MODE,
    DEFAULT_SHOW_VERSION_OVERLAY, DEFAULT_SMOOTH_HISTOGRAM, DEFAULT_SMX_IDLE_LIGHTS_BLACK,
    DEFAULT_SMX_INPUT, DEFAULT_SMX_MANAGES_PAD_CONFIG, DEFAULT_SMX_PANEL_LIGHTS,
    DEFAULT_SMX_UNDERGLOW_GRB, DEFAULT_SMX_UNDERGLOW_THEME, DEFAULT_SOFTWARE_RENDERER_THREADS,
    DEFAULT_SONG_PARSING_THREADS, DEFAULT_SORT_MUSIC_WHEEL_BY_SERIES,
    DEFAULT_SUBMIT_ARROWCLOUD_FAILS, DEFAULT_THREE_KEY_NAVIGATION, DEFAULT_TRANSLATED_TITLES,
    DEFAULT_UPDATER_INSTALL_ENABLED, DEFAULT_USE_FSRS,
};
use crate::ini::SimpleIni;
use crate::machine::{
    DEFAULT_FRAME_STATS_OVERLAY_ANCHOR, DEFAULT_FRAME_STATS_OVERLAY_STYLE,
    canonical_frame_stats_overlay_anchor, canonical_frame_stats_overlay_style,
    clamp_smx_light_brightness_percent,
};
use crate::numbers::parse_auto_threads_u8;
use crate::theme::{
    AUTO_SS_CLEARS, AUTO_SS_FAILS, AUTO_SS_PBS, AUTO_SS_QUADS, AUTO_SS_QUINTS,
    ArrowCloudQrLoginWhen, BreakdownStyle, DefaultFailType, DefaultSyncOffset, GameFlag,
    GrooveStatsQrLoginWhen, LanguageFlag, LogLevel, MachineBarColor, MachineEvaluationStyle,
    MachineFont, MachinePreferredPlayMode, MachinePreferredPlayStyle, NewPackMode,
    RandomBackgroundMode, SelectMusicItlRankMode, SelectMusicItlWheelMode,
    SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement, SelectMusicSongSelectBgMode,
    SelectMusicStepArtistBoxMode, SelectMusicWheelStyle, SrpgShopFolder, SrpgVariant,
    SyncGraphMode, ThemeFlag, VersionOverlaySide, VisualStyle, auto_screenshot_bit,
    auto_screenshot_mask_from_str, auto_screenshot_mask_to_str,
};
use crate::writer::{push_bool, push_line};
#[cfg(windows)]
use deadsync_input_native::WindowsPadBackend;
use deadsync_lights::{DriverKind as LightsDriverKind, GameplayPadLightMode};
use std::str::FromStr;
use std::time::Duration;

pub const SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES: usize = 4;
pub const SELECT_MUSIC_CHART_INFO_NUM_CHOICES: usize = 3;
pub const MUSIC_WHEEL_SCROLL_SPEED_VALUES: [u8; 7] = [5, 10, 15, 25, 30, 45, 100];
pub const SHOW_STATS_MODE_MAX: u8 = 3;
pub const MAX_FPS_MIN: u16 = 5;
pub const MAX_FPS_MAX: u16 = 2_500;
pub const MAX_FPS_STEP: u16 = 1;
pub const MAX_FPS_DEFAULT: u16 = 60;
pub const MAX_FPS_HOLD_FAST_AFTER: Duration = Duration::from_millis(700);
pub const MAX_FPS_HOLD_FASTER_AFTER: Duration = Duration::from_millis(1200);
pub const MAX_FPS_HOLD_FASTEST_AFTER: Duration = Duration::from_millis(1800);

pub const fn lights_driver_choice_index(driver: LightsDriverKind) -> usize {
    match driver {
        LightsDriverKind::Off => 0,
        LightsDriverKind::Snek => 1,
        LightsDriverKind::Litboard => 2,
        LightsDriverKind::Win32Serial => 3,
        LightsDriverKind::Fusion => 4,
        LightsDriverKind::Gpb => 5,
        LightsDriverKind::PacDrive => 6,
        LightsDriverKind::PiuioLeds => 7,
        LightsDriverKind::Itgio => 8,
        LightsDriverKind::HidBlueDot => 9,
        LightsDriverKind::Stac2 => 10,
        LightsDriverKind::MinimaidHid => 11,
    }
}

pub const fn lights_driver_from_choice(idx: usize) -> LightsDriverKind {
    match idx {
        1 => LightsDriverKind::Snek,
        2 => LightsDriverKind::Litboard,
        3 => LightsDriverKind::Win32Serial,
        4 => LightsDriverKind::Fusion,
        5 => LightsDriverKind::Gpb,
        6 => LightsDriverKind::PacDrive,
        7 => LightsDriverKind::PiuioLeds,
        8 => LightsDriverKind::Itgio,
        9 => LightsDriverKind::HidBlueDot,
        10 => LightsDriverKind::Stac2,
        11 => LightsDriverKind::MinimaidHid,
        _ => LightsDriverKind::Off,
    }
}

pub const fn lights_gameplay_pad_choice_index(mode: GameplayPadLightMode) -> usize {
    match mode {
        GameplayPadLightMode::Input => 0,
        GameplayPadLightMode::Chart => 1,
    }
}

pub const fn lights_gameplay_pad_from_choice(idx: usize) -> GameplayPadLightMode {
    match idx {
        1 => GameplayPadLightMode::Chart,
        _ => GameplayPadLightMode::Input,
    }
}

#[cfg(windows)]
pub const fn windows_pad_backend_choice_index(backend: WindowsPadBackend) -> usize {
    match backend {
        WindowsPadBackend::Auto | WindowsPadBackend::RawInput => 0,
        #[cfg(target_vendor = "win7")]
        WindowsPadBackend::Wgi => 0,
        #[cfg(not(target_vendor = "win7"))]
        WindowsPadBackend::Wgi => 1,
    }
}

#[cfg(windows)]
pub const fn windows_pad_backend_from_choice(idx: usize) -> WindowsPadBackend {
    #[cfg(target_vendor = "win7")]
    {
        let _ = idx;
        WindowsPadBackend::RawInput
    }
    #[cfg(not(target_vendor = "win7"))]
    match idx {
        0 => WindowsPadBackend::RawInput,
        _ => WindowsPadBackend::Wgi,
    }
}

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

/// Byte capacity of an SMX animation pack name.
const SMX_PACK_NAME_CAP: usize = 64;

/// Fixed-capacity pack-directory name, so `SystemOptions` (and the app
/// `Config`) stays `Copy`. Empty means the built-in (`common/`) animation set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmxPackName {
    bytes: [u8; SMX_PACK_NAME_CAP],
    len: u8,
}

impl Default for SmxPackName {
    fn default() -> Self {
        Self {
            bytes: [0; SMX_PACK_NAME_CAP],
            len: 0,
        }
    }
}

impl SmxPackName {
    /// Parse a pack name from the ini. Names longer than the capacity fall
    /// back to empty (the built-in set).
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.len() > SMX_PACK_NAME_CAP {
            return Self::default();
        }
        let mut out = Self::default();
        out.bytes[..trimmed.len()].copy_from_slice(trimmed.as_bytes());
        out.len = trimmed.len() as u8;
        out
    }

    pub fn as_str(&self) -> &str {
        // Always valid: the bytes are a prefix copied from a &str.
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemOptions {
    pub game_flag: GameFlag,
    pub auto_download_unlocks: bool,
    pub auto_populate_gs_scores: bool,
    pub updater_install_enabled: bool,
    pub enable_groovestats: bool,
    pub show_srpg_shop: bool,
    pub srpg_shop_folder: SrpgShopFolder,
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
    /// Send platform strip (underglow) colours in GRB wire order instead of
    /// RGB, for strip hardware that consumes WS2812 channel order.
    pub smx_underglow_grb: bool,
    /// When pad gif lighting is on but nothing resolves for the current
    /// screen, hold the pads solid black (true) instead of releasing them to
    /// the pad firmware's built-in lighting (false, the default).
    pub smx_idle_lights_black: bool,
    /// User animation pack supplying the pad backgrounds (a directory under
    /// `assets/smx-pad-lights/dance/`). Empty selects the built-in set.
    pub smx_pad_gifs_pack: SmxPackName,
    /// User animation pack supplying the judgement GIFs (a directory under
    /// `assets/smx-judge-lights/dance/`). Empty selects the built-in set.
    pub smx_judge_gifs_pack: SmxPackName,
    pub gfx_debug: bool,
    pub global_offset_seconds: f32,
    pub language_flag: LanguageFlag,
    pub log_level: LogLevel,
    pub log_to_file: bool,
    pub show_console: bool,
}

impl Default for SystemOptions {
    fn default() -> Self {
        Self {
            game_flag: GameFlag::Dance,
            auto_download_unlocks: DEFAULT_AUTO_DOWNLOAD_UNLOCKS,
            auto_populate_gs_scores: DEFAULT_AUTO_POPULATE_GS_SCORES,
            updater_install_enabled: DEFAULT_UPDATER_INSTALL_ENABLED,
            enable_groovestats: DEFAULT_ENABLE_GROOVESTATS,
            show_srpg_shop: DEFAULT_SHOW_SRPG_SHOP,
            srpg_shop_folder: SrpgShopFolder::default(),
            enable_arrowcloud: DEFAULT_ENABLE_ARROWCLOUD,
            enable_boogiestats: DEFAULT_ENABLE_BOOGIESTATS,
            submit_arrowcloud_fails: DEFAULT_SUBMIT_ARROWCLOUD_FAILS,
            arrowcloud_qr_login_when: ArrowCloudQrLoginWhen::Sometimes,
            groovestats_qr_login_when: GrooveStatsQrLoginWhen::Sometimes,
            separate_unlocks_by_player: DEFAULT_SEPARATE_UNLOCKS_BY_PLAYER,
            mine_hit_sound: DEFAULT_MINE_HIT_SOUND,
            show_stats_mode: DEFAULT_SHOW_STATS_MODE,
            frame_stats_overlay_anchor: DEFAULT_FRAME_STATS_OVERLAY_ANCHOR,
            frame_stats_overlay_style: DEFAULT_FRAME_STATS_OVERLAY_STYLE,
            translated_titles: DEFAULT_TRANSLATED_TITLES,
            bg_brightness: DEFAULT_BG_BRIGHTNESS,
            center_1player_notefield: DEFAULT_CENTER_1PLAYER_NOTEFIELD,
            center_image_translate_x: DEFAULT_CENTER_IMAGE_TRANSLATE_X,
            center_image_translate_y: DEFAULT_CENTER_IMAGE_TRANSLATE_Y,
            center_image_add_width: DEFAULT_CENTER_IMAGE_ADD_WIDTH,
            center_image_add_height: DEFAULT_CENTER_IMAGE_ADD_HEIGHT,
            autosubmit_course_scores_individually: DEFAULT_AUTOSUBMIT_COURSE_SCORES_INDIVIDUALLY,
            show_course_individual_scores: DEFAULT_SHOW_COURSE_INDIVIDUAL_SCORES,
            show_most_played_courses: DEFAULT_SHOW_MOST_PLAYED_COURSES,
            show_random_courses: DEFAULT_SHOW_RANDOM_COURSES,
            default_fail_type: DefaultFailType::ImmediateContinue,
            banner_cache: DEFAULT_BANNER_CACHE,
            cdtitle_cache: DEFAULT_CDTITLE_CACHE,
            high_dpi: DEFAULT_HIGH_DPI,
            hide_mouse_cursor: DEFAULT_HIDE_MOUSE_CURSOR,
            allow_shutdown_host: DEFAULT_ALLOW_SHUTDOWN_HOST,
            smx_input: DEFAULT_SMX_INPUT,
            smx_manages_pad_config: DEFAULT_SMX_MANAGES_PAD_CONFIG,
            smx_panel_lights: DEFAULT_SMX_PANEL_LIGHTS,
            smx_idle_lights_black: DEFAULT_SMX_IDLE_LIGHTS_BLACK,
            smx_underglow_theme: DEFAULT_SMX_UNDERGLOW_THEME,
            smx_underglow_grb: DEFAULT_SMX_UNDERGLOW_GRB,
            smx_pad_gifs_pack: SmxPackName::default(),
            smx_judge_gifs_pack: SmxPackName::default(),
            gfx_debug: DEFAULT_GFX_DEBUG,
            global_offset_seconds: DEFAULT_GLOBAL_OFFSET_SECONDS,
            language_flag: LanguageFlag::Auto,
            log_level: LogLevel::Warn,
            log_to_file: DEFAULT_LOG_TO_FILE,
            show_console: DEFAULT_SHOW_CONSOLE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemInputHardwareOptions<'a> {
    pub system: SystemOptions,
    pub gamepad_backend: &'a str,
    pub smx_default_pad_config: &'a str,
    pub smx_default_light_brightness: u8,
    pub smx_underglow_theme: Option<bool>,
    /// Written alongside `smx_underglow_theme` (both omitted from the
    /// defaults template together).
    pub smx_underglow_grb: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayOptions<'a> {
    pub width: u32,
    pub height: u32,
    pub monitor: usize,
    pub fullscreen_type: &'a str,
    pub max_fps: u16,
    pub present_mode_policy: &'a str,
    pub video_renderer: &'a str,
    pub vsync: bool,
    pub windowed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeIoOptions<'a> {
    pub linux_audio_backend: &'a str,
    pub input_debounce_seconds: f32,
    pub lights_driver: &'a str,
    pub gameplay_pad_lights: &'a str,
    pub lights_com_port: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayLoadOptions<F, P, V> {
    pub vsync: bool,
    pub max_fps: u16,
    pub present_mode_policy: P,
    pub windowed: bool,
    pub fullscreen_type: F,
    pub monitor: usize,
    pub width: u32,
    pub height: u32,
    pub video_renderer: V,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemInputHardwareLoadOptions<W, S> {
    pub gamepad_backend: W,
    pub smx_default_pad_config: S,
    pub smx_default_light_brightness: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeIoLoadOptions<D, G, P> {
    pub input_debounce_seconds: f32,
    pub lights_driver: D,
    pub gameplay_pad_lights: G,
    pub lights_com_port: P,
}

pub fn load_display_options<F, P, V>(
    conf: &SimpleIni,
    default: DisplayLoadOptions<F, P, V>,
    parse_fullscreen_type: impl Fn(&str) -> Option<F>,
    parse_present_mode_policy: impl Fn(&str) -> Option<P>,
    legacy_balanced_policy: P,
    legacy_unhinged_policy: P,
    parse_video_renderer: impl Fn(&str) -> Option<V>,
) -> DisplayLoadOptions<F, P, V>
where
    F: Copy,
    P: Copy,
    V: Copy,
{
    DisplayLoadOptions {
        vsync: parse_u8_bool_or_default(conf.get("Options", "Vsync").as_deref(), default.vsync),
        max_fps: conf
            .get("Options", "MaxFps")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(default.max_fps),
        present_mode_policy: conf
            .get("Options", "PresentModePolicy")
            .and_then(|value| parse_present_mode_policy(&value))
            .or_else(|| {
                conf.get("Options", "UncappedMode").and_then(|value| {
                    parse_legacy_present_mode(
                        &value,
                        legacy_balanced_policy,
                        legacy_unhinged_policy,
                    )
                })
            })
            .unwrap_or(default.present_mode_policy),
        windowed: parse_u8_bool_or_default(
            conf.get("Options", "Windowed").as_deref(),
            default.windowed,
        ),
        fullscreen_type: conf
            .get("Options", "FullscreenType")
            .and_then(|value| parse_fullscreen_type(&value))
            .unwrap_or(default.fullscreen_type),
        monitor: conf
            .get("Options", "DisplayMonitor")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(default.monitor),
        width: conf
            .get("Options", "DisplayWidth")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(default.width),
        height: conf
            .get("Options", "DisplayHeight")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(default.height),
        video_renderer: conf
            .get("Options", "VideoRenderer")
            .and_then(|value| parse_video_renderer(&value))
            .unwrap_or(default.video_renderer),
    }
}

fn parse_legacy_present_mode<P>(raw: &str, balanced_policy: P, unhinged_policy: P) -> Option<P>
where
    P: Copy,
{
    match raw.trim().to_ascii_lowercase().as_str() {
        "balanced" => Some(balanced_policy),
        "unhinged" | "maxfps" | "max_fps" | "max-fps" => Some(unhinged_policy),
        _ => None,
    }
}

pub fn load_system_input_hardware_options<W, S>(
    conf: &SimpleIni,
    default: SystemInputHardwareLoadOptions<W, S>,
    parse_gamepad_backend: impl Fn(&str) -> Option<W>,
    parse_smx_pad_config: impl Fn(&str) -> Option<S>,
) -> SystemInputHardwareLoadOptions<W, S>
where
    W: Copy,
    S: Copy,
{
    SystemInputHardwareLoadOptions {
        gamepad_backend: conf
            .get("Options", "GamepadBackend")
            .and_then(|value| parse_gamepad_backend(&value))
            .unwrap_or(default.gamepad_backend),
        smx_default_pad_config: conf
            .get("Options", "SmxDefaultPadConfig")
            .and_then(|value| parse_smx_pad_config(&value))
            .unwrap_or(default.smx_default_pad_config),
        smx_default_light_brightness: conf
            .get("Options", "SmxDefaultLightBrightness")
            .and_then(|value| value.parse::<u8>().ok())
            .map_or(
                default.smx_default_light_brightness,
                clamp_smx_light_brightness_percent,
            ),
    }
}

pub fn load_runtime_io_options<D, G, P>(
    conf: &SimpleIni,
    default: RuntimeIoLoadOptions<D, G, P>,
    parse_input_debounce_seconds: impl Fn(&str) -> Option<f32>,
    parse_lights_driver: impl Fn(&str, D) -> D,
    parse_gameplay_pad_lights: impl Fn(&str, G) -> G,
    parse_lights_com_port: impl Fn(&str, P) -> P,
) -> RuntimeIoLoadOptions<D, G, P>
where
    D: Copy,
    G: Copy,
    P: Copy,
{
    RuntimeIoLoadOptions {
        input_debounce_seconds: conf
            .get("Options", "InputDebounceTime")
            .and_then(|value| parse_input_debounce_seconds(&value))
            .unwrap_or(default.input_debounce_seconds),
        lights_driver: conf
            .get("Options", "LightsDriver")
            .map(|value| parse_lights_driver(&value, default.lights_driver))
            .unwrap_or(default.lights_driver),
        gameplay_pad_lights: conf
            .get("Options", "GameplayPadLights")
            .map(|value| parse_gameplay_pad_lights(&value, default.gameplay_pad_lights))
            .unwrap_or(default.gameplay_pad_lights),
        lights_com_port: conf
            .get("Options", "LightsComPort")
            .map(|value| parse_lights_com_port(&value, default.lights_com_port))
            .unwrap_or(default.lights_com_port),
    }
}

pub fn load_gameplay_bg_color<C>(
    conf: &SimpleIni,
    default: C,
    parse_color: impl Fn(&str) -> Option<C>,
) -> C
where
    C: Copy,
{
    conf.get("Options", "GameplayBgColor")
        .and_then(|value| parse_color(&value))
        .unwrap_or(default)
}

pub fn load_bool_option(conf: &SimpleIni, section: &str, key: &str, default: bool) -> bool {
    conf.get(section, key)
        .and_then(|value| parse_bool_str(&value))
        .unwrap_or(default)
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
        show_srpg_shop: parse_u8_bool_or_default(
            conf.get("Options", "ShowSrpgShop").as_deref(),
            default.show_srpg_shop,
        ),
        srpg_shop_folder: conf
            .get("Options", "SrpgShopFolder")
            .and_then(|value| SrpgShopFolder::from_str(&value).ok())
            .unwrap_or(default.srpg_shop_folder),
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
        smx_idle_lights_black: conf
            .get("Options", "SmxIdleLightsBlack")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.smx_idle_lights_black),
        smx_underglow_theme: conf
            .get("Options", "SmxUnderglowTheme")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.smx_underglow_theme),
        smx_underglow_grb: conf
            .get("Options", "SmxUnderglowGrb")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.smx_underglow_grb),
        smx_pad_gifs_pack: conf
            .get("Options", "SmxPadGifsPack")
            .map(|value| SmxPackName::parse(&value))
            .unwrap_or(default.smx_pad_gifs_pack),
        smx_judge_gifs_pack: conf
            .get("Options", "SmxJudgeGifsPack")
            .map(|value| SmxPackName::parse(&value))
            .unwrap_or(default.smx_judge_gifs_pack),
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

pub fn push_system_download_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(
        content,
        "AutoDownloadUnlocks",
        options.auto_download_unlocks,
    );
    push_bool(
        content,
        "AutoPopulateGrooveStatsScores",
        options.auto_populate_gs_scores,
    );
    push_bool(
        content,
        "UpdaterInstallEnabled",
        options.updater_install_enabled,
    );
}

pub fn push_system_bg_brightness_option_lines(content: &mut String, options: SystemOptions) {
    push_line(
        content,
        "BGBrightness",
        clamp_bg_brightness(options.bg_brightness),
    );
}

pub fn push_gameplay_bg_color_option_line(content: &mut String, color_hex: &str) {
    push_line(content, "GameplayBgColor", color_hex);
}

pub fn push_system_banner_cache_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(content, "BannerCache", options.banner_cache);
}

pub fn push_system_cdtitle_center_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(content, "CDTitleCache", options.cdtitle_cache);
    push_bool(content, "Center1Player", options.center_1player_notefield);
    push_line(
        content,
        "CenterImageTranslateX",
        options.center_image_translate_x,
    );
    push_line(
        content,
        "CenterImageTranslateY",
        options.center_image_translate_y,
    );
    push_line(
        content,
        "CenterImageAddWidth",
        options.center_image_add_width,
    );
    push_line(
        content,
        "CenterImageAddHeight",
        options.center_image_add_height,
    );
}

pub fn push_system_course_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(
        content,
        "CourseAutosubmitScoresIndividually",
        options.autosubmit_course_scores_individually,
    );
    push_bool(
        content,
        "CourseShowIndividualScores",
        options.show_course_individual_scores,
    );
    push_bool(
        content,
        "CourseShowMostPlayed",
        options.show_most_played_courses,
    );
    push_bool(content, "CourseShowRandom", options.show_random_courses);
    push_line(
        content,
        "DefaultFailType",
        options.default_fail_type.as_str(),
    );
}

pub fn push_system_online_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(content, "EnableArrowCloud", options.enable_arrowcloud);
    push_bool(content, "EnableBoogieStats", options.enable_boogiestats);
    push_bool(content, "EnableGrooveStats", options.enable_groovestats);
    push_bool(content, "ShowSrpgShop", options.show_srpg_shop);
    push_line(content, "SrpgShopFolder", options.srpg_shop_folder.as_str());
    push_bool(
        content,
        "SubmitArrowCloudFails",
        options.submit_arrowcloud_fails,
    );
    push_line(
        content,
        "ArrowCloudQrLoginWhen",
        options.arrowcloud_qr_login_when.as_str(),
    );
    push_line(
        content,
        "GrooveStatsQrLoginWhen",
        options.groovestats_qr_login_when.as_str(),
    );
}

pub fn push_system_input_hardware_option_lines(
    content: &mut String,
    options: SystemInputHardwareOptions<'_>,
) {
    push_line(content, "Game", options.system.game_flag.as_str());
    push_line(content, "GamepadBackend", options.gamepad_backend);
    push_bool(content, "AllowShutdown", options.system.allow_shutdown_host);
    push_bool(content, "SmxInput", options.system.smx_input);
    push_bool(
        content,
        "SmxManagesPadConfig",
        options.system.smx_manages_pad_config,
    );
    push_bool(content, "SmxPanelLights", options.system.smx_panel_lights);
    push_bool(
        content,
        "SmxIdleLightsBlack",
        options.system.smx_idle_lights_black,
    );
    if let Some(enabled) = options.smx_underglow_theme {
        push_bool(content, "SmxUnderglowTheme", enabled);
    }
    if let Some(grb) = options.smx_underglow_grb {
        push_bool(content, "SmxUnderglowGrb", grb);
    }
    push_line(
        content,
        "SmxPadGifsPack",
        options.system.smx_pad_gifs_pack.as_str(),
    );
    push_line(
        content,
        "SmxJudgeGifsPack",
        options.system.smx_judge_gifs_pack.as_str(),
    );
    push_line(
        content,
        "SmxDefaultPadConfig",
        options.smx_default_pad_config,
    );
    push_line(
        content,
        "SmxDefaultLightBrightness",
        clamp_smx_light_brightness_percent(options.smx_default_light_brightness),
    );
}

pub fn push_display_size_option_lines(content: &mut String, options: DisplayOptions<'_>) {
    push_line(content, "DisplayHeight", options.height);
    push_line(content, "DisplayWidth", options.width);
}

pub fn push_display_monitor_option_lines(content: &mut String, options: DisplayOptions<'_>) {
    push_line(content, "DisplayMonitor", options.monitor);
}

pub fn push_display_fullscreen_option_lines(content: &mut String, options: DisplayOptions<'_>) {
    push_line(content, "FullscreenType", options.fullscreen_type);
}

pub fn push_display_frame_timing_option_lines(content: &mut String, options: DisplayOptions<'_>) {
    push_line(content, "MaxFps", options.max_fps);
    push_line(content, "PresentModePolicy", options.present_mode_policy);
}

pub fn push_display_video_tail_option_lines(content: &mut String, options: DisplayOptions<'_>) {
    push_line(content, "VideoRenderer", options.video_renderer);
    push_bool(content, "Vsync", options.vsync);
    push_bool(content, "Windowed", options.windowed);
}

pub fn push_runtime_audio_backend_option_lines(
    content: &mut String,
    options: RuntimeIoOptions<'_>,
) {
    push_line(content, "LinuxAudioBackend", options.linux_audio_backend);
}

pub fn push_runtime_input_debounce_option_lines(
    content: &mut String,
    options: RuntimeIoOptions<'_>,
) {
    push_line(
        content,
        "InputDebounceTime",
        format!("{:.3}", options.input_debounce_seconds),
    );
}

pub fn push_runtime_lights_driver_option_lines(
    content: &mut String,
    options: RuntimeIoOptions<'_>,
) {
    push_line(content, "LightsDriver", options.lights_driver);
    push_line(content, "GameplayPadLights", options.gameplay_pad_lights);
}

pub fn push_runtime_lights_port_option_lines(content: &mut String, options: RuntimeIoOptions<'_>) {
    push_line(content, "LightsComPort", options.lights_com_port);
}

pub fn push_system_diagnostics_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(content, "GfxDebug", options.gfx_debug);
    push_bool(content, "HighDPI", options.high_dpi);
    push_bool(content, "HideMouseCursor", options.hide_mouse_cursor);
    push_line(
        content,
        "GlobalOffsetSeconds",
        options.global_offset_seconds,
    );
    push_line(content, "Language", options.language_flag.as_str());
    push_line(content, "LogLevel", options.log_level.as_str());
    push_bool(content, "LogToFile", options.log_to_file);
    push_bool(content, "ShowConsole", options.show_console);
}

pub fn push_system_mine_hit_sound_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(content, "MineHitSound", options.mine_hit_sound);
}

pub fn push_system_translation_option_lines(content: &mut String, options: SystemOptions) {
    push_bool(content, "TranslatedTitles", options.translated_titles);
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
    pub sort_wheel_by_series: bool,
    pub itl_rank_mode: SelectMusicItlRankMode,
    pub itl_wheel_mode: SelectMusicItlWheelMode,
    pub wheel_style: SelectMusicWheelStyle,
    pub song_select_bg_mode: SelectMusicSongSelectBgMode,
    pub new_pack_mode: NewPackMode,
    pub show_folder_stats: bool,
    pub show_previews: bool,
    pub show_preview_marker: bool,
    pub preview_loop: bool,
    pub preview_starts_immediately: bool,
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

impl Default for SelectMusicOptions {
    fn default() -> Self {
        Self {
            breakdown_style: BreakdownStyle::Sl,
            show_banners: DEFAULT_SHOW_SELECT_MUSIC_BANNERS,
            show_version_overlay: DEFAULT_SHOW_VERSION_OVERLAY,
            version_overlay_side: VersionOverlaySide::Right,
            show_video_banners: DEFAULT_SHOW_SELECT_MUSIC_VIDEO_BANNERS,
            show_breakdown: DEFAULT_SHOW_SELECT_MUSIC_BREAKDOWN,
            show_stage_display: DEFAULT_SHOW_SELECT_MUSIC_STAGE_DISPLAY,
            show_cdtitles: DEFAULT_SHOW_SELECT_MUSIC_CDTITLES,
            show_wheel_grades: DEFAULT_SHOW_MUSIC_WHEEL_GRADES,
            show_wheel_lamps: DEFAULT_SHOW_MUSIC_WHEEL_LAMPS,
            sort_wheel_by_series: DEFAULT_SORT_MUSIC_WHEEL_BY_SERIES,
            itl_rank_mode: SelectMusicItlRankMode::None,
            itl_wheel_mode: SelectMusicItlWheelMode::Score,
            wheel_style: SelectMusicWheelStyle::Itg,
            song_select_bg_mode: SelectMusicSongSelectBgMode::Off,
            new_pack_mode: NewPackMode::Disabled,
            show_folder_stats: DEFAULT_SHOW_SELECT_MUSIC_FOLDER_STATS,
            show_previews: DEFAULT_SHOW_SELECT_MUSIC_PREVIEWS,
            show_preview_marker: DEFAULT_SHOW_SELECT_MUSIC_PREVIEW_MARKER,
            preview_loop: DEFAULT_SELECT_MUSIC_PREVIEW_LOOP,
            preview_starts_immediately: DEFAULT_SELECT_MUSIC_PREVIEW_STARTS_IMMEDIATELY,
            pattern_info_mode: SelectMusicPatternInfoMode::Tech,
            step_artist_box_mode: SelectMusicStepArtistBoxMode::Default,
            show_scorebox: DEFAULT_SHOW_SELECT_MUSIC_SCOREBOX,
            scorebox_placement: SelectMusicScoreboxPlacement::Auto,
            scorebox_cycle_itg: DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_ITG,
            scorebox_cycle_ex: DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_EX,
            scorebox_cycle_hard_ex: DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_HARD_EX,
            scorebox_cycle_tournaments: DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_TOURNAMENTS,
            chart_info_peak_nps: DEFAULT_SELECT_MUSIC_CHART_INFO_PEAK_NPS,
            chart_info_effective_bpm: DEFAULT_SELECT_MUSIC_CHART_INFO_EFFECTIVE_BPM,
            chart_info_matrix_rating: DEFAULT_SELECT_MUSIC_CHART_INFO_MATRIX_RATING,
            auto_screenshot_eval: DEFAULT_AUTO_SCREENSHOT_EVAL,
        }
    }
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
        sort_wheel_by_series: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicSortBySeries").as_deref(),
            default.sort_wheel_by_series,
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
        preview_starts_immediately: parse_u8_bool_or_default(
            conf.get("Options", "SelectMusicPreviewStartsImmediately")
                .as_deref(),
            default.preview_starts_immediately,
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
pub struct SelectMusicSaveOptions {
    pub select_music: SelectMusicOptions,
    pub separate_unlocks_by_player: bool,
}

pub fn push_select_music_option_lines(content: &mut String, options: SelectMusicSaveOptions) {
    let select = options.select_music;
    push_line(
        content,
        "SelectMusicBreakdown",
        select.breakdown_style.as_str(),
    );
    push_bool(content, "SelectMusicShowBanners", select.show_banners);
    push_bool(content, "ShowVersionOverlay", select.show_version_overlay);
    push_line(
        content,
        "VersionOverlaySide",
        select.version_overlay_side.as_str(),
    );
    push_bool(
        content,
        "SelectMusicShowVideoBanners",
        select.show_video_banners,
    );
    push_bool(content, "SelectMusicShowBreakdown", select.show_breakdown);
    push_bool(
        content,
        "SelectMusicShowStageDisplay",
        select.show_stage_display,
    );
    push_bool(content, "SelectMusicShowCDTitles", select.show_cdtitles);
    push_bool(content, "SelectMusicWheelGrades", select.show_wheel_grades);
    push_bool(content, "SelectMusicWheelLamps", select.show_wheel_lamps);
    push_bool(
        content,
        "SelectMusicSortBySeries",
        select.sort_wheel_by_series,
    );
    push_line(
        content,
        "SelectMusicWheelITLRank",
        select.itl_rank_mode.as_str(),
    );
    push_line(
        content,
        "SelectMusicWheelITL",
        select.itl_wheel_mode.as_str(),
    );
    push_line(
        content,
        "SelectMusicWheelStyle",
        select.wheel_style.as_str(),
    );
    push_line(content, "SongSelectBG", select.song_select_bg_mode.as_str());
    push_line(
        content,
        "SelectMusicNewPackMode",
        select.new_pack_mode.as_str(),
    );
    push_bool(content, "SelectMusicFolderStats", select.show_folder_stats);
    push_bool(content, "SelectMusicPreviews", select.show_previews);
    push_bool(
        content,
        "SelectMusicPreviewMarker",
        select.show_preview_marker,
    );
    push_bool(content, "SelectMusicPreviewLoop", select.preview_loop);
    push_bool(
        content,
        "SelectMusicPreviewStartsImmediately",
        select.preview_starts_immediately,
    );
    push_line(
        content,
        "SelectMusicPatternInfo",
        select.pattern_info_mode.as_str(),
    );
    push_line(
        content,
        "SelectMusicStepArtistBox",
        select.step_artist_box_mode.as_str(),
    );
    push_bool(content, "SelectMusicScorebox", select.show_scorebox);
    push_line(
        content,
        "SelectMusicScoreboxPlacement",
        select.scorebox_placement.as_str(),
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleItg",
        select.scorebox_cycle_itg,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleEx",
        select.scorebox_cycle_ex,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleHardEx",
        select.scorebox_cycle_hard_ex,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleTournaments",
        select.scorebox_cycle_tournaments,
    );
    push_bool(
        content,
        "SelectMusicChartInfoPeakNps",
        select.chart_info_peak_nps,
    );
    push_bool(
        content,
        "SelectMusicChartInfoEffectiveBpm",
        select.chart_info_effective_bpm,
    );
    push_bool(
        content,
        "SelectMusicChartInfoMatrixRating",
        select.chart_info_matrix_rating,
    );
    push_bool(
        content,
        "SeparateUnlocksByPlayer",
        options.separate_unlocks_by_player,
    );
    push_line(
        content,
        "AutoScreenshotEval",
        auto_screenshot_mask_to_str(select.auto_screenshot_eval),
    );
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

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            fastload: DEFAULT_FASTLOAD,
            cachesongs: DEFAULT_CACHE_SONGS,
            song_parsing_threads: DEFAULT_SONG_PARSING_THREADS,
            smooth_histogram: DEFAULT_SMOOTH_HISTOGRAM,
            shade_scatterplot_judgments: DEFAULT_SHADE_SCATTERPLOT_JUDGMENTS,
            arcade_options_navigation: DEFAULT_ARCADE_OPTIONS_NAVIGATION,
            delayed_back: DEFAULT_DELAYED_BACK,
            three_key_navigation: DEFAULT_THREE_KEY_NAVIGATION,
            use_fsrs: DEFAULT_USE_FSRS,
            lights_simplify_bass: DEFAULT_LIGHTS_SIMPLIFY_BASS,
            only_dedicated_menu_buttons: DEFAULT_ONLY_DEDICATED_MENU_BUTTONS,
            theme_flag: ThemeFlag::SimplyLove,
            software_renderer_threads: DEFAULT_SOFTWARE_RENDERER_THREADS,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsOverlayOptions<'a> {
    pub show_stats_mode: u8,
    pub frame_stats_overlay_anchor: Option<&'a str>,
    pub frame_stats_overlay_style: Option<&'a str>,
    pub smooth_histogram: bool,
    pub shade_scatterplot_judgments: bool,
}

pub fn push_stats_overlay_option_lines(content: &mut String, options: StatsOverlayOptions<'_>) {
    push_bool(content, "ShowStats", options.show_stats_mode != 0);
    push_line(
        content,
        "ShowStatsMode",
        clamp_show_stats_mode(options.show_stats_mode),
    );
    if let Some(anchor) = options.frame_stats_overlay_anchor {
        push_line(content, "FrameStatsOverlayAnchor", anchor);
    }
    if let Some(style) = options.frame_stats_overlay_style {
        push_line(content, "FrameStatsOverlayStyle", style);
    }
    push_bool(content, "SmoothHistogram", options.smooth_histogram);
    push_bool(
        content,
        "ShadeScatterplotJudgments",
        options.shade_scatterplot_judgments,
    );
}

pub fn push_runtime_cache_option_lines(content: &mut String, options: RuntimeOptions) {
    push_bool(content, "CacheSongs", options.cachesongs);
}

pub fn push_runtime_fastload_option_lines(content: &mut String, options: RuntimeOptions) {
    push_bool(content, "FastLoad", options.fastload);
}

pub fn push_runtime_navigation_option_lines(content: &mut String, options: RuntimeOptions) {
    push_bool(
        content,
        "ArcadeOptionsNavigation",
        options.arcade_options_navigation,
    );
    push_bool(content, "DelayedBack", options.delayed_back);
    push_bool(content, "ThreeKeyNavigation", options.three_key_navigation);
    push_bool(content, "UseFSRs", options.use_fsrs);
}

pub fn push_runtime_lights_option_lines(content: &mut String, options: RuntimeOptions) {
    push_bool(content, "LightsSimplifyBass", options.lights_simplify_bass);
}

pub fn push_runtime_menu_option_lines(content: &mut String, options: RuntimeOptions) {
    push_bool(
        content,
        "OnlyDedicatedMenuButtons",
        options.only_dedicated_menu_buttons,
    );
}

pub fn push_runtime_worker_theme_option_lines(content: &mut String, options: RuntimeOptions) {
    push_line(content, "SongParsingThreads", options.song_parsing_threads);
    push_line(
        content,
        "SoftwareRendererThreads",
        options.software_renderer_threads,
    );
    push_line(content, "Theme", options.theme_flag.as_str());
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

pub const fn srpg_shop_folder_choice_index(folder: SrpgShopFolder) -> usize {
    match folder {
        SrpgShopFolder::Unlocks => 0,
        SrpgShopFolder::Shops => 1,
        SrpgShopFolder::Faction => 2,
    }
}

pub const fn srpg_shop_folder_from_choice(idx: usize) -> SrpgShopFolder {
    match idx {
        1 => SrpgShopFolder::Shops,
        2 => SrpgShopFolder::Faction,
        _ => SrpgShopFolder::Unlocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_choices_match_config_order() {
        let drivers = [
            LightsDriverKind::Off,
            LightsDriverKind::Snek,
            LightsDriverKind::Litboard,
            LightsDriverKind::Win32Serial,
            LightsDriverKind::Fusion,
            LightsDriverKind::Gpb,
            LightsDriverKind::PacDrive,
            LightsDriverKind::PiuioLeds,
            LightsDriverKind::Itgio,
            LightsDriverKind::HidBlueDot,
            LightsDriverKind::Stac2,
            LightsDriverKind::MinimaidHid,
        ];
        for (idx, driver) in drivers.into_iter().enumerate() {
            assert_eq!(lights_driver_choice_index(driver), idx);
            assert_eq!(lights_driver_from_choice(idx), driver);
        }
        assert_eq!(
            lights_gameplay_pad_from_choice(lights_gameplay_pad_choice_index(
                GameplayPadLightMode::Chart
            )),
            GameplayPadLightMode::Chart
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_pad_backend_choices_match_options_order() {
        assert_eq!(windows_pad_backend_choice_index(WindowsPadBackend::Auto), 0);
        assert_eq!(
            windows_pad_backend_choice_index(WindowsPadBackend::RawInput),
            0
        );
        assert_eq!(
            windows_pad_backend_from_choice(0),
            WindowsPadBackend::RawInput
        );

        #[cfg(target_vendor = "win7")]
        {
            assert_eq!(windows_pad_backend_choice_index(WindowsPadBackend::Wgi), 0);
            assert_eq!(
                windows_pad_backend_from_choice(99),
                WindowsPadBackend::RawInput
            );
        }
        #[cfg(not(target_vendor = "win7"))]
        {
            assert_eq!(windows_pad_backend_choice_index(WindowsPadBackend::Wgi), 1);
            assert_eq!(windows_pad_backend_from_choice(1), WindowsPadBackend::Wgi);
            assert_eq!(windows_pad_backend_from_choice(99), WindowsPadBackend::Wgi);
        }
    }

    fn ini(content: &str) -> SimpleIni {
        let mut conf = SimpleIni::new();
        conf.load_str(content);
        conf
    }

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
            sort_wheel_by_series: true,
            itl_rank_mode: SelectMusicItlRankMode::None,
            itl_wheel_mode: SelectMusicItlWheelMode::Off,
            wheel_style: SelectMusicWheelStyle::Itg,
            song_select_bg_mode: SelectMusicSongSelectBgMode::Off,
            new_pack_mode: NewPackMode::Disabled,
            show_folder_stats: false,
            show_previews: true,
            show_preview_marker: true,
            preview_loop: false,
            preview_starts_immediately: false,
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
            show_srpg_shop: true,
            srpg_shop_folder: SrpgShopFolder::Unlocks,
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
            smx_idle_lights_black: false,
            smx_underglow_theme: false,
            smx_underglow_grb: false,
            smx_pad_gifs_pack: SmxPackName::default(),
            smx_judge_gifs_pack: SmxPackName::default(),
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

    fn parse_letter_token(raw: &str) -> Option<char> {
        match raw.trim() {
            "a" => Some('a'),
            "b" => Some('b'),
            "c" => Some('c'),
            _ => None,
        }
    }

    #[test]
    fn loads_display_options_with_token_parsers() {
        let loaded = load_display_options(
            &ini(r#"
                [Options]
                Vsync=1
                MaxFps=144
                PresentModePolicy=b
                Windowed=0
                FullscreenType=c
                DisplayMonitor=2
                DisplayWidth=1920
                DisplayHeight=1080
                VideoRenderer=a
                "#),
            DisplayLoadOptions {
                vsync: false,
                max_fps: 60,
                present_mode_policy: 'a',
                windowed: true,
                fullscreen_type: 'a',
                monitor: 0,
                width: 1600,
                height: 900,
                video_renderer: 'b',
            },
            parse_letter_token,
            parse_letter_token,
            'a',
            'c',
            parse_letter_token,
        );

        assert_eq!(
            loaded,
            DisplayLoadOptions {
                vsync: true,
                max_fps: 144,
                present_mode_policy: 'b',
                windowed: false,
                fullscreen_type: 'c',
                monitor: 2,
                width: 1920,
                height: 1080,
                video_renderer: 'a',
            },
        );
    }

    #[test]
    fn display_options_support_legacy_uncapped_mode() {
        let loaded = load_display_options(
            &ini("[Options]\nUncappedMode=max-fps\n"),
            DisplayLoadOptions {
                vsync: false,
                max_fps: 60,
                present_mode_policy: 'a',
                windowed: true,
                fullscreen_type: 'a',
                monitor: 0,
                width: 1600,
                height: 900,
                video_renderer: 'b',
            },
            parse_letter_token,
            parse_letter_token,
            'b',
            'c',
            parse_letter_token,
        );

        assert_eq!(loaded.present_mode_policy, 'c');
    }

    #[test]
    fn loads_system_input_hardware_options_with_token_parsers() {
        let loaded = load_system_input_hardware_options(
            &ini(r#"
                [Options]
                GamepadBackend=b
                SmxDefaultPadConfig=c
                SmxDefaultLightBrightness=250
                "#),
            SystemInputHardwareLoadOptions {
                gamepad_backend: 'a',
                smx_default_pad_config: 'a',
                smx_default_light_brightness: 80,
            },
            parse_letter_token,
            parse_letter_token,
        );

        assert_eq!(
            loaded,
            SystemInputHardwareLoadOptions {
                gamepad_backend: 'b',
                smx_default_pad_config: 'c',
                smx_default_light_brightness: 100,
            },
        );
    }

    #[test]
    fn loads_runtime_io_options_with_token_parsers() {
        let loaded = load_runtime_io_options(
            &ini(r#"
                [Options]
                InputDebounceTime=0.050
                LightsDriver=b
                GameplayPadLights=c
                LightsComPort=a
                "#),
            RuntimeIoLoadOptions {
                input_debounce_seconds: 0.02,
                lights_driver: 'a',
                gameplay_pad_lights: 'a',
                lights_com_port: 'b',
            },
            |raw| raw.parse::<f32>().ok(),
            |raw, default| parse_letter_token(raw).unwrap_or(default),
            |raw, default| parse_letter_token(raw).unwrap_or(default),
            |raw, default| parse_letter_token(raw).unwrap_or(default),
        );

        assert_eq!(
            loaded,
            RuntimeIoLoadOptions {
                input_debounce_seconds: 0.05,
                lights_driver: 'b',
                gameplay_pad_lights: 'c',
                lights_com_port: 'a',
            },
        );
    }

    #[test]
    fn loads_gameplay_bg_color_with_token_parser() {
        let loaded = load_gameplay_bg_color(
            &ini(r#"
                [Options]
                GameplayBgColor=b
                "#),
            'a',
            parse_letter_token,
        );

        assert_eq!(loaded, 'b');
    }

    #[test]
    fn loads_bool_option_from_section_key() {
        let conf = ini("[Options]\nLogToFile=false\nShowConsole=bad\n");

        assert!(!load_bool_option(&conf, "Options", "LogToFile", true));
        assert!(load_bool_option(&conf, "Options", "ShowConsole", true));
        assert!(!load_bool_option(&conf, "Options", "Missing", false));
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
            ShowSrpgShop=0
            SrpgShopFolder=Faction
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
            SmxIdleLightsBlack=1
            SmxUnderglowTheme=1
            SmxUnderglowGrb=1
            SmxPadGifsPack=senpi-basic
            SmxJudgeGifsPack=none
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
        assert!(!loaded.show_srpg_shop);
        assert_eq!(loaded.srpg_shop_folder, SrpgShopFolder::Faction);
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
        assert!(loaded.smx_idle_lights_black);
        assert!(loaded.smx_underglow_theme);
        assert!(loaded.smx_underglow_grb);
        assert_eq!(loaded.smx_pad_gifs_pack.as_str(), "senpi-basic");
        assert_eq!(loaded.smx_judge_gifs_pack.as_str(), "none");
        assert!(loaded.gfx_debug);
        assert_eq!(loaded.global_offset_seconds, 0.125);
        assert_eq!(loaded.language_flag, LanguageFlag::Japanese);
        assert_eq!(loaded.log_level, LogLevel::Trace);
        assert!(!loaded.log_to_file);
        assert!(loaded.show_console);
    }

    #[test]
    fn smx_pack_name_parse_trims_and_caps() {
        assert_eq!(
            SmxPackName::parse("  senpi-basic  ").as_str(),
            "senpi-basic"
        );
        assert!(SmxPackName::parse("").is_empty());
        assert!(SmxPackName::parse("   ").is_empty());
        // Over-capacity names fall back to empty (the built-in set).
        assert!(SmxPackName::parse(&"x".repeat(65)).is_empty());
        assert_eq!(SmxPackName::parse(&"x".repeat(64)).as_str(), "x".repeat(64));
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
    fn writes_system_option_line_groups() {
        let mut content = String::new();
        let mut options = default_system_options();
        options.auto_download_unlocks = true;
        options.auto_populate_gs_scores = true;
        options.updater_install_enabled = false;
        options.autosubmit_course_scores_individually = false;
        options.show_course_individual_scores = true;
        options.show_most_played_courses = false;
        options.show_random_courses = true;
        options.default_fail_type = DefaultFailType::Immediate;
        options.enable_arrowcloud = true;
        options.enable_boogiestats = false;
        options.enable_groovestats = true;
        options.submit_arrowcloud_fails = true;
        options.arrowcloud_qr_login_when = ArrowCloudQrLoginWhen::Always;
        options.groovestats_qr_login_when = GrooveStatsQrLoginWhen::Disabled;
        options.gfx_debug = true;
        options.high_dpi = true;
        options.hide_mouse_cursor = false;
        options.global_offset_seconds = -0.012;
        options.language_flag = LanguageFlag::German;
        options.log_level = LogLevel::Trace;
        options.log_to_file = false;
        options.show_console = true;

        push_system_download_option_lines(&mut content, options);
        push_system_course_option_lines(&mut content, options);
        push_system_online_option_lines(&mut content, options);
        push_system_diagnostics_option_lines(&mut content, options);

        assert_eq!(
            content,
            concat!(
                "AutoDownloadUnlocks=1\n",
                "AutoPopulateGrooveStatsScores=1\n",
                "UpdaterInstallEnabled=0\n",
                "CourseAutosubmitScoresIndividually=0\n",
                "CourseShowIndividualScores=1\n",
                "CourseShowMostPlayed=0\n",
                "CourseShowRandom=1\n",
                "DefaultFailType=Immediate\n",
                "EnableArrowCloud=1\n",
                "EnableBoogieStats=0\n",
                "EnableGrooveStats=1\n",
                "ShowSrpgShop=1\n",
                "SrpgShopFolder=Unlocks\n",
                "SubmitArrowCloudFails=1\n",
                "ArrowCloudQrLoginWhen=Always\n",
                "GrooveStatsQrLoginWhen=Disabled\n",
                "GfxDebug=1\n",
                "HighDPI=1\n",
                "HideMouseCursor=0\n",
                "GlobalOffsetSeconds=-0.012\n",
                "Language=German\n",
                "LogLevel=Trace\n",
                "LogToFile=0\n",
                "ShowConsole=1\n",
            ),
        );
    }

    #[test]
    fn writes_system_input_hardware_lines() {
        let mut content = String::new();
        let mut options = default_system_options();
        options.allow_shutdown_host = true;
        options.smx_input = true;
        options.smx_manages_pad_config = true;
        options.smx_panel_lights = false;
        options.smx_idle_lights_black = true;
        let hardware_options = SystemInputHardwareOptions {
            system: options,
            gamepad_backend: "RawInput",
            smx_default_pad_config: "High",
            smx_default_light_brightness: 120,
            smx_underglow_theme: Some(true),
            smx_underglow_grb: Some(false),
        };

        push_system_input_hardware_option_lines(&mut content, hardware_options);

        assert_eq!(
            content,
            concat!(
                "Game=dance\n",
                "GamepadBackend=RawInput\n",
                "AllowShutdown=1\n",
                "SmxInput=1\n",
                "SmxManagesPadConfig=1\n",
                "SmxPanelLights=0\n",
                "SmxIdleLightsBlack=1\n",
                "SmxUnderglowTheme=1\n",
                "SmxUnderglowGrb=0\n",
                "SmxPadGifsPack=\n",
                "SmxJudgeGifsPack=\n",
                "SmxDefaultPadConfig=High\n",
                "SmxDefaultLightBrightness=100\n",
            ),
        );

        content.clear();
        push_system_input_hardware_option_lines(
            &mut content,
            SystemInputHardwareOptions {
                smx_underglow_theme: None,
                smx_underglow_grb: None,
                ..hardware_options
            },
        );

        assert!(!content.contains("SmxUnderglowTheme"));
        assert!(!content.contains("SmxUnderglowGrb"));
    }

    #[test]
    fn writes_display_option_line_groups() {
        let mut content = String::new();
        let options = DisplayOptions {
            width: 1600,
            height: 900,
            monitor: 2,
            fullscreen_type: "Borderless",
            max_fps: 144,
            present_mode_policy: "immediate",
            video_renderer: "OpenGL",
            vsync: true,
            windowed: false,
        };

        push_display_size_option_lines(&mut content, options);
        push_display_monitor_option_lines(&mut content, options);
        push_display_fullscreen_option_lines(&mut content, options);
        push_display_frame_timing_option_lines(&mut content, options);
        push_display_video_tail_option_lines(&mut content, options);

        assert_eq!(
            content,
            concat!(
                "DisplayHeight=900\n",
                "DisplayWidth=1600\n",
                "DisplayMonitor=2\n",
                "FullscreenType=Borderless\n",
                "MaxFps=144\n",
                "PresentModePolicy=immediate\n",
                "VideoRenderer=OpenGL\n",
                "Vsync=1\n",
                "Windowed=0\n",
            ),
        );
    }

    #[test]
    fn writes_runtime_io_option_line_groups() {
        let mut content = String::new();
        let options = RuntimeIoOptions {
            linux_audio_backend: "PipeWire",
            input_debounce_seconds: 0.0174,
            lights_driver: "StepManiaX",
            gameplay_pad_lights: "Judgment",
            lights_com_port: "COM4",
        };

        push_runtime_audio_backend_option_lines(&mut content, options);
        push_runtime_input_debounce_option_lines(&mut content, options);
        push_runtime_lights_driver_option_lines(&mut content, options);
        push_runtime_lights_port_option_lines(&mut content, options);

        assert_eq!(
            content,
            concat!(
                "LinuxAudioBackend=PipeWire\n",
                "InputDebounceTime=0.017\n",
                "LightsDriver=StepManiaX\n",
                "GameplayPadLights=Judgment\n",
                "LightsComPort=COM4\n",
            ),
        );
    }

    #[test]
    fn writes_system_split_option_line_groups() {
        let mut content = String::new();
        let mut options = default_system_options();
        options.bg_brightness = 1.25;
        options.banner_cache = false;
        options.cdtitle_cache = false;
        options.center_1player_notefield = true;
        options.center_image_translate_x = -8;
        options.center_image_translate_y = 6;
        options.center_image_add_width = 14;
        options.center_image_add_height = -2;
        options.mine_hit_sound = false;
        options.translated_titles = true;

        push_system_bg_brightness_option_lines(&mut content, options);
        push_system_banner_cache_option_lines(&mut content, options);
        push_system_cdtitle_center_option_lines(&mut content, options);
        push_system_mine_hit_sound_option_lines(&mut content, options);
        push_system_translation_option_lines(&mut content, options);

        assert_eq!(
            content,
            concat!(
                "BGBrightness=1\n",
                "BannerCache=0\n",
                "CDTitleCache=0\n",
                "Center1Player=1\n",
                "CenterImageTranslateX=-8\n",
                "CenterImageTranslateY=6\n",
                "CenterImageAddWidth=14\n",
                "CenterImageAddHeight=-2\n",
                "MineHitSound=0\n",
                "TranslatedTitles=1\n",
            ),
        );
    }

    #[test]
    fn writes_gameplay_bg_color_option_line() {
        let mut content = String::new();

        push_gameplay_bg_color_option_line(&mut content, "#102030");

        assert_eq!(content, "GameplayBgColor=#102030\n");
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
            SelectMusicSortBySeries=0
            SelectMusicWheelITLRank=Overall
            SelectMusicWheelITL=Points
            SelectMusicWheelStyle=IIDX
            SongSelectBG=BG
            SelectMusicNewPackMode=OpenPack
            SelectMusicFolderStats=1
            SelectMusicPreviews=0
            SelectMusicPreviewMarker=0
            SelectMusicPreviewLoop=1
            SelectMusicPreviewStartsImmediately=1
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
        assert!(!loaded.sort_wheel_by_series);
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
    fn writes_select_music_option_lines() {
        let mut content = String::new();
        let mut select_music = default_select_music_options();
        select_music.auto_screenshot_eval = AUTO_SS_PBS | AUTO_SS_QUADS;

        push_select_music_option_lines(
            &mut content,
            SelectMusicSaveOptions {
                select_music,
                separate_unlocks_by_player: true,
            },
        );

        assert_eq!(
            content,
            concat!(
                "SelectMusicBreakdown=SL\n",
                "SelectMusicShowBanners=1\n",
                "ShowVersionOverlay=0\n",
                "VersionOverlaySide=Right\n",
                "SelectMusicShowVideoBanners=0\n",
                "SelectMusicShowBreakdown=1\n",
                "SelectMusicShowStageDisplay=0\n",
                "SelectMusicShowCDTitles=1\n",
                "SelectMusicWheelGrades=1\n",
                "SelectMusicWheelLamps=0\n",
                "SelectMusicSortBySeries=1\n",
                "SelectMusicWheelITLRank=None\n",
                "SelectMusicWheelITL=Off\n",
                "SelectMusicWheelStyle=ITG\n",
                "SongSelectBG=Off\n",
                "SelectMusicNewPackMode=Disabled\n",
                "SelectMusicFolderStats=0\n",
                "SelectMusicPreviews=1\n",
                "SelectMusicPreviewMarker=1\n",
                "SelectMusicPreviewLoop=0\n",
                "SelectMusicPreviewStartsImmediately=0\n",
                "SelectMusicPatternInfo=Auto\n",
                "SelectMusicStepArtistBox=Default\n",
                "SelectMusicScorebox=0\n",
                "SelectMusicScoreboxPlacement=Auto\n",
                "SelectMusicScoreboxCycleItg=1\n",
                "SelectMusicScoreboxCycleEx=0\n",
                "SelectMusicScoreboxCycleHardEx=0\n",
                "SelectMusicScoreboxCycleTournaments=0\n",
                "SelectMusicChartInfoPeakNps=1\n",
                "SelectMusicChartInfoEffectiveBpm=1\n",
                "SelectMusicChartInfoMatrixRating=0\n",
                "SeparateUnlocksByPlayer=1\n",
                "AutoScreenshotEval=PBs|Quads\n",
            ),
        );
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
    fn writes_stats_overlay_lines_with_frame_options() {
        let mut content = String::new();

        push_stats_overlay_option_lines(
            &mut content,
            StatsOverlayOptions {
                show_stats_mode: 7,
                frame_stats_overlay_anchor: Some("top-right"),
                frame_stats_overlay_style: Some("compact"),
                smooth_histogram: true,
                shade_scatterplot_judgments: false,
            },
        );

        assert_eq!(
            content,
            concat!(
                "ShowStats=1\n",
                "ShowStatsMode=3\n",
                "FrameStatsOverlayAnchor=top-right\n",
                "FrameStatsOverlayStyle=compact\n",
                "SmoothHistogram=1\n",
                "ShadeScatterplotJudgments=0\n",
            ),
        );
    }

    #[test]
    fn writes_stats_overlay_lines_without_frame_options() {
        let mut content = String::new();

        push_stats_overlay_option_lines(
            &mut content,
            StatsOverlayOptions {
                show_stats_mode: 0,
                frame_stats_overlay_anchor: None,
                frame_stats_overlay_style: None,
                smooth_histogram: false,
                shade_scatterplot_judgments: true,
            },
        );

        assert_eq!(
            content,
            concat!(
                "ShowStats=0\n",
                "ShowStatsMode=0\n",
                "SmoothHistogram=0\n",
                "ShadeScatterplotJudgments=1\n",
            ),
        );
    }

    #[test]
    fn writes_runtime_option_line_groups() {
        let mut content = String::new();
        let options = RuntimeOptions {
            fastload: true,
            cachesongs: false,
            song_parsing_threads: 6,
            smooth_histogram: true,
            shade_scatterplot_judgments: false,
            arcade_options_navigation: true,
            delayed_back: false,
            three_key_navigation: true,
            use_fsrs: false,
            lights_simplify_bass: true,
            only_dedicated_menu_buttons: false,
            theme_flag: ThemeFlag::SimplyLove,
            software_renderer_threads: 3,
        };

        push_runtime_cache_option_lines(&mut content, options);
        push_runtime_fastload_option_lines(&mut content, options);
        push_runtime_navigation_option_lines(&mut content, options);
        push_runtime_lights_option_lines(&mut content, options);
        push_runtime_menu_option_lines(&mut content, options);
        push_runtime_worker_theme_option_lines(&mut content, options);

        assert_eq!(
            content,
            concat!(
                "CacheSongs=0\n",
                "FastLoad=1\n",
                "ArcadeOptionsNavigation=1\n",
                "DelayedBack=0\n",
                "ThreeKeyNavigation=1\n",
                "UseFSRs=0\n",
                "LightsSimplifyBass=1\n",
                "OnlyDedicatedMenuButtons=0\n",
                "SongParsingThreads=6\n",
                "SoftwareRendererThreads=3\n",
                "Theme=Simply Love\n",
            ),
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
        assert_eq!(MAX_FPS_MAX, 2_500);
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

        assert_eq!(srpg_shop_folder_choice_index(SrpgShopFolder::Unlocks), 0);
        assert_eq!(srpg_shop_folder_from_choice(1), SrpgShopFolder::Shops);
        assert_eq!(srpg_shop_folder_from_choice(2), SrpgShopFolder::Faction);
        assert_eq!(srpg_shop_folder_from_choice(99), SrpgShopFolder::Unlocks);
    }
}
