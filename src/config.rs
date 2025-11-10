use crate::core::gfx::BackendType;
use configparser::ini::Ini;
use log::{info, warn};
use once_cell::sync::Lazy;
use std::str::FromStr;
use std::sync::Mutex;

const CONFIG_PATH: &str = "deadsync.ini";

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub vsync: bool,
    pub windowed: bool,
    pub show_stats: bool,
    pub display_width: u32,
    pub display_height: u32,
    pub video_renderer: BackendType,
    pub simply_love_color: i32,
    pub global_offset_seconds: f32,
    pub master_volume: u8,
    pub music_volume: u8,
    pub sfx_volume: u8,
    pub fastload: bool,
    pub cachesongs: bool,
    // Whether to apply Gaussian smoothing to the eval histogram (Simply Love style)
    pub smooth_histogram: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vsync: false,
            windowed: true,
            show_stats: false,
            display_width: 1600,
            display_height: 900,
            video_renderer: BackendType::OpenGL,
            simply_love_color: 2, // Corresponds to DEFAULT_COLOR_INDEX
            global_offset_seconds: -0.008,
            master_volume: 90,
            music_volume: 100,
            sfx_volume: 100,
            fastload: true,
            cachesongs: true,
            smooth_histogram: true,
        }
    }
}

// Global, mutable configuration instance.
static CONFIG: Lazy<Mutex<Config>> = Lazy::new(|| Mutex::new(Config::default()));


// --- File I/O ---

fn create_default_config_file() -> Result<(), std::io::Error> {
    info!("'{}' not found, creating with default values.", CONFIG_PATH);
    let default = Config::default();

    let mut content = String::new();

    // [Options] section - keys in alphabetical order
    content.push_str("[Options]\n");
    content.push_str(&format!("CacheSongs={}\n", if default.cachesongs { "1" } else { "0" }));
    content.push_str(&format!("DisplayHeight={}\n", default.display_height));
    content.push_str(&format!("DisplayWidth={}\n", default.display_width));
    content.push_str(&format!("FastLoad={}\n", if default.fastload { "1" } else { "0" }));
    content.push_str(&format!("GlobalOffsetSeconds={}\n", default.global_offset_seconds));
    content.push_str(&format!("MasterVolume={}\n", default.master_volume));
    content.push_str(&format!("MusicVolume={}\n", default.music_volume));
    content.push_str(&format!("ShowStats={}\n", if default.show_stats { "1" } else { "0" }));
    content.push_str(&format!("SmoothHistogram={}\n", if default.smooth_histogram { "1" } else { "0" }));
    content.push_str(&format!("SFXVolume={}\n", default.sfx_volume));
    content.push_str(&format!("VideoRenderer={}\n", default.video_renderer));
    content.push_str(&format!("Vsync={}\n", if default.vsync { "1" } else { "0" }));
    content.push_str(&format!("Windowed={}\n", if default.windowed { "1" } else { "0" }));
    content.push('\n');

    // [Theme] section
    content.push_str("[Theme]\n");
    content.push_str(&format!("SimplyLoveColor={}\n", default.simply_love_color));
    content.push('\n');

    // [keymaps] section with sane defaults
    let km = crate::core::input::get_keymap();
    content.push_str(&crate::core::input::keymap_to_ini_section_string(&km));

    std::fs::write(CONFIG_PATH, content)
}

pub fn load() {
    // --- Load main deadsync.ini ---
    if !std::path::Path::new(CONFIG_PATH).exists() {
        if let Err(e) = create_default_config_file() {
            warn!("Failed to create default config file: {}", e);
        }
    }

    let mut conf = Ini::new();
    match conf.load(CONFIG_PATH) {
        Ok(_) => {
            // This block populates the global CONFIG struct from the file,
            // using default values for any missing keys.
            {
                let mut cfg = CONFIG.lock().unwrap();
                let default = Config::default();
                
                cfg.vsync = conf.get("Options", "Vsync").and_then(|v| v.parse::<u8>().ok()).map_or(default.vsync, |v| v != 0);
                cfg.windowed = conf.get("Options", "Windowed").and_then(|v| v.parse::<u8>().ok()).map_or(default.windowed, |v| v != 0);
                cfg.show_stats = conf.get("Options", "ShowStats").and_then(|v| v.parse::<u8>().ok()).map_or(default.show_stats, |v| v != 0);
                cfg.display_width = conf.get("Options", "DisplayWidth").and_then(|v| v.parse().ok()).unwrap_or(default.display_width);
                cfg.display_height = conf.get("Options", "DisplayHeight").and_then(|v| v.parse().ok()).unwrap_or(default.display_height);
                cfg.video_renderer = conf.get("Options", "VideoRenderer")
                    .and_then(|s| BackendType::from_str(&s).ok())
                    .unwrap_or(default.video_renderer);
                cfg.global_offset_seconds = conf.get("Options", "GlobalOffsetSeconds").and_then(|v| v.parse().ok()).unwrap_or(default.global_offset_seconds);
                cfg.master_volume = conf.get("Options", "MasterVolume").and_then(|v| v.parse().ok()).map(|v: u8| v.clamp(0, 100)).unwrap_or(default.master_volume);
                cfg.music_volume = conf.get("Options", "MusicVolume").and_then(|v| v.parse().ok()).map(|v: u8| v.clamp(0, 100)).unwrap_or(default.music_volume);
                cfg.sfx_volume = conf.get("Options", "SFXVolume").and_then(|v| v.parse().ok()).map(|v: u8| v.clamp(0, 100)).unwrap_or(default.sfx_volume);
                cfg.fastload = conf.get("Options", "FastLoad").and_then(|v| v.parse::<u8>().ok()).map_or(default.fastload, |v| v != 0);
                cfg.cachesongs = conf.get("Options", "CacheSongs").and_then(|v| v.parse::<u8>().ok()).map_or(default.cachesongs, |v| v != 0);
                cfg.smooth_histogram = conf.get("Options", "SmoothHistogram").and_then(|v| v.parse::<u8>().ok()).map_or(default.smooth_histogram, |v| v != 0);
                cfg.simply_love_color = conf.get("Theme", "SimplyLoveColor").and_then(|v| v.parse().ok()).unwrap_or(default.simply_love_color);
                
                info!("Configuration loaded from '{}'.", CONFIG_PATH);
            } // Lock on CONFIG is released here.

            // Load keymaps from the same INI and publish globally
            let km = crate::core::input::load_keymap_from_ini(&conf);
            crate::core::input::set_keymap(km);

            // Ensure [Keymaps] exist with primary bindings if missing; do not overwrite existing keys.
            let mut need_write_keymaps = false;
            let mut conf2 = conf.clone();
            let has_keymaps_new = conf.get_map_ref().get("Keymaps").is_some();
            let has_keymaps_old = conf.get_map_ref().get("keymaps").is_some();
            let section_present = has_keymaps_new || has_keymaps_old;
            let sec_name = if has_keymaps_new { "Keymaps" } else if has_keymaps_old { "keymaps" } else { "Keymaps" };
            let ensure = |ini: &mut Ini, key: &str, val: &str| -> bool {
                let cur = ini.get(sec_name, key);
                if cur.is_none() {
                    ini.set(sec_name, key, Some(val.to_string()));
                    true
                } else { false }
            };
            if !section_present {
                // Seed the whole section (no default pad bindings)
                conf2.set("Keymaps", "P1_Back", Some("KeyCode::Escape".to_string()));
                conf2.set("Keymaps", "P1_Start", Some("KeyCode::Enter".to_string()));
                conf2.set("Keymaps", "P1_Up", Some("KeyCode::ArrowUp;KeyCode::KeyW".to_string()));
                conf2.set("Keymaps", "P1_Down", Some("KeyCode::ArrowDown;KeyCode::KeyS".to_string()));
                conf2.set("Keymaps", "P1_Left", Some("KeyCode::ArrowLeft;KeyCode::KeyA".to_string()));
                conf2.set("Keymaps", "P1_Right", Some("KeyCode::ArrowRight;KeyCode::KeyD".to_string()));
                conf2.set("Keymaps", "P1_Select", Some("".to_string()));
                conf2.set("Keymaps", "P1_Operator", Some("".to_string()));
                conf2.set("Keymaps", "P1_Restart", Some("".to_string()));
                need_write_keymaps = true;
            } else {
                // Add only missing keys
                need_write_keymaps |= ensure(&mut conf2, "P1_Back", "KeyCode::Escape");
                need_write_keymaps |= ensure(&mut conf2, "P1_Start", "KeyCode::Enter");
                need_write_keymaps |= ensure(&mut conf2, "P1_Up", "KeyCode::ArrowUp;KeyCode::KeyW");
                need_write_keymaps |= ensure(&mut conf2, "P1_Down", "KeyCode::ArrowDown;KeyCode::KeyS");
                need_write_keymaps |= ensure(&mut conf2, "P1_Left", "KeyCode::ArrowLeft;KeyCode::KeyA");
                need_write_keymaps |= ensure(&mut conf2, "P1_Right", "KeyCode::ArrowRight;KeyCode::KeyD");
                need_write_keymaps |= ensure(&mut conf2, "P1_Select", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_Operator", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_Restart", "");
            }
            if need_write_keymaps {
                if let Err(e) = conf2.write(CONFIG_PATH) { warn!("Failed to append missing keymaps: {}", e); }
            }

            // Only write [Options]/[Theme] if any of those keys are missing.
            let missing_opts = {
                let has = |sec: &str, key: &str| conf.get(sec, key).is_some();
                let mut miss = false;
                let options_keys = [
                    "CacheSongs","DisplayHeight","DisplayWidth","FastLoad","GlobalOffsetSeconds",
                    "MasterVolume","MusicVolume","ShowStats","SmoothHistogram","SFXVolume",
                    "VideoRenderer","Vsync","Windowed"
                ];
                for k in options_keys { if !has("Options", k) { miss = true; break; } }
                if !miss { if !has("Theme","SimplyLoveColor") { miss = true; } }
                miss
            };
            if missing_opts {
                save_without_keymaps();
                info!("'{}' updated with default values for any missing fields (keymaps preserved).", CONFIG_PATH);
            } else {
                info!("Configuration OK; no write needed.");
            }
        }
        Err(e) => {
            warn!("Failed to load '{}': {}. Using default values.", CONFIG_PATH, e);
        }
    }
}

fn save_without_keymaps() {
    // Manual writer that keeps [Options]/[Theme] sorted and preserves existing [keymaps] block.
    let cfg = CONFIG.lock().unwrap();

    // Try to extract existing [Keymaps] (or legacy [keymaps]) block verbatim
    let existing = std::fs::read_to_string(CONFIG_PATH).unwrap_or_default();
    let mut keymaps_block = String::new();
    if let Some(start) = existing.find("[Keymaps]").or_else(|| existing.find("[keymaps]")) {
        // find next section header or EOF
        let rest = &existing[start..];
        let mut end_idx = rest.len();
        for (i, line) in rest.lines().enumerate().skip(1) {
            if line.starts_with('[') && line.ends_with(']') {
                // end before this line
                end_idx = rest[..].lines().take(i).map(|l| l.len() + 1).sum();
                break;
            }
        }
        keymaps_block = rest[..end_idx].to_string();
        if !keymaps_block.ends_with('\n') { keymaps_block.push('\n'); }
    }

    let mut content = String::new();

    // [Options] (alphabetical order)
    content.push_str("[Options]\n");
    content.push_str(&format!("CacheSongs={}\n", if cfg.cachesongs { "1" } else { "0" }));
    content.push_str(&format!("DisplayHeight={}\n", cfg.display_height));
    content.push_str(&format!("DisplayWidth={}\n", cfg.display_width));
    content.push_str(&format!("FastLoad={}\n", if cfg.fastload { "1" } else { "0" }));
    content.push_str(&format!("GlobalOffsetSeconds={}\n", cfg.global_offset_seconds));
    content.push_str(&format!("MasterVolume={}\n", cfg.master_volume));
    content.push_str(&format!("MusicVolume={}\n", cfg.music_volume));
    content.push_str(&format!("ShowStats={}\n", if cfg.show_stats { "1" } else { "0" }));
    content.push_str(&format!("SmoothHistogram={}\n", if cfg.smooth_histogram { "1" } else { "0" }));
    content.push_str(&format!("SFXVolume={}\n", cfg.sfx_volume));
    content.push_str(&format!("VideoRenderer={}\n", cfg.video_renderer));
    content.push_str(&format!("Vsync={}\n", if cfg.vsync { "1" } else { "0" }));
    content.push_str(&format!("Windowed={}\n", if cfg.windowed { "1" } else { "0" }));
    content.push('\n');

    // [Theme]
    content.push_str("[Theme]\n");
    content.push_str(&format!("SimplyLoveColor={}\n", cfg.simply_love_color));
    content.push('\n');

    // Append preserved [keymaps] if present
    if !keymaps_block.is_empty() {
        content.push_str(&keymaps_block);
        if !content.ends_with('\n') { content.push('\n'); }
    }

    if let Err(e) = std::fs::write(CONFIG_PATH, content) {
        warn!("Failed to save config file: {}", e);
    }
}

pub fn get() -> Config {
    *CONFIG.lock().unwrap()
}

pub fn update_simply_love_color(index: i32) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        // No change, no need to write to disk.
        if cfg.simply_love_color == index { return; }
        cfg.simply_love_color = index;
    }
    save_without_keymaps();
}

#[allow(dead_code)]
pub fn update_global_offset(offset: f32) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if (cfg.global_offset_seconds - offset).abs() < f32::EPSILON { return; }
        cfg.global_offset_seconds = offset;
    }
    save_without_keymaps();
}
