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
    let dirty = {
        let mut cfg = lock_config();
        let Config {
            display_width,
            display_height,
            ..
        } = &mut *cfg;
        set_pair_if_changed(display_width, width, display_height, height)
    };
    if dirty {
        save_without_keymaps();
    }
}

pub fn update_display_monitor(monitor: usize) {
    update_config_value(monitor, |cfg| &mut cfg.display_monitor);
}

pub fn update_video_renderer(renderer: BackendType) {
    update_config_value(renderer, |cfg| &mut cfg.video_renderer);
}

pub fn update_gfx_debug(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.gfx_debug);
}

pub fn update_high_dpi(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.high_dpi);
}

pub fn update_hide_mouse_cursor(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.hide_mouse_cursor);
}

pub fn update_simply_love_color(index: i32) {
    if update_config_value(index, |cfg| &mut cfg.simply_love_color) {
        send_smx_underglow_color();
    }
}

/// Push the current theme colour to the SMX pad edge LED strips. P1 pad gets
/// `simply_love_color`; P2 pad gets `simply_love_color - 2` (the existing P2
/// differentiation offset). A lone pad (only one slot connected) always gets
/// the main theme colour regardless of which side it sits on. No-op when SMX
/// input is disabled.
pub fn send_smx_underglow_color() {
    let cfg = get();
    if !cfg.smx_input || !cfg.smx_underglow_theme {
        return;
    }
    let index = cfg.simply_love_color;
    // Keep the wire order in sync before any send: this path also runs from
    // pad-connect events, which can fire before the user ever opens options.
    deadsync_smx::set_platform_lights_grb(cfg.smx_underglow_grb);
    let to_u8 = |c: f32| (c * 255.0).round() as u8;
    let rgba_to_rgb =
        |rgba: [f32; 4]| -> [u8; 3] { [to_u8(rgba[0]), to_u8(rgba[1]), to_u8(rgba[2])] };
    let p1_rgb = rgba_to_rgb(deadlib_present::color::decorative_rgba(index));
    let lone_pad = deadsync_smx::get_info(0).connected ^ deadsync_smx::get_info(1).connected;
    let p2_rgb = if lone_pad {
        p1_rgb
    } else {
        rgba_to_rgb(deadlib_present::color::decorative_rgba(index - 2))
    };
    deadsync_smx::set_platform_lights_solid([Some(p1_rgb), Some(p2_rgb)]);
}

pub fn update_global_offset(offset: f32) {
    update_config_f32(offset, |cfg| &mut cfg.global_offset_seconds);
}

pub fn update_visual_delay_seconds(delay: f32) {
    let clamped = delay.clamp(-1.0, 1.0);
    update_config_f32(clamped, |cfg| &mut cfg.visual_delay_seconds);
}

pub fn update_vsync(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.vsync);
}

pub fn update_max_fps(max_fps: u16) {
    update_config_value(max_fps, |cfg| &mut cfg.max_fps);
}

pub fn update_present_mode_policy(mode: PresentModePolicy) {
    update_config_value(mode, |cfg| &mut cfg.present_mode_policy);
}

pub fn update_show_stats_mode(mode: u8) {
    let mode = clamp_show_stats_mode(mode);
    update_config_value(mode, |cfg| &mut cfg.show_stats_mode);
}

pub fn update_frame_stats_overlay_anchor(key: &'static str) {
    update_config_value(key, |cfg| &mut cfg.frame_stats_overlay_anchor);
}

pub fn update_frame_stats_overlay_style(key: &'static str) {
    update_config_value(key, |cfg| &mut cfg.frame_stats_overlay_style);
}

pub fn update_log_level(level: LogLevel) {
    log::set_max_level(level.as_level_filter());
    update_config_value(level, |cfg| &mut cfg.log_level);
}

pub fn update_log_to_file(enabled: bool) {
    logging::set_file_logging_enabled(enabled);
    update_config_value(enabled, |cfg| &mut cfg.log_to_file);
}

#[cfg(target_os = "windows")]
pub fn update_windows_gamepad_backend(backend: WindowsPadBackend) {
    update_config_value(backend, |cfg| &mut cfg.windows_gamepad_backend);
}

pub fn update_bg_brightness(brightness: f32) {
    let clamped = clamp_bg_brightness(brightness);
    update_config_f32(clamped, |cfg| &mut cfg.bg_brightness);
}

pub fn update_center_1player_notefield(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.center_1player_notefield);
}

/// Commit overscan (CenterImage) adjustment to config + disk and sync the live
/// render mirror. The overscan screen also calls `space::set_overscan` directly
/// for live preview; this persists the committed values.
pub fn update_overscan(translate_x: i32, translate_y: i32, add_width: i32, add_height: i32) {
    let dirty = {
        let mut cfg = lock_config();
        let Config {
            center_image_translate_x,
            center_image_translate_y,
            center_image_add_width,
            center_image_add_height,
            ..
        } = &mut *cfg;
        set_quad_if_changed(
            center_image_translate_x,
            translate_x,
            center_image_translate_y,
            translate_y,
            center_image_add_width,
            add_width,
            center_image_add_height,
            add_height,
        )
    };
    deadlib_present::space::set_overscan(translate_x, translate_y, add_width, add_height);
    if dirty {
        save_without_keymaps();
    }
}

pub fn update_banner_cache(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.banner_cache);
}

pub fn update_cdtitle_cache(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.cdtitle_cache);
}

pub fn update_song_parsing_threads(threads: u8) {
    update_config_value(threads, |cfg| &mut cfg.song_parsing_threads);
}

pub fn update_cache_songs(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.cachesongs);
}

pub fn update_fastload(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.fastload);
}
