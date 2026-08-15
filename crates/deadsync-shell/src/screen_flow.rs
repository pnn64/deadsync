use deadsync_config::app_config::Config;
#[cfg(test)]
use deadsync_profile::PlayerSide;
#[cfg(test)]
use deadsync_theme_simply_love::SimplyLoveEffect as ThemeEffect;
#[cfg(test)]
use deadsync_theme_simply_love::screens::SelectMusicJoinPlan;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
pub(crate) use deadsync_theme_simply_love::screens::{
    LateJoinContext, ProfileSelectionContext, SelectMusicJoinContext, evaluation_summary_return_to,
    late_join_side, profile_selection_plan, select_music_join_plan,
};
use deadsync_theme_simply_love::screens::{
    SimplyLoveNavigationPlan, SimplyLoveNavigationPolicy, resolve_navigation,
};
pub(crate) use deadsync_theme_simply_love::{
    SimplyLoveEffectRouteContext as ThemeEffectRouteContext,
    resolve_effect_route as theme_effect_route_plan,
};

const fn navigation_policy(config: &Config) -> SimplyLoveNavigationPolicy {
    SimplyLoveNavigationPolicy {
        show_select_profile: config.machine_show_select_profile,
        show_select_color: config.machine_show_select_color,
        show_select_style: config.machine_show_select_style,
        show_select_play_mode: config.machine_show_select_play_mode,
        show_eval_summary: config.machine_show_eval_summary,
        show_name_entry: config.machine_show_name_entry,
        show_gameover: config.machine_show_gameover,
    }
}

pub(crate) fn navigation_route_plan(
    config: &Config,
    from: Screen,
    requested: Screen,
    has_played_stages: bool,
) -> SimplyLoveNavigationPlan {
    resolve_navigation(
        navigation_policy(config),
        from,
        requested,
        has_played_stages,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_input::VirtualAction;
    use deadsync_profile::PlayStyle;

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

    fn action_context() -> ThemeEffectRouteContext {
        ThemeEffectRouteContext {
            current_screen: Screen::Gameplay,
            restart_pending: false,
            course_active: false,
            course_has_next_stage: false,
            gameplay_failed: false,
        }
    }

    #[test]
    fn fast_profile_switch_refreshes_wheel_and_skips_redundant_navigation() {
        let plan = profile_selection_plan(ProfileSelectionContext {
            preferred_difficulties: [2, 4],
            active_side: PlayerSide::P2,
            fast_switch: true,
            current_screen: Screen::SelectMusic,
            show_groovestats_login: true,
            show_arrowcloud_login: true,
        });
        assert_eq!(plan.preferred_active, 4);
        assert_eq!(plan.preferred_p2, 4);
        assert!(plan.refresh_select_music);
        assert_eq!(plan.navigation_target, None);
    }

    #[test]
    fn fast_profile_switch_returns_to_wheel_from_other_screens() {
        let plan = profile_selection_plan(ProfileSelectionContext {
            preferred_difficulties: [2, 4],
            active_side: PlayerSide::P1,
            fast_switch: true,
            current_screen: Screen::SelectProfile,
            show_groovestats_login: false,
            show_arrowcloud_login: false,
        });
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
            let plan = profile_selection_plan(ProfileSelectionContext {
                preferred_difficulties: [2, 4],
                active_side: PlayerSide::P1,
                fast_switch: false,
                current_screen: Screen::SelectProfile,
                show_groovestats_login: groovestats,
                show_arrowcloud_login: arrowcloud,
            });
            assert!(!plan.refresh_select_music);
            assert_eq!(plan.navigation_target, Some(expected));
        }
    }

    #[test]
    fn restart_redirect_takes_priority_over_course_wheel_routing() {
        let plan = theme_effect_route_plan(
            ThemeEffect::NavigateNoFade(Screen::SelectMusic),
            ThemeEffectRouteContext {
                restart_pending: true,
                course_active: true,
                ..action_context()
            },
        );
        assert!(matches!(
            plan.action,
            ThemeEffect::NavigateNoFade(Screen::Gameplay)
        ));
        assert!(plan.clear_restart_pending);
    }

    #[test]
    fn restart_redirect_requires_gameplay_and_pending_restart() {
        let plan = theme_effect_route_plan(
            ThemeEffect::NavigateNoFade(Screen::SelectMusic),
            ThemeEffectRouteContext {
                current_screen: Screen::Evaluation,
                restart_pending: true,
                ..action_context()
            },
        );
        assert!(matches!(
            plan.action,
            ThemeEffect::NavigateNoFade(Screen::SelectMusic)
        ));
        assert!(!plan.clear_restart_pending);
    }

    #[test]
    fn passing_course_stage_chains_back_to_gameplay() {
        let plan = theme_effect_route_plan(
            ThemeEffect::Navigate(Screen::Evaluation),
            ThemeEffectRouteContext {
                course_active: true,
                course_has_next_stage: true,
                ..action_context()
            },
        );
        assert!(matches!(
            plan.action,
            ThemeEffect::Navigate(Screen::Gameplay)
        ));
    }

    #[test]
    fn failed_or_final_course_stage_enters_evaluation() {
        for context in [
            ThemeEffectRouteContext {
                course_active: true,
                course_has_next_stage: true,
                gameplay_failed: true,
                ..action_context()
            },
            ThemeEffectRouteContext {
                course_active: true,
                course_has_next_stage: false,
                ..action_context()
            },
        ] {
            let plan = theme_effect_route_plan(ThemeEffect::Navigate(Screen::Evaluation), context);
            assert!(matches!(
                plan.action,
                ThemeEffect::Navigate(Screen::Evaluation)
            ));
        }
    }

    #[test]
    fn course_wheel_redirect_preserves_fade_mode() {
        let context = ThemeEffectRouteContext {
            course_active: true,
            ..action_context()
        };
        let fade = theme_effect_route_plan(ThemeEffect::Navigate(Screen::SelectMusic), context);
        assert!(matches!(
            fade.action,
            ThemeEffect::Navigate(Screen::SelectCourse)
        ));

        let no_fade =
            theme_effect_route_plan(ThemeEffect::NavigateNoFade(Screen::SelectMusic), context);
        assert!(matches!(
            no_fade.action,
            ThemeEffect::NavigateNoFade(Screen::SelectCourse)
        ));
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
