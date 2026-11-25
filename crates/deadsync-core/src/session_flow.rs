//! Session preferences and routing around a theme's optional screens.

use crate::preference_key::NormalizedAsciiKey;
use std::str::FromStr;

/// The active dance game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameFlag {
    #[default]
    Dance,
    Pump,
}

impl GameFlag {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dance => "dance",
            Self::Pump => "pump",
        }
    }
}

impl FromStr for GameFlag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        match s.len() {
            4 if s.eq_ignore_ascii_case("pump") => Ok(Self::Pump),
            5 if s.eq_ignore_ascii_case("dance") => Ok(Self::Dance),
            _ => Err(()),
        }
    }
}

/// The cabinet's credit policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CoinMode {
    Pay,
    Free,
    #[default]
    Home,
}

impl CoinMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pay => "Pay",
            Self::Free => "Free",
            Self::Home => "Home",
        }
    }
}

impl FromStr for CoinMode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().as_bytes() {
            [a, b, c] if [*a | 0x20, *b | 0x20, *c | 0x20] == *b"pay" => Ok(Self::Pay),
            [a, b, c, d] => {
                const FREE: u32 = u32::from_le_bytes(*b"free");
                const HOME: u32 = u32::from_le_bytes(*b"home");
                match u32::from_le_bytes([*a, *b, *c, *d]) | 0x20202020 {
                    FREE => Ok(Self::Free),
                    HOME => Ok(Self::Home),
                    _ => Err(()),
                }
            }
            _ => Err(()),
        }
    }
}

/// Preferred number and arrangement of players before applying join state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachinePreferredPlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl MachinePreferredPlayStyle {
    #[must_use]
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
        let Some(key) = NormalizedAsciiKey::parse(s) else {
            return Err(());
        };
        match key.as_str() {
            "single" | "1player" | "oneplayer" => Ok(Self::Single),
            "versus" | "2player" | "2players" | "twoplayer" | "twoplayers" => Ok(Self::Versus),
            "double" => Ok(Self::Double),
            _ => Err(()),
        }
    }
}

/// Preferred session mode when the play mode screen is skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachinePreferredPlayMode {
    #[default]
    Regular,
    Marathon,
}

impl MachinePreferredPlayMode {
    #[must_use]
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
        let s = s.trim();
        match s.len() {
            7 if s.eq_ignore_ascii_case("regular") => Ok(Self::Regular),
            8 if s.eq_ignore_ascii_case("marathon") => Ok(Self::Marathon),
            _ => Err(()),
        }
    }
}

/// When to auto-show the `ArrowCloud` QR-login screen after the user picks
/// a profile.  Mirrors Simply Love's `QRLogin` theme pref.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionScreens<S> {
    pub menu: S,
    pub select_music: S,
    pub select_course: S,
    pub select_profile: Option<S>,
    pub select_color: Option<S>,
    pub select_style: Option<S>,
    pub select_play_mode: Option<S>,
    pub profile_load: S,
    pub evaluation_summary: Option<S>,
    pub initials: Option<S>,
    pub gameover: Option<S>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileLoginScreens<S> {
    pub groovestats: Option<S>,
    pub arrowcloud: Option<S>,
}

/// The selection that started the play session shown in Evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluationOrigin {
    Song,
    Course,
}

pub const fn evaluation_return_to<S: Copy>(
    screens: SessionScreens<S>,
    origin: EvaluationOrigin,
) -> S {
    match origin {
        EvaluationOrigin::Song => screens.select_music,
        EvaluationOrigin::Course => screens.select_course,
    }
}

pub fn profile_selection_target<S: Copy + Eq>(
    screens: SessionScreens<S>,
    login: ProfileLoginScreens<S>,
    current_screen: S,
    fast_switch: bool,
    show_groovestats_login: bool,
    show_arrowcloud_login: bool,
) -> Option<S> {
    if fast_switch {
        return (current_screen != screens.select_music).then_some(screens.select_music);
    }
    let startup = [
        login.groovestats,
        login.arrowcloud,
        screens.select_color,
        screens.select_style,
        screens.select_play_mode,
    ];
    let enabled = [
        show_groovestats_login,
        show_arrowcloud_login,
        true,
        true,
        true,
    ];
    Some(next_enabled_screen(&startup, &enabled).unwrap_or(screens.profile_load))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionNavigationPolicy {
    pub show_select_profile: bool,
    pub show_select_color: bool,
    pub show_select_style: bool,
    pub show_select_play_mode: bool,
    pub show_eval_summary: bool,
    pub show_name_entry: bool,
    pub show_gameover: bool,
}

impl SessionNavigationPolicy {
    /// A step is visible only when both the caller and the installed presentation allow it.
    pub const fn intersection(self, other: Self) -> Self {
        Self {
            show_select_profile: self.show_select_profile && other.show_select_profile,
            show_select_color: self.show_select_color && other.show_select_color,
            show_select_style: self.show_select_style && other.show_select_style,
            show_select_play_mode: self.show_select_play_mode && other.show_select_play_mode,
            show_eval_summary: self.show_eval_summary && other.show_eval_summary,
            show_name_entry: self.show_name_entry && other.show_name_entry,
            show_gameover: self.show_gameover && other.show_gameover,
        }
    }
}

impl Default for SessionNavigationPolicy {
    fn default() -> Self {
        Self {
            show_select_profile: true,
            show_select_color: true,
            show_select_style: true,
            show_select_play_mode: true,
            show_eval_summary: true,
            show_name_entry: true,
            show_gameover: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionNavigationPlan<S> {
    pub target: S,
    pub pending_post_select_summary_exit: Option<bool>,
    pub apply_preferred_style: bool,
    pub apply_preferred_play_mode: bool,
    pub initialize_session_side: bool,
}

pub fn resolve_navigation<S: Copy + Eq>(
    screens: SessionScreens<S>,
    policy: SessionNavigationPolicy,
    from: S,
    requested: S,
    has_played_stages: bool,
) -> SessionNavigationPlan<S> {
    let startup = [
        screens.select_profile,
        screens.select_color,
        screens.select_style,
        screens.select_play_mode,
    ];
    let startup_enabled = [
        policy.show_select_profile,
        policy.show_select_color,
        policy.show_select_style,
        policy.show_select_play_mode,
    ];
    let post_play = [
        screens.evaluation_summary,
        screens.initials,
        screens.gameover,
    ];
    let post_play_enabled = [
        policy.show_eval_summary,
        policy.show_name_entry,
        policy.show_gameover,
    ];

    let leaving_selection = from == screens.select_music || from == screens.select_course;
    let mut target = requested;
    let mut pending_post_select_summary_exit = None;
    if leaving_selection && target == screens.menu && has_played_stages {
        target = next_enabled_screen(&post_play, &post_play_enabled).unwrap_or(screens.menu);
        pending_post_select_summary_exit = Some(Some(target) == screens.evaluation_summary);
    } else if Some(target) == screens.evaluation_summary {
        pending_post_select_summary_exit = Some(false);
    }

    let startup_index = startup.iter().position(|screen| *screen == Some(target));
    let startup_flow = startup_index.is_some()
        || (target == screens.profile_load
            && (from == screens.menu || startup.contains(&Some(from))));
    if startup_flow && let Some(index) = startup_index {
        target = next_enabled_screen(&startup[index..], &startup_enabled[index..])
            .unwrap_or(screens.profile_load);
    }
    if let Some(index) = post_play.iter().position(|screen| *screen == Some(target)) {
        target = next_enabled_screen(&post_play[index..], &post_play_enabled[index..])
            .unwrap_or(screens.menu);
    }

    let style_visible = policy.show_select_style && screens.select_style.is_some();
    let play_mode_visible = policy.show_select_play_mode && screens.select_play_mode.is_some();
    let profile_visible = policy.show_select_profile && screens.select_profile.is_some();
    SessionNavigationPlan {
        target,
        pending_post_select_summary_exit,
        apply_preferred_style: startup_flow
            && !style_visible
            && (Some(target) == screens.select_play_mode || target == screens.profile_load),
        apply_preferred_play_mode: startup_flow
            && !play_mode_visible
            && target == screens.profile_load,
        initialize_session_side: startup_flow && from == screens.menu && !profile_visible,
    }
}

fn next_enabled_screen<S: Copy>(screens: &[Option<S>], enabled: &[bool]) -> Option<S> {
    screens
        .iter()
        .zip(enabled)
        .find_map(|(&screen, &enabled)| if enabled { screen } else { None })
}

pub fn evaluation_summary_return_to<S: Copy + Eq>(
    screens: SessionScreens<S>,
    previous: S,
    pending_post_select_summary_exit: bool,
) -> S {
    if !pending_post_select_summary_exit
        && (previous == screens.select_music || previous == screens.select_course)
    {
        previous
    } else {
        screens
            .initials
            .or(screens.gameover)
            .unwrap_or(screens.menu)
    }
}
