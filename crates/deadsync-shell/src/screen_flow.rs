use deadsync_config::app_config::Config;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::session_flow::{
    self, EvaluationOrigin, SessionNavigationPlan, SessionNavigationPolicy,
};
use deadsync_profile::PlayerSide;
use deadsync_theme::ThemeSessionFlow;
use deadsync_theme::profile_picker::ProfilePickerContext;
#[cfg(test)]
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
#[cfg(test)]
use deadsync_theme_simply_love::screens::{
    LateJoinContext, ProfileSelectionContext, SelectMusicJoinContext, SelectMusicJoinPlan,
    late_join_side, profile_selection_plan, select_music_join_plan,
};
const fn navigation_policy(config: &Config) -> SessionNavigationPolicy {
    SessionNavigationPolicy {
        show_select_profile: config.machine_show_select_profile,
        show_select_color: config.machine_show_select_color,
        show_select_style: config.machine_show_select_style,
        show_select_play_mode: config.machine_show_select_play_mode,
        show_eval_summary: config.machine_show_eval_summary,
        show_name_entry: config.machine_show_name_entry,
        show_gameover: config.machine_show_gameover,
    }
}

pub(crate) fn navigation_route_plan<T: ThemeSessionFlow>(
    config: &Config,
    from: T::Screen,
    requested: T::Screen,
    has_played_stages: bool,
) -> SessionNavigationPlan<T::Screen> {
    session_flow::resolve_navigation(
        T::SESSION_SCREENS,
        navigation_policy(config),
        from,
        requested,
        has_played_stages,
    )
}

pub(crate) fn evaluation_return_to<T: ThemeSessionFlow>(origin: EvaluationOrigin) -> T::Screen {
    session_flow::evaluation_return_to(T::SESSION_SCREENS, origin)
}

pub(crate) fn evaluation_summary_return_to<T: ThemeSessionFlow>(
    previous: T::Screen,
    pending_post_select_summary_exit: bool,
) -> T::Screen {
    session_flow::evaluation_summary_return_to(
        T::SESSION_SCREENS,
        previous,
        pending_post_select_summary_exit,
    )
}

pub(crate) fn profile_picker_context<T: ThemeSessionFlow>(
    previous: T::Screen,
    color_index: i32,
    menu_start_side: PlayerSide,
    joined_players: impl FnOnce() -> [bool; MAX_PLAYERS],
) -> ProfilePickerContext {
    let fast_switch = previous == T::SESSION_SCREENS.select_music;
    let joined = if previous == T::SESSION_SCREENS.menu {
        Some([
            menu_start_side == PlayerSide::P1,
            menu_start_side == PlayerSide::P2,
        ])
    } else if fast_switch {
        Some(joined_players())
    } else {
        None
    };
    ProfilePickerContext {
        color_index,
        joined,
        fast_switch,
    }
}

pub(crate) fn profile_selection_target<T: ThemeSessionFlow>(
    current_screen: T::Screen,
    fast_switch: bool,
    show_groovestats_login: bool,
    show_arrowcloud_login: bool,
) -> Option<T::Screen> {
    session_flow::profile_selection_target(
        T::SESSION_SCREENS,
        T::PROFILE_LOGIN_SCREENS,
        current_screen,
        fast_switch,
        show_groovestats_login,
        show_arrowcloud_login,
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
    use crate::ActiveTheme;
    use crate::main_menu::MainMenuRuntimeView;
    use deadlib_platform::input::RawKeyboardEvent;
    use deadlib_present::actors::Actor;
    use deadsync_core::input::InputSource;
    use deadsync_core::session_flow::{ProfileLoginScreens, SessionScreens};
    use deadsync_input::{InputEvent, VirtualAction};
    use deadsync_profile::PlayStyle;
    use deadsync_theme::main_menu::{MainMenuFrame, MainMenuPhase, ThemeMainMenu};
    use deadsync_theme::{Theme, ThemeEffect, ThemeScreenFlow, ThemeScreenId, ThemeScreens, ThemeTransitions, TransitionKind};

    struct AlternateTheme;

    impl Theme for AlternateTheme {
        type Screen = &'static str;
        type RuntimeRequest = ();

        fn screen_id(screen: Self::Screen) -> ThemeScreenId {
            ThemeScreenId::new(screen)
        }
    }

    impl ThemeScreenFlow for AlternateTheme {
        const SCREENS: ThemeScreens<Self::Screen> = ThemeScreens {
            menu: "home",
            gameplay: "play",
            practice: "editor",
            results: "result",
            song_selection: "songs",
            course_selection: "courses",
            player_options: "settings",
        };
    }

    impl ThemeSessionFlow for AlternateTheme {
        const SESSION_SCREENS: SessionScreens<Self::Screen> = SessionScreens {
            menu: Self::SCREENS.menu,
            select_music: Self::SCREENS.song_selection,
            select_course: Self::SCREENS.course_selection,
            select_profile: None,
            select_color: None,
            select_style: None,
            select_play_mode: Some("mode"),
            profile_load: "prepare",
            evaluation_summary: Some("set-results"),
            initials: None,
            gameover: Some("finished"),
        };
        const PROFILE_LOGIN_SCREENS: ProfileLoginScreens<Self::Screen> = ProfileLoginScreens {
            groovestats: None,
            arrowcloud: Some("link-account"),
        };
    }

    impl ThemeTransitions for AlternateTheme {
        type Actor = Actor;
        type EntryContext<'a> = ();

        fn transition_kind(_: Self::Screen, _: Self::Screen) -> TransitionKind {
            TransitionKind::Direct
        }

        fn actor_entry_duration(_: Self::Screen) -> Option<f32> { None }

        fn actor_exit_duration(_: Self::Screen) -> f32 { 0.0 }

        fn skip_global_entry_fade(_: Self::Screen, _: Self::Screen) -> bool { true }

        fn out_transition(_: Self::Screen, _: Self::Screen) -> (Vec<Actor>, f32) {
            (Vec::new(), 0.0)
        }

        fn in_transition(_: Self::Screen, _: ()) -> (Vec<Actor>, f32) {
            (Vec::new(), 0.0)
        }
    }

    struct AlternateMenu {
        color_index: i32,
        credits: u32,
        active_time: f32,
    }

    impl ThemeMainMenu for AlternateTheme {
        type MenuState = AlternateMenu;
        type MenuRuntimeView = MainMenuRuntimeView;
        type MenuInput = InputEvent;
        type MenuRawKey = RawKeyboardEvent;
        type MenuVisualPolicy = ();

        fn init_menu(color_index: i32) -> AlternateMenu {
            AlternateMenu { color_index, credits: 0, active_time: 0.0 }
        }

        fn menu_color_index(state: &AlternateMenu) -> i32 {
            state.color_index
        }

        fn set_menu_color_index(state: &mut AlternateMenu, color_index: i32) {
            state.color_index = color_index;
        }

        fn sync_menu_runtime(state: &mut AlternateMenu, view: MainMenuRuntimeView) {
            state.credits = view.credits;
        }

        fn update_menu(
            state: &mut AlternateMenu, delta_time: f32, phase: MainMenuPhase,
        ) -> ThemeEffect<Self::Screen, ()> {
            if phase == MainMenuPhase::Active {
                state.active_time += delta_time;
            }
            ThemeEffect::None
        }

        fn handle_menu_input(_: &mut AlternateMenu, input: &InputEvent) -> ThemeEffect<Self::Screen, ()> {
            if input.pressed && matches!(input.action, VirtualAction::p2_start) {
                ThemeEffect::Navigate("mode")
            } else {
                ThemeEffect::None
            }
        }

        fn push_menu_actors(
            actors: &mut Vec<Actor>, state: &AlternateMenu, frame: MainMenuFrame<'_, Self::Screen, ()>,
        ) {
            actors.push(deadlib_present::__act_from_builder!(
                (xy(8.0, 8.0): setsize(12.0, 12.0): diffuse(1.0, 1.0, 1.0, frame.alpha_multiplier): z(state.credits as i16))
                deadlib_assets::present_dsl::SpriteBuilder::solid()
            ));
        }
    }

    #[test]
    fn evaluation_returns_to_the_origin_in_the_installed_theme() {
        assert_eq!(evaluation_return_to::<AlternateTheme>(EvaluationOrigin::Song), "songs");
        assert_eq!(evaluation_return_to::<AlternateTheme>(EvaluationOrigin::Course), "courses");
    }

    #[test]
    fn profile_picker_uses_game_session_and_theme_roles() {
        let from_menu = profile_picker_context::<AlternateTheme>("home", 7, PlayerSide::P2, || {
            panic!("startup uses the side that pressed Start");
        });
        assert_eq!(from_menu.joined, Some([false, true]));
        assert_eq!(from_menu.color_index, 7);
        assert!(!from_menu.fast_switch);

        let from_songs =
            profile_picker_context::<AlternateTheme>("songs", 7, PlayerSide::P1, || [true, true]);
        assert_eq!(from_songs.joined, Some([true, true]));
        assert!(from_songs.fast_switch);
        assert_eq!(
            profile_selection_target::<AlternateTheme>("songs", true, true, true),
            None,
        );
        assert_eq!(
            profile_selection_target::<AlternateTheme>("picker", true, true, true),
            Some("songs"),
        );
        assert_eq!(
            profile_selection_target::<AlternateTheme>("picker", false, true, true),
            Some("link-account"),
        );
        assert_eq!(
            profile_selection_target::<AlternateTheme>("picker", false, true, false),
            Some("mode"),
        );
    }

    #[test]
    fn alternate_title_menu_uses_game_session_flow() {
        let config = Config {
            machine_show_select_play_mode: false,
            machine_show_eval_summary: true,
            ..Config::default()
        };
        let mut menu = AlternateTheme::init_menu(0);
        AlternateTheme::set_menu_color_index(&mut menu, 7);
        AlternateTheme::sync_menu_runtime(&mut menu, MainMenuRuntimeView { credits: 3, ..Default::default() });
        AlternateTheme::update_menu(&mut menu, 0.5, MainMenuPhase::Entering);
        AlternateTheme::update_menu(&mut menu, 0.25, MainMenuPhase::Active);
        assert_eq!(menu.active_time, 0.25);
        let mut actors = Vec::new();
        AlternateTheme::push_menu_actors(&mut actors, &menu, MainMenuFrame {
            update_banner_tag: None,
            background_key: None,
            alpha_multiplier: 0.75,
            transition: &deadsync_theme::TransitionState::Idle,
            visual_policy: (),
        });
        assert!(matches!(actors.as_slice(), [Actor::Sprite { z: 3, tint, .. }] if tint[3] == 0.75));

        let mut session = crate::SessionState::new(0, [0; 2]);
        let now = std::time::Instant::now();
        let start = InputEvent::new(VirtualAction::p2_start, 0, true, InputSource::Keyboard, now, 0, now, now);
        let action = crate::main_menu::handle_input::<AlternateTheme>(&mut menu, &mut session, &start);
        let ThemeEffect::Navigate(target) = action else { panic!("menu should request a startup screen") };
        assert_eq!(session.menu_start_side, PlayerSide::P2);
        let plan = navigation_route_plan::<AlternateTheme>(&config, "home", target, false);
        assert_eq!(plan.target, "prepare");
        assert!(plan.initialize_session_side);
        assert!(plan.apply_preferred_style);
        assert!(plan.apply_preferred_play_mode);

        let plan = navigation_route_plan::<AlternateTheme>(&config, "courses", "home", true);
        assert_eq!(plan.target, "set-results");
        assert_eq!(plan.pending_post_select_summary_exit, Some(true));
        assert_eq!(evaluation_summary_return_to::<AlternateTheme>("courses", false), "courses");
        assert_eq!(evaluation_summary_return_to::<AlternateTheme>("courses", true), "finished");
        AlternateTheme::reset_menu(&mut menu);
        assert_eq!(AlternateTheme::menu_color_index(&menu), 7);
        assert_eq!(menu.active_time, 0.0);
    }

    #[test]
    fn post_select_exit_enters_configured_summary_flow() {
        let cfg = Config {
            machine_show_eval_summary: true,
            ..Config::default()
        };
        let plan = navigation_route_plan::<ActiveTheme>(&cfg, Screen::SelectMusic, Screen::Menu, true);
        assert_eq!(plan.target, Screen::EvaluationSummary);
        assert_eq!(plan.pending_post_select_summary_exit, Some(true));

        let plan = navigation_route_plan::<ActiveTheme>(&cfg, Screen::SelectMusic, Screen::Menu, false);
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
        let plan = navigation_route_plan::<ActiveTheme>(&cfg, Screen::Menu, Screen::SelectProfile, false);
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
                let selection = deadsync_profile::requests::profile_selection_session_plan(
                    deadsync_profile::play_style_from_machine_preference(preferred, game),
                    true,
                    true,
                );
                let navigation =
                    navigation_route_plan::<ActiveTheme>(&cfg, Screen::SelectProfile, Screen::SelectColor, false);
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
        let plan = navigation_route_plan::<ActiveTheme>(
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
            evaluation_summary_return_to::<ActiveTheme>(Screen::SelectMusic, false),
            Screen::SelectMusic,
        );
        assert_eq!(
            evaluation_summary_return_to::<ActiveTheme>(Screen::SelectCourse, false),
            Screen::SelectCourse,
        );
        assert_eq!(
            evaluation_summary_return_to::<ActiveTheme>(Screen::SelectMusic, true),
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
