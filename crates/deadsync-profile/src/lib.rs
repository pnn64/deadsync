use bitflags::bitflags;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const PLAYER_SLOTS: usize = 2;
pub const DEFAULT_WEIGHT_POUNDS: i32 = 120;
pub const DEFAULT_BIRTH_YEAR: i32 = 1995;
pub const PLAYER_INITIALS_MAX_LEN: usize = 4;
pub const HUD_OFFSET_MIN: i32 = -250;
pub const HUD_OFFSET_MAX: i32 = 250;
pub const SPACING_PERCENT_MIN: i32 = -100;
pub const SPACING_PERCENT_MAX: i32 = 100;
pub const MINI_PERCENT_MIN: i32 = -100;
pub const MINI_PERCENT_MAX: i32 = 150;
pub const NOTE_FIELD_OFFSET_X_MIN: i32 = 0;
pub const NOTE_FIELD_OFFSET_X_MAX: i32 = 50;
pub const NOTE_FIELD_OFFSET_Y_MIN: i32 = -50;
pub const NOTE_FIELD_OFFSET_Y_MAX: i32 = 50;
pub const VISUAL_DELAY_MS_MIN: i32 = -100;
pub const VISUAL_DELAY_MS_MAX: i32 = 100;
pub const TILT_THRESHOLD_MIN_MS: u32 = 0;
pub const TILT_THRESHOLD_MAX_MS: u32 = 100;
pub const TILT_MIN_THRESHOLD_DEFAULT_MS: u32 = 0;
pub const TILT_MAX_THRESHOLD_DEFAULT_MS: u32 = 50;
pub const LONG_ERROR_BAR_INTENSITY_MIN: f32 = 1.0;
pub const LONG_ERROR_BAR_INTENSITY_MAX: f32 = 2.0;
pub const LONG_ERROR_BAR_INTENSITY_STEP: f32 = 0.25;
pub const LONG_ERROR_BAR_INTENSITY_DEFAULT: f32 = 2.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_MIN: f32 = 1.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_MAX: f32 = 2.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_STEP: f32 = 0.25;
pub const AVERAGE_ERROR_BAR_INTENSITY_DEFAULT: f32 = 1.0;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MIN: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MAX: u32 = 2000;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_STEP: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT: u32 = 400;
pub const LONG_ERROR_BAR_THRESHOLD_MS_MIN: u32 = 1;
pub const LONG_ERROR_BAR_THRESHOLD_MS_MAX: u32 = 15;
pub const LONG_ERROR_BAR_THRESHOLD_MS_DEFAULT: u32 = 4;
pub const LONG_ERROR_BAR_MIN_SAMPLES_MIN: u32 = 4;
pub const LONG_ERROR_BAR_MIN_SAMPLES_MAX: u32 = 64;
pub const LONG_ERROR_BAR_MIN_SAMPLES_DEFAULT: u32 = 16;
pub const LONG_ERROR_BAR_BUFFER_CAP_MIN: u32 = 16;
pub const LONG_ERROR_BAR_BUFFER_CAP_MAX: u32 = 128;
pub const LONG_ERROR_BAR_BUFFER_CAP_DEFAULT: u32 = 64;
pub const CUSTOM_FANTASTIC_WINDOW_MIN_MS: u8 = 1;
pub const CUSTOM_FANTASTIC_WINDOW_MAX_MS: u8 = 22;
pub const CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS: u8 = 10;

#[inline(always)]
pub const fn clamp_tilt_threshold_ms(ms: u32) -> u32 {
    if ms > TILT_THRESHOLD_MAX_MS {
        TILT_THRESHOLD_MAX_MS
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_long_error_bar_threshold_ms(ms: u32) -> u32 {
    if ms < LONG_ERROR_BAR_THRESHOLD_MS_MIN {
        LONG_ERROR_BAR_THRESHOLD_MS_MIN
    } else if ms > LONG_ERROR_BAR_THRESHOLD_MS_MAX {
        LONG_ERROR_BAR_THRESHOLD_MS_MAX
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_long_error_bar_min_samples(n: u32) -> u32 {
    if n < LONG_ERROR_BAR_MIN_SAMPLES_MIN {
        LONG_ERROR_BAR_MIN_SAMPLES_MIN
    } else if n > LONG_ERROR_BAR_MIN_SAMPLES_MAX {
        LONG_ERROR_BAR_MIN_SAMPLES_MAX
    } else {
        n
    }
}

#[inline]
pub const fn clamp_long_error_bar_buffer_cap(n: u32) -> u32 {
    if n < LONG_ERROR_BAR_BUFFER_CAP_MIN {
        LONG_ERROR_BAR_BUFFER_CAP_MIN
    } else if n > LONG_ERROR_BAR_BUFFER_CAP_MAX {
        LONG_ERROR_BAR_BUFFER_CAP_MAX
    } else {
        n
    }
}

#[inline]
pub fn clamp_long_error_bar_intensity(value: f32) -> f32 {
    if !value.is_finite() {
        return LONG_ERROR_BAR_INTENSITY_DEFAULT;
    }
    let clamped = value.clamp(LONG_ERROR_BAR_INTENSITY_MIN, LONG_ERROR_BAR_INTENSITY_MAX);
    let steps = ((clamped - LONG_ERROR_BAR_INTENSITY_MIN) / LONG_ERROR_BAR_INTENSITY_STEP).round();
    (LONG_ERROR_BAR_INTENSITY_MIN + steps * LONG_ERROR_BAR_INTENSITY_STEP)
        .clamp(LONG_ERROR_BAR_INTENSITY_MIN, LONG_ERROR_BAR_INTENSITY_MAX)
}

#[inline]
pub fn clamp_average_error_bar_intensity(value: f32) -> f32 {
    if !value.is_finite() {
        return AVERAGE_ERROR_BAR_INTENSITY_DEFAULT;
    }
    let clamped = value.clamp(
        AVERAGE_ERROR_BAR_INTENSITY_MIN,
        AVERAGE_ERROR_BAR_INTENSITY_MAX,
    );
    let steps =
        ((clamped - AVERAGE_ERROR_BAR_INTENSITY_MIN) / AVERAGE_ERROR_BAR_INTENSITY_STEP).round();
    (AVERAGE_ERROR_BAR_INTENSITY_MIN + steps * AVERAGE_ERROR_BAR_INTENSITY_STEP).clamp(
        AVERAGE_ERROR_BAR_INTENSITY_MIN,
        AVERAGE_ERROR_BAR_INTENSITY_MAX,
    )
}

#[inline]
pub const fn clamp_average_error_bar_interval_ms(ms: u32) -> u32 {
    let clamped = if ms < AVERAGE_ERROR_BAR_INTERVAL_MS_MIN {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
    } else if ms > AVERAGE_ERROR_BAR_INTERVAL_MS_MAX {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
    } else {
        ms
    };
    let steps = (clamped - AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        + AVERAGE_ERROR_BAR_INTERVAL_MS_STEP / 2)
        / AVERAGE_ERROR_BAR_INTERVAL_MS_STEP;
    AVERAGE_ERROR_BAR_INTERVAL_MS_MIN + steps * AVERAGE_ERROR_BAR_INTERVAL_MS_STEP
}

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
    #[inline(always)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingTickMode {
    #[default]
    Off,
    Assist,
    Hit,
}

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
pub enum ComboFont {
    #[default]
    Wendy,
    ArialRounded,
    Asap,
    BebasNeue,
    SourceCode,
    Work,
    WendyCursed,
    Mega,
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
            "mega" => Ok(Self::Mega),
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
            Self::Mega => write!(f, "Mega"),
            Self::None => write!(f, "None"),
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

bitflags! {
    /// Persisted bitmask of live timing statistics shown during gameplay.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct LiveTimingStatsMask: u8 {
        const MEAN     = 1 << 0;
        const MEAN_ABS = 1 << 1;
        const MAX      = 1 << 2;
    }
}

bitflags! {
    /// Persisted bitmask for the Error Bar SelectMultiple row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ErrorBarMask: u8 {
        const COLORFUL   = 1 << 0;
        const MONOCHROME = 1 << 1;
        const TEXT       = 1 << 2;
        const HIGHLIGHT  = 1 << 3;
        const AVERAGE    = 1 << 4;
    }
}

#[inline(always)]
pub const fn error_bar_mask_from_style(style: ErrorBarStyle, text: bool) -> ErrorBarMask {
    let text_bits = if text { ErrorBarMask::TEXT.bits() } else { 0 };
    let style_bits = match style {
        ErrorBarStyle::None => 0,
        ErrorBarStyle::Colorful => ErrorBarMask::COLORFUL.bits(),
        ErrorBarStyle::Monochrome => ErrorBarMask::MONOCHROME.bits(),
        ErrorBarStyle::Text => ErrorBarMask::TEXT.bits(),
        ErrorBarStyle::Highlight => ErrorBarMask::HIGHLIGHT.bits(),
        ErrorBarStyle::Average => ErrorBarMask::AVERAGE.bits(),
    };
    ErrorBarMask::from_bits_truncate(text_bits | style_bits)
}

#[inline(always)]
pub const fn error_bar_style_from_mask(mask: ErrorBarMask) -> ErrorBarStyle {
    if mask.contains(ErrorBarMask::COLORFUL) {
        ErrorBarStyle::Colorful
    } else if mask.contains(ErrorBarMask::MONOCHROME) {
        ErrorBarStyle::Monochrome
    } else if mask.contains(ErrorBarMask::HIGHLIGHT) {
        ErrorBarStyle::Highlight
    } else if mask.contains(ErrorBarMask::AVERAGE) {
        ErrorBarStyle::Average
    } else {
        ErrorBarStyle::None
    }
}

#[inline(always)]
pub const fn error_bar_text_from_mask(mask: ErrorBarMask) -> bool {
    mask.contains(ErrorBarMask::TEXT)
}

bitflags! {
    /// Persisted bitmask of enabled appearance transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct AppearanceEffectsMask: u8 {
        const HIDDEN         = 1 << 0;
        const SUDDEN         = 1 << 1;
        const STEALTH        = 1 << 2;
        const BLINK          = 1 << 3;
        const RANDOM_VANISH  = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled acceleration transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct AccelEffectsMask: u8 {
        const BOOST     = 1 << 0;
        const BRAKE     = 1 << 1;
        const WAVE      = 1 << 2;
        const EXPAND    = 1 << 3;
        const BOOMERANG = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled hold transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct HoldsMask: u8 {
        const PLANTED        = 1 << 0;
        const FLOORED        = 1 << 1;
        const TWISTER        = 1 << 2;
        const NO_ROLLS       = 1 << 3;
        const HOLDS_TO_ROLLS = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled visual transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct VisualEffectsMask: u16 {
        const DRUNK     = 1 << 0;
        const DIZZY     = 1 << 1;
        const CONFUSION = 1 << 2;
        const BIG       = 1 << 3;
        const FLIP      = 1 << 4;
        const INVERT    = 1 << 5;
        const TORNADO   = 1 << 6;
        const TIPSY     = 1 << 7;
        const BUMPY     = 1 << 8;
        const BEAT      = 1 << 9;
    }
}

bitflags! {
    /// Persisted bitmask of enabled chart insert transforms.
    ///
    /// Bit layout matches the runtime insert-mask constants, except bit 7
    /// (Mines) is runtime/attack-only and is deliberately not represented
    /// here.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct InsertMask: u8 {
        const WIDE   = 1 << 0;
        const BIG    = 1 << 1;
        const QUICK  = 1 << 2;
        const BMRIZE = 1 << 3;
        const SKIPPY = 1 << 4;
        const ECHO   = 1 << 5;
        const STOMP  = 1 << 6;
    }
}

bitflags! {
    /// Persisted bitmask of enabled chart removal transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct RemoveMask: u8 {
        const LITTLE   = 1 << 0;
        const NO_MINES = 1 << 1;
        const NO_HOLDS = 1 << 2;
        const NO_JUMPS = 1 << 3;
        const NO_HANDS = 1 << 4;
        const NO_QUADS = 1 << 5;
        const NO_LIFTS = 1 << 6;
        const NO_FAKES = 1 << 7;
    }
}

bitflags! {
    /// Persisted bitmask of tap explosion windows enabled for gameplay.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TapExplosionMask: u8 {
        const FANTASTIC = 1 << 0;
        const EXCELLENT = 1 << 1;
        const GREAT     = 1 << 2;
        const DECENT    = 1 << 3;
        const WAY_OFF   = 1 << 4;
        const HELD      = 1 << 5;
        const MISS      = 1 << 6;
        const HOLDING   = 1 << 7;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttackMode {
    Off,
    #[default]
    On,
    Random,
}

impl FromStr for AttackMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "off" | "noattacks" | "noattack" => Ok(Self::Off),
            "on" | "normal" => Ok(Self::On),
            "random" | "randomattacks" => Ok(Self::Random),
            other => Err(format!("'{other}' is not a valid AttackMode setting")),
        }
    }
}

impl core::fmt::Display for AttackMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::On => write!(f, "On"),
            Self::Random => write!(f, "Random"),
        }
    }
}

/// Hard cap for the evaluation scatter plot's vertical scale, selectable
/// per profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScatterplotMaxWindow {
    #[default]
    Off,
    Fantastic,
    Excellent,
    Great,
}

impl FromStr for ScatterplotMaxWindow {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "off" | "none" | "autoscale" | "0" => Ok(Self::Off),
            "fantastic" | "fantasticmax" | "fa" => Ok(Self::Fantastic),
            "excellent" | "excellentmax" | "ex" => Ok(Self::Excellent),
            "great" | "greatmax" | "gr" => Ok(Self::Great),
            other => Err(format!(
                "'{other}' is not a valid ScatterplotMaxWindow setting"
            )),
        }
    }
}

impl core::fmt::Display for ScatterplotMaxWindow {
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
pub enum TimingWindowsOption {
    #[default]
    None,
    WayOffs,
    DecentsAndWayOffs,
    FantasticsAndExcellents,
}

impl TimingWindowsOption {
    #[inline(always)]
    pub const fn disabled_windows(self) -> [bool; 5] {
        match self {
            Self::None => [false; 5],
            Self::WayOffs => [false, false, false, false, true],
            Self::DecentsAndWayOffs => [false, false, false, true, true],
            Self::FantasticsAndExcellents => [true, true, false, false, false],
        }
    }
}

impl FromStr for TimingWindowsOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "way offs" | "wayoffs" => Ok(Self::WayOffs),
            "decents + way offs" | "decents+wayoffs" | "decents and way offs" => {
                Ok(Self::DecentsAndWayOffs)
            }
            "fantastics + excellents" | "fantastics+excellents" | "fantastics and excellents" => {
                Ok(Self::FantasticsAndExcellents)
            }
            other => Err(format!("'{other}' is not a valid TimingWindows setting")),
        }
    }
}

impl core::fmt::Display for TimingWindowsOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::WayOffs => write!(f, "Way Offs"),
            Self::DecentsAndWayOffs => write!(f, "Decents + Way Offs"),
            Self::FantasticsAndExcellents => write!(f, "Fantastics + Excellents"),
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
pub enum MiniIndicatorScoreType {
    #[default]
    Itg,
    Ex,
    HardEx,
}

impl FromStr for MiniIndicatorScoreType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "itg" => Ok(Self::Itg),
            "ex" => Ok(Self::Ex),
            "hardex" | "hex" => Ok(Self::HardEx),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorScoreType setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorScoreType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Itg => write!(f, "ITG"),
            Self::Ex => write!(f, "Ex"),
            Self::HardEx => write!(f, "HardEx"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorSize {
    #[default]
    Default,
    Large,
}

impl FromStr for MiniIndicatorSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" => Ok(Self::Default),
            "large" | "big" => Ok(Self::Large),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorSize setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Large => write!(f, "Large"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorColor {
    #[default]
    Default,
    Detailed,
}

impl FromStr for MiniIndicatorColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" => Ok(Self::Default),
            "detailed" => Ok(Self::Detailed),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorColor setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorColor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Detailed => write!(f, "Detailed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HideLightType {
    #[default]
    NoHideLights,
    HideAllLights,
    HideMarqueeLights,
    HideBassLights,
}

impl FromStr for HideLightType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "nohidelights" => Ok(Self::NoHideLights),
            "hidealllights" => Ok(Self::HideAllLights),
            "hidemarqueelights" => Ok(Self::HideMarqueeLights),
            "hidebasslights" => Ok(Self::HideBassLights),
            other => Err(format!("'{other}' is not a valid HideLightType setting")),
        }
    }
}

impl core::fmt::Display for HideLightType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoHideLights => write!(f, "NoHideLights"),
            Self::HideAllLights => write!(f, "HideAllLights"),
            Self::HideMarqueeLights => write!(f, "HideMarqueeLights"),
            Self::HideBassLights => write!(f, "HideBassLights"),
        }
    }
}

/// Background-darkening alpha for the per-notefield underlay quad, expressed
/// as an integer percentage in `0..=100` (0 = no filter, 100 = fully opaque
/// black). Reads accept the legacy enum labels (`Off|Dark|Darker|Darkest`) so
/// existing profiles migrate automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundFilter(u8);

impl BackgroundFilter {
    /// Default for new profiles. Matches the old `Darkest` enum variant.
    pub const DEFAULT: Self = Self(95);
    pub const OFF: Self = Self(0);
    pub const MAX_PERCENT: u8 = 100;

    /// Construct from a raw percentage, clamping to `0..=100`.
    #[inline]
    pub const fn from_percent(value: u8) -> Self {
        let clamped = if value > Self::MAX_PERCENT {
            Self::MAX_PERCENT
        } else {
            value
        };
        Self(clamped)
    }

    /// Construct from any signed integer, clamping to `0..=100`.
    #[inline]
    pub fn from_i32(value: i32) -> Self {
        Self::from_percent(value.clamp(0, Self::MAX_PERCENT as i32) as u8)
    }

    /// Underlying percentage value `0..=100`.
    #[inline]
    pub const fn percent(self) -> u8 {
        self.0
    }

    /// Alpha value in `0.0..=1.0` to be passed to `diffuse`.
    #[inline]
    pub fn alpha(self) -> f32 {
        self.0 as f32 / Self::MAX_PERCENT as f32
    }

    /// Convenience for branches that toggle on the "no filter" case.
    #[inline]
    pub const fn is_off(self) -> bool {
        self.0 == 0
    }
}

impl Default for BackgroundFilter {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl FromStr for BackgroundFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "off" => return Ok(Self(0)),
            "dark" => return Ok(Self(50)),
            "darker" => return Ok(Self(75)),
            "darkest" => return Ok(Self(95)),
            _ => {}
        }

        let numeric = trimmed.trim_end_matches('%').trim();
        let value: i32 = numeric
            .parse()
            .map_err(|_| format!("'{s}' is not a valid BackgroundFilter setting"))?;
        if !(0..=Self::MAX_PERCENT as i32).contains(&value) {
            return Err(format!(
                "BackgroundFilter percent {value} out of range 0..=100"
            ));
        }
        Ok(Self(value as u8))
    }
}

impl core::fmt::Display for BackgroundFilter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteSkin {
    raw: String,
}

impl NoteSkin {
    pub const DEFAULT_NAME: &'static str = "default";
    pub const CEL_NAME: &'static str = "cel";
    pub const NONE_NAME: &'static str = "__none__";

    #[inline(always)]
    fn normalize(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_ascii_lowercase())
    }

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self::from_str(raw).unwrap_or_default()
    }

    #[inline(always)]
    pub fn none_choice() -> Self {
        Self {
            raw: Self::NONE_NAME.to_string(),
        }
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    #[inline(always)]
    pub fn is_none_choice(&self) -> bool {
        self.raw == Self::NONE_NAME
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

fn normalize_graphic_key(
    raw: &str,
    folder: &str,
    stock_aliases: &[(&str, &str)],
) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("graphic setting was empty".to_string());
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok("None".to_string());
    }

    let basename = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .trim();
    if basename.eq_ignore_ascii_case("none") {
        return Ok("None".to_string());
    }

    let normalized = basename.to_ascii_lowercase();
    if let Some((_, key)) = stock_aliases
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(&normalized))
    {
        return Ok((*key).to_string());
    }

    Ok(format!("{folder}/{basename}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldJudgmentGraphic(String);

impl HoldJudgmentGraphic {
    pub const DEFAULT_KEY: &'static str = "hold_judgements/Love 1x2 (doubleres).png";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("love", Self::DEFAULT_KEY),
        ("love 1x2 (doubleres).png", Self::DEFAULT_KEY),
        (
            "hold_judgements/love 1x2 (doubleres).png",
            Self::DEFAULT_KEY,
        ),
        ("mute", "hold_judgements/mute 1x2 (doubleres).png"),
        (
            "mute 1x2 (doubleres).png",
            "hold_judgements/mute 1x2 (doubleres).png",
        ),
        (
            "hold_judgements/mute 1x2 (doubleres).png",
            "hold_judgements/mute 1x2 (doubleres).png",
        ),
        ("itg2", "hold_judgements/ITG2 1x2 (doubleres).png"),
        (
            "itg2 1x2 (doubleres).png",
            "hold_judgements/ITG2 1x2 (doubleres).png",
        ),
        (
            "hold_judgements/itg2 1x2 (doubleres).png",
            "hold_judgements/ITG2 1x2 (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "hold_judgements", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for HoldJudgmentGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for HoldJudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "hold_judgements", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for HoldJudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldMissGraphic(String);

impl HeldMissGraphic {
    pub const DEFAULT_KEY: &'static str = "None";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("love", "held_miss/Love (doubleres).png"),
        ("love (doubleres).png", "held_miss/Love (doubleres).png"),
        (
            "held_miss/love (doubleres).png",
            "held_miss/Love (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "held_miss", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for HeldMissGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for HeldMissGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "held_miss", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for HeldMissGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgmentGraphic(String);

impl JudgmentGraphic {
    pub const DEFAULT_KEY: &'static str = "judgements/Love 2x7 (doubleres).png";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("bebas", "judgements/Bebas 2x7 (doubleres).png"),
        (
            "bebas 2x7 (doubleres).png",
            "judgements/Bebas 2x7 (doubleres).png",
        ),
        (
            "judgements/bebas 2x7 (doubleres).png",
            "judgements/Bebas 2x7 (doubleres).png",
        ),
        ("censored", "judgements/Censored 1x7 (doubleres).png"),
        (
            "censored 1x7 (doubleres).png",
            "judgements/Censored 1x7 (doubleres).png",
        ),
        (
            "judgements/censored 1x7 (doubleres).png",
            "judgements/Censored 1x7 (doubleres).png",
        ),
        ("chromatic", "judgements/Chromatic 2x7 (doubleres).png"),
        (
            "chromatic 2x7 (doubleres).png",
            "judgements/Chromatic 2x7 (doubleres).png",
        ),
        (
            "judgements/chromatic 2x7 (doubleres).png",
            "judgements/Chromatic 2x7 (doubleres).png",
        ),
        ("code", "judgements/Code 2x7 (doubleres).png"),
        (
            "code 2x7 (doubleres).png",
            "judgements/Code 2x7 (doubleres).png",
        ),
        (
            "judgements/code 2x7 (doubleres).png",
            "judgements/Code 2x7 (doubleres).png",
        ),
        ("comic sans", "judgements/Comic Sans 2x7 (doubleres).png"),
        ("comicsans", "judgements/Comic Sans 2x7 (doubleres).png"),
        (
            "comic sans 2x7 (doubleres).png",
            "judgements/Comic Sans 2x7 (doubleres).png",
        ),
        (
            "judgements/comic sans 2x7 (doubleres).png",
            "judgements/Comic Sans 2x7 (doubleres).png",
        ),
        ("emoticon", "judgements/Emoticon 2x7 (doubleres).png"),
        (
            "emoticon 2x7 (doubleres).png",
            "judgements/Emoticon 2x7 (doubleres).png",
        ),
        (
            "judgements/emoticon 2x7 (doubleres).png",
            "judgements/Emoticon 2x7 (doubleres).png",
        ),
        ("focus", "judgements/Focus 2x7 (doubleres).png"),
        (
            "focus 2x7 (doubleres).png",
            "judgements/Focus 2x7 (doubleres).png",
        ),
        (
            "judgements/focus 2x7 (doubleres).png",
            "judgements/Focus 2x7 (doubleres).png",
        ),
        ("grammar", "judgements/Grammar 2x7 (doubleres).png"),
        (
            "grammar 2x7 (doubleres).png",
            "judgements/Grammar 2x7 (doubleres).png",
        ),
        (
            "judgements/grammar 2x7 (doubleres).png",
            "judgements/Grammar 2x7 (doubleres).png",
        ),
        (
            "groovenights",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "groove nights",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "groovenights 2x7 (doubleres).png",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "judgements/groovenights 2x7 (doubleres).png",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        ("itg2", "judgements/ITG2 2x7 (doubleres).png"),
        (
            "itg2 2x7 (doubleres).png",
            "judgements/ITG2 2x7 (doubleres).png",
        ),
        (
            "judgements/itg2 2x7 (doubleres).png",
            "judgements/ITG2 2x7 (doubleres).png",
        ),
        ("love", Self::DEFAULT_KEY),
        ("love 2x7 (doubleres).png", Self::DEFAULT_KEY),
        ("judgements/love 2x7 (doubleres).png", Self::DEFAULT_KEY),
        ("love chroma", "judgements/Love Chroma 2x7 (doubleres).png"),
        ("lovechroma", "judgements/Love Chroma 2x7 (doubleres).png"),
        (
            "love chroma 2x7 (doubleres).png",
            "judgements/Love Chroma 2x7 (doubleres).png",
        ),
        (
            "judgements/love chroma 2x7 (doubleres).png",
            "judgements/Love Chroma 2x7 (doubleres).png",
        ),
        ("miso", "judgements/Miso 2x7 (doubleres).png"),
        (
            "miso 2x7 (doubleres).png",
            "judgements/Miso 2x7 (doubleres).png",
        ),
        (
            "judgements/miso 2x7 (doubleres).png",
            "judgements/Miso 2x7 (doubleres).png",
        ),
        ("papyrus", "judgements/Papyrus 2x7 (doubleres).png"),
        (
            "papyrus 2x7 (doubleres).png",
            "judgements/Papyrus 2x7 (doubleres).png",
        ),
        (
            "judgements/papyrus 2x7 (doubleres).png",
            "judgements/Papyrus 2x7 (doubleres).png",
        ),
        (
            "rainbowmatic",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        (
            "rainbowmatic 2x7 (doubleres).png",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        (
            "judgements/rainbowmatic 2x7 (doubleres).png",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        ("roboto", "judgements/Roboto 2x7 (doubleres).png"),
        (
            "roboto 2x7 (doubleres).png",
            "judgements/Roboto 2x7 (doubleres).png",
        ),
        (
            "judgements/roboto 2x7 (doubleres).png",
            "judgements/Roboto 2x7 (doubleres).png",
        ),
        ("shift", "judgements/Shift 2x7 (doubleres).png"),
        (
            "shift 2x7 (doubleres).png",
            "judgements/Shift 2x7 (doubleres).png",
        ),
        (
            "judgements/shift 2x7 (doubleres).png",
            "judgements/Shift 2x7 (doubleres).png",
        ),
        ("tactics", "judgements/Tactics 2x7 (doubleres).png"),
        (
            "tactics 2x7 (doubleres).png",
            "judgements/Tactics 2x7 (doubleres).png",
        ),
        (
            "judgements/tactics 2x7 (doubleres).png",
            "judgements/Tactics 2x7 (doubleres).png",
        ),
        ("wendy", "judgements/Wendy 2x7 (doubleres).png"),
        (
            "wendy 2x7 (doubleres).png",
            "judgements/Wendy 2x7 (doubleres).png",
        ),
        (
            "judgements/wendy 2x7 (doubleres).png",
            "judgements/Wendy 2x7 (doubleres).png",
        ),
        (
            "wendy chroma",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
        ("wendychroma", "judgements/Wendy Chroma 2x7 (doubleres).png"),
        (
            "wendy chroma 2x7 (doubleres).png",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
        (
            "judgements/wendy chroma 2x7 (doubleres).png",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "judgements", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for JudgmentGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for JudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "judgements", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for JudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameplayHudPlayerSnapshot {
    pub joined: bool,
    pub guest: bool,
    pub display_name: String,
    pub avatar_texture_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameplayHudSnapshot {
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub p1: GameplayHudPlayerSnapshot,
    pub p2: GameplayHudPlayerSnapshot,
}

pub struct LocalProfileSummary {
    pub id: String,
    pub display_name: String,
    pub avatar_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastPlayed {
    pub song_music_path: Option<String>,
    pub chart_hash: Option<String>,
    pub difficulty_index: usize,
}

impl Default for LastPlayed {
    fn default() -> Self {
        Self {
            song_music_path: None,
            chart_hash: None,
            // Mirror FILE_DIFFICULTY_NAMES[2] ("Medium") as the default.
            difficulty_index: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LastPlayedCourse {
    pub course_path: Option<String>,
    pub difficulty_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_style_reports_chart_type() {
        assert_eq!(PlayStyle::Single.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Versus.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Double.chart_type(), "dance-double");
    }

    #[test]
    fn defaults_match_single_player_session() {
        assert_eq!(PLAYER_SLOTS, 2);
        assert_eq!(DEFAULT_WEIGHT_POUNDS, 120);
        assert_eq!(DEFAULT_BIRTH_YEAR, 1995);
        assert_eq!(PLAYER_INITIALS_MAX_LEN, 4);
        assert_eq!((HUD_OFFSET_MIN, HUD_OFFSET_MAX), (-250, 250));
        assert_eq!((SPACING_PERCENT_MIN, SPACING_PERCENT_MAX), (-100, 100));
        assert_eq!((MINI_PERCENT_MIN, MINI_PERCENT_MAX), (-100, 150));
        assert_eq!((NOTE_FIELD_OFFSET_X_MIN, NOTE_FIELD_OFFSET_X_MAX), (0, 50));
        assert_eq!(
            (NOTE_FIELD_OFFSET_Y_MIN, NOTE_FIELD_OFFSET_Y_MAX),
            (-50, 50)
        );
        assert_eq!((VISUAL_DELAY_MS_MIN, VISUAL_DELAY_MS_MAX), (-100, 100));
        assert_eq!((TILT_THRESHOLD_MIN_MS, TILT_THRESHOLD_MAX_MS), (0, 100));
        assert_eq!(
            (TILT_MIN_THRESHOLD_DEFAULT_MS, TILT_MAX_THRESHOLD_DEFAULT_MS),
            (0, 50)
        );
        assert_eq!(
            (
                CUSTOM_FANTASTIC_WINDOW_MIN_MS,
                CUSTOM_FANTASTIC_WINDOW_MAX_MS,
                CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS
            ),
            (1, 22, 10)
        );
        assert_eq!(PlayStyle::default(), PlayStyle::Single);
        assert_eq!(PlayMode::default(), PlayMode::Regular);
        assert_eq!(PlayerSide::default(), PlayerSide::P1);
        assert_eq!(TimingTickMode::default(), TimingTickMode::Off);
    }

    #[test]
    fn long_error_bar_intensity_clamps_to_supported_range() {
        assert!((LONG_ERROR_BAR_INTENSITY_DEFAULT - 2.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.0) - 1.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(2.0) - 2.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(0.0) - LONG_ERROR_BAR_INTENSITY_MIN).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(5.0) - LONG_ERROR_BAR_INTENSITY_MAX).abs() < 1e-6);
        assert!(
            (clamp_long_error_bar_intensity(f32::NAN) - LONG_ERROR_BAR_INTENSITY_DEFAULT).abs()
                < 1e-6
        );
        assert!(
            (clamp_long_error_bar_intensity(f32::INFINITY) - LONG_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn long_error_bar_intensity_snaps_to_quarter_step_grid() {
        assert!((clamp_long_error_bar_intensity(1.10) - 1.00).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.13) - 1.25).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.40) - 1.50).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.75) - 1.75).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.95) - 2.00).abs() < 1e-6);
        let count = ((LONG_ERROR_BAR_INTENSITY_MAX - LONG_ERROR_BAR_INTENSITY_MIN)
            / LONG_ERROR_BAR_INTENSITY_STEP)
            .round() as usize
            + 1;
        assert_eq!(count, 5);
    }

    #[test]
    fn average_error_bar_intensity_clamps_to_supported_range() {
        assert!((AVERAGE_ERROR_BAR_INTENSITY_DEFAULT - 1.0).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.0) - 1.0).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(2.0) - 2.0).abs() < 1e-6);
        assert!(
            (clamp_average_error_bar_intensity(0.0) - AVERAGE_ERROR_BAR_INTENSITY_MIN).abs() < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(5.0) - AVERAGE_ERROR_BAR_INTENSITY_MAX).abs() < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(f32::NAN) - AVERAGE_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(f32::INFINITY)
                - AVERAGE_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn average_error_bar_intensity_snaps_to_quarter_step_grid() {
        assert!((clamp_average_error_bar_intensity(1.10) - 1.00).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.13) - 1.25).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.40) - 1.50).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.75) - 1.75).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.95) - 2.00).abs() < 1e-6);
        let count = ((AVERAGE_ERROR_BAR_INTENSITY_MAX - AVERAGE_ERROR_BAR_INTENSITY_MIN)
            / AVERAGE_ERROR_BAR_INTENSITY_STEP)
            .round() as usize
            + 1;
        assert_eq!(count, 5);
    }

    #[test]
    fn average_error_bar_interval_clamps_to_supported_range() {
        assert_eq!(AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT, 400);
        assert_eq!(clamp_average_error_bar_interval_ms(100), 100);
        assert_eq!(clamp_average_error_bar_interval_ms(2000), 2000);
        assert_eq!(
            clamp_average_error_bar_interval_ms(0),
            AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        );
        assert_eq!(
            clamp_average_error_bar_interval_ms(4000),
            AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
        );
    }

    #[test]
    fn average_error_bar_interval_snaps_to_100ms_step_grid() {
        assert_eq!(AVERAGE_ERROR_BAR_INTERVAL_MS_STEP, 100);
        assert_eq!(clamp_average_error_bar_interval_ms(149), 100);
        assert_eq!(clamp_average_error_bar_interval_ms(150), 200);
        assert_eq!(clamp_average_error_bar_interval_ms(349), 300);
        assert_eq!(clamp_average_error_bar_interval_ms(350), 400);
        assert_eq!(clamp_average_error_bar_interval_ms(1951), 2000);
    }

    #[test]
    fn profile_window_clamps_keep_supported_ranges() {
        assert_eq!(clamp_tilt_threshold_ms(0), 0);
        assert_eq!(clamp_tilt_threshold_ms(50), 50);
        assert_eq!(clamp_tilt_threshold_ms(101), TILT_THRESHOLD_MAX_MS);
        assert_eq!(
            clamp_custom_fantastic_window_ms(0),
            CUSTOM_FANTASTIC_WINDOW_MIN_MS
        );
        assert_eq!(clamp_custom_fantastic_window_ms(10), 10);
        assert_eq!(
            clamp_custom_fantastic_window_ms(23),
            CUSTOM_FANTASTIC_WINDOW_MAX_MS
        );
        assert_eq!(
            clamp_long_error_bar_threshold_ms(0),
            LONG_ERROR_BAR_THRESHOLD_MS_MIN
        );
        assert_eq!(
            clamp_long_error_bar_threshold_ms(99),
            LONG_ERROR_BAR_THRESHOLD_MS_MAX
        );
        assert_eq!(
            clamp_long_error_bar_min_samples(0),
            LONG_ERROR_BAR_MIN_SAMPLES_MIN
        );
        assert_eq!(
            clamp_long_error_bar_min_samples(99),
            LONG_ERROR_BAR_MIN_SAMPLES_MAX
        );
        assert_eq!(
            clamp_long_error_bar_buffer_cap(0),
            LONG_ERROR_BAR_BUFFER_CAP_MIN
        );
        assert_eq!(
            clamp_long_error_bar_buffer_cap(999),
            LONG_ERROR_BAR_BUFFER_CAP_MAX
        );
    }

    #[test]
    fn hud_player_snapshot_defaults_to_guestless_unjoined() {
        let snapshot = GameplayHudPlayerSnapshot::default();
        assert!(!snapshot.joined);
        assert!(!snapshot.guest);
        assert_eq!(snapshot.display_name, "");
        assert_eq!(snapshot.avatar_texture_key, None);
    }

    #[test]
    fn last_played_defaults_to_medium_song_and_empty_course() {
        let last_song = LastPlayed::default();
        assert_eq!(last_song.song_music_path, None);
        assert_eq!(last_song.chart_hash, None);
        assert_eq!(last_song.difficulty_index, 2);

        let last_course = LastPlayedCourse::default();
        assert_eq!(last_course.course_path, None);
        assert_eq!(last_course.difficulty_name, None);
    }

    #[test]
    fn hide_light_type_round_trips() {
        for setting in [
            HideLightType::NoHideLights,
            HideLightType::HideAllLights,
            HideLightType::HideMarqueeLights,
            HideLightType::HideBassLights,
        ] {
            assert_eq!(setting.to_string().parse::<HideLightType>(), Ok(setting));
        }
        assert!(HideLightType::from_str("unknown").is_err());
    }

    #[test]
    fn perspective_round_trips_and_reports_tilt_skew() {
        for (setting, skew) in [
            (Perspective::Overhead, (0.0, 0.0)),
            (Perspective::Hallway, (-1.0, 0.0)),
            (Perspective::Distant, (1.0, 0.0)),
            (Perspective::Incoming, (-1.0, 1.0)),
            (Perspective::Space, (1.0, 1.0)),
        ] {
            assert_eq!(setting.to_string().parse::<Perspective>(), Ok(setting));
            assert_eq!(setting.tilt_skew(), skew);
        }
        assert!(Perspective::from_str("flat").is_err());
    }

    #[test]
    fn turn_option_round_trips_and_accepts_aliases() {
        for setting in [
            TurnOption::None,
            TurnOption::Mirror,
            TurnOption::Left,
            TurnOption::Right,
            TurnOption::LRMirror,
            TurnOption::UDMirror,
            TurnOption::Shuffle,
            TurnOption::Blender,
            TurnOption::Random,
        ] {
            assert_eq!(setting.to_string().parse::<TurnOption>(), Ok(setting));
        }
        assert_eq!(TurnOption::from_str("NoTurn"), Ok(TurnOption::None));
        assert_eq!(
            TurnOption::from_str("super shuffle"),
            Ok(TurnOption::Blender)
        );
        assert_eq!(
            TurnOption::from_str("hyper shuffle"),
            Ok(TurnOption::Random)
        );
        assert!(TurnOption::from_str("up").is_err());
    }

    #[test]
    fn scroll_option_parses_and_formats_combined_flags() {
        for setting in [
            ScrollOption::Normal,
            ScrollOption::Reverse,
            ScrollOption::Split,
            ScrollOption::Alternate,
            ScrollOption::Cross,
            ScrollOption::Centered,
        ] {
            assert_eq!(setting.to_string().parse::<ScrollOption>(), Ok(setting));
        }

        let combined = ScrollOption::from_str("Reverse+Cross Centered").unwrap();
        assert!(combined.contains(ScrollOption::Reverse));
        assert!(combined.contains(ScrollOption::Cross));
        assert!(combined.contains(ScrollOption::Centered));
        assert_eq!(combined.to_string(), "Reverse+Cross+Centered");

        assert_eq!(
            ScrollOption::from_str("Normal,Reverse"),
            Ok(ScrollOption::Reverse)
        );
        assert!(ScrollOption::from_str("").is_err());
        assert!(ScrollOption::from_str("hidden").is_err());
    }

    #[test]
    fn combo_mode_round_trips() {
        for setting in [ComboMode::FullCombo, ComboMode::CurrentCombo] {
            assert_eq!(setting.to_string().parse::<ComboMode>(), Ok(setting));
        }
        assert!(ComboMode::from_str("sessioncombo").is_err());
    }

    #[test]
    fn combo_colors_round_trips() {
        for setting in [
            ComboColors::Glow,
            ComboColors::Solid,
            ComboColors::Rainbow,
            ComboColors::RainbowScroll,
            ComboColors::None,
        ] {
            assert_eq!(setting.to_string().parse::<ComboColors>(), Ok(setting));
        }
        assert!(ComboColors::from_str("flashing").is_err());
    }

    #[test]
    fn combo_font_round_trips_and_accepts_aliases() {
        for setting in [
            ComboFont::Wendy,
            ComboFont::ArialRounded,
            ComboFont::Asap,
            ComboFont::BebasNeue,
            ComboFont::SourceCode,
            ComboFont::Work,
            ComboFont::WendyCursed,
            ComboFont::Mega,
            ComboFont::None,
        ] {
            assert_eq!(setting.to_string().parse::<ComboFont>(), Ok(setting));
        }
        assert_eq!(ComboFont::from_str("bebasneue"), Ok(ComboFont::BebasNeue));
        assert_eq!(ComboFont::from_str("sourcecode"), Ok(ComboFont::SourceCode));
        assert_eq!(
            ComboFont::from_str("wendycursed"),
            Ok(ComboFont::WendyCursed)
        );
        assert!(ComboFont::from_str("comic sans").is_err());
    }

    #[test]
    fn target_score_setting_parses_legacy_forms() {
        for (raw, setting) in [
            ("cminus", TargetScoreSetting::CMinus),
            ("c", TargetScoreSetting::C),
            ("cplus", TargetScoreSetting::CPlus),
            ("bminus", TargetScoreSetting::BMinus),
            ("b", TargetScoreSetting::B),
            ("bplus", TargetScoreSetting::BPlus),
            ("aminus", TargetScoreSetting::AMinus),
            ("a", TargetScoreSetting::A),
            ("aplus", TargetScoreSetting::APlus),
            ("sminus", TargetScoreSetting::SMinus),
            ("", TargetScoreSetting::S),
            ("s", TargetScoreSetting::S),
            ("splus", TargetScoreSetting::SPlus),
            ("machine", TargetScoreSetting::MachineBest),
            ("machinebest", TargetScoreSetting::MachineBest),
            ("personal", TargetScoreSetting::PersonalBest),
            ("personalbest", TargetScoreSetting::PersonalBest),
        ] {
            assert_eq!(TargetScoreSetting::from_str(raw), Ok(setting));
        }

        // Preserve the existing punctuation-stripping parser behavior.
        assert_eq!(
            TargetScoreSetting::from_str("C-"),
            Ok(TargetScoreSetting::C)
        );
        assert_eq!(
            TargetScoreSetting::from_str("A+"),
            Ok(TargetScoreSetting::A)
        );
        assert_eq!(
            TargetScoreSetting::from_str("S-"),
            Ok(TargetScoreSetting::S)
        );
        assert!(TargetScoreSetting::from_str("ss").is_err());
    }

    #[test]
    fn error_bar_style_round_trips() {
        for setting in [
            ErrorBarStyle::None,
            ErrorBarStyle::Colorful,
            ErrorBarStyle::Monochrome,
            ErrorBarStyle::Text,
            ErrorBarStyle::Highlight,
            ErrorBarStyle::Average,
        ] {
            assert_eq!(setting.to_string().parse::<ErrorBarStyle>(), Ok(setting));
        }
        assert!(ErrorBarStyle::from_str("split").is_err());
    }

    #[test]
    fn live_timing_stats_mask_layout_is_stable() {
        assert_eq!(LiveTimingStatsMask::MEAN.bits(), 1 << 0);
        assert_eq!(LiveTimingStatsMask::MEAN_ABS.bits(), 1 << 1);
        assert_eq!(LiveTimingStatsMask::MAX.bits(), 1 << 2);
        assert_eq!(LiveTimingStatsMask::all().bits(), 0b0000_0111);
        assert_eq!(
            LiveTimingStatsMask::from_bits_truncate(u8::MAX),
            LiveTimingStatsMask::all()
        );
    }

    #[test]
    fn error_bar_mask_layout_is_stable() {
        assert_eq!(ErrorBarMask::COLORFUL.bits(), 1 << 0);
        assert_eq!(ErrorBarMask::MONOCHROME.bits(), 1 << 1);
        assert_eq!(ErrorBarMask::TEXT.bits(), 1 << 2);
        assert_eq!(ErrorBarMask::HIGHLIGHT.bits(), 1 << 3);
        assert_eq!(ErrorBarMask::AVERAGE.bits(), 1 << 4);
        assert_eq!(ErrorBarMask::all().bits(), 0b0001_1111);
        assert_eq!(
            ErrorBarMask::from_bits_truncate(u8::MAX),
            ErrorBarMask::all()
        );
    }

    #[test]
    fn error_bar_helpers_roundtrip_through_mask() {
        let mask = error_bar_mask_from_style(ErrorBarStyle::Colorful, true);
        assert!(mask.contains(ErrorBarMask::COLORFUL));
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);
        assert!(error_bar_text_from_mask(mask));

        let mask = ErrorBarMask::COLORFUL | ErrorBarMask::MONOCHROME;
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);

        let mask = error_bar_mask_from_style(ErrorBarStyle::Text, false);
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert!(!mask.contains(ErrorBarMask::COLORFUL));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(error_bar_text_from_mask(mask));

        let mask = error_bar_mask_from_style(ErrorBarStyle::None, false);
        assert!(mask.is_empty());
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(!error_bar_text_from_mask(mask));
    }

    #[test]
    fn appearance_effects_mask_layout_is_stable() {
        assert_eq!(AppearanceEffectsMask::HIDDEN.bits(), 1 << 0);
        assert_eq!(AppearanceEffectsMask::SUDDEN.bits(), 1 << 1);
        assert_eq!(AppearanceEffectsMask::STEALTH.bits(), 1 << 2);
        assert_eq!(AppearanceEffectsMask::BLINK.bits(), 1 << 3);
        assert_eq!(AppearanceEffectsMask::RANDOM_VANISH.bits(), 1 << 4);
        assert_eq!(AppearanceEffectsMask::all().bits(), 0b0001_1111);
        assert_eq!(
            AppearanceEffectsMask::from_bits_truncate(u8::MAX),
            AppearanceEffectsMask::all()
        );
    }

    #[test]
    fn accel_effects_mask_layout_is_stable() {
        assert_eq!(AccelEffectsMask::BOOST.bits(), 1 << 0);
        assert_eq!(AccelEffectsMask::BRAKE.bits(), 1 << 1);
        assert_eq!(AccelEffectsMask::WAVE.bits(), 1 << 2);
        assert_eq!(AccelEffectsMask::EXPAND.bits(), 1 << 3);
        assert_eq!(AccelEffectsMask::BOOMERANG.bits(), 1 << 4);
        assert_eq!(AccelEffectsMask::all().bits(), 0b0001_1111);
        assert_eq!(
            AccelEffectsMask::from_bits_truncate(u8::MAX),
            AccelEffectsMask::all()
        );
    }

    #[test]
    fn holds_mask_layout_is_stable() {
        assert_eq!(HoldsMask::PLANTED.bits(), 1 << 0);
        assert_eq!(HoldsMask::FLOORED.bits(), 1 << 1);
        assert_eq!(HoldsMask::TWISTER.bits(), 1 << 2);
        assert_eq!(HoldsMask::NO_ROLLS.bits(), 1 << 3);
        assert_eq!(HoldsMask::HOLDS_TO_ROLLS.bits(), 1 << 4);
        assert_eq!(HoldsMask::all().bits(), 0b0001_1111);
        assert_eq!(HoldsMask::from_bits_truncate(u8::MAX), HoldsMask::all());
    }

    #[test]
    fn visual_effects_mask_layout_is_stable() {
        assert_eq!(VisualEffectsMask::DRUNK.bits(), 1 << 0);
        assert_eq!(VisualEffectsMask::DIZZY.bits(), 1 << 1);
        assert_eq!(VisualEffectsMask::CONFUSION.bits(), 1 << 2);
        assert_eq!(VisualEffectsMask::BIG.bits(), 1 << 3);
        assert_eq!(VisualEffectsMask::FLIP.bits(), 1 << 4);
        assert_eq!(VisualEffectsMask::INVERT.bits(), 1 << 5);
        assert_eq!(VisualEffectsMask::TORNADO.bits(), 1 << 6);
        assert_eq!(VisualEffectsMask::TIPSY.bits(), 1 << 7);
        assert_eq!(VisualEffectsMask::BUMPY.bits(), 1 << 8);
        assert_eq!(VisualEffectsMask::BEAT.bits(), 1 << 9);
        assert_eq!(VisualEffectsMask::all().bits(), 0b11_1111_1111);
        assert_eq!(
            VisualEffectsMask::from_bits_truncate(u16::MAX),
            VisualEffectsMask::all()
        );
    }

    #[test]
    fn insert_mask_layout_is_stable() {
        assert_eq!(InsertMask::WIDE.bits(), 1 << 0);
        assert_eq!(InsertMask::BIG.bits(), 1 << 1);
        assert_eq!(InsertMask::QUICK.bits(), 1 << 2);
        assert_eq!(InsertMask::BMRIZE.bits(), 1 << 3);
        assert_eq!(InsertMask::SKIPPY.bits(), 1 << 4);
        assert_eq!(InsertMask::ECHO.bits(), 1 << 5);
        assert_eq!(InsertMask::STOMP.bits(), 1 << 6);
        assert_eq!(InsertMask::all().bits(), 0b0111_1111);
        assert_eq!(InsertMask::from_bits_truncate(u8::MAX), InsertMask::all());
    }

    #[test]
    fn remove_mask_layout_is_stable() {
        assert_eq!(RemoveMask::LITTLE.bits(), 1 << 0);
        assert_eq!(RemoveMask::NO_MINES.bits(), 1 << 1);
        assert_eq!(RemoveMask::NO_HOLDS.bits(), 1 << 2);
        assert_eq!(RemoveMask::NO_JUMPS.bits(), 1 << 3);
        assert_eq!(RemoveMask::NO_HANDS.bits(), 1 << 4);
        assert_eq!(RemoveMask::NO_QUADS.bits(), 1 << 5);
        assert_eq!(RemoveMask::NO_LIFTS.bits(), 1 << 6);
        assert_eq!(RemoveMask::NO_FAKES.bits(), 1 << 7);
        assert_eq!(RemoveMask::all().bits(), u8::MAX);
        assert_eq!(RemoveMask::from_bits_truncate(u8::MAX), RemoveMask::all());
    }

    #[test]
    fn tap_explosion_mask_layout_is_stable() {
        assert_eq!(TapExplosionMask::FANTASTIC.bits(), 1 << 0);
        assert_eq!(TapExplosionMask::EXCELLENT.bits(), 1 << 1);
        assert_eq!(TapExplosionMask::GREAT.bits(), 1 << 2);
        assert_eq!(TapExplosionMask::DECENT.bits(), 1 << 3);
        assert_eq!(TapExplosionMask::WAY_OFF.bits(), 1 << 4);
        assert_eq!(TapExplosionMask::HELD.bits(), 1 << 5);
        assert_eq!(TapExplosionMask::MISS.bits(), 1 << 6);
        assert_eq!(TapExplosionMask::HOLDING.bits(), 1 << 7);
        assert_eq!(TapExplosionMask::all().bits(), u8::MAX);
        assert_eq!(
            TapExplosionMask::from_bits_truncate(u8::MAX),
            TapExplosionMask::all()
        );
    }

    #[test]
    fn attack_mode_round_trips() {
        for setting in [AttackMode::Off, AttackMode::On, AttackMode::Random] {
            assert_eq!(setting.to_string().parse::<AttackMode>(), Ok(setting));
        }
        assert_eq!(AttackMode::from_str("NoAttacks"), Ok(AttackMode::Off));
        assert_eq!(AttackMode::from_str("normal"), Ok(AttackMode::On));
        assert_eq!(
            AttackMode::from_str("random attacks"),
            Ok(AttackMode::Random)
        );
        assert!(AttackMode::from_str("chaos").is_err());
    }

    #[test]
    fn scatterplot_max_window_round_trips() {
        for setting in [
            ScatterplotMaxWindow::Off,
            ScatterplotMaxWindow::Fantastic,
            ScatterplotMaxWindow::Excellent,
            ScatterplotMaxWindow::Great,
        ] {
            assert_eq!(
                setting.to_string().parse::<ScatterplotMaxWindow>(),
                Ok(setting)
            );
        }
        assert_eq!(
            ScatterplotMaxWindow::from_str("autoscale"),
            Ok(ScatterplotMaxWindow::Off)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("fa"),
            Ok(ScatterplotMaxWindow::Fantastic)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("excellent max"),
            Ok(ScatterplotMaxWindow::Excellent)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("greatmax"),
            Ok(ScatterplotMaxWindow::Great)
        );
        assert!(ScatterplotMaxWindow::from_str("decent").is_err());
    }

    #[test]
    fn life_meter_type_round_trips() {
        for setting in [
            LifeMeterType::Standard,
            LifeMeterType::Surround,
            LifeMeterType::Vertical,
        ] {
            assert_eq!(setting.to_string().parse::<LifeMeterType>(), Ok(setting));
        }
        assert_eq!(LifeMeterType::from_str(""), Ok(LifeMeterType::Standard));
        assert!(LifeMeterType::from_str("horizontal").is_err());
    }

    #[test]
    fn error_bar_trim_round_trips() {
        for setting in [
            ErrorBarTrim::Off,
            ErrorBarTrim::Fantastic,
            ErrorBarTrim::Excellent,
            ErrorBarTrim::Great,
        ] {
            assert_eq!(setting.to_string().parse::<ErrorBarTrim>(), Ok(setting));
        }
        assert!(ErrorBarTrim::from_str("decent").is_err());
    }

    #[test]
    fn timing_windows_option_round_trips_and_reports_disabled_windows() {
        for (setting, disabled) in [
            (TimingWindowsOption::None, [false; 5]),
            (
                TimingWindowsOption::WayOffs,
                [false, false, false, false, true],
            ),
            (
                TimingWindowsOption::DecentsAndWayOffs,
                [false, false, false, true, true],
            ),
            (
                TimingWindowsOption::FantasticsAndExcellents,
                [true, true, false, false, false],
            ),
        ] {
            assert_eq!(
                setting.to_string().parse::<TimingWindowsOption>(),
                Ok(setting)
            );
            assert_eq!(setting.disabled_windows(), disabled);
        }
        assert_eq!(
            TimingWindowsOption::from_str("decents and way offs"),
            Ok(TimingWindowsOption::DecentsAndWayOffs)
        );
        assert_eq!(
            TimingWindowsOption::from_str("fantastics+excellents"),
            Ok(TimingWindowsOption::FantasticsAndExcellents)
        );
        assert!(TimingWindowsOption::from_str("misses").is_err());
    }

    #[test]
    fn data_visualizations_round_trips_and_accepts_aliases() {
        for setting in [
            DataVisualizations::None,
            DataVisualizations::TargetScoreGraph,
            DataVisualizations::StepStatistics,
        ] {
            assert_eq!(
                setting.to_string().parse::<DataVisualizations>(),
                Ok(setting)
            );
        }
        assert_eq!(
            DataVisualizations::from_str("target"),
            Ok(DataVisualizations::TargetScoreGraph)
        );
        assert_eq!(
            DataVisualizations::from_str("stepstats"),
            Ok(DataVisualizations::StepStatistics)
        );
        assert!(DataVisualizations::from_str("lanes").is_err());
    }

    #[test]
    fn measure_counter_round_trips_and_reports_stream_thresholds() {
        for (setting, threshold, multiplier) in [
            (MeasureCounter::None, None, 1.0),
            (MeasureCounter::Eighth, Some(8), 1.0),
            (MeasureCounter::Twelfth, Some(12), 1.0),
            (MeasureCounter::Sixteenth, Some(16), 1.0),
            (MeasureCounter::TwentyFourth, Some(24), 1.5),
            (MeasureCounter::ThirtySecond, Some(32), 2.0),
        ] {
            assert_eq!(setting.to_string().parse::<MeasureCounter>(), Ok(setting));
            assert_eq!(setting.notes_threshold(), threshold);
            assert_eq!(setting.multiplier(), multiplier);
        }
        assert!(MeasureCounter::from_str("quarter").is_err());
    }

    #[test]
    fn measure_lines_round_trips() {
        for setting in [
            MeasureLines::Off,
            MeasureLines::Measure,
            MeasureLines::Quarter,
            MeasureLines::Eighth,
        ] {
            assert_eq!(setting.to_string().parse::<MeasureLines>(), Ok(setting));
        }
        assert!(MeasureLines::from_str("sixteenth").is_err());
    }

    #[test]
    fn mini_indicator_round_trips_and_accepts_aliases() {
        for setting in [
            MiniIndicator::None,
            MiniIndicator::SubtractiveScoring,
            MiniIndicator::PredictiveScoring,
            MiniIndicator::PaceScoring,
            MiniIndicator::RivalScoring,
            MiniIndicator::Pacemaker,
            MiniIndicator::StreamProg,
        ] {
            assert_eq!(setting.to_string().parse::<MiniIndicator>(), Ok(setting));
        }
        assert_eq!(
            MiniIndicator::from_str("subtractive"),
            Ok(MiniIndicator::SubtractiveScoring)
        );
        assert_eq!(
            MiniIndicator::from_str("stream progress"),
            Ok(MiniIndicator::StreamProg)
        );
        assert!(MiniIndicator::from_str("combo").is_err());
    }

    #[test]
    fn mini_indicator_score_type_round_trips_and_accepts_hex_alias() {
        for setting in [
            MiniIndicatorScoreType::Itg,
            MiniIndicatorScoreType::Ex,
            MiniIndicatorScoreType::HardEx,
        ] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorScoreType>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorScoreType::from_str("hex"),
            Ok(MiniIndicatorScoreType::HardEx)
        );
        assert!(MiniIndicatorScoreType::from_str("percent").is_err());
    }

    #[test]
    fn mini_indicator_size_round_trips_and_accepts_big_alias() {
        for setting in [MiniIndicatorSize::Default, MiniIndicatorSize::Large] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorSize>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorSize::from_str("big"),
            Ok(MiniIndicatorSize::Large)
        );
        assert!(MiniIndicatorSize::from_str("small").is_err());
    }

    #[test]
    fn mini_indicator_color_round_trips() {
        for setting in [MiniIndicatorColor::Default, MiniIndicatorColor::Detailed] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorColor>(),
                Ok(setting)
            );
        }
        assert!(MiniIndicatorColor::from_str("rainbow").is_err());
    }

    #[test]
    fn background_filter_default_matches_legacy_darkest_value() {
        assert_eq!(BackgroundFilter::default(), BackgroundFilter::DEFAULT);
        assert_eq!(BackgroundFilter::default().percent(), 95);
    }

    #[test]
    fn background_filter_from_percent_clamps_above_max() {
        assert_eq!(BackgroundFilter::from_percent(200).percent(), 100);
        assert_eq!(BackgroundFilter::from_i32(-5).percent(), 0);
        assert_eq!(BackgroundFilter::from_i32(250).percent(), 100);
    }

    #[test]
    fn background_filter_alpha_maps_percent_to_unit_range() {
        assert!((BackgroundFilter::from_percent(0).alpha() - 0.0).abs() < 1e-6);
        assert!((BackgroundFilter::from_percent(100).alpha() - 1.0).abs() < 1e-6);
        assert!((BackgroundFilter::from_percent(50).alpha() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn background_filter_migrates_legacy_enum_labels() {
        assert_eq!(
            BackgroundFilter::from_str("Off").unwrap(),
            BackgroundFilter::OFF
        );
        assert_eq!(
            BackgroundFilter::from_str("Dark").unwrap(),
            BackgroundFilter::from_percent(50)
        );
        assert_eq!(
            BackgroundFilter::from_str("DARKER").unwrap(),
            BackgroundFilter::from_percent(75)
        );
        assert_eq!(
            BackgroundFilter::from_str("darkest").unwrap(),
            BackgroundFilter::from_percent(95)
        );
    }

    #[test]
    fn background_filter_parses_numeric_with_optional_percent_suffix() {
        assert_eq!(
            BackgroundFilter::from_str("0").unwrap(),
            BackgroundFilter::OFF
        );
        assert_eq!(
            BackgroundFilter::from_str("42").unwrap(),
            BackgroundFilter::from_percent(42)
        );
        assert_eq!(
            BackgroundFilter::from_str("42%").unwrap(),
            BackgroundFilter::from_percent(42)
        );
        assert_eq!(
            BackgroundFilter::from_str("100").unwrap(),
            BackgroundFilter::from_percent(100)
        );
    }

    #[test]
    fn background_filter_rejects_out_of_range_or_garbage() {
        assert!(BackgroundFilter::from_str("101").is_err());
        assert!(BackgroundFilter::from_str("-1").is_err());
        assert!(BackgroundFilter::from_str("Dimmer").is_err());
        assert!(BackgroundFilter::from_str("").is_err());
    }

    #[test]
    fn background_filter_display_round_trips_through_from_str() {
        for v in [0u8, 1, 25, 50, 75, 95, 100] {
            let filter = BackgroundFilter::from_percent(v);
            let s = filter.to_string();
            let parsed = BackgroundFilter::from_str(&s).expect("must round-trip");
            assert_eq!(parsed, filter);
        }
    }

    #[test]
    fn noteskin_normalizes_names_and_preserves_none_choice() {
        assert_eq!(NoteSkin::default().as_str(), NoteSkin::CEL_NAME);
        assert_eq!(NoteSkin::new(" Default ").as_str(), NoteSkin::DEFAULT_NAME);
        assert_eq!(NoteSkin::none_choice().as_str(), NoteSkin::NONE_NAME);
        assert!(NoteSkin::from_str("").is_err());
    }

    #[test]
    fn graphic_settings_normalize_stock_aliases_and_none() {
        assert_eq!(
            JudgmentGraphic::new("Wendy").as_str(),
            "judgements/Wendy 2x7 (doubleres).png"
        );
        assert_eq!(
            HoldJudgmentGraphic::new("itg2").as_str(),
            "hold_judgements/ITG2 1x2 (doubleres).png"
        );
        assert_eq!(HeldMissGraphic::new("none").as_str(), "None");
        assert_eq!(
            JudgmentGraphic::from_str("custom.png").unwrap().as_str(),
            "judgements/custom.png"
        );
        assert!(HoldJudgmentGraphic::from_str("").is_err());
    }
}
