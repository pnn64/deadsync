use crate::{ActiveTheme, Command, ThemeScreen as Screen, TransitionState};
use crate::interaction::{ExitIntent, ProcessExitPlan, ProcessExitRequest, ShellInteractionState};
use crate::runtime::ShellState;
use deadsync_gameplay::exit_route::GameplayExitDestination;
use deadsync_theme::{
    ThemeScreenFlow, ThemeTransitions, TransitionKind, TransitionState as ThemeTransitionState,
};
use std::path::PathBuf;

pub struct TransitionMusicPaths {
    pub menu: PathBuf,
    pub course: PathBuf,
    pub credits: PathBuf,
    pub gameover: PathBuf,
}

pub struct TransitionAudioPlan {
    pub commands: Vec<Command>,
    pub stop_screen_sfx: bool,
    pub clear_play_background: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenChangePlan {
    pub leave_lobby: bool,
    pub exit_gameplay: bool,
    pub clear_text_layout_cache: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationTransitionStart<S> {
    DirectEntry { target: S },
    Busy,
    ActorFade { from: S, target: S },
    GlobalFade { target: S, stop_screen_sfx: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub enum FadeCompletionExitPlan {
    Continue,
    Exit,
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationTransitionEffectPlan<S> {
    pub start: NavigationTransitionStart<S>,
    pub clear_exit_intent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessExitNavigationLog {
    BeginExitFade,
    BeginShutdownFade,
    ExecuteExit,
    ExecuteShutdown,
}

pub enum ProcessExitNavigationEffect {
    BeginFade { target: Screen },
    Execute(Command),
}

pub struct ProcessExitNavigationPlan {
    pub log: ProcessExitNavigationLog,
    pub effect: ProcessExitNavigationEffect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FadeCompletionEffect {
    Continue,
    Exit,
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FadeCompletionPlan {
    pub effect: FadeCompletionEffect,
    pub log: Option<&'static str>,
}

impl ProcessExitNavigationLog {
    pub const fn message(self) -> &'static str {
        match self {
            Self::BeginExitFade => {
                "Exit requested from Menu; playing menu out-transition before shutdown."
            }
            Self::BeginShutdownFade => {
                "Host-shutdown requested from Menu; playing out-transition first."
            }
            Self::ExecuteExit => "Exit action received. Shutting down.",
            Self::ExecuteShutdown => "Host-shutdown action received. Powering off.",
        }
    }
}

/// The simulation advances under the normal entry cover and the exit to results.
pub fn tick_gameplay_during_transition<T: ThemeScreenFlow>(
    screen: T::Screen,
    transition: &ThemeTransitionState<T::Screen>,
) -> bool {
    screen == T::SCREENS.gameplay
        && match transition {
            ThemeTransitionState::FadingIn { .. } => true,
            ThemeTransitionState::FadingOut { target, .. } => *target == T::SCREENS.results,
            _ => false,
        }
}

pub fn screen_change_plan<T: ThemeScreenFlow>(
    previous: T::Screen,
    target: T::Screen,
) -> ScreenChangePlan {
    ScreenChangePlan {
        leave_lobby: previous != target && target == T::SCREENS.menu,
        exit_gameplay: previous == T::SCREENS.gameplay && target != T::SCREENS.gameplay,
        clear_text_layout_cache: previous != target,
    }
}

/// Keep Practice's temporary rate across editor options and chart reloads,
/// then restore the preferred rate when leaving that flow.
pub fn practice_music_rate_on_screen_change<T: ThemeScreenFlow>(
    saved_rate: &mut Option<f32>,
    target: T::Screen,
    current_rate: f32,
) -> Option<f32> {
    if target == T::SCREENS.practice {
        saved_rate.get_or_insert(current_rate);
        None
    } else if target == T::SCREENS.player_options {
        None
    } else {
        saved_rate.take()
    }
}

#[inline(always)]
pub fn course_plan_required<T: ThemeScreenFlow>(previous: T::Screen, target: T::Screen) -> bool {
    previous == T::SCREENS.course_selection
        && (target == T::SCREENS.gameplay || target == T::SCREENS.player_options)
}

pub fn gameplay_exit_screen<T: ThemeScreenFlow>(destination: GameplayExitDestination) -> T::Screen {
    match destination {
        GameplayExitDestination::Results => T::SCREENS.results,
        GameplayExitDestination::SongSelection => T::SCREENS.song_selection,
        GameplayExitDestination::CourseSelection => T::SCREENS.course_selection,
        GameplayExitDestination::NextCourseStage | GameplayExitDestination::RestartStage => {
            T::SCREENS.gameplay
        }
    }
}

pub fn stage_exit_duration<T: ThemeScreenFlow>(screen: T::Screen) -> Option<f32> {
    is_play_screen::<T>(screen)
        .then_some(deadsync_song_lua::gameplay_session::GAMEPLAY_EXIT_TRANSITION_SECONDS)
}

pub fn stage_entry_duration<T: ThemeScreenFlow>(
    screen: T::Screen,
    restarting: bool,
) -> Option<f32> {
    if screen == T::SCREENS.gameplay && restarting {
        Some(deadsync_gameplay::GAMEPLAY_RESTART_TRANSITION_SECONDS)
    } else {
        is_play_screen::<T>(screen).then_some(deadsync_gameplay::GAMEPLAY_ENTRY_TRANSITION_SECONDS)
    }
}

/// Build audio commands for a completed actor-only screen transition.
pub fn actor_transition_music_commands(
    previous: Screen,
    target: Screen,
    menu_music_enabled: bool,
    gameover_music_enabled: bool,
    menu_path: PathBuf,
    gameover_path: PathBuf,
) -> Vec<Command> {
    let target_menu_music =
        menu_music_enabled && matches!(target, Screen::SelectColor | Screen::SelectStyle);
    let previous_menu_music =
        menu_music_enabled && matches!(previous, Screen::SelectColor | Screen::SelectStyle);
    let target_gameover_music = target == Screen::GameOver && gameover_music_enabled;
    let previous_gameover_music = previous == Screen::GameOver && gameover_music_enabled;
    let keep_preview = matches!(
        (previous, target),
        (Screen::SelectMusic, Screen::PlayerOptions) | (Screen::PlayerOptions, Screen::SelectMusic)
    );

    if target_menu_music {
        if previous_menu_music {
            Vec::new()
        } else {
            vec![Command::PlayMusic {
                path: menu_path,
                looped: true,
                volume: 1.0,
            }]
        }
    } else if target_gameover_music {
        if previous_gameover_music {
            Vec::new()
        } else {
            vec![Command::PlayMusic {
                path: gameover_path,
                looped: false,
                volume: 1.0,
            }]
        }
    } else if previous_menu_music || !keep_preview {
        vec![Command::StopMusic]
    } else {
        Vec::new()
    }
}

pub fn transition_audio_plan(
    previous: Screen,
    target: Screen,
    menu_music_enabled: bool,
    gameover_music_enabled: bool,
    paths: TransitionMusicPaths,
) -> TransitionAudioPlan {
    let mut commands = Vec::new();
    let target_menu_music = menu_music_enabled
        && matches!(
            target,
            Screen::SelectColor | Screen::SelectStyle | Screen::SelectPlayMode
        );
    let previous_menu_music = menu_music_enabled
        && matches!(
            previous,
            Screen::SelectColor | Screen::SelectStyle | Screen::SelectPlayMode
        );
    let target_course_music = target == Screen::SelectCourse;
    let previous_course_music = previous == Screen::SelectCourse;
    let target_credits_music = target == Screen::Credits;
    let previous_credits_music = previous == Screen::Credits;
    let target_gameover_music = target == Screen::GameOver && gameover_music_enabled;
    let previous_gameover_music = previous == Screen::GameOver && gameover_music_enabled;
    let keep_preview = matches!(
        (previous, target),
        (Screen::SelectMusic, Screen::PlayerOptions) | (Screen::PlayerOptions, Screen::SelectMusic)
    );

    if target_menu_music {
        if !previous_menu_music {
            commands.push(Command::PlayMusic {
                path: paths.menu,
                looped: true,
                volume: 1.0,
            });
        }
    } else if target_course_music {
        if !previous_course_music {
            commands.push(Command::PlayMusic {
                path: paths.course,
                looped: true,
                volume: 1.0,
            });
        }
    } else if target_credits_music {
        if !previous_credits_music {
            commands.push(Command::PlayMusic {
                path: paths.credits,
                looped: true,
                volume: 1.0,
            });
        }
    } else if target_gameover_music {
        if !previous_gameover_music {
            commands.push(Command::PlayMusic {
                path: paths.gameover,
                looped: false,
                volume: 1.0,
            });
        }
    } else if (previous_menu_music || previous_course_music || previous_credits_music)
        && target != Screen::Gameplay
    {
        commands.push(Command::StopMusic);
    } else if target != Screen::Gameplay && !keep_preview {
        commands.push(Command::StopMusic);
    }

    let clear_play_background = matches!(previous, Screen::Gameplay | Screen::Practice)
        && !matches!(target, Screen::Gameplay | Screen::Practice);
    if clear_play_background
        && !target_menu_music
        && !target_course_music
        && !target_credits_music
        && !target_gameover_music
    {
        commands.push(Command::StopMusic);
    }

    TransitionAudioPlan {
        commands,
        stop_screen_sfx: previous == Screen::Evaluation && target != Screen::Evaluation,
        clear_play_background,
    }
}

pub fn actor_fade_out_transition<T: ThemeTransitions>(
    from: T::Screen,
    target: T::Screen,
) -> ThemeTransitionState<T::Screen> {
    let duration = T::actor_exit_duration(from);
    ThemeTransitionState::ActorsFadeOut {
        elapsed: 0.0,
        duration,
        target,
    }
}

pub const fn global_fade_out_transition<S>(target: S, duration: f32) -> ThemeTransitionState<S> {
    ThemeTransitionState::FadingOut {
        elapsed: 0.0,
        duration,
        target,
    }
}

pub fn actor_entry_transition<T: ThemeTransitions>(
    target: T::Screen,
) -> ThemeTransitionState<T::Screen> {
    match T::actor_entry_duration(target) {
        Some(duration) => ThemeTransitionState::ActorsFadeIn {
            elapsed: 0.0,
            duration,
        },
        None => ThemeTransitionState::Idle,
    }
}

pub fn global_entry_transition<T: ThemeScreenFlow + ThemeTransitions>(
    previous: T::Screen,
    target: T::Screen,
    duration: f32,
) -> ThemeTransitionState<T::Screen> {
    if !is_play_screen::<T>(target) && T::skip_global_entry_fade(previous, target) {
        ThemeTransitionState::Idle
    } else {
        ThemeTransitionState::FadingIn {
            elapsed: 0.0,
            duration,
        }
    }
}

pub fn apply_actor_entry_transition(shell: &mut ShellState, target: Screen) {
    shell.transition = actor_entry_transition::<ActiveTheme>(target);
}

pub fn apply_actor_fade_out_transition(shell: &mut ShellState, from: Screen, target: Screen) {
    shell.transition = actor_fade_out_transition::<ActiveTheme>(from, target);
}

pub const fn apply_global_fade_out_transition(
    shell: &mut ShellState,
    target: Screen,
    duration: f32,
) {
    shell.transition = global_fade_out_transition(target, duration);
}

pub fn apply_global_entry_transition(
    shell: &mut ShellState,
    previous: Screen,
    target: Screen,
    duration: f32,
) {
    shell.transition = global_entry_transition::<ActiveTheme>(previous, target, duration);
}

pub fn menu_exit_uses_fade<T: ThemeScreenFlow>(
    screen: T::Screen,
    transition: &ThemeTransitionState<T::Screen>,
) -> bool {
    screen == T::SCREENS.menu && transition.is_idle_or_arriving()
}

pub fn navigation_transition_start<T: ThemeScreenFlow + ThemeTransitions>(
    from: T::Screen,
    target: T::Screen,
    transition: &ThemeTransitionState<T::Screen>,
) -> NavigationTransitionStart<T::Screen> {
    // Gameplay and Practice need the shell's stage entry and exit lifecycle.
    let kind = if is_play_screen::<T>(from) || is_play_screen::<T>(target) {
        TransitionKind::Global
    } else {
        T::transition_kind(from, target)
    };
    match kind {
        TransitionKind::Direct => NavigationTransitionStart::DirectEntry { target },
        _ if !matches!(transition, ThemeTransitionState::Idle)
            && !(from == T::SCREENS.menu && transition.is_idle_or_arriving()) =>
        {
            NavigationTransitionStart::Busy
        }
        TransitionKind::Actors => NavigationTransitionStart::ActorFade { from, target },
        TransitionKind::Global => NavigationTransitionStart::GlobalFade {
            target,
            stop_screen_sfx: from == T::SCREENS.results && target != T::SCREENS.results,
        },
    }
}

pub fn navigation_transition_effect_plan<T: ThemeScreenFlow + ThemeTransitions>(
    from: T::Screen,
    target: T::Screen,
    transition: &ThemeTransitionState<T::Screen>,
) -> NavigationTransitionEffectPlan<T::Screen> {
    let start = navigation_transition_start::<T>(from, target, transition);
    NavigationTransitionEffectPlan {
        clear_exit_intent: matches!(
            start,
            NavigationTransitionStart::ActorFade { .. }
                | NavigationTransitionStart::GlobalFade { .. }
        ),
        start,
    }
}

pub fn process_exit_navigation_plan(
    interaction: &mut ShellInteractionState,
    request: ProcessExitRequest,
    current: Screen,
    transition: &TransitionState,
) -> ProcessExitNavigationPlan {
    match interaction.plan_process_exit(request, current, transition) {
        ProcessExitPlan::BeginFade => ProcessExitNavigationPlan {
            log: match request {
                ProcessExitRequest::Exit => ProcessExitNavigationLog::BeginExitFade,
                ProcessExitRequest::Shutdown => ProcessExitNavigationLog::BeginShutdownFade,
            },
            effect: ProcessExitNavigationEffect::BeginFade { target: current },
        },
        ProcessExitPlan::Execute(command) => ProcessExitNavigationPlan {
            log: match request {
                ProcessExitRequest::Exit => ProcessExitNavigationLog::ExecuteExit,
                ProcessExitRequest::Shutdown => ProcessExitNavigationLog::ExecuteShutdown,
            },
            effect: ProcessExitNavigationEffect::Execute(command),
        },
    }
}

#[cfg(test)]
pub const fn fade_completion_exit_plan(intent: crate::ExitIntent) -> FadeCompletionExitPlan {
    match intent {
        crate::ExitIntent::None => FadeCompletionExitPlan::Continue,
        crate::ExitIntent::Exit => FadeCompletionExitPlan::Exit,
        crate::ExitIntent::Shutdown => FadeCompletionExitPlan::Shutdown,
    }
}

pub const fn fade_completion_plan(intent: ExitIntent) -> FadeCompletionPlan {
    match intent {
        ExitIntent::None => FadeCompletionPlan {
            effect: FadeCompletionEffect::Continue,
            log: None,
        },
        ExitIntent::Exit => FadeCompletionPlan {
            effect: FadeCompletionEffect::Exit,
            log: Some("Fade-out complete; exiting application."),
        },
        ExitIntent::Shutdown => FadeCompletionPlan {
            effect: FadeCompletionEffect::Shutdown,
            log: Some("Fade-out complete; powering off host and exiting."),
        },
    }
}

#[inline]
pub fn is_play_screen<T: ThemeScreenFlow>(screen: T::Screen) -> bool {
    screen == T::SCREENS.gameplay || screen == T::SCREENS.practice
}

pub fn write_current_screen_file(name: &str, path: &std::path::Path) {
    if !deadsync_config::runtime::get().write_current_screen {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        log::warn!("Failed to create current_screen.txt parent dir: {e}");
        return;
    }
    if let Err(e) = std::fs::write(path, name) {
        log::warn!("Failed to write current_screen.txt: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_theme::{
        Theme, ThemeScreenId, ThemeScreens, TransitionCompletion, TransitionFrame,
    };

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
            menu: "title",
            gameplay: "dance",
            practice: "rehearsal",
            results: "report",
            song_selection: "library",
            course_selection: "marathons",
            player_options: "setup",
        };
    }

    impl ThemeTransitions for AlternateTheme {
        type Actor = ();
        type EntryContext<'a> = ();

        fn transition_kind(_: Self::Screen, _: Self::Screen) -> TransitionKind {
            TransitionKind::Direct
        }

        fn actor_entry_duration(_: Self::Screen) -> Option<f32> {
            None
        }

        fn actor_exit_duration(_: Self::Screen) -> f32 {
            0.0
        }

        fn skip_global_entry_fade(_: Self::Screen, _: Self::Screen) -> bool {
            true
        }

        fn out_transition(_: Self::Screen, _: Self::Screen) -> (Vec<()>, f32) {
            (Vec::new(), 0.0)
        }

        fn in_transition(_: Self::Screen, _: ()) -> (Vec<()>, f32) {
            (Vec::new(), 0.0)
        }
    }

    #[test]
    fn alternate_theme_uses_the_stage_lifecycle_with_its_own_screen_ids() {
        let idle = ThemeTransitionState::Idle;
        assert_eq!(
            navigation_transition_start::<AlternateTheme>("title", "library", &idle),
            NavigationTransitionStart::DirectEntry { target: "library" }
        );
        for (from, to) in [
            ("library", "dance"),
            ("library", "rehearsal"),
            ("dance", "report"),
        ] {
            assert_eq!(
                navigation_transition_start::<AlternateTheme>(from, to, &idle),
                NavigationTransitionStart::GlobalFade {
                    target: to,
                    stop_screen_sfx: false
                }
            );
        }
        let entry = stage_entry_duration::<AlternateTheme>("dance", true).unwrap();
        assert_eq!(
            entry,
            deadsync_gameplay::GAMEPLAY_RESTART_TRANSITION_SECONDS
        );
        assert_eq!(
            stage_entry_duration::<AlternateTheme>("rehearsal", true),
            Some(deadsync_gameplay::GAMEPLAY_ENTRY_TRANSITION_SECONDS)
        );
        let fade_in = global_entry_transition::<AlternateTheme>("library", "dance", entry);
        assert!(tick_gameplay_during_transition::<AlternateTheme>(
            "dance", &fade_in
        ));
        let duration = stage_exit_duration::<AlternateTheme>("dance").unwrap();
        let fade_out = global_fade_out_transition("report", duration);
        assert!(tick_gameplay_during_transition::<AlternateTheme>(
            "dance", &fade_out
        ));
        assert!(!tick_gameplay_during_transition::<AlternateTheme>(
            "rehearsal",
            &fade_out
        ));
        assert!(!tick_gameplay_during_transition::<AlternateTheme>(
            "dance",
            &global_fade_out_transition("library", duration),
        ));
        assert!(screen_change_plan::<AlternateTheme>("dance", "report").exit_gameplay);
        assert!(!screen_change_plan::<AlternateTheme>("dance", "dance").exit_gameplay);
    }

    #[test]
    fn alternate_theme_uses_standard_destinations_for_course_and_practice() {
        for (destination, expected) in [
            (GameplayExitDestination::Results, "report"),
            (GameplayExitDestination::SongSelection, "library"),
            (GameplayExitDestination::CourseSelection, "marathons"),
            (GameplayExitDestination::NextCourseStage, "dance"),
            (GameplayExitDestination::RestartStage, "dance"),
        ] {
            assert_eq!(
                gameplay_exit_screen::<AlternateTheme>(destination),
                expected
            );
        }
        assert!(course_plan_required::<AlternateTheme>("marathons", "setup"));
        assert!(course_plan_required::<AlternateTheme>("marathons", "dance"));
        assert!(!course_plan_required::<AlternateTheme>("library", "dance"));

        let mut saved_rate = None;
        assert_eq!(
            practice_music_rate_on_screen_change::<AlternateTheme>(
                &mut saved_rate,
                "rehearsal",
                1.2
            ),
            None
        );
        assert_eq!(
            practice_music_rate_on_screen_change::<AlternateTheme>(&mut saved_rate, "setup", 0.8),
            None
        );
        assert_eq!(
            practice_music_rate_on_screen_change::<AlternateTheme>(&mut saved_rate, "library", 0.8),
            Some(1.2)
        );
    }

    #[test]
    fn actor_fade_policy_uses_screen_contract() {
        assert_eq!(ActiveTheme::actor_entry_duration(Screen::Menu), Some(0.65));
        assert_eq!(ActiveTheme::transition_kind(Screen::Menu, Screen::Options), TransitionKind::Actors);
        assert_eq!(
            ActiveTheme::transition_kind(Screen::Gameplay, Screen::Evaluation),
            TransitionKind::Global,
        );
    }

    #[test]
    fn actor_transition_music_preserves_preview_and_avoids_restarts() {
        assert!(
            actor_transition_music_commands(
                Screen::SelectMusic,
                Screen::PlayerOptions,
                true,
                false,
                "menu.ogg".into(),
                "gameover.ogg".into(),
            )
            .is_empty()
        );

        assert!(matches!(
            actor_transition_music_commands(
                Screen::Menu,
                Screen::SelectColor,
                true,
                false,
                "menu.ogg".into(),
                "gameover.ogg".into(),
            )
            .as_slice(),
            [Command::PlayMusic { looped: true, .. }]
        ));

        assert!(
            actor_transition_music_commands(
                Screen::SelectColor,
                Screen::SelectStyle,
                true,
                false,
                "menu.ogg".into(),
                "gameover.ogg".into(),
            )
            .is_empty()
        );

        assert!(matches!(
            actor_transition_music_commands(
                Screen::Menu,
                Screen::GameOver,
                false,
                true,
                "menu.ogg".into(),
                "gameover.ogg".into(),
            )
            .as_slice(),
            [Command::PlayMusic { looped: false, .. }]
        ));

        assert!(matches!(
            actor_transition_music_commands(
                Screen::Options,
                Screen::Menu,
                true,
                false,
                "menu.ogg".into(),
                "gameover.ogg".into(),
            )
            .as_slice(),
            [Command::StopMusic]
        ));
    }

    fn music_paths() -> TransitionMusicPaths {
        TransitionMusicPaths {
            menu: "menu.ogg".into(),
            course: "course.ogg".into(),
            credits: "credits.ogg".into(),
            gameover: "gameover.ogg".into(),
        }
    }

    #[test]
    fn transition_audio_plan_preserves_music_and_cleanup_policy() {
        let menu = transition_audio_plan(
            Screen::Menu,
            Screen::SelectColor,
            true,
            false,
            music_paths(),
        );
        assert!(matches!(
            menu.commands.as_slice(),
            [Command::PlayMusic { looped: true, .. }]
        ));

        let preview = transition_audio_plan(
            Screen::SelectMusic,
            Screen::PlayerOptions,
            true,
            false,
            music_paths(),
        );
        assert!(preview.commands.is_empty());

        let leaving_play = transition_audio_plan(
            Screen::Gameplay,
            Screen::Evaluation,
            false,
            false,
            music_paths(),
        );
        assert!(leaving_play.clear_play_background);
        assert!(matches!(
            leaving_play.commands.as_slice(),
            [Command::StopMusic, Command::StopMusic]
        ));

        let evaluation = transition_audio_plan(
            Screen::Evaluation,
            Screen::Menu,
            false,
            false,
            music_paths(),
        );
        assert!(evaluation.stop_screen_sfx);
    }

    #[test]
    fn transition_constructors_keep_theme_fade_policy() {
        assert!(matches!(
            actor_fade_out_transition::<ActiveTheme>(Screen::Menu, Screen::Options),
            TransitionState::ActorsFadeOut { duration: 1.0, .. }
        ));
        assert!(matches!(
            actor_fade_out_transition::<ActiveTheme>(Screen::Menu, Screen::ProfileLoad),
            TransitionState::ActorsFadeOut { duration: 1.0, .. }
        ));
        assert!(matches!(
            actor_entry_transition::<ActiveTheme>(Screen::Menu),
            TransitionState::ActorsFadeIn { .. }
        ));
        assert!(matches!(
            global_entry_transition::<ActiveTheme>(Screen::Options, Screen::Credits, 0.5),
            TransitionState::Idle
        ));
        assert!(menu_exit_uses_fade::<ActiveTheme>(
            Screen::Menu,
            &TransitionState::Idle
        ));
        assert!(menu_exit_uses_fade::<ActiveTheme>(
            Screen::Menu,
            &TransitionState::FadingIn {
                elapsed: 0.0,
                duration: 0.4,
            }
        ));
        assert!(!menu_exit_uses_fade::<ActiveTheme>(
            Screen::Menu,
            &TransitionState::ActorsFadeOut {
                elapsed: 0.0,
                duration: 1.0,
                target: Screen::Options,
            }
        ));
        assert!(!menu_exit_uses_fade::<ActiveTheme>(
            Screen::Gameplay,
            &TransitionState::Idle
        ));
    }

    #[test]
    fn shell_transition_mutators_apply_existing_transition_policy() {
        let mut cfg = deadsync_config::app_config::Config::default();
        cfg.display_width = 1280;
        cfg.display_height = 720;
        let mut shell = ShellState::new(&cfg, 0);

        apply_actor_entry_transition(&mut shell, Screen::Menu);
        assert!(matches!(
            shell.transition,
            TransitionState::ActorsFadeIn { .. }
        ));

        apply_actor_fade_out_transition(&mut shell, Screen::Menu, Screen::Options);
        assert!(matches!(
            shell.transition,
            TransitionState::ActorsFadeOut {
                target: Screen::Options,
                duration: 1.0,
                ..
            }
        ));

        apply_global_fade_out_transition(&mut shell, Screen::Evaluation, 0.4);
        assert!(matches!(
            shell.transition,
            TransitionState::FadingOut {
                target: Screen::Evaluation,
                duration: 0.4,
                ..
            }
        ));

        apply_global_entry_transition(&mut shell, Screen::Gameplay, Screen::Evaluation, 0.5);
        assert!(matches!(
            shell.transition,
            TransitionState::FadingIn { duration: 0.5, .. }
        ));
    }

    #[test]
    fn screen_change_plan_captures_root_side_effects() {
        assert_eq!(
            screen_change_plan::<ActiveTheme>(Screen::Gameplay, Screen::Menu),
            ScreenChangePlan {
                leave_lobby: true,
                exit_gameplay: true,
                clear_text_layout_cache: true,
            }
        );
        assert_eq!(
            screen_change_plan::<ActiveTheme>(Screen::Menu, Screen::Menu),
            ScreenChangePlan {
                leave_lobby: false,
                exit_gameplay: false,
                clear_text_layout_cache: false,
            }
        );
        assert_eq!(
            screen_change_plan::<ActiveTheme>(Screen::Evaluation, Screen::Menu),
            ScreenChangePlan {
                leave_lobby: true,
                exit_gameplay: false,
                clear_text_layout_cache: true,
            }
        );
    }

    #[test]
    fn practice_exit_restores_the_preferred_music_rate() {
        for preferred_rate in [1.0, 1.25] {
            let mut saved_rate = None;
            assert_eq!(
                practice_music_rate_on_screen_change::<ActiveTheme>(
                    &mut saved_rate,
                    Screen::Practice,
                    preferred_rate,
                ),
                None
            );
            assert_eq!(
                practice_music_rate_on_screen_change::<ActiveTheme>(
                    &mut saved_rate,
                    Screen::SelectMusic,
                    0.75
                ),
                Some(preferred_rate)
            );
            assert_eq!(saved_rate, None);
        }
    }

    #[test]
    fn practice_options_and_reload_keep_the_original_music_rate() {
        let mut saved_rate = None;
        practice_music_rate_on_screen_change::<ActiveTheme>(&mut saved_rate, Screen::Practice, 1.0);
        for target in [Screen::PlayerOptions, Screen::Practice, Screen::Practice] {
            assert_eq!(
                practice_music_rate_on_screen_change::<ActiveTheme>(&mut saved_rate, target, 0.75),
                None
            );
            assert_eq!(saved_rate, Some(1.0));
        }
        assert_eq!(
            practice_music_rate_on_screen_change::<ActiveTheme>(
                &mut saved_rate,
                Screen::SelectMusic,
                0.75
            ),
            Some(1.0)
        );
    }

    #[test]
    fn leaving_practice_options_restores_rate_and_next_practice_captures_it_again() {
        let mut saved_rate = None;
        practice_music_rate_on_screen_change::<ActiveTheme>(&mut saved_rate, Screen::Practice, 1.0);
        practice_music_rate_on_screen_change::<ActiveTheme>(
            &mut saved_rate,
            Screen::PlayerOptions,
            0.75,
        );
        assert_eq!(
            practice_music_rate_on_screen_change::<ActiveTheme>(
                &mut saved_rate,
                Screen::SelectMusic,
                0.5
            ),
            Some(1.0)
        );
        practice_music_rate_on_screen_change::<ActiveTheme>(
            &mut saved_rate,
            Screen::Practice,
            1.25,
        );
        assert_eq!(
            practice_music_rate_on_screen_change::<ActiveTheme>(
                &mut saved_rate,
                Screen::SelectMusic,
                0.75
            ),
            Some(1.25)
        );
    }

    #[test]
    fn ordinary_player_options_and_gameplay_leave_music_rate_unchanged() {
        let mut saved_rate = None;
        for target in [
            Screen::PlayerOptions,
            Screen::Gameplay,
            Screen::Evaluation,
            Screen::SelectMusic,
        ] {
            assert_eq!(
                practice_music_rate_on_screen_change::<ActiveTheme>(&mut saved_rate, target, 1.25),
                None
            );
        }
    }

    #[test]
    fn selected_course_routes_require_a_playable_course_plan() {
        assert!(course_plan_required::<ActiveTheme>(
            Screen::SelectCourse,
            Screen::Gameplay
        ));
        assert!(course_plan_required::<ActiveTheme>(
            Screen::SelectCourse,
            Screen::PlayerOptions
        ));
        assert!(!course_plan_required::<ActiveTheme>(
            Screen::SelectCourse,
            Screen::Menu
        ));
        assert!(!course_plan_required::<ActiveTheme>(
            Screen::SelectMusic,
            Screen::Gameplay
        ));
    }

    #[test]
    fn navigation_transition_start_preserves_entry_and_fade_policy() {
        assert_eq!(
            navigation_transition_start::<ActiveTheme>(
                Screen::Init,
                Screen::Menu,
                &TransitionState::Idle
            ),
            NavigationTransitionStart::DirectEntry {
                target: Screen::Menu
            }
        );
        assert_eq!(
            navigation_transition_start::<ActiveTheme>(
                Screen::Menu,
                Screen::Options,
                &TransitionState::FadingIn {
                    elapsed: 0.0,
                    duration: 0.4,
                },
            ),
            NavigationTransitionStart::ActorFade {
                from: Screen::Menu,
                target: Screen::Options,
            }
        );
        assert_eq!(
            navigation_transition_start::<ActiveTheme>(
                Screen::Menu,
                Screen::Options,
                &TransitionState::ActorsFadeIn { elapsed: 0.0, duration: 0.65 },
            ),
            NavigationTransitionStart::ActorFade {
                from: Screen::Menu,
                target: Screen::Options,
            }
        );
        assert_eq!(
            navigation_transition_start::<ActiveTheme>(
                Screen::SelectMusic,
                Screen::Menu,
                &TransitionState::FadingIn {
                    elapsed: 0.0,
                    duration: 0.4,
                },
            ),
            NavigationTransitionStart::Busy
        );
        assert_eq!(
            navigation_transition_start::<ActiveTheme>(
                Screen::Menu,
                Screen::Options,
                &TransitionState::Idle
            ),
            NavigationTransitionStart::ActorFade {
                from: Screen::Menu,
                target: Screen::Options,
            }
        );
        assert_eq!(
            navigation_transition_start::<ActiveTheme>(
                Screen::Menu,
                Screen::Init,
                &TransitionState::Idle
            ),
            NavigationTransitionStart::GlobalFade {
                target: Screen::Init,
                stop_screen_sfx: false,
            }
        );
        assert_eq!(
            navigation_transition_start::<ActiveTheme>(
                Screen::Evaluation,
                Screen::Menu,
                &TransitionState::Idle,
            ),
            NavigationTransitionStart::GlobalFade {
                target: Screen::Menu,
                stop_screen_sfx: true,
            }
        );
    }

    #[test]
    fn navigation_transition_effect_marks_exit_intent_clear_points() {
        let direct = navigation_transition_effect_plan::<ActiveTheme>(
            Screen::Init,
            Screen::Menu,
            &TransitionState::Idle,
        );
        assert_eq!(
            direct,
            NavigationTransitionEffectPlan {
                start: NavigationTransitionStart::DirectEntry {
                    target: Screen::Menu
                },
                clear_exit_intent: false,
            }
        );

        let actor = navigation_transition_effect_plan::<ActiveTheme>(
            Screen::Menu,
            Screen::ProfileLoad,
            &TransitionState::Idle,
        );
        assert_eq!(
            actor,
            NavigationTransitionEffectPlan {
                start: NavigationTransitionStart::ActorFade {
                    from: Screen::Menu,
                    target: Screen::ProfileLoad,
                },
                clear_exit_intent: true,
            }
        );

        let arriving_menu = navigation_transition_effect_plan::<ActiveTheme>(
            Screen::Menu,
            Screen::Options,
            &TransitionState::FadingIn {
                elapsed: 0.0,
                duration: 0.4,
            },
        );
        assert_eq!(
            arriving_menu,
            NavigationTransitionEffectPlan {
                start: NavigationTransitionStart::ActorFade {
                    from: Screen::Menu,
                    target: Screen::Options,
                },
                clear_exit_intent: true,
            }
        );

        let busy = navigation_transition_effect_plan::<ActiveTheme>(
            Screen::SelectMusic,
            Screen::Menu,
            &TransitionState::FadingIn {
                elapsed: 0.0,
                duration: 0.4,
            },
        );
        assert_eq!(
            busy,
            NavigationTransitionEffectPlan {
                start: NavigationTransitionStart::Busy,
                clear_exit_intent: false,
            }
        );
    }

    #[test]
    fn process_exit_navigation_plan_supplies_logs_and_effects() {
        let mut interaction = ShellInteractionState::new(false);

        let fade = process_exit_navigation_plan(
            &mut interaction,
            ProcessExitRequest::Exit,
            Screen::Menu,
            &TransitionState::Idle,
        );
        assert_eq!(fade.log, ProcessExitNavigationLog::BeginExitFade);
        assert_eq!(
            fade.log.message(),
            "Exit requested from Menu; playing menu out-transition before shutdown."
        );
        assert!(matches!(
            fade.effect,
            ProcessExitNavigationEffect::BeginFade {
                target: Screen::Menu
            }
        ));
        assert_eq!(interaction.exit_intent(), ExitIntent::Exit);

        let command = process_exit_navigation_plan(
            &mut interaction,
            ProcessExitRequest::Shutdown,
            Screen::Gameplay,
            &TransitionState::Idle,
        );
        assert_eq!(command.log, ProcessExitNavigationLog::ExecuteShutdown);
        assert!(matches!(
            command.effect,
            ProcessExitNavigationEffect::Execute(Command::Shutdown)
        ));
    }

    #[test]
    fn fade_completion_exit_plan_maps_latched_intent() {
        assert_eq!(
            fade_completion_exit_plan(crate::ExitIntent::None),
            FadeCompletionExitPlan::Continue
        );
        assert_eq!(
            fade_completion_exit_plan(crate::ExitIntent::Exit),
            FadeCompletionExitPlan::Exit
        );
        assert_eq!(
            fade_completion_exit_plan(crate::ExitIntent::Shutdown),
            FadeCompletionExitPlan::Shutdown
        );
    }

    #[test]
    fn fade_completion_plan_supplies_root_exit_effects_and_logs() {
        assert_eq!(
            fade_completion_plan(ExitIntent::None),
            FadeCompletionPlan {
                effect: FadeCompletionEffect::Continue,
                log: None,
            }
        );
        assert_eq!(
            fade_completion_plan(ExitIntent::Exit),
            FadeCompletionPlan {
                effect: FadeCompletionEffect::Exit,
                log: Some("Fade-out complete; exiting application."),
            }
        );
        assert_eq!(
            fade_completion_plan(ExitIntent::Shutdown),
            FadeCompletionPlan {
                effect: FadeCompletionEffect::Shutdown,
                log: Some("Fade-out complete; powering off host and exiting."),
            }
        );
    }

    #[test]
    fn idle_transition_steps_the_active_screen() {
        let mut transition = TransitionState::Idle;

        assert_eq!(
            transition.advance_frame(0.1),
            TransitionFrame {
                step_screen: true,
                sync_input_capture: false,
                completion: None,
            }
        );
    }

    #[test]
    fn play_routes_use_full_entry_even_for_theme_actor_transitions() {
        for target in [Screen::Gameplay, Screen::Practice] {
            assert_eq!(ActiveTheme::transition_kind(Screen::Menu, target), TransitionKind::Actors);
            assert!(matches!(
                navigation_transition_start::<ActiveTheme>(Screen::Menu, target, &TransitionState::Idle),
                NavigationTransitionStart::GlobalFade { target: actual, .. } if actual == target
            ));
        }
    }

    #[test]
    fn evaluation_fade_keeps_gameplay_running_until_completion() {
        let mut transition = global_fade_out_transition(Screen::Evaluation, 0.4);

        assert!(tick_gameplay_during_transition::<ActiveTheme>(
            Screen::Gameplay,
            &transition
        ));
        let first = transition.advance_frame(0.2);
        assert_eq!(first.completion, None);

        assert!(tick_gameplay_during_transition::<ActiveTheme>(
            Screen::Gameplay,
            &transition
        ));
        let finished = transition.advance_frame(0.2);
        assert_eq!(
            finished.completion,
            Some(TransitionCompletion::GlobalFadeOut(Screen::Evaluation))
        );
    }

    #[test]
    fn unrelated_fade_out_does_not_tick_gameplay() {
        let mut transition = global_fade_out_transition(Screen::Menu, 0.1);

        assert!(!tick_gameplay_during_transition::<ActiveTheme>(
            Screen::Gameplay,
            &transition
        ));
        let plan = transition.advance_frame(0.1);

        assert_eq!(
            plan.completion,
            Some(TransitionCompletion::GlobalFadeOut(Screen::Menu))
        );
    }

    #[test]
    fn fade_in_finishes_without_stepping_screen_on_same_frame() {
        let mut transition = TransitionState::FadingIn {
            elapsed: 0.3,
            duration: 0.4,
        };

        assert!(tick_gameplay_during_transition::<ActiveTheme>(
            Screen::Gameplay,
            &transition
        ));
        let plan = transition.advance_frame(0.1);

        assert!(!plan.step_screen);
        assert!(plan.sync_input_capture);
        assert!(matches!(transition, TransitionState::Idle));
    }

    #[test]
    fn actor_fades_complete_and_enter_idle_at_their_own_boundaries() {
        let mut fade_out = TransitionState::ActorsFadeOut {
            elapsed: 0.3,
            duration: 0.4,
            target: Screen::Options,
        };
        assert_eq!(
            fade_out.advance_frame(0.1).completion,
            Some(TransitionCompletion::ActorFadeOut(Screen::Options))
        );

        let mut fade_in = TransitionState::ActorsFadeIn { elapsed: 0.6, duration: 0.65 };
        let plan = fade_in.advance_frame(0.05);
        assert!(!plan.step_screen);
        assert!(plan.sync_input_capture);
        assert!(matches!(fade_in, TransitionState::Idle));
    }
}
