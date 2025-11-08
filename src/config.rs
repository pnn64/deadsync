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
    content.push_str(&format!("ShowStats={}\n", if default.show_stats { "1" } else { "0" }));
    content.push_str(&format!("SmoothHistogram={}\n", if default.smooth_histogram { "1" } else { "0" }));
    content.push_str(&format!("VideoRenderer={}\n", default.video_renderer));
    content.push_str(&format!("Vsync={}\n", if default.vsync { "1" } else { "0" }));
    content.push_str(&format!("Windowed={}\n", if default.windowed { "1" } else { "0" }));
    content.push('\n');

    // [Theme] section
    content.push_str("[Theme]\n");
    content.push_str(&format!("SimplyLoveColor={}\n", default.simply_love_color));
    content.push('\n');

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
                cfg.fastload = conf.get("Options", "FastLoad").and_then(|v| v.parse::<u8>().ok()).map_or(default.fastload, |v| v != 0);
                cfg.cachesongs = conf.get("Options", "CacheSongs").and_then(|v| v.parse::<u8>().ok()).map_or(default.cachesongs, |v| v != 0);
                cfg.smooth_histogram = conf.get("Options", "SmoothHistogram").and_then(|v| v.parse::<u8>().ok()).map_or(default.smooth_histogram, |v| v != 0);
                cfg.simply_love_color = conf.get("Theme", "SimplyLoveColor").and_then(|v| v.parse().ok()).unwrap_or(default.simply_love_color);
                
                info!("Configuration loaded from '{}'.", CONFIG_PATH);
            } // Lock on CONFIG is released here.

            // Now, write the fully-populated config back to disk. This preserves existing
            // user settings while adding any new fields with their default values.
            save();
            info!("'{}' updated with default values for any missing fields.", CONFIG_PATH);
        }
        Err(e) => {
            warn!("Failed to load '{}': {}. Using default values.", CONFIG_PATH, e);
        }
    }
}

fn save() {
    let cfg = CONFIG.lock().unwrap();
    
    let mut content = String::new();

    // [Options] section - keys in alphabetical order
    content.push_str("[Options]\n");
    content.push_str(&format!("CacheSongs={}\n", if cfg.cachesongs { "1" } else { "0" }));
    content.push_str(&format!("DisplayHeight={}\n", cfg.display_height));
    content.push_str(&format!("DisplayWidth={}\n", cfg.display_width));
    content.push_str(&format!("FastLoad={}\n", if cfg.fastload { "1" } else { "0" }));
    content.push_str(&format!("GlobalOffsetSeconds={}\n", cfg.global_offset_seconds));
    content.push_str(&format!("ShowStats={}\n", if cfg.show_stats { "1" } else { "0" }));
    content.push_str(&format!("SmoothHistogram={}\n", if cfg.smooth_histogram { "1" } else { "0" }));
    content.push_str(&format!("VideoRenderer={}\n", cfg.video_renderer));
    content.push_str(&format!("Vsync={}\n", if cfg.vsync { "1" } else { "0" }));
    content.push_str(&format!("Windowed={}\n", if cfg.windowed { "1" } else { "0" }));
    content.push('\n');

    // [Theme] section
    content.push_str("[Theme]\n");
    content.push_str(&format!("SimplyLoveColor={}\n", cfg.simply_love_color));
    content.push('\n');

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
    save();
}

#[allow(dead_code)]
pub fn update_global_offset(offset: f32) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if (cfg.global_offset_seconds - offset).abs() < f32::EPSILON { return; }
        cfg.global_offset_seconds = offset;
    }
    save();
}
