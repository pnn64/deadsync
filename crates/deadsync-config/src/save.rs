use crate::app_config::Config;
use crate::audio::{
    AudioDeviceOptions, AudioOptions, push_audio_device_option_lines,
    push_audio_music_option_lines, push_audio_playback_prefix_lines, push_audio_tail_option_lines,
    push_audio_write_current_screen_option_lines,
};
use crate::cache::push_never_cache_list_option_line;
use crate::folders::{AdditionalSongFolder, push_additional_song_folder_option_lines};
use crate::machine::push_default_noteskin_option_line;
use crate::null_or_die::{NullOrDieOptions, push_null_or_die_option_lines};
use crate::options::{
    DisplayOptions, RuntimeIoOptions, RuntimeOptions, SelectMusicOptions, SelectMusicSaveOptions,
    StatsOverlayOptions, SystemInputHardwareOptions, SystemOptions,
    push_display_frame_timing_option_lines, push_display_fullscreen_option_lines,
    push_display_monitor_option_lines, push_display_size_option_lines,
    push_display_video_tail_option_lines, push_gameplay_bg_color_option_line,
    push_runtime_audio_backend_option_lines, push_runtime_cache_option_lines,
    push_runtime_fastload_option_lines, push_runtime_input_debounce_option_lines,
    push_runtime_lights_driver_option_lines, push_runtime_lights_option_lines,
    push_runtime_lights_port_option_lines, push_runtime_menu_option_lines,
    push_runtime_navigation_option_lines, push_runtime_worker_theme_option_lines,
    push_select_music_option_lines, push_stats_overlay_option_lines,
    push_system_banner_cache_option_lines, push_system_bg_brightness_option_lines,
    push_system_cdtitle_center_option_lines, push_system_course_option_lines,
    push_system_diagnostics_option_lines, push_system_download_option_lines,
    push_system_input_hardware_option_lines, push_system_mine_hit_sound_option_lines,
    push_system_online_option_lines, push_system_translation_option_lines,
};
use crate::runtime_state::{
    RuntimeStateIdTokens, push_pad_order_option_lines, push_runtime_state_id_option_lines,
};
use crate::theme::{
    MachineFlowOptions, ThemePresentationOptions, ThemeShortcutTokens, push_theme_option_lines,
};
use crate::writer::push_section;
use deadsync_input::{Keymap, keycode_to_token};
use std::fmt::Display;

pub struct SavedOptionSection<'a, P> {
    pub audio: AudioOptions,
    pub audio_device: AudioDeviceOptions<'a>,
    pub additional_song_folders: &'a [AdditionalSongFolder],
    pub never_cache_list: &'a [String],
    pub system: SystemOptions,
    pub input_hardware: SystemInputHardwareOptions<'a>,
    pub display: DisplayOptions<'a>,
    pub runtime_io: RuntimeIoOptions<'a>,
    pub runtime: RuntimeOptions,
    pub stats_overlay: StatsOverlayOptions<'a>,
    pub select_music: SelectMusicSaveOptions,
    pub null_or_die: NullOrDieOptions,
    pub gameplay_bg_color: &'a str,
    pub default_noteskin: &'a str,
    pub runtime_state_ids: RuntimeStateIdTokens<'a>,
    pub pad_order_lines: P,
}

pub struct DefaultOptionSection<'a, P> {
    pub audio: AudioOptions,
    pub audio_device: AudioDeviceOptions<'a>,
    pub additional_song_folders: &'a [AdditionalSongFolder],
    pub never_cache_list: &'a [String],
    pub system: SystemOptions,
    pub input_hardware: SystemInputHardwareOptions<'a>,
    pub display: DisplayOptions<'a>,
    pub runtime_io: RuntimeIoOptions<'a>,
    pub runtime: RuntimeOptions,
    pub stats_overlay: StatsOverlayOptions<'a>,
    pub select_music: SelectMusicSaveOptions,
    pub gameplay_bg_color: &'a str,
    pub default_noteskin: &'a str,
    pub runtime_state_ids: RuntimeStateIdTokens<'a>,
    pub pad_order_lines: P,
}

pub struct ThemeSection<'a> {
    pub presentation: ThemePresentationOptions,
    pub machine: MachineFlowOptions,
    pub shortcuts: ThemeShortcutTokens<'a>,
    pub null_or_die: Option<NullOrDieOptions>,
}

pub struct SavedConfigFile<'a, P, K> {
    pub options: SavedOptionSection<'a, P>,
    pub keymap: K,
    pub theme: ThemeSection<'a>,
}

pub struct DefaultConfigFile<'a, P, K> {
    pub options: DefaultOptionSection<'a, P>,
    pub keymap: K,
    pub theme: ThemeSection<'a>,
}

pub fn build_saved_config_file<P, V, K>(
    file: SavedConfigFile<'_, P, K>,
    push_keymap: impl FnOnce(&mut String, K),
) -> String
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    let mut content = String::with_capacity(4096);
    push_saved_option_section(&mut content, file.options);
    push_keymap(&mut content, file.keymap);
    push_theme_section(&mut content, file.theme);
    content
}

pub fn build_default_config_file<P, V, K>(
    file: DefaultConfigFile<'_, P, K>,
    push_keymap: impl FnOnce(&mut String, K),
) -> String
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    let mut content = String::with_capacity(4096);
    push_default_option_section(&mut content, file.options);
    push_keymap(&mut content, file.keymap);
    push_theme_section(&mut content, file.theme);
    content
}

pub fn push_saved_option_section<P, V>(content: &mut String, options: SavedOptionSection<'_, P>)
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    push_section(content, "[Options]");
    push_audio_device_option_lines(content, options.audio_device);
    push_additional_song_folder_option_lines(content, options.additional_song_folders);
    push_system_download_option_lines(content, options.system);
    push_system_bg_brightness_option_lines(content, options.system);
    push_gameplay_bg_color_option_line(content, options.gameplay_bg_color);
    push_system_banner_cache_option_lines(content, options.system);
    push_runtime_cache_option_lines(content, options.runtime);
    push_never_cache_list_option_line(content, options.never_cache_list);
    push_system_cdtitle_center_option_lines(content, options.system);
    push_system_course_option_lines(content, options.system);
    push_null_or_die_option_lines(content, options.null_or_die);
    push_default_noteskin_option_line(content, options.default_noteskin);
    push_display_size_option_lines(content, options.display);
    push_system_online_option_lines(content, options.system);
    push_runtime_fastload_option_lines(content, options.runtime);
    push_display_fullscreen_option_lines(content, options.display);
    push_system_input_hardware_option_lines(content, options.input_hardware);
    push_runtime_state_id_option_lines(content, options.runtime_state_ids);
    push_pad_order_option_lines(content, options.pad_order_lines);
    push_system_diagnostics_option_lines(content, options.system);
    push_runtime_audio_backend_option_lines(content, options.runtime_io);
    push_display_frame_timing_option_lines(content, options.display);
    push_audio_playback_prefix_lines(content, options.audio);
    push_system_mine_hit_sound_option_lines(content, options.system);
    push_audio_music_option_lines(content, options.audio);
    push_select_music_option_lines(content, options.select_music);
    push_stats_overlay_option_lines(content, options.stats_overlay);
    push_runtime_input_debounce_option_lines(content, options.runtime_io);
    push_runtime_navigation_option_lines(content, options.runtime);
    push_runtime_lights_driver_option_lines(content, options.runtime_io);
    push_runtime_lights_option_lines(content, options.runtime);
    push_runtime_lights_port_option_lines(content, options.runtime_io);
    push_runtime_menu_option_lines(content, options.runtime);
    push_display_monitor_option_lines(content, options.display);
    push_runtime_worker_theme_option_lines(content, options.runtime);
    push_audio_tail_option_lines(content, options.audio);
    push_system_translation_option_lines(content, options.system);
    push_display_video_tail_option_lines(content, options.display);
    push_audio_write_current_screen_option_lines(content, options.audio);
    content.push('\n');
}

pub fn push_default_option_section<P, V>(content: &mut String, options: DefaultOptionSection<'_, P>)
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    push_section(content, "[Options]");
    push_audio_device_option_lines(content, options.audio_device);
    push_additional_song_folder_option_lines(content, options.additional_song_folders);
    push_system_download_option_lines(content, options.system);
    push_system_bg_brightness_option_lines(content, options.system);
    push_gameplay_bg_color_option_line(content, options.gameplay_bg_color);
    push_system_banner_cache_option_lines(content, options.system);
    push_runtime_cache_option_lines(content, options.runtime);
    push_never_cache_list_option_line(content, options.never_cache_list);
    push_system_cdtitle_center_option_lines(content, options.system);
    push_system_course_option_lines(content, options.system);
    push_default_noteskin_option_line(content, options.default_noteskin);
    push_display_size_option_lines(content, options.display);
    push_display_monitor_option_lines(content, options.display);
    push_system_online_option_lines(content, options.system);
    push_runtime_fastload_option_lines(content, options.runtime);
    push_display_fullscreen_option_lines(content, options.display);
    push_system_input_hardware_option_lines(content, options.input_hardware);
    push_runtime_state_id_option_lines(content, options.runtime_state_ids);
    push_pad_order_option_lines(content, options.pad_order_lines);
    push_system_diagnostics_option_lines(content, options.system);
    push_runtime_audio_backend_option_lines(content, options.runtime_io);
    push_display_frame_timing_option_lines(content, options.display);
    push_audio_playback_prefix_lines(content, options.audio);
    push_system_mine_hit_sound_option_lines(content, options.system);
    push_audio_music_option_lines(content, options.audio);
    push_select_music_option_lines(content, options.select_music);
    push_stats_overlay_option_lines(content, options.stats_overlay);
    push_runtime_input_debounce_option_lines(content, options.runtime_io);
    push_runtime_navigation_option_lines(content, options.runtime);
    push_runtime_lights_driver_option_lines(content, options.runtime_io);
    push_runtime_lights_option_lines(content, options.runtime);
    push_runtime_lights_port_option_lines(content, options.runtime_io);
    push_runtime_menu_option_lines(content, options.runtime);
    push_runtime_worker_theme_option_lines(content, options.runtime);
    push_audio_tail_option_lines(content, options.audio);
    push_system_translation_option_lines(content, options.system);
    push_display_video_tail_option_lines(content, options.display);
    push_audio_write_current_screen_option_lines(content, options.audio);
    content.push('\n');
}

pub fn push_theme_section(content: &mut String, section: ThemeSection<'_>) {
    push_section(content, "[Theme]");
    push_theme_option_lines(
        content,
        section.presentation,
        section.machine,
        section.shortcuts,
    );
    if let Some(null_or_die) = section.null_or_die {
        push_null_or_die_option_lines(content, null_or_die);
    }
    content.push('\n');
}

pub fn build_saved_app_config_file(
    cfg: &Config,
    keymap: &Keymap,
    machine_default_noteskin: &str,
    additional_song_folders: &[AdditionalSongFolder],
    never_cache_list: &[String],
    smx_p1_serial: &str,
    smx_p2_serial: &str,
    default_profile_p1: &str,
    default_profile_p2: &str,
) -> String {
    let gameplay_bg_color = cfg.gameplay_bg_color.to_hex();
    let video_renderer = cfg.video_renderer.to_string();
    let practice = keycode_to_token(cfg.music_select_shortcut_practice);
    let song_search = keycode_to_token(cfg.music_select_shortcut_song_search);
    let load_songs = keycode_to_token(cfg.music_select_shortcut_load_songs);
    let test_input = keycode_to_token(cfg.music_select_shortcut_test_input);

    build_saved_config_file(
        SavedConfigFile {
            options: SavedOptionSection {
                audio: audio_options(cfg),
                audio_device: audio_device_options(cfg, cfg.audio_output_mode.as_str()),
                additional_song_folders,
                never_cache_list,
                system: system_options(cfg),
                input_hardware: system_input_hardware_options(cfg, true),
                display: display_options(
                    cfg,
                    cfg.present_mode_policy.as_str(),
                    video_renderer.as_str(),
                ),
                runtime_io: runtime_io_options(
                    cfg,
                    cfg.linux_audio_backend.as_str(),
                    cfg.lights_driver.as_str(),
                    cfg.lights_gameplay_pad_lights.as_str(),
                    cfg.lights_com_port.as_str(),
                ),
                runtime: runtime_options(cfg),
                stats_overlay: stats_overlay_options(
                    cfg,
                    Some(cfg.frame_stats_overlay_anchor),
                    Some(cfg.frame_stats_overlay_style),
                ),
                select_music: select_music_save_options(cfg),
                null_or_die: null_or_die_options(cfg),
                gameplay_bg_color: gameplay_bg_color.as_str(),
                default_noteskin: machine_default_noteskin,
                runtime_state_ids: runtime_state_ids(
                    smx_p1_serial,
                    smx_p2_serial,
                    default_profile_p1,
                    default_profile_p2,
                ),
                pad_order_lines: deadsync_input_native::pad_order_ini_lines(),
            },
            keymap,
            theme: ThemeSection {
                presentation: theme_presentation_options(cfg),
                machine: machine_flow_options(cfg),
                shortcuts: theme_shortcut_tokens(
                    practice.as_str(),
                    song_search.as_str(),
                    load_songs.as_str(),
                    test_input.as_str(),
                ),
                null_or_die: None,
            },
        },
        deadsync_input::write_keymap_ini_section,
    )
}

pub fn build_default_app_config_file() -> String {
    let default = Config::default();
    let gameplay_bg_color = default.gameplay_bg_color.to_hex();
    let video_renderer = default.video_renderer.to_string();
    let practice = keycode_to_token(default.music_select_shortcut_practice);
    let song_search = keycode_to_token(default.music_select_shortcut_song_search);
    let load_songs = keycode_to_token(default.music_select_shortcut_load_songs);
    let test_input = keycode_to_token(default.music_select_shortcut_test_input);

    build_default_config_file(
        DefaultConfigFile {
            options: DefaultOptionSection {
                audio: audio_options(&default),
                audio_device: audio_device_options(&default, "Auto"),
                additional_song_folders: &[],
                never_cache_list: &[],
                system: system_options(&default),
                input_hardware: system_input_hardware_options(&default, false),
                display: display_options(
                    &default,
                    default.present_mode_policy.as_str(),
                    video_renderer.as_str(),
                ),
                runtime_io: runtime_io_options(
                    &default,
                    default.linux_audio_backend.as_str(),
                    default.lights_driver.as_str(),
                    default.lights_gameplay_pad_lights.as_str(),
                    default.lights_com_port.as_str(),
                ),
                runtime: runtime_options(&default),
                stats_overlay: stats_overlay_options(&default, None, None),
                select_music: select_music_save_options(&default),
                gameplay_bg_color: gameplay_bg_color.as_str(),
                default_noteskin: crate::machine::DEFAULT_MACHINE_NOTESKIN,
                runtime_state_ids: runtime_state_ids("", "", "", ""),
                pad_order_lines: deadsync_input_native::DEFAULT_PAD_ORDER_INI_LINES,
            },
            keymap: (),
            theme: ThemeSection {
                presentation: theme_presentation_options(&default),
                machine: machine_flow_options(&default),
                shortcuts: theme_shortcut_tokens(
                    practice.as_str(),
                    song_search.as_str(),
                    load_songs.as_str(),
                    test_input.as_str(),
                ),
                null_or_die: Some(null_or_die_options(&default)),
            },
        },
        |content, ()| deadsync_input::write_default_keymap_ini_section(content),
    )
}

fn runtime_state_ids<'a>(
    smx_p1_serial: &'a str,
    smx_p2_serial: &'a str,
    default_profile_p1: &'a str,
    default_profile_p2: &'a str,
) -> RuntimeStateIdTokens<'a> {
    RuntimeStateIdTokens {
        smx_p1_serial,
        smx_p2_serial,
        default_profile_p1,
        default_profile_p2,
    }
}

fn audio_options(cfg: &Config) -> AudioOptions {
    AudioOptions {
        visual_delay_seconds: cfg.visual_delay_seconds,
        master_volume: cfg.master_volume,
        menu_music: cfg.menu_music,
        custom_sounds_enabled: cfg.custom_sounds_enabled,
        music_volume: cfg.music_volume,
        music_wheel_switch_speed: cfg.music_wheel_switch_speed,
        sfx_volume: cfg.sfx_volume,
        assist_tick_volume: cfg.assist_tick_volume,
        output_device_index: cfg.audio_output_device_index,
        sample_rate_hz: cfg.audio_sample_rate_hz,
        rate_mod_preserves_pitch: cfg.rate_mod_preserves_pitch,
        enable_replaygain: cfg.enable_replaygain,
        write_current_screen: cfg.write_current_screen,
        tab_acceleration: cfg.tab_acceleration,
    }
}

fn audio_device_options<'a>(cfg: &Config, output_mode: &'a str) -> AudioDeviceOptions<'a> {
    AudioDeviceOptions {
        output_device_index: cfg.audio_output_device_index,
        output_mode,
        sample_rate_hz: cfg.audio_sample_rate_hz,
    }
}

fn display_options<'a>(
    cfg: &Config,
    present_mode_policy: &'a str,
    video_renderer: &'a str,
) -> DisplayOptions<'a> {
    DisplayOptions {
        width: cfg.display_width,
        height: cfg.display_height,
        monitor: cfg.display_monitor,
        fullscreen_type: cfg.fullscreen_type.as_str(),
        max_fps: cfg.max_fps,
        present_mode_policy,
        video_renderer,
        vsync: cfg.vsync,
        windowed: cfg.windowed,
    }
}

fn runtime_io_options<'a>(
    cfg: &'a Config,
    linux_audio_backend: &'a str,
    lights_driver: &'a str,
    gameplay_pad_lights: &'a str,
    lights_com_port: &'a str,
) -> RuntimeIoOptions<'a> {
    RuntimeIoOptions {
        linux_audio_backend,
        input_debounce_seconds: cfg.input_debounce_seconds,
        lights_driver,
        gameplay_pad_lights,
        lights_com_port,
    }
}

fn system_options(cfg: &Config) -> SystemOptions {
    SystemOptions {
        game_flag: cfg.game_flag,
        auto_download_unlocks: cfg.auto_download_unlocks,
        auto_populate_gs_scores: cfg.auto_populate_gs_scores,
        updater_install_enabled: cfg.updater_install_enabled,
        enable_groovestats: cfg.enable_groovestats,
        enable_arrowcloud: cfg.enable_arrowcloud,
        enable_boogiestats: cfg.enable_boogiestats,
        submit_arrowcloud_fails: cfg.submit_arrowcloud_fails,
        arrowcloud_qr_login_when: cfg.arrowcloud_qr_login_when,
        groovestats_qr_login_when: cfg.groovestats_qr_login_when,
        separate_unlocks_by_player: cfg.separate_unlocks_by_player,
        mine_hit_sound: cfg.mine_hit_sound,
        show_stats_mode: cfg.show_stats_mode,
        frame_stats_overlay_anchor: cfg.frame_stats_overlay_anchor,
        frame_stats_overlay_style: cfg.frame_stats_overlay_style,
        translated_titles: cfg.translated_titles,
        bg_brightness: cfg.bg_brightness,
        center_1player_notefield: cfg.center_1player_notefield,
        center_image_translate_x: cfg.center_image_translate_x,
        center_image_translate_y: cfg.center_image_translate_y,
        center_image_add_width: cfg.center_image_add_width,
        center_image_add_height: cfg.center_image_add_height,
        autosubmit_course_scores_individually: cfg.autosubmit_course_scores_individually,
        show_course_individual_scores: cfg.show_course_individual_scores,
        show_most_played_courses: cfg.show_most_played_courses,
        show_random_courses: cfg.show_random_courses,
        default_fail_type: cfg.default_fail_type,
        banner_cache: cfg.banner_cache,
        cdtitle_cache: cfg.cdtitle_cache,
        high_dpi: cfg.high_dpi,
        hide_mouse_cursor: cfg.hide_mouse_cursor,
        allow_shutdown_host: cfg.allow_shutdown_host,
        smx_input: cfg.smx_input,
        smx_manages_pad_config: cfg.smx_manages_pad_config,
        smx_panel_lights: cfg.smx_panel_lights,
        smx_underglow_theme: cfg.smx_underglow_theme,
        smx_underglow_grb: cfg.smx_underglow_grb,
        smx_pad_gifs_pack: cfg.smx_pad_gifs_pack,
        smx_judge_gifs_pack: cfg.smx_judge_gifs_pack,
        gfx_debug: cfg.gfx_debug,
        global_offset_seconds: cfg.global_offset_seconds,
        language_flag: cfg.language_flag,
        log_level: cfg.log_level,
        log_to_file: cfg.log_to_file,
        show_console: cfg.show_console,
    }
}

fn system_input_hardware_options(
    cfg: &Config,
    include_underglow_theme: bool,
) -> SystemInputHardwareOptions<'_> {
    SystemInputHardwareOptions {
        system: system_options(cfg),
        gamepad_backend: cfg.windows_gamepad_backend.as_str(),
        smx_default_pad_config: cfg.smx_default_pad_config.as_str(),
        smx_default_light_brightness: cfg.smx_default_light_brightness,
        smx_underglow_theme: include_underglow_theme.then_some(cfg.smx_underglow_theme),
        smx_underglow_grb: include_underglow_theme.then_some(cfg.smx_underglow_grb),
    }
}

fn runtime_options(cfg: &Config) -> RuntimeOptions {
    RuntimeOptions {
        fastload: cfg.fastload,
        cachesongs: cfg.cachesongs,
        song_parsing_threads: cfg.song_parsing_threads,
        smooth_histogram: cfg.smooth_histogram,
        shade_scatterplot_judgments: cfg.shade_scatterplot_judgments,
        arcade_options_navigation: cfg.arcade_options_navigation,
        delayed_back: cfg.delayed_back,
        three_key_navigation: cfg.three_key_navigation,
        use_fsrs: cfg.use_fsrs,
        lights_simplify_bass: cfg.lights_simplify_bass,
        only_dedicated_menu_buttons: cfg.only_dedicated_menu_buttons,
        theme_flag: cfg.theme_flag,
        software_renderer_threads: cfg.software_renderer_threads,
    }
}

fn stats_overlay_options<'a>(
    cfg: &Config,
    frame_stats_overlay_anchor: Option<&'a str>,
    frame_stats_overlay_style: Option<&'a str>,
) -> StatsOverlayOptions<'a> {
    StatsOverlayOptions {
        show_stats_mode: cfg.show_stats_mode,
        frame_stats_overlay_anchor,
        frame_stats_overlay_style,
        smooth_histogram: cfg.smooth_histogram,
        shade_scatterplot_judgments: cfg.shade_scatterplot_judgments,
    }
}

fn theme_presentation_options(cfg: &Config) -> ThemePresentationOptions {
    ThemePresentationOptions {
        simply_love_color: cfg.simply_love_color,
        show_select_music_gameplay_timer: cfg.show_select_music_gameplay_timer,
        keyboard_features: cfg.keyboard_features,
        visual_style: cfg.visual_style,
        srpg_variant: cfg.srpg_variant,
        show_video_backgrounds: cfg.show_video_backgrounds,
        random_background_mode: cfg.random_background_mode,
        zmod_rating_box_text: cfg.zmod_rating_box_text,
        show_bpm_decimal: cfg.show_bpm_decimal,
        gameplay_bpm_position: cfg.gameplay_bpm_position,
    }
}

fn machine_flow_options(cfg: &Config) -> MachineFlowOptions {
    MachineFlowOptions {
        machine_show_eval_summary: cfg.machine_show_eval_summary,
        machine_nice_sound: cfg.machine_nice_sound,
        machine_show_name_entry: cfg.machine_show_name_entry,
        machine_show_gameover: cfg.machine_show_gameover,
        machine_show_select_profile: cfg.machine_show_select_profile,
        allow_switch_profile_in_menu: cfg.allow_switch_profile_in_menu,
        machine_show_select_color: cfg.machine_show_select_color,
        machine_show_select_style: cfg.machine_show_select_style,
        machine_show_select_play_mode: cfg.machine_show_select_play_mode,
        machine_enable_replays: cfg.machine_enable_replays,
        machine_allow_per_player_global_offsets: cfg.machine_allow_per_player_global_offsets,
        machine_pack_ini_offsets: cfg.machine_pack_ini_offsets,
        machine_default_sync_offset: cfg.machine_default_sync_offset,
        machine_preferred_style: cfg.machine_preferred_style,
        machine_preferred_play_mode: cfg.machine_preferred_play_mode,
        machine_font: cfg.machine_font,
        machine_bar_color: cfg.machine_bar_color,
        machine_evaluation_style: cfg.machine_evaluation_style,
    }
}

fn null_or_die_options(cfg: &Config) -> NullOrDieOptions {
    NullOrDieOptions {
        sync_graph: cfg.null_or_die_sync_graph,
        confidence_percent: cfg.null_or_die_confidence_percent,
        pack_sync_threads: cfg.null_or_die_pack_sync_threads,
        fingerprint_ms: cfg.null_or_die_fingerprint_ms,
        window_ms: cfg.null_or_die_window_ms,
        step_ms: cfg.null_or_die_step_ms,
        magic_offset_ms: cfg.null_or_die_magic_offset_ms,
        kernel_target: cfg.null_or_die_kernel_target,
        kernel_type: cfg.null_or_die_kernel_type,
        full_spectrogram: cfg.null_or_die_full_spectrogram,
    }
}

fn select_music_options(cfg: &Config) -> SelectMusicOptions {
    SelectMusicOptions {
        breakdown_style: cfg.select_music_breakdown_style,
        show_banners: cfg.show_select_music_banners,
        show_version_overlay: cfg.show_version_overlay,
        version_overlay_side: cfg.version_overlay_side,
        show_video_banners: cfg.show_select_music_video_banners,
        show_breakdown: cfg.show_select_music_breakdown,
        show_stage_display: cfg.show_select_music_stage_display,
        show_cdtitles: cfg.show_select_music_cdtitles,
        show_wheel_grades: cfg.show_music_wheel_grades,
        show_wheel_lamps: cfg.show_music_wheel_lamps,
        itl_rank_mode: cfg.select_music_itl_rank_mode,
        itl_wheel_mode: cfg.select_music_itl_wheel_mode,
        wheel_style: cfg.select_music_wheel_style,
        song_select_bg_mode: cfg.select_music_song_select_bg_mode,
        new_pack_mode: cfg.select_music_new_pack_mode,
        show_folder_stats: cfg.show_select_music_folder_stats,
        show_previews: cfg.show_select_music_previews,
        show_preview_marker: cfg.show_select_music_preview_marker,
        preview_loop: cfg.select_music_preview_loop,
        pattern_info_mode: cfg.select_music_pattern_info_mode,
        step_artist_box_mode: cfg.select_music_step_artist_box_mode,
        show_scorebox: cfg.show_select_music_scorebox,
        scorebox_placement: cfg.select_music_scorebox_placement,
        scorebox_cycle_itg: cfg.select_music_scorebox_cycle_itg,
        scorebox_cycle_ex: cfg.select_music_scorebox_cycle_ex,
        scorebox_cycle_hard_ex: cfg.select_music_scorebox_cycle_hard_ex,
        scorebox_cycle_tournaments: cfg.select_music_scorebox_cycle_tournaments,
        chart_info_peak_nps: cfg.select_music_chart_info_peak_nps,
        chart_info_effective_bpm: cfg.select_music_chart_info_effective_bpm,
        chart_info_matrix_rating: cfg.select_music_chart_info_matrix_rating,
        auto_screenshot_eval: cfg.auto_screenshot_eval,
    }
}

fn select_music_save_options(cfg: &Config) -> SelectMusicSaveOptions {
    SelectMusicSaveOptions {
        select_music: select_music_options(cfg),
        separate_unlocks_by_player: cfg.separate_unlocks_by_player,
    }
}

fn theme_shortcut_tokens<'a>(
    practice: &'a str,
    song_search: &'a str,
    load_songs: &'a str,
    test_input: &'a str,
) -> ThemeShortcutTokens<'a> {
    ThemeShortcutTokens {
        practice,
        song_search,
        load_songs,
        test_input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::SimpleIni;

    #[test]
    fn saved_content_round_trips_smx_underglow_options() {
        let mut cfg = Config::default();
        cfg.smx_underglow_theme = true;
        cfg.smx_underglow_grb = true;
        cfg.smx_pad_gifs_pack = crate::options::SmxPackName::parse("senpi-basic");
        cfg.smx_judge_gifs_pack = crate::options::SmxPackName::parse("none");
        let content =
            build_saved_app_config_file(&cfg, &Keymap::default(), "", &[], &[], "", "", "", "");
        assert!(content.contains("SmxUnderglowTheme=1"));
        assert!(content.contains("SmxUnderglowGrb=1"));
        assert!(content.contains("SmxPadGifsPack=senpi-basic"));
        assert!(content.contains("SmxJudgeGifsPack=none"));

        let mut conf = SimpleIni::new();
        conf.load_str(&content);
        let defaults = system_options(&Config::default());
        assert!(!defaults.smx_underglow_theme && !defaults.smx_underglow_grb);
        let loaded = crate::options::load_system_options(&conf, defaults);
        assert!(loaded.smx_underglow_theme);
        assert!(loaded.smx_underglow_grb);
        assert_eq!(loaded.smx_pad_gifs_pack.as_str(), "senpi-basic");
        assert_eq!(loaded.smx_judge_gifs_pack.as_str(), "none");
    }
}
