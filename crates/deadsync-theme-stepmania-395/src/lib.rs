//! Native Rust presentation using the bundled StepMania 3.95 theme assets.

mod intro;
mod menu;
mod presentation;
mod resources;
mod stage;

use deadlib_present::actors::Actor;
use deadsync_theme::{Screen, Theme, ThemeScreenId, ThemeTransitions, TransitionKind};

pub use intro::StepManiaIntro;
pub use menu::MenuState;
pub use stage::StepManiaStageCard;

pub struct StepManiaTheme;

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
