//! Native Rust presentation using the bundled StepMania 3.95 theme assets.

mod evaluation;
mod gameplay;
mod gameplay_transition;
mod intro;
mod menu;
mod options;
mod presentation;
mod resources;
mod select_music;
mod stage;
mod stage_presentation;

use deadlib_present::actors::Actor;
use deadsync_theme::{
    Screen, Theme, ThemeScreenId, ThemeTransitionAudio, ThemeTransitions, TransitionKind,
    TransitionMusic,
};

pub use intro::StepManiaIntro;
pub use menu::MenuState;
pub use options::{StepManiaPlayerOptions, StepManiaSystemOptions};
pub use select_music::StepManiaMusicWheel;
pub use stage::StepManiaStageCard;
pub use stage_presentation::StepManiaStagePresentation;

pub struct StepManiaTheme;

impl ThemeTransitionAudio for StepManiaTheme {
    type AudioContext = bool;

    fn transition_audio(
        _from: Screen,
        to: Screen,
        _kind: TransitionKind,
        music_enabled: bool,
    ) -> TransitionMusic {
        match to {
            Screen::Menu | Screen::Options | Screen::Init => TransitionMusic::Stop,
            Screen::SelectProfile | Screen::SelectStyle | Screen::SelectPlayMode
                if music_enabled =>
            {
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
}

impl Theme for StepManiaTheme {
    type Screen = Screen;
    type RuntimeRequest = std::convert::Infallible;

    fn screen_id(screen: Screen) -> ThemeScreenId {
        ThemeScreenId::new(screen.current_screen_file_name())
    }
}

impl ThemeTransitions for StepManiaTheme {
    type Actor = Actor;
    type EntryContext<'a> = ();

    fn transition_kind(_from: Screen, _to: Screen) -> TransitionKind {
        TransitionKind::Direct
    }

    fn actor_entry_duration(screen: Screen) -> Option<f32> {
        (screen == Screen::Menu).then_some(0.3)
    }

    fn actor_exit_duration(screen: Screen) -> f32 {
        if screen == Screen::Menu { 0.3 } else { 0.0 }
    }

    fn skip_global_entry_fade(_from: Screen, _to: Screen) -> bool {
        false
    }

    fn in_transition(_screen: Screen, (): ()) -> (Vec<Actor>, f32) {
        (Vec::new(), 0.3)
    }

    fn out_transition(_from: Screen, _to: Screen) -> (Vec<Actor>, f32) {
        (Vec::new(), 0.3)
    }
}
