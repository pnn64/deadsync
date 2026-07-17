use crate::Command;
use crate::interaction::ProcessExitRequest;
use deadlib_platform::dirs;
use deadsync_config::app_config::Config;
use deadsync_profile::PlayerSide;
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
    SimplyLoveDebugRequest, SimplyLoveEffect as ThemeEffect, SimplyLoveMediaRequest,
    SimplyLoveOnlineRequest, SimplyLoveRuntimeRequest,
};
pub(crate) use deadsync_theme_simply_love::{
    SimplyLoveEffectRouteContext as ThemeEffectRouteContext,
    SimplyLoveEffectRoutePlan as ThemeEffectRoutePlan,
    resolve_effect_route as theme_effect_route_plan,
};
use std::path::PathBuf;

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

pub struct OnlineProfileLinkPlan {
    pub target: Screen,
    pub profile_id: String,
    pub display_name: String,
}

pub enum ThemeEffectExecution {
    None,
    Batch(Vec<ThemeEffect>),
    Navigate(Screen),
    NavigateNoFade(Screen),
    ProcessExit(ProcessExitRequest),
    RequestScreenshot(Option<PlayerSide>),
    RunCommands(Vec<Command>),
    LinkOnlineProfile(OnlineProfileLinkPlan),
    WriteFsrDump { path: PathBuf },
    Runtime(SimplyLoveRuntimeRequest),
}

pub struct ThemeEffectExecutionPlan {
    pub effect: ThemeEffectExecution,
    pub clear_restart_pending: bool,
}

pub(crate) fn execute_effect_batch<E>(
    effects: Vec<ThemeEffect>,
    mut execute: impl FnMut(ThemeEffect) -> Result<(), E>,
) -> Result<(), E> {
    for effect in effects {
        execute(effect)?;
    }
    Ok(())
}

pub fn theme_effect_execution_plan(
    action: ThemeEffect,
    context: ThemeEffectRouteContext,
) -> ThemeEffectExecutionPlan {
    let route = match action {
        action @ (ThemeEffect::Navigate(_) | ThemeEffect::NavigateNoFade(_)) => {
            theme_effect_route_plan(action, context)
        }
        action => ThemeEffectRoutePlan {
            action,
            clear_restart_pending: false,
        },
    };

    let effect = match route.action {
        ThemeEffect::None | ThemeEffect::ConsumeInput => ThemeEffectExecution::None,
        ThemeEffect::Batch(effects) => ThemeEffectExecution::Batch(effects),
        ThemeEffect::Navigate(screen) => ThemeEffectExecution::Navigate(screen),
        ThemeEffect::NavigateNoFade(screen) => ThemeEffectExecution::NavigateNoFade(screen),
        ThemeEffect::Exit => ThemeEffectExecution::ProcessExit(ProcessExitRequest::Exit),
        ThemeEffect::Shutdown => ThemeEffectExecution::ProcessExit(ProcessExitRequest::Shutdown),
        ThemeEffect::Runtime(request) => match request {
            SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::LinkArrowCloud {
                profile_id,
                display_name,
            }) => ThemeEffectExecution::LinkOnlineProfile(OnlineProfileLinkPlan {
                target: Screen::ArrowCloudLogin,
                profile_id,
                display_name,
            }),
            SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::LinkGrooveStats {
                profile_id,
                display_name,
            }) => ThemeEffectExecution::LinkOnlineProfile(OnlineProfileLinkPlan {
                target: Screen::GrooveStatsLogin,
                profile_id,
                display_name,
            }),
            SimplyLoveRuntimeRequest::Media(SimplyLoveMediaRequest::Screenshot(side)) => {
                ThemeEffectExecution::RequestScreenshot(side)
            }
            SimplyLoveRuntimeRequest::Media(SimplyLoveMediaRequest::Banner(path_opt)) => {
                ThemeEffectExecution::RunCommands(vec![Command::SetBanner(path_opt)])
            }
            SimplyLoveRuntimeRequest::Media(SimplyLoveMediaRequest::CdTitle(path_opt)) => {
                ThemeEffectExecution::RunCommands(vec![Command::SetCdTitle(path_opt)])
            }
            SimplyLoveRuntimeRequest::Media(SimplyLoveMediaRequest::PackBanner(path_opt)) => {
                ThemeEffectExecution::RunCommands(vec![Command::SetPackBanner(path_opt)])
            }
            SimplyLoveRuntimeRequest::Media(SimplyLoveMediaRequest::WheelItemBackgrounds(
                paths,
            )) => ThemeEffectExecution::RunCommands(vec![Command::SetWheelItemBackgrounds(paths)]),
            SimplyLoveRuntimeRequest::Media(SimplyLoveMediaRequest::DensityGraph {
                slot,
                chart_opt,
            }) => ThemeEffectExecution::RunCommands(vec![Command::SetDensityGraph {
                slot,
                chart_opt,
            }]),
            SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::FetchGrade(hash)) => {
                ThemeEffectExecution::RunCommands(vec![Command::FetchOnlineGrade(hash)])
            }
            SimplyLoveRuntimeRequest::Debug(SimplyLoveDebugRequest::WriteFsrDump) => {
                ThemeEffectExecution::WriteFsrDump {
                    path: dirs::app_dirs().data_dir.join("fsrdump.txt"),
                }
            }
            request => ThemeEffectExecution::Runtime(request),
        },
    };

    ThemeEffectExecutionPlan {
        effect,
        clear_restart_pending: route.clear_restart_pending,
    }
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

    #[test]
    fn action_effect_plan_routes_navigation_and_restart_state() {
        let plan = theme_effect_execution_plan(
            ThemeEffect::NavigateNoFade(Screen::SelectMusic),
            ThemeEffectRouteContext {
                restart_pending: true,
                ..action_context()
            },
        );

        assert!(matches!(
            plan.effect,
            ThemeEffectExecution::NavigateNoFade(Screen::Gameplay)
        ));
        assert!(plan.clear_restart_pending);
    }

    #[test]
    fn action_effect_plan_maps_process_and_screenshot_effects() {
        let exit = theme_effect_execution_plan(ThemeEffect::Exit, action_context());
        assert!(matches!(
            exit.effect,
            ThemeEffectExecution::ProcessExit(ProcessExitRequest::Exit)
        ));

        let shot = theme_effect_execution_plan(
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Media(
                SimplyLoveMediaRequest::Screenshot(Some(PlayerSide::P2)),
            )),
            action_context(),
        );
        assert!(matches!(
            shot.effect,
            ThemeEffectExecution::RequestScreenshot(Some(PlayerSide::P2))
        ));
    }

    #[test]
    fn audio_request_reaches_runtime_execution() {
        let plan = theme_effect_execution_plan(
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(
                deadsync_theme::AudioRequest::PlaySfx("assets/sounds/start.ogg".to_owned()),
            )),
            action_context(),
        );

        assert!(matches!(
            plan.effect,
            ThemeEffectExecution::Runtime(SimplyLoveRuntimeRequest::Audio(
                deadsync_theme::AudioRequest::PlaySfx(path)
            )) if path == "assets/sounds/start.ogg"
        ));
    }

    #[test]
    fn batch_executes_in_order_and_routes_each_nested_effect() {
        let plan = theme_effect_execution_plan(
            ThemeEffect::Batch(vec![
                ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(
                    deadsync_theme::AudioRequest::PlaySfx("assets/sounds/start.ogg".to_owned()),
                )),
                ThemeEffect::NavigateNoFade(Screen::SelectMusic),
            ]),
            ThemeEffectRouteContext {
                restart_pending: true,
                ..action_context()
            },
        );
        assert!(!plan.clear_restart_pending);
        let ThemeEffectExecution::Batch(effects) = plan.effect else {
            panic!("expected batch effect");
        };

        let mut steps = Vec::new();
        execute_effect_batch(effects, |effect| {
            let nested = theme_effect_execution_plan(
                effect,
                ThemeEffectRouteContext {
                    restart_pending: true,
                    ..action_context()
                },
            );
            match nested.effect {
                ThemeEffectExecution::Runtime(SimplyLoveRuntimeRequest::Audio(
                    deadsync_theme::AudioRequest::PlaySfx(path),
                )) => {
                    assert_eq!(path, "assets/sounds/start.ogg");
                    assert!(!nested.clear_restart_pending);
                    steps.push("audio");
                }
                ThemeEffectExecution::NavigateNoFade(Screen::Gameplay) => {
                    assert!(nested.clear_restart_pending);
                    steps.push("redirect");
                }
                _ => panic!("unexpected nested effect"),
            }
            Ok::<(), ()>(())
        })
        .expect("batch execution should succeed");

        assert_eq!(steps, ["audio", "redirect"]);
    }

    #[test]
    fn action_effect_plan_maps_media_requests_to_commands() {
        let plan = theme_effect_execution_plan(
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Media(
                SimplyLoveMediaRequest::DensityGraph {
                    slot:
                        deadsync_theme_simply_love::views::SimplyLoveDensityGraphSlot::SelectMusicP1,
                    chart_opt: None,
                },
            )),
            action_context(),
        );

        let ThemeEffectExecution::RunCommands(commands) = plan.effect else {
            panic!("expected command effect");
        };
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands.into_iter().next(),
            Some(Command::SetDensityGraph {
                slot: deadsync_theme_simply_love::views::SimplyLoveDensityGraphSlot::SelectMusicP1,
                chart_opt: None
            })
        ));
    }

    #[test]
    fn action_effect_plan_resolves_online_profile_targets() {
        let plan = theme_effect_execution_plan(
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Online(
                SimplyLoveOnlineRequest::LinkGrooveStats {
                    profile_id: "profile".to_string(),
                    display_name: "Player".to_string(),
                },
            )),
            action_context(),
        );

        let ThemeEffectExecution::LinkOnlineProfile(link) = plan.effect else {
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
