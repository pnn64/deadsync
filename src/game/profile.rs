pub use super::scroll::ScrollSpeedSetting;
use configparser::ini::Ini;
use log::{info, warn};
use once_cell::sync::Lazy;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

// --- Profile Data ---
const PROFILE_DIR: &str = "save/profiles/00000000";
const PROFILE_INI_PATH: &str = "save/profiles/00000000/profile.ini";
const GROOVESTATS_INI_PATH: &str = "save/profiles/00000000/groovestats.ini";
const PROFILE_AVATAR_PATH: &str = "save/profiles/00000000/profile.png";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundFilter {
    Off,
    Dark,
    Darker,
    Darkest,
}

impl Default for BackgroundFilter {
    fn default() -> Self {
        BackgroundFilter::Darkest
    }
}

impl FromStr for BackgroundFilter {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "dark" => Ok(Self::Dark),
            "darker" => Ok(Self::Darker),
            "darkest" => Ok(Self::Darkest),
            _ => Err(format!("'{}' is not a valid BackgroundFilter setting", s)),
        }
    }
}

impl core::fmt::Display for BackgroundFilter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Dark => write!(f, "Dark"),
            Self::Darker => write!(f, "Darker"),
            Self::Darkest => write!(f, "Darkest"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub display_name: String,
    pub player_initials: String,
    pub groovestats_api_key: String,
    pub groovestats_is_pad_player: bool,
    pub groovestats_username: String,
    pub background_filter: BackgroundFilter,
    pub avatar_path: Option<PathBuf>,
    pub avatar_texture_key: Option<String>,
    pub scroll_speed: ScrollSpeedSetting,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            display_name: "Player 1".to_string(),
            player_initials: "P1".to_string(),
            groovestats_api_key: "".to_string(),
            groovestats_is_pad_player: false,
            groovestats_username: "".to_string(),
            background_filter: BackgroundFilter::default(),
            avatar_path: None,
            avatar_texture_key: None,
            scroll_speed: ScrollSpeedSetting::default(),
        }
    }
}

// Global static for the current profile.
static PROFILE: Lazy<Mutex<Profile>> = Lazy::new(|| Mutex::new(Profile::default()));

/// Creates the default profile directory and .ini files if they don't exist.
fn create_default_files() -> Result<(), std::io::Error> {
    info!(
        "Profile files not found, creating defaults in '{}'.",
        PROFILE_DIR
    );
    fs::create_dir_all(PROFILE_DIR)?;

    // Create profile.ini
    if !Path::new(PROFILE_INI_PATH).exists() {
        let default_profile = Profile::default();
        let mut content = String::new();

        content.push_str("[PlayerOptions]\n");
        content.push_str(&format!("BackgroundFilter = {}\n", default_profile.background_filter));
        content.push_str(&format!("ScrollSpeed = {}\n", default_profile.scroll_speed));
        content.push('\n');

        content.push_str("[userprofile]\n");
        content.push_str(&format!("DisplayName = {}\n", default_profile.display_name));
        content.push_str(&format!("PlayerInitials = {}\n", default_profile.player_initials));
        content.push('\n');

        fs::write(PROFILE_INI_PATH, content)?;
    }

    // Create groovestats.ini
    if !Path::new(GROOVESTATS_INI_PATH).exists() {
        let mut content = String::new();

        content.push_str("[GrooveStats]\n");
        content.push_str("ApiKey = \n");
        content.push_str("IsPadPlayer = 0\n");
        content.push_str("Username = \n");
        content.push('\n');

        fs::write(GROOVESTATS_INI_PATH, content)?;
    }

    Ok(())
}

fn save_profile_ini() {
    let profile = PROFILE.lock().unwrap();
    let mut content = String::new();

    content.push_str("[PlayerOptions]\n");
    content.push_str(&format!("BackgroundFilter={}\n", profile.background_filter));
    content.push_str(&format!("ScrollSpeed={}\n", profile.scroll_speed));
    content.push('\n');

    content.push_str("[userprofile]\n");
    content.push_str(&format!("DisplayName={}\n", profile.display_name));
    content.push_str(&format!("PlayerInitials={}\n", profile.player_initials));
    content.push('\n');

    if let Err(e) = fs::write(PROFILE_INI_PATH, content) {
        warn!("Failed to save {}: {}", PROFILE_INI_PATH, e);
    }
}

fn save_groovestats_ini() {
    let profile = PROFILE.lock().unwrap();
    let mut content = String::new();

    content.push_str("[GrooveStats]\n");
    content.push_str(&format!("ApiKey={}\n", profile.groovestats_api_key));
    content.push_str(&format!("IsPadPlayer={}\n", if profile.groovestats_is_pad_player { "1" } else { "0" }));
    content.push_str(&format!("Username={}\n", profile.groovestats_username));
    content.push('\n');

    if let Err(e) = fs::write(GROOVESTATS_INI_PATH, content) {
        warn!("Failed to save {}: {}", GROOVESTATS_INI_PATH, e);
    }
}

pub fn load() {
    if !Path::new(PROFILE_INI_PATH).exists() || !Path::new(GROOVESTATS_INI_PATH).exists() {
        if let Err(e) = create_default_files() {
            warn!("Failed to create default profile files: {}", e);
            // Proceed with default struct values and attempt to save them.
        }
    }

    {
        let mut profile = PROFILE.lock().unwrap();
        let default_profile = Profile::default();

        // Load profile.ini
        let mut profile_conf = Ini::new();
        if profile_conf.load(PROFILE_INI_PATH).is_ok() {
            profile.display_name = profile_conf
                .get("userprofile", "DisplayName")
                .unwrap_or(default_profile.display_name.clone());
            profile.player_initials = profile_conf
                .get("userprofile", "PlayerInitials")
                .unwrap_or(default_profile.player_initials.clone());
            profile.background_filter = profile_conf
                .get("PlayerOptions", "BackgroundFilter")
                .and_then(|s| BackgroundFilter::from_str(&s).ok())
                .unwrap_or(default_profile.background_filter);
            profile.scroll_speed = profile_conf
                .get("PlayerOptions", "ScrollSpeed")
                .and_then(|s| ScrollSpeedSetting::from_str(&s).ok())
                .unwrap_or(default_profile.scroll_speed);
        } else {
            warn!(
                "Failed to load '{}', using default profile settings.",
                PROFILE_INI_PATH
            );
        }

        // Load groovestats.ini
        let mut gs_conf = Ini::new();
        if gs_conf.load(GROOVESTATS_INI_PATH).is_ok() {
            profile.groovestats_api_key = gs_conf
                .get("GrooveStats", "ApiKey")
                .unwrap_or(default_profile.groovestats_api_key.clone());
            profile.groovestats_is_pad_player = gs_conf
                .get("GrooveStats", "IsPadPlayer")
                .and_then(|v| v.parse::<u8>().ok())
                .map_or(default_profile.groovestats_is_pad_player, |v| v != 0);
            profile.groovestats_username = gs_conf
                .get("GrooveStats", "Username")
                .unwrap_or(default_profile.groovestats_username.clone());
        } else {
            warn!(
                "Failed to load '{}', using default GrooveStats info.",
                GROOVESTATS_INI_PATH
            );
        }

        let avatar_path = Path::new(PROFILE_AVATAR_PATH);
        profile.avatar_path = if avatar_path.exists() {
            Some(avatar_path.to_path_buf())
        } else {
            None
        };
        profile.avatar_texture_key = None;
    } // Lock is released here.

    save_profile_ini();
    save_groovestats_ini();
    info!("Profile configuration files updated with default values for any missing fields.");
}

/// Returns a copy of the currently loaded profile data.
pub fn get() -> Profile {
    PROFILE.lock().unwrap().clone()
}

pub fn set_avatar_texture_key(key: Option<String>) {
    let mut profile = PROFILE.lock().unwrap();
    profile.avatar_texture_key = key;
}

pub fn update_scroll_speed(setting: ScrollSpeedSetting) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.scroll_speed == setting {
            return;
        }
        profile.scroll_speed = setting;
    }
    save_profile_ini();
}

pub fn update_background_filter(setting: BackgroundFilter) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.background_filter == setting {
            return;
        }
        profile.background_filter = setting;
    }
    save_profile_ini();
}
