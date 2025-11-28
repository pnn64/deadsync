use crate::core::gfx::BackendType;
use crate::core::input::{Keymap, VirtualAction, InputBinding, PadDir, PadButton, FaceBtn, GamepadCodeBinding};
use winit::keyboard::KeyCode;
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
    // None = auto (use device default sample rate)
    pub audio_sample_rate_hz: Option<u32>,
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
            audio_sample_rate_hz: None,
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
    content.push_str("AudioSampleRateHz=Auto\n");
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
    content.push_str("P1_Up=KeyCode::ArrowUp,KeyCode::KeyW\n\n");

    std::fs::write(CONFIG_PATH, content)
}

pub fn load() {
    // --- Load main deadsync.ini ---
    if !std::path::Path::new(CONFIG_PATH).exists()
        && let Err(e) = create_default_config_file() {
            warn!("Failed to create default config file: {}", e);
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
                cfg.fastload = conf.get("Options", "FastLoad").and_then(|v| v.parse::<u8>().ok()).map_or(default.fastload, |v| v != 0);
                cfg.cachesongs = conf.get("Options", "CacheSongs").and_then(|v| v.parse::<u8>().ok()).map_or(default.cachesongs, |v| v != 0);
                cfg.smooth_histogram = conf.get("Options", "SmoothHistogram").and_then(|v| v.parse::<u8>().ok()).map_or(default.smooth_histogram, |v| v != 0);
                cfg.simply_love_color = conf.get("Theme", "SimplyLoveColor").and_then(|v| v.parse().ok()).unwrap_or(default.simply_love_color);
                
                info!("Configuration loaded from '{}'.", CONFIG_PATH);
            } // Lock on CONFIG is released here.

            // Load keymaps from the same INI and publish globally
            let km = load_keymap_from_ini_local(&conf);
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
                conf2.set("Keymaps", "P1_Up", Some("KeyCode::ArrowUp,KeyCode::KeyW".to_string()));
                conf2.set("Keymaps", "P1_Down", Some("KeyCode::ArrowDown,KeyCode::KeyS".to_string()));
                conf2.set("Keymaps", "P1_Left", Some("KeyCode::ArrowLeft,KeyCode::KeyA".to_string()));
                conf2.set("Keymaps", "P1_Right", Some("KeyCode::ArrowRight,KeyCode::KeyD".to_string()));
                conf2.set("Keymaps", "P1_MenuUp", Some("".to_string()));
                conf2.set("Keymaps", "P1_MenuDown", Some("".to_string()));
                conf2.set("Keymaps", "P1_MenuLeft", Some("".to_string()));
                conf2.set("Keymaps", "P1_MenuRight", Some("".to_string()));
                conf2.set("Keymaps", "P1_Select", Some("".to_string()));
                conf2.set("Keymaps", "P1_Operator", Some("".to_string()));
                conf2.set("Keymaps", "P1_Restart", Some("".to_string()));
                need_write_keymaps = true;
            } else {
                // Add only missing keys
                need_write_keymaps |= ensure(&mut conf2, "P1_Back", "KeyCode::Escape");
                need_write_keymaps |= ensure(&mut conf2, "P1_Start", "KeyCode::Enter");
                need_write_keymaps |= ensure(&mut conf2, "P1_Up", "KeyCode::ArrowUp,KeyCode::KeyW");
                need_write_keymaps |= ensure(&mut conf2, "P1_Down", "KeyCode::ArrowDown,KeyCode::KeyS");
                need_write_keymaps |= ensure(&mut conf2, "P1_Left", "KeyCode::ArrowLeft,KeyCode::KeyA");
                need_write_keymaps |= ensure(&mut conf2, "P1_Right", "KeyCode::ArrowRight,KeyCode::KeyD");
                need_write_keymaps |= ensure(&mut conf2, "P1_MenuUp", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_MenuDown", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_MenuLeft", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_MenuRight", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_Select", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_Operator", "");
                need_write_keymaps |= ensure(&mut conf2, "P1_Restart", "");
            }
            if need_write_keymaps
                && let Err(e) = conf2.write(CONFIG_PATH) { warn!("Failed to append missing keymaps: {}", e); }

            // Only write [Options]/[Theme] if any of those keys are missing.
            let missing_opts = {
                let has = |sec: &str, key: &str| conf.get(sec, key).is_some();
                let mut miss = false;
                let options_keys = [
                    "AudioSampleRateHz","CacheSongs","DisplayHeight","DisplayWidth","FastLoad","GlobalOffsetSeconds",
                    "MasterVolume","MusicVolume","ShowStats","SmoothHistogram","SFXVolume",
                    "VideoRenderer","Vsync","Windowed"
                ];
                for k in options_keys { if !has("Options", k) { miss = true; break; } }
                if !miss && !has("Theme","SimplyLoveColor") { miss = true; }
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

// --- Keymap defaults and parsing (kept in config to avoid coupling input.rs to config) ---

fn default_keymap_local() -> Keymap {
    use VirtualAction as A;
    let mut km = Keymap::default();
    km.bind(A::p1_up,    &[
        InputBinding::Key(KeyCode::ArrowUp), InputBinding::Key(KeyCode::KeyW),
    ]);
    km.bind(A::p1_down,  &[
        InputBinding::Key(KeyCode::ArrowDown), InputBinding::Key(KeyCode::KeyS),
    ]);
    km.bind(A::p1_left,  &[
        InputBinding::Key(KeyCode::ArrowLeft), InputBinding::Key(KeyCode::KeyA),
    ]);
    km.bind(A::p1_right, &[
        InputBinding::Key(KeyCode::ArrowRight), InputBinding::Key(KeyCode::KeyD),
    ]);
    km.bind(A::p1_start, &[
        InputBinding::Key(KeyCode::Enter)
    ]);
    km.bind(A::p1_back, &[
        InputBinding::Key(KeyCode::Escape)
    ]);
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
        _ => None,
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
            // Letter keys A-Z
            "KeyA" => KeyCode::KeyA, "KeyB" => KeyCode::KeyB, "KeyC" => KeyCode::KeyC, "KeyD" => KeyCode::KeyD,
            "KeyE" => KeyCode::KeyE, "KeyF" => KeyCode::KeyF, "KeyG" => KeyCode::KeyG, "KeyH" => KeyCode::KeyH,
            "KeyI" => KeyCode::KeyI, "KeyJ" => KeyCode::KeyJ, "KeyK" => KeyCode::KeyK, "KeyL" => KeyCode::KeyL,
            "KeyM" => KeyCode::KeyM, "KeyN" => KeyCode::KeyN, "KeyO" => KeyCode::KeyO, "KeyP" => KeyCode::KeyP,
            "KeyQ" => KeyCode::KeyQ, "KeyR" => KeyCode::KeyR, "KeyS" => KeyCode::KeyS, "KeyT" => KeyCode::KeyT,
            "KeyU" => KeyCode::KeyU, "KeyV" => KeyCode::KeyV, "KeyW" => KeyCode::KeyW, "KeyX" => KeyCode::KeyX,
            "KeyY" => KeyCode::KeyY, "KeyZ" => KeyCode::KeyZ,
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
    // where 0x... or decimal is gilrs::ev::Code::into_u32(), @N restricts to device index N,
    // and #... restricts to a 16-byte UUID (32 hex chars, no dashes).
    if let Some(rest) = t.strip_prefix("PadCode[") {
        if let Some(end) = rest.find(']') {
            let code_str = &rest[..end];
            let mut tail = &rest[end + 1..];

            let code_u32 = if let Some(hex) = code_str.strip_prefix("0x").or_else(|| code_str.strip_prefix("0X")) {
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

    // Gamepad (any pad): PadDir::Up, PadButton::Confirm, Face::WestX
    if let Some(rest) = t.strip_prefix("PadDir::") {
        let dir = match rest { "Up" => PadDir::Up, "Down" => PadDir::Down, "Left" => PadDir::Left, "Right" => PadDir::Right, _ => return None };
        return Some(InputBinding::PadDir(dir));
    }
    if let Some(rest) = t.strip_prefix("PadButton::") {
        let btn = match rest { "Confirm" => PadButton::Confirm, "Back" => PadButton::Back, _ => return None };
        return Some(InputBinding::PadButton(btn));
    }
    if let Some(rest) = t.strip_prefix("Face::") {
        let btn = match rest { "SouthA" => FaceBtn::SouthA, "EastB" => FaceBtn::EastB, "WestX" => FaceBtn::WestX, "NorthY" => FaceBtn::NorthY, _ => return None };
        return Some(InputBinding::Face(btn));
    }

    // Gamepad (device-specific): Pad0::Dir::Up, Pad1::Button::Confirm, Pad0::Face::WestX
    // Also accept Pad::... as any pad (handled above) but keep here for clarity.
    // Split by "::"
    let parts: Vec<&str> = t.split("::").collect();
    if parts.len() == 3 {
        let (pad_part, kind, name) = (parts[0], parts[1], parts[2]);
        // Parse device index from PadN
        if let Some(dev_str) = pad_part.strip_prefix("Pad") {
            if dev_str.is_empty() {
                // Treat as any-pad; handled at top via PadDir/PadButton/Face prefixes.
                // But allow here too for flexibility.
                return match kind {
                    "Dir" => match name { "Up" => Some(InputBinding::PadDir(PadDir::Up)), "Down" => Some(InputBinding::PadDir(PadDir::Down)), "Left" => Some(InputBinding::PadDir(PadDir::Left)), "Right" => Some(InputBinding::PadDir(PadDir::Right)), _ => None },
                    "Button" => match name { "Confirm" => Some(InputBinding::PadButton(PadButton::Confirm)), "Back" => Some(InputBinding::PadButton(PadButton::Back)), _ => None },
                    "Face" => match name { "SouthA" => Some(InputBinding::Face(FaceBtn::SouthA)), "EastB" => Some(InputBinding::Face(FaceBtn::EastB)), "WestX" => Some(InputBinding::Face(FaceBtn::WestX)), "NorthY" => Some(InputBinding::Face(FaceBtn::NorthY)), _ => None },
                    _ => None,
                };
            }
            if let Ok(device) = dev_str.parse::<usize>() {
                return match kind {
                    "Dir" => match name {
                        "Up" => Some(InputBinding::PadDirOn { device, dir: PadDir::Up }),
                        "Down" => Some(InputBinding::PadDirOn { device, dir: PadDir::Down }),
                        "Left" => Some(InputBinding::PadDirOn { device, dir: PadDir::Left }),
                        "Right" => Some(InputBinding::PadDirOn { device, dir: PadDir::Right }),
                        _ => None,
                    },
                    "Button" => match name {
                        "Confirm" => Some(InputBinding::PadButtonOn { device, btn: PadButton::Confirm }),
                        "Back" => Some(InputBinding::PadButtonOn { device, btn: PadButton::Back }),
                        _ => None,
                    },
                    "Face" => match name {
                        "SouthA" => Some(InputBinding::FaceOn { device, btn: FaceBtn::SouthA }),
                        "EastB" => Some(InputBinding::FaceOn { device, btn: FaceBtn::EastB }),
                        "WestX" => Some(InputBinding::FaceOn { device, btn: FaceBtn::WestX }),
                        "NorthY" => Some(InputBinding::FaceOn { device, btn: FaceBtn::NorthY }),
                        _ => None,
                    },
                    _ => None,
                };
            }
        }
    }

    None
}

fn load_keymap_from_ini_local(conf: &Ini) -> Keymap {
    let section = conf
        .get_map_ref()
        .get("Keymaps")
        .or_else(|| conf.get_map_ref().get("keymaps"));
    if let Some(section) = section {
        let mut km = Keymap::default();
        for (k, v_opt) in section {
            let key = k.to_ascii_lowercase();
            if let Some(action) = parse_action_key_lower(&key) {
                let mut bindings = Vec::new();
                if let Some(value) = v_opt.as_deref() {
                    for tok in value.split(',') {
                        if let Some(b) = parse_binding_token(tok) { bindings.push(b); }
                    }
                }
                km.bind(action, &bindings);
            }
        }
        return km;
    }
    default_keymap_local()
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
    let audio_rate_str = match cfg.audio_sample_rate_hz {
        None => "Auto".to_string(),
        Some(hz) => hz.to_string(),
    };
    content.push_str(&format!("AudioSampleRateHz={}\n", audio_rate_str));
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
