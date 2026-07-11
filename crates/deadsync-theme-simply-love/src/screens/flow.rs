use deadsync_input::VirtualAction;
use deadsync_profile::{
    PLAYER_SLOTS, PlayStyle, PlayerSide, Profile, player_side_index, preferred_difficulty_indices,
    profile_combo_carry,
};

/// Concrete screen identity for Simply Love.
///
/// This list and its redirects are theme-owned: another theme may expose a
/// completely different set of screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SimplyLoveScreen {
    Menu,
    Gameplay,
    Practice,
    Options,
    Credits,
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

impl SimplyLoveScreen {
    /// Opaque identity exposed through the generic theme contract.
    #[inline(always)]
    pub const fn id(self) -> deadsync_theme::ThemeScreenId {
        deadsync_theme::ThemeScreenId::new(self.id_str())
    }

    /// Stable theme-local identifier used when adapting to `ThemeScreenId`.
    pub const fn id_str(self) -> &'static str {
        match self {
            Self::Menu => "simply-love/menu",
            Self::Gameplay => "simply-love/gameplay",
            Self::Practice => "simply-love/practice",
            Self::Options => "simply-love/options",
            Self::Credits => "simply-love/credits",
            Self::ManageLocalProfiles => "simply-love/manage-local-profiles",
            Self::Init => "simply-love/init",
            Self::Initials => "simply-love/initials",
            Self::GameOver => "simply-love/game-over",
            Self::Mappings => "simply-love/mappings",
            Self::Input => "simply-love/input",
            Self::SelectProfile => "simply-love/select-profile",
            Self::GrooveStatsLogin => "simply-love/groovestats-login",
            Self::ArrowCloudLogin => "simply-love/arrowcloud-login",
            Self::SelectColor => "simply-love/select-color",
            Self::SelectStyle => "simply-love/select-style",
            Self::SelectPlayMode => "simply-love/select-play-mode",
            Self::ProfileLoad => "simply-love/profile-load",
            Self::SelectMusic => "simply-love/select-music",
            Self::SelectCourse => "simply-love/select-course",
            Self::Sandbox => "simply-love/sandbox",
            Self::Evaluation => "simply-love/evaluation",
            Self::EvaluationSummary => "simply-love/evaluation-summary",
            Self::PlayerOptions => "simply-love/player-options",
            Self::TestLights => "simply-love/test-lights",
            Self::OverscanAdjustment => "simply-love/overscan-adjustment",
            Self::ConfigurePads => "simply-love/configure-pads",
            Self::SmxAssignPads => "simply-love/smx-assign-pads",
        }
    }

    /// Stable external screen name written to `save/current_screen.txt`.
    pub const fn current_screen_file_name(self) -> &'static str {
        match self {
            Self::Menu => "ScreenTitleMenu",
            Self::Gameplay => "ScreenGameplay",
            Self::Practice => "ScreenPractice",
            Self::Options => "ScreenOptionsService",
            Self::Credits => "ScreenCredits",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelectionContext {
    pub play_style: PlayStyle,
    pub active_side: PlayerSide,
    pub fast_switch: bool,
    pub current_screen: SimplyLoveScreen,
    pub show_groovestats_login: bool,
    pub show_arrowcloud_login: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelectionPlan {
    pub combo_carry: [u32; PLAYER_SLOTS],
    pub preferred_active: usize,
    pub preferred_p2: usize,
    pub refresh_select_music: bool,
    pub navigation_target: Option<SimplyLoveScreen>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LateJoinContext {
    pub screen: SimplyLoveScreen,
    pub screen_allows_join: bool,
    pub play_style: PlayStyle,
    pub joined: [bool; PLAYER_SLOTS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicJoinContext {
    pub active_side: PlayerSide,
    pub join_side: PlayerSide,
    pub selected_steps: usize,
    pub preferred_difficulty: usize,
    pub p1_profile_preferred: usize,
    pub p2_profile_preferred: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicJoinPlan {
    pub selected_steps: usize,
    pub preferred_difficulty: usize,
    pub p2_selected_steps: usize,
    pub p2_preferred_difficulty: usize,
}

pub fn late_join_side(
    pressed: bool,
    action: VirtualAction,
    context: LateJoinContext,
) -> Option<PlayerSide> {
    if !pressed || !context.screen_allows_join || context.play_style == PlayStyle::Double {
        return None;
    }
    if !matches!(
        context.screen,
        SimplyLoveScreen::SelectColor
            | SimplyLoveScreen::SelectStyle
            | SimplyLoveScreen::SelectPlayMode
            | SimplyLoveScreen::SelectMusic
            | SimplyLoveScreen::SelectCourse
    ) {
        return None;
    }
    let side = match action {
        VirtualAction::p1_start => PlayerSide::P1,
        VirtualAction::p2_start => PlayerSide::P2,
        _ => return None,
    };
    let side_index = player_side_index(side);
    let joined_count = context.joined.into_iter().filter(|joined| *joined).count();
    (joined_count == 1 && !context.joined[side_index]).then_some(side)
}

pub const fn select_music_join_plan(context: SelectMusicJoinContext) -> SelectMusicJoinPlan {
    if matches!(context.active_side, PlayerSide::P2) && matches!(context.join_side, PlayerSide::P1)
    {
        SelectMusicJoinPlan {
            selected_steps: context.p1_profile_preferred,
            preferred_difficulty: context.p1_profile_preferred,
            p2_selected_steps: context.selected_steps,
            p2_preferred_difficulty: context.preferred_difficulty,
        }
    } else {
        SelectMusicJoinPlan {
            selected_steps: context.selected_steps,
            preferred_difficulty: context.preferred_difficulty,
            p2_selected_steps: context.p2_profile_preferred,
            p2_preferred_difficulty: context.p2_profile_preferred,
        }
    }
}

pub fn profile_selection_plan(
    profiles: &[Profile; PLAYER_SLOTS],
    context: ProfileSelectionContext,
) -> ProfileSelectionPlan {
    let preferred = preferred_difficulty_indices(profiles, context.play_style);
    let preferred_active = preferred[player_side_index(context.active_side)];
    let navigation_target = if context.fast_switch {
        (context.current_screen != SimplyLoveScreen::SelectMusic)
            .then_some(SimplyLoveScreen::SelectMusic)
    } else if context.show_groovestats_login {
        Some(SimplyLoveScreen::GrooveStatsLogin)
    } else if context.show_arrowcloud_login {
        Some(SimplyLoveScreen::ArrowCloudLogin)
    } else {
        Some(SimplyLoveScreen::SelectColor)
    };

    ProfileSelectionPlan {
        combo_carry: profile_combo_carry(profiles),
        preferred_active,
        preferred_p2: preferred[player_side_index(PlayerSide::P2)],
        refresh_select_music: context.fast_switch,
        navigation_target,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimplyLoveNavigationPlan {
    pub target: SimplyLoveScreen,
    pub pending_post_select_summary_exit: Option<bool>,
    pub apply_preferred_style: bool,
    pub apply_preferred_play_mode: bool,
    pub initialize_session_side: bool,
}

#[inline(always)]
const fn startup_screen_enabled(
    cfg: &deadsync_config::app_config::Config,
    screen: SimplyLoveScreen,
) -> bool {
    match screen {
        SimplyLoveScreen::SelectProfile => cfg.machine_show_select_profile,
        SimplyLoveScreen::SelectColor => cfg.machine_show_select_color,
        SimplyLoveScreen::SelectStyle => cfg.machine_show_select_style,
        SimplyLoveScreen::SelectPlayMode => cfg.machine_show_select_play_mode,
        _ => true,
    }
}

fn resolve_startup_target(
    cfg: &deadsync_config::app_config::Config,
    target: SimplyLoveScreen,
) -> SimplyLoveScreen {
    const ORDER: [SimplyLoveScreen; 4] = [
        SimplyLoveScreen::SelectProfile,
        SimplyLoveScreen::SelectColor,
        SimplyLoveScreen::SelectStyle,
        SimplyLoveScreen::SelectPlayMode,
    ];
    let Some(start_idx) = ORDER.iter().position(|screen| *screen == target) else {
        return target;
    };
    ORDER
        .iter()
        .skip(start_idx)
        .copied()
        .find(|screen| startup_screen_enabled(cfg, *screen))
        .unwrap_or(SimplyLoveScreen::ProfileLoad)
}

#[inline(always)]
const fn first_post_select_target(cfg: &deadsync_config::app_config::Config) -> SimplyLoveScreen {
    if cfg.machine_show_eval_summary {
        SimplyLoveScreen::EvaluationSummary
    } else if cfg.machine_show_name_entry {
        SimplyLoveScreen::Initials
    } else if cfg.machine_show_gameover {
        SimplyLoveScreen::GameOver
    } else {
        SimplyLoveScreen::Menu
    }
}

#[inline(always)]
const fn resolve_post_select_target(
    cfg: &deadsync_config::app_config::Config,
    target: SimplyLoveScreen,
) -> SimplyLoveScreen {
    match target {
        SimplyLoveScreen::EvaluationSummary => first_post_select_target(cfg),
        SimplyLoveScreen::Initials if !cfg.machine_show_name_entry => {
            if cfg.machine_show_gameover {
                SimplyLoveScreen::GameOver
            } else {
                SimplyLoveScreen::Menu
            }
        }
        SimplyLoveScreen::GameOver if !cfg.machine_show_gameover => SimplyLoveScreen::Menu,
        _ => target,
    }
}

/// Resolve Simply Love's optional startup and post-play screens.
pub fn resolve_navigation(
    cfg: &deadsync_config::app_config::Config,
    from: SimplyLoveScreen,
    requested: SimplyLoveScreen,
    has_played_stages: bool,
) -> SimplyLoveNavigationPlan {
    let mut target = requested;
    let mut pending_post_select_summary_exit = None;

    if matches!(
        from,
        SimplyLoveScreen::SelectMusic | SimplyLoveScreen::SelectCourse
    ) && target == SimplyLoveScreen::Menu
        && has_played_stages
    {
        target = first_post_select_target(cfg);
        pending_post_select_summary_exit = Some(target == SimplyLoveScreen::EvaluationSummary);
    } else if target == SimplyLoveScreen::EvaluationSummary {
        pending_post_select_summary_exit = Some(false);
    }

    let startup_flow = matches!(
        from,
        SimplyLoveScreen::Menu
            | SimplyLoveScreen::SelectProfile
            | SimplyLoveScreen::SelectColor
            | SimplyLoveScreen::SelectStyle
            | SimplyLoveScreen::SelectPlayMode
    ) && matches!(
        target,
        SimplyLoveScreen::SelectProfile
            | SimplyLoveScreen::SelectColor
            | SimplyLoveScreen::SelectStyle
            | SimplyLoveScreen::SelectPlayMode
            | SimplyLoveScreen::ProfileLoad
    );
    if startup_flow {
        target = resolve_startup_target(cfg, target);
    }
    target = resolve_post_select_target(cfg, target);

    SimplyLoveNavigationPlan {
        target,
        pending_post_select_summary_exit,
        apply_preferred_style: startup_flow
            && !cfg.machine_show_select_style
            && matches!(
                target,
                SimplyLoveScreen::SelectPlayMode | SimplyLoveScreen::ProfileLoad
            ),
        apply_preferred_play_mode: startup_flow
            && !cfg.machine_show_select_play_mode
            && target == SimplyLoveScreen::ProfileLoad,
        initialize_session_side: startup_flow
            && from == SimplyLoveScreen::Menu
            && target != SimplyLoveScreen::SelectProfile
            && !cfg.machine_show_select_profile
            && matches!(
                target,
                SimplyLoveScreen::SelectColor
                    | SimplyLoveScreen::SelectStyle
                    | SimplyLoveScreen::SelectPlayMode
                    | SimplyLoveScreen::ProfileLoad
            ),
    }
}

#[inline(always)]
pub const fn evaluation_summary_return_to(
    previous: SimplyLoveScreen,
    pending_post_select_summary_exit: bool,
) -> SimplyLoveScreen {
    if pending_post_select_summary_exit {
        return SimplyLoveScreen::Initials;
    }
    match previous {
        SimplyLoveScreen::SelectMusic => SimplyLoveScreen::SelectMusic,
        SimplyLoveScreen::SelectCourse => SimplyLoveScreen::SelectCourse,
        _ => SimplyLoveScreen::Initials,
    }
}

/// Whether Simply Love renders this screen's transition with actor tweens.
pub const fn uses_actor_fade(screen: SimplyLoveScreen) -> bool {
    matches!(
        screen,
        SimplyLoveScreen::Menu
            | SimplyLoveScreen::Options
            | SimplyLoveScreen::ManageLocalProfiles
            | SimplyLoveScreen::Mappings
            | SimplyLoveScreen::Input
            | SimplyLoveScreen::TestLights
            | SimplyLoveScreen::OverscanAdjustment
            | SimplyLoveScreen::SmxAssignPads
            | SimplyLoveScreen::SelectProfile
            | SimplyLoveScreen::SelectColor
    )
}

/// Simply Love's exact actor-only transition pairs.
pub const fn uses_actor_only_transition(from: SimplyLoveScreen, to: SimplyLoveScreen) -> bool {
    matches!(
        (from, to),
        (
            SimplyLoveScreen::Menu,
            SimplyLoveScreen::Options
                | SimplyLoveScreen::SelectProfile
                | SimplyLoveScreen::SelectColor
        ) | (
            SimplyLoveScreen::Options
                | SimplyLoveScreen::SelectProfile
                | SimplyLoveScreen::SelectColor,
            SimplyLoveScreen::Menu
        ) | (
            SimplyLoveScreen::SelectProfile,
            SimplyLoveScreen::SelectColor | SimplyLoveScreen::SelectStyle
        ) | (
            SimplyLoveScreen::SelectStyle,
            SimplyLoveScreen::SelectProfile | SimplyLoveScreen::SelectColor
        ) | (SimplyLoveScreen::SelectColor, SimplyLoveScreen::SelectStyle)
            | (
                SimplyLoveScreen::Options,
                SimplyLoveScreen::Mappings
                    | SimplyLoveScreen::TestLights
                    | SimplyLoveScreen::OverscanAdjustment
                    | SimplyLoveScreen::SmxAssignPads
                    | SimplyLoveScreen::ManageLocalProfiles
            )
            | (
                SimplyLoveScreen::Mappings
                    | SimplyLoveScreen::TestLights
                    | SimplyLoveScreen::OverscanAdjustment
                    | SimplyLoveScreen::SmxAssignPads
                    | SimplyLoveScreen::ManageLocalProfiles,
                SimplyLoveScreen::Options
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_config::app_config::Config;

    #[test]
    fn identifiers_are_theme_scoped_and_unique() {
        let screens = [
            SimplyLoveScreen::Menu,
            SimplyLoveScreen::Gameplay,
            SimplyLoveScreen::Practice,
            SimplyLoveScreen::Options,
            SimplyLoveScreen::Credits,
            SimplyLoveScreen::ManageLocalProfiles,
            SimplyLoveScreen::Init,
            SimplyLoveScreen::Initials,
            SimplyLoveScreen::GameOver,
            SimplyLoveScreen::Mappings,
            SimplyLoveScreen::Input,
            SimplyLoveScreen::SelectProfile,
            SimplyLoveScreen::GrooveStatsLogin,
            SimplyLoveScreen::ArrowCloudLogin,
            SimplyLoveScreen::SelectColor,
            SimplyLoveScreen::SelectStyle,
            SimplyLoveScreen::SelectPlayMode,
            SimplyLoveScreen::ProfileLoad,
            SimplyLoveScreen::SelectMusic,
            SimplyLoveScreen::SelectCourse,
            SimplyLoveScreen::Sandbox,
            SimplyLoveScreen::Evaluation,
            SimplyLoveScreen::EvaluationSummary,
            SimplyLoveScreen::PlayerOptions,
            SimplyLoveScreen::TestLights,
            SimplyLoveScreen::OverscanAdjustment,
            SimplyLoveScreen::ConfigurePads,
            SimplyLoveScreen::SmxAssignPads,
        ];
        let mut ids = screens.map(SimplyLoveScreen::id_str);
        ids.sort_unstable();
        assert!(ids.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(ids.iter().all(|id| id.starts_with("simply-love/")));
    }

    #[test]
    fn compatibility_file_names_match_simply_love_names() {
        assert_eq!(
            SimplyLoveScreen::Menu.current_screen_file_name(),
            "ScreenTitleMenu"
        );
        assert_eq!(
            SimplyLoveScreen::Evaluation.current_screen_file_name(),
            "ScreenEvaluationStage"
        );
        assert_eq!(
            SimplyLoveScreen::ManageLocalProfiles.current_screen_file_name(),
            "ScreenOptionsManageProfiles"
        );
    }

    #[test]
    fn startup_flow_skips_disabled_screens() {
        let cfg = Config {
            machine_show_select_profile: false,
            machine_show_select_color: false,
            machine_show_select_style: true,
            ..Config::default()
        };
        let plan = resolve_navigation(
            &cfg,
            SimplyLoveScreen::Menu,
            SimplyLoveScreen::SelectProfile,
            false,
        );
        assert_eq!(plan.target, SimplyLoveScreen::SelectStyle);
        assert!(plan.initialize_session_side);
    }

    #[test]
    fn actor_transition_policy_is_theme_owned() {
        assert!(uses_actor_fade(SimplyLoveScreen::Menu));
        assert!(uses_actor_only_transition(
            SimplyLoveScreen::Menu,
            SimplyLoveScreen::SelectProfile,
        ));
        assert!(!uses_actor_only_transition(
            SimplyLoveScreen::Gameplay,
            SimplyLoveScreen::Evaluation,
        ));
    }
}
