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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldJudgmentGraphic {
    Love,
    Mute,
    ITG2,
    None,
}

impl Default for HoldJudgmentGraphic {
    fn default() -> Self {
        HoldJudgmentGraphic::Love
    }
}

impl FromStr for HoldJudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "love" => Ok(Self::Love),
            "mute" => Ok(Self::Mute),
            "itg2" => Ok(Self::ITG2),
            "none" => Ok(Self::None),
            other => Err(format!("'{}' is not a valid HoldJudgmentGraphic setting", other)),
        }
    }
}

impl core::fmt::Display for HoldJudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Love => write!(f, "Love"),
            Self::Mute => write!(f, "mute"),
            Self::ITG2 => write!(f, "ITG2"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgmentGraphic {
    Bebas,
    Censored,
    Chromatic,
    Code,
    ComicSans,
    Emoticon,
    Focus,
    Grammar,
    GrooveNights,
    ITG2,
    Love,
    LoveChroma,
    Miso,
    Papyrus,
    Rainbowmatic,
    Roboto,
    Shift,
    Tactics,
    Wendy,
    WendyChroma,
    None,
}

impl Default for JudgmentGraphic {
    fn default() -> Self {
        JudgmentGraphic::Love
    }
}

impl FromStr for JudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim().to_lowercase();
        match v.as_str() {
            "bebas" => Ok(Self::Bebas),
            "censored" => Ok(Self::Censored),
            "chromatic" => Ok(Self::Chromatic),
            "code" => Ok(Self::Code),
            "comic sans" => Ok(Self::ComicSans),
            "comicsans" => Ok(Self::ComicSans),
            "emoticon" => Ok(Self::Emoticon),
            "focus" => Ok(Self::Focus),
            "grammar" => Ok(Self::Grammar),
            "groovenights" => Ok(Self::GrooveNights),
            "groove nights" => Ok(Self::GrooveNights),
            "itg2" => Ok(Self::ITG2),
            "love" => Ok(Self::Love),
            "love chroma" => Ok(Self::LoveChroma),
            "lovechroma" => Ok(Self::LoveChroma),
            "miso" => Ok(Self::Miso),
            "papyrus" => Ok(Self::Papyrus),
            "rainbowmatic" => Ok(Self::Rainbowmatic),
            "roboto" => Ok(Self::Roboto),
            "shift" => Ok(Self::Shift),
            "tactics" => Ok(Self::Tactics),
            "wendy" => Ok(Self::Wendy),
            "wendy chroma" => Ok(Self::WendyChroma),
            "wendychroma" => Ok(Self::WendyChroma),
            "none" => Ok(Self::None),
            other => Err(format!("'{}' is not a valid JudgmentGraphic setting", other)),
        }
    }
}

impl core::fmt::Display for JudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bebas => write!(f, "Bebas"),
            Self::Censored => write!(f, "Censored"),
            Self::Chromatic => write!(f, "Chromatic"),
            Self::Code => write!(f, "Code"),
            Self::ComicSans => write!(f, "Comic Sans"),
            Self::Emoticon => write!(f, "Emoticon"),
            Self::Focus => write!(f, "Focus"),
            Self::Grammar => write!(f, "Grammar"),
            Self::GrooveNights => write!(f, "GrooveNights"),
            Self::ITG2 => write!(f, "ITG2"),
            Self::Love => write!(f, "Love"),
            Self::LoveChroma => write!(f, "Love Chroma"),
            Self::Miso => write!(f, "Miso"),
            Self::Papyrus => write!(f, "Papyrus"),
            Self::Rainbowmatic => write!(f, "Rainbowmatic"),
            Self::Roboto => write!(f, "Roboto"),
            Self::Shift => write!(f, "Shift"),
            Self::Tactics => write!(f, "Tactics"),
            Self::Wendy => write!(f, "Wendy"),
            Self::WendyChroma => write!(f, "Wendy Chroma"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboFont {
    Wendy,
    ArialRounded,
    Asap,
    BebasNeue,
    SourceCode,
    Work,
    WendyCursed,
    None,
}

impl Default for ComboFont {
    fn default() -> Self {
        ComboFont::Wendy
    }
}

impl FromStr for ComboFont {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim().to_lowercase();
        match v.as_str() {
            "wendy" => Ok(Self::Wendy),
            "arial rounded" | "arialrounded" => Ok(Self::ArialRounded),
            "asap" => Ok(Self::Asap),
            "bebas neue" | "bebasneue" => Ok(Self::BebasNeue),
            "source code" | "sourcecode" => Ok(Self::SourceCode),
            "work" => Ok(Self::Work),
            "wendy (cursed)" | "wendy cursed" | "wendycursed" => Ok(Self::WendyCursed),
            "none" => Ok(Self::None),
            other => Err(format!("'{}' is not a valid ComboFont setting", other)),
        }
    }
}

impl core::fmt::Display for ComboFont {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Wendy => write!(f, "Wendy"),
            Self::ArialRounded => write!(f, "Arial Rounded"),
            Self::Asap => write!(f, "Asap"),
            Self::BebasNeue => write!(f, "Bebas Neue"),
            Self::SourceCode => write!(f, "Source Code"),
            Self::Work => write!(f, "Work"),
            Self::WendyCursed => write!(f, "Wendy (Cursed)"),
            Self::None => write!(f, "None"),
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
    pub hold_judgment_graphic: HoldJudgmentGraphic,
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
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
            hold_judgment_graphic: HoldJudgmentGraphic::default(),
            judgment_graphic: JudgmentGraphic::default(),
            combo_font: ComboFont::default(),
            avatar_path: None,
            avatar_texture_key: None,
            scroll_speed: ScrollSpeedSetting::default(),
        }
    }
}

// Global static for the current profile.
static PROFILE: Lazy<Mutex<Profile>> = Lazy::new(|| Mutex::new(Profile::default()));

// --- Session-scoped state (not persisted) ---
#[derive(Debug)]
struct SessionState {
    music_rate: f32,
}

static SESSION: Lazy<Mutex<SessionState>> = Lazy::new(|| Mutex::new(SessionState { music_rate: 1.0 }));

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
        content.push_str(&format!(
            "HoldJudgmentGraphic = {}\n",
            default_profile.hold_judgment_graphic
        ));
        content.push_str(&format!(
            "JudgmentGraphic = {}\n",
            default_profile.judgment_graphic
        ));
        content.push_str(&format!(
            "ComboFont = {}\n",
            default_profile.combo_font
        ));
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
    content.push_str(&format!(
        "HoldJudgmentGraphic={}\n",
        profile.hold_judgment_graphic
    ));
    content.push_str(&format!(
        "JudgmentGraphic={}\n",
        profile.judgment_graphic
    ));
    content.push_str(&format!(
        "ComboFont={}\n",
        profile.combo_font
    ));
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
            profile.hold_judgment_graphic = profile_conf
                .get("PlayerOptions", "HoldJudgmentGraphic")
                .and_then(|s| HoldJudgmentGraphic::from_str(&s).ok())
                .unwrap_or(default_profile.hold_judgment_graphic);
            profile.judgment_graphic = profile_conf
                .get("PlayerOptions", "JudgmentGraphic")
                .and_then(|s| JudgmentGraphic::from_str(&s).ok())
                .unwrap_or(default_profile.judgment_graphic);
            profile.combo_font = profile_conf
                .get("PlayerOptions", "ComboFont")
                .and_then(|s| ComboFont::from_str(&s).ok())
                .unwrap_or(default_profile.combo_font);
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

// --- Session helpers ---
pub fn get_session_music_rate() -> f32 {
    let s = SESSION.lock().unwrap();
    let r = s.music_rate;
    if r.is_finite() && r > 0.0 { r } else { 1.0 }
}

pub fn set_session_music_rate(rate: f32) {
    let mut s = SESSION.lock().unwrap();
    s.music_rate = if rate.is_finite() && rate > 0.0 { rate.clamp(0.5, 3.0) } else { 1.0 };
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

pub fn update_hold_judgment_graphic(setting: HoldJudgmentGraphic) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.hold_judgment_graphic == setting {
            return;
        }
        profile.hold_judgment_graphic = setting;
    }
    save_profile_ini();
}

pub fn update_judgment_graphic(setting: JudgmentGraphic) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.judgment_graphic == setting {
            return;
        }
        profile.judgment_graphic = setting;
    }
    save_profile_ini();
}

pub fn update_combo_font(setting: ComboFont) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.combo_font == setting {
            return;
        }
        profile.combo_font = setting;
    }
    save_profile_ini();
}
