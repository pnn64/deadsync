pub use super::scroll::ScrollSpeedSetting;
use crate::config::SimpleIni;
use bincode::{Decode, Encode};
use chrono::Local;
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
const PROFILE_STATS_VERSION_V1: u16 = 1;

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

#[inline(always)]
fn profile_stats_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("stats.bin")
}

#[inline(always)]
fn profile_stats_tmp_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("stats.bin.tmp")
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
struct ProfileStatsV1 {
    version: u16,
    current_combo: u32,
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
pub enum ErrorBarStyle {
    #[default]
    None,
    Colorful,
    Monochrome,
    Text,
    Highlight,
    Average,
}

impl FromStr for ErrorBarStyle {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "colorful" => Ok(Self::Colorful),
            "monochrome" => Ok(Self::Monochrome),
            "text" => Ok(Self::Text),
            "highlight" => Ok(Self::Highlight),
            "average" => Ok(Self::Average),
            other => Err(format!("'{other}' is not a valid ErrorBar setting")),
        }
    }
}

impl core::fmt::Display for ErrorBarStyle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Colorful => write!(f, "Colorful"),
            Self::Monochrome => write!(f, "Monochrome"),
            Self::Text => write!(f, "Text"),
            Self::Highlight => write!(f, "Highlight"),
            Self::Average => write!(f, "Average"),
        }
    }
}

pub const ERROR_BAR_BIT_COLORFUL: u8 = 1 << 0;
pub const ERROR_BAR_BIT_MONOCHROME: u8 = 1 << 1;
pub const ERROR_BAR_BIT_TEXT: u8 = 1 << 2;
pub const ERROR_BAR_BIT_HIGHLIGHT: u8 = 1 << 3;
pub const ERROR_BAR_BIT_AVERAGE: u8 = 1 << 4;
pub const ERROR_BAR_ACTIVE_BITS: u8 = ERROR_BAR_BIT_COLORFUL
    | ERROR_BAR_BIT_MONOCHROME
    | ERROR_BAR_BIT_TEXT
    | ERROR_BAR_BIT_HIGHLIGHT
    | ERROR_BAR_BIT_AVERAGE;

#[inline(always)]
pub const fn normalize_error_bar_mask(mask: u8) -> u8 {
    mask & ERROR_BAR_ACTIVE_BITS
}

#[inline(always)]
pub const fn error_bar_mask_from_style(style: ErrorBarStyle, text: bool) -> u8 {
    let mut mask = if text { ERROR_BAR_BIT_TEXT } else { 0 };
    mask |= match style {
        ErrorBarStyle::None => 0,
        ErrorBarStyle::Colorful => ERROR_BAR_BIT_COLORFUL,
        ErrorBarStyle::Monochrome => ERROR_BAR_BIT_MONOCHROME,
        ErrorBarStyle::Text => ERROR_BAR_BIT_TEXT,
        ErrorBarStyle::Highlight => ERROR_BAR_BIT_HIGHLIGHT,
        ErrorBarStyle::Average => ERROR_BAR_BIT_AVERAGE,
    };
    normalize_error_bar_mask(mask)
}

#[inline(always)]
pub const fn error_bar_style_from_mask(mask: u8) -> ErrorBarStyle {
    let mask = normalize_error_bar_mask(mask);
    if (mask & ERROR_BAR_BIT_COLORFUL) != 0 {
        ErrorBarStyle::Colorful
    } else if (mask & ERROR_BAR_BIT_MONOCHROME) != 0 {
        ErrorBarStyle::Monochrome
    } else if (mask & ERROR_BAR_BIT_HIGHLIGHT) != 0 {
        ErrorBarStyle::Highlight
    } else if (mask & ERROR_BAR_BIT_AVERAGE) != 0 {
        ErrorBarStyle::Average
    } else {
        ErrorBarStyle::None
    }
}

#[inline(always)]
pub const fn error_bar_text_from_mask(mask: u8) -> bool {
    (normalize_error_bar_mask(mask) & ERROR_BAR_BIT_TEXT) != 0
}

pub const CUSTOM_FANTASTIC_WINDOW_MIN_MS: u8 = 1;
pub const CUSTOM_FANTASTIC_WINDOW_MAX_MS: u8 = 22;
pub const CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS: u8 = 10;

#[inline(always)]
pub const fn clamp_custom_fantastic_window_ms(ms: u8) -> u8 {
    if ms < CUSTOM_FANTASTIC_WINDOW_MIN_MS {
        CUSTOM_FANTASTIC_WINDOW_MIN_MS
    } else if ms > CUSTOM_FANTASTIC_WINDOW_MAX_MS {
        CUSTOM_FANTASTIC_WINDOW_MAX_MS
    } else {
        ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorBarTrim {
    #[default]
    Off,
    Fantastic,
    Excellent,
    Great,
}

impl FromStr for ErrorBarTrim {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "fantastic" => Ok(Self::Fantastic),
            "excellent" => Ok(Self::Excellent),
            "great" => Ok(Self::Great),
            other => Err(format!("'{other}' is not a valid ErrorBarTrim setting")),
        }
    }
}

impl core::fmt::Display for ErrorBarTrim {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Fantastic => write!(f, "Fantastic"),
            Self::Excellent => write!(f, "Excellent"),
            Self::Great => write!(f, "Great"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeasureCounter {
    #[default]
    None,
    Eighth,
    Twelfth,
    Sixteenth,
    TwentyFourth,
    ThirtySecond,
}

impl MeasureCounter {
    #[inline(always)]
    pub const fn notes_threshold(self) -> Option<usize> {
        match self {
            Self::None => None,
            Self::Eighth => Some(8),
            Self::Twelfth => Some(12),
            Self::Sixteenth => Some(16),
            Self::TwentyFourth => Some(24),
            Self::ThirtySecond => Some(32),
        }
    }

    #[inline(always)]
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::TwentyFourth => 1.5,
            Self::ThirtySecond => 2.0,
            _ => 1.0,
        }
    }
}

impl FromStr for MeasureCounter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "8th" => Ok(Self::Eighth),
            "12th" => Ok(Self::Twelfth),
            "16th" => Ok(Self::Sixteenth),
            "24th" => Ok(Self::TwentyFourth),
            "32nd" => Ok(Self::ThirtySecond),
            other => Err(format!("'{other}' is not a valid MeasureCounter setting")),
        }
    }
}

impl core::fmt::Display for MeasureCounter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Eighth => write!(f, "8th"),
            Self::Twelfth => write!(f, "12th"),
            Self::Sixteenth => write!(f, "16th"),
            Self::TwentyFourth => write!(f, "24th"),
            Self::ThirtySecond => write!(f, "32nd"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeasureLines {
    #[default]
    Off,
    Measure,
    Quarter,
    Eighth,
}

impl FromStr for MeasureLines {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "measure" => Ok(Self::Measure),
            "quarter" => Ok(Self::Quarter),
            "eighth" => Ok(Self::Eighth),
            other => Err(format!("'{other}' is not a valid MeasureLines setting")),
        }
    }
}

impl core::fmt::Display for MeasureLines {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Measure => write!(f, "Measure"),
            Self::Quarter => write!(f, "Quarter"),
            Self::Eighth => write!(f, "Eighth"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataVisualizations {
    #[default]
    None,
    TargetScoreGraph,
    StepStatistics,
}

impl FromStr for DataVisualizations {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" => Ok(Self::None),
            "targetscoregraph" | "targetscore" | "target" => Ok(Self::TargetScoreGraph),
            "stepstatistics" | "stepstats" => Ok(Self::StepStatistics),
            other => Err(format!(
                "'{other}' is not a valid DataVisualizations setting"
            )),
        }
    }
}

impl core::fmt::Display for DataVisualizations {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::TargetScoreGraph => write!(f, "Target Score Graph"),
            Self::StepStatistics => write!(f, "Step Statistics"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifeMeterType {
    #[default]
    Standard,
    Surround,
    Vertical,
}

impl FromStr for LifeMeterType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "" | "standard" => Ok(Self::Standard),
            "surround" => Ok(Self::Surround),
            "vertical" => Ok(Self::Vertical),
            other => Err(format!("'{other}' is not a valid LifeMeterType setting")),
        }
    }
}

impl core::fmt::Display for LifeMeterType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Standard => write!(f, "Standard"),
            Self::Surround => write!(f, "Surround"),
            Self::Vertical => write!(f, "Vertical"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteSkin {
    raw: String,
}

impl NoteSkin {
    pub const DEFAULT_NAME: &'static str = "default";
    pub const CEL_NAME: &'static str = "cel";

    #[inline(always)]
    fn normalize(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let lower = trimmed.to_ascii_lowercase();
        Some(lower)
    }

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self::from_str(raw).unwrap_or_default()
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl Default for NoteSkin {
    fn default() -> Self {
        Self {
            raw: Self::CEL_NAME.to_string(),
        }
    }
}

impl FromStr for NoteSkin {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = Self::normalize(s)
            .ok_or_else(|| format!("'{}' is not a valid NoteSkin setting", s.trim()))?;
        Ok(Self { raw: normalized })
    }
}

impl core::fmt::Display for NoteSkin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.raw)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboColors {
    #[default]
    Glow,
    Solid,
    Rainbow,
    RainbowScroll,
    None,
}

impl FromStr for ComboColors {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "glow" => Ok(Self::Glow),
            "solid" => Ok(Self::Solid),
            "rainbow" => Ok(Self::Rainbow),
            "rainbowscroll" => Ok(Self::RainbowScroll),
            "none" => Ok(Self::None),
            other => Err(format!("'{other}' is not a valid ComboColors setting")),
        }
    }
}

impl core::fmt::Display for ComboColors {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Glow => write!(f, "Glow"),
            Self::Solid => write!(f, "Solid"),
            Self::Rainbow => write!(f, "Rainbow"),
            Self::RainbowScroll => write!(f, "RainbowScroll"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboMode {
    #[default]
    FullCombo,
    CurrentCombo,
}

impl FromStr for ComboMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "fullcombo" => Ok(Self::FullCombo),
            "currentcombo" => Ok(Self::CurrentCombo),
            other => Err(format!("'{other}' is not a valid ComboMode setting")),
        }
    }
}

impl core::fmt::Display for ComboMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FullCombo => write!(f, "FullCombo"),
            Self::CurrentCombo => write!(f, "CurrentCombo"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicator {
    #[default]
    None,
    SubtractiveScoring,
    PredictiveScoring,
    PaceScoring,
    RivalScoring,
    Pacemaker,
    StreamProg,
}

impl FromStr for MiniIndicator {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" => Ok(Self::None),
            "subtractivescoring" | "subtractive" => Ok(Self::SubtractiveScoring),
            "predictivescoring" | "predictive" => Ok(Self::PredictiveScoring),
            "pacescoring" | "pace" => Ok(Self::PaceScoring),
            "rivalscoring" | "rival" => Ok(Self::RivalScoring),
            "pacemaker" => Ok(Self::Pacemaker),
            "streamprog" | "streamprogress" | "stream" => Ok(Self::StreamProg),
            other => Err(format!("'{other}' is not a valid MiniIndicator setting")),
        }
    }
}

impl core::fmt::Display for MiniIndicator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::SubtractiveScoring => write!(f, "SubtractiveScoring"),
            Self::PredictiveScoring => write!(f, "PredictiveScoring"),
            Self::PaceScoring => write!(f, "PaceScoring"),
            Self::RivalScoring => write!(f, "RivalScoring"),
            Self::Pacemaker => write!(f, "Pacemaker"),
            Self::StreamProg => write!(f, "StreamProg"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetScoreSetting {
    CMinus,
    C,
    CPlus,
    BMinus,
    B,
    BPlus,
    AMinus,
    A,
    APlus,
    SMinus,
    #[default]
    S,
    SPlus,
    MachineBest,
    PersonalBest,
}

impl FromStr for TargetScoreSetting {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "cminus" | "c-" => Ok(Self::CMinus),
            "c" => Ok(Self::C),
            "cplus" | "c+" => Ok(Self::CPlus),
            "bminus" | "b-" => Ok(Self::BMinus),
            "b" => Ok(Self::B),
            "bplus" | "b+" => Ok(Self::BPlus),
            "aminus" | "a-" => Ok(Self::AMinus),
            "a" => Ok(Self::A),
            "aplus" | "a+" => Ok(Self::APlus),
            "sminus" | "s-" => Ok(Self::SMinus),
            "" | "s" => Ok(Self::S),
            "splus" | "s+" => Ok(Self::SPlus),
            "machinebest" | "machine" => Ok(Self::MachineBest),
            "personalbest" | "personal" => Ok(Self::PersonalBest),
            other => Err(format!("'{other}' is not a valid TargetScore setting")),
        }
    }
}

impl core::fmt::Display for TargetScoreSetting {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CMinus => write!(f, "C-"),
            Self::C => write!(f, "C"),
            Self::CPlus => write!(f, "C+"),
            Self::BMinus => write!(f, "B-"),
            Self::B => write!(f, "B"),
            Self::BPlus => write!(f, "B+"),
            Self::AMinus => write!(f, "A-"),
            Self::A => write!(f, "A"),
            Self::APlus => write!(f, "A+"),
            Self::SMinus => write!(f, "S-"),
            Self::S => write!(f, "S"),
            Self::SPlus => write!(f, "S+"),
            Self::MachineBest => write!(f, "Machine Best"),
            Self::PersonalBest => write!(f, "Personal Best"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub display_name: String,
    pub player_initials: String,
    // Profile stats (Simply Love / StepMania semantics).
    pub calories_burned_today: f32,
    pub calories_burned_day: String,
    pub ignore_step_count_calories: bool,
    pub groovestats_api_key: String,
    pub groovestats_is_pad_player: bool,
    pub groovestats_username: String,
    pub background_filter: BackgroundFilter,
    pub hold_judgment_graphic: HoldJudgmentGraphic,
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
    pub combo_colors: ComboColors,
    pub combo_mode: ComboMode,
    pub carry_combo_between_songs: bool,
    pub current_combo: u32,
    pub noteskin: NoteSkin,
    pub avatar_path: Option<PathBuf>,
    pub avatar_texture_key: Option<String>,
    pub scroll_speed: ScrollSpeedSetting,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    pub turn_option: TurnOption,
    // Allow early Decent/WayOff hits to be rescored to better judgments.
    pub rescore_early_hits: bool,
    // Visual behavior for early Decent/Way Off hits (Simply Love semantics).
    pub hide_early_dw_judgments: bool,
    pub hide_early_dw_flash: bool,
    // FA+ visual options (Simply Love semantics).
    // These do not change core timing semantics; they only affect HUD/UX.
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    // 10ms blue Fantastic window for FA+ window display (Arrow Cloud: "SmallerWhite").
    pub fa_plus_10ms_blue_window: bool,
    // Custom blue Fantastic window in milliseconds (1..22), shared by FA+ W0 and H.EX split.
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
    // Judgment tilt (Simply Love semantics).
    pub judgment_tilt: bool,
    pub column_cues: bool,
    // zmod ExtraAesthetics: offset indicator (ErrorMSDisplay).
    pub error_ms_display: bool,
    pub display_scorebox: bool,
    pub tilt_multiplier: f32,
    // Error bar (zmod semantics): each bit toggles one submodule in the
    // SelectMultiple row (Colorful/Monochrome/Text/Highlight/Average).
    pub error_bar_active_mask: u8,
    // Backward-compatible primary style string written to profile.ini.
    pub error_bar: ErrorBarStyle,
    // Backward-compatible text flag written to profile.ini.
    pub error_bar_text: bool,
    pub error_bar_up: bool,
    pub error_bar_multi_tick: bool,
    pub error_bar_trim: ErrorBarTrim,
    pub data_visualizations: DataVisualizations,
    pub target_score: TargetScoreSetting,
    pub lifemeter_type: LifeMeterType,
    pub measure_counter: MeasureCounter,
    pub measure_counter_lookahead: u8,
    pub measure_counter_left: bool,
    pub measure_counter_up: bool,
    pub measure_counter_vert: bool,
    pub broken_run: bool,
    pub run_timer: bool,
    pub measure_lines: MeasureLines,
    // "Hide" options (Simply Love semantics).
    pub hide_targets: bool,
    pub hide_song_bg: bool,
    pub hide_combo: bool,
    pub hide_lifebar: bool,
    pub hide_score: bool,
    pub hide_danger: bool,
    pub hide_combo_explosions: bool,
    // Gameplay extras (Simply Love semantics).
    pub column_flash_on_miss: bool,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
    pub nps_graph_at_top: bool,
    pub mini_indicator: MiniIndicator,
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
            calories_burned_today: 0.0,
            calories_burned_day: String::new(),
            ignore_step_count_calories: false,
            groovestats_api_key: String::new(),
            groovestats_is_pad_player: false,
            groovestats_username: String::new(),
            background_filter: BackgroundFilter::default(),
            hold_judgment_graphic: HoldJudgmentGraphic::default(),
            judgment_graphic: JudgmentGraphic::default(),
            combo_font: ComboFont::default(),
            combo_colors: ComboColors::default(),
            combo_mode: ComboMode::default(),
            carry_combo_between_songs: true,
            current_combo: 0,
            noteskin: NoteSkin::default(),
            avatar_path: None,
            avatar_texture_key: None,
            scroll_speed: ScrollSpeedSetting::default(),
            scroll_option: ScrollOption::default(),
            reverse_scroll: false,
            turn_option: TurnOption::default(),
            rescore_early_hits: true,
            hide_early_dw_judgments: false,
            hide_early_dw_flash: false,
            show_fa_plus_window: false,
            show_ex_score: false,
            show_hard_ex_score: false,
            show_fa_plus_pane: false,
            fa_plus_10ms_blue_window: false,
            custom_fantastic_window: false,
            custom_fantastic_window_ms: CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS,
            judgment_tilt: false,
            column_cues: false,
            error_ms_display: false,
            display_scorebox: true,
            tilt_multiplier: 1.0,
            error_bar: ErrorBarStyle::default(),
            error_bar_active_mask: error_bar_mask_from_style(ErrorBarStyle::default(), false),
            error_bar_text: false,
            error_bar_up: false,
            error_bar_multi_tick: false,
            error_bar_trim: ErrorBarTrim::default(),
            data_visualizations: DataVisualizations::default(),
            target_score: TargetScoreSetting::default(),
            lifemeter_type: LifeMeterType::default(),
            measure_counter: MeasureCounter::default(),
            measure_counter_lookahead: 2,
            measure_counter_left: true,
            measure_counter_up: false,
            measure_counter_vert: false,
            broken_run: false,
            run_timer: false,
            measure_lines: MeasureLines::default(),
            hide_targets: false,
            hide_song_bg: false,
            hide_combo: false,
            hide_lifebar: false,
            hide_score: false,
            hide_danger: false,
            hide_combo_explosions: false,
            column_flash_on_miss: false,
            subtractive_scoring: false,
            pacemaker: false,
            nps_graph_at_top: false,
            mini_indicator: MiniIndicator::None,
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
    fast_profile_switch_from_select_music: bool,
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
        fast_profile_switch_from_select_music: false,
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
            "RescoreEarlyHits = {}\n",
            i32::from(default_profile.rescore_early_hits)
        ));
        content.push_str(&format!(
            "HideEarlyDecentWayOffJudgments = {}\n",
            i32::from(default_profile.hide_early_dw_judgments)
        ));
        content.push_str(&format!(
            "HideEarlyDecentWayOffFlash = {}\n",
            i32::from(default_profile.hide_early_dw_flash)
        ));
        content.push_str(&format!(
            "HideTargets = {}\n",
            i32::from(default_profile.hide_targets)
        ));
        content.push_str(&format!(
            "HideSongBG = {}\n",
            i32::from(default_profile.hide_song_bg)
        ));
        content.push_str(&format!(
            "HideCombo = {}\n",
            i32::from(default_profile.hide_combo)
        ));
        content.push_str(&format!(
            "HideLifebar = {}\n",
            i32::from(default_profile.hide_lifebar)
        ));
        content.push_str(&format!(
            "HideScore = {}\n",
            i32::from(default_profile.hide_score)
        ));
        content.push_str(&format!(
            "HideDanger = {}\n",
            i32::from(default_profile.hide_danger)
        ));
        content.push_str(&format!(
            "HideComboExplosions = {}\n",
            i32::from(default_profile.hide_combo_explosions)
        ));
        content.push_str(&format!(
            "ColumnFlashOnMiss = {}\n",
            i32::from(default_profile.column_flash_on_miss)
        ));
        content.push_str(&format!(
            "SubtractiveScoring = {}\n",
            i32::from(default_profile.subtractive_scoring)
        ));
        content.push_str(&format!(
            "Pacemaker = {}\n",
            i32::from(default_profile.pacemaker)
        ));
        content.push_str(&format!(
            "NPSGraphAtTop = {}\n",
            i32::from(default_profile.nps_graph_at_top)
        ));
        content.push_str(&format!(
            "MiniIndicator = {}\n",
            default_profile.mini_indicator
        ));
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
            "ShowHardEXScore = {}\n",
            i32::from(default_profile.show_hard_ex_score)
        ));
        content.push_str(&format!(
            "ShowFaPlusPane = {}\n",
            i32::from(default_profile.show_fa_plus_pane)
        ));
        content.push_str(&format!(
            "SmallerWhite = {}\n",
            i32::from(default_profile.fa_plus_10ms_blue_window)
        ));
        content.push_str(&format!(
            "CustomFantasticWindow = {}\n",
            i32::from(default_profile.custom_fantastic_window)
        ));
        content.push_str(&format!(
            "CustomFantasticWindowMs = {}\n",
            default_profile.custom_fantastic_window_ms
        ));
        content.push_str(&format!(
            "JudgmentTilt = {}\n",
            i32::from(default_profile.judgment_tilt)
        ));
        content.push_str(&format!(
            "ColumnCues = {}\n",
            i32::from(default_profile.column_cues)
        ));
        content.push_str(&format!(
            "ErrorMSDisplay = {}\n",
            i32::from(default_profile.error_ms_display)
        ));
        content.push_str(&format!(
            "DisplayScorebox = {}\n",
            i32::from(default_profile.display_scorebox)
        ));
        content.push_str(&format!(
            "TiltMultiplier = {}\n",
            default_profile.tilt_multiplier
        ));
        content.push_str(&format!("ErrorBar = {}\n", default_profile.error_bar));
        content.push_str(&format!(
            "ErrorBarText = {}\n",
            i32::from(default_profile.error_bar_text)
        ));
        content.push_str(&format!(
            "ErrorBarMask = {}\n",
            default_profile.error_bar_active_mask
        ));
        content.push_str(&format!(
            "Colorful = {}\n",
            i32::from((default_profile.error_bar_active_mask & ERROR_BAR_BIT_COLORFUL) != 0)
        ));
        content.push_str(&format!(
            "Monochrome = {}\n",
            i32::from((default_profile.error_bar_active_mask & ERROR_BAR_BIT_MONOCHROME) != 0)
        ));
        content.push_str(&format!(
            "Text = {}\n",
            i32::from((default_profile.error_bar_active_mask & ERROR_BAR_BIT_TEXT) != 0)
        ));
        content.push_str(&format!(
            "Highlight = {}\n",
            i32::from((default_profile.error_bar_active_mask & ERROR_BAR_BIT_HIGHLIGHT) != 0)
        ));
        content.push_str(&format!(
            "Average = {}\n",
            i32::from((default_profile.error_bar_active_mask & ERROR_BAR_BIT_AVERAGE) != 0)
        ));
        content.push_str(&format!(
            "ErrorBarUp = {}\n",
            i32::from(default_profile.error_bar_up)
        ));
        content.push_str(&format!(
            "ErrorBarMultiTick = {}\n",
            i32::from(default_profile.error_bar_multi_tick)
        ));
        content.push_str(&format!(
            "ErrorBarTrim = {}\n",
            default_profile.error_bar_trim
        ));
        content.push_str(&format!(
            "DataVisualizations = {}\n",
            default_profile.data_visualizations
        ));
        content.push_str(&format!("TargetScore = {}\n", default_profile.target_score));
        content.push_str(&format!(
            "LifeMeterType = {}\n",
            default_profile.lifemeter_type
        ));
        content.push_str(&format!(
            "MeasureCounter = {}\n",
            default_profile.measure_counter
        ));
        content.push_str(&format!(
            "MeasureCounterLookahead = {}\n",
            default_profile.measure_counter_lookahead
        ));
        content.push_str(&format!(
            "MeasureCounterLeft = {}\n",
            i32::from(default_profile.measure_counter_left)
        ));
        content.push_str(&format!(
            "MeasureCounterUp = {}\n",
            i32::from(default_profile.measure_counter_up)
        ));
        content.push_str(&format!(
            "MeasureCounterVert = {}\n",
            i32::from(default_profile.measure_counter_vert)
        ));
        content.push_str(&format!(
            "BrokenRun = {}\n",
            i32::from(default_profile.broken_run)
        ));
        content.push_str(&format!(
            "RunTimer = {}\n",
            i32::from(default_profile.run_timer)
        ));
        content.push_str(&format!(
            "MeasureLines = {}\n",
            default_profile.measure_lines
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
        content.push_str(&format!("ComboColors = {}\n", default_profile.combo_colors));
        content.push_str(&format!("ComboMode = {}\n", default_profile.combo_mode));
        content.push_str(&format!(
            "CarryComboBetweenSongs = {}\n",
            i32::from(default_profile.carry_combo_between_songs)
        ));
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

        // Stats (for ScreenGameOver parity)
        let today = Local::now().date_naive().to_string();
        content.push_str("[Stats]\n");
        content.push_str(&format!("CaloriesBurnedDate = {today}\n"));
        content.push_str(&format!(
            "CaloriesBurnedToday = {}\n",
            default_profile.calories_burned_today
        ));
        content.push_str(&format!(
            "IgnoreStepCountCalories = {}\n",
            i32::from(default_profile.ignore_step_count_calories)
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
        "RescoreEarlyHits={}\n",
        i32::from(profile.rescore_early_hits)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffJudgments={}\n",
        i32::from(profile.hide_early_dw_judgments)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffFlash={}\n",
        i32::from(profile.hide_early_dw_flash)
    ));
    content.push_str(&format!(
        "HideTargets={}\n",
        i32::from(profile.hide_targets)
    ));
    content.push_str(&format!("HideSongBG={}\n", i32::from(profile.hide_song_bg)));
    content.push_str(&format!("HideCombo={}\n", i32::from(profile.hide_combo)));
    content.push_str(&format!(
        "HideLifebar={}\n",
        i32::from(profile.hide_lifebar)
    ));
    content.push_str(&format!("HideScore={}\n", i32::from(profile.hide_score)));
    content.push_str(&format!("HideDanger={}\n", i32::from(profile.hide_danger)));
    content.push_str(&format!(
        "HideComboExplosions={}\n",
        i32::from(profile.hide_combo_explosions)
    ));
    content.push_str(&format!(
        "ColumnFlashOnMiss={}\n",
        i32::from(profile.column_flash_on_miss)
    ));
    content.push_str(&format!(
        "SubtractiveScoring={}\n",
        i32::from(profile.subtractive_scoring)
    ));
    content.push_str(&format!("Pacemaker={}\n", i32::from(profile.pacemaker)));
    content.push_str(&format!(
        "NPSGraphAtTop={}\n",
        i32::from(profile.nps_graph_at_top)
    ));
    content.push_str(&format!("MiniIndicator={}\n", profile.mini_indicator));
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
        "ShowHardEXScore={}\n",
        i32::from(profile.show_hard_ex_score)
    ));
    content.push_str(&format!(
        "ShowFaPlusPane={}\n",
        i32::from(profile.show_fa_plus_pane)
    ));
    content.push_str(&format!(
        "SmallerWhite={}\n",
        i32::from(profile.fa_plus_10ms_blue_window)
    ));
    content.push_str(&format!(
        "CustomFantasticWindow={}\n",
        i32::from(profile.custom_fantastic_window)
    ));
    content.push_str(&format!(
        "CustomFantasticWindowMs={}\n",
        profile.custom_fantastic_window_ms
    ));
    content.push_str(&format!(
        "JudgmentTilt={}\n",
        i32::from(profile.judgment_tilt)
    ));
    content.push_str(&format!("ColumnCues={}\n", i32::from(profile.column_cues)));
    content.push_str(&format!(
        "ErrorMSDisplay={}\n",
        i32::from(profile.error_ms_display)
    ));
    content.push_str(&format!(
        "DisplayScorebox={}\n",
        i32::from(profile.display_scorebox)
    ));
    content.push_str(&format!("TiltMultiplier={}\n", profile.tilt_multiplier));
    content.push_str(&format!("ErrorBar={}\n", profile.error_bar));
    content.push_str(&format!(
        "ErrorBarText={}\n",
        i32::from(profile.error_bar_text)
    ));
    content.push_str(&format!("ErrorBarMask={}\n", profile.error_bar_active_mask));
    content.push_str(&format!(
        "Colorful={}\n",
        i32::from((profile.error_bar_active_mask & ERROR_BAR_BIT_COLORFUL) != 0)
    ));
    content.push_str(&format!(
        "Monochrome={}\n",
        i32::from((profile.error_bar_active_mask & ERROR_BAR_BIT_MONOCHROME) != 0)
    ));
    content.push_str(&format!(
        "Text={}\n",
        i32::from((profile.error_bar_active_mask & ERROR_BAR_BIT_TEXT) != 0)
    ));
    content.push_str(&format!(
        "Highlight={}\n",
        i32::from((profile.error_bar_active_mask & ERROR_BAR_BIT_HIGHLIGHT) != 0)
    ));
    content.push_str(&format!(
        "Average={}\n",
        i32::from((profile.error_bar_active_mask & ERROR_BAR_BIT_AVERAGE) != 0)
    ));
    content.push_str(&format!("ErrorBarUp={}\n", i32::from(profile.error_bar_up)));
    content.push_str(&format!(
        "ErrorBarMultiTick={}\n",
        i32::from(profile.error_bar_multi_tick)
    ));
    content.push_str(&format!("ErrorBarTrim={}\n", profile.error_bar_trim));
    content.push_str(&format!(
        "DataVisualizations={}\n",
        profile.data_visualizations
    ));
    content.push_str(&format!("TargetScore={}\n", profile.target_score));
    content.push_str(&format!("LifeMeterType={}\n", profile.lifemeter_type));
    content.push_str(&format!("MeasureCounter={}\n", profile.measure_counter));
    content.push_str(&format!(
        "MeasureCounterLookahead={}\n",
        profile.measure_counter_lookahead
    ));
    content.push_str(&format!(
        "MeasureCounterLeft={}\n",
        i32::from(profile.measure_counter_left)
    ));
    content.push_str(&format!(
        "MeasureCounterUp={}\n",
        i32::from(profile.measure_counter_up)
    ));
    content.push_str(&format!(
        "MeasureCounterVert={}\n",
        i32::from(profile.measure_counter_vert)
    ));
    content.push_str(&format!("BrokenRun={}\n", i32::from(profile.broken_run)));
    content.push_str(&format!("RunTimer={}\n", i32::from(profile.run_timer)));
    content.push_str(&format!("MeasureLines={}\n", profile.measure_lines));
    content.push_str(&format!(
        "HoldJudgmentGraphic={}\n",
        profile.hold_judgment_graphic
    ));
    content.push_str(&format!("JudgmentGraphic={}\n", profile.judgment_graphic));
    content.push_str(&format!("ComboFont={}\n", profile.combo_font));
    content.push_str(&format!("ComboColors={}\n", profile.combo_colors));
    content.push_str(&format!("ComboMode={}\n", profile.combo_mode));
    content.push_str(&format!(
        "CarryComboBetweenSongs={}\n",
        i32::from(profile.carry_combo_between_songs)
    ));
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

    content.push_str("[Stats]\n");
    content.push_str(&format!(
        "CaloriesBurnedDate={}\n",
        profile.calories_burned_day
    ));
    content.push_str(&format!(
        "CaloriesBurnedToday={}\n",
        profile.calories_burned_today
    ));
    content.push_str(&format!(
        "IgnoreStepCountCalories={}\n",
        i32::from(profile.ignore_step_count_calories)
    ));
    content.push('\n');

    let path = profile_ini_path(&profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

#[inline(always)]
fn decode_profile_stats_current_combo(bytes: &[u8], path: &Path) -> Option<u32> {
    let Ok((stats, _)) =
        bincode::decode_from_slice::<ProfileStatsV1, _>(bytes, bincode::config::standard())
    else {
        warn!("Failed to decode profile stats '{}'.", path.display());
        return None;
    };
    if stats.version != PROFILE_STATS_VERSION_V1 {
        warn!(
            "Unsupported profile stats version {} in '{}'.",
            stats.version,
            path.display()
        );
        return None;
    }
    Some(stats.current_combo)
}

fn load_profile_stats_current_combo(path: &Path) -> Option<u32> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to read {}: {}", path.display(), e);
            }
            return None;
        }
    };
    decode_profile_stats_current_combo(&bytes, path)
}

fn save_profile_stats_for_side(side: PlayerSide) {
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

    let current_combo = PROFILES.lock().unwrap()[side_ix(side)].current_combo;
    let payload = ProfileStatsV1 {
        version: PROFILE_STATS_VERSION_V1,
        current_combo,
    };
    let Ok(buf) = bincode::encode_to_vec(payload, bincode::config::standard()) else {
        warn!("Failed to encode profile stats for '{}'.", profile_id);
        return;
    };

    let path = profile_stats_path(&profile_id);
    let tmp_path = profile_stats_tmp_path(&profile_id);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(
            "Failed to create profile stats directory '{}': {}",
            parent.display(),
            e
        );
        return;
    }
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write {}: {}", tmp_path.display(), e);
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        warn!("Failed to save {}: {}", path.display(), e);
        let _ = fs::remove_file(&tmp_path);
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
            profile.combo_colors = profile_conf
                .get("PlayerOptions", "ComboColors")
                .and_then(|s| ComboColors::from_str(&s).ok())
                .unwrap_or(default_profile.combo_colors);
            profile.combo_mode = profile_conf
                .get("PlayerOptions", "ComboMode")
                .and_then(|s| ComboMode::from_str(&s).ok())
                .unwrap_or(default_profile.combo_mode);
            profile.carry_combo_between_songs = profile_conf
                .get("PlayerOptions", "CarryComboBetweenSongs")
                .or_else(|| profile_conf.get("PlayerOptions", "ComboContinuesBetweenSongs"))
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.carry_combo_between_songs, |v| v != 0);
            profile.noteskin = profile_conf
                .get("PlayerOptions", "NoteSkin")
                .and_then(|s| NoteSkin::from_str(&s).ok())
                .unwrap_or_else(|| default_profile.noteskin.clone());
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
            profile.show_hard_ex_score = profile_conf
                .get("PlayerOptions", "ShowHardEXScore")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.show_hard_ex_score, |v| v != 0);
            profile.show_fa_plus_pane = profile_conf
                .get("PlayerOptions", "ShowFaPlusPane")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.show_fa_plus_pane, |v| v != 0);
            profile.fa_plus_10ms_blue_window = profile_conf
                .get("PlayerOptions", "SmallerWhite")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.fa_plus_10ms_blue_window, |v| v != 0);
            profile.custom_fantastic_window = profile_conf
                .get("PlayerOptions", "CustomFantasticWindow")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.custom_fantastic_window, |v| v != 0);
            profile.custom_fantastic_window_ms = profile_conf
                .get("PlayerOptions", "CustomFantasticWindowMs")
                .and_then(|s| s.parse::<u8>().ok())
                .map(clamp_custom_fantastic_window_ms)
                .unwrap_or(default_profile.custom_fantastic_window_ms);
            profile.judgment_tilt = profile_conf
                .get("PlayerOptions", "JudgmentTilt")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.judgment_tilt, |v| v != 0);
            profile.column_cues = profile_conf
                .get("PlayerOptions", "ColumnCues")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.column_cues, |v| v != 0);
            profile.error_ms_display = profile_conf
                .get("PlayerOptions", "ErrorMSDisplay")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.error_ms_display, |v| v != 0);
            profile.display_scorebox = profile_conf
                .get("PlayerOptions", "DisplayScorebox")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.display_scorebox, |v| v != 0);
            profile.tilt_multiplier = profile_conf
                .get("PlayerOptions", "TiltMultiplier")
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(default_profile.tilt_multiplier);
            profile.error_bar = profile_conf
                .get("PlayerOptions", "ErrorBar")
                .and_then(|s| ErrorBarStyle::from_str(&s).ok())
                .unwrap_or(default_profile.error_bar);
            profile.error_bar_text = profile_conf
                .get("PlayerOptions", "ErrorBarText")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.error_bar_text, |v| v != 0);
            let mask_from_key = profile_conf
                .get("PlayerOptions", "ErrorBarMask")
                .and_then(|s| s.parse::<u8>().ok())
                .map(normalize_error_bar_mask);
            let colorful = profile_conf
                .get("PlayerOptions", "Colorful")
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v != 0);
            let monochrome = profile_conf
                .get("PlayerOptions", "Monochrome")
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v != 0);
            let text = profile_conf
                .get("PlayerOptions", "Text")
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v != 0);
            let highlight = profile_conf
                .get("PlayerOptions", "Highlight")
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v != 0);
            let average = profile_conf
                .get("PlayerOptions", "Average")
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v != 0);
            let mask_from_flags = if colorful.is_some()
                || monochrome.is_some()
                || text.is_some()
                || highlight.is_some()
                || average.is_some()
            {
                let mut mask: u8 = 0;
                if colorful.unwrap_or(false) {
                    mask |= ERROR_BAR_BIT_COLORFUL;
                }
                if monochrome.unwrap_or(false) {
                    mask |= ERROR_BAR_BIT_MONOCHROME;
                }
                if text.unwrap_or(false) {
                    mask |= ERROR_BAR_BIT_TEXT;
                }
                if highlight.unwrap_or(false) {
                    mask |= ERROR_BAR_BIT_HIGHLIGHT;
                }
                if average.unwrap_or(false) {
                    mask |= ERROR_BAR_BIT_AVERAGE;
                }
                Some(normalize_error_bar_mask(mask))
            } else {
                None
            };
            profile.error_bar_active_mask =
                mask_from_key.or(mask_from_flags).unwrap_or_else(|| {
                    error_bar_mask_from_style(profile.error_bar, profile.error_bar_text)
                });
            profile.error_bar = error_bar_style_from_mask(profile.error_bar_active_mask);
            profile.error_bar_text = error_bar_text_from_mask(profile.error_bar_active_mask);
            profile.error_bar_up = profile_conf
                .get("PlayerOptions", "ErrorBarUp")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.error_bar_up, |v| v != 0);
            profile.error_bar_multi_tick = profile_conf
                .get("PlayerOptions", "ErrorBarMultiTick")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.error_bar_multi_tick, |v| v != 0);
            profile.error_bar_trim = profile_conf
                .get("PlayerOptions", "ErrorBarTrim")
                .and_then(|s| ErrorBarTrim::from_str(&s).ok())
                .unwrap_or(default_profile.error_bar_trim);
            profile.data_visualizations = profile_conf
                .get("PlayerOptions", "DataVisualizations")
                .and_then(|s| DataVisualizations::from_str(&s).ok())
                .unwrap_or(default_profile.data_visualizations);
            profile.target_score = profile_conf
                .get("PlayerOptions", "TargetScore")
                .and_then(|s| TargetScoreSetting::from_str(&s).ok())
                .unwrap_or(default_profile.target_score);
            profile.lifemeter_type = profile_conf
                .get("PlayerOptions", "LifeMeterType")
                .and_then(|s| LifeMeterType::from_str(&s).ok())
                .unwrap_or(default_profile.lifemeter_type);
            profile.measure_counter = profile_conf
                .get("PlayerOptions", "MeasureCounter")
                .and_then(|s| MeasureCounter::from_str(&s).ok())
                .unwrap_or(default_profile.measure_counter);
            profile.measure_counter_lookahead = profile_conf
                .get("PlayerOptions", "MeasureCounterLookahead")
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v.min(4))
                .unwrap_or(default_profile.measure_counter_lookahead);
            profile.measure_counter_left = profile_conf
                .get("PlayerOptions", "MeasureCounterLeft")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.measure_counter_left, |v| v != 0);
            profile.measure_counter_up = profile_conf
                .get("PlayerOptions", "MeasureCounterUp")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.measure_counter_up, |v| v != 0);
            profile.measure_counter_vert = profile_conf
                .get("PlayerOptions", "MeasureCounterVert")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.measure_counter_vert, |v| v != 0);
            profile.broken_run = profile_conf
                .get("PlayerOptions", "BrokenRun")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.broken_run, |v| v != 0);
            profile.run_timer = profile_conf
                .get("PlayerOptions", "RunTimer")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.run_timer, |v| v != 0);
            profile.measure_lines = profile_conf
                .get("PlayerOptions", "MeasureLines")
                .and_then(|s| MeasureLines::from_str(&s).ok())
                .unwrap_or(default_profile.measure_lines);
            profile.scroll_speed = profile_conf
                .get("PlayerOptions", "ScrollSpeed")
                .and_then(|s| ScrollSpeedSetting::from_str(&s).ok())
                .unwrap_or(default_profile.scroll_speed);
            profile.turn_option = profile_conf
                .get("PlayerOptions", "Turn")
                .and_then(|s| TurnOption::from_str(&s).ok())
                .unwrap_or(default_profile.turn_option);
            profile.rescore_early_hits = profile_conf
                .get("PlayerOptions", "RescoreEarlyHits")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.rescore_early_hits, |v| v != 0);
            profile.hide_early_dw_judgments = profile_conf
                .get("PlayerOptions", "HideEarlyDecentWayOffJudgments")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_early_dw_judgments, |v| v != 0);
            profile.hide_early_dw_flash = profile_conf
                .get("PlayerOptions", "HideEarlyDecentWayOffFlash")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_early_dw_flash, |v| v != 0);
            profile.hide_targets = profile_conf
                .get("PlayerOptions", "HideTargets")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_targets, |v| v != 0);
            profile.hide_song_bg = profile_conf
                .get("PlayerOptions", "HideSongBG")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_song_bg, |v| v != 0);
            profile.hide_combo = profile_conf
                .get("PlayerOptions", "HideCombo")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_combo, |v| v != 0);
            profile.hide_lifebar = profile_conf
                .get("PlayerOptions", "HideLifebar")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_lifebar, |v| v != 0);
            profile.hide_score = profile_conf
                .get("PlayerOptions", "HideScore")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_score, |v| v != 0);
            profile.hide_danger = profile_conf
                .get("PlayerOptions", "HideDanger")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_danger, |v| v != 0);
            profile.hide_combo_explosions = profile_conf
                .get("PlayerOptions", "HideComboExplosions")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.hide_combo_explosions, |v| v != 0);
            profile.column_flash_on_miss = profile_conf
                .get("PlayerOptions", "ColumnFlashOnMiss")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.column_flash_on_miss, |v| v != 0);
            profile.subtractive_scoring = profile_conf
                .get("PlayerOptions", "SubtractiveScoring")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.subtractive_scoring, |v| v != 0);
            profile.pacemaker = profile_conf
                .get("PlayerOptions", "Pacemaker")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.pacemaker, |v| v != 0);
            profile.nps_graph_at_top = profile_conf
                .get("PlayerOptions", "NPSGraphAtTop")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.nps_graph_at_top, |v| v != 0);
            profile.mini_indicator = profile_conf
                .get("PlayerOptions", "MiniIndicator")
                .and_then(|s| MiniIndicator::from_str(&s).ok())
                .unwrap_or_else(|| {
                    if profile.subtractive_scoring {
                        MiniIndicator::SubtractiveScoring
                    } else if profile.pacemaker {
                        MiniIndicator::Pacemaker
                    } else {
                        default_profile.mini_indicator
                    }
                });
            if profile.mini_indicator == MiniIndicator::SubtractiveScoring {
                profile.subtractive_scoring = true;
            }
            if profile.mini_indicator == MiniIndicator::Pacemaker {
                profile.pacemaker = true;
            }
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

            // Profile stats (ScreenGameOver parity)
            profile.ignore_step_count_calories = profile_conf
                .get("Stats", "IgnoreStepCountCalories")
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.ignore_step_count_calories, |v| v != 0);

            let today = Local::now().date_naive().to_string();
            let saved_day = profile_conf
                .get("Stats", "CaloriesBurnedDate")
                .unwrap_or_default();
            let saved_cals = profile_conf
                .get("Stats", "CaloriesBurnedToday")
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(default_profile.calories_burned_today);

            if saved_day.trim() == today {
                profile.calories_burned_day = today;
                profile.calories_burned_today = saved_cals;
            } else {
                profile.calories_burned_day = today;
                profile.calories_burned_today = 0.0;
            }
        } else {
            warn!(
                "Failed to load '{}', using default profile settings.",
                profile_ini.display()
            );
        }

        profile.current_combo =
            load_profile_stats_current_combo(&profile_stats_path(&profile_id))
                .unwrap_or(default_profile.current_combo);

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
    save_profile_stats_for_side(side);
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
pub fn get_active_profile_for_side(side: PlayerSide) -> ActiveProfile {
    SESSION.lock().unwrap().active_profiles[side_ix(side)].clone()
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

#[inline(always)]
fn is_local_profile_id(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn scan_local_profiles() -> Vec<LocalProfileSummary> {
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
        if !is_local_profile_id(&id) {
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

const LOCAL_PROFILE_MAX_ID: u32 = 99_999_999;

fn scan_local_profile_numbers() -> Vec<u32> {
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
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.len() != 8 {
            continue;
        }
        let Ok(n) = name.parse::<u32>() else {
            continue;
        };
        if n <= LOCAL_PROFILE_MAX_ID {
            out.push(n);
        }
    }
    out
}

fn allocate_local_profile_id() -> Result<String, std::io::Error> {
    let mut nums = scan_local_profile_numbers();
    nums.sort_unstable();
    nums.dedup();

    let mut first_free = 0_u32;
    for &n in &nums {
        if n == first_free {
            first_free += 1;
        } else if n > first_free {
            break;
        }
    }

    let mut next = nums.last().copied().unwrap_or(0);
    if !nums.is_empty() {
        next = next.saturating_add(1);
    }
    if next > LOCAL_PROFILE_MAX_ID {
        if first_free > LOCAL_PROFILE_MAX_ID {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Too many profiles",
            ));
        }
        next = first_free;
    }
    Ok(format!("{next:08}"))
}

fn initials_from_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
            if out.len() >= 2 {
                break;
            }
        }
    }
    match out.len() {
        0 => "??".to_string(),
        1 => {
            out.push('?');
            out
        }
        _ => out,
    }
}

pub fn create_local_profile(display_name: &str) -> Result<String, std::io::Error> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let id = allocate_local_profile_id()?;
    let dir = local_profile_dir(&id);
    fs::create_dir_all(&dir)?;

    let default_profile = Profile::default();
    let initials = initials_from_name(name);
    let mut content = String::new();
    content.push_str("[PlayerOptions]\n");
    content.push_str(&format!("ScrollSpeed={}\n", default_profile.scroll_speed));
    content.push_str(&format!("Scroll={}\n", default_profile.scroll_option));
    content.push('\n');
    content.push_str("[userprofile]\n");
    content.push_str(&format!("DisplayName={name}\n"));
    content.push_str(&format!("PlayerInitials={initials}\n"));
    content.push('\n');

    let today = Local::now().date_naive().to_string();
    content.push_str("[Stats]\n");
    content.push_str(&format!("CaloriesBurnedDate={today}\n"));
    content.push_str("CaloriesBurnedToday=0\n");
    content.push_str("IgnoreStepCountCalories=0\n");
    content.push('\n');
    fs::write(profile_ini_path(&id), content)?;

    let mut gs = String::new();
    gs.push_str("[GrooveStats]\n");
    gs.push_str("ApiKey=\n");
    gs.push_str("IsPadPlayer=0\n");
    gs.push_str("Username=\n");
    gs.push('\n');
    fs::write(groovestats_ini_path(&id), gs)?;

    Ok(id)
}

fn rewrite_profile_display_name(path: &Path, display_name: &str) -> Result<(), std::io::Error> {
    let src = fs::read_to_string(path)?;
    let mut out = String::with_capacity(src.len() + display_name.len() + 32);
    let mut in_userprofile = false;
    let mut saw_userprofile = false;
    let mut wrote_display = false;

    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_userprofile && !wrote_display {
                out.push_str("DisplayName=");
                out.push_str(display_name);
                out.push('\n');
                wrote_display = true;
            }
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_userprofile = section.eq_ignore_ascii_case("userprofile");
            if in_userprofile {
                saw_userprofile = true;
            }
            out.push_str(raw_line);
            out.push('\n');
            continue;
        }

        if in_userprofile && let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim();
            if key.eq_ignore_ascii_case("DisplayName") {
                out.push_str("DisplayName=");
                out.push_str(display_name);
                out.push('\n');
                wrote_display = true;
                continue;
            }
        }

        out.push_str(raw_line);
        out.push('\n');
    }

    if !saw_userprofile {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("[userprofile]\n");
        out.push_str("DisplayName=");
        out.push_str(display_name);
        out.push('\n');
    } else if in_userprofile && !wrote_display {
        out.push_str("DisplayName=");
        out.push_str(display_name);
        out.push('\n');
    }

    fs::write(path, out)
}

pub fn rename_local_profile(id: &str, display_name: &str) -> Result<(), std::io::Error> {
    if !is_local_profile_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid local profile id",
        ));
    }

    let name = display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let ini_path = profile_ini_path(id);
    if !ini_path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }
    rewrite_profile_display_name(&ini_path, name)?;

    let p1_active = active_local_profile_id_for_side(PlayerSide::P1)
        .as_deref()
        .is_some_and(|active_id| active_id == id);
    let p2_active = active_local_profile_id_for_side(PlayerSide::P2)
        .as_deref()
        .is_some_and(|active_id| active_id == id);
    if p1_active || p2_active {
        let mut profiles = PROFILES.lock().unwrap();
        if p1_active {
            profiles[side_ix(PlayerSide::P1)].display_name = name.to_string();
        }
        if p2_active {
            profiles[side_ix(PlayerSide::P2)].display_name = name.to_string();
        }
    }

    Ok(())
}

pub fn delete_local_profile(id: &str) -> Result<(), std::io::Error> {
    if !is_local_profile_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid local profile id",
        ));
    }

    let dir = local_profile_dir(id);
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }

    fs::remove_dir_all(&dir)?;

    for side in [PlayerSide::P1, PlayerSide::P2] {
        let is_active = active_local_profile_id_for_side(side)
            .as_deref()
            .is_some_and(|active_id| active_id == id);
        if is_active {
            let _ = set_active_profile_for_side(side, ActiveProfile::Guest);
        }
    }

    Ok(())
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

pub fn set_fast_profile_switch_from_select_music(enabled: bool) {
    SESSION
        .lock()
        .unwrap()
        .fast_profile_switch_from_select_music = enabled;
}

pub fn fast_profile_switch_from_select_music() -> bool {
    SESSION
        .lock()
        .unwrap()
        .fast_profile_switch_from_select_music
}

pub fn take_fast_profile_switch_from_select_music() -> bool {
    let mut session = SESSION.lock().unwrap();
    let was_set = session.fast_profile_switch_from_select_music;
    session.fast_profile_switch_from_select_music = false;
    was_set
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

pub fn add_stage_calories_for_side(side: PlayerSide, notes_hit: u32) {
    if session_side_is_guest(side) {
        return;
    }

    let today = Local::now().date_naive().to_string();
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];

        if profile.calories_burned_day.trim() != today {
            profile.calories_burned_day = today.clone();
            profile.calories_burned_today = 0.0;
        }

        if !profile.ignore_step_count_calories {
            // TODO: Implement StepMania's actual calorie model.
            const KCAL_PER_NOTE_HIT: f32 = 0.032;
            let add = notes_hit as f32 * KCAL_PER_NOTE_HIT;
            if add.is_finite() && add >= 0.0 {
                profile.calories_burned_today = (profile.calories_burned_today + add).max(0.0);
            }
        }
    }
    save_profile_ini_for_side(side);
}

pub fn update_player_initials_for_side(side: PlayerSide, initials: &str) {
    if session_side_is_guest(side) {
        return;
    }
    let initials = initials.trim();
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.player_initials == initials {
            return;
        }
        profile.player_initials = initials.to_string();
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

pub fn update_combo_colors_for_side(side: PlayerSide, setting: ComboColors) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_colors == setting {
            return;
        }
        profile.combo_colors = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_combo_mode_for_side(side: PlayerSide, setting: ComboMode) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_mode == setting {
            return;
        }
        profile.combo_mode = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_carry_combo_between_songs_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.carry_combo_between_songs == enabled {
            return;
        }
        profile.carry_combo_between_songs = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_current_combo_for_side(side: PlayerSide, combo: u32) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.current_combo == combo {
            return;
        }
        profile.current_combo = combo;
    }
    save_profile_stats_for_side(side);
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

pub fn update_rescore_early_hits_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.rescore_early_hits == enabled {
            return;
        }
        profile.rescore_early_hits = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_early_dw_options_for_side(side: PlayerSide, hide_judgments: bool, hide_flash: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.hide_early_dw_judgments == hide_judgments
            && profile.hide_early_dw_flash == hide_flash
        {
            return;
        }
        profile.hide_early_dw_judgments = hide_judgments;
        profile.hide_early_dw_flash = hide_flash;
    }
    save_profile_ini_for_side(side);
}

pub fn update_hide_options_for_side(
    side: PlayerSide,
    hide_targets: bool,
    hide_song_bg: bool,
    hide_combo: bool,
    hide_lifebar: bool,
    hide_score: bool,
    hide_danger: bool,
    hide_combo_explosions: bool,
) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.hide_targets == hide_targets
            && profile.hide_song_bg == hide_song_bg
            && profile.hide_combo == hide_combo
            && profile.hide_lifebar == hide_lifebar
            && profile.hide_score == hide_score
            && profile.hide_danger == hide_danger
            && profile.hide_combo_explosions == hide_combo_explosions
        {
            return;
        }
        profile.hide_targets = hide_targets;
        profile.hide_song_bg = hide_song_bg;
        profile.hide_combo = hide_combo;
        profile.hide_lifebar = hide_lifebar;
        profile.hide_score = hide_score;
        profile.hide_danger = hide_danger;
        profile.hide_combo_explosions = hide_combo_explosions;
    }
    save_profile_ini_for_side(side);
}

pub fn update_gameplay_extras_for_side(
    side: PlayerSide,
    column_flash_on_miss: bool,
    subtractive_scoring: bool,
    pacemaker: bool,
    nps_graph_at_top: bool,
) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.column_flash_on_miss == column_flash_on_miss
            && profile.subtractive_scoring == subtractive_scoring
            && profile.pacemaker == pacemaker
            && profile.nps_graph_at_top == nps_graph_at_top
        {
            return;
        }
        profile.column_flash_on_miss = column_flash_on_miss;
        profile.subtractive_scoring = subtractive_scoring;
        profile.pacemaker = pacemaker;
        profile.nps_graph_at_top = nps_graph_at_top;
        if subtractive_scoring {
            profile.mini_indicator = MiniIndicator::SubtractiveScoring;
        } else if pacemaker {
            profile.mini_indicator = MiniIndicator::Pacemaker;
        } else if matches!(
            profile.mini_indicator,
            MiniIndicator::SubtractiveScoring | MiniIndicator::Pacemaker
        ) {
            profile.mini_indicator = MiniIndicator::None;
        }
    }
    save_profile_ini_for_side(side);
}

pub fn update_mini_indicator_for_side(side: PlayerSide, setting: MiniIndicator) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.mini_indicator == setting {
            return;
        }
        profile.mini_indicator = setting;
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

pub fn update_show_hard_ex_score_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_hard_ex_score == enabled {
            return;
        }
        profile.show_hard_ex_score = enabled;
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

pub fn update_fa_plus_10ms_blue_window_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.fa_plus_10ms_blue_window == enabled {
            return;
        }
        profile.fa_plus_10ms_blue_window = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_custom_fantastic_window_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.custom_fantastic_window == enabled {
            return;
        }
        profile.custom_fantastic_window = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_custom_fantastic_window_ms_for_side(side: PlayerSide, ms: u8) {
    if session_side_is_guest(side) {
        return;
    }
    let clamped = clamp_custom_fantastic_window_ms(ms);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.custom_fantastic_window_ms == clamped {
            return;
        }
        profile.custom_fantastic_window_ms = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_judgment_tilt_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.judgment_tilt == enabled {
            return;
        }
        profile.judgment_tilt = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_column_cues_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.column_cues == enabled {
            return;
        }
        profile.column_cues = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_ms_display_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_ms_display == enabled {
            return;
        }
        profile.error_ms_display = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_display_scorebox_for_side(side: PlayerSide, enabled: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.display_scorebox == enabled {
            return;
        }
        profile.display_scorebox = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_tilt_multiplier_for_side(side: PlayerSide, multiplier: f32) {
    if session_side_is_guest(side) {
        return;
    }
    if !multiplier.is_finite() {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if (profile.tilt_multiplier - multiplier).abs() < 1e-6 {
            return;
        }
        profile.tilt_multiplier = multiplier;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_mask_for_side(side: PlayerSide, mask: u8) {
    if session_side_is_guest(side) {
        return;
    }
    let mask = normalize_error_bar_mask(mask);
    let style = error_bar_style_from_mask(mask);
    let text = error_bar_text_from_mask(mask);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_active_mask == mask {
            return;
        }
        profile.error_bar_active_mask = mask;
        profile.error_bar = style;
        profile.error_bar_text = text;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_trim_for_side(side: PlayerSide, setting: ErrorBarTrim) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_trim == setting {
            return;
        }
        profile.error_bar_trim = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_data_visualizations_for_side(side: PlayerSide, setting: DataVisualizations) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.data_visualizations == setting {
            return;
        }
        profile.data_visualizations = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_target_score_for_side(side: PlayerSide, setting: TargetScoreSetting) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.target_score == setting {
            return;
        }
        profile.target_score = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_lifemeter_type_for_side(side: PlayerSide, setting: LifeMeterType) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.lifemeter_type == setting {
            return;
        }
        profile.lifemeter_type = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_options_for_side(side: PlayerSide, up: bool, multi_tick: bool) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_up == up && profile.error_bar_multi_tick == multi_tick {
            return;
        }
        profile.error_bar_up = up;
        profile.error_bar_multi_tick = multi_tick;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_counter_for_side(side: PlayerSide, setting: MeasureCounter) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_counter == setting {
            return;
        }
        profile.measure_counter = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_counter_lookahead_for_side(side: PlayerSide, lookahead: u8) {
    if session_side_is_guest(side) {
        return;
    }
    let lookahead = lookahead.min(4);
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_counter_lookahead == lookahead {
            return;
        }
        profile.measure_counter_lookahead = lookahead;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_counter_options_for_side(
    side: PlayerSide,
    left: bool,
    up: bool,
    vert: bool,
    broken_run: bool,
    run_timer: bool,
) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_counter_left == left
            && profile.measure_counter_up == up
            && profile.measure_counter_vert == vert
            && profile.broken_run == broken_run
            && profile.run_timer == run_timer
        {
            return;
        }
        profile.measure_counter_left = left;
        profile.measure_counter_up = up;
        profile.measure_counter_vert = vert;
        profile.broken_run = broken_run;
        profile.run_timer = run_timer;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_lines_for_side(side: PlayerSide, setting: MeasureLines) {
    if session_side_is_guest(side) {
        return;
    }
    {
        let mut profiles = PROFILES.lock().unwrap();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_lines == setting {
            return;
        }
        profile.measure_lines = setting;
    }
    save_profile_ini_for_side(side);
}
