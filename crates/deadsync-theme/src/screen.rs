/// Application destinations shared by the installed DeadSync presentations.
///
/// The screen role identifies the controller the shell should run. Each
/// presentation chooses its own actors, transitions, and theme-local ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    Menu,
    Gameplay,
    Practice,
    Options,
    Credits,
    ContentBrowser,
    ManageLocalProfiles,
    Init,
    Initials,
    GameOver,
    Mappings,
    Input,
    SelectProfile,
    GrooveStatsLogin,
    ArrowCloudLogin,
    SelectColor,
    SelectStyle,
    SelectPlayMode,
    ProfileLoad,
    SelectMusic,
    SelectCourse,
    Sandbox,
    Evaluation,
    EvaluationSummary,
    PlayerOptions,
    TestLights,
    OverscanAdjustment,
    ConfigurePads,
    SmxAssignPads,
}

impl Screen {
    /// Destinations supported by the game's profile and session flow.
    pub const SESSION_SCREENS: deadsync_core::session_flow::SessionScreens<Self> =
        deadsync_core::session_flow::SessionScreens {
            menu: Self::Menu,
            select_music: Self::SelectMusic,
            select_course: Self::SelectCourse,
            select_profile: Some(Self::SelectProfile),
            select_color: Some(Self::SelectColor),
            select_style: Some(Self::SelectStyle),
            select_play_mode: Some(Self::SelectPlayMode),
            profile_load: Self::ProfileLoad,
            evaluation_summary: Some(Self::EvaluationSummary),
            initials: Some(Self::Initials),
            gameover: Some(Self::GameOver),
        };

    /// Account-link screens available during profile selection.
    pub const PROFILE_LOGIN_SCREENS: deadsync_core::session_flow::ProfileLoginScreens<Self> =
        deadsync_core::session_flow::ProfileLoginScreens {
            groovestats: Some(Self::GrooveStatsLogin),
            arrowcloud: Some(Self::ArrowCloudLogin),
        };

    /// Whether this screen owns an active or rehearsed gameplay stage.
    #[must_use]
    pub const fn is_play(self) -> bool {
        matches!(self, Self::Gameplay | Self::Practice)
    }

    /// Resolve a legacy destination named by song Lua. Gameplay reload has its own path.
    #[must_use]
    pub fn from_song_lua_name(name: &str) -> Option<Self> {
        [
            Self::Menu,
            Self::Practice,
            Self::Options,
            Self::Credits,
            Self::ContentBrowser,
            Self::ManageLocalProfiles,
            Self::Init,
            Self::Initials,
            Self::GameOver,
            Self::Mappings,
            Self::Input,
            Self::SelectProfile,
            Self::GrooveStatsLogin,
            Self::ArrowCloudLogin,
            Self::SelectColor,
            Self::SelectStyle,
            Self::SelectPlayMode,
            Self::ProfileLoad,
            Self::SelectMusic,
            Self::SelectCourse,
            Self::Sandbox,
            Self::Evaluation,
            Self::EvaluationSummary,
            Self::PlayerOptions,
            Self::TestLights,
            Self::OverscanAdjustment,
            Self::ConfigurePads,
            Self::SmxAssignPads,
        ]
        .into_iter()
        .find(|screen| screen.current_screen_file_name() == name)
    }

    /// Stable external screen name written to `save/current_screen.txt`.
    #[must_use]
    pub const fn current_screen_file_name(self) -> &'static str {
        match self {
            Self::Menu => "ScreenTitleMenu",
            Self::Gameplay => "ScreenGameplay",
            Self::Practice => "ScreenPractice",
            Self::Options => "ScreenOptionsService",
            Self::Credits => "ScreenCredits",
            Self::ContentBrowser => "ScreenContentBrowser",
            Self::ManageLocalProfiles => "ScreenOptionsManageProfiles",
            Self::Init => "ScreenInit",
            Self::Initials => "ScreenNameEntryTraditional",
            Self::GameOver => "ScreenGameOver",
            Self::Mappings => "ScreenMapControllers",
            Self::Input => "ScreenTestInput",
            Self::SelectProfile => "ScreenSelectProfile",
            Self::GrooveStatsLogin => "ScreenGrooveStatsLogin",
            Self::ArrowCloudLogin => "ScreenArrowCloudLogin",
            Self::SelectColor => "ScreenSelectColor",
            Self::SelectStyle => "ScreenSelectStyle",
            Self::SelectPlayMode => "ScreenSelectPlayMode",
            Self::ProfileLoad => "ScreenProfileLoad",
            Self::SelectMusic => "ScreenSelectMusic",
            Self::SelectCourse => "ScreenSelectCourse",
            Self::Sandbox => "ScreenSandbox",
            Self::Evaluation => "ScreenEvaluationStage",
            Self::EvaluationSummary => "ScreenEvaluationSummary",
            Self::PlayerOptions => "ScreenPlayerOptions",
            Self::TestLights => "ScreenTestLights",
            Self::OverscanAdjustment => "ScreenOverscanConfig",
            Self::ConfigurePads => "ScreenConfigurePads",
            Self::SmxAssignPads => "ScreenSmxAssignPads",
        }
    }
}

/// Stable identity for a screen supplied by a concrete theme.
///
/// The shell treats this value as opaque. Only the concrete theme interprets
/// the identifier or decides how it participates in that theme's screen flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThemeScreenId(&'static str);

impl ThemeScreenId {
    #[inline(always)]
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    #[inline(always)]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Generic identity/effect boundary implemented by each concrete theme.
pub trait Theme {
    type Screen: Copy + Eq;
    type RuntimeRequest;

    /// Optional session pages supported by this presentation.
    fn session_screen_support() -> deadsync_core::session_flow::SessionNavigationPolicy {
        deadsync_core::session_flow::SessionNavigationPolicy {
            show_select_color: false,
            ..Default::default()
        }
    }

    fn screen_id(screen: Self::Screen) -> ThemeScreenId;
}

#[cfg(test)]
mod tests {
    use super::ThemeScreenId;

    #[test]
    fn screen_ids_are_opaque_stable_strings() {
        const MENU: ThemeScreenId = ThemeScreenId::new("ScreenTitleMenu");
        assert_eq!(MENU.as_str(), "ScreenTitleMenu");
    }
}
