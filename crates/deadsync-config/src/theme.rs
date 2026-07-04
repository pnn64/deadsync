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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum RandomBackgroundMode {
    #[default]
    Off,
    RandomMovies,
}

impl RandomBackgroundMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::RandomMovies => "RandomMovies",
        }
    }
}

impl FromStr for RandomBackgroundMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "off" | "none" | "false" | "0" => Ok(Self::Off),
            "randommovies" | "randommovie" | "movies" | "movie" | "on" | "true" | "1" => {
                Ok(Self::RandomMovies)
            }
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum DefaultSyncOffset {
    #[default]
    Null,
    Itg,
}

impl DefaultSyncOffset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Null => "NULL",
            Self::Itg => "ITG",
        }
    }

    pub const fn sync_pref(self) -> deadsync_chart::SyncPref {
        match self {
            Self::Null => deadsync_chart::SyncPref::Null,
            Self::Itg => deadsync_chart::SyncPref::Itg,
        }
    }

    pub const fn from_sync_pref(pref: deadsync_chart::SyncPref) -> Self {
        match pref {
            deadsync_chart::SyncPref::Itg => Self::Itg,
            deadsync_chart::SyncPref::Default | deadsync_chart::SyncPref::Null => Self::Null,
        }
    }
}

impl FromStr for DefaultSyncOffset {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "null" | "none" | "off" => Ok(Self::Null),
            "itg" => Ok(Self::Itg),
            _ => Err(()),
        }
    }
}

/// Which side of the screen the persistent build-version watermark sits on.
/// Independent of [`Config::show_version_overlay`] so toggling visibility
/// doesn't forget the user's preferred side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum VersionOverlaySide {
    Left,
    #[default]
    Right,
}

impl VersionOverlaySide {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

impl FromStr for VersionOverlaySide {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" | "l" | "lhs" => Ok(Self::Left),
            "right" | "r" | "rhs" => Ok(Self::Right),
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
pub enum SelectMusicStepArtistBoxMode {
    Default,
    Legacy,
    Expanded,
}

impl SelectMusicStepArtistBoxMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Legacy => "Legacy",
            Self::Expanded => "Expanded",
        }
    }

    pub const fn is_expanded(self, theme: ThemeFlag) -> bool {
        match self {
            Self::Default => select_music_step_artist_default_expanded(theme),
            Self::Legacy => false,
            Self::Expanded => true,
        }
    }
}

const fn select_music_step_artist_default_expanded(theme: ThemeFlag) -> bool {
    match theme {
        ThemeFlag::SimplyLove => false,
    }
}

impl FromStr for SelectMusicStepArtistBoxMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "default" | "theme" | "themedefault" => Ok(Self::Default),
            "legacy" | "small" | "sl" | "simplylove" => Ok(Self::Legacy),
            "expanded" | "large" | "arrowcloud" | "ac" => Ok(Self::Expanded),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectMusicSongSelectBgMode {
    #[default]
    Off,
    Banner,
    Bg,
}

impl SelectMusicSongSelectBgMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Banner => "Banner",
            Self::Bg => "BG",
        }
    }
}

impl FromStr for SelectMusicSongSelectBgMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "off" | "none" | "false" | "0" => Ok(Self::Off),
            "banner" | "banners" => Ok(Self::Banner),
            "bg" | "background" | "backgrounds" => Ok(Self::Bg),
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
pub enum VisualStyle {
    Hearts,
    Arrows,
    Bears,
    Ducks,
    Cats,
    Spooky,
    Gay,
    Stars,
    Thonk,
    Technique,
    Srpg9,
}

impl VisualStyle {
    pub const ALL: [Self; 11] = [
        Self::Hearts,
        Self::Arrows,
        Self::Bears,
        Self::Ducks,
        Self::Cats,
        Self::Spooky,
        Self::Gay,
        Self::Stars,
        Self::Thonk,
        Self::Technique,
        Self::Srpg9,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hearts => "Hearts",
            Self::Arrows => "Arrows",
            Self::Bears => "Bears",
            Self::Ducks => "Ducks",
            Self::Cats => "Cats",
            Self::Spooky => "Spooky",
            Self::Gay => "Gay",
            Self::Stars => "Stars",
            Self::Thonk => "Thonk",
            Self::Technique => "Technique",
            Self::Srpg9 => "SRPG9",
        }
    }

    pub const fn is_srpg(self) -> bool {
        matches!(self, Self::Srpg9)
    }
}

impl FromStr for VisualStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "hearts" | "heart" | "default" => Ok(Self::Hearts),
            "arrows" | "arrow" => Ok(Self::Arrows),
            "bears" | "bear" => Ok(Self::Bears),
            "ducks" | "duck" => Ok(Self::Ducks),
            "cats" | "cat" => Ok(Self::Cats),
            "spooky" => Ok(Self::Spooky),
            "gay" => Ok(Self::Gay),
            "stars" | "star" => Ok(Self::Stars),
            "thonk" => Ok(Self::Thonk),
            "technique" => Ok(Self::Technique),
            "srpg9" | "srpg10" | "srpg" => Ok(Self::Srpg9),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SrpgVariant {
    #[default]
    Srpg9,
    Srpg10,
}

impl SrpgVariant {
    pub const ALL: [Self; 2] = [Self::Srpg9, Self::Srpg10];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srpg9 => "SRPG9",
            Self::Srpg10 => "SRPG10",
        }
    }

    pub const fn asset_folder(self) -> &'static str {
        match self {
            Self::Srpg9 => "srpg9",
            Self::Srpg10 => "srpg10",
        }
    }

    pub fn from_visual_style_str(s: &str) -> Option<Self> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "srpg9" | "srpg" => Some(Self::Srpg9),
            "srpg10" => Some(Self::Srpg10),
            _ => None,
        }
    }
}

impl FromStr for SrpgVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_visual_style_str(s).ok_or(())
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

/// When to auto-show the ArrowCloud QR-login screen after the user picks
/// a profile.  Mirrors Simply Love's `QRLogin` theme pref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowCloudQrLoginWhen {
    /// Always show the login screen after Select Profile.
    Always,
    /// Show only when at least one joined Local player has no saved API
    /// key.  Default.
    #[default]
    Sometimes,
    /// Never auto-show; only the manual Options entry can launch it.
    Disabled,
}

impl ArrowCloudQrLoginWhen {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::Sometimes => "Sometimes",
            Self::Disabled => "Disabled",
        }
    }
}

impl FromStr for ArrowCloudQrLoginWhen {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "always" => Ok(Self::Always),
            "sometimes" => Ok(Self::Sometimes),
            "disabled" | "never" | "off" | "no" => Ok(Self::Disabled),
            _ => Err(()),
        }
    }
}

/// When to auto-show the GrooveStats QR-login screen after the user picks
/// a profile.  Mirrors Simply Love's `QRLogin` theme pref — same wire
/// values and same default as the ArrowCloud variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrooveStatsQrLoginWhen {
    /// Always show the login screen after Select Profile.
    Always,
    /// Show only when at least one joined Local player has no saved
    /// GrooveStats API key.  Default.
    #[default]
    Sometimes,
    /// Never auto-show; only the manual Options entry / Manage Local
    /// Profiles "Link GrooveStats" action can launch it.
    Disabled,
}

impl GrooveStatsQrLoginWhen {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::Sometimes => "Sometimes",
            Self::Disabled => "Disabled",
        }
    }
}

impl FromStr for GrooveStatsQrLoginWhen {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "always" => Ok(Self::Always),
            "sometimes" => Ok(Self::Sometimes),
            "disabled" | "never" | "off" | "no" => Ok(Self::Disabled),
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
    Wendy,
    /// Bold/Header/Footer = Mega alphanumeric, numbers = Mega monospace.
    Mega,
}

impl MachineFont {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wendy => "Wendy",
            Self::Mega => "Mega",
        }
    }
}

impl FromStr for MachineFont {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "common" | "wendy" => Ok(Self::Wendy),
            "mega" => Ok(Self::Mega),
            _ => Err(()),
        }
    }
}

pub const MACHINE_FONT_VARIANTS: [MachineFont; 2] = [MachineFont::Wendy, MachineFont::Mega];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MachineBarColor {
    #[default]
    Default,
    Colored,
    Transparent,
}

impl MachineBarColor {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Colored => "Colored",
            Self::Transparent => "Transparent",
        }
    }

    pub const fn resolve(self, visual_style: VisualStyle) -> Self {
        match (self, visual_style) {
            (Self::Default, VisualStyle::Technique) => Self::Transparent,
            (Self::Default, VisualStyle::Srpg9) => Self::Colored,
            _ => self,
        }
    }
}

impl FromStr for MachineBarColor {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "default" => Ok(Self::Default),
            "colored" | "color" | "colour" | "coloured" | "srpg" | "srpg9" => Ok(Self::Colored),
            "transparent" | "clear" | "technique" => Ok(Self::Transparent),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MachineEvaluationStyle {
    #[default]
    Default,
    Opaque,
    Transparent,
}

impl MachineEvaluationStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Opaque => "Opaque",
            Self::Transparent => "Transparent",
        }
    }

    pub const fn resolve(self, visual_style: VisualStyle) -> Self {
        match (self, visual_style) {
            (Self::Default, VisualStyle::Technique) => Self::Transparent,
            (Self::Default, _) => Self::Opaque,
            _ => self,
        }
    }
}

impl FromStr for MachineEvaluationStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "default" | "auto" | "theme" | "themedriven" => Ok(Self::Default),
            "opaque" | "solid" | "hearts" => Ok(Self::Opaque),
            "transparent" | "transparant" | "clear" | "technique" => Ok(Self::Transparent),
            _ => Err(()),
        }
    }
}

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

pub fn parse_visual_style(
    raw_style: Option<&str>,
    raw_legacy_style: Option<&str>,
    default: VisualStyle,
) -> VisualStyle {
    raw_style
        .or(raw_legacy_style)
        .and_then(|v| VisualStyle::from_str(v).ok())
        .unwrap_or(default)
}

pub fn parse_srpg_variant(
    raw_variant: Option<&str>,
    raw_legacy_variant: Option<&str>,
    raw_visual_style: Option<&str>,
    default: SrpgVariant,
) -> SrpgVariant {
    raw_variant
        .or(raw_legacy_variant)
        .and_then(|v| SrpgVariant::from_str(v).ok())
        .or_else(|| raw_visual_style.and_then(SrpgVariant::from_visual_style_str))
        .unwrap_or(default)
}

pub fn parse_machine_default_sync_offset(
    raw_offset: Option<&str>,
    raw_legacy_offset: Option<&str>,
    default: DefaultSyncOffset,
) -> DefaultSyncOffset {
    raw_offset
        .or(raw_legacy_offset)
        .and_then(|v| DefaultSyncOffset::from_str(v).ok())
        .unwrap_or(default)
}

pub fn parse_machine_font(
    raw_font: Option<&str>,
    raw_legacy_font: Option<&str>,
    default: MachineFont,
) -> MachineFont {
    raw_font
        .or(raw_legacy_font)
        .and_then(|v| MachineFont::from_str(v).ok())
        .unwrap_or(default)
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
    fn machine_font_default_is_wendy() {
        assert_eq!(MachineFont::default(), MachineFont::Wendy);
    }

    #[test]
    fn machine_font_round_trips_through_from_str_display() {
        for &v in &MACHINE_FONT_VARIANTS {
            assert_eq!(MachineFont::from_str(v.as_str()), Ok(v));
        }
    }

    #[test]
    fn machine_font_from_str_is_case_insensitive_and_accepts_common_alias() {
        assert_eq!(MachineFont::from_str("Wendy"), Ok(MachineFont::Wendy));
        assert_eq!(MachineFont::from_str("wendy"), Ok(MachineFont::Wendy));
        assert_eq!(MachineFont::from_str("Mega"), Ok(MachineFont::Mega));
        assert_eq!(MachineFont::from_str("mega"), Ok(MachineFont::Mega));
        // SL/Arrow Cloud use Common as the font-redir family internally;
        // accept old saved DeadSync configs and hand-edited ini values.
        assert_eq!(MachineFont::from_str("common"), Ok(MachineFont::Wendy));
        assert_eq!(MachineFont::from_str("COMMON"), Ok(MachineFont::Wendy));
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
        assert!(MACHINE_FONT_VARIANTS.contains(&MachineFont::Wendy));
        assert!(MACHINE_FONT_VARIANTS.contains(&MachineFont::Mega));
    }

    #[test]
    fn machine_font_parse_uses_primary_before_legacy_key() {
        assert_eq!(
            parse_machine_font(Some("Mega"), Some("Wendy"), MachineFont::Wendy),
            MachineFont::Mega
        );
        assert_eq!(
            parse_machine_font(None, Some("Mega"), MachineFont::Wendy),
            MachineFont::Mega
        );
        assert_eq!(
            parse_machine_font(Some("bad"), Some("Mega"), MachineFont::Wendy),
            MachineFont::Wendy
        );
    }

    #[test]
    fn visual_style_srpg10_alias_uses_srpg_family_icon() {
        assert_eq!(VisualStyle::from_str("SRPG10"), Ok(VisualStyle::Srpg9));
    }

    #[test]
    fn visual_style_parse_uses_primary_before_legacy_key() {
        assert_eq!(
            parse_visual_style(Some("Technique"), Some("Hearts"), VisualStyle::Arrows),
            VisualStyle::Technique
        );
        assert_eq!(
            parse_visual_style(None, Some("Cats"), VisualStyle::Arrows),
            VisualStyle::Cats
        );
        assert_eq!(
            parse_visual_style(Some("bad"), Some("Cats"), VisualStyle::Arrows),
            VisualStyle::Arrows
        );
    }

    #[test]
    fn srpg_variant_round_trips() {
        for value in SrpgVariant::ALL {
            assert_eq!(SrpgVariant::from_str(value.as_str()), Ok(value));
        }
    }

    #[test]
    fn srpg_variant_accepts_import_aliases() {
        assert_eq!(SrpgVariant::from_str("SRPG"), Ok(SrpgVariant::Srpg9));
        assert_eq!(SrpgVariant::from_str("SRPG10"), Ok(SrpgVariant::Srpg10));
    }

    #[test]
    fn srpg_variant_parse_uses_variant_then_visual_style_fallback() {
        assert_eq!(
            parse_srpg_variant(
                Some("SRPG10"),
                Some("SRPG9"),
                Some("SRPG9"),
                SrpgVariant::Srpg9,
            ),
            SrpgVariant::Srpg10
        );
        assert_eq!(
            parse_srpg_variant(None, Some("SRPG10"), Some("SRPG9"), SrpgVariant::Srpg9),
            SrpgVariant::Srpg10
        );
        assert_eq!(
            parse_srpg_variant(
                Some("bad"),
                Some("SRPG10"),
                Some("SRPG10"),
                SrpgVariant::Srpg9,
            ),
            SrpgVariant::Srpg10
        );
        assert_eq!(
            parse_srpg_variant(
                Some("bad"),
                Some("SRPG10"),
                Some("Hearts"),
                SrpgVariant::Srpg9,
            ),
            SrpgVariant::Srpg9
        );
    }

    #[test]
    fn machine_bar_color_defaults_to_default() {
        assert_eq!(MachineBarColor::default(), MachineBarColor::Default);
    }

    #[test]
    fn machine_bar_color_round_trips() {
        for value in [
            MachineBarColor::Default,
            MachineBarColor::Colored,
            MachineBarColor::Transparent,
        ] {
            assert_eq!(MachineBarColor::from_str(value.as_str()), Ok(value));
        }
    }

    #[test]
    fn machine_bar_color_accepts_theme_style_aliases() {
        assert_eq!(
            MachineBarColor::from_str("SRPG9"),
            Ok(MachineBarColor::Colored)
        );
        assert_eq!(
            MachineBarColor::from_str("Technique"),
            Ok(MachineBarColor::Transparent)
        );
    }

    #[test]
    fn machine_bar_color_default_resolves_from_visual_style() {
        assert_eq!(
            MachineBarColor::Default.resolve(VisualStyle::Hearts),
            MachineBarColor::Default
        );
        assert_eq!(
            MachineBarColor::Default.resolve(VisualStyle::Technique),
            MachineBarColor::Transparent
        );
        assert_eq!(
            MachineBarColor::Default.resolve(VisualStyle::Srpg9),
            MachineBarColor::Colored
        );
        assert_eq!(
            MachineBarColor::Transparent.resolve(VisualStyle::Srpg9),
            MachineBarColor::Transparent
        );
    }

    #[test]
    fn machine_evaluation_style_defaults_to_default() {
        assert_eq!(
            MachineEvaluationStyle::default(),
            MachineEvaluationStyle::Default
        );
    }

    #[test]
    fn machine_evaluation_style_round_trips() {
        for value in [
            MachineEvaluationStyle::Default,
            MachineEvaluationStyle::Opaque,
            MachineEvaluationStyle::Transparent,
        ] {
            assert_eq!(MachineEvaluationStyle::from_str(value.as_str()), Ok(value));
        }
    }

    #[test]
    fn machine_evaluation_style_accepts_theme_style_aliases() {
        assert_eq!(
            MachineEvaluationStyle::from_str("Hearts"),
            Ok(MachineEvaluationStyle::Opaque)
        );
        assert_eq!(
            MachineEvaluationStyle::from_str("Technique"),
            Ok(MachineEvaluationStyle::Transparent)
        );
        assert_eq!(
            MachineEvaluationStyle::from_str("Transparant"),
            Ok(MachineEvaluationStyle::Transparent)
        );
    }

    #[test]
    fn machine_evaluation_style_default_resolves_from_visual_style() {
        assert_eq!(
            MachineEvaluationStyle::Default.resolve(VisualStyle::Hearts),
            MachineEvaluationStyle::Opaque
        );
        assert_eq!(
            MachineEvaluationStyle::Default.resolve(VisualStyle::Technique),
            MachineEvaluationStyle::Transparent
        );
        assert_eq!(
            MachineEvaluationStyle::Transparent.resolve(VisualStyle::Hearts),
            MachineEvaluationStyle::Transparent
        );
        assert_eq!(
            MachineEvaluationStyle::Opaque.resolve(VisualStyle::Technique),
            MachineEvaluationStyle::Opaque
        );
    }

    #[test]
    fn random_background_mode_defaults_to_off() {
        assert_eq!(RandomBackgroundMode::default(), RandomBackgroundMode::Off);
    }

    #[test]
    fn random_background_mode_round_trips() {
        assert_eq!(
            RandomBackgroundMode::from_str(RandomBackgroundMode::Off.as_str()),
            Ok(RandomBackgroundMode::Off)
        );
        assert_eq!(
            RandomBackgroundMode::from_str(RandomBackgroundMode::RandomMovies.as_str()),
            Ok(RandomBackgroundMode::RandomMovies)
        );
    }

    #[test]
    fn random_background_mode_accepts_common_aliases() {
        assert_eq!(
            RandomBackgroundMode::from_str("Random Movies"),
            Ok(RandomBackgroundMode::RandomMovies)
        );
        assert_eq!(
            RandomBackgroundMode::from_str("0"),
            Ok(RandomBackgroundMode::Off)
        );
    }

    #[test]
    fn default_sync_offset_defaults_to_null() {
        assert_eq!(DefaultSyncOffset::default(), DefaultSyncOffset::Null);
    }

    #[test]
    fn default_sync_offset_round_trips() {
        assert_eq!(
            DefaultSyncOffset::from_str(DefaultSyncOffset::Null.as_str()),
            Ok(DefaultSyncOffset::Null)
        );
        assert_eq!(
            DefaultSyncOffset::from_str(DefaultSyncOffset::Itg.as_str()),
            Ok(DefaultSyncOffset::Itg)
        );
    }

    #[test]
    fn default_sync_offset_maps_chart_sync_pref() {
        assert_eq!(
            DefaultSyncOffset::Null.sync_pref(),
            deadsync_chart::SyncPref::Null
        );
        assert_eq!(
            DefaultSyncOffset::Itg.sync_pref(),
            deadsync_chart::SyncPref::Itg
        );
        assert_eq!(
            DefaultSyncOffset::from_sync_pref(deadsync_chart::SyncPref::Default),
            DefaultSyncOffset::Null
        );
        assert_eq!(
            DefaultSyncOffset::from_sync_pref(deadsync_chart::SyncPref::Itg),
            DefaultSyncOffset::Itg
        );
    }

    #[test]
    fn machine_default_sync_offset_parse_uses_primary_before_legacy_key() {
        assert_eq!(
            parse_machine_default_sync_offset(Some("ITG"), Some("NULL"), DefaultSyncOffset::Null,),
            DefaultSyncOffset::Itg
        );
        assert_eq!(
            parse_machine_default_sync_offset(None, Some("ITG"), DefaultSyncOffset::Null),
            DefaultSyncOffset::Itg
        );
        assert_eq!(
            parse_machine_default_sync_offset(Some("bad"), Some("ITG"), DefaultSyncOffset::Null,),
            DefaultSyncOffset::Null
        );
    }

    #[test]
    fn song_select_bg_mode_defaults_to_off() {
        assert_eq!(
            SelectMusicSongSelectBgMode::default(),
            SelectMusicSongSelectBgMode::Off
        );
    }

    #[test]
    fn song_select_bg_mode_round_trips() {
        for value in [
            SelectMusicSongSelectBgMode::Off,
            SelectMusicSongSelectBgMode::Banner,
            SelectMusicSongSelectBgMode::Bg,
        ] {
            assert_eq!(
                SelectMusicSongSelectBgMode::from_str(value.as_str()),
                Ok(value)
            );
        }
    }

    #[test]
    fn song_select_bg_mode_accepts_background_alias() {
        assert_eq!(
            SelectMusicSongSelectBgMode::from_str("Background"),
            Ok(SelectMusicSongSelectBgMode::Bg)
        );
    }

    #[test]
    fn step_artist_box_mode_round_trips() {
        for value in [
            SelectMusicStepArtistBoxMode::Default,
            SelectMusicStepArtistBoxMode::Legacy,
            SelectMusicStepArtistBoxMode::Expanded,
        ] {
            assert_eq!(
                SelectMusicStepArtistBoxMode::from_str(value.as_str()),
                Ok(value)
            );
        }
    }

    #[test]
    fn step_artist_box_default_tracks_theme() {
        assert!(!SelectMusicStepArtistBoxMode::Default.is_expanded(ThemeFlag::SimplyLove));
        assert!(!SelectMusicStepArtistBoxMode::Legacy.is_expanded(ThemeFlag::SimplyLove));
        assert!(SelectMusicStepArtistBoxMode::Expanded.is_expanded(ThemeFlag::SimplyLove));
    }
}
