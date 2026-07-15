use crate::app_config::Config;
use crate::audio::{
    AudioOptions, AudioRuntimeOptions, load_audio_options, load_audio_runtime_options,
};
use crate::ini::SimpleIni;
use crate::null_or_die::{NullOrDieOptions, load_null_or_die_options};
use crate::options::{
    DisplayLoadOptions, RuntimeIoLoadOptions, RuntimeOptions, SelectMusicOptions,
    SystemInputHardwareLoadOptions, SystemOptions, load_display_options, load_gameplay_bg_color,
    load_runtime_io_options, load_runtime_options, load_select_music_options,
    load_system_input_hardware_options, load_system_options,
};
use crate::theme::{
    MachineFlowOptions, ThemePresentationOptions, ThemeShortcutOptions, load_machine_flow_options,
    load_theme_presentation_options, load_theme_shortcut_options,
};
use deadlib_platform::display::FullscreenType;
use deadlib_present::color::Color;
use deadlib_render::{BackendType, PresentModePolicy};
use deadsync_audio::{AudioOutputMode, LinuxAudioBackend};
use deadsync_input::parse_keycode_to_key;
use deadsync_input_native::WindowsPadBackend;
use deadsync_lights::{
    SerialPortName, parse_driver_or_default, parse_gameplay_pad_lights_or_default,
};
use std::path::Path;
use std::str::FromStr;
use winit::keyboard::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfigLoadDefaults<F, P, V, W, S, L, M, D, G, R, C, K> {
    pub display: DisplayLoadOptions<F, P, V>,
    pub input_hardware: SystemInputHardwareLoadOptions<W, S>,
    pub audio_runtime: AudioRuntimeOptions<L, M>,
    pub runtime_io: RuntimeIoLoadOptions<D, G, R>,
    pub gameplay_bg_color: C,
    pub shortcuts: ThemeShortcutOptions<K>,
}

#[derive(Clone, Copy)]
pub struct ConfigLoadParsers<F, P, V, W, S, L, M, D, G, R, C, K> {
    pub parse_fullscreen_type: fn(&str) -> Option<F>,
    pub parse_present_mode_policy: fn(&str) -> Option<P>,
    pub legacy_balanced_policy: P,
    pub legacy_unhinged_policy: P,
    pub parse_video_renderer: fn(&str) -> Option<V>,
    pub parse_gamepad_backend: fn(&str) -> Option<W>,
    pub parse_smx_pad_config: fn(&str) -> Option<S>,
    pub parse_linux_backend: fn(&str) -> Option<L>,
    pub parse_audio_output_mode: fn(&str) -> Option<M>,
    pub parse_input_debounce_seconds: fn(&str) -> Option<f32>,
    pub parse_lights_driver: fn(&str, D) -> D,
    pub parse_gameplay_pad_lights: fn(&str, G) -> G,
    pub parse_lights_com_port: fn(&str, R) -> R,
    pub parse_color: fn(&str) -> Option<C>,
    pub parse_key: fn(&str) -> Option<K>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadedConfigSections<F, P, V, W, S, L, M, D, G, R, C, K> {
    pub display: DisplayLoadOptions<F, P, V>,
    pub system: SystemOptions,
    pub input_hardware: SystemInputHardwareLoadOptions<W, S>,
    pub gameplay_bg_color: C,
    pub null_or_die: NullOrDieOptions,
    pub audio_runtime: AudioRuntimeOptions<L, M>,
    pub audio: AudioOptions,
    pub select_music: SelectMusicOptions,
    pub runtime: RuntimeOptions,
    pub runtime_io: RuntimeIoLoadOptions<D, G, R>,
    pub theme_presentation: ThemePresentationOptions,
    pub machine_flow: MachineFlowOptions,
    pub shortcuts: ThemeShortcutOptions<K>,
}

pub fn load_config_sections<F, P, V, W, S, L, M, D, G, R, C, K>(
    conf: &SimpleIni,
    default: ConfigLoadDefaults<F, P, V, W, S, L, M, D, G, R, C, K>,
    parsers: ConfigLoadParsers<F, P, V, W, S, L, M, D, G, R, C, K>,
) -> LoadedConfigSections<F, P, V, W, S, L, M, D, G, R, C, K>
where
    F: Copy,
    P: Copy,
    V: Copy,
    W: Copy,
    S: Copy,
    L: Copy,
    M: Copy,
    D: Copy,
    G: Copy,
    R: Copy,
    C: Copy,
    K: Copy,
{
    LoadedConfigSections {
        display: load_display_options(
            conf,
            default.display,
            parsers.parse_fullscreen_type,
            parsers.parse_present_mode_policy,
            parsers.legacy_balanced_policy,
            parsers.legacy_unhinged_policy,
            parsers.parse_video_renderer,
        ),
        system: load_system_options(conf, SystemOptions::default()),
        input_hardware: load_system_input_hardware_options(
            conf,
            default.input_hardware,
            parsers.parse_gamepad_backend,
            parsers.parse_smx_pad_config,
        ),
        gameplay_bg_color: load_gameplay_bg_color(
            conf,
            default.gameplay_bg_color,
            parsers.parse_color,
        ),
        null_or_die: load_null_or_die_options(conf, NullOrDieOptions::default()),
        audio_runtime: load_audio_runtime_options(
            conf,
            default.audio_runtime,
            parsers.parse_linux_backend,
            parsers.parse_audio_output_mode,
        ),
        audio: load_audio_options(conf, AudioOptions::default()),
        select_music: load_select_music_options(conf, SelectMusicOptions::default()),
        runtime: load_runtime_options(conf, RuntimeOptions::default()),
        runtime_io: load_runtime_io_options(
            conf,
            default.runtime_io,
            parsers.parse_input_debounce_seconds,
            parsers.parse_lights_driver,
            parsers.parse_gameplay_pad_lights,
            parsers.parse_lights_com_port,
        ),
        theme_presentation: load_theme_presentation_options(
            conf,
            ThemePresentationOptions::default(),
        ),
        machine_flow: load_machine_flow_options(conf, MachineFlowOptions::default()),
        shortcuts: load_theme_shortcut_options(conf, default.shortcuts, parsers.parse_key),
    }
}

pub fn load_app_config(conf: &SimpleIni, default: Config) -> Config {
    let loaded = load_config_sections(
        conf,
        ConfigLoadDefaults {
            display: DisplayLoadOptions {
                vsync: default.vsync,
                max_fps: default.max_fps,
                present_mode_policy: default.present_mode_policy,
                windowed: default.windowed,
                fullscreen_type: default.fullscreen_type,
                monitor: default.display_monitor,
                width: default.display_width,
                height: default.display_height,
                video_renderer: default.video_renderer,
            },
            input_hardware: SystemInputHardwareLoadOptions {
                gamepad_backend: default.windows_gamepad_backend,
                smx_default_pad_config: default.smx_default_pad_config,
                smx_default_light_brightness: default.smx_default_light_brightness,
            },
            audio_runtime: AudioRuntimeOptions {
                linux_audio_backend: default.linux_audio_backend,
                output_mode: default.audio_output_mode,
            },
            runtime_io: RuntimeIoLoadOptions {
                input_debounce_seconds: default.input_debounce_seconds,
                lights_driver: default.lights_driver,
                gameplay_pad_lights: default.lights_gameplay_pad_lights,
                lights_com_port: default.lights_com_port,
            },
            gameplay_bg_color: default.gameplay_bg_color,
            shortcuts: ThemeShortcutOptions {
                practice: default.music_select_shortcut_practice,
                song_search: default.music_select_shortcut_song_search,
                load_songs: default.music_select_shortcut_load_songs,
                test_input: default.music_select_shortcut_test_input,
            },
        },
        ConfigLoadParsers {
            parse_fullscreen_type: |value| FullscreenType::from_str(value).ok(),
            parse_present_mode_policy: |value| PresentModePolicy::from_str(value).ok(),
            legacy_balanced_policy: PresentModePolicy::Mailbox,
            legacy_unhinged_policy: PresentModePolicy::Immediate,
            parse_video_renderer: |value| BackendType::from_str(value).ok(),
            parse_gamepad_backend: |value| WindowsPadBackend::from_str(value).ok(),
            parse_smx_pad_config: |value| deadsync_smx::SmxPadPreset::from_str(value).ok(),
            parse_linux_backend: |value| LinuxAudioBackend::from_str(value).ok(),
            parse_audio_output_mode: |value| AudioOutputMode::from_str(value).ok(),
            parse_input_debounce_seconds: deadsync_input::parse_input_debounce_seconds,
            parse_lights_driver: parse_driver_or_default,
            parse_gameplay_pad_lights: parse_gameplay_pad_lights_or_default,
            parse_lights_com_port: SerialPortName::parse,
            parse_color: Color::from_hex,
            parse_key: parse_keycode_to_key,
        },
    );

    let mut cfg = default;
    apply_display_opts(loaded.display, &mut cfg);
    apply_system_opts(loaded.system, &mut cfg);
    apply_system_hardware_opts(loaded.input_hardware, &mut cfg);
    cfg.gameplay_bg_color = loaded.gameplay_bg_color;
    apply_null_or_die_opts(loaded.null_or_die, &mut cfg);
    apply_audio_opts(loaded.audio_runtime, loaded.audio, &mut cfg);
    apply_select_music_opts(loaded.select_music, &mut cfg);
    apply_runtime_opts(loaded.runtime, loaded.runtime_io, &mut cfg);
    apply_theme_presentation(loaded.theme_presentation, &mut cfg);
    apply_machine_flow(loaded.machine_flow, loaded.shortcuts, &mut cfg);
    cfg
}

pub fn load_bootstrap_bool(path: &Path, key: &str, default: bool) -> bool {
    let mut conf = SimpleIni::new();
    if conf.load(path).is_err() {
        return default;
    }
    crate::options::load_bool_option(&conf, "Options", key, default)
}

fn apply_display_opts(
    display: DisplayLoadOptions<FullscreenType, PresentModePolicy, BackendType>,
    cfg: &mut Config,
) {
    cfg.vsync = display.vsync;
    cfg.max_fps = display.max_fps;
    cfg.present_mode_policy = display.present_mode_policy;
    cfg.windowed = display.windowed;
    cfg.fullscreen_type = display.fullscreen_type;
    cfg.display_monitor = display.monitor;
    cfg.display_width = display.width;
    cfg.display_height = display.height;
    cfg.video_renderer = display.video_renderer;
}

fn apply_system_opts(loaded: SystemOptions, cfg: &mut Config) {
    cfg.game_flag = loaded.game_flag;
    cfg.auto_download_unlocks = loaded.auto_download_unlocks;
    cfg.auto_populate_gs_scores = loaded.auto_populate_gs_scores;
    cfg.updater_install_enabled = loaded.updater_install_enabled;
    cfg.enable_groovestats = loaded.enable_groovestats;
    cfg.show_srpg_shop = loaded.show_srpg_shop;
    cfg.srpg_shop_folder = loaded.srpg_shop_folder;
    cfg.enable_arrowcloud = loaded.enable_arrowcloud;
    cfg.enable_boogiestats = loaded.enable_boogiestats;
    cfg.submit_arrowcloud_fails = loaded.submit_arrowcloud_fails;
    cfg.arrowcloud_qr_login_when = loaded.arrowcloud_qr_login_when;
    cfg.groovestats_qr_login_when = loaded.groovestats_qr_login_when;
    cfg.separate_unlocks_by_player = loaded.separate_unlocks_by_player;
    cfg.mine_hit_sound = loaded.mine_hit_sound;
    cfg.show_stats_mode = loaded.show_stats_mode;
    cfg.frame_stats_overlay_anchor = loaded.frame_stats_overlay_anchor;
    cfg.frame_stats_overlay_style = loaded.frame_stats_overlay_style;
    cfg.translated_titles = loaded.translated_titles;
    cfg.bg_brightness = loaded.bg_brightness;
    cfg.center_1player_notefield = loaded.center_1player_notefield;
    cfg.center_image_translate_x = loaded.center_image_translate_x;
    cfg.center_image_translate_y = loaded.center_image_translate_y;
    cfg.center_image_add_width = loaded.center_image_add_width;
    cfg.center_image_add_height = loaded.center_image_add_height;
    cfg.autosubmit_course_scores_individually = loaded.autosubmit_course_scores_individually;
    cfg.show_course_individual_scores = loaded.show_course_individual_scores;
    cfg.show_most_played_courses = loaded.show_most_played_courses;
    cfg.show_random_courses = loaded.show_random_courses;
    cfg.default_fail_type = loaded.default_fail_type;
    cfg.banner_cache = loaded.banner_cache;
    cfg.cdtitle_cache = loaded.cdtitle_cache;
    cfg.high_dpi = loaded.high_dpi;
    cfg.hide_mouse_cursor = loaded.hide_mouse_cursor;
    cfg.allow_shutdown_host = loaded.allow_shutdown_host;
    cfg.smx_input = loaded.smx_input;
    cfg.smx_manages_pad_config = loaded.smx_manages_pad_config;
    cfg.smx_panel_lights = loaded.smx_panel_lights;
    cfg.smx_pad_gifs_pack = loaded.smx_pad_gifs_pack;
    cfg.smx_judge_gifs_pack = loaded.smx_judge_gifs_pack;
    cfg.smx_underglow_theme = loaded.smx_underglow_theme;
    cfg.smx_underglow_grb = loaded.smx_underglow_grb;
    cfg.gfx_debug = loaded.gfx_debug;
    cfg.global_offset_seconds = loaded.global_offset_seconds;
    cfg.language_flag = loaded.language_flag;
    cfg.log_level = loaded.log_level;
    cfg.log_to_file = loaded.log_to_file;
    cfg.show_console = loaded.show_console;
}

fn apply_system_hardware_opts(
    hardware: SystemInputHardwareLoadOptions<WindowsPadBackend, deadsync_smx::SmxPadPreset>,
    cfg: &mut Config,
) {
    cfg.windows_gamepad_backend = hardware.gamepad_backend;
    cfg.smx_default_pad_config = hardware.smx_default_pad_config;
    cfg.smx_default_light_brightness = hardware.smx_default_light_brightness;
}

fn apply_null_or_die_opts(loaded: NullOrDieOptions, cfg: &mut Config) {
    cfg.null_or_die_sync_graph = loaded.sync_graph;
    cfg.null_or_die_confidence_percent = loaded.confidence_percent;
    cfg.null_or_die_pack_sync_threads = loaded.pack_sync_threads;
    cfg.null_or_die_fingerprint_ms = loaded.fingerprint_ms;
    cfg.null_or_die_window_ms = loaded.window_ms;
    cfg.null_or_die_step_ms = loaded.step_ms;
    cfg.null_or_die_magic_offset_ms = loaded.magic_offset_ms;
    cfg.null_or_die_kernel_target = loaded.kernel_target;
    cfg.null_or_die_kernel_type = loaded.kernel_type;
    cfg.null_or_die_full_spectrogram = loaded.full_spectrogram;
}

fn apply_audio_opts(
    runtime: AudioRuntimeOptions<LinuxAudioBackend, AudioOutputMode>,
    loaded: AudioOptions,
    cfg: &mut Config,
) {
    cfg.linux_audio_backend = runtime.linux_audio_backend;
    cfg.audio_output_mode = runtime.output_mode;
    cfg.visual_delay_seconds = loaded.visual_delay_seconds;
    cfg.master_volume = loaded.master_volume;
    cfg.menu_music = loaded.menu_music;
    cfg.custom_sounds_enabled = loaded.custom_sounds_enabled;
    cfg.music_volume = loaded.music_volume;
    cfg.music_wheel_switch_speed = loaded.music_wheel_switch_speed;
    cfg.sfx_volume = loaded.sfx_volume;
    cfg.assist_tick_volume = loaded.assist_tick_volume;
    cfg.audio_output_device_index = loaded.output_device_index;
    cfg.audio_sample_rate_hz = loaded.sample_rate_hz;
    cfg.rate_mod_preserves_pitch = loaded.rate_mod_preserves_pitch;
    cfg.enable_replaygain = loaded.enable_replaygain;
    cfg.write_current_screen = loaded.write_current_screen;
    cfg.tab_acceleration = loaded.tab_acceleration;
}

fn apply_select_music_opts(loaded: SelectMusicOptions, cfg: &mut Config) {
    cfg.select_music_breakdown_style = loaded.breakdown_style;
    cfg.show_select_music_banners = loaded.show_banners;
    cfg.show_version_overlay = loaded.show_version_overlay;
    cfg.version_overlay_side = loaded.version_overlay_side;
    cfg.show_select_music_video_banners = loaded.show_video_banners;
    cfg.show_select_music_breakdown = loaded.show_breakdown;
    cfg.show_select_music_stage_display = loaded.show_stage_display;
    cfg.show_select_music_cdtitles = loaded.show_cdtitles;
    cfg.show_music_wheel_grades = loaded.show_wheel_grades;
    cfg.show_music_wheel_lamps = loaded.show_wheel_lamps;
    cfg.sort_music_wheel_by_series = loaded.sort_wheel_by_series;
    cfg.select_music_itl_rank_mode = loaded.itl_rank_mode;
    cfg.select_music_itl_wheel_mode = loaded.itl_wheel_mode;
    cfg.select_music_wheel_style = loaded.wheel_style;
    cfg.select_music_song_select_bg_mode = loaded.song_select_bg_mode;
    cfg.select_music_new_pack_mode = loaded.new_pack_mode;
    cfg.show_select_music_folder_stats = loaded.show_folder_stats;
    cfg.show_select_music_previews = loaded.show_previews;
    cfg.show_select_music_preview_marker = loaded.show_preview_marker;
    cfg.select_music_preview_loop = loaded.preview_loop;
    cfg.select_music_preview_starts_immediately = loaded.preview_starts_immediately;
    cfg.select_music_pattern_info_mode = loaded.pattern_info_mode;
    cfg.select_music_step_artist_box_mode = loaded.step_artist_box_mode;
    cfg.show_select_music_scorebox = loaded.show_scorebox;
    cfg.select_music_scorebox_placement = loaded.scorebox_placement;
    cfg.select_music_scorebox_cycle_itg = loaded.scorebox_cycle_itg;
    cfg.select_music_scorebox_cycle_ex = loaded.scorebox_cycle_ex;
    cfg.select_music_scorebox_cycle_hard_ex = loaded.scorebox_cycle_hard_ex;
    cfg.select_music_scorebox_cycle_tournaments = loaded.scorebox_cycle_tournaments;
    cfg.select_music_chart_info_peak_nps = loaded.chart_info_peak_nps;
    cfg.select_music_chart_info_effective_bpm = loaded.chart_info_effective_bpm;
    cfg.select_music_chart_info_matrix_rating = loaded.chart_info_matrix_rating;
    cfg.auto_screenshot_eval = loaded.auto_screenshot_eval;
}

fn apply_runtime_opts(
    loaded: RuntimeOptions,
    io: RuntimeIoLoadOptions<
        deadsync_lights::DriverKind,
        deadsync_lights::GameplayPadLightMode,
        SerialPortName,
    >,
    cfg: &mut Config,
) {
    cfg.fastload = loaded.fastload;
    cfg.cachesongs = loaded.cachesongs;
    cfg.song_parsing_threads = loaded.song_parsing_threads;
    cfg.smooth_histogram = loaded.smooth_histogram;
    cfg.shade_scatterplot_judgments = loaded.shade_scatterplot_judgments;
    cfg.arcade_options_navigation = loaded.arcade_options_navigation;
    cfg.delayed_back = loaded.delayed_back;
    cfg.three_key_navigation = loaded.three_key_navigation;
    cfg.use_fsrs = loaded.use_fsrs;
    cfg.lights_simplify_bass = loaded.lights_simplify_bass;
    cfg.only_dedicated_menu_buttons = loaded.only_dedicated_menu_buttons;
    cfg.theme_flag = loaded.theme_flag;
    cfg.software_renderer_threads = loaded.software_renderer_threads;

    cfg.input_debounce_seconds = io.input_debounce_seconds;
    cfg.lights_driver = io.lights_driver;
    cfg.lights_gameplay_pad_lights = io.gameplay_pad_lights;
    cfg.lights_com_port = io.lights_com_port;
}

fn apply_theme_presentation(loaded: ThemePresentationOptions, cfg: &mut Config) {
    cfg.simply_love_color = loaded.simply_love_color;
    cfg.show_select_music_gameplay_timer = loaded.show_select_music_gameplay_timer;
    cfg.keyboard_features = loaded.keyboard_features;
    cfg.visual_style = loaded.visual_style;
    cfg.srpg_variant = loaded.srpg_variant;
    cfg.show_video_backgrounds = loaded.show_video_backgrounds;
    cfg.random_background_mode = loaded.random_background_mode;
    cfg.zmod_rating_box_text = loaded.zmod_rating_box_text;
    cfg.show_bpm_decimal = loaded.show_bpm_decimal;
    cfg.gameplay_bpm_position = loaded.gameplay_bpm_position;
}

fn apply_machine_flow(
    loaded: MachineFlowOptions,
    shortcuts: ThemeShortcutOptions<KeyCode>,
    cfg: &mut Config,
) {
    cfg.machine_show_eval_summary = loaded.machine_show_eval_summary;
    cfg.machine_nice_sound = loaded.machine_nice_sound;
    cfg.machine_show_name_entry = loaded.machine_show_name_entry;
    cfg.machine_show_gameover = loaded.machine_show_gameover;
    cfg.machine_show_select_profile = loaded.machine_show_select_profile;
    cfg.allow_switch_profile_in_menu = loaded.allow_switch_profile_in_menu;
    cfg.machine_show_select_color = loaded.machine_show_select_color;
    cfg.machine_show_select_style = loaded.machine_show_select_style;
    cfg.machine_show_select_play_mode = loaded.machine_show_select_play_mode;
    cfg.machine_enable_replays = loaded.machine_enable_replays;
    cfg.machine_enable_heart_rate_monitors = loaded.machine_enable_heart_rate_monitors;
    cfg.machine_allow_per_player_global_offsets = loaded.machine_allow_per_player_global_offsets;
    cfg.machine_pack_ini_offsets = loaded.machine_pack_ini_offsets;
    cfg.machine_default_sync_offset = loaded.machine_default_sync_offset;
    cfg.machine_preferred_style = loaded.machine_preferred_style;
    cfg.machine_preferred_play_mode = loaded.machine_preferred_play_mode;
    cfg.machine_font = loaded.machine_font;
    cfg.machine_bar_color = loaded.machine_bar_color;
    cfg.machine_evaluation_style = loaded.machine_evaluation_style;
    cfg.music_select_shortcut_practice = shortcuts.practice;
    cfg.music_select_shortcut_song_search = shortcuts.song_search;
    cfg.music_select_shortcut_load_songs = shortcuts.load_songs;
    cfg.music_select_shortcut_test_input = shortcuts.test_input;
}
