//! Native Rust presentation using the bundled StepMania 3.95 theme assets.

mod evaluation;
mod gameplay;
mod gameplay_transition;
mod intro;
mod menu;
mod options;
mod play_selection;
mod post_game;
mod presentation;
mod radar;
mod ranking;
mod resources;
mod select_music;
mod selection;
mod stage;
mod stage_presentation;
mod startup;
mod training_pad;
mod transitions;
mod tutorial;

use deadsync_theme::{
    Screen, Theme, ThemeScreenId, ThemeTransitionAudio, TransitionKind, TransitionMusic,
};

pub use intro::StepManiaIntro;
pub use menu::MenuState;
pub use options::{StepManiaPlayerOptions, StepManiaSystemOptions};
pub use post_game::StepManiaSessionEnd;
pub use select_music::StepManiaMusicWheel;
pub use stage::StepManiaStageCard;
pub use stage_presentation::StepManiaStagePresentation;
pub use startup::StepManiaStartup;

pub struct StepManiaTheme;

impl ThemeTransitionAudio for StepManiaTheme {
    type AudioContext = bool;

    fn transition_audio(
        from: Screen,
        to: Screen,
        _kind: TransitionKind,
        music_enabled: bool,
    ) -> TransitionMusic {
        match to {
            Screen::Menu
            | Screen::Options
            | Screen::Init
            | Screen::GameOver
            | Screen::EvaluationSummary => TransitionMusic::Stop,
            Screen::Initials if music_enabled => TransitionMusic::Play {
                path: "assets/media/stepmania-395/name-entry.ogg".into(),
                looping: true,
            },
            Screen::SelectProfile | Screen::SelectStyle | Screen::SelectPlayMode
                if music_enabled =>
            {
                if matches!(
                    from,
                    Screen::SelectProfile
                        | Screen::ProfileLoad
                        | Screen::SelectStyle
                        | Screen::SelectPlayMode
                ) {
                    return TransitionMusic::Keep;
                }
                TransitionMusic::Play {
                    path: "assets/media/stepmania-395/menu.ogg".into(),
                    looping: true,
                }
            }
            _ => TransitionMusic::Keep,
        }
    }

    fn stop_screen_sfx_on_exit(_from: Screen, _to: Screen) -> bool {
        true
    }

    fn entry_sound(screen: Screen) -> Option<&'static str> {
        match screen {
            Screen::SelectProfile
            | Screen::SelectStyle
            | Screen::SelectPlayMode
            | Screen::SelectMusic
            | Screen::SelectCourse
            | Screen::Options
            | Screen::PlayerOptions => Some(resources::SWOOSH),
            Screen::Evaluation | Screen::EvaluationSummary => Some(resources::RESULT_SWOOSH),
            _ => None,
        }
    }

    fn exit_sound(from: Screen, _to: Screen) -> Option<&'static str> {
        match from {
            Screen::SelectProfile
            | Screen::SelectStyle
            | Screen::SelectPlayMode
            | Screen::Options
            | Screen::PlayerOptions => Some(resources::SWOOSH),
            Screen::Evaluation | Screen::EvaluationSummary => Some(resources::RESULT_SWOOSH),
            _ => None,
        }
    }
}

impl Theme for StepManiaTheme {
    type Screen = Screen;
    type RuntimeRequest = std::convert::Infallible;

    fn screen_id(screen: Screen) -> ThemeScreenId {
        ThemeScreenId::new(screen.current_screen_file_name())
    }
}
