use crate::app_config::{Config, DisplayMode};
use crate::app_update as config_update;
use crate::runtime::{RUNTIME_CONFIG, get, save_without_keymaps};
use crate::theme::{
    ArrowCloudQrLoginWhen, BreakdownStyle, DefaultFailType, DefaultSyncOffset, GameFlag,
    GameplayBpmPosition, GrooveStatsQrLoginWhen, LanguageFlag, MachineBarColor,
    MachineEvaluationStyle, MachineFont, MachinePreferredPlayMode, MachinePreferredPlayStyle,
    NewPackMode, RandomBackgroundMode, SelectMusicItlRankMode, SelectMusicItlWheelMode,
    SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement, SelectMusicSongSelectBgMode,
    SelectMusicStepArtistBoxMode, SelectMusicWheelStyle, SrpgVariant, SyncGraphMode, ThemeFlag,
    VersionOverlaySide, VisualStyle,
};
use deadlib_platform::logging;
use deadlib_render::{BackendType, PresentModePolicy};
use deadsync_audio::AudioOutputMode;
#[cfg(target_os = "linux")]
use deadsync_audio::LinuxAudioBackend;
use deadsync_input_native::WindowsPadBackend;
use deadsync_lights::{DriverKind as LightsDriverKind, GameplayPadLightMode};
use log::warn;
use null_or_die::{BiasKernel, KernelTarget};

macro_rules! update_config_fn {
    ($(#[$meta:meta])* $vis:vis fn $name:ident($arg:ident: $ty:ty) => $setter:ident) => {
        $(#[$meta])*
        $vis fn $name($arg: $ty) {
            update_config(|cfg| config_update::$setter(cfg, $arg));
        }
    };
}

macro_rules! update_config_fn2 {
    ($(#[$meta:meta])* $vis:vis fn $name:ident($a:ident: $ta:ty, $b:ident: $tb:ty) => $setter:ident) => {
        $(#[$meta])*
        $vis fn $name($a: $ta, $b: $tb) {
            update_config(|cfg| config_update::$setter(cfg, $a, $b));
        }
    };
}

macro_rules! runtime_config_fn {
    ($(#[$meta:meta])* $vis:vis fn $name:ident($arg:ident: $ty:ty) => $method:ident) => {
        $(#[$meta])*
        $vis fn $name($arg: $ty) {
            if RUNTIME_CONFIG.$method($arg) {
                save_without_keymaps();
            }
        }
    };
}

macro_rules! runtime_config_fn2 {
    ($(#[$meta:meta])* $vis:vis fn $name:ident($a:ident: $ta:ty, $b:ident: $tb:ty) => $method:ident) => {
        $(#[$meta])*
        $vis fn $name($a: $ta, $b: $tb) {
            if RUNTIME_CONFIG.$method($a, $b) {
                save_without_keymaps();
            }
        }
    };
}

fn update_config(apply: impl FnOnce(&mut Config) -> bool) -> bool {
    let changed = RUNTIME_CONFIG.update_config(apply);
    if changed {
        save_without_keymaps();
    }
    changed
}

update_config_fn!(pub fn update_menu_music(enabled: bool) => set_menu_music);
update_config_fn!(pub fn update_software_renderer_threads(threads: u8) => set_software_renderer_threads);
update_config_fn!(pub fn update_audio_sample_rate(rate: Option<u32>) => set_audio_sample_rate);
update_config_fn!(pub fn update_audio_output_device(index: Option<u16>) => set_audio_output_device);
update_config_fn!(pub fn update_audio_output_mode(mode: AudioOutputMode) => set_audio_output_mode);
update_config_fn!(#[cfg(target_os = "linux")] pub fn update_linux_audio_backend(backend: LinuxAudioBackend) => set_linux_audio_backend);
update_config_fn!(pub fn update_mine_hit_sound(enabled: bool) => set_mine_hit_sound);
update_config_fn!(pub fn update_music_wheel_switch_speed(speed: u8) => set_music_wheel_switch_speed);
update_config_fn!(pub fn update_translated_titles(enabled: bool) => set_translated_titles);

update_config_fn!(pub fn update_display_mode(mode: DisplayMode) => set_display_mode);
update_config_fn2!(pub fn update_display_resolution(width: u32, height: u32) => set_display_resolution);
update_config_fn!(pub fn update_display_monitor(monitor: usize) => set_display_monitor);
update_config_fn!(pub fn update_video_renderer(renderer: BackendType) => set_video_renderer);
update_config_fn!(pub fn update_gfx_debug(enabled: bool) => set_gfx_debug);
update_config_fn!(pub fn update_high_dpi(enabled: bool) => set_high_dpi);
update_config_fn!(pub fn update_hide_mouse_cursor(enabled: bool) => set_hide_mouse_cursor);
update_config_fn!(pub fn update_global_offset(offset: f32) => set_global_offset_seconds);
update_config_fn!(pub fn update_visual_delay_seconds(delay: f32) => set_visual_delay_seconds);
update_config_fn!(pub fn update_vsync(enabled: bool) => set_vsync);
update_config_fn!(pub fn update_max_fps(max_fps: u16) => set_max_fps);
update_config_fn!(pub fn update_present_mode_policy(mode: PresentModePolicy) => set_present_mode_policy);
update_config_fn!(pub fn update_show_stats_mode(mode: u8) => set_show_stats_mode);
update_config_fn!(pub fn update_frame_stats_overlay_anchor(key: &'static str) => set_frame_stats_overlay_anchor);
update_config_fn!(pub fn update_frame_stats_overlay_style(key: &'static str) => set_frame_stats_overlay_style);
update_config_fn!(#[cfg(target_os = "windows")] pub fn update_windows_gamepad_backend(backend: WindowsPadBackend) => set_windows_gamepad_backend);
update_config_fn!(pub fn update_bg_brightness(brightness: f32) => set_bg_brightness);
update_config_fn!(pub fn update_center_1player_notefield(enabled: bool) => set_center_1player_notefield);
update_config_fn!(pub fn update_banner_cache(enabled: bool) => set_banner_cache);
update_config_fn!(pub fn update_cdtitle_cache(enabled: bool) => set_cdtitle_cache);
update_config_fn!(pub fn update_song_parsing_threads(threads: u8) => set_song_parsing_threads);
update_config_fn!(pub fn update_cache_songs(enabled: bool) => set_cache_songs);
update_config_fn!(pub fn update_fastload(enabled: bool) => set_fastload);

update_config_fn!(pub fn update_arcade_options_navigation(enabled: bool) => set_arcade_options_navigation);
update_config_fn!(pub fn update_delayed_back(enabled: bool) => set_delayed_back);
update_config_fn!(pub fn update_use_fsrs(enabled: bool) => set_use_fsrs);
runtime_config_fn!(pub fn update_smx_input(enabled: bool) => update_smx_input);
runtime_config_fn!(pub fn update_smx_manages_pad_config(enabled: bool) => update_smx_manages_pad_config);
runtime_config_fn!(pub fn update_smx_panel_lights(enabled: bool) => update_smx_panel_lights);
runtime_config_fn!(pub fn update_smx_pad_gifs_pack(pack: crate::options::SmxPackName) => update_smx_pad_gifs_pack);
runtime_config_fn!(pub fn update_smx_judge_gifs_pack(pack: crate::options::SmxPackName) => update_smx_judge_gifs_pack);
runtime_config_fn!(pub fn update_smx_default_pad_config(preset: deadsync_smx::SmxPadPreset) => update_smx_default_pad_config);
runtime_config_fn!(pub fn update_smx_default_light_brightness(percent: u8) => update_smx_default_light_brightness);
runtime_config_fn2!(pub fn update_default_profiles(p1: Option<String>, p2: Option<String>) => set_default_profiles);
update_config_fn!(pub fn update_keyboard_features(enabled: bool) => set_keyboard_features);
update_config_fn!(pub fn update_visual_style(style: VisualStyle) => set_visual_style);
update_config_fn!(pub fn update_srpg_variant(variant: SrpgVariant) => set_srpg_variant);
update_config_fn!(pub fn update_machine_show_select_profile(enabled: bool) => set_machine_show_select_profile);
update_config_fn!(pub fn update_allow_switch_profile_in_menu(enabled: bool) => set_allow_switch_profile_in_menu);
update_config_fn!(pub fn update_show_video_backgrounds(enabled: bool) => set_show_video_backgrounds);
update_config_fn!(pub fn update_random_background_mode(mode: RandomBackgroundMode) => set_random_background_mode);
update_config_fn!(pub fn update_write_current_screen(enabled: bool) => set_write_current_screen);
update_config_fn!(pub fn update_machine_show_select_color(enabled: bool) => set_machine_show_select_color);
update_config_fn!(pub fn update_machine_show_select_style(enabled: bool) => set_machine_show_select_style);
update_config_fn!(pub fn update_machine_show_select_play_mode(enabled: bool) => set_machine_show_select_play_mode);
update_config_fn!(pub fn update_machine_preferred_style(style: MachinePreferredPlayStyle) => set_machine_preferred_style);
update_config_fn!(pub fn update_machine_preferred_play_mode(mode: MachinePreferredPlayMode) => set_machine_preferred_play_mode);
update_config_fn!(pub fn update_machine_show_eval_summary(enabled: bool) => set_machine_show_eval_summary);
update_config_fn!(pub fn update_machine_nice_sound(enabled: bool) => set_machine_nice_sound);
update_config_fn!(pub fn update_machine_show_name_entry(enabled: bool) => set_machine_show_name_entry);
update_config_fn!(pub fn update_machine_show_gameover(enabled: bool) => set_machine_show_gameover);
update_config_fn!(pub fn update_machine_enable_replays(enabled: bool) => set_machine_enable_replays);
update_config_fn!(pub fn update_machine_allow_per_player_global_offsets(enabled: bool) => set_machine_allow_per_player_global_offsets);
update_config_fn!(pub fn update_machine_pack_ini_offsets(enabled: bool) => set_machine_pack_ini_offsets);
update_config_fn!(pub fn update_machine_default_sync_offset(offset: DefaultSyncOffset) => set_machine_default_sync_offset);
update_config_fn!(pub fn update_enable_groovestats(enabled: bool) => set_enable_groovestats);
update_config_fn!(pub fn update_enable_boogiestats(enabled: bool) => set_enable_boogiestats);
update_config_fn!(pub fn update_enable_arrowcloud(enabled: bool) => set_enable_arrowcloud);
update_config_fn!(pub fn update_submit_arrowcloud_fails(enabled: bool) => set_submit_arrowcloud_fails);
update_config_fn!(pub fn update_arrowcloud_qr_login_when(when: ArrowCloudQrLoginWhen) => set_arrowcloud_qr_login_when);
update_config_fn!(pub fn update_groovestats_qr_login_when(when: GrooveStatsQrLoginWhen) => set_groovestats_qr_login_when);
update_config_fn!(pub fn update_auto_download_unlocks(enabled: bool) => set_auto_download_unlocks);
update_config_fn!(pub fn update_auto_populate_gs_scores(enabled: bool) => set_auto_populate_gs_scores);
update_config_fn!(pub fn update_separate_unlocks_by_player(enabled: bool) => set_separate_unlocks_by_player);
update_config_fn!(pub fn update_game_flag(flag: GameFlag) => set_game_flag);
update_config_fn!(pub fn update_theme_flag(flag: ThemeFlag) => set_theme_flag);
update_config_fn!(pub fn update_language_flag(flag: LanguageFlag) => set_language_flag);
runtime_config_fn!(pub fn update_machine_default_noteskin(noteskin: &str) => set_machine_default_noteskin);

update_config_fn!(pub fn update_machine_font(font: MachineFont) => set_machine_font);
update_config_fn!(pub fn update_machine_bar_color(color: MachineBarColor) => set_machine_bar_color);
update_config_fn!(pub fn update_machine_evaluation_style(style: MachineEvaluationStyle) => set_machine_evaluation_style);
update_config_fn!(pub fn update_select_music_breakdown_style(style: BreakdownStyle) => set_select_music_breakdown_style);
update_config_fn!(pub fn update_show_select_music_breakdown(enabled: bool) => set_show_select_music_breakdown);
update_config_fn!(pub fn update_show_select_music_banners(enabled: bool) => set_show_select_music_banners);
update_config_fn!(pub fn update_show_version_overlay(enabled: bool) => set_show_version_overlay);
update_config_fn!(pub fn update_version_overlay_side(side: VersionOverlaySide) => set_version_overlay_side);
update_config_fn!(pub fn update_show_select_music_video_banners(enabled: bool) => set_show_select_music_video_banners);
update_config_fn!(pub fn update_show_select_music_cdtitles(enabled: bool) => set_show_select_music_cdtitles);
update_config_fn!(pub fn update_show_music_wheel_grades(enabled: bool) => set_show_music_wheel_grades);
update_config_fn!(pub fn update_show_music_wheel_lamps(enabled: bool) => set_show_music_wheel_lamps);
update_config_fn!(pub fn update_select_music_itl_rank_mode(mode: SelectMusicItlRankMode) => set_select_music_itl_rank_mode);
update_config_fn!(pub fn update_select_music_itl_wheel_mode(mode: SelectMusicItlWheelMode) => set_select_music_itl_wheel_mode);
update_config_fn!(pub fn update_select_music_wheel_style(style: SelectMusicWheelStyle) => set_select_music_wheel_style);
update_config_fn!(pub fn update_select_music_song_select_bg_mode(mode: SelectMusicSongSelectBgMode) => set_select_music_song_select_bg_mode);
update_config_fn!(pub fn update_select_music_new_pack_mode(mode: NewPackMode) => set_select_music_new_pack_mode);
update_config_fn!(pub fn update_show_select_music_folder_stats(enabled: bool) => set_show_select_music_folder_stats);
update_config_fn!(pub fn update_show_select_music_previews(enabled: bool) => set_show_select_music_previews);
update_config_fn!(pub fn update_show_select_music_preview_marker(enabled: bool) => set_show_select_music_preview_marker);
update_config_fn!(pub fn update_select_music_preview_loop(enabled: bool) => set_select_music_preview_loop);
update_config_fn!(pub fn update_select_music_pattern_info_mode(mode: SelectMusicPatternInfoMode) => set_select_music_pattern_info_mode);
update_config_fn!(pub fn update_select_music_step_artist_box_mode(mode: SelectMusicStepArtistBoxMode) => set_select_music_step_artist_box_mode);
update_config_fn!(pub fn update_show_select_music_gameplay_timer(enabled: bool) => set_show_select_music_gameplay_timer);
update_config_fn!(pub fn update_show_select_music_stage_display(enabled: bool) => set_show_select_music_stage_display);
update_config_fn!(pub fn update_show_select_music_scorebox(enabled: bool) => set_show_select_music_scorebox);
update_config_fn!(pub fn update_select_music_scorebox_placement(mode: SelectMusicScoreboxPlacement) => set_select_music_scorebox_placement);
update_config_fn!(pub fn update_select_music_scorebox_cycle_itg(enabled: bool) => set_select_music_scorebox_cycle_itg);
update_config_fn!(pub fn update_select_music_scorebox_cycle_ex(enabled: bool) => set_select_music_scorebox_cycle_ex);
update_config_fn!(pub fn update_select_music_scorebox_cycle_hard_ex(enabled: bool) => set_select_music_scorebox_cycle_hard_ex);
update_config_fn!(pub fn update_select_music_scorebox_cycle_tournaments(enabled: bool) => set_select_music_scorebox_cycle_tournaments);
update_config_fn!(pub fn update_select_music_chart_info_peak_nps(enabled: bool) => set_select_music_chart_info_peak_nps);
update_config_fn!(pub fn update_select_music_chart_info_effective_bpm(enabled: bool) => set_select_music_chart_info_effective_bpm);
update_config_fn!(pub fn update_select_music_chart_info_matrix_rating(enabled: bool) => set_select_music_chart_info_matrix_rating);
update_config_fn!(pub fn update_auto_screenshot_eval(mask: u8) => set_auto_screenshot_eval);
update_config_fn!(pub fn update_show_random_courses(enabled: bool) => set_show_random_courses);
update_config_fn!(pub fn update_show_most_played_courses(enabled: bool) => set_show_most_played_courses);
update_config_fn!(pub fn update_show_course_individual_scores(enabled: bool) => set_show_course_individual_scores);
update_config_fn!(pub fn update_autosubmit_course_scores_individually(enabled: bool) => set_autosubmit_course_scores_individually);
update_config_fn!(pub fn update_zmod_rating_box_text(enabled: bool) => set_zmod_rating_box_text);
update_config_fn!(pub fn update_show_bpm_decimal(enabled: bool) => set_show_bpm_decimal);
update_config_fn!(pub fn update_gameplay_bpm_position(position: GameplayBpmPosition) => set_gameplay_bpm_position);
update_config_fn!(pub fn update_default_fail_type(fail_type: DefaultFailType) => set_default_fail_type);

update_config_fn!(pub fn update_null_or_die_sync_graph(mode: SyncGraphMode) => set_null_or_die_sync_graph);
update_config_fn!(pub fn update_null_or_die_confidence_percent(value: u8) => set_null_or_die_confidence_percent);
update_config_fn!(pub fn update_null_or_die_pack_sync_threads(threads: u8) => set_null_or_die_pack_sync_threads);
update_config_fn!(pub fn update_null_or_die_fingerprint_ms(value: f64) => set_null_or_die_fingerprint_ms);
update_config_fn!(pub fn update_null_or_die_window_ms(value: f64) => set_null_or_die_window_ms);
update_config_fn!(pub fn update_null_or_die_step_ms(value: f64) => set_null_or_die_step_ms);
update_config_fn!(pub fn update_null_or_die_magic_offset_ms(value: f64) => set_null_or_die_magic_offset_ms);
update_config_fn!(pub fn update_null_or_die_kernel_target(value: KernelTarget) => set_null_or_die_kernel_target);
update_config_fn!(pub fn update_null_or_die_kernel_type(value: BiasKernel) => set_null_or_die_kernel_type);
update_config_fn!(pub fn update_null_or_die_full_spectrogram(enabled: bool) => set_null_or_die_full_spectrogram);

update_config_fn!(pub fn update_lights_driver(driver: LightsDriverKind) => set_lights_driver);
update_config_fn!(pub fn update_lights_gameplay_pad_lights(mode: GameplayPadLightMode) => set_lights_gameplay_pad_lights);
update_config_fn!(pub fn update_lights_simplify_bass(enabled: bool) => set_lights_simplify_bass);

pub fn update_master_volume(volume: u8) {
    if let Some(levels) = RUNTIME_CONFIG.update_master_volume(volume) {
        deadsync_audio::set_audio_mix_levels(levels);
        save_without_keymaps();
    }
}

pub fn update_music_volume(volume: u8) {
    if let Some(levels) = RUNTIME_CONFIG.update_music_volume(volume) {
        deadsync_audio::set_audio_mix_levels(levels);
        save_without_keymaps();
    }
}

pub fn update_sfx_volume(volume: u8) {
    if let Some(levels) = RUNTIME_CONFIG.update_sfx_volume(volume) {
        deadsync_audio::set_audio_mix_levels(levels);
        save_without_keymaps();
    }
}

pub fn update_assist_tick_volume(volume: u8) {
    if let Some(levels) = RUNTIME_CONFIG.update_assist_tick_volume(volume) {
        deadsync_audio::set_audio_mix_levels(levels);
        save_without_keymaps();
    }
}

pub fn update_rate_mod_preserves_pitch(enabled: bool) {
    if RUNTIME_CONFIG.update_rate_mod_preserves_pitch(enabled) {
        deadsync_audio_stream::set_preserve_pitch_enabled(enabled);
        save_without_keymaps();
    }
}

pub fn update_enable_replaygain(enabled: bool) {
    if RUNTIME_CONFIG.update_enable_replaygain(enabled) {
        deadsync_audio_stream::set_replaygain_enabled(enabled);
        save_without_keymaps();
    }
}

#[inline(always)]
fn dedicated_menu_buttons_supported(three_key_navigation: bool) -> bool {
    deadsync_input::any_player_has_dedicated_menu_buttons_for_mode(three_key_navigation)
}

pub fn update_input_debounce_seconds(seconds: f32) {
    if let Some(seconds) = RUNTIME_CONFIG.update_input_debounce_seconds(seconds) {
        deadsync_input::set_input_debounce_seconds(seconds);
        save_without_keymaps();
    }
}

pub fn update_three_key_navigation(enabled: bool) {
    let update = RUNTIME_CONFIG
        .update_three_key_navigation(enabled, dedicated_menu_buttons_supported(enabled));
    if !update.changed {
        return;
    }
    if update.dedicated.disabled_by_missing_bindings {
        warn!(
            "three_key_navigation changed to {} but no player has the required dedicated menu buttons mapped - disabling dedicated-only menu navigation.",
            crate::update::dedicated_menu_navigation_label(enabled)
        );
    }
    deadsync_input::set_only_dedicated_menu_buttons(update.dedicated.enabled);
    save_without_keymaps();
}

pub fn update_smx_underglow_theme(enabled: bool) {
    if !RUNTIME_CONFIG.update_smx_underglow_theme(enabled) {
        return;
    }
    save_without_keymaps();
    if enabled {
        send_smx_underglow_color();
    }
}

pub fn update_smx_underglow_grb(grb: bool) {
    if !RUNTIME_CONFIG.update_smx_underglow_grb(grb) {
        return;
    }
    save_without_keymaps();
    deadsync_smx::set_platform_lights_grb(grb);
    send_smx_underglow_color();
}

pub fn update_smx_pad_assignment(p1_serial: Option<String>, p2_serial: Option<String>) {
    if !RUNTIME_CONFIG.set_smx_pad_assignment(p1_serial.clone(), p2_serial.clone()) {
        return;
    }
    deadsync_smx::set_player_assignment(p1_serial, p2_serial);
    save_without_keymaps();
}

pub fn swap_smx_pad_assignment() -> bool {
    let [s0, s1] = deadsync_smx::connected_serials();
    if let (Some(a), Some(b)) = (s0, s1) {
        update_smx_pad_assignment(Some(b), Some(a));
        true
    } else {
        false
    }
}

pub fn update_only_dedicated_menu_buttons(enabled: bool) {
    let three_key_navigation = get().three_key_navigation;
    let update = RUNTIME_CONFIG.update_only_dedicated_menu_buttons(
        enabled,
        dedicated_menu_buttons_supported(three_key_navigation),
    );
    if update.dedicated.disabled_by_missing_bindings {
        warn!(
            "only_dedicated_menu_buttons requires dedicated menu buttons for {} mode, but no player has the required bindings mapped - leaving gameplay button fallback enabled.",
            crate::update::dedicated_menu_navigation_label(update.three_key_navigation)
        );
    }
    if !update.changed {
        return;
    }
    deadsync_input::set_only_dedicated_menu_buttons(update.dedicated.enabled);
    save_without_keymaps();
}

pub fn update_simply_love_color(index: i32) {
    if RUNTIME_CONFIG.update_simply_love_color(index) {
        send_smx_underglow_color();
        save_without_keymaps();
    }
}

pub fn send_smx_underglow_color() {
    let lone_pad = deadsync_smx::get_info(0).connected ^ deadsync_smx::get_info(1).connected;
    if let Some(plan) = RUNTIME_CONFIG.smx_underglow_colors(lone_pad) {
        deadsync_smx::set_platform_lights_grb(plan.grb);
        deadsync_smx::set_platform_lights_solid(plan.colors);
    }
}

pub fn update_log_level(level: crate::theme::LogLevel) {
    log::set_max_level(level.as_level_filter());
    if RUNTIME_CONFIG.update_log_level(level) {
        save_without_keymaps();
    }
}

pub fn update_log_to_file(enabled: bool) {
    logging::set_file_logging_enabled(enabled);
    if RUNTIME_CONFIG.update_log_to_file(enabled) {
        save_without_keymaps();
    }
}

pub fn update_overscan(translate_x: i32, translate_y: i32, add_width: i32, add_height: i32) {
    let dirty = RUNTIME_CONFIG.update_overscan(translate_x, translate_y, add_width, add_height);
    deadlib_present::space::set_overscan(translate_x, translate_y, add_width, add_height);
    if dirty {
        save_without_keymaps();
    }
}
