use crate::Command;
use crate::interaction::{ExitIntent, ProcessExitPlan, ProcessExitRequest, ShellInteractionState};
use crate::runtime::ShellState;
use deadlib_platform::dirs;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use std::path::PathBuf;

const FADE_OUT_DURATION: f32 = 0.4;
const MENU_TO_SELECT_COLOR_OUT_DURATION: f32 = 1.0;

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

#[derive(Debug)]
pub enum TransitionState {
    Idle,
    FadingOut {
        elapsed: f32,
        duration: f32,
        target: Screen,
    },
    FadingIn {
        elapsed: f32,
        duration: f32,
    },
    ActorsFadeOut {
        elapsed: f32,
        duration: f32,
        target: Screen,
    },
    ActorsFadeIn {
        elapsed: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionCompletion {
    GlobalFadeOut(Screen),
    ActorFadeOut(Screen),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionFramePlan {
    pub tick_gameplay: bool,
    pub step_screen: bool,
    pub completion: Option<TransitionCompletion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenChangePlan {
    pub leave_lobby: bool,
    pub exit_gameplay: bool,
    pub clear_text_layout_cache: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationTransitionStart {
    DirectEntry {
        target: Screen,
    },
    Busy,
    ActorFade {
        from: Screen,
        target: Screen,
    },
    GlobalFade {
        target: Screen,
        stop_screen_sfx: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FadeCompletionExitPlan {
    Continue,
    Exit,
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationTransitionEffectPlan {
    pub start: NavigationTransitionStart,
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

impl TransitionState {
    pub fn advance_frame(
        &mut self,
        delta_time: f32,
        current_screen: Screen,
        actor_fade_in_duration: f32,
    ) -> TransitionFramePlan {
        let mut plan = TransitionFramePlan {
            tick_gameplay: false,
            step_screen: false,
            completion: None,
        };
        match self {
            Self::Idle => plan.step_screen = true,
            Self::FadingOut {
                elapsed,
                duration,
                target,
            } => {
                *elapsed += delta_time;
                plan.tick_gameplay =
                    *target == Screen::Evaluation && current_screen == Screen::Gameplay;
                if *elapsed >= *duration {
                    plan.completion = Some(TransitionCompletion::GlobalFadeOut(*target));
                }
            }
            Self::ActorsFadeOut {
                elapsed,
                duration,
                target,
            } => {
                *elapsed += delta_time;
                if *elapsed >= *duration {
                    plan.completion = Some(TransitionCompletion::ActorFadeOut(*target));
                }
            }
            Self::FadingIn { elapsed, duration } => {
                *elapsed += delta_time;
                plan.tick_gameplay = current_screen == Screen::Gameplay;
                if *elapsed >= *duration {
                    *self = Self::Idle;
                }
            }
            Self::ActorsFadeIn { elapsed } => {
                *elapsed += delta_time;
                if *elapsed >= actor_fade_in_duration {
                    *self = Self::Idle;
                }
            }
        }
        plan
    }
}

pub fn screen_change_plan(previous: Screen, target: Screen) -> ScreenChangePlan {
    ScreenChangePlan {
        leave_lobby: previous != target && matches!(target, Screen::Menu),
        exit_gameplay: previous == Screen::Gameplay && target != Screen::Gameplay,
        clear_text_layout_cache: previous != target,
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

pub fn actor_fade_out_transition(
    from: Screen,
    target: Screen,
    select_color_duration: f32,
    select_profile_duration: f32,
) -> TransitionState {
    let duration = if from == Screen::Menu
        && matches!(
            target,
            Screen::SelectProfile | Screen::SelectColor | Screen::Options
        ) {
        MENU_TO_SELECT_COLOR_OUT_DURATION
    } else if from == Screen::SelectColor {
        select_color_duration
    } else if from == Screen::SelectProfile {
        select_profile_duration
    } else {
        FADE_OUT_DURATION
    };
    TransitionState::ActorsFadeOut {
        elapsed: 0.0,
        duration,
        target,
    }
}

pub const fn global_fade_out_transition(target: Screen, duration: f32) -> TransitionState {
    TransitionState::FadingOut {
        elapsed: 0.0,
        duration,
        target,
    }
}

pub const fn actor_entry_transition(target: Screen) -> TransitionState {
    if is_actor_fade_screen(target) {
        TransitionState::ActorsFadeIn { elapsed: 0.0 }
    } else {
        TransitionState::Idle
    }
}

pub const fn global_entry_transition(
    previous: Screen,
    target: Screen,
    duration: f32,
) -> TransitionState {
    if matches!(
        (previous, target),
        (Screen::Options, Screen::Credits) | (Screen::Credits, Screen::Options)
    ) {
        TransitionState::Idle
    } else {
        TransitionState::FadingIn {
            elapsed: 0.0,
            duration,
        }
    }
}

pub fn apply_actor_entry_transition(shell: &mut ShellState, target: Screen) {
    shell.transition = actor_entry_transition(target);
}

pub fn apply_actor_fade_out_transition(
    shell: &mut ShellState,
    from: Screen,
    target: Screen,
    select_color_duration: f32,
    select_profile_duration: f32,
) {
    shell.transition =
        actor_fade_out_transition(from, target, select_color_duration, select_profile_duration);
}

pub fn apply_global_fade_out_transition(shell: &mut ShellState, target: Screen, duration: f32) {
    shell.transition = global_fade_out_transition(target, duration);
}

pub fn apply_global_entry_transition(
    shell: &mut ShellState,
    previous: Screen,
    target: Screen,
    duration: f32,
) {
    shell.transition = global_entry_transition(previous, target, duration);
}

pub fn menu_exit_uses_fade(screen: Screen, transition: &TransitionState) -> bool {
    screen == Screen::Menu && matches!(transition, TransitionState::Idle)
}

pub fn navigation_transition_start(
    from: Screen,
    target: Screen,
    transition: &TransitionState,
) -> NavigationTransitionStart {
    if from == Screen::Init && target == Screen::Menu {
        return NavigationTransitionStart::DirectEntry { target };
    }
    if !matches!(transition, TransitionState::Idle) {
        return NavigationTransitionStart::Busy;
    }
    if is_actor_only_transition(from, target) {
        NavigationTransitionStart::ActorFade { from, target }
    } else {
        NavigationTransitionStart::GlobalFade {
            target,
            stop_screen_sfx: from == Screen::Evaluation && target != Screen::Evaluation,
        }
    }
}

pub fn navigation_transition_effect_plan(
    from: Screen,
    target: Screen,
    transition: &TransitionState,
) -> NavigationTransitionEffectPlan {
    let start = navigation_transition_start(from, target, transition);
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

#[inline(always)]
pub const fn is_actor_fade_screen(screen: Screen) -> bool {
    deadsync_theme_simply_love::screens::uses_actor_fade(screen)
}

#[inline(always)]
pub const fn is_actor_only_transition(from: Screen, to: Screen) -> bool {
    deadsync_theme_simply_love::screens::uses_actor_only_transition(from, to)
}

pub fn write_current_screen_file(screen: Screen) {
    if !deadsync_config::runtime::get().write_current_screen {
        return;
    }
    let path = dirs::app_dirs().current_screen_path();
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        log::warn!("Failed to create current_screen.txt parent dir: {e}");
        return;
    }
    if let Err(e) = std::fs::write(&path, screen.current_screen_file_name()) {
        log::warn!("Failed to write current_screen.txt: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_fade_policy_uses_screen_contract() {
        assert!(is_actor_fade_screen(Screen::Menu));
        assert!(is_actor_only_transition(Screen::Menu, Screen::Options));
        assert!(!is_actor_only_transition(
            Screen::Gameplay,
            Screen::Evaluation
        ));
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
    fn transition_constructors_keep_existing_fade_policy() {
        assert!(matches!(
            actor_fade_out_transition(Screen::Menu, Screen::Options, 0.2, 0.3),
            TransitionState::ActorsFadeOut { duration: 1.0, .. }
        ));
        assert!(matches!(
            actor_entry_transition(Screen::Menu),
            TransitionState::ActorsFadeIn { .. }
        ));
        assert!(matches!(
            global_entry_transition(Screen::Options, Screen::Credits, 0.5),
            TransitionState::Idle
        ));
        assert!(menu_exit_uses_fade(Screen::Menu, &TransitionState::Idle));
        assert!(!menu_exit_uses_fade(
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

        apply_actor_fade_out_transition(&mut shell, Screen::Menu, Screen::Options, 0.2, 0.3);
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
            screen_change_plan(Screen::Gameplay, Screen::Menu),
            ScreenChangePlan {
                leave_lobby: true,
                exit_gameplay: true,
                clear_text_layout_cache: true,
            }
        );
        assert_eq!(
            screen_change_plan(Screen::Menu, Screen::Menu),
            ScreenChangePlan {
                leave_lobby: false,
                exit_gameplay: false,
                clear_text_layout_cache: false,
            }
        );
        assert_eq!(
            screen_change_plan(Screen::Evaluation, Screen::Menu),
            ScreenChangePlan {
                leave_lobby: true,
                exit_gameplay: false,
                clear_text_layout_cache: true,
            }
        );
    }

    #[test]
    fn navigation_transition_start_preserves_entry_and_fade_policy() {
        assert_eq!(
            navigation_transition_start(Screen::Init, Screen::Menu, &TransitionState::Idle),
            NavigationTransitionStart::DirectEntry {
                target: Screen::Menu
            }
        );
        assert_eq!(
            navigation_transition_start(
                Screen::Menu,
                Screen::Options,
                &TransitionState::FadingIn {
                    elapsed: 0.0,
                    duration: 0.4,
                },
            ),
            NavigationTransitionStart::Busy
        );
        assert_eq!(
            navigation_transition_start(Screen::Menu, Screen::Options, &TransitionState::Idle),
            NavigationTransitionStart::ActorFade {
                from: Screen::Menu,
                target: Screen::Options,
            }
        );
        assert_eq!(
            navigation_transition_start(Screen::Evaluation, Screen::Menu, &TransitionState::Idle,),
            NavigationTransitionStart::GlobalFade {
                target: Screen::Menu,
                stop_screen_sfx: true,
            }
        );
    }

    #[test]
    fn navigation_transition_effect_marks_exit_intent_clear_points() {
        let direct =
            navigation_transition_effect_plan(Screen::Init, Screen::Menu, &TransitionState::Idle);
        assert_eq!(
            direct,
            NavigationTransitionEffectPlan {
                start: NavigationTransitionStart::DirectEntry {
                    target: Screen::Menu
                },
                clear_exit_intent: false,
            }
        );

        let actor = navigation_transition_effect_plan(
            Screen::Menu,
            Screen::Options,
            &TransitionState::Idle,
        );
        assert_eq!(
            actor,
            NavigationTransitionEffectPlan {
                start: NavigationTransitionStart::ActorFade {
                    from: Screen::Menu,
                    target: Screen::Options,
                },
                clear_exit_intent: true,
            }
        );

        let busy = navigation_transition_effect_plan(
            Screen::Menu,
            Screen::Options,
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
            transition.advance_frame(0.1, Screen::Menu, 0.65),
            TransitionFramePlan {
                tick_gameplay: false,
                step_screen: true,
                completion: None,
            }
        );
    }

    #[test]
    fn evaluation_fade_keeps_gameplay_running_until_completion() {
        let mut transition = global_fade_out_transition(Screen::Evaluation, 0.4);

        let first = transition.advance_frame(0.2, Screen::Gameplay, 0.65);
        assert!(first.tick_gameplay);
        assert_eq!(first.completion, None);

        let finished = transition.advance_frame(0.2, Screen::Gameplay, 0.65);
        assert!(finished.tick_gameplay);
        assert_eq!(
            finished.completion,
            Some(TransitionCompletion::GlobalFadeOut(Screen::Evaluation))
        );
    }

    #[test]
    fn unrelated_fade_out_does_not_tick_gameplay() {
        let mut transition = global_fade_out_transition(Screen::Menu, 0.1);

        let plan = transition.advance_frame(0.1, Screen::Gameplay, 0.65);

        assert!(!plan.tick_gameplay);
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

        let plan = transition.advance_frame(0.1, Screen::Gameplay, 0.65);

        assert!(plan.tick_gameplay);
        assert!(!plan.step_screen);
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
            fade_out.advance_frame(0.1, Screen::Menu, 0.65).completion,
            Some(TransitionCompletion::ActorFadeOut(Screen::Options))
        );

        let mut fade_in = TransitionState::ActorsFadeIn { elapsed: 0.6 };
        let plan = fade_in.advance_frame(0.05, Screen::Menu, 0.65);
        assert!(!plan.step_screen);
        assert!(matches!(fade_in, TransitionState::Idle));
    }
}
