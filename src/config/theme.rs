use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakdownStyle {
    Sl,
    Sn,
}

impl BreakdownStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sl => "SL",
            Self::Sn => "SN",
        }
    }
}

impl FromStr for BreakdownStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sl" => Ok(Self::Sl),
            "sn" => Ok(Self::Sn),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultFailType {
    Immediate,
    ImmediateContinue,
}

impl DefaultFailType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "Immediate",
            Self::ImmediateContinue => "ImmediateContinue",
        }
    }
}

impl FromStr for DefaultFailType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "immediate" => Ok(Self::Immediate),
            "immediatecontinue" | "immediate_continue" => Ok(Self::ImmediateContinue),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMusicPatternInfoMode {
    Tech,
    Stamina,
    Auto,
}

impl SelectMusicPatternInfoMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tech => "Tech",
            Self::Stamina => "Stamina",
            Self::Auto => "Auto",
        }
    }
}

impl FromStr for SelectMusicPatternInfoMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tech" => Ok(Self::Tech),
            "stamina" => Ok(Self::Stamina),
            "auto" => Ok(Self::Auto),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMusicItlRankMode {
    None,
    Chart,
    Overall,
}

impl SelectMusicItlRankMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Chart => "Chart",
            Self::Overall => "Overall",
        }
    }
}

impl FromStr for SelectMusicItlRankMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "none" | "off" | "disabled" | "disable" => Ok(Self::None),
            "chart" | "chartrank" | "leaderboard" | "leaderrank" => Ok(Self::Chart),
            "overall" | "overallrank" | "zmod" | "tournament" => Ok(Self::Overall),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMusicItlWheelMode {
    Off,
    Score,
    PointsAndScore,
}

impl SelectMusicItlWheelMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Score => "Score",
            Self::PointsAndScore => "PointsAndScore",
        }
    }
}

impl FromStr for SelectMusicItlWheelMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "off" | "disable" | "disabled" => Ok(Self::Off),
            "score" | "scores" => Ok(Self::Score),
            "pointsandscore" | "pointsscore" | "points" => Ok(Self::PointsAndScore),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMusicWheelStyle {
    Itg,
    Iidx,
}

impl SelectMusicWheelStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Itg => "ITG",
            Self::Iidx => "IIDX",
        }
    }
}

impl FromStr for SelectMusicWheelStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "itg" => Ok(Self::Itg),
            "iidx" => Ok(Self::Iidx),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewPackMode {
    Disabled,
    OpenPack,
    HasScore,
}

impl NewPackMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::OpenPack => "OpenPack",
            Self::HasScore => "HasScore",
        }
    }
}

impl FromStr for NewPackMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "disabled" | "disable" | "off" => Ok(Self::Disabled),
            "openpack" | "open" => Ok(Self::OpenPack),
            "hasscore" | "score" | "scored" => Ok(Self::HasScore),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMusicScoreboxPlacement {
    Auto,
    StepPane,
}

impl SelectMusicScoreboxPlacement {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::StepPane => "StepPane",
        }
    }
}

impl FromStr for SelectMusicScoreboxPlacement {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "auto" => Ok(Self::Auto),
            "steppane" | "pane" => Ok(Self::StepPane),
            _ => Err(()),
        }
    }
}

pub const AUTO_SS_NUM_FLAGS: usize = 5;
pub const AUTO_SS_FLAG_NAMES: [&str; AUTO_SS_NUM_FLAGS] =
    ["PBs", "Fails", "Clears", "Quads", "Quints"];
pub const AUTO_SS_PBS: u8 = 1 << 0;
pub const AUTO_SS_FAILS: u8 = 1 << 1;
pub const AUTO_SS_CLEARS: u8 = 1 << 2;
pub const AUTO_SS_QUADS: u8 = 1 << 3;
pub const AUTO_SS_QUINTS: u8 = 1 << 4;

#[inline(always)]
pub const fn auto_screenshot_bit(idx: usize) -> u8 {
    match idx {
        0 => AUTO_SS_PBS,
        1 => AUTO_SS_FAILS,
        2 => AUTO_SS_CLEARS,
        3 => AUTO_SS_QUADS,
        4 => AUTO_SS_QUINTS,
        _ => 0,
    }
}

pub fn auto_screenshot_mask_to_str(mask: u8) -> String {
    if mask == 0 {
        return "Off".to_string();
    }
    let mut parts = Vec::with_capacity(AUTO_SS_NUM_FLAGS);
    for (idx, name) in AUTO_SS_FLAG_NAMES.iter().enumerate() {
        if (mask & auto_screenshot_bit(idx)) != 0 {
            parts.push(*name);
        }
    }
    parts.join("|")
}

pub fn auto_screenshot_mask_from_str(s: &str) -> u8 {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("off") {
        return 0;
    }
    let mut mask = 0u8;
    for part in trimmed.split('|') {
        match part.trim().to_ascii_lowercase().as_str() {
            "pbs" => mask |= AUTO_SS_PBS,
            "fails" => mask |= AUTO_SS_FAILS,
            "clears" => mask |= AUTO_SS_CLEARS,
            "quads" => mask |= AUTO_SS_QUADS,
            "quints" => mask |= AUTO_SS_QUINTS,
            _ => {}
        }
    }
    mask
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncGraphMode {
    Frequency,
    BeatIndex,
    PostKernelFingerprint,
}

impl SyncGraphMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Frequency => "Frequency",
            Self::BeatIndex => "BeatIndex",
            Self::PostKernelFingerprint => "PostKernelFingerprint",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Frequency => "Frequency",
            Self::BeatIndex => "Beat index",
            Self::PostKernelFingerprint => "Post-kernel fingerprint",
        }
    }
}

impl FromStr for SyncGraphMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "frequency" => Ok(Self::Frequency),
            "beatindex" | "beatdigest" | "digest" => Ok(Self::BeatIndex),
            "postkernelfingerprint" | "postkernel" | "fingerprint" => {
                Ok(Self::PostKernelFingerprint)
            }
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachinePreferredPlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl MachinePreferredPlayStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Versus => "Versus",
            Self::Double => "Double",
        }
    }
}

impl FromStr for MachinePreferredPlayStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "single" | "1player" | "oneplayer" => Ok(Self::Single),
            "versus" | "2player" | "2players" | "twoplayer" | "twoplayers" => Ok(Self::Versus),
            "double" => Ok(Self::Double),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachinePreferredPlayMode {
    #[default]
    Regular,
    Marathon,
}

impl MachinePreferredPlayMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "Regular",
            Self::Marathon => "Marathon",
        }
    }
}

impl FromStr for MachinePreferredPlayMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "regular" => Ok(Self::Regular),
            "marathon" => Ok(Self::Marathon),
            _ => Err(()),
        }
    }
}

/// Machine-wide font preference, ported from Simply Love's `ThemeFont` pref.
///
/// Controls which font is used for the Bold / Header / Footer / numbers /
/// ScreenEval roles in static UI text. The Normal (body) role stays Miso
/// regardless of this pref -- matches SL's `Mega Normal.redir ->
/// Miso/_miso light`.
///
/// Gameplay-side fonts (combo, judgment, hold judgment) are not affected;
/// those follow each player's `ComboFont` profile pref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MachineFont {
    /// Default; Bold/Header/Footer = Wendy, numbers = Wendy monospace.
    #[default]
    Common,
    /// Bold/Header/Footer = Mega alphanumeric, numbers = Mega monospace.
    Mega,
}

impl MachineFont {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Common => "Common",
            Self::Mega => "Mega",
        }
    }
}

impl FromStr for MachineFont {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "common" | "wendy" => Ok(Self::Common),
            "mega" => Ok(Self::Mega),
            _ => Err(()),
        }
    }
}

pub const MACHINE_FONT_VARIANTS: [MachineFont; 2] = [MachineFont::Common, MachineFont::Mega];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameFlag {
    Dance,
}

impl GameFlag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dance => "dance",
        }
    }
}

impl FromStr for GameFlag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dance" => Ok(Self::Dance),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeFlag {
    SimplyLove,
}

impl ThemeFlag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SimplyLove => "Simply Love",
        }
    }
}

impl FromStr for ThemeFlag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "simplylove" => Ok(Self::SimplyLove),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageFlag {
    Auto,
    English,
    German,
    Spanish,
    French,
    Italian,
    Japanese,
    Polish,
    PortugueseBrazil,
    Russian,
    Swedish,
    Pseudo,
}

impl LanguageFlag {
    /// Returns the config token persisted to `deadsync.ini`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::English => "English",
            Self::German => "German",
            Self::Spanish => "Spanish",
            Self::French => "French",
            Self::Italian => "Italian",
            Self::Japanese => "Japanese",
            Self::Polish => "Polish",
            Self::PortugueseBrazil => "PortugueseBrazil",
            Self::Russian => "Russian",
            Self::Swedish => "Swedish",
            Self::Pseudo => "Pseudo",
        }
    }

    /// Returns the locale code used by the i18n system.
    pub const fn locale_code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::English => "en",
            Self::German => "de",
            Self::Spanish => "es",
            Self::French => "fr",
            Self::Italian => "it",
            Self::Japanese => "ja",
            Self::Polish => "pl",
            Self::PortugueseBrazil => "pt-br",
            Self::Russian => "ru",
            Self::Swedish => "sv",
            Self::Pseudo => "pseudo",
        }
    }
}

impl FromStr for LanguageFlag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "auto" => Ok(Self::Auto),
            "english" | "en" => Ok(Self::English),
            "german" | "de" => Ok(Self::German),
            "spanish" | "es" => Ok(Self::Spanish),
            "french" | "fr" => Ok(Self::French),
            "italian" | "it" => Ok(Self::Italian),
            "japanese" | "ja" => Ok(Self::Japanese),
            "polish" | "pl" => Ok(Self::Polish),
            "portuguesebrazil" | "brazilianportuguese" | "ptbr" => Ok(Self::PortugueseBrazil),
            "russian" | "ru" => Ok(Self::Russian),
            "swedish" | "sv" => Ok(Self::Swedish),
            "pseudo" => Ok(Self::Pseudo),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warn => "Warn",
            Self::Info => "Info",
            Self::Debug => "Debug",
            Self::Trace => "Trace",
        }
    }

    pub const fn as_level_filter(self) -> log::LevelFilter {
        match self {
            Self::Error => log::LevelFilter::Error,
            Self::Warn => log::LevelFilter::Warn,
            Self::Info => log::LevelFilter::Info,
            Self::Debug => log::LevelFilter::Debug,
            Self::Trace => log::LevelFilter::Trace,
        }
    }
}

impl FromStr for LogLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" | "err" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_font_default_is_common() {
        assert_eq!(MachineFont::default(), MachineFont::Common);
    }

    #[test]
    fn machine_font_round_trips_through_from_str_display() {
        for &v in &MACHINE_FONT_VARIANTS {
            assert_eq!(MachineFont::from_str(v.as_str()), Ok(v));
        }
    }

    #[test]
    fn machine_font_from_str_is_case_insensitive_and_accepts_wendy_alias() {
        assert_eq!(MachineFont::from_str("common"), Ok(MachineFont::Common));
        assert_eq!(MachineFont::from_str("COMMON"), Ok(MachineFont::Common));
        assert_eq!(MachineFont::from_str("Mega"), Ok(MachineFont::Mega));
        assert_eq!(MachineFont::from_str("mega"), Ok(MachineFont::Mega));
        // SL labels Common as "Wendy" in its UI; accept that token too so a
        // user editing the ini by hand isn't surprised.
        assert_eq!(MachineFont::from_str("Wendy"), Ok(MachineFont::Common));
    }

    #[test]
    fn machine_font_from_str_rejects_unknown() {
        assert_eq!(MachineFont::from_str(""), Err(()));
        assert_eq!(MachineFont::from_str("Unprofessional"), Err(()));
        assert_eq!(MachineFont::from_str("miso"), Err(()));
    }

    #[test]
    fn machine_font_variants_table_is_exhaustive() {
        // Sanity: every enum variant is in MACHINE_FONT_VARIANTS so the
        // operator UI cycles through everything the type can represent.
        // Update this if a new variant is added.
        assert_eq!(MACHINE_FONT_VARIANTS.len(), 2);
        assert!(MACHINE_FONT_VARIANTS.contains(&MachineFont::Common));
        assert!(MACHINE_FONT_VARIANTS.contains(&MachineFont::Mega));
    }
}
