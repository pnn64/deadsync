use crate::core::gfx::BackendType;
use crate::core::input::{GamepadCodeBinding, InputBinding, Keymap, PadDir, VirtualAction};
use log::{info, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;
use winit::keyboard::KeyCode;

const CONFIG_PATH: &str = "deadsync.ini";

// --- Minimal INI reader ---
#[derive(Debug, Default)]
pub struct SimpleIni {
    sections: HashMap<String, HashMap<String, String>>,
}

impl SimpleIni {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        self.sections.clear();

        let mut current_section: Option<String> = None;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            // Section header: [SectionName]
            if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
                let name = &line[1..line.len() - 1];
                let section = name.trim().to_string();
                current_section = Some(section.clone());
                self.sections.entry(section).or_default();
                continue;
            }

            // Key/value pair: key=value
            if let Some(eq_idx) = line.find('=') {
                let (key_raw, value_raw) = line.split_at(eq_idx);
                let key = key_raw.trim();
                if key.is_empty() {
                    continue;
                }
                // Skip '=' and trim whitespace from the value.
                let value = value_raw[1..].trim().to_string();
                let section = current_section.clone().unwrap_or_default();
                self.sections
                    .entry(section)
                    .or_default()
                    .insert(key.to_string(), value);
            }
        }

        Ok(())
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections.get(section).and_then(|s| s.get(key)).cloned()
    }

    pub fn get_section(&self, section: &str) -> Option<&HashMap<String, String>> {
        self.sections.get(section)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenType {
    Exclusive,
    Borderless,
}

impl FullscreenType {
    fn as_str(&self) -> &'static str {
        match self {
            FullscreenType::Exclusive => "Exclusive",
            FullscreenType::Borderless => "Borderless",
        }
    }
}

impl FromStr for FullscreenType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "exclusive" => Ok(FullscreenType::Exclusive),
            "borderless" => Ok(FullscreenType::Borderless),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Windowed,
    Fullscreen(FullscreenType),
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub vsync: bool,
    pub windowed: bool,
    pub fullscreen_type: FullscreenType,
    pub display_monitor: usize,
    pub show_stats: bool,
    pub translated_titles: bool,
    pub mine_hit_sound: bool,
    pub display_width: u32,
    pub display_height: u32,
    pub video_renderer: BackendType,
    // When using the Software video renderer:
    // 0 = Auto (use all logical cores)
    // 1 = Single-threaded
    // N >= 2 = cap at N threads (clamped to available cores).
    pub software_renderer_threads: u8,
    // When parsing simfiles at startup:
    // 0 = Auto (use all logical cores) for cache misses
    // 1 = Single-threaded
    // N >= 2 = cap at N threads (clamped to available cores).
    pub song_parsing_threads: u8,
    pub simply_love_color: i32,
    pub global_offset_seconds: f32,
    pub master_volume: u8,
    pub menu_music: bool,
    pub music_volume: u8,
    pub sfx_volume: u8,
    // None = auto (use device default sample rate)
    pub audio_sample_rate_hz: Option<u32>,
    pub rate_mod_preserves_pitch: bool,
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
            fullscreen_type: FullscreenType::Exclusive,
            display_monitor: 0,
            show_stats: false,
            translated_titles: false,
            mine_hit_sound: true,
            display_width: 1600,
            display_height: 900,
            video_renderer: BackendType::OpenGL,
            software_renderer_threads: 1,
            song_parsing_threads: 0,
            simply_love_color: 2, // Corresponds to DEFAULT_COLOR_INDEX
            global_offset_seconds: -0.008,
            master_volume: 90,
            menu_music: true,
            music_volume: 100,
            sfx_volume: 100,
            audio_sample_rate_hz: None,
            rate_mod_preserves_pitch: false,
            fastload: true,
            cachesongs: true,
            smooth_histogram: true,
        }
    }
}

impl Config {
    pub fn display_mode(&self) -> DisplayMode {
        if self.windowed {
            DisplayMode::Windowed
        } else {
            DisplayMode::Fullscreen(self.fullscreen_type)
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
    content.push_str("AudioSampleRateHz=Auto\n");
    content.push_str(&format!(
        "CacheSongs={}\n",
        if default.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!("DisplayHeight={}\n", default.display_height));
    content.push_str(&format!("DisplayWidth={}\n", default.display_width));
    content.push_str(&format!("DisplayMonitor={}\n", default.display_monitor));
    content.push_str(&format!(
        "FastLoad={}\n",
        if default.fastload { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FullscreenType={}\n",
        default.fullscreen_type.as_str()
    ));
    content.push_str(&format!(
        "GlobalOffsetSeconds={}\n",
        default.global_offset_seconds
    ));
    content.push_str(&format!("MasterVolume={}\n", default.master_volume));
    content.push_str(&format!(
        "MenuMusic={}\n",
        if default.menu_music { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MineHitSound={}\n",
        if default.mine_hit_sound { "1" } else { "0" }
    ));
    content.push_str(&format!("MusicVolume={}\n", default.music_volume));
    content.push_str(&format!(
        "RateModPreservesPitch={}\n",
        if default.rate_mod_preserves_pitch {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowStats={}\n",
        if default.show_stats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if default.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "SongParsingThreads={}\n",
        default.song_parsing_threads
    ));
    content.push_str(&format!(
        "SoftwareRendererThreads={}\n",
        default.software_renderer_threads
    ));
    content.push_str(&format!("SFXVolume={}\n", default.sfx_volume));
    content.push_str(&format!(
        "TranslatedTitles={}\n",
        if default.translated_titles { "1" } else { "0" }
    ));
    content.push_str(&format!("VideoRenderer={}\n", default.video_renderer));
    content.push_str(&format!(
        "Vsync={}\n",
        if default.vsync { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "Windowed={}\n",
        if default.windowed { "1" } else { "0" }
    ));
    content.push('\n');

    // [Keymaps] section with sane defaults (comma-separated)
    content.push_str("[Keymaps]\n");
    content.push_str("P1_Back=KeyCode::Escape\n");
    content.push_str("P1_Down=KeyCode::ArrowDown,KeyCode::KeyS\n");
    content.push_str("P1_Left=KeyCode::ArrowLeft,KeyCode::KeyA\n");
    content.push_str("P1_MenuDown=\n");
    content.push_str("P1_MenuLeft=\n");
    content.push_str("P1_MenuRight=\n");
    content.push_str("P1_MenuUp=\n");
    content.push_str("P1_Operator=\n");
    content.push_str("P1_Restart=\n");
    content.push_str("P1_Right=KeyCode::ArrowRight,KeyCode::KeyD\n");
    content.push_str("P1_Select=\n");
    content.push_str("P1_Start=KeyCode::Enter\n");
    content.push_str("P1_Up=KeyCode::ArrowUp,KeyCode::KeyW\n");
    // Player 2 keyboard defaults: numpad directions + Start on NumpadEnter + Back on Numpad0.
    content.push_str("P2_Back=KeyCode::Numpad0\n");
    content.push_str("P2_Down=KeyCode::Numpad2\n");
    content.push_str("P2_Left=KeyCode::Numpad4\n");
    content.push_str("P2_MenuDown=\n");
    content.push_str("P2_MenuLeft=\n");
    content.push_str("P2_MenuRight=\n");
    content.push_str("P2_MenuUp=\n");
    content.push_str("P2_Operator=\n");
    content.push_str("P2_Restart=\n");
    content.push_str("P2_Right=KeyCode::Numpad6\n");
    content.push_str("P2_Select=\n");
    content.push_str("P2_Start=KeyCode::NumpadEnter\n");
    content.push_str("P2_Up=KeyCode::Numpad8\n");
    content.push('\n');

    // [Theme] section should be last
    content.push_str("[Theme]\n");
    content.push_str(&format!("SimplyLoveColor={}\n", default.simply_love_color));
    content.push('\n');

    std::fs::write(CONFIG_PATH, content)
}

pub fn load() {
    // --- Load main deadsync.ini ---
    if !std::path::Path::new(CONFIG_PATH).exists()
        && let Err(e) = create_default_config_file()
    {
        warn!("Failed to create default config file: {}", e);
    }

    let mut conf = SimpleIni::new();
    match conf.load(CONFIG_PATH) {
        Ok(_) => {
            // This block populates the global CONFIG struct from the file,
            // using default values for any missing keys.
            {
                let mut cfg = CONFIG.lock().unwrap();
                let default = Config::default();

                cfg.vsync = conf
                    .get("Options", "Vsync")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.vsync, |v| v != 0);
                cfg.windowed = conf
                    .get("Options", "Windowed")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.windowed, |v| v != 0);
                cfg.fullscreen_type = conf
                    .get("Options", "FullscreenType")
                    .and_then(|v| FullscreenType::from_str(&v).ok())
                    .unwrap_or(default.fullscreen_type);
                cfg.display_monitor = conf
                    .get("Options", "DisplayMonitor")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(default.display_monitor);
                cfg.mine_hit_sound = conf
                    .get("Options", "MineHitSound")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.mine_hit_sound, |v| v != 0);
                cfg.show_stats = conf
                    .get("Options", "ShowStats")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_stats, |v| v != 0);
                cfg.translated_titles = conf
                    .get("Options", "TranslatedTitles")
                    .or_else(|| conf.get("Options", "translatedtitles"))
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.translated_titles);
                cfg.display_width = conf
                    .get("Options", "DisplayWidth")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.display_width);
                cfg.display_height = conf
                    .get("Options", "DisplayHeight")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.display_height);
                cfg.video_renderer = conf
                    .get("Options", "VideoRenderer")
                    .and_then(|s| BackendType::from_str(&s).ok())
                    .unwrap_or(default.video_renderer);
                cfg.global_offset_seconds = conf
                    .get("Options", "GlobalOffsetSeconds")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.global_offset_seconds);
                cfg.master_volume = conf
                    .get("Options", "MasterVolume")
                    .and_then(|v| v.parse().ok())
                    .map(|v: u8| v.clamp(0, 100))
                    .unwrap_or(default.master_volume);
                cfg.menu_music = conf
                    .get("Options", "MenuMusic")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.menu_music, |v| v != 0);
                cfg.music_volume = conf
                    .get("Options", "MusicVolume")
                    .and_then(|v| v.parse().ok())
                    .map(|v: u8| v.clamp(0, 100))
                    .unwrap_or(default.music_volume);
                cfg.sfx_volume = conf
                    .get("Options", "SFXVolume")
                    .and_then(|v| v.parse().ok())
                    .map(|v: u8| v.clamp(0, 100))
                    .unwrap_or(default.sfx_volume);
                cfg.audio_sample_rate_hz = conf
                    .get("Options", "AudioSampleRateHz")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                            None
                        } else {
                            v.parse::<u32>().ok()
                        }
                    })
                    .or(default.audio_sample_rate_hz);
                cfg.rate_mod_preserves_pitch = conf
                    .get("Options", "RateModPreservesPitch")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.rate_mod_preserves_pitch, |v| v != 0);
                cfg.fastload = conf
                    .get("Options", "FastLoad")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.fastload, |v| v != 0);
                cfg.cachesongs = conf
                    .get("Options", "CacheSongs")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.cachesongs, |v| v != 0);
                cfg.song_parsing_threads = conf
                    .get("Options", "SongParsingThreads")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                            Some(0u8)
                        } else {
                            v.parse::<u8>().ok()
                        }
                    })
                    .unwrap_or(default.song_parsing_threads);
                cfg.smooth_histogram = conf
                    .get("Options", "SmoothHistogram")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.smooth_histogram, |v| v != 0);
                cfg.software_renderer_threads = conf
                    .get("Options", "SoftwareRendererThreads")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                            Some(0u8)
                        } else {
                            v.parse::<u8>().ok()
                        }
                    })
                    .unwrap_or(default.software_renderer_threads);
                cfg.simply_love_color = conf
                    .get("Theme", "SimplyLoveColor")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.simply_love_color);

                info!("Configuration loaded from '{}'.", CONFIG_PATH);
            } // Lock on CONFIG is released here.

            // Load keymaps from the same INI and publish globally.
            let km = load_keymap_from_ini_local(&conf);
            crate::core::input::set_keymap(km);

            // Only write [Options]/[Theme] if any of those keys are missing.
                let missing_opts = {
                    let has = |sec: &str, key: &str| conf.get(sec, key).is_some();
                    let mut miss = false;
                    let options_keys = [
                        "AudioSampleRateHz",
                        "CacheSongs",
                        "DisplayHeight",
                        "DisplayWidth",
                        "FastLoad",
                        "FullscreenType",
                        "GlobalOffsetSeconds",
                        "MasterVolume",
                        "MenuMusic",
                        "MineHitSound",
                        "MusicVolume",
                        "SongParsingThreads",
                        "RateModPreservesPitch",
                        "ShowStats",
                        "SmoothHistogram",
                        "SFXVolume",
                        "SoftwareRendererThreads",
                        "TranslatedTitles",
                        "VideoRenderer",
                        "Vsync",
                        "Windowed",
                    ];
                for k in options_keys {
                    if !has("Options", k) {
                        miss = true;
                        break;
                    }
                }
                if !miss && !has("Theme", "SimplyLoveColor") {
                    miss = true;
                }
                miss
            };
            if missing_opts {
                save_without_keymaps();
                info!(
                    "'{}' updated with default values for any missing fields.",
                    CONFIG_PATH
                );
            } else {
                info!("Configuration OK; no write needed.");
            }
        }
        Err(e) => {
            warn!(
                "Failed to load '{}': {}. Using default values.",
                CONFIG_PATH, e
            );
        }
    }
}

// --- Keymap defaults and parsing (kept in config to avoid coupling input.rs to config) ---

// Stable iteration order for all virtual actions when serializing [Keymaps].
const ALL_VIRTUAL_ACTIONS: [VirtualAction; 26] = [
    VirtualAction::p1_back,
    VirtualAction::p1_down,
    VirtualAction::p1_left,
    VirtualAction::p1_menu_down,
    VirtualAction::p1_menu_left,
    VirtualAction::p1_menu_right,
    VirtualAction::p1_menu_up,
    VirtualAction::p1_operator,
    VirtualAction::p1_restart,
    VirtualAction::p1_right,
    VirtualAction::p1_select,
    VirtualAction::p1_start,
    VirtualAction::p1_up,
    VirtualAction::p2_back,
    VirtualAction::p2_down,
    VirtualAction::p2_left,
    VirtualAction::p2_menu_down,
    VirtualAction::p2_menu_left,
    VirtualAction::p2_menu_right,
    VirtualAction::p2_menu_up,
    VirtualAction::p2_operator,
    VirtualAction::p2_restart,
    VirtualAction::p2_right,
    VirtualAction::p2_select,
    VirtualAction::p2_start,
    VirtualAction::p2_up,
];

fn default_keymap_local() -> Keymap {
    use VirtualAction as A;
    let mut km = Keymap::default();
    // Player 1 defaults (WASD + arrows, Enter/Escape).
    km.bind(
        A::p1_up,
        &[
            InputBinding::Key(KeyCode::ArrowUp),
            InputBinding::Key(KeyCode::KeyW),
        ],
    );
    km.bind(
        A::p1_down,
        &[
            InputBinding::Key(KeyCode::ArrowDown),
            InputBinding::Key(KeyCode::KeyS),
        ],
    );
    km.bind(
        A::p1_left,
        &[
            InputBinding::Key(KeyCode::ArrowLeft),
            InputBinding::Key(KeyCode::KeyA),
        ],
    );
    km.bind(
        A::p1_right,
        &[
            InputBinding::Key(KeyCode::ArrowRight),
            InputBinding::Key(KeyCode::KeyD),
        ],
    );
    km.bind(A::p1_start, &[InputBinding::Key(KeyCode::Enter)]);
    km.bind(A::p1_back, &[InputBinding::Key(KeyCode::Escape)]);
    // Player 2 defaults (numpad directions + Start on NumpadEnter).
    km.bind(A::p2_up, &[InputBinding::Key(KeyCode::Numpad8)]);
    km.bind(A::p2_down, &[InputBinding::Key(KeyCode::Numpad2)]);
    km.bind(A::p2_left, &[InputBinding::Key(KeyCode::Numpad4)]);
    km.bind(A::p2_right, &[InputBinding::Key(KeyCode::Numpad6)]);
    km.bind(A::p2_start, &[InputBinding::Key(KeyCode::NumpadEnter)]);
    km.bind(A::p2_back, &[InputBinding::Key(KeyCode::Numpad0)]);
    // Leave P2_Menu/Select/Operator/Restart unbound by default for now.
    km
}

#[inline(always)]
fn parse_action_key_lower(k: &str) -> Option<VirtualAction> {
    use VirtualAction::*;
    match k {
        "p1_up" => Some(p1_up),
        "p1_down" => Some(p1_down),
        "p1_left" => Some(p1_left),
        "p1_right" => Some(p1_right),
        "p1_start" => Some(p1_start),
        "p1_back" => Some(p1_back),
        "p1_menuup" => Some(p1_menu_up),
        "p1_menudown" => Some(p1_menu_down),
        "p1_menuleft" => Some(p1_menu_left),
        "p1_menuright" => Some(p1_menu_right),
        "p1_select" => Some(p1_select),
        "p1_operator" => Some(p1_operator),
        "p1_restart" => Some(p1_restart),
        "p2_up" => Some(p2_up),
        "p2_down" => Some(p2_down),
        "p2_left" => Some(p2_left),
        "p2_right" => Some(p2_right),
        "p2_start" => Some(p2_start),
        "p2_back" => Some(p2_back),
        "p2_menuup" => Some(p2_menu_up),
        "p2_menudown" => Some(p2_menu_down),
        "p2_menuleft" => Some(p2_menu_left),
        "p2_menuright" => Some(p2_menu_right),
        "p2_select" => Some(p2_select),
        "p2_operator" => Some(p2_operator),
        "p2_restart" => Some(p2_restart),
        _ => None,
    }
}

#[inline(always)]
fn action_to_ini_key(action: VirtualAction) -> &'static str {
    use VirtualAction::*;
    match action {
        p1_up => "P1_Up",
        p1_down => "P1_Down",
        p1_left => "P1_Left",
        p1_right => "P1_Right",
        p1_start => "P1_Start",
        p1_back => "P1_Back",
        p1_menu_up => "P1_MenuUp",
        p1_menu_down => "P1_MenuDown",
        p1_menu_left => "P1_MenuLeft",
        p1_menu_right => "P1_MenuRight",
        p1_select => "P1_Select",
        p1_operator => "P1_Operator",
        p1_restart => "P1_Restart",
        p2_up => "P2_Up",
        p2_down => "P2_Down",
        p2_left => "P2_Left",
        p2_right => "P2_Right",
        p2_start => "P2_Start",
        p2_back => "P2_Back",
        p2_menu_up => "P2_MenuUp",
        p2_menu_down => "P2_MenuDown",
        p2_menu_left => "P2_MenuLeft",
        p2_menu_right => "P2_MenuRight",
        p2_select => "P2_Select",
        p2_operator => "P2_Operator",
        p2_restart => "P2_Restart",
    }
}

#[inline(always)]
fn binding_to_token(binding: InputBinding) -> String {
    match binding {
        InputBinding::Key(code) => format!("KeyCode::{:?}", code),
        InputBinding::PadDir(dir) => format!("PadDir::{:?}", dir),
        InputBinding::PadDirOn { device, dir } => {
            format!("Pad{}::Dir::{:?}", device, dir)
        }
        InputBinding::GamepadCode(binding) => {
            let mut s = String::new();
            use std::fmt::Write;
            let _ = write!(&mut s, "PadCode[0x{:08X}]", binding.code_u32);
            if let Some(device) = binding.device {
                let _ = write!(&mut s, "@{}", device);
            }
            if let Some(uuid) = binding.uuid {
                s.push('#');
                for b in &uuid {
                    let _ = write!(&mut s, "{:02X}", b);
                }
            }
            s
        }
    }
}

#[inline(always)]
fn parse_binding_token(tok: &str) -> Option<InputBinding> {
    let t = tok.trim();
    // Keyboard
    if let Some(rest) = t.strip_prefix("KeyCode::") {
        let code = match rest {
            // Special keys
            "Enter" => KeyCode::Enter,
            "Escape" => KeyCode::Escape,
            "ArrowUp" => KeyCode::ArrowUp,
            "ArrowDown" => KeyCode::ArrowDown,
            "ArrowLeft" => KeyCode::ArrowLeft,
            "ArrowRight" => KeyCode::ArrowRight,
            // Numpad keys
            "Numpad0" => KeyCode::Numpad0,
            "Numpad1" => KeyCode::Numpad1,
            "Numpad2" => KeyCode::Numpad2,
            "Numpad3" => KeyCode::Numpad3,
            "Numpad4" => KeyCode::Numpad4,
            "Numpad5" => KeyCode::Numpad5,
            "Numpad6" => KeyCode::Numpad6,
            "Numpad7" => KeyCode::Numpad7,
            "Numpad8" => KeyCode::Numpad8,
            "Numpad9" => KeyCode::Numpad9,
            "NumpadAdd" => KeyCode::NumpadAdd,
            "NumpadDivide" => KeyCode::NumpadDivide,
            "NumpadDecimal" => KeyCode::NumpadDecimal,
            "NumpadComma" => KeyCode::NumpadComma,
            "NumpadEnter" => KeyCode::NumpadEnter,
            "NumpadEqual" => KeyCode::NumpadEqual,
            "NumpadMultiply" => KeyCode::NumpadMultiply,
            "NumpadSubtract" => KeyCode::NumpadSubtract,
            // Letter keys A-Z
            "KeyA" => KeyCode::KeyA,
            "KeyB" => KeyCode::KeyB,
            "KeyC" => KeyCode::KeyC,
            "KeyD" => KeyCode::KeyD,
            "KeyE" => KeyCode::KeyE,
            "KeyF" => KeyCode::KeyF,
            "KeyG" => KeyCode::KeyG,
            "KeyH" => KeyCode::KeyH,
            "KeyI" => KeyCode::KeyI,
            "KeyJ" => KeyCode::KeyJ,
            "KeyK" => KeyCode::KeyK,
            "KeyL" => KeyCode::KeyL,
            "KeyM" => KeyCode::KeyM,
            "KeyN" => KeyCode::KeyN,
            "KeyO" => KeyCode::KeyO,
            "KeyP" => KeyCode::KeyP,
            "KeyQ" => KeyCode::KeyQ,
            "KeyR" => KeyCode::KeyR,
            "KeyS" => KeyCode::KeyS,
            "KeyT" => KeyCode::KeyT,
            "KeyU" => KeyCode::KeyU,
            "KeyV" => KeyCode::KeyV,
            "KeyW" => KeyCode::KeyW,
            "KeyX" => KeyCode::KeyX,
            "KeyY" => KeyCode::KeyY,
            "KeyZ" => KeyCode::KeyZ,
            _ => return None,
        };
        return Some(InputBinding::Key(code));
    }

    // Gamepad low-level code binding:
    //   PadCode[0xDEADBEEF]
    //   PadCode[0xDEADBEEF]@0
    //   PadCode[0xDEADBEEF]#00112233AABBCCDDEEFF001122334455
    //   PadCode[0xDEADBEEF]@0#00112233AABBCCDDEEFF001122334455
    //
    // where 0x... or decimal is the `PadCode(u32)` shown in the Sandbox/Input screens,
    // @N restricts to device index N,
    // and #... restricts to a 16-byte UUID (32 hex chars, no dashes).
    if let Some(rest) = t.strip_prefix("PadCode[") {
        if let Some(end) = rest.find(']') {
            let code_str = &rest[..end];
            let mut tail = &rest[end + 1..];

            let code_u32 = if let Some(hex) = code_str
                .strip_prefix("0x")
                .or_else(|| code_str.strip_prefix("0X"))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                u32::from_str(code_str).ok()?
            };

            let mut device: Option<usize> = None;
            let mut uuid: Option<[u8; 16]> = None;

            // Parse optional @device and #uuid, in any order.
            loop {
                if let Some(rest2) = tail.strip_prefix('@') {
                    let mut digits = String::new();
                    for ch in rest2.chars() {
                        if ch.is_ascii_digit() {
                            digits.push(ch);
                        } else {
                            break;
                        }
                    }
                    if digits.is_empty() {
                        break;
                    }
                    if let Ok(dev_idx) = usize::from_str(&digits) {
                        device = Some(dev_idx);
                    }
                    tail = &rest2[digits.len()..];
                    continue;
                }

                if let Some(rest2) = tail.strip_prefix('#') {
                    let mut hex_digits = String::new();
                    for ch in rest2.chars() {
                        if ch.is_ascii_hexdigit() {
                            hex_digits.push(ch);
                        } else {
                            break;
                        }
                    }
                    if hex_digits.len() == 32 {
                        let mut bytes = [0u8; 16];
                        let mut ok = true;
                        for i in 0..16 {
                            let start = i * 2;
                            let end = start + 2;
                            match u8::from_str_radix(&hex_digits[start..end], 16) {
                                Ok(b) => bytes[i] = b,
                                Err(_) => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            uuid = Some(bytes);
                        }
                    }
                    tail = &rest2[hex_digits.len()..];
                    continue;
                }

                break;
            }

            return Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32,
                device,
                uuid,
            }));
        }
    }

    // Gamepad (any pad): PadDir::Up
    if let Some(rest) = t.strip_prefix("PadDir::") {
        let dir = match rest {
            "Up" => PadDir::Up,
            "Down" => PadDir::Down,
            "Left" => PadDir::Left,
            "Right" => PadDir::Right,
            _ => return None,
        };
        return Some(InputBinding::PadDir(dir));
    }

    // Gamepad (device-specific): Pad0::Dir::Up
    // Split by "::"
    let parts: Vec<&str> = t.split("::").collect();
    if parts.len() == 3 {
        let (pad_part, kind, name) = (parts[0], parts[1], parts[2]);
        // Parse device index from PadN
        if let Some(dev_str) = pad_part.strip_prefix("Pad") {
            if dev_str.is_empty() {
                // Treat as any-pad; handled at top via PadDir prefix.
                return match kind {
                    "Dir" => match name {
                        "Up" => Some(InputBinding::PadDir(PadDir::Up)),
                        "Down" => Some(InputBinding::PadDir(PadDir::Down)),
                        "Left" => Some(InputBinding::PadDir(PadDir::Left)),
                        "Right" => Some(InputBinding::PadDir(PadDir::Right)),
                        _ => None,
                    },
                    _ => None,
                };
            }
            if let Ok(device) = dev_str.parse::<usize>() {
                return match kind {
                    "Dir" => match name {
                        "Up" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Up,
                        }),
                        "Down" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Down,
                        }),
                        "Left" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Left,
                        }),
                        "Right" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Right,
                        }),
                        _ => None,
                    },
                    _ => None,
                };
            }
        }
    }

    None
}

fn load_keymap_from_ini_local(conf: &SimpleIni) -> Keymap {
    // When [Keymaps] is present, start from explicit user entries and then fill
    // in any completely missing actions from built-in defaults. When the whole
    // section is absent, fall back to defaults entirely.
    if let Some(section) = conf
        .get_section("Keymaps")
        .or_else(|| conf.get_section("keymaps"))
    {
        let mut km = Keymap::default();
        let mut seen: Vec<VirtualAction> = Vec::new();

        for (k, v) in section {
            let key = k.to_ascii_lowercase();
            if let Some(action) = parse_action_key_lower(&key) {
                let mut bindings = Vec::new();
                for tok in v.split(',') {
                    if let Some(b) = parse_binding_token(tok) {
                        bindings.push(b);
                    }
                }
                km.bind(action, &bindings);
                seen.push(action);
            }
        }

        let defaults = default_keymap_local();
        for act in ALL_VIRTUAL_ACTIONS {
            if !seen.contains(&act) {
                let mut bindings = Vec::new();
                let mut i = 0;
                while let Some(b) = defaults.binding_at(act, i) {
                    bindings.push(b);
                    i += 1;
                }
                if !bindings.is_empty() {
                    km.bind(act, &bindings);
                }
            }
        }

        km
    } else {
        default_keymap_local()
    }
}

/// Update a keyboard binding in Primary/Secondary slots, ensuring that the
/// given key code is not used in any other Primary/Secondary slot for P1/P2.
/// Default slots (index 0) are never modified.
pub fn update_keymap_binding_unique_keyboard(
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) {
    // Update keyboard bindings while ensuring that `keycode` is unique across
    // all Primary/Secondary slots (index >= 1) for P1/P2.
    let current = crate::core::input::get_keymap();
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings: Vec<InputBinding> = Vec::new();
        let mut i = 0;
        while let Some(b) = current.binding_at(act, i) {
            bindings.push(b);
            i += 1;
        }

        // Remove this key from all Primary/Secondary slots (index >= 1).
        if !bindings.is_empty() {
            let mut filtered: Vec<InputBinding> = Vec::with_capacity(bindings.len());
            for (slot_idx, b) in bindings.iter().enumerate() {
                if slot_idx >= 1 {
                    if let InputBinding::Key(code) = b {
                        if *code == keycode {
                            continue;
                        }
                    }
                }
                filtered.push(*b);
            }
            bindings = filtered;
        }

        if act == action {
            // If Secondary requested but there is no Primary yet, collapse this
            // to the first non-default slot so we don't implicitly duplicate
            // defaults into Primary.
            let slot_count_before = bindings.len();
            let mut effective_index = index;
            if index >= 2 && slot_count_before <= 1 {
                effective_index = 1;
            }

            let new_binding = InputBinding::Key(keycode);
            if effective_index == 0 {
                if bindings.is_empty() {
                    bindings.push(new_binding);
                } else {
                    bindings[0] = new_binding;
                }
            } else if bindings.len() <= effective_index {
                if bindings.is_empty() {
                    bindings.push(new_binding);
                } else {
                    bindings.push(new_binding);
                }
            } else {
                bindings[effective_index] = new_binding;
            }
        }

        new_map.bind(act, &bindings);
    }

    crate::core::input::set_keymap(new_map);
    save_without_keymaps();
}

/// Update a gamepad binding in Primary/Secondary slots, ensuring that the
/// given physical binding is not used in any other Primary/Secondary slot
/// for P1/P2. Default slots (index 0) are never modified.
pub fn update_keymap_binding_unique_gamepad(
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) {
    let current = crate::core::input::get_keymap();
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings: Vec<InputBinding> = Vec::new();
        let mut i = 0;
        while let Some(b) = current.binding_at(act, i) {
            bindings.push(b);
            i += 1;
        }

        // Remove this binding from all Primary/Secondary slots (index >= 1).
        if !bindings.is_empty() {
            let mut filtered: Vec<InputBinding> = Vec::with_capacity(bindings.len());
            for (slot_idx, b) in bindings.iter().enumerate() {
                if slot_idx >= 1 && *b == binding {
                    continue;
                }
                filtered.push(*b);
            }
            bindings = filtered;
        }

        if act == action {
            // If Secondary requested but there is no Primary yet, collapse this
            // to the first non-default slot so we don't implicitly duplicate
            // defaults into Primary.
            let slot_count_before = bindings.len();
            let mut effective_index = index;
            if index >= 2 && slot_count_before <= 1 {
                effective_index = 1;
            }

            if effective_index == 0 {
                if bindings.is_empty() {
                    bindings.push(binding);
                } else {
                    bindings[0] = binding;
                }
            } else if bindings.len() <= effective_index {
                if bindings.is_empty() {
                    bindings.push(binding);
                } else {
                    bindings.push(binding);
                }
            } else {
                bindings[effective_index] = binding;
            }
        }

        new_map.bind(act, &bindings);
    }

    crate::core::input::set_keymap(new_map);
    save_without_keymaps();
}

fn save_without_keymaps() {
    // Manual writer that keeps [Options]/[Theme] sorted and emits a stable,
    // CamelCase [Keymaps] section derived from the current in-memory keymap.
    let cfg = CONFIG.lock().unwrap();
    let keymap = crate::core::input::get_keymap();

    let mut content = String::new();

    // [Options] (alphabetical order)
    content.push_str("[Options]\n");
    let audio_rate_str = match cfg.audio_sample_rate_hz {
        None => "Auto".to_string(),
        Some(hz) => hz.to_string(),
    };
    content.push_str(&format!("AudioSampleRateHz={}\n", audio_rate_str));
    content.push_str(&format!(
        "CacheSongs={}\n",
        if cfg.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!("DisplayHeight={}\n", cfg.display_height));
    content.push_str(&format!("DisplayWidth={}\n", cfg.display_width));
    content.push_str(&format!(
        "FastLoad={}\n",
        if cfg.fastload { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FullscreenType={}\n",
        cfg.fullscreen_type.as_str()
    ));
    content.push_str(&format!(
        "GlobalOffsetSeconds={}\n",
        cfg.global_offset_seconds
    ));
    content.push_str(&format!("MasterVolume={}\n", cfg.master_volume));
    content.push_str(&format!(
        "MenuMusic={}\n",
        if cfg.menu_music { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MineHitSound={}\n",
        if cfg.mine_hit_sound { "1" } else { "0" }
    ));
    content.push_str(&format!("MusicVolume={}\n", cfg.music_volume));
    content.push_str(&format!(
        "RateModPreservesPitch={}\n",
        if cfg.rate_mod_preserves_pitch {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowStats={}\n",
        if cfg.show_stats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if cfg.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!("DisplayMonitor={}\n", cfg.display_monitor));
    content.push_str(&format!(
        "SongParsingThreads={}\n",
        cfg.song_parsing_threads
    ));
    content.push_str(&format!(
        "SoftwareRendererThreads={}\n",
        cfg.software_renderer_threads
    ));
    content.push_str(&format!("SFXVolume={}\n", cfg.sfx_volume));
    content.push_str(&format!(
        "TranslatedTitles={}\n",
        if cfg.translated_titles { "1" } else { "0" }
    ));
    content.push_str(&format!("VideoRenderer={}\n", cfg.video_renderer));
    content.push_str(&format!("Vsync={}\n", if cfg.vsync { "1" } else { "0" }));
    content.push_str(&format!(
        "Windowed={}\n",
        if cfg.windowed { "1" } else { "0" }
    ));
    content.push('\n');

    // [Keymaps] – stable order with CamelCase keys.
    content.push_str("[Keymaps]\n");
    for act in ALL_VIRTUAL_ACTIONS {
        let key_name = action_to_ini_key(act);
        let mut tokens: Vec<String> = Vec::new();
        let mut i = 0;
        while let Some(binding) = keymap.binding_at(act, i) {
            tokens.push(binding_to_token(binding));
            i += 1;
        }
        let value = tokens.join(",");
        content.push_str(key_name);
        content.push('=');
        content.push_str(&value);
        content.push('\n');
    }

    // [Theme] – last section
    content.push('\n');
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

pub fn update_display_mode(mode: DisplayMode) {
    let mut dirty = false;
    {
        let mut cfg = CONFIG.lock().unwrap();
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
        let mut cfg = CONFIG.lock().unwrap();
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
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.display_monitor == monitor {
            return;
        }
        cfg.display_monitor = monitor;
    }
    save_without_keymaps();
}

pub fn update_video_renderer(renderer: BackendType) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.video_renderer == renderer {
            return;
        }
        cfg.video_renderer = renderer;
    }
    save_without_keymaps();
}

pub fn update_simply_love_color(index: i32) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        // No change, no need to write to disk.
        if cfg.simply_love_color == index {
            return;
        }
        cfg.simply_love_color = index;
    }
    save_without_keymaps();
}

#[allow(dead_code)]
pub fn update_global_offset(offset: f32) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if (cfg.global_offset_seconds - offset).abs() < f32::EPSILON {
            return;
        }
        cfg.global_offset_seconds = offset;
    }
    save_without_keymaps();
}

pub fn update_vsync(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.vsync == enabled {
            return;
        }
        cfg.vsync = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_stats(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_stats == enabled {
            return;
        }
        cfg.show_stats = enabled;
    }
    save_without_keymaps();
}

pub fn update_master_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.master_volume == vol {
            return;
        }
        cfg.master_volume = vol;
    }
    save_without_keymaps();
}

pub fn update_audio_sample_rate(rate: Option<u32>) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.audio_sample_rate_hz == rate {
            return;
        }
        cfg.audio_sample_rate_hz = rate;
    }
    save_without_keymaps();
}

pub fn update_mine_hit_sound(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.mine_hit_sound == enabled {
            return;
        }
        cfg.mine_hit_sound = enabled;
    }
    save_without_keymaps();
}

pub fn update_rate_mod_preserves_pitch(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.rate_mod_preserves_pitch == enabled {
            return;
        }
        cfg.rate_mod_preserves_pitch = enabled;
    }
    save_without_keymaps();
}
