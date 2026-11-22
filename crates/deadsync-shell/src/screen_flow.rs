use deadsync_config::app_config::Config;
#[cfg(test)]
use deadsync_profile::PlayerSide;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
#[cfg(test)]
use deadsync_theme_simply_love::screens::{
    LateJoinContext, SelectMusicJoinContext, SelectMusicJoinPlan, late_join_side,
    select_music_join_plan,
};
pub(crate) use deadsync_theme_simply_love::screens::{
    ProfileSelectionContext, evaluation_summary_return_to, profile_selection_plan,
};
use deadsync_theme_simply_love::screens::{
    SimplyLoveNavigationPlan, SimplyLoveNavigationPolicy, resolve_navigation,
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

pub(crate) fn preferred_play_style_for_joined(
    config: &Config,
    joined: [bool; 2],
) -> deadsync_profile::PlayStyle {
    let preferred = deadsync_profile::play_style_from_machine_preference(
        config.machine_preferred_style,
        config.game_flag,
    );
    deadsync_profile::play_style_for_joined(preferred, joined[0], joined[1])
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
    fn skipped_style_selection_preserves_two_confirmed_players() {
        use deadsync_config::theme::{GameFlag, MachinePreferredPlayStyle};

        for (game, versus) in [
            (GameFlag::Dance, PlayStyle::Versus),
            (GameFlag::Pump, PlayStyle::PumpVersus),
        ] {
            for preferred in [
                MachinePreferredPlayStyle::Single,
                MachinePreferredPlayStyle::Double,
                MachinePreferredPlayStyle::Versus,
            ] {
                let cfg = Config {
                    game_flag: game,
                    machine_preferred_style: preferred,
                    machine_show_select_color: false,
                    machine_show_select_style: false,
                    machine_show_select_play_mode: false,
                    ..Config::default()
                };
                let selection = crate::profile_session::profile_selection_session_plan(
                    deadsync_profile::play_style_from_machine_preference(preferred, game),
                    true,
                    true,
                );
                let navigation =
                    navigation_route_plan(&cfg, Screen::SelectProfile, Screen::SelectColor, false);
                assert_eq!(navigation.target, Screen::ProfileLoad);
                assert!(navigation.apply_preferred_style);
                assert_eq!(
                    preferred_play_style_for_joined(
                        &cfg,
                        [selection.p1_joined, selection.p2_joined],
                    ),
                    versus,
                );
            }
        }
    }

    #[test]
    fn preferred_style_respects_solo_profile_selection() {
        use deadsync_config::theme::{GameFlag, MachinePreferredPlayStyle};

        for (game, single, double) in [
            (GameFlag::Dance, PlayStyle::Single, PlayStyle::Double),
            (GameFlag::Pump, PlayStyle::PumpSingle, PlayStyle::PumpDouble),
        ] {
            for joined in [[true, false], [false, true]] {
                for (preferred, expected) in [
                    (MachinePreferredPlayStyle::Single, single),
                    (MachinePreferredPlayStyle::Versus, single),
                    (MachinePreferredPlayStyle::Double, double),
                ] {
                    let cfg = Config {
                        game_flag: game,
                        machine_preferred_style: preferred,
                        ..Config::default()
                    };
                    assert_eq!(preferred_play_style_for_joined(&cfg, joined), expected);
                }
            }
        }
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
