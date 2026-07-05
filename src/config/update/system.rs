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

pub fn update_high_dpi(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.high_dpi == enabled {
            return;
        }
        cfg.high_dpi = enabled;
    }
    save_without_keymaps();
}

pub fn update_hide_mouse_cursor(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.hide_mouse_cursor == enabled {
            return;
        }
        cfg.hide_mouse_cursor = enabled;
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
    send_smx_underglow_color();
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
    let mode = clamp_show_stats_mode(mode);
    {
        let mut cfg = lock_config();
        if cfg.show_stats_mode == mode {
            return;
        }
        cfg.show_stats_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_frame_stats_overlay_anchor(key: &'static str) {
    {
        let mut cfg = lock_config();
        if cfg.frame_stats_overlay_anchor == key {
            return;
        }
        cfg.frame_stats_overlay_anchor = key;
    }
    save_without_keymaps();
}

pub fn update_frame_stats_overlay_style(key: &'static str) {
    {
        let mut cfg = lock_config();
        if cfg.frame_stats_overlay_style == key {
            return;
        }
        cfg.frame_stats_overlay_style = key;
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
    let clamped = clamp_bg_brightness(brightness);
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

/// Commit overscan (CenterImage) adjustment to config + disk and sync the live
/// render mirror. The overscan screen also calls `space::set_overscan` directly
/// for live preview; this persists the committed values.
pub fn update_overscan(translate_x: i32, translate_y: i32, add_width: i32, add_height: i32) {
    let mut dirty = false;
    {
        let mut cfg = lock_config();
        if cfg.center_image_translate_x != translate_x {
            cfg.center_image_translate_x = translate_x;
            dirty = true;
        }
        if cfg.center_image_translate_y != translate_y {
            cfg.center_image_translate_y = translate_y;
            dirty = true;
        }
        if cfg.center_image_add_width != add_width {
            cfg.center_image_add_width = add_width;
            dirty = true;
        }
        if cfg.center_image_add_height != add_height {
            cfg.center_image_add_height = add_height;
            dirty = true;
        }
    }
    deadlib_present::space::set_overscan(translate_x, translate_y, add_width, add_height);
    if dirty {
        save_without_keymaps();
    }
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
