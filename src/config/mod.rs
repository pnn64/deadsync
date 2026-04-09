mod audio;
pub mod dirs;
mod ini;
mod keybinds;
mod load;
#[path = "null_or_die.rs"]
mod null_or_die_cfg;
mod runtime;
mod store;
#[cfg(test)]
mod tests;
mod theme;
mod update;

pub use self::audio::{AudioMixLevels, AudioOutputMode, LinuxAudioBackend};
pub use self::ini::SimpleIni;
pub use self::keybinds::{
    update_keymap_binding_unique_gamepad, update_keymap_binding_unique_keyboard,
};
pub use self::load::{bootstrap_log_to_file, load};
pub use self::null_or_die_cfg::null_or_die_bias_cfg;
pub use self::runtime::{
    additional_song_folders, audio_mix_levels, flush_pending_saves, get, machine_default_noteskin,
};
pub use self::theme::{
    AUTO_SS_CLEARS, AUTO_SS_FAILS, AUTO_SS_FLAG_NAMES, AUTO_SS_NUM_FLAGS, AUTO_SS_PBS,
    AUTO_SS_QUADS, AUTO_SS_QUINTS, BreakdownStyle, DefaultFailType, GameFlag, LanguageFlag,
    LogLevel, MachinePreferredPlayMode, MachinePreferredPlayStyle, NewPackMode,
    SelectMusicItlWheelMode, SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement,
    SelectMusicWheelStyle, SyncGraphMode, ThemeFlag, auto_screenshot_bit,
    auto_screenshot_mask_from_str, auto_screenshot_mask_to_str,
};
pub use self::update::*;

use self::keybinds::{
    ALL_VIRTUAL_ACTIONS, action_to_ini_key, binding_to_token, load_keymap_from_ini_local,
};
use self::null_or_die_cfg::{
    clamp_null_or_die_confidence_percent, clamp_null_or_die_magic_offset_ms,
    clamp_null_or_die_positive_ms, null_or_die_kernel_target_str, null_or_die_kernel_type_str,
    parse_null_or_die_kernel_target, parse_null_or_die_kernel_type,
};
use self::runtime::{
    ADDITIONAL_SONG_FOLDERS, MACHINE_DEFAULT_NOTESKIN, lock_config, queue_save_write,
    sync_audio_mix_levels_from_config,
};
use self::store::{normalize_machine_default_noteskin, save_without_keymaps};
use crate::engine::gfx::{BackendType, PresentModePolicy};
use crate::engine::input::WindowsPadBackend;
use crate::engine::logging;
use log::{info, warn};
use null_or_die::{BiasCfg, BiasKernel, KernelTarget};
use std::str::FromStr;

const DEFAULT_MACHINE_NOTESKIN: &str = "cel";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenType {
    Exclusive,
    Borderless,
}

impl FullscreenType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exclusive => "Exclusive",
            Self::Borderless => "Borderless",
        }
    }
}

impl FromStr for FullscreenType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "exclusive" => Ok(Self::Exclusive),
            "borderless" => Ok(Self::Borderless),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Windowed,
    Fullscreen(FullscreenType),
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub vsync: bool,
    /// Stored MaxFPS cap value. `0` means "off".
    pub max_fps: u16,
    pub present_mode_policy: PresentModePolicy,
    pub windowed: bool,
    pub fullscreen_type: FullscreenType,
    pub display_monitor: usize,
    pub game_flag: GameFlag,
    pub theme_flag: ThemeFlag,
    pub language_flag: LanguageFlag,
    pub log_level: LogLevel,
    pub log_to_file: bool,
    /// Write the active screen name to save/current_screen.txt on each transition.
    pub write_current_screen: bool,
    /// 0=Off, 1=FPS, 2=FPS+Stutter.
    pub show_stats_mode: u8,
    pub translated_titles: bool,
    pub mine_hit_sound: bool,
    // Global background brightness during gameplay (ITGmania: Pref "BGBrightness").
    // 1.0 = full brightness, 0.0 = black.
    pub bg_brightness: f32,
    // ITGmania/Simply Love parity: center the active single-player notefield in gameplay.
    pub center_1player_notefield: bool,
    /// ITGmania-style wheel banner cache toggle.
    pub banner_cache: bool,
    /// Cache Select Music CDTitles as raw RGBA blobs on disk.
    pub cdtitle_cache: bool,
    pub display_width: u32,
    pub display_height: u32,
    pub video_renderer: BackendType,
    pub gfx_debug: bool,
    /// Windows-only: choose which gamepad backend to use.
    pub windows_gamepad_backend: WindowsPadBackend,
    // When using the Software video renderer:
    // 0 = Auto (use all logical cores)
    // 1 = Single-threaded
    // N >= 2 = cap at N threads (clamped to available cores).
    pub software_renderer_threads: u8,
    // When parsing simfiles at startup:
    // 0 = Auto (use all logical cores) for cache misses
    // 1 = Single-threaded
    // N >= 2 = cap at N threads (clamped to available cores).
    pub song_parsing_threads: u8,
    pub simply_love_color: i32,
    pub show_select_music_gameplay_timer: bool,
    pub show_select_music_banners: bool,
    pub show_select_music_video_banners: bool,
    pub show_select_music_breakdown: bool,
    pub show_select_music_cdtitles: bool,
    pub show_music_wheel_grades: bool,
    pub show_music_wheel_lamps: bool,
    pub select_music_itl_wheel_mode: SelectMusicItlWheelMode,
    /// Simply Love MusicWheelStyle parity: IIDX only shows the active pack when expanded.
    pub select_music_wheel_style: SelectMusicWheelStyle,
    pub select_music_new_pack_mode: NewPackMode,
    pub show_select_music_previews: bool,
    pub show_select_music_preview_marker: bool,
    pub select_music_preview_loop: bool,
    /// zmod parity: enable keyboard-only shortcuts like Ctrl+R restart.
    pub keyboard_features: bool,
    /// Enable or disable animated gameplay background videos.
    pub show_video_backgrounds: bool,
    /// Startup flow: show Select Profile before continuing.
    pub machine_show_select_profile: bool,
    /// Startup flow: show Select Color before continuing.
    pub machine_show_select_color: bool,
    /// Startup flow: show Select Style before continuing.
    pub machine_show_select_style: bool,
    /// Startup flow: show Select Play Mode before continuing.
    pub machine_show_select_play_mode: bool,
    /// Startup flow fallback style used when Select Style is disabled.
    pub machine_preferred_style: MachinePreferredPlayStyle,
    /// Startup flow fallback mode used when Select Play Mode is disabled.
    pub machine_preferred_play_mode: MachinePreferredPlayMode,
    /// Machine-wide replay recording and replay menu visibility.
    pub machine_enable_replays: bool,
    /// Post-session flow from Select Music/Course: show Evaluation Summary.
    pub machine_show_eval_summary: bool,
    /// Post-session flow from Select Music/Course: show Name Entry.
    pub machine_show_name_entry: bool,
    /// Post-session flow from Select Music/Course: show GameOver.
    pub machine_show_gameover: bool,
    /// zmod parity: gameplay/eval difficulty meter also displays text labels.
    pub zmod_rating_box_text: bool,
    /// Show one decimal place for live gameplay BPM when BPM is non-integer.
    pub show_bpm_decimal: bool,
    /// Machine default fail behavior (ITGmania DefaultFailType).
    pub default_fail_type: DefaultFailType,
    /// Choose which null-or-die sync graph the Select Music overlay displays.
    pub null_or_die_sync_graph: SyncGraphMode,
    /// Minimum confidence percent required for pack sync saves.
    pub null_or_die_confidence_percent: u8,
    /// Worker threads for null-or-die pack/all sync analysis.
    pub null_or_die_pack_sync_threads: u8,
    pub null_or_die_fingerprint_ms: f64,
    pub null_or_die_window_ms: f64,
    pub null_or_die_step_ms: f64,
    pub null_or_die_magic_offset_ms: f64,
    pub null_or_die_kernel_target: KernelTarget,
    pub null_or_die_kernel_type: BiasKernel,
    pub null_or_die_full_spectrogram: bool,
    pub select_music_breakdown_style: BreakdownStyle,
    pub select_music_pattern_info_mode: SelectMusicPatternInfoMode,
    pub show_select_music_scorebox: bool,
    pub select_music_scorebox_placement: SelectMusicScoreboxPlacement,
    pub select_music_scorebox_cycle_itg: bool,
    pub select_music_scorebox_cycle_ex: bool,
    pub select_music_scorebox_cycle_hard_ex: bool,
    pub select_music_scorebox_cycle_tournaments: bool,
    pub select_music_chart_info_peak_nps: bool,
    pub select_music_chart_info_matrix_rating: bool,
    pub show_random_courses: bool,
    pub show_most_played_courses: bool,
    pub show_course_individual_scores: bool,
    pub autosubmit_course_scores_individually: bool,
    pub global_offset_seconds: f32,
    pub visual_delay_seconds: f32,
    pub master_volume: u8,
    pub menu_music: bool,
    pub music_volume: u8,
    // ITGmania PrefsManager "MusicWheelSwitchSpeed" (default 15).
    pub music_wheel_switch_speed: u8,
    pub assist_tick_volume: u8,
    pub sfx_volume: u8,
    // None = auto (use the backend default output route); Some(N) = startup output-device index.
    pub audio_output_device_index: Option<u16>,
    pub audio_output_mode: AudioOutputMode,
    pub linux_audio_backend: LinuxAudioBackend,
    // None = auto (use device default sample rate)
    pub audio_sample_rate_hz: Option<u32>,
    pub auto_download_unlocks: bool,
    pub auto_populate_gs_scores: bool,
    pub rate_mod_preserves_pitch: bool,
    pub enable_arrowcloud: bool,
    pub enable_boogiestats: bool,
    pub enable_groovestats: bool,
    pub separate_unlocks_by_player: bool,
    pub fastload: bool,
    pub cachesongs: bool,
    // Whether to apply Gaussian smoothing to the eval histogram (Simply Love style)
    pub smooth_histogram: bool,
    /// Conditions for auto-screenshotting the Evaluation screen.
    pub auto_screenshot_eval: u8,
    /// ITGmania InputFilter parity: per-input debounce window in seconds.
    pub input_debounce_seconds: f32,
    /// StepMania parity: option menus use Start-to-advance arcade navigation.
    pub arcade_options_navigation: bool,
    /// ITGmania/Simply Love parity: use left/right/start style menu navigation.
    pub three_key_navigation: bool,
    /// When true, gameplay arrow buttons (p*_up/down/left/right) are excluded from
    /// menu navigation. Only explicitly-bound menu buttons (p*_menu_*) work in menus.
    pub only_dedicated_menu_buttons: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vsync: false,
            max_fps: 0,
            present_mode_policy: PresentModePolicy::Immediate,
            windowed: true,
            fullscreen_type: FullscreenType::Exclusive,
            display_monitor: 0,
            game_flag: GameFlag::Dance,
            theme_flag: ThemeFlag::SimplyLove,
            language_flag: LanguageFlag::English,
            log_level: LogLevel::Warn,
            log_to_file: true,
            write_current_screen: false,
            show_stats_mode: 0,
            translated_titles: false,
            mine_hit_sound: true,
            bg_brightness: 0.7,
            center_1player_notefield: false,
            banner_cache: true,
            cdtitle_cache: true,
            display_width: 1600,
            display_height: 900,
            video_renderer: BackendType::OpenGL,
            gfx_debug: false,
            windows_gamepad_backend: WindowsPadBackend::RawInput,
            software_renderer_threads: 1,
            song_parsing_threads: 0,
            simply_love_color: 2, // Corresponds to DEFAULT_COLOR_INDEX
            show_select_music_gameplay_timer: true,
            show_select_music_banners: true,
            show_select_music_video_banners: true,
            show_select_music_breakdown: true,
            show_select_music_cdtitles: true,
            show_music_wheel_grades: true,
            show_music_wheel_lamps: true,
            select_music_itl_wheel_mode: SelectMusicItlWheelMode::Score,
            select_music_wheel_style: SelectMusicWheelStyle::Itg,
            select_music_new_pack_mode: NewPackMode::Disabled,
            show_select_music_previews: true,
            show_select_music_preview_marker: false,
            select_music_preview_loop: true,
            keyboard_features: true,
            show_video_backgrounds: true,
            machine_show_select_profile: true,
            machine_show_select_color: true,
            machine_show_select_style: true,
            machine_show_select_play_mode: true,
            machine_preferred_style: MachinePreferredPlayStyle::Single,
            machine_preferred_play_mode: MachinePreferredPlayMode::Regular,
            machine_enable_replays: true,
            machine_show_eval_summary: true,
            machine_show_name_entry: true,
            machine_show_gameover: true,
            zmod_rating_box_text: false,
            show_bpm_decimal: false,
            default_fail_type: DefaultFailType::ImmediateContinue,
            null_or_die_sync_graph: SyncGraphMode::PostKernelFingerprint,
            null_or_die_confidence_percent: 80,
            null_or_die_pack_sync_threads: 0,
            null_or_die_fingerprint_ms: 50.0,
            null_or_die_window_ms: 10.0,
            null_or_die_step_ms: 0.2,
            null_or_die_magic_offset_ms: 0.0,
            null_or_die_kernel_target: KernelTarget::Digest,
            null_or_die_kernel_type: BiasKernel::Rising,
            null_or_die_full_spectrogram: false,
            select_music_breakdown_style: BreakdownStyle::Sl,
            select_music_pattern_info_mode: SelectMusicPatternInfoMode::Tech,
            show_select_music_scorebox: true,
            select_music_scorebox_placement: SelectMusicScoreboxPlacement::Auto,
            select_music_scorebox_cycle_itg: true,
            select_music_scorebox_cycle_ex: true,
            select_music_scorebox_cycle_hard_ex: true,
            select_music_scorebox_cycle_tournaments: true,
            select_music_chart_info_peak_nps: true,
            select_music_chart_info_matrix_rating: false,
            show_random_courses: true,
            show_most_played_courses: true,
            show_course_individual_scores: true,
            autosubmit_course_scores_individually: true,
            global_offset_seconds: -0.008,
            visual_delay_seconds: 0.0,
            master_volume: 90,
            menu_music: true,
            music_volume: 100,
            music_wheel_switch_speed: 15,
            assist_tick_volume: 100,
            sfx_volume: 100,
            audio_output_device_index: None,
            audio_output_mode: AudioOutputMode::Auto,
            linux_audio_backend: LinuxAudioBackend::Auto,
            audio_sample_rate_hz: None,
            auto_download_unlocks: false,
            auto_populate_gs_scores: false,
            rate_mod_preserves_pitch: true,
            enable_arrowcloud: false,
            enable_boogiestats: false,
            enable_groovestats: false,
            separate_unlocks_by_player: false,
            fastload: true,
            cachesongs: true,
            smooth_histogram: true,
            auto_screenshot_eval: 0,
            input_debounce_seconds: 0.02,
            arcade_options_navigation: false,
            three_key_navigation: false,
            only_dedicated_menu_buttons: false,
        }
    }
}

impl Config {
    pub const fn display_mode(&self) -> DisplayMode {
        if self.windowed {
            DisplayMode::Windowed
        } else {
            DisplayMode::Fullscreen(self.fullscreen_type)
        }
    }
}
