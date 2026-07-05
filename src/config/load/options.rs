use super::*;
use deadsync_config::audio::{
    AudioOptions, AudioRuntimeOptions, load_audio_options, load_audio_runtime_options,
};
use deadsync_config::null_or_die::{NullOrDieOptions, load_null_or_die_options};
use deadsync_config::options::{
    DisplayLoadOptions, RuntimeIoLoadOptions, RuntimeOptions, SelectMusicOptions,
    SystemInputHardwareLoadOptions, SystemOptions, load_display_options, load_gameplay_bg_color,
    load_runtime_io_options, load_runtime_options, load_select_music_options,
    load_system_input_hardware_options, load_system_options,
};
use deadsync_lights::{
    SerialPortName, parse_driver_or_default, parse_gameplay_pad_lights_or_default,
};

pub(super) fn load(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    load_system_opts(conf, default, cfg);
    load_null_or_die_opts(conf, default, cfg);
    load_audio_opts(conf, default, cfg);
    load_select_music_opts(conf, default, cfg);
    load_runtime_opts(conf, default, cfg);
}

fn load_system_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    let display = load_display_options(
        conf,
        DisplayLoadOptions {
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
        |value| FullscreenType::from_str(value).ok(),
        |value| PresentModePolicy::from_str(value).ok(),
        PresentModePolicy::Mailbox,
        PresentModePolicy::Immediate,
        |value| BackendType::from_str(value).ok(),
    );
    cfg.vsync = display.vsync;
    cfg.max_fps = display.max_fps;
    cfg.present_mode_policy = display.present_mode_policy;
    cfg.windowed = display.windowed;
    cfg.fullscreen_type = display.fullscreen_type;
    cfg.display_monitor = display.monitor;
    cfg.display_width = display.width;
    cfg.display_height = display.height;
    cfg.video_renderer = display.video_renderer;

    let loaded = load_system_options(
        conf,
        SystemOptions {
            game_flag: default.game_flag,
            auto_download_unlocks: default.auto_download_unlocks,
            auto_populate_gs_scores: default.auto_populate_gs_scores,
            updater_install_enabled: default.updater_install_enabled,
            enable_groovestats: default.enable_groovestats,
            enable_arrowcloud: default.enable_arrowcloud,
            enable_boogiestats: default.enable_boogiestats,
            submit_arrowcloud_fails: default.submit_arrowcloud_fails,
            arrowcloud_qr_login_when: default.arrowcloud_qr_login_when,
            groovestats_qr_login_when: default.groovestats_qr_login_when,
            separate_unlocks_by_player: default.separate_unlocks_by_player,
            mine_hit_sound: default.mine_hit_sound,
            show_stats_mode: default.show_stats_mode,
            frame_stats_overlay_anchor: default.frame_stats_overlay_anchor,
            frame_stats_overlay_style: default.frame_stats_overlay_style,
            translated_titles: default.translated_titles,
            bg_brightness: default.bg_brightness,
            center_1player_notefield: default.center_1player_notefield,
            center_image_translate_x: default.center_image_translate_x,
            center_image_translate_y: default.center_image_translate_y,
            center_image_add_width: default.center_image_add_width,
            center_image_add_height: default.center_image_add_height,
            autosubmit_course_scores_individually: default.autosubmit_course_scores_individually,
            show_course_individual_scores: default.show_course_individual_scores,
            show_most_played_courses: default.show_most_played_courses,
            show_random_courses: default.show_random_courses,
            default_fail_type: default.default_fail_type,
            banner_cache: default.banner_cache,
            cdtitle_cache: default.cdtitle_cache,
            high_dpi: default.high_dpi,
            hide_mouse_cursor: default.hide_mouse_cursor,
            allow_shutdown_host: default.allow_shutdown_host,
            smx_input: default.smx_input,
            smx_manages_pad_config: default.smx_manages_pad_config,
            smx_panel_lights: default.smx_panel_lights,
            smx_underglow_theme: default.smx_underglow_theme,
            gfx_debug: default.gfx_debug,
            global_offset_seconds: default.global_offset_seconds,
            language_flag: default.language_flag,
            log_level: default.log_level,
            log_to_file: default.log_to_file,
            show_console: default.show_console,
        },
    );
    cfg.game_flag = loaded.game_flag;
    cfg.auto_download_unlocks = loaded.auto_download_unlocks;
    cfg.auto_populate_gs_scores = loaded.auto_populate_gs_scores;
    cfg.updater_install_enabled = loaded.updater_install_enabled;
    cfg.enable_groovestats = loaded.enable_groovestats;
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
    cfg.smx_underglow_theme = loaded.smx_underglow_theme;
    cfg.gfx_debug = loaded.gfx_debug;
    cfg.global_offset_seconds = loaded.global_offset_seconds;
    cfg.language_flag = loaded.language_flag;
    cfg.log_level = loaded.log_level;
    cfg.log_to_file = loaded.log_to_file;
    cfg.show_console = loaded.show_console;

    cfg.gameplay_bg_color =
        load_gameplay_bg_color(conf, default.gameplay_bg_color, Color::from_hex);
    let hardware = load_system_input_hardware_options(
        conf,
        SystemInputHardwareLoadOptions {
            gamepad_backend: default.windows_gamepad_backend,
            smx_default_pad_config: default.smx_default_pad_config,
            smx_default_light_brightness: default.smx_default_light_brightness,
        },
        |value| WindowsPadBackend::from_str(value).ok(),
        |value| crate::config::SmxPadPreset::from_str(value).ok(),
    );
    cfg.windows_gamepad_backend = hardware.gamepad_backend;
    cfg.smx_default_pad_config = hardware.smx_default_pad_config;
    cfg.smx_default_light_brightness = hardware.smx_default_light_brightness;
}

fn load_null_or_die_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    let loaded = load_null_or_die_options(
        conf,
        NullOrDieOptions {
            sync_graph: default.null_or_die_sync_graph,
            confidence_percent: default.null_or_die_confidence_percent,
            pack_sync_threads: default.null_or_die_pack_sync_threads,
            fingerprint_ms: default.null_or_die_fingerprint_ms,
            window_ms: default.null_or_die_window_ms,
            step_ms: default.null_or_die_step_ms,
            magic_offset_ms: default.null_or_die_magic_offset_ms,
            kernel_target: default.null_or_die_kernel_target,
            kernel_type: default.null_or_die_kernel_type,
            full_spectrogram: default.null_or_die_full_spectrogram,
        },
    );
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

fn load_audio_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    let runtime = load_audio_runtime_options(
        conf,
        AudioRuntimeOptions {
            linux_audio_backend: default.linux_audio_backend,
            output_mode: default.audio_output_mode,
        },
        |value| LinuxAudioBackend::from_str(value).ok(),
        |value| AudioOutputMode::from_str(value).ok(),
    );
    cfg.linux_audio_backend = runtime.linux_audio_backend;
    cfg.audio_output_mode = runtime.output_mode;
    let loaded = load_audio_options(
        conf,
        AudioOptions {
            visual_delay_seconds: default.visual_delay_seconds,
            master_volume: default.master_volume,
            menu_music: default.menu_music,
            custom_sounds_enabled: default.custom_sounds_enabled,
            music_volume: default.music_volume,
            music_wheel_switch_speed: default.music_wheel_switch_speed,
            sfx_volume: default.sfx_volume,
            assist_tick_volume: default.assist_tick_volume,
            output_device_index: default.audio_output_device_index,
            sample_rate_hz: default.audio_sample_rate_hz,
            rate_mod_preserves_pitch: default.rate_mod_preserves_pitch,
            enable_replaygain: default.enable_replaygain,
            write_current_screen: default.write_current_screen,
            tab_acceleration: default.tab_acceleration,
        },
    );
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

fn load_select_music_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    let loaded = load_select_music_options(
        conf,
        SelectMusicOptions {
            breakdown_style: default.select_music_breakdown_style,
            show_banners: default.show_select_music_banners,
            show_version_overlay: default.show_version_overlay,
            version_overlay_side: default.version_overlay_side,
            show_video_banners: default.show_select_music_video_banners,
            show_breakdown: default.show_select_music_breakdown,
            show_stage_display: default.show_select_music_stage_display,
            show_cdtitles: default.show_select_music_cdtitles,
            show_wheel_grades: default.show_music_wheel_grades,
            show_wheel_lamps: default.show_music_wheel_lamps,
            itl_rank_mode: default.select_music_itl_rank_mode,
            itl_wheel_mode: default.select_music_itl_wheel_mode,
            wheel_style: default.select_music_wheel_style,
            song_select_bg_mode: default.select_music_song_select_bg_mode,
            new_pack_mode: default.select_music_new_pack_mode,
            show_folder_stats: default.show_select_music_folder_stats,
            show_previews: default.show_select_music_previews,
            show_preview_marker: default.show_select_music_preview_marker,
            preview_loop: default.select_music_preview_loop,
            pattern_info_mode: default.select_music_pattern_info_mode,
            step_artist_box_mode: default.select_music_step_artist_box_mode,
            show_scorebox: default.show_select_music_scorebox,
            scorebox_placement: default.select_music_scorebox_placement,
            scorebox_cycle_itg: default.select_music_scorebox_cycle_itg,
            scorebox_cycle_ex: default.select_music_scorebox_cycle_ex,
            scorebox_cycle_hard_ex: default.select_music_scorebox_cycle_hard_ex,
            scorebox_cycle_tournaments: default.select_music_scorebox_cycle_tournaments,
            chart_info_peak_nps: default.select_music_chart_info_peak_nps,
            chart_info_effective_bpm: default.select_music_chart_info_effective_bpm,
            chart_info_matrix_rating: default.select_music_chart_info_matrix_rating,
            auto_screenshot_eval: default.auto_screenshot_eval,
        },
    );
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
    cfg.select_music_itl_rank_mode = loaded.itl_rank_mode;
    cfg.select_music_itl_wheel_mode = loaded.itl_wheel_mode;
    cfg.select_music_wheel_style = loaded.wheel_style;
    cfg.select_music_song_select_bg_mode = loaded.song_select_bg_mode;
    cfg.select_music_new_pack_mode = loaded.new_pack_mode;
    cfg.show_select_music_folder_stats = loaded.show_folder_stats;
    cfg.show_select_music_previews = loaded.show_previews;
    cfg.show_select_music_preview_marker = loaded.show_preview_marker;
    cfg.select_music_preview_loop = loaded.preview_loop;
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

fn load_runtime_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    let loaded = load_runtime_options(
        conf,
        RuntimeOptions {
            fastload: default.fastload,
            cachesongs: default.cachesongs,
            song_parsing_threads: default.song_parsing_threads,
            smooth_histogram: default.smooth_histogram,
            shade_scatterplot_judgments: default.shade_scatterplot_judgments,
            arcade_options_navigation: default.arcade_options_navigation,
            delayed_back: default.delayed_back,
            three_key_navigation: default.three_key_navigation,
            use_fsrs: default.use_fsrs,
            lights_simplify_bass: default.lights_simplify_bass,
            only_dedicated_menu_buttons: default.only_dedicated_menu_buttons,
            theme_flag: default.theme_flag,
            software_renderer_threads: default.software_renderer_threads,
        },
    );
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

    let io = load_runtime_io_options(
        conf,
        RuntimeIoLoadOptions {
            input_debounce_seconds: default.input_debounce_seconds,
            lights_driver: default.lights_driver,
            gameplay_pad_lights: default.lights_gameplay_pad_lights,
            lights_com_port: default.lights_com_port,
        },
        deadsync_input::parse_input_debounce_seconds,
        parse_driver_or_default,
        parse_gameplay_pad_lights_or_default,
        SerialPortName::parse,
    );
    cfg.input_debounce_seconds = io.input_debounce_seconds;
    cfg.lights_driver = io.lights_driver;
    cfg.lights_gameplay_pad_lights = io.gameplay_pad_lights;
    cfg.lights_com_port = io.lights_com_port;
}
