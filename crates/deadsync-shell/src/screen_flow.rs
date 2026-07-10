use crate::Command;
use crate::interaction::ProcessExitRequest;
use crate::navigation::{machine_flow_screen, screen_from_machine_flow};
use deadlib_platform::dirs;
use deadsync_config::app_config::Config;
use deadsync_config::navigation::{
    machine_first_post_select_target, machine_resolve_post_select_target,
    machine_resolve_startup_target,
};
use deadsync_input::VirtualAction;
use deadsync_profile::{
    PLAYER_SLOTS, PlayStyle, PlayerSide, Profile, player_side_index, preferred_difficulty_indices,
    profile_combo_carry,
};
use deadsync_screens::{Screen, ScreenAction};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationRoutePlan {
    pub target: Screen,
    pub pending_post_select_summary_exit: Option<bool>,
    pub apply_preferred_style: bool,
    pub apply_preferred_play_mode: bool,
    pub initialize_session_side: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenActionRouteContext {
    pub current_screen: Screen,
    pub restart_pending: bool,
    pub course_active: bool,
    pub course_has_next_stage: bool,
    pub gameplay_failed: bool,
}

#[derive(Clone, Debug)]
pub struct ScreenActionRoutePlan {
    pub action: ScreenAction,
    pub clear_restart_pending: bool,
}

pub struct OnlineProfileLinkPlan {
    pub target: Screen,
    pub profile_id: String,
    pub display_name: String,
}

pub enum ScreenActionEffect {
    None,
    Navigate(Screen),
    NavigateNoFade(Screen),
    ProcessExit(ProcessExitRequest),
    RequestScreenshot(Option<PlayerSide>),
    RunCommands(Vec<Command>),
    LinkOnlineProfile(OnlineProfileLinkPlan),
    WriteFsrDump { path: PathBuf },
    Root(ScreenAction),
}

pub struct ScreenActionEffectPlan {
    pub effect: ScreenActionEffect,
    pub clear_restart_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelectionContext {
    pub play_style: PlayStyle,
    pub active_side: PlayerSide,
    pub fast_switch: bool,
    pub current_screen: Screen,
    pub show_groovestats_login: bool,
    pub show_arrowcloud_login: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelectionPlan {
    pub combo_carry: [u32; PLAYER_SLOTS],
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
        (context.current_screen != Screen::SelectMusic).then_some(Screen::SelectMusic)
    } else if context.show_groovestats_login {
        Some(Screen::GrooveStatsLogin)
    } else if context.show_arrowcloud_login {
        Some(Screen::ArrowCloudLogin)
    } else {
        Some(Screen::SelectColor)
    };

    ProfileSelectionPlan {
        combo_carry: profile_combo_carry(profiles),
        preferred_active,
        preferred_p2: preferred[player_side_index(PlayerSide::P2)],
        refresh_select_music: context.fast_switch,
        navigation_target,
    }
}

pub fn screen_action_route_plan(
    action: ScreenAction,
    context: ScreenActionRouteContext,
) -> ScreenActionRoutePlan {
    let (action, clear_restart_pending) = match action {
        // SL/zmod parity: a restart-triggered Cancel exit returns to the wheel.
        // Redirect it to Gameplay so the player skips the wheel round-trip.
        ScreenAction::NavigateNoFade(Screen::SelectMusic)
            if context.restart_pending && context.current_screen == Screen::Gameplay =>
        {
            (ScreenAction::NavigateNoFade(Screen::Gameplay), true)
        }
        ScreenAction::Navigate(Screen::Evaluation)
            if context.current_screen == Screen::Gameplay
                && context.course_has_next_stage
                && !context.gameplay_failed =>
        {
            (ScreenAction::Navigate(Screen::Gameplay), false)
        }
        ScreenAction::Navigate(Screen::SelectMusic)
            if context.current_screen == Screen::Gameplay && context.course_active =>
        {
            (ScreenAction::Navigate(Screen::SelectCourse), false)
        }
        ScreenAction::NavigateNoFade(Screen::SelectMusic)
            if context.current_screen == Screen::Gameplay && context.course_active =>
        {
            (ScreenAction::NavigateNoFade(Screen::SelectCourse), false)
        }
        action => (action, false),
    };
    ScreenActionRoutePlan {
        action,
        clear_restart_pending,
    }
}

pub fn screen_action_effect_plan(
    action: ScreenAction,
    context: ScreenActionRouteContext,
) -> ScreenActionEffectPlan {
    let route = match action {
        action @ (ScreenAction::Navigate(_) | ScreenAction::NavigateNoFade(_)) => {
            screen_action_route_plan(action, context)
        }
        action => ScreenActionRoutePlan {
            action,
            clear_restart_pending: false,
        },
    };

    let effect = match route.action {
        ScreenAction::None | ScreenAction::ConsumeInput => ScreenActionEffect::None,
        ScreenAction::Navigate(screen) => ScreenActionEffect::Navigate(screen),
        ScreenAction::NavigateNoFade(screen) => ScreenActionEffect::NavigateNoFade(screen),
        ScreenAction::Exit => ScreenActionEffect::ProcessExit(ProcessExitRequest::Exit),
        ScreenAction::Shutdown => ScreenActionEffect::ProcessExit(ProcessExitRequest::Shutdown),
        ScreenAction::LinkArrowCloud {
            profile_id,
            display_name,
        } => ScreenActionEffect::LinkOnlineProfile(OnlineProfileLinkPlan {
            target: Screen::ArrowCloudLogin,
            profile_id,
            display_name,
        }),
        ScreenAction::LinkGrooveStats {
            profile_id,
            display_name,
        } => ScreenActionEffect::LinkOnlineProfile(OnlineProfileLinkPlan {
            target: Screen::GrooveStatsLogin,
            profile_id,
            display_name,
        }),
        ScreenAction::RequestScreenshot(side) => ScreenActionEffect::RequestScreenshot(side),
        ScreenAction::RequestBanner(path_opt) => {
            ScreenActionEffect::RunCommands(vec![Command::SetBanner(path_opt)])
        }
        ScreenAction::RequestCdTitle(path_opt) => {
            ScreenActionEffect::RunCommands(vec![Command::SetCdTitle(path_opt)])
        }
        ScreenAction::RequestPackBanner(path_opt) => {
            ScreenActionEffect::RunCommands(vec![Command::SetPackBanner(path_opt)])
        }
        ScreenAction::RequestWheelItemBackgrounds(paths) => {
            ScreenActionEffect::RunCommands(vec![Command::SetWheelItemBackgrounds(paths)])
        }
        ScreenAction::RequestDensityGraph { slot, chart_opt } => {
            ScreenActionEffect::RunCommands(vec![Command::SetDensityGraph { slot, chart_opt }])
        }
        ScreenAction::FetchOnlineGrade(hash) => {
            ScreenActionEffect::RunCommands(vec![Command::FetchOnlineGrade(hash)])
        }
        ScreenAction::WriteFsrDump => ScreenActionEffect::WriteFsrDump {
            path: dirs::app_dirs().data_dir.join("fsrdump.txt"),
        },
        other => ScreenActionEffect::Root(other),
    };

    ScreenActionEffectPlan {
        effect,
        clear_restart_pending: route.clear_restart_pending,
    }
}

pub fn navigation_route_plan(
    cfg: &Config,
    from: Screen,
    requested: Screen,
    has_played_stages: bool,
) -> NavigationRoutePlan {
    let mut target = requested;
    let mut pending_post_select_summary_exit = None;

    if matches!(from, Screen::SelectMusic | Screen::SelectCourse)
        && target == Screen::Menu
        && has_played_stages
    {
        target = screen_from_machine_flow(machine_first_post_select_target(cfg));
        pending_post_select_summary_exit = Some(target == Screen::EvaluationSummary);
    } else if target == Screen::EvaluationSummary {
        pending_post_select_summary_exit = Some(false);
    }

    let startup_flow = matches!(
        from,
        Screen::Menu
            | Screen::SelectProfile
            | Screen::SelectColor
            | Screen::SelectStyle
            | Screen::SelectPlayMode
    ) && matches!(
        target,
        Screen::SelectProfile
            | Screen::SelectColor
            | Screen::SelectStyle
            | Screen::SelectPlayMode
            | Screen::ProfileLoad
    );
    if startup_flow && let Some(route) = machine_flow_screen(target) {
        target = screen_from_machine_flow(machine_resolve_startup_target(cfg, route));
    }
    if let Some(route) = machine_flow_screen(target) {
        target = screen_from_machine_flow(machine_resolve_post_select_target(cfg, route));
    }

    NavigationRoutePlan {
        target,
        pending_post_select_summary_exit,
        apply_preferred_style: startup_flow
            && !cfg.machine_show_select_style
            && matches!(target, Screen::SelectPlayMode | Screen::ProfileLoad),
        apply_preferred_play_mode: startup_flow
            && !cfg.machine_show_select_play_mode
            && target == Screen::ProfileLoad,
        initialize_session_side: startup_flow
            && from == Screen::Menu
            && target != Screen::SelectProfile
            && !cfg.machine_show_select_profile
            && matches!(
                target,
                Screen::SelectColor
                    | Screen::SelectStyle
                    | Screen::SelectPlayMode
                    | Screen::ProfileLoad
            ),
    }
}

#[inline(always)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_select_exit_enters_configured_summary_flow() {
        let cfg = Config {
            machine_show_eval_summary: true,
            ..Config::default()
        };
        let plan = navigation_route_plan(&cfg, Screen::SelectMusic, Screen::Menu, true);
        assert_eq!(plan.target, Screen::EvaluationSummary);
        assert_eq!(plan.pending_post_select_summary_exit, Some(true));

        let plan = navigation_route_plan(&cfg, Screen::SelectMusic, Screen::Menu, false);
        assert_eq!(plan.target, Screen::Menu);
        assert_eq!(plan.pending_post_select_summary_exit, None);
    }

    #[test]
    fn skipped_startup_screens_request_root_session_defaults() {
        let cfg = Config {
            machine_show_select_profile: false,
            machine_show_select_color: false,
            machine_show_select_style: false,
            machine_show_select_play_mode: false,
            ..Config::default()
        };
        let plan = navigation_route_plan(&cfg, Screen::Menu, Screen::SelectProfile, false);
        assert_eq!(plan.target, Screen::ProfileLoad);
        assert!(plan.apply_preferred_style);
        assert!(plan.apply_preferred_play_mode);
        assert!(plan.initialize_session_side);
    }

    #[test]
    fn direct_summary_navigation_clears_pending_exit() {
        let plan = navigation_route_plan(
            &Config::default(),
            Screen::Evaluation,
            Screen::EvaluationSummary,
            false,
        );
        assert_eq!(plan.pending_post_select_summary_exit, Some(false));
    }

    #[test]
    fn evaluation_summary_returns_to_wheel_until_exit_flow_is_pending() {
        assert_eq!(
            evaluation_summary_return_to(Screen::SelectMusic, false),
            Screen::SelectMusic,
        );
        assert_eq!(
            evaluation_summary_return_to(Screen::SelectCourse, false),
            Screen::SelectCourse,
        );
        assert_eq!(
            evaluation_summary_return_to(Screen::SelectMusic, true),
            Screen::Initials,
        );
    }

    fn action_context() -> ScreenActionRouteContext {
        ScreenActionRouteContext {
            current_screen: Screen::Gameplay,
            restart_pending: false,
            course_active: false,
            course_has_next_stage: false,
            gameplay_failed: false,
        }
    }

    fn profiles() -> [Profile; PLAYER_SLOTS] {
        let mut profiles = std::array::from_fn(|_| Profile::default());
        profiles[0].current_combo = 12;
        profiles[1].current_combo = 34;
        profiles[0]
            .last_played_mut(PlayStyle::Single)
            .difficulty_index = 2;
        profiles[1]
            .last_played_mut(PlayStyle::Single)
            .difficulty_index = 5;
        profiles
    }

    #[test]
    fn fast_profile_switch_refreshes_wheel_and_skips_redundant_navigation() {
        let plan = profile_selection_plan(
            &profiles(),
            ProfileSelectionContext {
                play_style: PlayStyle::Single,
                active_side: PlayerSide::P2,
                fast_switch: true,
                current_screen: Screen::SelectMusic,
                show_groovestats_login: true,
                show_arrowcloud_login: true,
            },
        );
        assert_eq!(plan.combo_carry, [12, 34]);
        assert_eq!(plan.preferred_active, 4);
        assert_eq!(plan.preferred_p2, 4);
        assert!(plan.refresh_select_music);
        assert_eq!(plan.navigation_target, None);
    }

    #[test]
    fn fast_profile_switch_returns_to_wheel_from_other_screens() {
        let plan = profile_selection_plan(
            &profiles(),
            ProfileSelectionContext {
                play_style: PlayStyle::Single,
                active_side: PlayerSide::P1,
                fast_switch: true,
                current_screen: Screen::SelectProfile,
                show_groovestats_login: false,
                show_arrowcloud_login: false,
            },
        );
        assert_eq!(plan.preferred_active, 2);
        assert_eq!(plan.navigation_target, Some(Screen::SelectMusic));
    }

    #[test]
    fn normal_profile_flow_prioritizes_login_services() {
        for (groovestats, arrowcloud, expected) in [
            (true, true, Screen::GrooveStatsLogin),
            (false, true, Screen::ArrowCloudLogin),
            (false, false, Screen::SelectColor),
        ] {
            let plan = profile_selection_plan(
                &profiles(),
                ProfileSelectionContext {
                    play_style: PlayStyle::Single,
                    active_side: PlayerSide::P1,
                    fast_switch: false,
                    current_screen: Screen::SelectProfile,
                    show_groovestats_login: groovestats,
                    show_arrowcloud_login: arrowcloud,
                },
            );
            assert!(!plan.refresh_select_music);
            assert_eq!(plan.navigation_target, Some(expected));
        }
    }

    #[test]
    fn restart_redirect_takes_priority_over_course_wheel_routing() {
        let plan = screen_action_route_plan(
            ScreenAction::NavigateNoFade(Screen::SelectMusic),
            ScreenActionRouteContext {
                restart_pending: true,
                course_active: true,
                ..action_context()
            },
        );
        assert!(matches!(
            plan.action,
            ScreenAction::NavigateNoFade(Screen::Gameplay)
        ));
        assert!(plan.clear_restart_pending);
    }

    #[test]
    fn restart_redirect_requires_gameplay_and_pending_restart() {
        let plan = screen_action_route_plan(
            ScreenAction::NavigateNoFade(Screen::SelectMusic),
            ScreenActionRouteContext {
                current_screen: Screen::Evaluation,
                restart_pending: true,
                ..action_context()
            },
        );
        assert!(matches!(
            plan.action,
            ScreenAction::NavigateNoFade(Screen::SelectMusic)
        ));
        assert!(!plan.clear_restart_pending);
    }

    #[test]
    fn passing_course_stage_chains_back_to_gameplay() {
        let plan = screen_action_route_plan(
            ScreenAction::Navigate(Screen::Evaluation),
            ScreenActionRouteContext {
                course_active: true,
                course_has_next_stage: true,
                ..action_context()
            },
        );
        assert!(matches!(
            plan.action,
            ScreenAction::Navigate(Screen::Gameplay)
        ));
    }

    #[test]
    fn failed_or_final_course_stage_enters_evaluation() {
        for context in [
            ScreenActionRouteContext {
                course_active: true,
                course_has_next_stage: true,
                gameplay_failed: true,
                ..action_context()
            },
            ScreenActionRouteContext {
                course_active: true,
                course_has_next_stage: false,
                ..action_context()
            },
        ] {
            let plan =
                screen_action_route_plan(ScreenAction::Navigate(Screen::Evaluation), context);
            assert!(matches!(
                plan.action,
                ScreenAction::Navigate(Screen::Evaluation)
            ));
        }
    }

    #[test]
    fn course_wheel_redirect_preserves_fade_mode() {
        let context = ScreenActionRouteContext {
            course_active: true,
            ..action_context()
        };
        let fade = screen_action_route_plan(ScreenAction::Navigate(Screen::SelectMusic), context);
        assert!(matches!(
            fade.action,
            ScreenAction::Navigate(Screen::SelectCourse)
        ));

        let no_fade =
            screen_action_route_plan(ScreenAction::NavigateNoFade(Screen::SelectMusic), context);
        assert!(matches!(
            no_fade.action,
            ScreenAction::NavigateNoFade(Screen::SelectCourse)
        ));
    }

    #[test]
    fn action_effect_plan_routes_navigation_and_restart_state() {
        let plan = screen_action_effect_plan(
            ScreenAction::NavigateNoFade(Screen::SelectMusic),
            ScreenActionRouteContext {
                restart_pending: true,
                ..action_context()
            },
        );

        assert!(matches!(
            plan.effect,
            ScreenActionEffect::NavigateNoFade(Screen::Gameplay)
        ));
        assert!(plan.clear_restart_pending);
    }

    #[test]
    fn action_effect_plan_maps_process_and_screenshot_effects() {
        let exit = screen_action_effect_plan(ScreenAction::Exit, action_context());
        assert!(matches!(
            exit.effect,
            ScreenActionEffect::ProcessExit(ProcessExitRequest::Exit)
        ));

        let shot = screen_action_effect_plan(
            ScreenAction::RequestScreenshot(Some(PlayerSide::P2)),
            action_context(),
        );
        assert!(matches!(
            shot.effect,
            ScreenActionEffect::RequestScreenshot(Some(PlayerSide::P2))
        ));
    }

    #[test]
    fn action_effect_plan_maps_media_requests_to_commands() {
        let plan = screen_action_effect_plan(
            ScreenAction::RequestDensityGraph {
                slot: deadsync_screens::DensityGraphSlot::SelectMusicP1,
                chart_opt: None,
            },
            action_context(),
        );

        let ScreenActionEffect::RunCommands(commands) = plan.effect else {
            panic!("expected command effect");
        };
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands.into_iter().next(),
            Some(Command::SetDensityGraph {
                slot: deadsync_screens::DensityGraphSlot::SelectMusicP1,
                chart_opt: None
            })
        ));
    }

    #[test]
    fn action_effect_plan_resolves_online_profile_targets() {
        let plan = screen_action_effect_plan(
            ScreenAction::LinkGrooveStats {
                profile_id: "profile".to_string(),
                display_name: "Player".to_string(),
            },
            action_context(),
        );

        let ScreenActionEffect::LinkOnlineProfile(link) = plan.effect else {
            panic!("expected online profile link effect");
        };
        assert_eq!(link.target, Screen::GrooveStatsLogin);
        assert_eq!(link.profile_id, "profile");
        assert_eq!(link.display_name, "Player");
    }

    fn late_join_context(screen: Screen) -> LateJoinContext {
        LateJoinContext {
            screen,
            screen_allows_join: true,
            play_style: PlayStyle::Single,
            joined: [true, false],
        }
    }

    #[test]
    fn late_join_requires_one_new_side_on_an_allowed_screen() {
        assert_eq!(
            late_join_side(
                true,
                VirtualAction::p2_start,
                late_join_context(Screen::SelectMusic),
            ),
            Some(PlayerSide::P2)
        );
        for context in [
            LateJoinContext {
                joined: [false, false],
                ..late_join_context(Screen::SelectMusic)
            },
            LateJoinContext {
                joined: [true, true],
                ..late_join_context(Screen::SelectMusic)
            },
            late_join_context(Screen::Gameplay),
        ] {
            assert_eq!(late_join_side(true, VirtualAction::p2_start, context), None);
        }
    }

    #[test]
    fn late_join_respects_screen_gates_double_and_press_state() {
        for (pressed, context) in [
            (false, late_join_context(Screen::SelectMusic)),
            (
                true,
                LateJoinContext {
                    screen_allows_join: false,
                    ..late_join_context(Screen::SelectMusic)
                },
            ),
            (
                true,
                LateJoinContext {
                    play_style: PlayStyle::Double,
                    ..late_join_context(Screen::SelectCourse)
                },
            ),
        ] {
            assert_eq!(
                late_join_side(pressed, VirtualAction::p2_start, context),
                None
            );
        }
    }

    #[test]
    fn p1_joining_a_p2_session_moves_current_wheel_choice_to_p2() {
        let plan = select_music_join_plan(SelectMusicJoinContext {
            active_side: PlayerSide::P2,
            join_side: PlayerSide::P1,
            selected_steps: 4,
            preferred_difficulty: 3,
            p1_profile_preferred: 2,
            p2_profile_preferred: 4,
        });
        assert_eq!(
            plan,
            SelectMusicJoinPlan {
                selected_steps: 2,
                preferred_difficulty: 2,
                p2_selected_steps: 4,
                p2_preferred_difficulty: 3,
            }
        );
    }

    #[test]
    fn p2_join_uses_its_profile_preference_without_moving_p1() {
        let plan = select_music_join_plan(SelectMusicJoinContext {
            active_side: PlayerSide::P1,
            join_side: PlayerSide::P2,
            selected_steps: 3,
            preferred_difficulty: 2,
            p1_profile_preferred: 2,
            p2_profile_preferred: 4,
        });
        assert_eq!(
            plan,
            SelectMusicJoinPlan {
                selected_steps: 3,
                preferred_difficulty: 2,
                p2_selected_steps: 4,
                p2_preferred_difficulty: 4,
            }
        );
    }
}
