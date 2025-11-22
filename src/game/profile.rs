pub use super::scroll::ScrollSpeedSetting;
use configparser::ini::Ini;
use log::{info, warn};
use once_cell::sync::Lazy;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollOption(u8);

#[allow(non_upper_case_globals)]
impl ScrollOption {
    pub const Normal: ScrollOption = ScrollOption(0);
    pub const Reverse: ScrollOption = ScrollOption(1 << 0);
    pub const Split: ScrollOption = ScrollOption(1 << 1);
    pub const Alternate: ScrollOption = ScrollOption(1 << 2);
    pub const Cross: ScrollOption = ScrollOption(1 << 3);

    #[inline(always)]
    pub const fn empty() -> ScrollOption {
        ScrollOption(0)
    }

    #[inline(always)]
    pub const fn contains(self, flag: ScrollOption) -> bool {
        (self.0 & flag.0) != 0
    }

    #[inline(always)]
    pub const fn union(self, other: ScrollOption) -> ScrollOption {
        ScrollOption(self.0 | other.0)
    }

    #[inline(always)]
    pub const fn is_normal(self) -> bool {
        self.0 == 0
    }
}

impl Default for ScrollOption {
    fn default() -> Self {
        ScrollOption::Normal
    }
}

impl FromStr for ScrollOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();
        if raw.is_empty() {
            return Err("Scroll setting is empty".to_string());
        }
        let lower = raw.to_lowercase();
        // Support both legacy single values ("Reverse") and combined values
        // like "Reverse+Cross" or "Reverse Cross".
        let mut result = ScrollOption::empty();
        for token in lower
            .split(|c: char| c == '+' || c == ',' || c.is_whitespace())
        {
            if token.is_empty() {
                continue;
            }
            let flag = match token {
                "normal" => ScrollOption::Normal,
                "reverse" => ScrollOption::Reverse,
                "split" => ScrollOption::Split,
                "alternate" => ScrollOption::Alternate,
                "cross" => ScrollOption::Cross,
                other => {
                    return Err(format!("'{}' is not a valid Scroll setting", other));
                }
            };
            // "Normal" means no flags; combining it with others is treated as just the others.
            if flag.0 != 0 {
                result = result.union(flag);
            }
        }
        Ok(result)
    }
}

impl core::fmt::Display for ScrollOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_normal() {
            return write!(f, "Normal");
        }

        let mut first = true;
        let mut write_flag = |name: &str, present: bool, f: &mut core::fmt::Formatter<'_>| {
            if !present {
                return Ok(());
            }
            if !first {
                write!(f, "+")?;
            }
            first = false;
            write!(f, "{}", name)
        };

        write_flag("Reverse", self.contains(ScrollOption::Reverse), f)?;
        write_flag("Split", self.contains(ScrollOption::Split), f)?;
        write_flag("Alternate", self.contains(ScrollOption::Alternate), f)?;
        write_flag("Cross", self.contains(ScrollOption::Cross), f)
    }
}

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
pub enum NoteSkin {
    Cel,
    Metal,
    EnchantmentV2,
    DevCel2024V3,
}

impl Default for NoteSkin {
    fn default() -> Self {
        NoteSkin::Cel
    }
}

impl FromStr for NoteSkin {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "cel" => Ok(Self::Cel),
            "metal" => Ok(Self::Metal),
            "enchantment-v2" => Ok(Self::EnchantmentV2),
            "devcel-2024-v3" => Ok(Self::DevCel2024V3),
            other => Err(format!("'{}' is not a valid NoteSkin setting", other)),
        }
    }
}

impl core::fmt::Display for NoteSkin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Cel => write!(f, "cel"),
            Self::Metal => write!(f, "metal"),
            Self::EnchantmentV2 => write!(f, "enchantment-v2"),
            Self::DevCel2024V3 => write!(f, "devcel-2024-v3"),
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
    pub noteskin: NoteSkin,
    pub avatar_path: Option<PathBuf>,
    pub avatar_texture_key: Option<String>,
    pub scroll_speed: ScrollSpeedSetting,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    // FA+ visual options (Simply Love semantics).
    // These do not change core timing semantics; they only affect HUD/UX.
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_fa_plus_pane: bool,
    // Mini modifier as a percentage, mirroring Simply Love semantics.
    // 0 = normal size, 100 = 100% Mini (smaller), negative values enlarge.
    pub mini_percent: i32,
    // NoteField positional offsets (Simply Love semantics).
    // X is non-negative and interpreted relative to player side:
    // for P1, positive values move the field left.
    pub note_field_offset_x: i32,
    // Y is applied directly to the notefield and related HUD,
    // positive values move everything down.
    pub note_field_offset_y: i32,
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
            noteskin: NoteSkin::default(),
            avatar_path: None,
            avatar_texture_key: None,
            scroll_speed: ScrollSpeedSetting::default(),
            scroll_option: ScrollOption::default(),
            reverse_scroll: false,
            show_fa_plus_window: false,
            show_ex_score: false,
            show_fa_plus_pane: false,
            mini_percent: 0,
            note_field_offset_x: 0,
            note_field_offset_y: 0,
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
        content.push_str(&format!("Scroll = {}\n", default_profile.scroll_option));
        content.push_str(&format!(
            "ReverseScroll = {}\n",
            if default_profile.reverse_scroll { 1 } else { 0 }
        ));
        content.push_str(&format!(
            "ShowFaPlusWindow = {}\n",
            if default_profile.show_fa_plus_window { 1 } else { 0 }
        ));
        content.push_str(&format!(
            "ShowExScore = {}\n",
            if default_profile.show_ex_score { 1 } else { 0 }
        ));
        content.push_str(&format!(
            "ShowFaPlusPane = {}\n",
            if default_profile.show_fa_plus_pane { 1 } else { 0 }
        ));
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
        content.push_str(&format!(
            "NoteSkin = {}\n",
            default_profile.noteskin
        ));
        content.push_str(&format!(
            "MiniPercent = {}\n",
            default_profile.mini_percent
        ));
        content.push_str(&format!(
            "NoteFieldOffsetX = {}\n",
            default_profile.note_field_offset_x
        ));
        content.push_str(&format!(
            "NoteFieldOffsetY = {}\n",
            default_profile.note_field_offset_y
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
    content.push_str(&format!("Scroll={}\n", profile.scroll_option));
    content.push_str(&format!(
        "ReverseScroll={}\n",
        if profile.reverse_scroll { 1 } else { 0 }
    ));
    content.push_str(&format!(
        "ShowFaPlusWindow={}\n",
        if profile.show_fa_plus_window { 1 } else { 0 }
    ));
    content.push_str(&format!(
        "ShowExScore={}\n",
        if profile.show_ex_score { 1 } else { 0 }
    ));
    content.push_str(&format!(
        "ShowFaPlusPane={}\n",
        if profile.show_fa_plus_pane { 1 } else { 0 }
    ));
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
    content.push_str(&format!("NoteSkin={}\n", profile.noteskin));
    content.push_str(&format!(
        "MiniPercent={}\n",
        profile.mini_percent
    ));
    content.push_str(&format!(
        "NoteFieldOffsetX={}\n",
        profile.note_field_offset_x
    ));
    content.push_str(&format!(
        "NoteFieldOffsetY={}\n",
        profile.note_field_offset_y
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
            profile.noteskin = profile_conf
                .get("PlayerOptions", "NoteSkin")
                .and_then(|s| NoteSkin::from_str(&s).ok())
                .unwrap_or(default_profile.noteskin);
            profile.mini_percent = profile_conf
                .get("PlayerOptions", "MiniPercent")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(default_profile.mini_percent);
            profile.note_field_offset_x = profile_conf
                .get("PlayerOptions", "NoteFieldOffsetX")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(default_profile.note_field_offset_x);
            profile.note_field_offset_y = profile_conf
                .get("PlayerOptions", "NoteFieldOffsetY")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(default_profile.note_field_offset_y);
            profile.show_fa_plus_window = profile_conf
                .get("PlayerOptions", "ShowFaPlusWindow")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.show_fa_plus_window, |v| v != 0);
            profile.show_ex_score = profile_conf
                .get("PlayerOptions", "ShowExScore")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.show_ex_score, |v| v != 0);
            profile.show_fa_plus_pane = profile_conf
                .get("PlayerOptions", "ShowFaPlusPane")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.show_fa_plus_pane, |v| v != 0);
            profile.scroll_speed = profile_conf
                .get("PlayerOptions", "ScrollSpeed")
                .and_then(|s| ScrollSpeedSetting::from_str(&s).ok())
                .unwrap_or(default_profile.scroll_speed);
            profile.scroll_option = profile_conf
                .get("PlayerOptions", "Scroll")
                .and_then(|s| ScrollOption::from_str(&s).ok())
                .unwrap_or_else(|| {
                    let reverse_enabled = profile_conf
                        .get("PlayerOptions", "ReverseScroll")
                        .and_then(|v| v.parse::<u8>().ok())
                        .map_or(default_profile.reverse_scroll, |v| v != 0);
                    if reverse_enabled { ScrollOption::Reverse } else { default_profile.scroll_option }
                });
            profile.reverse_scroll = profile.scroll_option.contains(ScrollOption::Reverse);
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

pub fn update_scroll_option(setting: ScrollOption) {
    {
        let mut profile = PROFILE.lock().unwrap();
        let reverse_enabled = setting.contains(ScrollOption::Reverse);
        if profile.scroll_option == setting && profile.reverse_scroll == reverse_enabled {
            return;
        }
        profile.scroll_option = setting;
        profile.reverse_scroll = reverse_enabled;
    }
    save_profile_ini();
}

pub fn update_noteskin(setting: NoteSkin) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.noteskin == setting {
            return;
        }
        profile.noteskin = setting;
    }
    save_profile_ini();
}

pub fn update_notefield_offset_x(offset: i32) {
    let clamped = offset.clamp(0, 50);
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.note_field_offset_x == clamped {
            return;
        }
        profile.note_field_offset_x = clamped;
    }
    save_profile_ini();
}

pub fn update_notefield_offset_y(offset: i32) {
    let clamped = offset.clamp(-50, 50);
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.note_field_offset_y == clamped {
            return;
        }
        profile.note_field_offset_y = clamped;
    }
    save_profile_ini();
}

pub fn update_mini_percent(percent: i32) {
    // Mirror Simply Love's range: -100% to +150%.
    let clamped = percent.clamp(-100, 150);
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.mini_percent == clamped {
            return;
        }
        profile.mini_percent = clamped;
    }
    save_profile_ini();
}

pub fn update_show_fa_plus_window(enabled: bool) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.show_fa_plus_window == enabled {
            return;
        }
        profile.show_fa_plus_window = enabled;
    }
    save_profile_ini();
}

pub fn update_show_ex_score(enabled: bool) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.show_ex_score == enabled {
            return;
        }
        profile.show_ex_score = enabled;
    }
    save_profile_ini();
}

pub fn update_show_fa_plus_pane(enabled: bool) {
    {
        let mut profile = PROFILE.lock().unwrap();
        if profile.show_fa_plus_pane == enabled {
            return;
        }
        profile.show_fa_plus_pane = enabled;
    }
    save_profile_ini();
}
