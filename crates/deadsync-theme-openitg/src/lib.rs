//! OpenITG-inspired presentation for DeadSync's shared game and session flow.

mod demonstration;
mod evaluation;
mod gameplay;
mod gameplay_outro;
mod intro;
mod menu;
mod option_explanation;
mod play_selection;
mod player_options;
mod presentation;
mod profile_picker;
mod resources;
mod select_music;
mod selection_presentation;
mod stage_loading;
mod stage_mount;
mod system_options;

use deadlib_present::actors::Actor;
use deadsync_theme::{Screen, Theme, ThemeScreenId, ThemeTransitions, TransitionKind};

pub use demonstration::{OpenItgTrainingFeedback, push_training_underlay};
pub use evaluation::{EVALUATION_OVERVIEW_SETTLED_SECONDS, OpenItgEvaluation};
pub use gameplay::GAMEPLAY_HUD;
pub use gameplay_outro::GameplayOutro;
pub use intro::{OpenItgDemonstration, OpenItgIntro, OpenItgIntroMedia};
pub use menu::MenuState;
pub use play_selection::{PlaySelectionCarousel, push_profile_load_actors};
pub use player_options::OpenItgPlayerOptions;
pub use profile_picker::push_profile_picker_actors;
pub use select_music::{
    WHEEL_BANNER_FADE, WHEEL_TO_PLAYER_OPTIONS_SECONDS, WheelBannerFrame, WheelBannerMotion,
    push_select_course_actors, push_select_music_actors,
};
pub use selection_presentation::OpenItgSelectionPresentation;
pub use stage_loading::push_stage_loading_actors;
pub use stage_mount::OpenItgStagePresentation;
pub use system_options::OpenItgSystemOptions;

/// The OpenITG-inspired theme.
pub struct OpenItgTheme;

impl Theme for OpenItgTheme {
    type Screen = Screen;
    type RuntimeRequest = std::convert::Infallible;

    fn screen_id(screen: Self::Screen) -> ThemeScreenId {
        ThemeScreenId::new(match screen {
            Screen::Init => "ScreenAttract",
            Screen::Options => "ScreenOptionsMenu",
            _ => screen.current_screen_file_name(),
        })
    }
}

impl ThemeTransitions for OpenItgTheme {
    type Actor = Actor;
    type EntryContext<'a> = ();

    fn transition_kind(from: Self::Screen, to: Self::Screen) -> TransitionKind {
        if matches!(from, Screen::Menu | Screen::PlayerOptions)
            || matches!(to, Screen::Menu | Screen::PlayerOptions)
        {
            TransitionKind::Actors
        } else {
            TransitionKind::Direct
        }
    }

    fn actor_entry_duration(screen: Self::Screen) -> Option<f32> {
        match screen {
            Screen::Menu => Some(menu::ENTRY_SECONDS),
            Screen::PlayerOptions => Some(player_options::ENTRY_SECONDS),
            _ => None,
        }
    }

    fn actor_exit_duration(screen: Self::Screen) -> f32 {
        match screen {
            Screen::Menu => menu::EXIT_SECONDS,
            Screen::PlayerOptions => player_options::TRANSITION_SECONDS,
            _ => 0.0,
        }
    }

    fn skip_global_entry_fade(_from: Self::Screen, _to: Self::Screen) -> bool {
        false
    }

    fn out_transition(from: Self::Screen, _to: Self::Screen) -> (Vec<Self::Actor>, f32) {
        let duration = Self::actor_exit_duration(from);
        let actors = if from == Screen::PlayerOptions {
            vec![presentation::black_fade(duration, false)]
        } else {
            // The title actors fade themselves using the shared clock.
            Vec::new()
        };
        (actors, duration)
    }

    fn in_transition(screen: Self::Screen, (): Self::EntryContext<'_>) -> (Vec<Self::Actor>, f32) {
        let duration = Self::actor_entry_duration(screen).unwrap_or_default();
        (Vec::new(), duration)
    }
}
