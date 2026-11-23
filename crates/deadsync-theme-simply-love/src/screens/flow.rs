use deadsync_input::VirtualAction;
use deadsync_profile::{
    PLAYER_SLOTS, PlayStyle, PlayerSide, joined_player_mask, late_join_play_style,
    player_side_index,
};
pub use deadsync_profile_gameplay::stage_charts::{
    SelectMusicJoinContext, SelectMusicJoinPlan, select_music_join_plan,
};
use deadsync_theme::Screen;

pub(crate) const fn screen_id(screen: Screen) -> deadsync_theme::ThemeScreenId {
    deadsync_theme::ThemeScreenId::new(match screen {
        Screen::Menu => "simply-love/menu",
        Screen::Gameplay => "simply-love/gameplay",
        Screen::Practice => "simply-love/practice",
        Screen::Options => "simply-love/options",
        Screen::Credits => "simply-love/credits",
        Screen::ContentBrowser => "simply-love/content-browser",
        Screen::ManageLocalProfiles => "simply-love/manage-local-profiles",
        Screen::Init => "simply-love/init",
        Screen::Initials => "simply-love/initials",
        Screen::GameOver => "simply-love/game-over",
        Screen::Mappings => "simply-love/mappings",
        Screen::Input => "simply-love/input",
        Screen::SelectProfile => "simply-love/select-profile",
        Screen::GrooveStatsLogin => "simply-love/groovestats-login",
        Screen::ArrowCloudLogin => "simply-love/arrowcloud-login",
        Screen::SelectColor => "simply-love/select-color",
        Screen::SelectStyle => "simply-love/select-style",
        Screen::SelectPlayMode => "simply-love/select-play-mode",
        Screen::ProfileLoad => "simply-love/profile-load",
        Screen::SelectMusic => "simply-love/select-music",
        Screen::SelectCourse => "simply-love/select-course",
        Screen::Sandbox => "simply-love/sandbox",
        Screen::Evaluation => "simply-love/evaluation",
        Screen::EvaluationSummary => "simply-love/evaluation-summary",
        Screen::PlayerOptions => "simply-love/player-options",
        Screen::TestLights => "simply-love/test-lights",
        Screen::OverscanAdjustment => "simply-love/overscan-adjustment",
        Screen::ConfigurePads => "simply-love/configure-pads",
        Screen::SmxAssignPads => "simply-love/smx-assign-pads",
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelectionContext {
    pub preferred_difficulties: [usize; PLAYER_SLOTS],
    pub active_side: PlayerSide,
    pub fast_switch: bool,
    pub current_screen: Screen,
    pub show_groovestats_login: bool,
    pub show_arrowcloud_login: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelectionPlan {
    pub preferred_active: usize,
    pub preferred_p2: usize,
    pub refresh_select_music: bool,
    pub navigation_target: Option<Screen>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LateJoinContext {
    pub screen: Screen,
    pub screen_allows_join: bool,
    pub play_style: PlayStyle,
    pub joined: [bool; PLAYER_SLOTS],
}

#[must_use]
pub fn late_join_side(
    pressed: bool,
    action: VirtualAction,
    context: LateJoinContext,
) -> Option<PlayerSide> {
    if !pressed || !context.screen_allows_join {
        return None;
    }
    if !matches!(
        context.screen,
        Screen::SelectColor
            | Screen::SelectStyle
            | Screen::SelectPlayMode
            | Screen::SelectMusic
            | Screen::SelectCourse
    ) {
        return None;
    }
    let side = match action {
        VirtualAction::p1_start => PlayerSide::P1,
        VirtualAction::p2_start => PlayerSide::P2,
        _ => return None,
    };
    let joined_mask = joined_player_mask(context.joined[0], context.joined[1]);
    late_join_play_style(context.play_style, joined_mask, side).map(|_| side)
}

#[must_use]
pub fn profile_selection_plan(context: ProfileSelectionContext) -> ProfileSelectionPlan {
    ProfileSelectionPlan {
        preferred_active: context.preferred_difficulties[player_side_index(context.active_side)],
        preferred_p2: context.preferred_difficulties[player_side_index(PlayerSide::P2)],
        refresh_select_music: context.fast_switch,
        navigation_target: profile_selection_target(
            context.current_screen,
            context.fast_switch,
            context.show_groovestats_login,
            context.show_arrowcloud_login,
        ),
    }
}

#[must_use]
pub fn profile_selection_target(
    current_screen: Screen,
    fast_switch: bool,
    show_groovestats_login: bool,
    show_arrowcloud_login: bool,
) -> Option<Screen> {
    use deadsync_theme::ThemeSessionFlow;

    deadsync_core::session_flow::profile_selection_target(
        crate::SimplyLoveTheme::SESSION_SCREENS,
        crate::SimplyLoveTheme::PROFILE_LOGIN_SCREENS,
        current_screen,
        fast_switch,
        show_groovestats_login,
        show_arrowcloud_login,
    )
}

pub type SimplyLoveNavigationPlan =
    deadsync_core::session_flow::SessionNavigationPlan<Screen>;
pub use deadsync_core::session_flow::SessionNavigationPolicy as SimplyLoveNavigationPolicy;

/// Resolve Simply Love's optional startup and post-play screens.
#[must_use]
pub fn resolve_navigation(
    policy: SimplyLoveNavigationPolicy,
    from: Screen,
    requested: Screen,
    has_played_stages: bool,
) -> SimplyLoveNavigationPlan {
    use deadsync_theme::ThemeSessionFlow;

    deadsync_core::session_flow::resolve_navigation(
        crate::SimplyLoveTheme::SESSION_SCREENS,
        policy,
        from,
        requested,
        has_played_stages,
    )
}

#[inline(always)]
#[must_use]
pub const fn evaluation_summary_return_to(
    previous: Screen,
    pending_post_select_summary_exit: bool,
) -> Screen {
    if pending_post_select_summary_exit {
        return Screen::Initials;
    }
    match previous {
        Screen::SelectMusic => Screen::SelectMusic,
        Screen::SelectCourse => Screen::SelectCourse,
        _ => Screen::Initials,
    }
}

/// Whether Simply Love renders this screen's transition with actor tweens.
#[must_use]
pub const fn uses_actor_fade(screen: Screen) -> bool {
    matches!(
        screen,
        Screen::Menu
            | Screen::Options
            | Screen::ManageLocalProfiles
            | Screen::Mappings
            | Screen::Input
            | Screen::TestLights
            | Screen::OverscanAdjustment
            | Screen::SmxAssignPads
            | Screen::SelectColor
    )
}

/// Simply Love's exact actor-only transition pairs.
#[must_use]
pub const fn uses_actor_only_transition(from: Screen, to: Screen) -> bool {
    if matches!(from, Screen::Menu) {
        return !matches!(to, Screen::Menu | Screen::Init);
    }
    matches!(
        (from, to),
        (
            Screen::Options
                | Screen::SelectProfile
                | Screen::SelectColor,
            Screen::Menu
        ) | (
            Screen::SelectProfile,
            Screen::SelectColor | Screen::SelectStyle
        ) | (
            Screen::SelectStyle,
            Screen::SelectProfile | Screen::SelectColor
        ) | (Screen::SelectColor, Screen::SelectStyle)
            | (
                Screen::Options,
                Screen::Mappings
                    | Screen::TestLights
                    | Screen::OverscanAdjustment
                    | Screen::SmxAssignPads
                    | Screen::ManageLocalProfiles
            )
            | (
                Screen::Mappings
                    | Screen::TestLights
                    | Screen::OverscanAdjustment
                    | Screen::SmxAssignPads
                    | Screen::ManageLocalProfiles,
                Screen::Options
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_theme_scoped_and_unique() {
        let screens = [
            Screen::Menu,
            Screen::Gameplay,
            Screen::Practice,
            Screen::Options,
            Screen::Credits,
            Screen::ContentBrowser,
            Screen::ManageLocalProfiles,
            Screen::Init,
            Screen::Initials,
            Screen::GameOver,
            Screen::Mappings,
            Screen::Input,
            Screen::SelectProfile,
            Screen::GrooveStatsLogin,
            Screen::ArrowCloudLogin,
            Screen::SelectColor,
            Screen::SelectStyle,
            Screen::SelectPlayMode,
            Screen::ProfileLoad,
            Screen::SelectMusic,
            Screen::SelectCourse,
            Screen::Sandbox,
            Screen::Evaluation,
            Screen::EvaluationSummary,
            Screen::PlayerOptions,
            Screen::TestLights,
            Screen::OverscanAdjustment,
            Screen::ConfigurePads,
            Screen::SmxAssignPads,
        ];
        let mut ids = screens.map(|screen| screen_id(screen).as_str());
        ids.sort_unstable();
        assert!(ids.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(ids.iter().all(|id| id.starts_with("simply-love/")));
    }

    #[test]
    fn compatibility_file_names_match_external_names() {
        assert_eq!(
            Screen::Menu.current_screen_file_name(),
            "ScreenTitleMenu"
        );
        assert_eq!(
            Screen::Evaluation.current_screen_file_name(),
            "ScreenEvaluationStage"
        );
        assert_eq!(
            Screen::ManageLocalProfiles.current_screen_file_name(),
            "ScreenOptionsManageProfiles"
        );
    }

    #[test]
    fn startup_flow_skips_disabled_screens() {
        let policy = SimplyLoveNavigationPolicy {
            show_select_profile: false,
            show_select_color: false,
            show_select_style: true,
            ..Default::default()
        };
        let plan = resolve_navigation(
            policy,
            Screen::Menu,
            Screen::SelectProfile,
            false,
        );
        assert_eq!(plan.target, Screen::SelectStyle);
        assert!(plan.initialize_session_side);
    }

    #[test]
    fn profile_selection_uses_prepared_player_values() {
        let plan = profile_selection_plan(ProfileSelectionContext {
            preferred_difficulties: [2, 5],
            active_side: PlayerSide::P2,
            fast_switch: true,
            current_screen: Screen::SelectProfile,
            show_groovestats_login: false,
            show_arrowcloud_login: false,
        });
        assert_eq!(plan.preferred_active, 5);
        assert_eq!(plan.preferred_p2, 5);
        assert_eq!(plan.navigation_target, Some(Screen::SelectMusic));
    }

    #[test]
    fn actor_transition_policy_is_theme_owned() {
        assert!(uses_actor_fade(Screen::Menu));
        assert!(!uses_actor_fade(Screen::SelectProfile));
        for target in [
            Screen::SelectProfile,
            Screen::SelectColor,
            Screen::SelectStyle,
            Screen::ProfileLoad,
            Screen::SelectMusic,
        ] {
            assert!(uses_actor_only_transition(Screen::Menu, target));
        }
        assert!(!uses_actor_only_transition(
            Screen::Gameplay,
            Screen::Evaluation,
        ));
        assert!(!uses_actor_only_transition(
            Screen::Menu,
            Screen::Init,
        ));
    }
}
