use super::*;

pub fn update_display_mode(mode: DisplayMode) {
    let mut dirty = false;
    {
        let mut cfg = lock_config();
        match mode {
            DisplayMode::Windowed => {
                if !cfg.windowed {
                    cfg.windowed = true;
                    dirty = true;
                }
            }
            DisplayMode::Fullscreen(fullscreen_type) => {
                if cfg.windowed {
                    cfg.windowed = false;
                    dirty = true;
                }
                if cfg.fullscreen_type != fullscreen_type {
                    cfg.fullscreen_type = fullscreen_type;
                    dirty = true;
                }
            }
        }
    }
    if dirty {
        save_without_keymaps();
    }
}

pub fn update_display_resolution(width: u32, height: u32) {
    let mut dirty = false;
    {
        let mut cfg = lock_config();
        if cfg.display_width != width {
            cfg.display_width = width;
            dirty = true;
        }
        if cfg.display_height != height {
            cfg.display_height = height;
            dirty = true;
        }
    }
    if dirty {
        save_without_keymaps();
    }
}

pub fn update_display_monitor(monitor: usize) {
    {
        let mut cfg = lock_config();
        if cfg.display_monitor == monitor {
            return;
        }
        cfg.display_monitor = monitor;
    }
    save_without_keymaps();
}

pub fn update_video_renderer(renderer: BackendType) {
    {
        let mut cfg = lock_config();
        if cfg.video_renderer == renderer {
            return;
        }
        cfg.video_renderer = renderer;
    }
    save_without_keymaps();
}

pub fn update_gfx_debug(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.gfx_debug == enabled {
            return;
        }
        cfg.gfx_debug = enabled;
    }
    save_without_keymaps();
}

pub fn update_simply_love_color(index: i32) {
    {
        let mut cfg = lock_config();
        if cfg.simply_love_color == index {
            return;
        }
        cfg.simply_love_color = index;
    }
    save_without_keymaps();
}

pub fn update_global_offset(offset: f32) {
    {
        let mut cfg = lock_config();
        if (cfg.global_offset_seconds - offset).abs() < f32::EPSILON {
            return;
        }
        cfg.global_offset_seconds = offset;
    }
    save_without_keymaps();
}

pub fn update_visual_delay_seconds(delay: f32) {
    let clamped = delay.clamp(-1.0, 1.0);
    {
        let mut cfg = lock_config();
        if (cfg.visual_delay_seconds - clamped).abs() < f32::EPSILON {
            return;
        }
        cfg.visual_delay_seconds = clamped;
    }
    save_without_keymaps();
}

pub fn update_vsync(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.vsync == enabled {
            return;
        }
        cfg.vsync = enabled;
    }
    save_without_keymaps();
}

pub fn update_max_fps(max_fps: u16) {
    {
        let mut cfg = lock_config();
        if cfg.max_fps == max_fps {
            return;
        }
        cfg.max_fps = max_fps;
    }
    save_without_keymaps();
}

pub fn update_present_mode_policy(mode: PresentModePolicy) {
    {
        let mut cfg = lock_config();
        if cfg.present_mode_policy == mode {
            return;
        }
        cfg.present_mode_policy = mode;
    }
    save_without_keymaps();
}

pub fn update_show_stats_mode(mode: u8) {
    let mode = mode.min(3);
    {
        let mut cfg = lock_config();
        if cfg.show_stats_mode == mode {
            return;
        }
        cfg.show_stats_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_log_level(level: LogLevel) {
    log::set_max_level(level.as_level_filter());
    {
        let mut cfg = lock_config();
        if cfg.log_level == level {
            return;
        }
        cfg.log_level = level;
    }
    save_without_keymaps();
}

pub fn update_log_to_file(enabled: bool) {
    logging::set_file_logging_enabled(enabled);
    {
        let mut cfg = lock_config();
        if cfg.log_to_file == enabled {
            return;
        }
        cfg.log_to_file = enabled;
    }
    save_without_keymaps();
}

#[cfg(target_os = "windows")]
pub fn update_windows_gamepad_backend(backend: WindowsPadBackend) {
    {
        let mut cfg = lock_config();
        if cfg.windows_gamepad_backend == backend {
            return;
        }
        cfg.windows_gamepad_backend = backend;
    }
    save_without_keymaps();
}

pub fn update_bg_brightness(brightness: f32) {
    let clamped = brightness.clamp(0.0, 1.0);
    {
        let mut cfg = lock_config();
        if (cfg.bg_brightness - clamped).abs() < f32::EPSILON {
            return;
        }
        cfg.bg_brightness = clamped;
    }
    save_without_keymaps();
}

pub fn update_center_1player_notefield(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.center_1player_notefield == enabled {
            return;
        }
        cfg.center_1player_notefield = enabled;
    }
    save_without_keymaps();
}

pub fn update_banner_cache(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.banner_cache == enabled {
            return;
        }
        cfg.banner_cache = enabled;
    }
    save_without_keymaps();
}

pub fn update_cdtitle_cache(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.cdtitle_cache == enabled {
            return;
        }
        cfg.cdtitle_cache = enabled;
    }
    save_without_keymaps();
}

pub fn update_song_parsing_threads(threads: u8) {
    {
        let mut cfg = lock_config();
        if cfg.song_parsing_threads == threads {
            return;
        }
        cfg.song_parsing_threads = threads;
    }
    save_without_keymaps();
}

pub fn update_cache_songs(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.cachesongs == enabled {
            return;
        }
        cfg.cachesongs = enabled;
    }
    save_without_keymaps();
}

pub fn update_fastload(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.fastload == enabled {
            return;
        }
        cfg.fastload = enabled;
    }
    save_without_keymaps();
}

pub fn update_master_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.master_volume == vol {
            return;
        }
        cfg.master_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_music_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.music_volume == vol {
            return;
        }
        cfg.music_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_menu_music(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.menu_music == enabled {
            return;
        }
        cfg.menu_music = enabled;
    }
    save_without_keymaps();
}

pub fn update_software_renderer_threads(threads: u8) {
    {
        let mut cfg = lock_config();
        if cfg.software_renderer_threads == threads {
            return;
        }
        cfg.software_renderer_threads = threads;
    }
    save_without_keymaps();
}

pub fn update_sfx_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.sfx_volume == vol {
            return;
        }
        cfg.sfx_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_assist_tick_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.assist_tick_volume == vol {
            return;
        }
        cfg.assist_tick_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_audio_sample_rate(rate: Option<u32>) {
    {
        let mut cfg = lock_config();
        if cfg.audio_sample_rate_hz == rate {
            return;
        }
        cfg.audio_sample_rate_hz = rate;
    }
    save_without_keymaps();
}

pub fn update_audio_output_device(index: Option<u16>) {
    {
        let mut cfg = lock_config();
        if cfg.audio_output_device_index == index {
            return;
        }
        cfg.audio_output_device_index = index;
    }
    save_without_keymaps();
}

pub fn update_audio_output_mode(mode: AudioOutputMode) {
    {
        let mut cfg = lock_config();
        if cfg.audio_output_mode == mode {
            return;
        }
        cfg.audio_output_mode = mode;
    }
    save_without_keymaps();
}

#[cfg(target_os = "linux")]
pub fn update_linux_audio_backend(backend: LinuxAudioBackend) {
    {
        let mut cfg = lock_config();
        if cfg.linux_audio_backend == backend {
            return;
        }
        cfg.linux_audio_backend = backend;
    }
    save_without_keymaps();
}

pub fn update_mine_hit_sound(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.mine_hit_sound == enabled {
            return;
        }
        cfg.mine_hit_sound = enabled;
    }
    save_without_keymaps();
}

pub fn update_music_wheel_switch_speed(speed: u8) {
    let speed = speed.max(1);
    {
        let mut cfg = lock_config();
        if cfg.music_wheel_switch_speed == speed {
            return;
        }
        cfg.music_wheel_switch_speed = speed;
    }
    save_without_keymaps();
}

pub fn update_translated_titles(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.translated_titles == enabled {
            return;
        }
        cfg.translated_titles = enabled;
    }
    save_without_keymaps();
}

pub fn update_rate_mod_preserves_pitch(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.rate_mod_preserves_pitch == enabled {
            return;
        }
        cfg.rate_mod_preserves_pitch = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_breakdown_style(style: BreakdownStyle) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_breakdown_style == style {
            return;
        }
        cfg.select_music_breakdown_style = style;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_breakdown(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_breakdown == enabled {
            return;
        }
        cfg.show_select_music_breakdown = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_banners(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_banners == enabled {
            return;
        }
        cfg.show_select_music_banners = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_video_banners(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_video_banners == enabled {
            return;
        }
        cfg.show_select_music_video_banners = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_cdtitles(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_cdtitles == enabled {
            return;
        }
        cfg.show_select_music_cdtitles = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_grades(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_music_wheel_grades == enabled {
            return;
        }
        cfg.show_music_wheel_grades = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_lamps(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_music_wheel_lamps == enabled {
            return;
        }
        cfg.show_music_wheel_lamps = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_itl_wheel_mode(mode: SelectMusicItlWheelMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_itl_wheel_mode == mode {
            return;
        }
        cfg.select_music_itl_wheel_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_select_music_new_pack_mode(mode: NewPackMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_new_pack_mode == mode {
            return;
        }
        cfg.select_music_new_pack_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_previews(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_previews == enabled {
            return;
        }
        cfg.show_select_music_previews = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_preview_marker(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_preview_marker == enabled {
            return;
        }
        cfg.show_select_music_preview_marker = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_preview_loop(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_preview_loop == enabled {
            return;
        }
        cfg.select_music_preview_loop = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_pattern_info_mode(mode: SelectMusicPatternInfoMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_pattern_info_mode == mode {
            return;
        }
        cfg.select_music_pattern_info_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_gameplay_timer(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_gameplay_timer == enabled {
            return;
        }
        cfg.show_select_music_gameplay_timer = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_scorebox(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_scorebox == enabled {
            return;
        }
        cfg.show_select_music_scorebox = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_placement(mode: SelectMusicScoreboxPlacement) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_placement == mode {
            return;
        }
        cfg.select_music_scorebox_placement = mode;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_itg(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_itg == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_itg = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_ex(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_ex == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_ex = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_hard_ex(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_hard_ex == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_hard_ex = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_tournaments(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_tournaments == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_tournaments = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_chart_info_peak_nps(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_chart_info_peak_nps == enabled {
            return;
        }
        cfg.select_music_chart_info_peak_nps = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_chart_info_matrix_rating(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_chart_info_matrix_rating == enabled {
            return;
        }
        cfg.select_music_chart_info_matrix_rating = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_screenshot_eval(mask: u8) {
    {
        let mut cfg = lock_config();
        if cfg.auto_screenshot_eval == mask {
            return;
        }
        cfg.auto_screenshot_eval = mask;
    }
    save_without_keymaps();
}

pub fn update_show_random_courses(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_random_courses == enabled {
            return;
        }
        cfg.show_random_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_most_played_courses(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_most_played_courses == enabled {
            return;
        }
        cfg.show_most_played_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_course_individual_scores(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_course_individual_scores == enabled {
            return;
        }
        cfg.show_course_individual_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_autosubmit_course_scores_individually(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.autosubmit_course_scores_individually == enabled {
            return;
        }
        cfg.autosubmit_course_scores_individually = enabled;
    }
    save_without_keymaps();
}

pub fn update_zmod_rating_box_text(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.zmod_rating_box_text == enabled {
            return;
        }
        cfg.zmod_rating_box_text = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_bpm_decimal(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_bpm_decimal == enabled {
            return;
        }
        cfg.show_bpm_decimal = enabled;
    }
    save_without_keymaps();
}

pub fn update_default_fail_type(fail_type: DefaultFailType) {
    {
        let mut cfg = lock_config();
        if cfg.default_fail_type == fail_type {
            return;
        }
        cfg.default_fail_type = fail_type;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_sync_graph(mode: SyncGraphMode) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_sync_graph == mode {
            return;
        }
        cfg.null_or_die_sync_graph = mode;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_confidence_percent(value: u8) {
    let value = clamp_null_or_die_confidence_percent(value);
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_confidence_percent == value {
            return;
        }
        cfg.null_or_die_confidence_percent = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_fingerprint_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_fingerprint_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_fingerprint_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_window_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_window_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_window_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_step_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_step_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_step_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_magic_offset_ms(value: f64) {
    let value = clamp_null_or_die_magic_offset_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_magic_offset_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_magic_offset_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_kernel_target(value: KernelTarget) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_kernel_target == value {
            return;
        }
        cfg.null_or_die_kernel_target = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_kernel_type(value: BiasKernel) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_kernel_type == value {
            return;
        }
        cfg.null_or_die_kernel_type = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_full_spectrogram(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_full_spectrogram == enabled {
            return;
        }
        cfg.null_or_die_full_spectrogram = enabled;
    }
    save_without_keymaps();
}

pub fn update_input_debounce_seconds(seconds: f32) {
    let seconds = seconds.clamp(0.0, 0.2);
    {
        let mut cfg = lock_config();
        if (cfg.input_debounce_seconds - seconds).abs() <= f32::EPSILON {
            return;
        }
        cfg.input_debounce_seconds = seconds;
    }
    crate::core::input::set_input_debounce_seconds(seconds);
    save_without_keymaps();
}

pub fn update_only_dedicated_menu_buttons(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.only_dedicated_menu_buttons == enabled {
            return;
        }
        cfg.only_dedicated_menu_buttons = enabled;
    }
    crate::core::input::set_only_dedicated_menu_buttons(enabled);
    save_without_keymaps();
}

pub fn update_keyboard_features(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.keyboard_features == enabled {
            return;
        }
        cfg.keyboard_features = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_profile(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_profile == enabled {
            return;
        }
        cfg.machine_show_select_profile = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_video_backgrounds(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_video_backgrounds == enabled {
            return;
        }
        cfg.show_video_backgrounds = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_color(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_color == enabled {
            return;
        }
        cfg.machine_show_select_color = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_style(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_style == enabled {
            return;
        }
        cfg.machine_show_select_style = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_play_mode(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_play_mode == enabled {
            return;
        }
        cfg.machine_show_select_play_mode = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_preferred_style(style: MachinePreferredPlayStyle) {
    {
        let mut cfg = lock_config();
        if cfg.machine_preferred_style == style {
            return;
        }
        cfg.machine_preferred_style = style;
    }
    save_without_keymaps();
}

pub fn update_machine_preferred_play_mode(mode: MachinePreferredPlayMode) {
    {
        let mut cfg = lock_config();
        if cfg.machine_preferred_play_mode == mode {
            return;
        }
        cfg.machine_preferred_play_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_machine_show_eval_summary(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_eval_summary == enabled {
            return;
        }
        cfg.machine_show_eval_summary = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_name_entry(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_name_entry == enabled {
            return;
        }
        cfg.machine_show_name_entry = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_gameover(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_gameover == enabled {
            return;
        }
        cfg.machine_show_gameover = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_enable_replays(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_enable_replays == enabled {
            return;
        }
        cfg.machine_enable_replays = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_groovestats(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_groovestats == enabled {
            return;
        }
        cfg.enable_groovestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_boogiestats(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_boogiestats == enabled {
            return;
        }
        cfg.enable_boogiestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_arrowcloud(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_arrowcloud == enabled {
            return;
        }
        cfg.enable_arrowcloud = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_download_unlocks(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.auto_download_unlocks == enabled {
            return;
        }
        cfg.auto_download_unlocks = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_populate_gs_scores(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.auto_populate_gs_scores == enabled {
            return;
        }
        cfg.auto_populate_gs_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_separate_unlocks_by_player(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.separate_unlocks_by_player == enabled {
            return;
        }
        cfg.separate_unlocks_by_player = enabled;
    }
    save_without_keymaps();
}

pub fn update_game_flag(flag: GameFlag) {
    {
        let mut cfg = lock_config();
        if cfg.game_flag == flag {
            return;
        }
        cfg.game_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_theme_flag(flag: ThemeFlag) {
    {
        let mut cfg = lock_config();
        if cfg.theme_flag == flag {
            return;
        }
        cfg.theme_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_language_flag(flag: LanguageFlag) {
    {
        let mut cfg = lock_config();
        if cfg.language_flag == flag {
            return;
        }
        cfg.language_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_machine_default_noteskin(noteskin: &str) {
    let normalized = normalize_machine_default_noteskin(noteskin);
    {
        let mut current = MACHINE_DEFAULT_NOTESKIN.lock().unwrap();
        if *current == normalized {
            return;
        }
        *current = normalized;
    }
    save_without_keymaps();
}
