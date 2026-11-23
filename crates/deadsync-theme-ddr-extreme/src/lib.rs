//! DDR Extreme's original arcade presentation for the shared DeadSync game.

mod menu;
mod options;
mod resources;
mod scene;
mod selection;
mod stage;

use deadlib_present::actors::Actor;
use deadsync_theme::{Screen, Theme, ThemeScreenId, ThemeTransitions, TransitionKind};

pub use options::{ExtremePlayerOptions, ExtremeSystemOptions};
pub use selection::ExtremeSelection;
pub use stage::{ExtremeAttract, ExtremeStage};

pub struct DdrExtremeTheme;

impl Theme for DdrExtremeTheme {
    type Screen = Screen;
    type RuntimeRequest = std::convert::Infallible;

    fn screen_id(screen: Screen) -> ThemeScreenId {
        ThemeScreenId::new(screen.current_screen_file_name())
    }
}

impl ThemeTransitions for DdrExtremeTheme {
    type Actor = Actor;
    type EntryContext<'a> = ();

    fn transition_kind(from: Screen, to: Screen) -> TransitionKind {
        if from == Screen::Menu || to == Screen::Menu {
            TransitionKind::Actors
        } else {
            TransitionKind::Direct
        }
    }

    fn actor_entry_duration(screen: Screen) -> Option<f32> {
        (screen == Screen::Menu).then_some(menu::ENTRY_SECONDS)
    }

    fn actor_exit_duration(screen: Screen) -> f32 {
        if screen == Screen::Menu {
            menu::EXIT_SECONDS
        } else {
            0.0
        }
    }

    fn skip_global_entry_fade(_from: Screen, _to: Screen) -> bool {
        false
    }

    fn out_transition(from: Screen, _to: Screen) -> (Vec<Actor>, f32) {
        (Vec::new(), Self::actor_exit_duration(from))
    }

    fn in_transition(screen: Screen, (): ()) -> (Vec<Actor>, f32) {
        (
            Vec::new(),
            Self::actor_entry_duration(screen).unwrap_or_default(),
        )
    }
}
