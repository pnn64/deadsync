pub use super::scroll::ScrollSpeedSetting;
use crate::config::SimpleIni;
use log::{info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Perspective {
    #[default]
    Overhead,
    Hallway,
    Distant,
    Incoming,
    Space,
}

impl Perspective {
    #[inline(always)]
    pub const fn tilt_skew(self) -> (f32, f32) {
        match self {
            Self::Overhead => (0.0, 0.0),
            Self::Hallway => (-1.0, 0.0),
            Self::Distant => (1.0, 0.0),
            Self::Incoming => (-1.0, 1.0),
            Self::Space => (1.0, 1.0),
        }
    }
}

impl FromStr for Perspective {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim().to_lowercase();
        match v.as_str() {
            "overhead" => Ok(Self::Overhead),
            "hallway" => Ok(Self::Hallway),
            "distant" => Ok(Self::Distant),
            "incoming" => Ok(Self::Incoming),
            "space" => Ok(Self::Space),
            other => Err(format!("'{other}' is not a valid Perspective setting")),
        }
    }
}

impl core::fmt::Display for Perspective {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Overhead => write!(f, "Overhead"),
            Self::Hallway => write!(f, "Hallway"),
            Self::Distant => write!(f, "Distant"),
            Self::Incoming => write!(f, "Incoming"),
            Self::Space => write!(f, "Space"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnOption {
    #[default]
    None,
    Mirror,
    Left,
    Right,
    LRMirror,
    UDMirror,
    Shuffle,
    Blender,
    Random,
}

impl FromStr for TurnOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" | "noturn" | "noturning" | "noturns" => Ok(Self::None),
            "mirror" => Ok(Self::Mirror),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "lrmirror" => Ok(Self::LRMirror),
            "udmirror" => Ok(Self::UDMirror),
            "shuffle" => Ok(Self::Shuffle),
            "blender" | "supershuffle" => Ok(Self::Blender),
            "random" | "hypershuffle" => Ok(Self::Random),
            other => Err(format!("'{other}' is not a valid Turn setting")),
        }
    }
}

impl core::fmt::Display for TurnOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Mirror => write!(f, "Mirror"),
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::LRMirror => write!(f, "LRMirror"),
            Self::UDMirror => write!(f, "UDMirror"),
            Self::Shuffle => write!(f, "Shuffle"),
            Self::Blender => write!(f, "Blender"),
            Self::Random => write!(f, "Random"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollOption(u8);

#[allow(non_upper_case_globals)]
impl ScrollOption {
    pub const Normal: Self = Self(0);
    pub const Reverse: Self = Self(1 << 0);
    pub const Split: Self = Self(1 << 1);
    pub const Alternate: Self = Self(1 << 2);
    pub const Cross: Self = Self(1 << 3);
    pub const Centered: Self = Self(1 << 4);

    #[inline(always)]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline(always)]
    pub const fn contains(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }

    #[inline(always)]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline(always)]
    pub const fn is_normal(self) -> bool {
        self.0 == 0
    }
}

impl Default for ScrollOption {
    fn default() -> Self {
        Self::Normal
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
        let mut result = Self::empty();
        for token in lower.split(|c: char| c == '+' || c == ',' || c.is_whitespace()) {
            if token.is_empty() {
                continue;
            }
            let flag = match token {
                "normal" => Self::Normal,
                "reverse" => Self::Reverse,
                "split" => Self::Split,
                "alternate" => Self::Alternate,
                "cross" => Self::Cross,
                "centered" => Self::Centered,
                other => {
                    return Err(format!("'{other}' is not a valid Scroll setting"));
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
            write!(f, "{name}")
        };

        write_flag("Reverse", self.contains(Self::Reverse), f)?;
        write_flag("Split", self.contains(Self::Split), f)?;
        write_flag("Alternate", self.contains(Self::Alternate), f)?;
        write_flag("Cross", self.contains(Self::Cross), f)?;
        write_flag("Centered", self.contains(Self::Centered), f)
    }
}

// --- Profile Data ---
const PROFILES_ROOT: &str = "save/profiles";
const DEFAULT_PROFILE_ID: &str = "00000000";

#[inline(always)]
fn local_profile_dir(id: &str) -> PathBuf {
    PathBuf::from(PROFILES_ROOT).join(id)
}

#[inline(always)]
fn profile_ini_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("profile.ini")
}

#[inline(always)]
fn groovestats_ini_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("groovestats.ini")
}

#[inline(always)]
fn profile_avatar_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("profile.png")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundFilter {
    Off,
    Dark,
    Darker,
    #[default]
    Darkest,
}

impl FromStr for BackgroundFilter {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "dark" => Ok(Self::Dark),
            "darker" => Ok(Self::Darker),
            "darkest" => Ok(Self::Darkest),
            _ => Err(format!("'{s}' is not a valid BackgroundFilter setting")),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HoldJudgmentGraphic {
    #[default]
    Love,
    Mute,
    ITG2,
    None,
}

impl FromStr for HoldJudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "love" => Ok(Self::Love),
            "mute" => Ok(Self::Mute),
            "itg2" => Ok(Self::ITG2),
            "none" => Ok(Self::None),
            other => Err(format!(
                "'{other}' is not a valid HoldJudgmentGraphic setting"
            )),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    #[default]
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
            other => Err(format!("'{other}' is not a valid JudgmentGraphic setting")),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteSkin {
    #[default]
    Cel,
    Metal,
    EnchantmentV2,
    DevCel2024V3,
}

impl FromStr for NoteSkin {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "cel" => Ok(Self::Cel),
            "metal" => Ok(Self::Metal),
            "enchantment-v2" => Ok(Self::EnchantmentV2),
            "devcel-2024-v3" => Ok(Self::DevCel2024V3),
            other => Err(format!("'{other}' is not a valid NoteSkin setting")),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboFont {
    #[default]
    Wendy,
    ArialRounded,
    Asap,
    BebasNeue,
    SourceCode,
    Work,
    WendyCursed,
    None,
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
            other => Err(format!("'{other}' is not a valid ComboFont setting")),
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
    pub turn_option: TurnOption,
    // FA+ visual options (Simply Love semantics).
    // These do not change core timing semantics; they only affect HUD/UX.
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_fa_plus_pane: bool,
    // Mini modifier as a percentage, mirroring Simply Love semantics.
    // 0 = normal size, 100 = 100% Mini (smaller), negative values enlarge.
    pub mini_percent: i32,
    pub perspective: Perspective,
    // NoteField positional offsets (Simply Love semantics).
    // X is non-negative and interpreted relative to player side:
    // for P1, positive values move the field left.
    pub note_field_offset_x: i32,
    // Y is applied directly to the notefield and related HUD,
    // positive values move everything down.
    pub note_field_offset_y: i32,
    // Per-player visual delay (Simply Love semantics). Stored in milliseconds.
    // Negative values shift arrows upwards; positive values shift them down.
    pub visual_delay_ms: i32,
    // Persisted "last played" selection so that SelectMusic can
    // reopen on the last song+difficulty the player actually played.
    // Stored as a serialized music file path and a raw difficulty index.
    pub last_song_music_path: Option<String>,
    pub last_chart_hash: Option<String>,
    pub last_difficulty_index: usize,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            display_name: "Player 1".to_string(),
            player_initials: "P1".to_string(),
            groovestats_api_key: String::new(),
            groovestats_is_pad_player: false,
            groovestats_username: String::new(),
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
            turn_option: TurnOption::default(),
            show_fa_plus_window: false,
            show_ex_score: false,
            show_fa_plus_pane: false,
            mini_percent: 0,
            perspective: Perspective::default(),
            note_field_offset_x: 0,
            note_field_offset_y: 0,
            visual_delay_ms: 0,
            last_song_music_path: None,
            last_chart_hash: None,
            // Mirror FILE_DIFFICULTY_NAMES[2] ("Medium") as the default.
            last_difficulty_index: 2,
        }
    }
}

const PLAYER_SLOTS: usize = 2;

#[inline(always)]
const fn side_ix(side: PlayerSide) -> usize {
    match side {
        PlayerSide::P1 => 0,
        PlayerSide::P2 => 1,
    }
}

// Global statics for the loaded player profiles.
static PROFILES: std::sync::LazyLock<Mutex<[Profile; PLAYER_SLOTS]>> =
    std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| Profile::default())));

// --- Session-scoped state (not persisted) ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveProfile {
    Guest,
    Local { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl PlayStyle {
    pub const fn chart_type(self) -> &'static str {
        match self {
            Self::Single | Self::Versus => "dance-single",
            Self::Double => "dance-double",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    Regular,
    Marathon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerSide {
    #[default]
    P1,
    P2,
}

pub const GUEST_SCROLL_SPEED: ScrollSpeedSetting = ScrollSpeedSetting::MMod(250.0);

const SESSION_JOINED_MASK_P1: u8 = 1 << 0;
const SESSION_JOINED_MASK_P2: u8 = 1 << 1;

#[inline(always)]
const fn side_joined_mask(side: PlayerSide) -> u8 {
    match side {
        PlayerSide::P1 => SESSION_JOINED_MASK_P1,
        PlayerSide::P2 => SESSION_JOINED_MASK_P2,
    }
}

#[derive(Debug)]
struct SessionState {
    active_profiles: [ActiveProfile; PLAYER_SLOTS],
    joined_mask: u8,
    music_rate: f32,
    play_style: PlayStyle,
    play_mode: PlayMode,
    player_side: PlayerSide,
}

static SESSION: std::sync::LazyLock<Mutex<SessionState>> = std::sync::LazyLock::new(|| {
    Mutex::new(SessionState {
        active_profiles: [
            ActiveProfile::Local {
                id: DEFAULT_PROFILE_ID.to_string(),
            },
            ActiveProfile::Guest,
        ],
        joined_mask: SESSION_JOINED_MASK_P1,
        music_rate: 1.0,
        play_style: PlayStyle::Single,
        play_mode: PlayMode::Regular,
        player_side: PlayerSide::P1,
    })
});

#[inline(always)]
fn session_side_is_guest(side: PlayerSide) -> bool {
    matches!(
        &SESSION.lock().unwrap().active_profiles[side_ix(side)],
        ActiveProfile::Guest
    )
}

fn make_guest_profile() -> Profile {
    let mut guest = Profile::default();
    guest.display_name = "[ GUEST ]".to_string();
    guest.scroll_speed = GUEST_SCROLL_SPEED;
    guest.avatar_path = None;
    guest.avatar_texture_key = None;
    guest
}

fn ensure_local_profile_files(id: &str) -> Result<(), std::io::Error> {
    let dir = local_profile_dir(id);
    let profile_ini = profile_ini_path(id);
    let groovestats_ini = groovestats_ini_path(id);

    info!(
        "Profile files not found, creating defaults in '{}'.",
        dir.display()
    );
    fs::create_dir_all(&dir)?;

    // Create profile.ini
    if !profile_ini.exists() {
        let default_profile = Profile::default();
        let mut content = String::new();

        content.push_str("[PlayerOptions]\n");
        content.push_str(&format!(
            "BackgroundFilter = {}\n",
            default_profile.background_filter
        ));
        content.push_str(&format!("ScrollSpeed = {}\n", default_profile.scroll_speed));
        content.push_str(&format!("Scroll = {}\n", default_profile.scroll_option));
        content.push_str(&format!("Turn = {}\n", default_profile.turn_option));
        content.push_str(&format!(
            "ReverseScroll = {}\n",
            i32::from(default_profile.reverse_scroll)
        ));
        content.push_str(&format!(
            "ShowFaPlusWindow = {}\n",
            i32::from(default_profile.show_fa_plus_window)
        ));
        content.push_str(&format!(
            "ShowExScore = {}\n",
            i32::from(default_profile.show_ex_score)
        ));
        content.push_str(&format!(
            "ShowFaPlusPane = {}\n",
            i32::from(default_profile.show_fa_plus_pane)
        ));
        content.push_str(&format!(
            "HoldJudgmentGraphic = {}\n",
            default_profile.hold_judgment_graphic
        ));
        content.push_str(&format!(
            "JudgmentGraphic = {}\n",
            default_profile.judgment_graphic
        ));
        content.push_str(&format!("ComboFont = {}\n", default_profile.combo_font));
        content.push_str(&format!("NoteSkin = {}\n", default_profile.noteskin));
        content.push_str(&format!("MiniPercent = {}\n", default_profile.mini_percent));
        content.push_str(&format!("Perspective = {}\n", default_profile.perspective));
        content.push_str(&format!(
            "NoteFieldOffsetX = {}\n",
            default_profile.note_field_offset_x
        ));
        content.push_str(&format!(
            "NoteFieldOffsetY = {}\n",
            default_profile.note_field_offset_y
        ));
        content.push_str(&format!(
            "VisualDelayMs = {}\n",
            default_profile.visual_delay_ms
        ));
        content.push('\n');

        content.push_str("[userprofile]\n");
        content.push_str(&format!("DisplayName = {}\n", default_profile.display_name));
        content.push_str(&format!(
            "PlayerInitials = {}\n",
            default_profile.player_initials
        ));
        content.push('\n');

        fs::write(profile_ini, content)?;
    }

    // Create groovestats.ini
    if !groovestats_ini.exists() {
        let mut content = String::new();

        content.push_str("[GrooveStats]\n");
        content.push_str("ApiKey = \n");
        content.push_str("IsPadPlayer = 0\n");
        content.push_str("Username = \n");
        content.push('\n');

        fs::write(groovestats_ini, content)?;
    }

    Ok(())
}

fn save_profile_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = SESSION.lock().unwrap();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let profile = PROFILES.lock().unwrap()[side_ix(side)].clone();
    let mut content = String::new();

    content.push_str("[PlayerOptions]\n");
    content.push_str(&format!("BackgroundFilter={}\n", profile.background_filter));
    content.push_str(&format!("ScrollSpeed={}\n", profile.scroll_speed));
    content.push_str(&format!("Scroll={}\n", profile.scroll_option));
    content.push_str(&format!("Turn={}\n", profile.turn_option));
    content.push_str(&format!(
        "ReverseScroll={}\n",
        i32::from(profile.reverse_scroll)
    ));
    content.push_str(&format!(
        "ShowFaPlusWindow={}\n",
        i32::from(profile.show_fa_plus_window)
    ));
    content.push_str(&format!(
        "ShowExScore={}\n",
        i32::from(profile.show_ex_score)
    ));
    content.push_str(&format!(
        "ShowFaPlusPane={}\n",
        i32::from(profile.show_fa_plus_pane)
    ));
    content.push_str(&format!(
        "HoldJudgmentGraphic={}\n",
        profile.hold_judgment_graphic
    ));
    content.push_str(&format!("JudgmentGraphic={}\n", profile.judgment_graphic));
    content.push_str(&format!("ComboFont={}\n", profile.combo_font));
    content.push_str(&format!("NoteSkin={}\n", profile.noteskin));
    content.push_str(&format!("MiniPercent={}\n", profile.mini_percent));
    content.push_str(&format!("Perspective={}\n", profile.perspective));
    content.push_str(&format!(
        "NoteFieldOffsetX={}\n",
        profile.note_field_offset_x
    ));
    content.push_str(&format!(
        "NoteFieldOffsetY={}\n",
        profile.note_field_offset_y
    ));
    content.push_str(&format!("VisualDelayMs={}\n", profile.visual_delay_ms));
    content.push('\n');

    content.push_str("[userprofile]\n");
    content.push_str(&format!("DisplayName={}\n", profile.display_name));
    content.push_str(&format!("PlayerInitials={}\n", profile.player_initials));
    content.push('\n');

    // Persist "last played" song + difficulty so that future sessions
    // can reopen SelectMusic on the most recently played chart.
    content.push_str("[LastPlayed]\n");
    if let Some(path) = &profile.last_song_music_path {
        content.push_str(&format!("MusicPath={path}\n"));
    } else {
        content.push_str("MusicPath=\n");
    }
    if let Some(hash) = &profile.last_chart_hash {
        content.push_str(&format!("ChartHash={hash}\n"));
    } else {
        content.push_str("ChartHash=\n");
    }
    content.push_str(&format!(
        "DifficultyIndex={}\n",
        profile.last_difficulty_index
    ));
    content.push('\n');

    let path = profile_ini_path(&profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

fn save_groovestats_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = SESSION.lock().unwrap();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let profile = PROFILES.lock().unwrap()[side_ix(side)].clone();
    let mut content = String::new();

    content.push_str("[GrooveStats]\n");
    content.push_str(&format!("ApiKey={}\n", profile.groovestats_api_key));
    content.push_str(&format!(
        "IsPadPlayer={}\n",
        if profile.groovestats_is_pad_player {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("Username={}\n", profile.groovestats_username));
    content.push('\n');

    let path = groovestats_ini_path(&profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

fn load_for_side(side: PlayerSide) {
    let profile_id = {
        let session = SESSION.lock().unwrap();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };

    let Some(profile_id) = profile_id else {
        let mut profiles = PROFILES.lock().unwrap();
        profiles[side_ix(side)] = make_guest_profile();
        return;
    };

    let profile_ini = profile_ini_path(&profile_id);
    let groovestats_ini = groovestats_ini_path(&profile_id);
    if (!profile_ini.exists() || !groovestats_ini.exists())
        && let Err(e) = ensure_local_profile_files(&profile_id)
    {
        warn!("Failed to create default profile files: {e}");
        // Proceed with default struct values and attempt to save them.
    }

    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        let default_profile = Profile::default();

        // Load profile.ini
        let mut profile_conf = SimpleIni::new();
        if profile_conf.load(&profile_ini).is_ok() {
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
            profile.perspective = profile_conf
                .get("PlayerOptions", "Perspective")
                .and_then(|s| Perspective::from_str(&s).ok())
                .unwrap_or(default_profile.perspective);
            profile.note_field_offset_x = profile_conf
                .get("PlayerOptions", "NoteFieldOffsetX")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(default_profile.note_field_offset_x);
            profile.note_field_offset_y = profile_conf
                .get("PlayerOptions", "NoteFieldOffsetY")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(default_profile.note_field_offset_y);
            profile.visual_delay_ms = profile_conf
                .get("PlayerOptions", "VisualDelayMs")
                .or_else(|| profile_conf.get("PlayerOptions", "VisualDelay"))
                .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
                .unwrap_or(default_profile.visual_delay_ms);
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
            profile.turn_option = profile_conf
                .get("PlayerOptions", "Turn")
                .and_then(|s| TurnOption::from_str(&s).ok())
                .unwrap_or(default_profile.turn_option);
            profile.scroll_option = profile_conf
                .get("PlayerOptions", "Scroll")
                .and_then(|s| ScrollOption::from_str(&s).ok())
                .unwrap_or_else(|| {
                    let reverse_enabled = profile_conf
                        .get("PlayerOptions", "ReverseScroll")
                        .and_then(|v| v.parse::<u8>().ok())
                        .map_or(default_profile.reverse_scroll, |v| v != 0);
                    if reverse_enabled {
                        ScrollOption::Reverse
                    } else {
                        default_profile.scroll_option
                    }
                });
            profile.reverse_scroll = profile.scroll_option.contains(ScrollOption::Reverse);

            // Optional last-played section: if missing, fall back to defaults.
            profile.last_song_music_path =
                profile_conf.get("LastPlayed", "MusicPath").and_then(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                });

            profile.last_chart_hash = profile_conf.get("LastPlayed", "ChartHash").and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

            let raw_last_diff = profile_conf
                .get("LastPlayed", "DifficultyIndex")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(default_profile.last_difficulty_index);
            // Do not assume any particular max here; clamp later at call sites.
            profile.last_difficulty_index = raw_last_diff;
        } else {
            warn!(
                "Failed to load '{}', using default profile settings.",
                profile_ini.display()
            );
        }

        // Load groovestats.ini
        let mut gs_conf = SimpleIni::new();
        if gs_conf.load(&groovestats_ini).is_ok() {
            profile.groovestats_api_key = gs_conf
                .get("GrooveStats", "ApiKey")
                .unwrap_or(default_profile.groovestats_api_key.clone());
            profile.groovestats_is_pad_player = gs_conf
                .get("GrooveStats", "IsPadPlayer")
                .and_then(|v| v.parse::<u8>().ok())
                .map_or(default_profile.groovestats_is_pad_player, |v| v != 0);
            profile.groovestats_username = gs_conf
                .get("GrooveStats", "Username")
                .unwrap_or(default_profile.groovestats_username);
        } else {
            warn!(
                "Failed to load '{}', using default GrooveStats info.",
                groovestats_ini.display()
            );
        }

        let avatar_path = profile_avatar_path(&profile_id);
        profile.avatar_path = if avatar_path.exists() {
            Some(avatar_path)
        } else {
            None
        };
        profile.avatar_texture_key = None;
    } // Lock is released here.

    save_profile_ini_for_side(side);
    save_groovestats_ini_for_side(side);
    info!("Profile configuration files updated with default values for any missing fields.");
}

pub fn load() {
    load_for_side(PlayerSide::P1);
    load_for_side(PlayerSide::P2);
}

/// Returns a copy of the currently loaded profile data.
pub fn get() -> Profile {
    get_for_side(get_session_player_side())
}

pub fn get_for_side(side: PlayerSide) -> Profile {
    PROFILES.lock().unwrap()[side_ix(side)].clone()
}

pub fn set_avatar_texture_key_for_side(side: PlayerSide, key: Option<String>) {
    let mut profiles = PROFILES.lock().unwrap();
    profiles[side_ix(side)].avatar_texture_key = key;
}

// --- Session helpers ---
pub fn get_active_profile() -> ActiveProfile {
    get_active_profile_for_side(get_session_player_side())
}

pub fn get_active_profile_for_side(side: PlayerSide) -> ActiveProfile {
    SESSION.lock().unwrap().active_profiles[side_ix(side)].clone()
}

pub fn active_local_profile_id() -> Option<String> {
    active_local_profile_id_for_side(get_session_player_side())
}

pub fn active_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    let session = SESSION.lock().unwrap();
    match &session.active_profiles[side_ix(side)] {
        ActiveProfile::Local { id } => Some(id.clone()),
        ActiveProfile::Guest => None,
    }
}

pub fn set_active_profile_for_side(side: PlayerSide, profile: ActiveProfile) -> Profile {
    {
        let mut session = SESSION.lock().unwrap();
        let slot = &mut session.active_profiles[side_ix(side)];
        if *slot == profile {
            return get_for_side(side);
        }
        *slot = profile;
    }
    load_for_side(side);
    get_for_side(side)
}

pub fn set_active_profiles(p1: ActiveProfile, p2: ActiveProfile) -> [Profile; PLAYER_SLOTS] {
    let _ = set_active_profile_for_side(PlayerSide::P1, p1);
    let _ = set_active_profile_for_side(PlayerSide::P2, p2);
    [get_for_side(PlayerSide::P1), get_for_side(PlayerSide::P2)]
}

pub struct LocalProfileSummary {
    pub id: String,
    pub display_name: String,
    pub avatar_path: Option<PathBuf>,
}

pub fn scan_local_profiles() -> Vec<LocalProfileSummary> {
    fn is_profile_id(s: &str) -> bool {
        s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }

    let root = Path::new(PROFILES_ROOT);
    let Ok(read_dir) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let Some(id) = entry
            .file_name()
            .to_str()
            .map(std::string::ToString::to_string)
        else {
            continue;
        };
        if !is_profile_id(&id) {
            continue;
        }

        let ini_path = entry.path().join("profile.ini");
        if !ini_path.is_file() {
            continue;
        }

        let mut display_name = id.clone();
        let mut ini = SimpleIni::new();
        if ini.load(&ini_path).is_ok()
            && let Some(name) = ini.get("userprofile", "DisplayName")
            && !name.trim().is_empty()
        {
            display_name = name;
        }

        let avatar_path = entry.path().join("profile.png");
        let avatar_path = avatar_path.is_file().then_some(avatar_path);

        out.push(LocalProfileSummary {
            id,
            display_name,
            avatar_path,
        });
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn get_session_music_rate() -> f32 {
    let s = SESSION.lock().unwrap();
    let r = s.music_rate;
    if r.is_finite() && r > 0.0 { r } else { 1.0 }
}

pub fn set_session_music_rate(rate: f32) {
    let mut s = SESSION.lock().unwrap();
    s.music_rate = if rate.is_finite() && rate > 0.0 {
        rate.clamp(0.5, 3.0)
    } else {
        1.0
    };
}

pub fn get_session_play_style() -> PlayStyle {
    SESSION.lock().unwrap().play_style
}

pub fn set_session_play_style(style: PlayStyle) {
    SESSION.lock().unwrap().play_style = style;
}

pub fn get_session_play_mode() -> PlayMode {
    SESSION.lock().unwrap().play_mode
}

pub fn set_session_play_mode(mode: PlayMode) {
    SESSION.lock().unwrap().play_mode = mode;
}

pub fn get_session_player_side() -> PlayerSide {
    SESSION.lock().unwrap().player_side
}

pub fn set_session_player_side(side: PlayerSide) {
    SESSION.lock().unwrap().player_side = side;
}

pub fn is_session_side_joined(side: PlayerSide) -> bool {
    let mask = SESSION.lock().unwrap().joined_mask;
    mask & side_joined_mask(side) != 0
}

pub fn is_session_side_guest(side: PlayerSide) -> bool {
    session_side_is_guest(side)
}

pub fn set_session_joined(p1: bool, p2: bool) {
    let mask = (u8::from(p1) * SESSION_JOINED_MASK_P1) | (u8::from(p2) * SESSION_JOINED_MASK_P2);
    SESSION.lock().unwrap().joined_mask = mask;
}

pub fn update_last_played_for_side(
    side: PlayerSide,
    music_path: Option<&Path>,
    chart_hash: Option<&str>,
    difficulty_index: usize,
) {
    if session_side_is_guest(side) {
        return;
    }
    let new_path = music_path.map(|p| p.to_string_lossy().into_owned());
    let new_hash = chart_hash.map(str::to_string);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        let mut changed = false;
        if profile.last_song_music_path != new_path {
            profile.last_song_music_path = new_path;
            changed = true;
        }
        if profile.last_chart_hash != new_hash {
            profile.last_chart_hash = new_hash;
            changed = true;
        }
        if profile.last_difficulty_index != difficulty_index {
            profile.last_difficulty_index = difficulty_index;
            changed = true;
        }
        if !changed {
            return;
        }
    }
    save_profile_ini_for_side(side);
}

pub fn update_scroll_speed_for_side(side: PlayerSide, setting: ScrollSpeedSetting) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.scroll_speed == setting {
            return;
        }
        profile.scroll_speed = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_background_filter_for_side(side: PlayerSide, setting: BackgroundFilter) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.background_filter == setting {
            return;
        }
        profile.background_filter = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_hold_judgment_graphic_for_side(side: PlayerSide, setting: HoldJudgmentGraphic) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.hold_judgment_graphic == setting {
            return;
        }
        profile.hold_judgment_graphic = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_judgment_graphic_for_side(side: PlayerSide, setting: JudgmentGraphic) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.judgment_graphic == setting {
            return;
        }
        profile.judgment_graphic = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_combo_font_for_side(side: PlayerSide, setting: ComboFont) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_font == setting {
            return;
        }
        profile.combo_font = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_scroll_option_for_side(side: PlayerSide, setting: ScrollOption) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        let reverse_enabled = setting.contains(ScrollOption::Reverse);
        if profile.scroll_option == setting && profile.reverse_scroll == reverse_enabled {
            return;
        }
        profile.scroll_option = setting;
        profile.reverse_scroll = reverse_enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_turn_option_for_side(side: PlayerSide, setting: TurnOption) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.turn_option == setting {
            return;
        }
        profile.turn_option = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_noteskin_for_side(side: PlayerSide, setting: NoteSkin) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.noteskin == setting {
            return;
        }
        profile.noteskin = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_notefield_offset_x_for_side(side: PlayerSide, offset: i32) {
    if session_side_is_guest(side) {
        return;
    }
    let clamped = offset.clamp(0, 50);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.note_field_offset_x == clamped {
            return;
        }
        profile.note_field_offset_x = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_notefield_offset_y_for_side(side: PlayerSide, offset: i32) {
    if session_side_is_guest(side) {
        return;
    }
    let clamped = offset.clamp(-50, 50);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.note_field_offset_y == clamped {
            return;
        }
        profile.note_field_offset_y = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_mini_percent_for_side(side: PlayerSide, percent: i32) {
    if session_side_is_guest(side) {
        return;
    }
    // Mirror Simply Love's range: -100% to +150%.
    let clamped = percent.clamp(-100, 150);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.mini_percent == clamped {
            return;
        }
        profile.mini_percent = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_perspective_for_side(side: PlayerSide, perspective: Perspective) {
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.perspective == perspective {
            return;
        }
        profile.perspective = perspective;
    }
    save_profile_ini_for_side(side);
}

pub fn update_visual_delay_ms_for_side(side: PlayerSide, ms: i32) {
    if session_side_is_guest(side) {
        return;
    }
    // Mirror Simply Love's range: -100ms to +100ms.
    let clamped = ms.clamp(-100, 100);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.visual_delay_ms == clamped {
            return;
        }
        profile.visual_delay_ms = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_fa_plus_window_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_fa_plus_window == enabled {
            return;
        }
        profile.show_fa_plus_window = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_ex_score_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_ex_score == enabled {
            return;
        }
        profile.show_ex_score = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_fa_plus_pane_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_fa_plus_pane == enabled {
            return;
        }
        profile.show_fa_plus_pane = enabled;
    }
    save_profile_ini_for_side(side);
}
