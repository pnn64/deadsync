use crate::bools::{parse_bool_str, parse_loose_bool_str};
use crate::defaults::{
    DEFAULT_ALLOW_SWITCH_PROFILE_IN_MENU, DEFAULT_KEYBOARD_FEATURES,
    DEFAULT_MACHINE_ALLOW_PER_PLAYER_GLOBAL_OFFSETS, DEFAULT_MACHINE_ENABLE_REPLAYS,
    DEFAULT_MACHINE_NICE_SOUND, DEFAULT_MACHINE_PACK_INI_OFFSETS,
    DEFAULT_MACHINE_SHOW_EVAL_SUMMARY, DEFAULT_MACHINE_SHOW_GAMEOVER,
    DEFAULT_MACHINE_SHOW_NAME_ENTRY, DEFAULT_MACHINE_SHOW_SELECT_COLOR,
    DEFAULT_MACHINE_SHOW_SELECT_PLAY_MODE, DEFAULT_MACHINE_SHOW_SELECT_PROFILE,
    DEFAULT_MACHINE_SHOW_SELECT_STYLE, DEFAULT_SHOW_BPM_DECIMAL,
    DEFAULT_SHOW_SELECT_MUSIC_GAMEPLAY_TIMER, DEFAULT_SHOW_VIDEO_BACKGROUNDS,
    DEFAULT_SIMPLY_LOVE_COLOR, DEFAULT_ZMOD_RATING_BOX_TEXT,
};
use crate::ini::SimpleIni;
use crate::writer::{push_bool, push_line};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayBpmPosition {
    TopCenter,
    NearField,
}

impl GameplayBpmPosition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopCenter => "TopCenter",
            Self::NearField => "NearField",
        }
    }
}

impl FromStr for GameplayBpmPosition {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "topcenter" | "center" | "centered" | "normal" => Ok(Self::TopCenter),
            "nearfield" | "nearnotefield" | "notefield" | "field" => Ok(Self::NearField),
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

pub const fn auto_screenshot_eval_matches(
    mask: u8,
    is_pb: bool,
    is_fail: bool,
    is_quad: bool,
    is_quint: bool,
) -> bool {
    mask != 0
        && (((mask & AUTO_SS_PBS) != 0 && is_pb)
            || ((mask & AUTO_SS_FAILS) != 0 && is_fail)
            || ((mask & AUTO_SS_CLEARS) != 0 && !is_fail)
            || ((mask & AUTO_SS_QUADS) != 0 && is_quad)
            || ((mask & AUTO_SS_QUINTS) != 0 && is_quint))
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

/// Normalize an OS locale string to the language file naming convention.
///
/// Examples:
///
/// - `"ja-JP"` -> `"ja-jp"`
/// - `"fr-FR.UTF-8"` -> `"fr-fr"`
/// - `"pt_BR"` -> `"pt-br"`
/// - `"zh-TW"` / `"zh-HK"` -> `"zh-Hant"`
/// - `"zh-CN"` / `"zh-SG"` -> `"zh-Hans"`
pub fn normalize_locale(raw: &str) -> String {
    let lower = raw
        .trim()
        .split('.')
        .next()
        .unwrap_or(raw)
        .split('@')
        .next()
        .unwrap_or(raw)
        .replace('_', "-")
        .to_ascii_lowercase();

    if lower.starts_with("zh") {
        if lower.contains("hant") || lower.contains("tw") || lower.contains("hk") {
            return "zh-Hant".to_string();
        }
        if lower.contains("hans") || lower.contains("cn") || lower.contains("sg") {
            return "zh-Hans".to_string();
        }
        return "zh-Hans".to_string();
    }

    lower
}

pub fn resolve_language_locale(
    flag: LanguageFlag,
    raw_os_locale: Option<&str>,
    locale_exists: impl Fn(&str) -> bool,
) -> String {
    if flag != LanguageFlag::Auto {
        return flag.locale_code().to_string();
    }

    let code = normalize_locale(raw_os_locale.unwrap_or("en"));
    if locale_exists(&code) {
        return code;
    }
    if let Some(base) = code.split('-').next()
        && base != code
        && locale_exists(base)
    {
        return base.to_string();
    }
    "en".to_string()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePresentationOptions {
    pub simply_love_color: i32,
    pub show_select_music_gameplay_timer: bool,
    pub keyboard_features: bool,
    pub visual_style: VisualStyle,
    pub srpg_variant: SrpgVariant,
    pub show_video_backgrounds: bool,
    pub random_background_mode: RandomBackgroundMode,
    pub zmod_rating_box_text: bool,
    pub show_bpm_decimal: bool,
    pub gameplay_bpm_position: GameplayBpmPosition,
}

impl Default for ThemePresentationOptions {
    fn default() -> Self {
        Self {
            simply_love_color: DEFAULT_SIMPLY_LOVE_COLOR,
            show_select_music_gameplay_timer: DEFAULT_SHOW_SELECT_MUSIC_GAMEPLAY_TIMER,
            keyboard_features: DEFAULT_KEYBOARD_FEATURES,
            visual_style: VisualStyle::Hearts,
            srpg_variant: SrpgVariant::Srpg9,
            show_video_backgrounds: DEFAULT_SHOW_VIDEO_BACKGROUNDS,
            random_background_mode: RandomBackgroundMode::Off,
            zmod_rating_box_text: DEFAULT_ZMOD_RATING_BOX_TEXT,
            show_bpm_decimal: DEFAULT_SHOW_BPM_DECIMAL,
            gameplay_bpm_position: GameplayBpmPosition::TopCenter,
        }
    }
}

pub fn load_theme_presentation_options(
    conf: &SimpleIni,
    default: ThemePresentationOptions,
) -> ThemePresentationOptions {
    let visual_style = conf.get("Theme", "VisualStyle");
    let legacy_visual_style = conf.get("Theme", "MenuBackgroundStyle");
    let srpg_variant = conf.get("Theme", "SrpgVariant");
    let legacy_srpg_variant = conf.get("Theme", "ThemeVariant");

    ThemePresentationOptions {
        simply_love_color: conf
            .get("Theme", "SimplyLoveColor")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(default.simply_love_color),
        show_select_music_gameplay_timer: conf
            .get("Theme", "ShowSelectMusicGameplayTimer")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.show_select_music_gameplay_timer),
        keyboard_features: conf
            .get("Theme", "KeyboardFeatures")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.keyboard_features),
        visual_style: parse_visual_style(
            visual_style.as_deref(),
            legacy_visual_style.as_deref(),
            default.visual_style,
        ),
        srpg_variant: parse_srpg_variant(
            srpg_variant.as_deref(),
            legacy_srpg_variant.as_deref(),
            visual_style.or(legacy_visual_style).as_deref(),
            default.srpg_variant,
        ),
        show_video_backgrounds: conf
            .get("Theme", "VideoBackgrounds")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.show_video_backgrounds),
        random_background_mode: conf
            .get("Theme", "RandomBackgroundMode")
            .and_then(|value| RandomBackgroundMode::from_str(&value).ok())
            .unwrap_or(default.random_background_mode),
        zmod_rating_box_text: conf
            .get("Theme", "ZmodRatingBoxText")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.zmod_rating_box_text),
        show_bpm_decimal: conf
            .get("Theme", "ShowBpmDecimal")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.show_bpm_decimal),
        gameplay_bpm_position: conf
            .get("Theme", "GameplayBpmPosition")
            .or_else(|| conf.get("Theme", "BpmPosition"))
            .and_then(|value| GameplayBpmPosition::from_str(&value).ok())
            .unwrap_or(default.gameplay_bpm_position),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineFlowOptions {
    pub machine_show_eval_summary: bool,
    pub machine_nice_sound: bool,
    pub machine_show_name_entry: bool,
    pub machine_show_gameover: bool,
    pub machine_show_select_profile: bool,
    pub allow_switch_profile_in_menu: bool,
    pub machine_show_select_color: bool,
    pub machine_show_select_style: bool,
    pub machine_show_select_play_mode: bool,
    pub machine_enable_replays: bool,
    pub machine_allow_per_player_global_offsets: bool,
    pub machine_pack_ini_offsets: bool,
    pub machine_default_sync_offset: DefaultSyncOffset,
    pub machine_preferred_style: MachinePreferredPlayStyle,
    pub machine_preferred_play_mode: MachinePreferredPlayMode,
    pub machine_font: MachineFont,
    pub machine_bar_color: MachineBarColor,
    pub machine_evaluation_style: MachineEvaluationStyle,
}

impl Default for MachineFlowOptions {
    fn default() -> Self {
        Self {
            machine_show_eval_summary: DEFAULT_MACHINE_SHOW_EVAL_SUMMARY,
            machine_nice_sound: DEFAULT_MACHINE_NICE_SOUND,
            machine_show_name_entry: DEFAULT_MACHINE_SHOW_NAME_ENTRY,
            machine_show_gameover: DEFAULT_MACHINE_SHOW_GAMEOVER,
            machine_show_select_profile: DEFAULT_MACHINE_SHOW_SELECT_PROFILE,
            allow_switch_profile_in_menu: DEFAULT_ALLOW_SWITCH_PROFILE_IN_MENU,
            machine_show_select_color: DEFAULT_MACHINE_SHOW_SELECT_COLOR,
            machine_show_select_style: DEFAULT_MACHINE_SHOW_SELECT_STYLE,
            machine_show_select_play_mode: DEFAULT_MACHINE_SHOW_SELECT_PLAY_MODE,
            machine_enable_replays: DEFAULT_MACHINE_ENABLE_REPLAYS,
            machine_allow_per_player_global_offsets:
                DEFAULT_MACHINE_ALLOW_PER_PLAYER_GLOBAL_OFFSETS,
            machine_pack_ini_offsets: DEFAULT_MACHINE_PACK_INI_OFFSETS,
            machine_default_sync_offset: DefaultSyncOffset::Null,
            machine_preferred_style: MachinePreferredPlayStyle::Single,
            machine_preferred_play_mode: MachinePreferredPlayMode::Regular,
            machine_font: MachineFont::Wendy,
            machine_bar_color: MachineBarColor::Default,
            machine_evaluation_style: MachineEvaluationStyle::Default,
        }
    }
}

pub fn load_machine_flow_options(
    conf: &SimpleIni,
    default: MachineFlowOptions,
) -> MachineFlowOptions {
    let machine_default_sync_offset = conf.get("Theme", "MachineDefaultSyncOffset");
    let legacy_default_sync_offset = conf.get("Theme", "DefaultSyncOffset");
    let machine_font = conf.get("Theme", "MachineFont");
    let legacy_machine_font = conf.get("Theme", "ThemeFont");

    MachineFlowOptions {
        machine_show_eval_summary: conf
            .get("Theme", "MachineShowEvalSummary")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.machine_show_eval_summary),
        machine_nice_sound: conf
            .get("Theme", "MachineNiceSound")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_nice_sound),
        machine_show_name_entry: conf
            .get("Theme", "MachineShowNameEntry")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_show_name_entry),
        machine_show_gameover: conf
            .get("Theme", "MachineShowGameOver")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_show_gameover),
        machine_show_select_profile: conf
            .get("Theme", "MachineShowSelectProfile")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_show_select_profile),
        allow_switch_profile_in_menu: conf
            .get("Theme", "AllowSwitchProfileInMenu")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.allow_switch_profile_in_menu),
        machine_show_select_color: conf
            .get("Theme", "MachineShowSelectColor")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_show_select_color),
        machine_show_select_style: conf
            .get("Theme", "MachineShowSelectStyle")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_show_select_style),
        machine_show_select_play_mode: conf
            .get("Theme", "MachineShowSelectPlayMode")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_show_select_play_mode),
        machine_enable_replays: conf
            .get("Theme", "MachineEnableReplays")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_enable_replays),
        machine_allow_per_player_global_offsets: conf
            .get("Theme", "MachineAllowPerPlayerGlobalOffsets")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_allow_per_player_global_offsets),
        machine_pack_ini_offsets: conf
            .get("Theme", "MachinePackIniOffsets")
            .and_then(|value| parse_loose_bool_str(&value))
            .unwrap_or(default.machine_pack_ini_offsets),
        machine_default_sync_offset: parse_machine_default_sync_offset(
            machine_default_sync_offset.as_deref(),
            legacy_default_sync_offset.as_deref(),
            default.machine_default_sync_offset,
        ),
        machine_preferred_style: conf
            .get("Theme", "MachinePreferredStyle")
            .and_then(|value| MachinePreferredPlayStyle::from_str(&value).ok())
            .unwrap_or(default.machine_preferred_style),
        machine_preferred_play_mode: conf
            .get("Theme", "MachinePreferredPlayMode")
            .and_then(|value| MachinePreferredPlayMode::from_str(&value).ok())
            .unwrap_or(default.machine_preferred_play_mode),
        machine_font: parse_machine_font(
            machine_font.as_deref(),
            legacy_machine_font.as_deref(),
            default.machine_font,
        ),
        machine_bar_color: conf
            .get("Theme", "MachineBarColor")
            .and_then(|value| MachineBarColor::from_str(&value).ok())
            .unwrap_or(default.machine_bar_color),
        machine_evaluation_style: conf
            .get("Theme", "MachineEvaluationStyle")
            .and_then(|value| MachineEvaluationStyle::from_str(&value).ok())
            .unwrap_or(default.machine_evaluation_style),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeShortcutOptions<K> {
    pub practice: K,
    pub song_search: K,
    pub load_songs: K,
    pub test_input: K,
}

pub fn load_theme_shortcut_options<K>(
    conf: &SimpleIni,
    default: ThemeShortcutOptions<K>,
    parse_key: impl Fn(&str) -> Option<K>,
) -> ThemeShortcutOptions<K>
where
    K: Copy,
{
    ThemeShortcutOptions {
        practice: conf
            .get("Theme", "SelectMusicShortcutPractice")
            .and_then(|value| parse_key(&value))
            .unwrap_or(default.practice),
        song_search: conf
            .get("Theme", "SelectMusicShortcutSongSearch")
            .and_then(|value| parse_key(&value))
            .unwrap_or(default.song_search),
        load_songs: conf
            .get("Theme", "SelectMusicShortcutLoadSongs")
            .and_then(|value| parse_key(&value))
            .unwrap_or(default.load_songs),
        test_input: conf
            .get("Theme", "SelectMusicShortcutTestInput")
            .and_then(|value| parse_key(&value))
            .unwrap_or(default.test_input),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeShortcutTokens<'a> {
    pub practice: &'a str,
    pub song_search: &'a str,
    pub load_songs: &'a str,
    pub test_input: &'a str,
}

pub fn push_theme_option_lines(
    content: &mut String,
    presentation: ThemePresentationOptions,
    machine: MachineFlowOptions,
    shortcuts: ThemeShortcutTokens<'_>,
) {
    push_bool(content, "KeyboardFeatures", presentation.keyboard_features);
    push_line(content, "VisualStyle", presentation.visual_style.as_str());
    push_line(content, "SrpgVariant", presentation.srpg_variant.as_str());
    push_bool(
        content,
        "VideoBackgrounds",
        presentation.show_video_backgrounds,
    );
    push_line(
        content,
        "RandomBackgroundMode",
        presentation.random_background_mode.as_str(),
    );
    push_bool(
        content,
        "MachineShowEvalSummary",
        machine.machine_show_eval_summary,
    );
    push_bool(content, "MachineNiceSound", machine.machine_nice_sound);
    push_bool(
        content,
        "MachineShowGameOver",
        machine.machine_show_gameover,
    );
    push_bool(
        content,
        "MachineShowNameEntry",
        machine.machine_show_name_entry,
    );
    push_bool(
        content,
        "MachineShowSelectColor",
        machine.machine_show_select_color,
    );
    push_bool(
        content,
        "MachineShowSelectPlayMode",
        machine.machine_show_select_play_mode,
    );
    push_bool(
        content,
        "MachineShowSelectProfile",
        machine.machine_show_select_profile,
    );
    push_bool(
        content,
        "AllowSwitchProfileInMenu",
        machine.allow_switch_profile_in_menu,
    );
    push_line(content, "SelectMusicShortcutPractice", shortcuts.practice);
    push_line(
        content,
        "SelectMusicShortcutSongSearch",
        shortcuts.song_search,
    );
    push_line(
        content,
        "SelectMusicShortcutLoadSongs",
        shortcuts.load_songs,
    );
    push_line(
        content,
        "SelectMusicShortcutTestInput",
        shortcuts.test_input,
    );
    push_bool(
        content,
        "MachineShowSelectStyle",
        machine.machine_show_select_style,
    );
    push_bool(
        content,
        "MachineEnableReplays",
        machine.machine_enable_replays,
    );
    push_bool(
        content,
        "MachineAllowPerPlayerGlobalOffsets",
        machine.machine_allow_per_player_global_offsets,
    );
    push_bool(
        content,
        "MachinePackIniOffsets",
        machine.machine_pack_ini_offsets,
    );
    push_line(
        content,
        "MachineDefaultSyncOffset",
        machine.machine_default_sync_offset.as_str(),
    );
    push_line(
        content,
        "MachinePreferredStyle",
        machine.machine_preferred_style.as_str(),
    );
    push_line(
        content,
        "MachinePreferredPlayMode",
        machine.machine_preferred_play_mode.as_str(),
    );
    push_line(content, "MachineFont", machine.machine_font.as_str());
    push_line(
        content,
        "MachineBarColor",
        machine.machine_bar_color.as_str(),
    );
    push_line(
        content,
        "MachineEvaluationStyle",
        machine.machine_evaluation_style.as_str(),
    );
    push_bool(
        content,
        "ShowSelectMusicGameplayTimer",
        presentation.show_select_music_gameplay_timer,
    );
    push_line(content, "SimplyLoveColor", presentation.simply_love_color);
    push_bool(
        content,
        "ZmodRatingBoxText",
        presentation.zmod_rating_box_text,
    );
    push_bool(content, "ShowBpmDecimal", presentation.show_bpm_decimal);
    push_line(
        content,
        "GameplayBpmPosition",
        presentation.gameplay_bpm_position.as_str(),
    );
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

    fn default_theme_presentation_options() -> ThemePresentationOptions {
        ThemePresentationOptions {
            simply_love_color: 2,
            show_select_music_gameplay_timer: true,
            keyboard_features: true,
            visual_style: VisualStyle::Hearts,
            srpg_variant: SrpgVariant::Srpg9,
            show_video_backgrounds: true,
            random_background_mode: RandomBackgroundMode::Off,
            zmod_rating_box_text: false,
            show_bpm_decimal: false,
            gameplay_bpm_position: GameplayBpmPosition::TopCenter,
        }
    }

    fn default_machine_flow_options() -> MachineFlowOptions {
        MachineFlowOptions {
            machine_show_eval_summary: true,
            machine_nice_sound: false,
            machine_show_name_entry: true,
            machine_show_gameover: true,
            machine_show_select_profile: true,
            allow_switch_profile_in_menu: false,
            machine_show_select_color: true,
            machine_show_select_style: true,
            machine_show_select_play_mode: true,
            machine_enable_replays: true,
            machine_allow_per_player_global_offsets: false,
            machine_pack_ini_offsets: false,
            machine_default_sync_offset: DefaultSyncOffset::Null,
            machine_preferred_style: MachinePreferredPlayStyle::Single,
            machine_preferred_play_mode: MachinePreferredPlayMode::Regular,
            machine_font: MachineFont::Wendy,
            machine_bar_color: MachineBarColor::Default,
            machine_evaluation_style: MachineEvaluationStyle::Default,
        }
    }

    #[test]
    fn writes_theme_option_lines() {
        let mut content = String::new();

        push_theme_option_lines(
            &mut content,
            default_theme_presentation_options(),
            default_machine_flow_options(),
            ThemeShortcutTokens {
                practice: "KeyP",
                song_search: "KeyS",
                load_songs: "KeyL",
                test_input: "KeyT",
            },
        );

        assert_eq!(
            content,
            "KeyboardFeatures=1\n\
VisualStyle=Hearts\n\
SrpgVariant=SRPG9\n\
VideoBackgrounds=1\n\
RandomBackgroundMode=Off\n\
MachineShowEvalSummary=1\n\
MachineNiceSound=0\n\
MachineShowGameOver=1\n\
MachineShowNameEntry=1\n\
MachineShowSelectColor=1\n\
MachineShowSelectPlayMode=1\n\
MachineShowSelectProfile=1\n\
AllowSwitchProfileInMenu=0\n\
SelectMusicShortcutPractice=KeyP\n\
SelectMusicShortcutSongSearch=KeyS\n\
SelectMusicShortcutLoadSongs=KeyL\n\
SelectMusicShortcutTestInput=KeyT\n\
MachineShowSelectStyle=1\n\
MachineEnableReplays=1\n\
MachineAllowPerPlayerGlobalOffsets=0\n\
MachinePackIniOffsets=0\n\
MachineDefaultSyncOffset=NULL\n\
MachinePreferredStyle=Single\n\
MachinePreferredPlayMode=Regular\n\
MachineFont=Wendy\n\
MachineBarColor=Default\n\
MachineEvaluationStyle=Default\n\
	ShowSelectMusicGameplayTimer=1\n\
	SimplyLoveColor=2\n\
	ZmodRatingBoxText=0\n\
	ShowBpmDecimal=0\n\
		GameplayBpmPosition=TopCenter\n"
        );
    }

    #[test]
    fn loads_theme_shortcut_options_with_token_parser() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Theme]
            SelectMusicShortcutPractice=p
            SelectMusicShortcutSongSearch=s
            SelectMusicShortcutLoadSongs=l
            SelectMusicShortcutTestInput=t
            "#,
        );

        let loaded = load_theme_shortcut_options(
            &conf,
            ThemeShortcutOptions {
                practice: 'a',
                song_search: 'a',
                load_songs: 'a',
                test_input: 'a',
            },
            |raw| raw.chars().next().filter(|ch| ch.is_ascii_lowercase()),
        );

        assert_eq!(
            loaded,
            ThemeShortcutOptions {
                practice: 'p',
                song_search: 's',
                load_songs: 'l',
                test_input: 't',
            },
        );
    }

    #[test]
    fn auto_screenshot_mask_roundtrips() {
        let mask = AUTO_SS_PBS | AUTO_SS_CLEARS | AUTO_SS_QUINTS;
        let encoded = auto_screenshot_mask_to_str(mask);
        assert_eq!(encoded, "PBs|Clears|Quints");
        assert_eq!(auto_screenshot_mask_from_str(&encoded), mask);
    }

    #[test]
    fn auto_screenshot_mask_handles_off_and_unknown_tokens() {
        assert_eq!(auto_screenshot_mask_from_str(""), 0);
        assert_eq!(auto_screenshot_mask_from_str("Off"), 0);
        assert_eq!(
            auto_screenshot_mask_from_str("PBs|unknown|Fails"),
            AUTO_SS_PBS | AUTO_SS_FAILS
        );
    }

    #[test]
    fn auto_screenshot_eval_policy_matches_enabled_result_flags() {
        assert!(!auto_screenshot_eval_matches(0, true, true, true, true));
        assert!(auto_screenshot_eval_matches(
            AUTO_SS_PBS,
            true,
            false,
            false,
            false
        ));
        assert!(auto_screenshot_eval_matches(
            AUTO_SS_FAILS,
            false,
            true,
            false,
            false
        ));
        assert!(auto_screenshot_eval_matches(
            AUTO_SS_CLEARS,
            false,
            false,
            false,
            false
        ));
        assert!(!auto_screenshot_eval_matches(
            AUTO_SS_CLEARS,
            false,
            true,
            false,
            false
        ));
        assert!(auto_screenshot_eval_matches(
            AUTO_SS_QUADS,
            false,
            false,
            true,
            false
        ));
        assert!(auto_screenshot_eval_matches(
            AUTO_SS_QUINTS,
            false,
            false,
            false,
            true
        ));
    }

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
    fn loads_theme_presentation_options_from_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Theme]
            SimplyLoveColor=7
            ShowSelectMusicGameplayTimer=0
            KeyboardFeatures=false
            VisualStyle=Technique
            SrpgVariant=SRPG10
            VideoBackgrounds=false
            RandomBackgroundMode=RandomMovies
            ZmodRatingBoxText=1
            ShowBpmDecimal=1
            GameplayBpmPosition=NearField
            "#,
        );

        let loaded = load_theme_presentation_options(&conf, default_theme_presentation_options());

        assert_eq!(loaded.simply_love_color, 7);
        assert!(!loaded.show_select_music_gameplay_timer);
        assert!(!loaded.keyboard_features);
        assert_eq!(loaded.visual_style, VisualStyle::Technique);
        assert_eq!(loaded.srpg_variant, SrpgVariant::Srpg10);
        assert!(!loaded.show_video_backgrounds);
        assert_eq!(
            loaded.random_background_mode,
            RandomBackgroundMode::RandomMovies
        );
        assert!(loaded.zmod_rating_box_text);
        assert!(loaded.show_bpm_decimal);
        assert_eq!(loaded.gameplay_bpm_position, GameplayBpmPosition::NearField);
    }

    #[test]
    fn theme_presentation_options_use_legacy_keys_and_defaults() {
        let default = default_theme_presentation_options();
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Theme]
            SimplyLoveColor=bad
            ShowSelectMusicGameplayTimer=bad
            KeyboardFeatures=bad
            MenuBackgroundStyle=Cats
            ThemeVariant=SRPG10
            VideoBackgrounds=bad
            RandomBackgroundMode=bad
            ZmodRatingBoxText=bad
            ShowBpmDecimal=bad
            GameplayBpmPosition=bad
            "#,
        );

        let loaded = load_theme_presentation_options(&conf, default);

        assert_eq!(loaded.simply_love_color, default.simply_love_color);
        assert_eq!(
            loaded.show_select_music_gameplay_timer,
            default.show_select_music_gameplay_timer
        );
        assert_eq!(loaded.keyboard_features, default.keyboard_features);
        assert_eq!(loaded.visual_style, VisualStyle::Cats);
        assert_eq!(loaded.srpg_variant, SrpgVariant::Srpg10);
        assert_eq!(
            loaded.show_video_backgrounds,
            default.show_video_backgrounds
        );
        assert_eq!(
            loaded.random_background_mode,
            default.random_background_mode
        );
        assert_eq!(loaded.zmod_rating_box_text, default.zmod_rating_box_text);
        assert_eq!(loaded.show_bpm_decimal, default.show_bpm_decimal);
        assert_eq!(loaded.gameplay_bpm_position, default.gameplay_bpm_position);
    }

    #[test]
    fn gameplay_bpm_position_round_trips_aliases() {
        assert_eq!(
            GameplayBpmPosition::from_str("TopCenter"),
            Ok(GameplayBpmPosition::TopCenter)
        );
        assert_eq!(
            GameplayBpmPosition::from_str("Normal"),
            Ok(GameplayBpmPosition::TopCenter)
        );
        assert_eq!(
            GameplayBpmPosition::from_str("Near Notefield"),
            Ok(GameplayBpmPosition::NearField)
        );
        assert!(GameplayBpmPosition::from_str("bottom").is_err());
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
    fn loads_machine_flow_options_from_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Theme]
            MachineShowEvalSummary=false
            MachineNiceSound=0
            MachineShowNameEntry=0
            MachineShowGameOver=0
            MachineShowSelectProfile=0
            AllowSwitchProfileInMenu=1
            MachineShowSelectColor=0
            MachineShowSelectStyle=0
            MachineShowSelectPlayMode=0
            MachineEnableReplays=0
            MachineAllowPerPlayerGlobalOffsets=1
            MachinePackIniOffsets=1
            MachineDefaultSyncOffset=ITG
            MachinePreferredStyle=Double
            MachinePreferredPlayMode=Marathon
            MachineFont=Mega
            MachineBarColor=Transparent
            MachineEvaluationStyle=Transparent
            "#,
        );

        let loaded = load_machine_flow_options(&conf, default_machine_flow_options());

        assert!(!loaded.machine_show_eval_summary);
        assert!(!loaded.machine_nice_sound);
        assert!(!loaded.machine_show_name_entry);
        assert!(!loaded.machine_show_gameover);
        assert!(!loaded.machine_show_select_profile);
        assert!(loaded.allow_switch_profile_in_menu);
        assert!(!loaded.machine_show_select_color);
        assert!(!loaded.machine_show_select_style);
        assert!(!loaded.machine_show_select_play_mode);
        assert!(!loaded.machine_enable_replays);
        assert!(loaded.machine_allow_per_player_global_offsets);
        assert!(loaded.machine_pack_ini_offsets);
        assert_eq!(loaded.machine_default_sync_offset, DefaultSyncOffset::Itg);
        assert_eq!(
            loaded.machine_preferred_style,
            MachinePreferredPlayStyle::Double
        );
        assert_eq!(
            loaded.machine_preferred_play_mode,
            MachinePreferredPlayMode::Marathon
        );
        assert_eq!(loaded.machine_font, MachineFont::Mega);
        assert_eq!(loaded.machine_bar_color, MachineBarColor::Transparent);
        assert_eq!(
            loaded.machine_evaluation_style,
            MachineEvaluationStyle::Transparent
        );
    }

    #[test]
    fn machine_flow_options_use_legacy_keys_and_defaults() {
        let default = default_machine_flow_options();
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Theme]
            MachineShowEvalSummary=bad
            MachineNiceSound=bad
            MachineShowNameEntry=bad
            MachineShowGameOver=bad
            MachineShowSelectProfile=bad
            AllowSwitchProfileInMenu=bad
            MachineShowSelectColor=bad
            MachineShowSelectStyle=bad
            MachineShowSelectPlayMode=bad
            MachineEnableReplays=bad
            MachineAllowPerPlayerGlobalOffsets=bad
            MachinePackIniOffsets=bad
            DefaultSyncOffset=ITG
            MachinePreferredStyle=bad
            MachinePreferredPlayMode=bad
            ThemeFont=Mega
            MachineBarColor=bad
            MachineEvaluationStyle=bad
            "#,
        );

        let loaded = load_machine_flow_options(&conf, default);

        assert_eq!(
            loaded.machine_show_eval_summary,
            default.machine_show_eval_summary
        );
        assert_eq!(loaded.machine_nice_sound, default.machine_nice_sound);
        assert_eq!(
            loaded.machine_show_name_entry,
            default.machine_show_name_entry
        );
        assert_eq!(loaded.machine_show_gameover, default.machine_show_gameover);
        assert_eq!(
            loaded.machine_show_select_profile,
            default.machine_show_select_profile
        );
        assert_eq!(
            loaded.allow_switch_profile_in_menu,
            default.allow_switch_profile_in_menu
        );
        assert_eq!(
            loaded.machine_show_select_color,
            default.machine_show_select_color
        );
        assert_eq!(
            loaded.machine_show_select_style,
            default.machine_show_select_style
        );
        assert_eq!(
            loaded.machine_show_select_play_mode,
            default.machine_show_select_play_mode
        );
        assert_eq!(
            loaded.machine_enable_replays,
            default.machine_enable_replays
        );
        assert_eq!(
            loaded.machine_allow_per_player_global_offsets,
            default.machine_allow_per_player_global_offsets
        );
        assert_eq!(
            loaded.machine_pack_ini_offsets,
            default.machine_pack_ini_offsets
        );
        assert_eq!(loaded.machine_default_sync_offset, DefaultSyncOffset::Itg);
        assert_eq!(
            loaded.machine_preferred_style,
            default.machine_preferred_style
        );
        assert_eq!(
            loaded.machine_preferred_play_mode,
            default.machine_preferred_play_mode
        );
        assert_eq!(loaded.machine_font, MachineFont::Mega);
        assert_eq!(loaded.machine_bar_color, default.machine_bar_color);
        assert_eq!(
            loaded.machine_evaluation_style,
            default.machine_evaluation_style
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

    #[test]
    fn normalize_locale_keeps_exact_region() {
        assert_eq!(normalize_locale("en-US"), "en-us");
        assert_eq!(normalize_locale("ja-JP"), "ja-jp");
        assert_eq!(normalize_locale("fr_FR.UTF-8"), "fr-fr");
        assert_eq!(normalize_locale("pt_BR"), "pt-br");
    }

    #[test]
    fn normalize_locale_handles_chinese_variants() {
        assert_eq!(normalize_locale("zh-TW"), "zh-Hant");
        assert_eq!(normalize_locale("zh-HK"), "zh-Hant");
        assert_eq!(normalize_locale("zh-Hant-TW"), "zh-Hant");
        assert_eq!(normalize_locale("zh-CN"), "zh-Hans");
        assert_eq!(normalize_locale("zh-SG"), "zh-Hans");
        assert_eq!(normalize_locale("zh-Hans-CN"), "zh-Hans");
        assert_eq!(normalize_locale("zh"), "zh-Hans");
    }

    #[test]
    fn resolve_language_locale_uses_explicit_flag_before_os() {
        assert_eq!(
            resolve_language_locale(LanguageFlag::Japanese, Some("fr-FR"), |_| true),
            "ja"
        );
    }

    #[test]
    fn resolve_language_locale_falls_back_through_region_and_english() {
        assert_eq!(
            resolve_language_locale(LanguageFlag::Auto, Some("fr-CA"), |code| code == "fr"),
            "fr"
        );
        assert_eq!(
            resolve_language_locale(LanguageFlag::Auto, Some("zz-ZZ"), |_| false),
            "en"
        );
    }
}
