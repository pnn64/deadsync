use super::*;
use deadlib_platform::dirs;
use deadsync_config::audio::{
    AudioDeviceOptions, AudioOptions, push_audio_device_option_lines,
    push_audio_music_option_lines, push_audio_playback_prefix_lines,
};
use deadsync_config::null_or_die::{NullOrDieOptions, push_null_or_die_option_lines};
use deadsync_config::options::{
    RuntimeOptions, SelectMusicOptions, SelectMusicSaveOptions, StatsOverlayOptions, SystemOptions,
    push_runtime_cache_option_lines, push_runtime_fastload_option_lines,
    push_runtime_lights_option_lines, push_runtime_menu_option_lines,
    push_runtime_navigation_option_lines, push_runtime_worker_theme_option_lines,
    push_select_music_option_lines, push_stats_overlay_option_lines,
    push_system_course_option_lines, push_system_diagnostics_option_lines,
    push_system_download_option_lines, push_system_online_option_lines,
};
use deadsync_config::theme::{
    MachineFlowOptions, ThemePresentationOptions, ThemeShortcutTokens, push_theme_option_lines,
};
pub(super) use deadsync_config::writer::{push_bool, push_line, push_section};

#[path = "store/defaults.rs"]
mod defaults;
#[path = "store/save.rs"]
mod save;

pub(super) fn create_default_config_file() -> Result<(), std::io::Error> {
    let path = dirs::app_dirs().config_path();
    info!(
        "'{}' not found, creating with default values.",
        path.display()
    );
    std::fs::write(path, defaults::build_content())
}

pub(super) fn current_save_content() -> String {
    let cfg = *lock_config();
    let keymap = deadsync_input::get_keymap();
    let machine_default_noteskin = MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone();
    let additional_song_folders = ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone();
    let never_cache_list = NEVER_CACHE_LIST.lock().unwrap().clone();
    let smx_p1_serial = SMX_P1_SERIAL.lock().unwrap().clone().unwrap_or_default();
    let smx_p2_serial = SMX_P2_SERIAL.lock().unwrap().clone().unwrap_or_default();
    let default_profile_p1 = DEFAULT_PROFILE_P1
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    let default_profile_p2 = DEFAULT_PROFILE_P2
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    save::build_content(
        &cfg,
        &keymap,
        &machine_default_noteskin,
        additional_song_folders.as_slice(),
        never_cache_list.as_slice(),
        &smx_p1_serial,
        &smx_p2_serial,
        &default_profile_p1,
        &default_profile_p2,
    )
}

pub(super) fn save_without_keymaps() {
    queue_save_write(current_save_content());
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

fn push_config_audio_device_lines(content: &mut String, cfg: &Config, output_mode: &str) {
    push_audio_device_option_lines(
        content,
        AudioDeviceOptions {
            output_device_index: cfg.audio_output_device_index,
            output_mode,
            sample_rate_hz: cfg.audio_sample_rate_hz,
        },
    );
}

fn push_config_audio_playback_prefix_lines(content: &mut String, cfg: &Config) {
    push_audio_playback_prefix_lines(content, audio_options(cfg));
}

fn push_config_audio_music_lines(content: &mut String, cfg: &Config) {
    push_audio_music_option_lines(content, audio_options(cfg));
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
        gfx_debug: cfg.gfx_debug,
        global_offset_seconds: cfg.global_offset_seconds,
        language_flag: cfg.language_flag,
        log_level: cfg.log_level,
        log_to_file: cfg.log_to_file,
        show_console: cfg.show_console,
    }
}

fn push_config_system_download_lines(content: &mut String, cfg: &Config) {
    push_system_download_option_lines(content, system_options(cfg));
}

fn push_config_system_course_lines(content: &mut String, cfg: &Config) {
    push_system_course_option_lines(content, system_options(cfg));
}

fn push_config_system_online_lines(content: &mut String, cfg: &Config) {
    push_system_online_option_lines(content, system_options(cfg));
}

fn push_config_system_diagnostics_lines(content: &mut String, cfg: &Config) {
    push_system_diagnostics_option_lines(content, system_options(cfg));
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

fn push_config_runtime_cache_lines(content: &mut String, cfg: &Config) {
    push_runtime_cache_option_lines(content, runtime_options(cfg));
}

fn push_config_runtime_fastload_lines(content: &mut String, cfg: &Config) {
    push_runtime_fastload_option_lines(content, runtime_options(cfg));
}

fn push_config_runtime_navigation_lines(content: &mut String, cfg: &Config) {
    push_runtime_navigation_option_lines(content, runtime_options(cfg));
}

fn push_config_runtime_lights_lines(content: &mut String, cfg: &Config) {
    push_runtime_lights_option_lines(content, runtime_options(cfg));
}

fn push_config_runtime_menu_lines(content: &mut String, cfg: &Config) {
    push_runtime_menu_option_lines(content, runtime_options(cfg));
}

fn push_config_runtime_worker_theme_lines(content: &mut String, cfg: &Config) {
    push_runtime_worker_theme_option_lines(content, runtime_options(cfg));
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

fn push_config_stats_overlay_lines(
    content: &mut String,
    cfg: &Config,
    include_frame_options: bool,
) {
    push_stats_overlay_option_lines(
        content,
        if include_frame_options {
            stats_overlay_options(
                cfg,
                Some(cfg.frame_stats_overlay_anchor),
                Some(cfg.frame_stats_overlay_style),
            )
        } else {
            stats_overlay_options(cfg, None, None)
        },
    );
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

fn push_config_null_or_die_lines(content: &mut String, cfg: &Config) {
    push_null_or_die_option_lines(content, null_or_die_options(cfg));
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

fn push_config_select_music_lines(content: &mut String, cfg: &Config) {
    push_select_music_option_lines(
        content,
        SelectMusicSaveOptions {
            select_music: select_music_options(cfg),
            separate_unlocks_by_player: cfg.separate_unlocks_by_player,
        },
    );
}

fn push_config_theme_lines(content: &mut String, cfg: &Config) {
    let practice = keycode_to_token(cfg.music_select_shortcut_practice);
    let song_search = keycode_to_token(cfg.music_select_shortcut_song_search);
    let load_songs = keycode_to_token(cfg.music_select_shortcut_load_songs);
    let test_input = keycode_to_token(cfg.music_select_shortcut_test_input);
    push_theme_option_lines(
        content,
        theme_presentation_options(cfg),
        machine_flow_options(cfg),
        ThemeShortcutTokens {
            practice: &practice,
            song_search: &song_search,
            load_songs: &load_songs,
            test_input: &test_input,
        },
    );
}
