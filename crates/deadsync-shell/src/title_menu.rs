//! Connects the installed title presentation to the shared session flow.

use crate::{ActiveTheme, SessionState, ThemeScreen as Screen, TransitionState};
use deadlib_platform::input::RawKeyboardEvent;
use deadlib_present::actors::Actor;
use deadsync_config::theme::ThemeFlag;
use deadsync_input::InputEvent;
use deadsync_theme::main_menu::{MainMenuFrame, MainMenuPhase, ThemeMainMenu};
use deadsync_theme::{AudioRequest, ThemeEffect, ThemeTransitions, UiCue};
use deadsync_theme_openitg::{OpenItgScreen, OpenItgTheme};
use deadsync_theme_simply_love::{
    SimplyLoveEffect, SimplyLoveInputResult, SimplyLoveRuntimeRequest,
    views::SimplyLoveVisualPolicyView,
};

pub(crate) enum InstalledMenu {
    SimplyLove(<ActiveTheme as ThemeMainMenu>::MenuState),
    OpenItg(<OpenItgTheme as ThemeMainMenu>::MenuState),
}

impl InstalledMenu {
    pub(crate) fn new(flag: ThemeFlag, color_index: i32) -> Self {
        match flag {
            ThemeFlag::SimplyLove => Self::SimplyLove(ActiveTheme::init_menu(color_index)),
            ThemeFlag::OpenItg => Self::OpenItg(OpenItgTheme::init_menu(color_index)),
        }
    }

    pub(crate) fn entry_transition(&self) -> (Vec<Actor>, f32) {
        match self {
            Self::SimplyLove(_) => ActiveTheme::in_transition(Screen::Menu, None),
            Self::OpenItg(_) => OpenItgTheme::in_transition(OpenItgScreen::Menu, ()),
        }
    }

    pub(crate) fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32) {
        match self {
            Self::SimplyLove(_) => ActiveTheme::out_transition(Screen::Menu, target),
            Self::OpenItg(_) => {
                OpenItgTheme::out_transition(OpenItgScreen::Menu, openitg_screen(target))
            }
        }
    }

    pub(crate) fn actor_entry(&self) -> TransitionState {
        let duration = match self {
            Self::SimplyLove(_) => ActiveTheme::actor_entry_duration(Screen::Menu),
            Self::OpenItg(_) => OpenItgTheme::actor_entry_duration(OpenItgScreen::Menu),
        };
        duration.map_or(TransitionState::Idle, |duration| {
            TransitionState::ActorsFadeIn {
                elapsed: 0.0,
                duration,
            }
        })
    }

    pub(crate) fn actor_exit(&self, target: Screen) -> TransitionState {
        let duration = match self {
            Self::SimplyLove(_) => ActiveTheme::actor_exit_duration(Screen::Menu),
            Self::OpenItg(_) => OpenItgTheme::actor_exit_duration(OpenItgScreen::Menu),
        };
        TransitionState::ActorsFadeOut {
            elapsed: 0.0,
            duration,
            target,
        }
    }

    pub(crate) fn color_index(&self) -> i32 {
        match self {
            Self::SimplyLove(state) => ActiveTheme::menu_color_index(state),
            Self::OpenItg(state) => OpenItgTheme::menu_color_index(state),
        }
    }

    pub(crate) fn set_color_index(&mut self, color_index: i32) {
        match self {
            Self::SimplyLove(state) => ActiveTheme::set_menu_color_index(state, color_index),
            Self::OpenItg(state) => OpenItgTheme::set_menu_color_index(state, color_index),
        }
    }

    pub(crate) fn reset(&mut self) {
        match self {
            Self::SimplyLove(state) => ActiveTheme::reset_menu(state),
            Self::OpenItg(state) => OpenItgTheme::reset_menu(state),
        }
    }

    pub(crate) fn sync_runtime(&mut self, view: crate::main_menu::MainMenuRuntimeView) {
        match self {
            Self::SimplyLove(state) => ActiveTheme::sync_menu_runtime(state, view),
            Self::OpenItg(state) => OpenItgTheme::sync_menu_runtime(state, view),
        }
    }

    pub(crate) fn update(&mut self, dt: f32, phase: MainMenuPhase) -> SimplyLoveEffect {
        match self {
            Self::SimplyLove(state) => ActiveTheme::update_menu(state, dt, phase),
            Self::OpenItg(state) => host_effect(OpenItgTheme::update_menu(state, dt, phase)),
        }
    }

    pub(crate) fn handle_input(
        &mut self,
        session: &mut SessionState,
        input: &InputEvent,
    ) -> SimplyLoveEffect {
        match self {
            Self::SimplyLove(state) => {
                crate::main_menu::handle_input::<ActiveTheme>(state, session, input)
            }
            Self::OpenItg(state) => host_effect(crate::main_menu::handle_input::<OpenItgTheme>(
                state, session, input,
            )),
        }
    }

    pub(crate) fn handle_raw_key(&mut self, key: &RawKeyboardEvent) -> SimplyLoveInputResult {
        match self {
            Self::SimplyLove(state) => ActiveTheme::handle_menu_raw_key(state, key),
            Self::OpenItg(state) => {
                let result = OpenItgTheme::handle_menu_raw_key(state, key);
                SimplyLoveInputResult {
                    consumed: result.consumed,
                    effect: host_effect(result.effect),
                }
            }
        }
    }

    pub(crate) fn push_actors(
        &self,
        actors: &mut Vec<Actor>,
        frame: MainMenuFrame<'_, Screen, SimplyLoveVisualPolicyView>,
    ) {
        match self {
            Self::SimplyLove(state) => ActiveTheme::push_menu_actors(actors, state, frame),
            Self::OpenItg(state) => {
                let transition = openitg_transition(frame.transition);
                OpenItgTheme::push_menu_actors(
                    actors,
                    state,
                    MainMenuFrame {
                        update_banner_tag: frame.update_banner_tag,
                        alpha_multiplier: frame.alpha_multiplier,
                        transition: &transition,
                        visual_policy: (),
                    },
                );
            }
        }
    }

    pub(crate) fn push_exit_overlay(
        &self,
        actors: &mut Vec<Actor>,
        transition: &TransitionState,
        visual_policy: SimplyLoveVisualPolicyView,
    ) {
        match self {
            Self::SimplyLove(state) => {
                ActiveTheme::push_menu_exit_overlay(actors, state, transition, visual_policy)
            }
            Self::OpenItg(state) => OpenItgTheme::push_menu_exit_overlay(
                actors,
                state,
                &openitg_transition(transition),
                (),
            ),
        }
    }
}

fn host_screen(screen: OpenItgScreen) -> Screen {
    match screen {
        OpenItgScreen::Menu => Screen::Menu,
        OpenItgScreen::Attract => Screen::Init,
        OpenItgScreen::Play => Screen::SelectProfile,
        OpenItgScreen::Options => Screen::Options,
        OpenItgScreen::Content => Screen::ContentBrowser,
        OpenItgScreen::Sandbox => Screen::Sandbox,
    }
}

fn host_effect(effect: ThemeEffect<OpenItgScreen, UiCue>) -> SimplyLoveEffect {
    match effect {
        ThemeEffect::None => ThemeEffect::None,
        ThemeEffect::Batch(effects) => {
            ThemeEffect::batch(effects.into_iter().map(host_effect).collect())
        }
        ThemeEffect::Navigate(screen) => ThemeEffect::Navigate(host_screen(screen)),
        ThemeEffect::NavigateNoFade(screen) => ThemeEffect::NavigateNoFade(host_screen(screen)),
        ThemeEffect::LeaveEvaluation => ThemeEffect::LeaveEvaluation,
        ThemeEffect::Exit => ThemeEffect::Exit,
        ThemeEffect::Shutdown => ThemeEffect::Shutdown,
        ThemeEffect::Runtime(cue) => ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(
            AudioRequest::PlayUiCue(cue),
        )),
    }
}

fn openitg_screen(screen: Screen) -> OpenItgScreen {
    match screen {
        Screen::Init => OpenItgScreen::Attract,
        Screen::SelectProfile | Screen::SelectPlayMode | Screen::SelectStyle => OpenItgScreen::Play,
        Screen::Options => OpenItgScreen::Options,
        Screen::ContentBrowser => OpenItgScreen::Content,
        Screen::Sandbox => OpenItgScreen::Sandbox,
        _ => OpenItgScreen::Menu,
    }
}

fn openitg_transition(
    transition: &TransitionState,
) -> deadsync_theme::TransitionState<OpenItgScreen> {
    use deadsync_theme::TransitionState::*;
    match *transition {
        Idle => Idle,
        FadingIn { elapsed, duration } => FadingIn { elapsed, duration },
        ActorsFadeIn { elapsed, duration } => ActorsFadeIn { elapsed, duration },
        FadingOut {
            elapsed,
            duration,
            target,
        } => FadingOut {
            elapsed,
            duration,
            target: openitg_screen(target),
        },
        ActorsFadeOut {
            elapsed,
            duration,
            target,
        } => ActorsFadeOut {
            elapsed,
            duration,
            target: openitg_screen(target),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::{InputSource, MAX_PLAYERS};
    use deadsync_input::VirtualAction;
    use std::time::Instant;

    #[test]
    fn both_title_menus_enter_the_same_session_from_p2() {
        for flag in [ThemeFlag::SimplyLove, ThemeFlag::OpenItg] {
            let mut menu = InstalledMenu::new(flag, 2);
            let mut session = SessionState::new(0, [0; MAX_PLAYERS]);
            let now = Instant::now();
            let start = InputEvent::new(
                VirtualAction::p2_start,
                0,
                true,
                InputSource::Keyboard,
                now,
                0,
                now,
                now,
            );
            let mut effects = Vec::new();
            menu.handle_input(&mut session, &start)
                .append_to(&mut effects);
            assert!(effects.iter().any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::SelectProfile))));
            assert_eq!(session.menu_start_side, deadsync_profile::PlayerSide::P2);
        }
    }
}
