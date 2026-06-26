mod audio;
mod color;
mod ini;
mod keybinds;
mod load;
#[path = "null_or_die.rs"]
mod null_or_die_cfg;
mod pad_order;
mod runtime;
mod store;
#[cfg(test)]
mod tests;
mod theme;
mod update;

pub use self::audio::{AudioMixLevels, AudioOutputMode, LinuxAudioBackend};
pub use self::color::Color;
pub use self::ini::SimpleIni;
pub use self::keybinds::{
    clear_keymap_binding, update_keymap_binding_unique_gamepad,
    update_keymap_binding_unique_keyboard,
};
pub(crate) use self::keybinds::{
    editable_key_binding_slot_indices, keycode_to_token, parse_keycode_to_key,
    protected_default_key_for_action,
};
pub use self::load::{bootstrap_log_to_file, bootstrap_show_console, load};
pub use self::null_or_die_cfg::null_or_die_bias_cfg;
pub use self::pad_order::pad_index_for_uuid;
pub use self::runtime::{
    additional_song_folder_roots, audio_mix_levels, default_profiles, flush_pending_saves, get,
    group_is_never_cached, machine_default_noteskin, never_cache_list, smx_pad_assignment,
    song_path_is_writable,
};
pub use self::theme::{
    AUTO_SS_CLEARS, AUTO_SS_FAILS, AUTO_SS_FLAG_NAMES, AUTO_SS_NUM_FLAGS, AUTO_SS_PBS,
    AUTO_SS_QUADS, AUTO_SS_QUINTS, ArrowCloudQrLoginWhen, BreakdownStyle, DefaultFailType,
    DefaultSyncOffset, GameFlag, GrooveStatsQrLoginWhen, LanguageFlag, LogLevel,
    MACHINE_FONT_VARIANTS, MachineBarColor, MachineEvaluationStyle, MachineFont,
    MachinePreferredPlayMode, MachinePreferredPlayStyle, NewPackMode, RandomBackgroundMode,
    SelectMusicItlRankMode, SelectMusicItlWheelMode, SelectMusicPatternInfoMode,
    SelectMusicScoreboxPlacement, SelectMusicSongSelectBgMode, SelectMusicStepArtistBoxMode,
    SelectMusicWheelStyle, SrpgVariant, SyncGraphMode, ThemeFlag, VersionOverlaySide, VisualStyle,
    auto_screenshot_bit, auto_screenshot_mask_from_str, auto_screenshot_mask_to_str,
};
pub use self::update::*;
pub use deadlib_platform::display::FullscreenType;

use self::keybinds::{
    ALL_VIRTUAL_ACTIONS, action_to_ini_key, binding_to_token, load_keymap_from_ini_local,
};
use self::null_or_die_cfg::{
    clamp_null_or_die_confidence_percent, clamp_null_or_die_magic_offset_ms,
    clamp_null_or_die_positive_ms, null_or_die_kernel_target_str, null_or_die_kernel_type_str,
    parse_null_or_die_kernel_target, parse_null_or_die_kernel_type,
};
use self::runtime::{
    ADDITIONAL_SONG_FOLDERS, DEFAULT_PROFILE_P1, DEFAULT_PROFILE_P2, MACHINE_DEFAULT_NOTESKIN,
    NEVER_CACHE_LIST, SMX_P1_SERIAL, SMX_P2_SERIAL, lock_config, queue_save_write,
    sync_audio_mix_levels_from_config,
};
use self::store::{normalize_machine_default_noteskin, save_without_keymaps};
use deadlib_platform::logging;
use deadlib_render::{BackendType, PresentModePolicy};
pub use deadsync_input_native::PadOrderBackend;
use deadsync_input_native::WindowsPadBackend;
use deadsync_lights::{DriverKind as LightsDriverKind, GameplayPadLightMode, SerialPortName};
pub use deadsync_smx::SmxPadPreset;
use log::{info, warn};
use null_or_die::{BiasCfg, BiasKernel, KernelTarget};
use std::str::FromStr;
use winit::keyboard::KeyCode;

const DEFAULT_MACHINE_NOTESKIN: &str = "cel";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdditionalSongFolder {
    pub path: String,
    pub writable: bool,
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
    /// Windows: open a console window for live log output. Off by default so the
    /// game launches cleanly with no stray terminal. Ignored on other platforms,
    /// which always inherit their controlling terminal. Applied at startup.
    pub show_console: bool,
    /// Write the active screen name to save/current_screen.txt on each transition.
    pub write_current_screen: bool,
    /// Hold-Tab fast-forward (4×) for non-gameplay screens. Issue #174 / ITGmania parity.
    /// Hold ` for slow (0.25×); both held = halt. Always disabled in Gameplay.
    pub tab_acceleration: bool,
    /// 0=Off, 1=FPS, 2=FPS+Stutter.
    pub show_stats_mode: u8,
    /// Last frame-statistics overlay corner (`OverlayAnchor::to_key`, e.g. "bottom-right"),
    /// or "auto" = follow play context until the user moves it. Remembered across sessions.
    pub frame_stats_overlay_anchor: &'static str,
    /// Frame-statistics overlay presentation style (`OverlayStyle::label`): "detailed" or
    /// "minimal". Remembered across sessions.
    pub frame_stats_overlay_style: &'static str,
    pub translated_titles: bool,
    pub mine_hit_sound: bool,
    // Global background brightness during gameplay (ITGmania: Pref "BGBrightness").
    // 1.0 = full brightness, 0.0 = black.
    pub bg_brightness: f32,
    // Gameplay backdrop color matching the Simply Love ScreenGameplay underlay
    // quad. Non-black values draw over song art and below notefield/HUD actors.
    // Parsed from a `#RRGGBB` hex string in `deadsync.ini` (key
    // `GameplayBgColor`). Default black preserves the standard song-background
    // brightness behavior.
    pub gameplay_bg_color: Color,
    // ITGmania/Simply Love parity: center the active single-player notefield in gameplay.
    pub center_1player_notefield: bool,
    /// ITGmania-style wheel banner cache toggle.
    pub banner_cache: bool,
    /// Cache Select Music CDTitles as raw RGBA blobs on disk.
    pub cdtitle_cache: bool,
    pub display_width: u32,
    pub display_height: u32,
    /// Overscan adjustment (CenterImage). Values are in
    /// physical window pixels and scale/translate the entire rendered image so
    /// content cut off by display overscan can be pulled back into view.
    pub center_image_translate_x: i32,
    pub center_image_translate_y: i32,
    pub center_image_add_width: i32,
    pub center_image_add_height: i32,
    pub video_renderer: BackendType,
    /// Native high-DPI/Retina rendering. Currently affects macOS OpenGL only.
    pub high_dpi: bool,
    /// Hide the OS mouse cursor while it is inside the DeadSync window.
    pub hide_mouse_cursor: bool,
    pub gfx_debug: bool,
    /// Enable a "Shutdown" entry on the main menu that powers off the host
    /// machine. Off by default; intended for cabinet use.
    pub allow_shutdown_host: bool,
    /// Windows-only: choose which gamepad backend to use.
    pub windows_gamepad_backend: WindowsPadBackend,
    /// Enable StepManiaX pad input via the RustManiaX SDK (all platforms).
    pub smx_input: bool,
    /// When true, DeadSync resolves and writes a pad config to each connected
    /// SMX pad (this pad's saved default → a global default → the built-in
    /// `smx_default_pad_config` preset). See `App::apply_smx_managed_preset`.
    pub smx_manages_pad_config: bool,
    /// Light SMX pad panels with the per-arrow judgement colour during gameplay,
    /// plus a sustained colour for held freezes and rolls.
    pub smx_panel_lights: bool,
    /// Set the SMX pad edge underglow LEDs to the player's theme colour.
    pub smx_underglow_theme: bool,
    /// Built-in pad preset flashed as the fallback when DeadSync manages pad
    /// config and no saved config resolves for the pad.
    pub smx_default_pad_config: SmxPadPreset,
    /// Machine-default pad-light brightness (0..=100). Seeds each new player
    /// profile's `pad_light_brightness`; players then adjust their own value in
    /// Player Options. Applied to every RGB byte deadsync sends to the pad.
    pub smx_default_light_brightness: u8,
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
    pub show_select_music_stage_display: bool,
    pub show_select_music_banners: bool,
    pub show_select_music_video_banners: bool,
    pub show_select_music_breakdown: bool,
    pub show_select_music_cdtitles: bool,
    pub show_music_wheel_grades: bool,
    pub show_music_wheel_lamps: bool,
    pub select_music_itl_rank_mode: SelectMusicItlRankMode,
    pub select_music_itl_wheel_mode: SelectMusicItlWheelMode,
    /// Simply Love MusicWheelStyle parity: IIDX only shows the active pack when expanded.
    pub select_music_wheel_style: SelectMusicWheelStyle,
    /// Arrow Cloud SongSelectBG parity: show song/pack art behind wheel rows.
    pub select_music_song_select_bg_mode: SelectMusicSongSelectBgMode,
    pub select_music_new_pack_mode: NewPackMode,
    /// Arrow Cloud FolderStats parity: pack clear summary box on Select Music.
    pub show_select_music_folder_stats: bool,
    pub show_select_music_previews: bool,
    pub show_select_music_preview_marker: bool,
    pub select_music_preview_loop: bool,
    /// zmod parity: enable keyboard-only shortcuts like Ctrl+R restart in gameplay/evaluation.
    pub keyboard_features: bool,
    /// Show a small build-version watermark in the bottom-right corner of
    /// every screen so the running version is visible in any
    /// screenshot/video. Default on; disablable via the Options menu.
    pub show_version_overlay: bool,
    /// Which side of the screen the version watermark anchors to. Stored
    /// separately from `show_version_overlay` so toggling visibility
    /// doesn't forget the preferred side.
    pub version_overlay_side: VersionOverlaySide,
    /// Simply Love visual style used by shared menu art.
    pub visual_style: VisualStyle,
    /// Variant used when the SRPG visual-style family is selected.
    pub srpg_variant: SrpgVariant,
    /// Enable or disable animated gameplay background videos.
    pub show_video_backgrounds: bool,
    /// ITGmania RandomBackgroundMode. DeadSync currently implements RandomMovies.
    pub random_background_mode: RandomBackgroundMode,
    /// Startup flow: show Select Profile before continuing.
    pub machine_show_select_profile: bool,
    /// Whether "Switch Profile" appears in the select music sort menu.
    pub allow_switch_profile_in_menu: bool,
    /// Select Music keyboard shortcut: open Practice Mode for the selected song.
    pub music_select_shortcut_practice: KeyCode,
    /// Select Music keyboard shortcut: open the Song Search prompt.
    pub music_select_shortcut_song_search: KeyCode,
    /// Select Music keyboard shortcut: reload songs & courses ("Load New Songs").
    pub music_select_shortcut_load_songs: KeyCode,
    /// Select Music keyboard shortcut: open the Test Input overlay.
    pub music_select_shortcut_test_input: KeyCode,
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
    /// Machine font for Bold/Header/Footer/numbers/ScreenEval roles.
    /// Default `Wendy` keeps Wendy; `Mega` swaps those roles to Mega.
    /// Body text (Normal role) stays Miso regardless.
    pub machine_font: MachineFont,
    /// Machine-wide screen bar color behavior.
    /// Default preserves each screen's current bar background choice.
    pub machine_bar_color: MachineBarColor,
    /// Machine-wide evaluation quad opacity behavior.
    /// Default follows the selected visual style.
    pub machine_evaluation_style: MachineEvaluationStyle,
    /// Machine-wide replay recording and replay menu visibility.
    pub machine_enable_replays: bool,
    /// Allow players to add a personal timing shift on top of machine global offset.
    pub machine_allow_per_player_global_offsets: bool,
    /// Apply ITGmania Pack.ini SyncOffset values to gameplay timing.
    pub machine_pack_ini_offsets: bool,
    /// Sync offset to assume for packs without a Pack.ini SyncOffset value.
    pub machine_default_sync_offset: DefaultSyncOffset,
    /// Post-session flow from Select Music/Course: show Evaluation Summary.
    pub machine_show_eval_summary: bool,
    /// Evaluation easter egg: play the "nice" clip when a score contains 69.
    pub machine_nice_sound: bool,
    /// Post-session flow from Select Music/Course: show Name Entry.
    pub machine_show_name_entry: bool,
    /// Post-session flow from Select Music/Course: show GameOver.
    pub machine_show_gameover: bool,
    /// zmod parity: gameplay/eval difficulty meter also displays text labels.
    pub zmod_rating_box_text: bool,
    /// Show one decimal place for live gameplay BPM when BPM is non-integer.
    pub show_bpm_decimal: bool,
    /// Require holding Back to leave gameplay instead of exiting on first press.
    pub delayed_back: bool,
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
    pub select_music_step_artist_box_mode: SelectMusicStepArtistBoxMode,
    pub show_select_music_scorebox: bool,
    pub select_music_scorebox_placement: SelectMusicScoreboxPlacement,
    pub select_music_scorebox_cycle_itg: bool,
    pub select_music_scorebox_cycle_ex: bool,
    pub select_music_scorebox_cycle_hard_ex: bool,
    pub select_music_scorebox_cycle_tournaments: bool,
    pub select_music_chart_info_peak_nps: bool,
    pub select_music_chart_info_effective_bpm: bool,
    pub select_music_chart_info_matrix_rating: bool,
    pub show_random_courses: bool,
    pub show_most_played_courses: bool,
    pub show_course_individual_scores: bool,
    pub autosubmit_course_scores_individually: bool,
    pub global_offset_seconds: f32,
    pub visual_delay_seconds: f32,
    pub master_volume: u8,
    pub menu_music: bool,
    pub custom_sounds_enabled: bool,
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
    /// Allows the in-app updater to download and install updates.
    /// Disable this for builds distributed through a channel that owns
    /// updates itself, such as a package manager or storefront.
    pub updater_install_enabled: bool,
    pub rate_mod_preserves_pitch: bool,
    /// Experimental: apply ReplayGain 2.0 / EBU R 128 loudness normalization
    /// to music playback. Loudness is computed in the background and cached
    /// on disk per song.
    pub enable_replaygain: bool,
    pub enable_arrowcloud: bool,
    pub enable_boogiestats: bool,
    pub enable_groovestats: bool,
    pub submit_arrowcloud_fails: bool,
    /// When to auto-show the ArrowCloud QR-login screen after Select
    /// Profile.  Mirrors Simply Love's `QRLogin` theme pref.
    pub arrowcloud_qr_login_when: ArrowCloudQrLoginWhen,
    /// When to auto-show the GrooveStats QR-login screen after Select
    /// Profile.  Mirrors Simply Love's `QRLogin` theme pref.
    pub groovestats_qr_login_when: GrooveStatsQrLoginWhen,
    pub separate_unlocks_by_player: bool,
    pub fastload: bool,
    pub cachesongs: bool,
    // Whether to apply Gaussian smoothing to the eval histogram (Simply Love style)
    pub smooth_histogram: bool,
    /// Tint the evaluation scatterplot background in horizontal bands matching
    /// the active scoring scale's judgment timing windows. Mirrors the
    /// Simply-Love-SM5-8ms judgment-region shading; off by default to preserve
    /// the existing solid background.
    pub shade_scatterplot_judgments: bool,
    /// Conditions for auto-screenshotting the Evaluation screen.
    pub auto_screenshot_eval: u8,
    /// ITGmania InputFilter parity: per-input debounce window in seconds.
    pub input_debounce_seconds: f32,
    /// StepMania parity: option menus use Start-to-advance arcade navigation.
    pub arcade_options_navigation: bool,
    /// ITGmania/Simply Love parity: use left/right/start style menu navigation.
    pub three_key_navigation: bool,
    /// Enable direct FSR device diagnostics in Test Input for supported controllers.
    pub use_fsrs: bool,
    /// Native cabinet/pad light output driver.
    pub lights_driver: LightsDriverKind,
    /// Source for gameplay arrow pad lights.
    pub lights_gameplay_pad_lights: GameplayPadLightMode,
    /// ITGmania parity: bass lights use quarter-note chart rows only.
    pub lights_simplify_bass: bool,
    /// Serial port used by the Litboard/Win32Serial/Sextet lights drivers.
    pub lights_com_port: SerialPortName,
    /// When true, gameplay arrow buttons (p*_up/down/left/right) are excluded from
    /// menu navigation. Only explicitly-bound menu buttons (p*_menu_*) work in menus.
    pub only_dedicated_menu_buttons: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vsync: false,
            max_fps: 0,
            present_mode_policy: PresentModePolicy::Mailbox,
            windowed: true,
            fullscreen_type: FullscreenType::Exclusive,
            display_monitor: 0,
            game_flag: GameFlag::Dance,
            theme_flag: ThemeFlag::SimplyLove,
            language_flag: LanguageFlag::Auto,
            log_level: LogLevel::Warn,
            log_to_file: true,
            show_console: false,
            write_current_screen: false,
            tab_acceleration: true,
            show_stats_mode: 0,
            frame_stats_overlay_anchor: "auto",
            frame_stats_overlay_style: "detailed",
            translated_titles: false,
            mine_hit_sound: true,
            bg_brightness: 0.7,
            gameplay_bg_color: Color::BLACK,
            center_1player_notefield: false,
            banner_cache: true,
            cdtitle_cache: true,
            display_width: 1600,
            display_height: 900,
            center_image_translate_x: 0,
            center_image_translate_y: 0,
            center_image_add_width: 0,
            center_image_add_height: 0,
            video_renderer: BackendType::OpenGL,
            high_dpi: false,
            hide_mouse_cursor: true,
            gfx_debug: false,
            allow_shutdown_host: false,
            windows_gamepad_backend: WindowsPadBackend::RawInput,
            smx_input: false,
            smx_manages_pad_config: false,
            smx_panel_lights: false,
            smx_underglow_theme: false,
            smx_default_pad_config: SmxPadPreset::Low,
            smx_default_light_brightness: 100,
            software_renderer_threads: 1,
            song_parsing_threads: 0,
            simply_love_color: 2, // Corresponds to DEFAULT_COLOR_INDEX
            show_select_music_gameplay_timer: true,
            show_select_music_stage_display: true,
            show_select_music_banners: true,
            show_select_music_video_banners: true,
            show_select_music_breakdown: true,
            show_select_music_cdtitles: true,
            show_music_wheel_grades: true,
            show_music_wheel_lamps: true,
            select_music_itl_rank_mode: SelectMusicItlRankMode::None,
            select_music_itl_wheel_mode: SelectMusicItlWheelMode::Score,
            select_music_wheel_style: SelectMusicWheelStyle::Itg,
            select_music_song_select_bg_mode: SelectMusicSongSelectBgMode::Off,
            select_music_new_pack_mode: NewPackMode::Disabled,
            show_select_music_folder_stats: false,
            show_select_music_previews: true,
            show_select_music_preview_marker: false,
            select_music_preview_loop: true,
            keyboard_features: true,
            show_version_overlay: true,
            version_overlay_side: VersionOverlaySide::Right,
            visual_style: VisualStyle::Hearts,
            srpg_variant: SrpgVariant::Srpg9,
            show_video_backgrounds: true,
            random_background_mode: RandomBackgroundMode::Off,
            machine_show_select_profile: true,
            allow_switch_profile_in_menu: false,
            music_select_shortcut_practice: KeyCode::KeyP,
            music_select_shortcut_song_search: KeyCode::KeyS,
            music_select_shortcut_load_songs: KeyCode::KeyL,
            music_select_shortcut_test_input: KeyCode::KeyT,
            machine_show_select_color: true,
            machine_show_select_style: true,
            machine_show_select_play_mode: true,
            machine_preferred_style: MachinePreferredPlayStyle::Single,
            machine_preferred_play_mode: MachinePreferredPlayMode::Regular,
            machine_font: MachineFont::Wendy,
            machine_bar_color: MachineBarColor::Default,
            machine_evaluation_style: MachineEvaluationStyle::Default,
            delayed_back: true,
            machine_enable_replays: true,
            machine_allow_per_player_global_offsets: false,
            machine_pack_ini_offsets: false,
            machine_default_sync_offset: DefaultSyncOffset::Null,
            machine_show_eval_summary: true,
            machine_nice_sound: true,
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
            select_music_step_artist_box_mode: SelectMusicStepArtistBoxMode::Default,
            show_select_music_scorebox: true,
            select_music_scorebox_placement: SelectMusicScoreboxPlacement::Auto,
            select_music_scorebox_cycle_itg: true,
            select_music_scorebox_cycle_ex: true,
            select_music_scorebox_cycle_hard_ex: true,
            select_music_scorebox_cycle_tournaments: true,
            select_music_chart_info_peak_nps: true,
            select_music_chart_info_effective_bpm: false,
            select_music_chart_info_matrix_rating: false,
            show_random_courses: true,
            show_most_played_courses: true,
            show_course_individual_scores: true,
            autosubmit_course_scores_individually: true,
            global_offset_seconds: -0.008,
            visual_delay_seconds: 0.0,
            master_volume: 90,
            menu_music: true,
            custom_sounds_enabled: true,
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
            updater_install_enabled: true,
            rate_mod_preserves_pitch: true,
            enable_replaygain: false,
            enable_arrowcloud: false,
            enable_boogiestats: false,
            enable_groovestats: false,
            submit_arrowcloud_fails: false,
            arrowcloud_qr_login_when: ArrowCloudQrLoginWhen::Sometimes,
            groovestats_qr_login_when: GrooveStatsQrLoginWhen::Sometimes,
            separate_unlocks_by_player: false,
            fastload: true,
            cachesongs: true,
            smooth_histogram: true,
            shade_scatterplot_judgments: false,
            auto_screenshot_eval: 0,
            input_debounce_seconds: 0.02,
            arcade_options_navigation: false,
            three_key_navigation: false,
            use_fsrs: false,
            lights_driver: LightsDriverKind::Off,
            lights_gameplay_pad_lights: GameplayPadLightMode::Input,
            lights_simplify_bass: false,
            lights_com_port: SerialPortName::default(),
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
