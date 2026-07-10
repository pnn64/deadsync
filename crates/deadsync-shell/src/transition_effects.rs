use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide};
use deadsync_screens::{Screen, player_options::SpeedMod};

use crate::{Command, TransitionMusicPaths, player_options_persist_plan, transition_audio_plan};

pub struct PlayerOptionsTransition<'a> {
    pub speed_mod: &'a [SpeedMod; MAX_PLAYERS],
    pub chart_difficulty_index: [usize; MAX_PLAYERS],
    pub music_rate: f32,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
}

pub struct TransitionEffectContext<'a> {
    pub previous: Screen,
    pub target: Screen,
    pub menu_music_enabled: bool,
    pub gameover_music_enabled: bool,
    pub music_paths: TransitionMusicPaths,
    pub player_options: Option<PlayerOptionsTransition<'a>>,
    pub select_music_preferred_difficulty: Option<usize>,
}

pub struct TransitionEffectPlan {
    pub commands: Vec<Command>,
    pub stop_screen_sfx: bool,
    pub clear_play_background: bool,
    pub preferred_difficulty_index: Option<usize>,
}

pub fn transition_effect_plan(context: TransitionEffectContext<'_>) -> TransitionEffectPlan {
    let audio = transition_audio_plan(
        context.previous,
        context.target,
        context.menu_music_enabled,
        context.gameover_music_enabled,
        context.music_paths,
    );
    let mut commands = audio.commands;
    let mut preferred_difficulty_index = None;

    if matches!(
        context.previous,
        Screen::SelectMusic | Screen::PlayerOptions
    ) {
        if let Some(options) = context.player_options {
            let persist = player_options_persist_plan(
                options.speed_mod,
                options.chart_difficulty_index,
                options.music_rate,
                options.play_style,
                options.player_side,
            );
            preferred_difficulty_index = Some(persist.preferred_difficulty_index);
            commands.extend(persist.commands);
        }

        if !matches!(
            context.target,
            Screen::SelectMusic | Screen::PlayerOptions | Screen::Gameplay | Screen::SelectCourse
        ) {
            commands.push(Command::StopMusic);
        }
    }

    if context.previous == Screen::SelectMusic {
        preferred_difficulty_index = context.select_music_preferred_difficulty;
    }

    TransitionEffectPlan {
        commands,
        stop_screen_sfx: audio.stop_screen_sfx,
        clear_play_background: audio.clear_play_background,
        preferred_difficulty_index,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_screens::player_options::{SpeedMod, SpeedModType};

    use super::*;

    fn paths() -> TransitionMusicPaths {
        TransitionMusicPaths {
            menu: PathBuf::from("menu.ogg"),
            course: PathBuf::from("course.ogg"),
            credits: PathBuf::from("credits.ogg"),
            gameover: PathBuf::from("gameover.ogg"),
        }
    }

    fn options<'a>(speed_mod: &'a [SpeedMod; MAX_PLAYERS]) -> PlayerOptionsTransition<'a> {
        PlayerOptionsTransition {
            speed_mod,
            chart_difficulty_index: [2, 5],
            music_rate: 1.1,
            play_style: PlayStyle::Single,
            player_side: PlayerSide::P2,
        }
    }

    #[test]
    fn player_options_persists_settings_before_leaving_wheel_flow() {
        let speed_mod = [
            SpeedMod {
                mod_type: SpeedModType::X,
                value: 1.5,
            },
            SpeedMod {
                mod_type: SpeedModType::M,
                value: 650.0,
            },
        ];
        let plan = transition_effect_plan(TransitionEffectContext {
            previous: Screen::PlayerOptions,
            target: Screen::Menu,
            menu_music_enabled: false,
            gameover_music_enabled: false,
            music_paths: paths(),
            player_options: Some(options(&speed_mod)),
            select_music_preferred_difficulty: None,
        });

        assert_eq!(plan.preferred_difficulty_index, Some(5));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            Command::UpdateScrollSpeed {
                side: PlayerSide::P2,
                setting: ScrollSpeedSetting::MMod(650.0),
            }
        )));
        assert!(matches!(plan.commands.last(), Some(Command::StopMusic)));
    }

    #[test]
    fn gameplay_handoff_keeps_music_and_persists_options() {
        let speed_mod = [
            SpeedMod {
                mod_type: SpeedModType::C,
                value: 300.0,
            },
            SpeedMod {
                mod_type: SpeedModType::C,
                value: 600.0,
            },
        ];
        let plan = transition_effect_plan(TransitionEffectContext {
            previous: Screen::PlayerOptions,
            target: Screen::Gameplay,
            menu_music_enabled: false,
            gameover_music_enabled: false,
            music_paths: paths(),
            player_options: Some(options(&speed_mod)),
            select_music_preferred_difficulty: None,
        });

        assert_eq!(plan.preferred_difficulty_index, Some(5));
        assert!(
            !plan
                .commands
                .iter()
                .any(|command| matches!(command, Command::StopMusic))
        );
        assert_eq!(plan.commands.len(), 3);
    }

    #[test]
    fn select_music_handoff_uses_wheel_preference_and_forwards_cleanup() {
        let plan = transition_effect_plan(TransitionEffectContext {
            previous: Screen::SelectMusic,
            target: Screen::Menu,
            menu_music_enabled: false,
            gameover_music_enabled: false,
            music_paths: paths(),
            player_options: None,
            select_music_preferred_difficulty: Some(4),
        });

        assert_eq!(plan.preferred_difficulty_index, Some(4));
        assert_eq!(
            plan.commands
                .iter()
                .filter(|command| matches!(command, Command::StopMusic))
                .count(),
            2,
        );
    }
}
