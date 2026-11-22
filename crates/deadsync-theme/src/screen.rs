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

    fn screen_id(screen: Self::Screen) -> ThemeScreenId;

    fn current_screen_file_name(screen: Self::Screen) -> &'static str {
        Self::screen_id(screen).as_str()
    }
}

/// The screens that connect a theme's flow to the application's game sessions.
/// A theme can supply additional screens and choose its own presentation and IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeScreens<S> {
    pub menu: S,
    pub gameplay: S,
    pub practice: S,
    pub results: S,
    pub song_selection: S,
    pub course_selection: S,
    pub player_options: S,
}

/// Supplies the distinct screens used by the shared game flow.
pub trait ThemeScreenFlow: Theme {
    const SCREENS: ThemeScreens<Self::Screen>;

    /// Resolve a song Lua name for a screen outside the standard game flow.
    fn named_song_lua_screen(_name: &str) -> Option<Self::Screen> {
        None
    }
}

/// Optional startup and post-play screens supplied to the game's session flow.
pub trait ThemeSessionFlow: ThemeScreenFlow {
    const SESSION_SCREENS: deadsync_core::session_flow::SessionScreens<Self::Screen>;
    const PROFILE_LOGIN_SCREENS: deadsync_core::session_flow::ProfileLoginScreens<Self::Screen> =
        deadsync_core::session_flow::ProfileLoginScreens {
            groovestats: None,
            arrowcloud: None,
        };
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
