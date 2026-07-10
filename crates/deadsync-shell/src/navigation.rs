use crate::Command;
use deadlib_platform::dirs;
use deadsync_config::navigation::{
    AppTransitionScreen, MachineFlowScreen, app_screen_actor_fades, app_transition_actor_only,
};
use deadsync_screens::Screen;
use std::path::PathBuf;

const FADE_OUT_DURATION: f32 = 0.4;
const MENU_TO_SELECT_COLOR_OUT_DURATION: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionMusicAction {
    Keep,
    PlayMenu,
    PlayGameOver,
    Stop,
}

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

/// Select the music action for a completed actor-only screen transition.
pub fn transition_music_action(
    previous: Screen,
    target: Screen,
    menu_music_enabled: bool,
    gameover_music_enabled: bool,
) -> TransitionMusicAction {
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
            TransitionMusicAction::Keep
        } else {
            TransitionMusicAction::PlayMenu
        }
    } else if target_gameover_music {
        if previous_gameover_music {
            TransitionMusicAction::Keep
        } else {
            TransitionMusicAction::PlayGameOver
        }
    } else if previous_menu_music || !keep_preview {
        TransitionMusicAction::Stop
    } else {
        TransitionMusicAction::Keep
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

pub fn menu_exit_uses_fade(screen: Screen, transition: &TransitionState) -> bool {
    screen == Screen::Menu && matches!(transition, TransitionState::Idle)
}

#[inline(always)]
pub const fn machine_flow_screen(screen: Screen) -> Option<MachineFlowScreen> {
    match screen {
        Screen::Menu => Some(MachineFlowScreen::Menu),
        Screen::SelectProfile => Some(MachineFlowScreen::SelectProfile),
        Screen::SelectColor => Some(MachineFlowScreen::SelectColor),
        Screen::SelectStyle => Some(MachineFlowScreen::SelectStyle),
        Screen::SelectPlayMode => Some(MachineFlowScreen::SelectPlayMode),
        Screen::ProfileLoad => Some(MachineFlowScreen::ProfileLoad),
        Screen::EvaluationSummary => Some(MachineFlowScreen::EvaluationSummary),
        Screen::Initials => Some(MachineFlowScreen::Initials),
        Screen::GameOver => Some(MachineFlowScreen::GameOver),
        _ => None,
    }
}

#[inline(always)]
pub const fn screen_from_machine_flow(screen: MachineFlowScreen) -> Screen {
    match screen {
        MachineFlowScreen::Menu => Screen::Menu,
        MachineFlowScreen::SelectProfile => Screen::SelectProfile,
        MachineFlowScreen::SelectColor => Screen::SelectColor,
        MachineFlowScreen::SelectStyle => Screen::SelectStyle,
        MachineFlowScreen::SelectPlayMode => Screen::SelectPlayMode,
        MachineFlowScreen::ProfileLoad => Screen::ProfileLoad,
        MachineFlowScreen::EvaluationSummary => Screen::EvaluationSummary,
        MachineFlowScreen::Initials => Screen::Initials,
        MachineFlowScreen::GameOver => Screen::GameOver,
    }
}

#[inline(always)]
const fn app_transition_screen(screen: Screen) -> Option<AppTransitionScreen> {
    match screen {
        Screen::Menu => Some(AppTransitionScreen::Menu),
        Screen::Options => Some(AppTransitionScreen::Options),
        Screen::SelectProfile => Some(AppTransitionScreen::SelectProfile),
        Screen::SelectColor => Some(AppTransitionScreen::SelectColor),
        Screen::SelectStyle => Some(AppTransitionScreen::SelectStyle),
        Screen::Mappings => Some(AppTransitionScreen::Mappings),
        Screen::Input => Some(AppTransitionScreen::Input),
        Screen::TestLights => Some(AppTransitionScreen::TestLights),
        Screen::OverscanAdjustment => Some(AppTransitionScreen::OverscanAdjustment),
        Screen::SmxAssignPads => Some(AppTransitionScreen::SmxAssignPads),
        Screen::ManageLocalProfiles => Some(AppTransitionScreen::ManageLocalProfiles),
        _ => None,
    }
}

#[inline(always)]
pub const fn is_actor_fade_screen(screen: Screen) -> bool {
    match app_transition_screen(screen) {
        Some(screen) => app_screen_actor_fades(screen),
        None => false,
    }
}

#[inline(always)]
pub const fn is_actor_only_transition(from: Screen, to: Screen) -> bool {
    match (app_transition_screen(from), app_transition_screen(to)) {
        (Some(from), Some(to)) => app_transition_actor_only(from, to),
        _ => false,
    }
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
    fn machine_flow_mapping_round_trips() {
        for screen in [
            Screen::Menu,
            Screen::SelectProfile,
            Screen::SelectColor,
            Screen::SelectStyle,
            Screen::SelectPlayMode,
            Screen::ProfileLoad,
            Screen::EvaluationSummary,
            Screen::Initials,
            Screen::GameOver,
        ] {
            assert_eq!(
                screen_from_machine_flow(machine_flow_screen(screen).unwrap()),
                screen
            );
        }
        assert_eq!(machine_flow_screen(Screen::Gameplay), None);
    }

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
    fn transition_music_preserves_preview_and_avoids_restarts() {
        assert_eq!(
            transition_music_action(Screen::SelectMusic, Screen::PlayerOptions, true, false),
            TransitionMusicAction::Keep
        );
        assert_eq!(
            transition_music_action(Screen::Menu, Screen::SelectColor, true, false),
            TransitionMusicAction::PlayMenu
        );
        assert_eq!(
            transition_music_action(Screen::SelectColor, Screen::SelectStyle, true, false),
            TransitionMusicAction::Keep
        );
        assert_eq!(
            transition_music_action(Screen::Menu, Screen::GameOver, false, true),
            TransitionMusicAction::PlayGameOver
        );
        assert_eq!(
            transition_music_action(Screen::Options, Screen::Menu, true, false),
            TransitionMusicAction::Stop
        );
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
}
