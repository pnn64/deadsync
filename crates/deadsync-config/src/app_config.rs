use crate::audio::AudioOptions;
use crate::defaults::*;
use crate::null_or_die::NullOrDieOptions;
use crate::options::{RuntimeOptions, SelectMusicOptions, SmxPackName, SystemOptions};
use crate::theme::{
    ArrowCloudQrLoginWhen, BreakdownStyle, DefaultFailType, DefaultSyncOffset, GameFlag,
    GameplayBpmPosition, GrooveStatsQrLoginWhen, LanguageFlag, LogLevel, MachineBarColor,
    MachineEvaluationStyle, MachineFlowOptions, MachineFont, MachinePreferredPlayMode,
    MachinePreferredPlayStyle, NewPackMode, RandomBackgroundMode, SelectMusicItlRankMode,
    SelectMusicItlWheelMode, SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement,
    SelectMusicSongSelectBgMode, SelectMusicStepArtistBoxMode, SelectMusicWheelStyle,
    SrpgShopFolder, SrpgVariant, SyncGraphMode, ThemeFlag, ThemePresentationOptions,
    VersionOverlaySide, VisualStyle,
};
use deadlib_platform::display::FullscreenType;
use deadlib_present::color::Color;
use deadlib_render::{BackendType, PresentModePolicy};
use deadsync_audio::{AudioOutputMode, LinuxAudioBackend};
use deadsync_input_native::WindowsPadBackend;
use deadsync_lights::{DriverKind as LightsDriverKind, GameplayPadLightMode, SerialPortName};
use deadsync_smx::SmxPadPreset;
use null_or_die::{BiasKernel, KernelTarget};
use winit::keyboard::KeyCode;

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
    /// Hold-Tab fast-forward (4x) for non-gameplay screens. Issue #174 / ITGmania parity.
    /// Hold ` for slow (0.25x); both held = halt. Always disabled in Gameplay.
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
    /// SMX pad (this pad's saved default -> a global default -> the built-in
    /// `smx_default_pad_config` preset). See `App::apply_smx_managed_preset`.
    pub smx_manages_pad_config: bool,
    /// Drive SMX pad panel LEDs with GIF animations (backgrounds, judgement
    /// effects, press feedback). While on, the game owns the LEDs and the
    /// pad's own firmware lighting is suppressed.
    pub smx_panel_lights: bool,
    /// User animation pack supplying the pad backgrounds (a directory under
    /// `assets/smx-pad-lights/dance/`). Empty selects the built-in set.
    pub smx_pad_gifs_pack: SmxPackName,
    /// User animation pack supplying the judgement GIFs (a directory under
    /// `assets/smx-judge-lights/dance/`). Empty selects the built-in set.
    pub smx_judge_gifs_pack: SmxPackName,
    /// Set the SMX pad edge underglow LEDs to the player's theme colour.
    pub smx_underglow_theme: bool,
    /// Send underglow strip colours in GRB wire order instead of RGB, for
    /// strip hardware that consumes WS2812 channel order (symptom when wrong:
    /// red shows as green, purple as cyan; yellow and blue look correct).
    pub smx_underglow_grb: bool,
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
    /// Start Select Music in the nested Series sort instead of Group sort.
    pub sort_music_wheel_by_series: bool,
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
    pub select_music_preview_starts_immediately: bool,
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
    /// Evaluation easter egg: show and play "nice" when a score contains 69.
    pub machine_nice_sound: bool,
    /// Post-session flow from Select Music/Course: show Name Entry.
    pub machine_show_name_entry: bool,
    /// Post-session flow from Select Music/Course: show GameOver.
    pub machine_show_gameover: bool,
    /// zmod parity: gameplay/eval difficulty meter also displays text labels.
    pub zmod_rating_box_text: bool,
    /// Show one decimal place for live gameplay BPM when BPM is non-integer.
    pub show_bpm_decimal: bool,
    /// Where to place the live gameplay BPM display.
    pub gameplay_bpm_position: GameplayBpmPosition,
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
    pub show_srpg_shop: bool,
    pub srpg_shop_folder: SrpgShopFolder,
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
        let system = SystemOptions::default();
        let theme = ThemePresentationOptions::default();
        let machine = MachineFlowOptions::default();
        let null_or_die = NullOrDieOptions::default();
        let select_music = SelectMusicOptions::default();
        let audio = AudioOptions::default();
        let runtime = RuntimeOptions::default();

        Self {
            vsync: DEFAULT_VSYNC,
            max_fps: DEFAULT_MAX_FPS,
            present_mode_policy: PresentModePolicy::Mailbox,
            windowed: DEFAULT_WINDOWED,
            fullscreen_type: FullscreenType::Exclusive,
            display_monitor: DEFAULT_DISPLAY_MONITOR,
            game_flag: system.game_flag,
            theme_flag: runtime.theme_flag,
            language_flag: system.language_flag,
            log_level: system.log_level,
            log_to_file: system.log_to_file,
            show_console: system.show_console,
            write_current_screen: audio.write_current_screen,
            tab_acceleration: audio.tab_acceleration,
            show_stats_mode: system.show_stats_mode,
            frame_stats_overlay_anchor: system.frame_stats_overlay_anchor,
            frame_stats_overlay_style: system.frame_stats_overlay_style,
            translated_titles: system.translated_titles,
            mine_hit_sound: system.mine_hit_sound,
            bg_brightness: system.bg_brightness,
            gameplay_bg_color: Color::BLACK,
            center_1player_notefield: system.center_1player_notefield,
            banner_cache: system.banner_cache,
            cdtitle_cache: system.cdtitle_cache,
            display_width: DEFAULT_DISPLAY_WIDTH,
            display_height: DEFAULT_DISPLAY_HEIGHT,
            center_image_translate_x: system.center_image_translate_x,
            center_image_translate_y: system.center_image_translate_y,
            center_image_add_width: system.center_image_add_width,
            center_image_add_height: system.center_image_add_height,
            video_renderer: BackendType::OpenGL,
            high_dpi: system.high_dpi,
            hide_mouse_cursor: system.hide_mouse_cursor,
            gfx_debug: system.gfx_debug,
            allow_shutdown_host: system.allow_shutdown_host,
            windows_gamepad_backend: WindowsPadBackend::RawInput,
            smx_input: system.smx_input,
            smx_manages_pad_config: system.smx_manages_pad_config,
            smx_panel_lights: system.smx_panel_lights,
            smx_pad_gifs_pack: system.smx_pad_gifs_pack,
            smx_judge_gifs_pack: system.smx_judge_gifs_pack,
            smx_underglow_theme: system.smx_underglow_theme,
            smx_underglow_grb: system.smx_underglow_grb,
            smx_default_pad_config: SmxPadPreset::Low,
            smx_default_light_brightness: DEFAULT_SMX_DEFAULT_LIGHT_BRIGHTNESS,
            software_renderer_threads: runtime.software_renderer_threads,
            song_parsing_threads: runtime.song_parsing_threads,
            simply_love_color: theme.simply_love_color,
            show_select_music_gameplay_timer: theme.show_select_music_gameplay_timer,
            show_select_music_stage_display: select_music.show_stage_display,
            show_select_music_banners: select_music.show_banners,
            show_select_music_video_banners: select_music.show_video_banners,
            show_select_music_breakdown: select_music.show_breakdown,
            show_select_music_cdtitles: select_music.show_cdtitles,
            show_music_wheel_grades: select_music.show_wheel_grades,
            show_music_wheel_lamps: select_music.show_wheel_lamps,
            sort_music_wheel_by_series: select_music.sort_wheel_by_series,
            select_music_itl_rank_mode: select_music.itl_rank_mode,
            select_music_itl_wheel_mode: select_music.itl_wheel_mode,
            select_music_wheel_style: select_music.wheel_style,
            select_music_song_select_bg_mode: select_music.song_select_bg_mode,
            select_music_new_pack_mode: select_music.new_pack_mode,
            show_select_music_folder_stats: select_music.show_folder_stats,
            show_select_music_previews: select_music.show_previews,
            show_select_music_preview_marker: select_music.show_preview_marker,
            select_music_preview_loop: select_music.preview_loop,
            select_music_preview_starts_immediately: select_music.preview_starts_immediately,
            keyboard_features: theme.keyboard_features,
            show_version_overlay: select_music.show_version_overlay,
            version_overlay_side: select_music.version_overlay_side,
            visual_style: theme.visual_style,
            srpg_variant: theme.srpg_variant,
            show_video_backgrounds: theme.show_video_backgrounds,
            random_background_mode: theme.random_background_mode,
            machine_show_select_profile: machine.machine_show_select_profile,
            allow_switch_profile_in_menu: machine.allow_switch_profile_in_menu,
            music_select_shortcut_practice: KeyCode::KeyP,
            music_select_shortcut_song_search: KeyCode::KeyS,
            music_select_shortcut_load_songs: KeyCode::KeyL,
            music_select_shortcut_test_input: KeyCode::KeyT,
            machine_show_select_color: machine.machine_show_select_color,
            machine_show_select_style: machine.machine_show_select_style,
            machine_show_select_play_mode: machine.machine_show_select_play_mode,
            machine_preferred_style: machine.machine_preferred_style,
            machine_preferred_play_mode: machine.machine_preferred_play_mode,
            machine_font: machine.machine_font,
            machine_bar_color: machine.machine_bar_color,
            machine_evaluation_style: machine.machine_evaluation_style,
            delayed_back: runtime.delayed_back,
            machine_enable_replays: machine.machine_enable_replays,
            machine_allow_per_player_global_offsets: machine
                .machine_allow_per_player_global_offsets,
            machine_pack_ini_offsets: machine.machine_pack_ini_offsets,
            machine_default_sync_offset: machine.machine_default_sync_offset,
            machine_show_eval_summary: machine.machine_show_eval_summary,
            machine_nice_sound: machine.machine_nice_sound,
            machine_show_name_entry: machine.machine_show_name_entry,
            machine_show_gameover: machine.machine_show_gameover,
            zmod_rating_box_text: theme.zmod_rating_box_text,
            show_bpm_decimal: theme.show_bpm_decimal,
            gameplay_bpm_position: theme.gameplay_bpm_position,
            default_fail_type: system.default_fail_type,
            null_or_die_sync_graph: null_or_die.sync_graph,
            null_or_die_confidence_percent: null_or_die.confidence_percent,
            null_or_die_pack_sync_threads: null_or_die.pack_sync_threads,
            null_or_die_fingerprint_ms: null_or_die.fingerprint_ms,
            null_or_die_window_ms: null_or_die.window_ms,
            null_or_die_step_ms: null_or_die.step_ms,
            null_or_die_magic_offset_ms: null_or_die.magic_offset_ms,
            null_or_die_kernel_target: null_or_die.kernel_target,
            null_or_die_kernel_type: null_or_die.kernel_type,
            null_or_die_full_spectrogram: null_or_die.full_spectrogram,
            select_music_breakdown_style: select_music.breakdown_style,
            select_music_pattern_info_mode: select_music.pattern_info_mode,
            select_music_step_artist_box_mode: select_music.step_artist_box_mode,
            show_select_music_scorebox: select_music.show_scorebox,
            select_music_scorebox_placement: select_music.scorebox_placement,
            select_music_scorebox_cycle_itg: select_music.scorebox_cycle_itg,
            select_music_scorebox_cycle_ex: select_music.scorebox_cycle_ex,
            select_music_scorebox_cycle_hard_ex: select_music.scorebox_cycle_hard_ex,
            select_music_scorebox_cycle_tournaments: select_music.scorebox_cycle_tournaments,
            select_music_chart_info_peak_nps: select_music.chart_info_peak_nps,
            select_music_chart_info_effective_bpm: select_music.chart_info_effective_bpm,
            select_music_chart_info_matrix_rating: select_music.chart_info_matrix_rating,
            show_random_courses: system.show_random_courses,
            show_most_played_courses: system.show_most_played_courses,
            show_course_individual_scores: system.show_course_individual_scores,
            autosubmit_course_scores_individually: system.autosubmit_course_scores_individually,
            global_offset_seconds: system.global_offset_seconds,
            visual_delay_seconds: audio.visual_delay_seconds,
            master_volume: audio.master_volume,
            menu_music: audio.menu_music,
            custom_sounds_enabled: audio.custom_sounds_enabled,
            music_volume: audio.music_volume,
            music_wheel_switch_speed: audio.music_wheel_switch_speed,
            assist_tick_volume: audio.assist_tick_volume,
            sfx_volume: audio.sfx_volume,
            audio_output_device_index: audio.output_device_index,
            audio_output_mode: AudioOutputMode::Auto,
            linux_audio_backend: LinuxAudioBackend::Auto,
            audio_sample_rate_hz: audio.sample_rate_hz,
            auto_download_unlocks: system.auto_download_unlocks,
            auto_populate_gs_scores: system.auto_populate_gs_scores,
            updater_install_enabled: system.updater_install_enabled,
            rate_mod_preserves_pitch: audio.rate_mod_preserves_pitch,
            enable_replaygain: audio.enable_replaygain,
            enable_arrowcloud: system.enable_arrowcloud,
            enable_boogiestats: system.enable_boogiestats,
            enable_groovestats: system.enable_groovestats,
            show_srpg_shop: system.show_srpg_shop,
            srpg_shop_folder: system.srpg_shop_folder,
            submit_arrowcloud_fails: system.submit_arrowcloud_fails,
            arrowcloud_qr_login_when: system.arrowcloud_qr_login_when,
            groovestats_qr_login_when: system.groovestats_qr_login_when,
            separate_unlocks_by_player: system.separate_unlocks_by_player,
            fastload: runtime.fastload,
            cachesongs: runtime.cachesongs,
            smooth_histogram: runtime.smooth_histogram,
            shade_scatterplot_judgments: runtime.shade_scatterplot_judgments,
            auto_screenshot_eval: select_music.auto_screenshot_eval,
            input_debounce_seconds: DEFAULT_INPUT_DEBOUNCE_SECONDS,
            arcade_options_navigation: runtime.arcade_options_navigation,
            three_key_navigation: runtime.three_key_navigation,
            use_fsrs: runtime.use_fsrs,
            lights_driver: LightsDriverKind::Off,
            lights_gameplay_pad_lights: GameplayPadLightMode::Input,
            lights_simplify_bass: runtime.lights_simplify_bass,
            lights_com_port: SerialPortName::default(),
            only_dedicated_menu_buttons: runtime.only_dedicated_menu_buttons,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{
        DEFAULT_FRAME_STATS_OVERLAY_ANCHOR, DEFAULT_FRAME_STATS_OVERLAY_STYLE,
        DEFAULT_MACHINE_NOTESKIN,
    };

    #[test]
    fn defaults_match_expected_runtime_toggles() {
        assert_eq!(
            Config::default().custom_sounds_enabled,
            DEFAULT_CUSTOM_SOUNDS_ENABLED
        );
        assert_eq!(
            Config::default().machine_nice_sound,
            DEFAULT_MACHINE_NICE_SOUND
        );
        assert_eq!(
            Config::default().hide_mouse_cursor,
            DEFAULT_HIDE_MOUSE_CURSOR
        );
        assert_eq!(
            Config::default().frame_stats_overlay_anchor,
            DEFAULT_FRAME_STATS_OVERLAY_ANCHOR
        );
        assert_eq!(
            Config::default().frame_stats_overlay_style,
            DEFAULT_FRAME_STATS_OVERLAY_STYLE
        );
        assert_eq!(DEFAULT_MACHINE_NOTESKIN, "cel");
    }
}
