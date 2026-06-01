use std::path::PathBuf;
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
        assert_eq!(PlayStyle::default(), PlayStyle::Single);
        assert_eq!(PlayMode::default(), PlayMode::Regular);
        assert_eq!(PlayerSide::default(), PlayerSide::P1);
        assert_eq!(TimingTickMode::default(), TimingTickMode::Off);
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
}
