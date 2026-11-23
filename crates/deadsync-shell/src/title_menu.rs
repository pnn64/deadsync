//! Connects the installed title presentation to the shared session flow.

use crate::{ActiveTheme, ThemeScreen as Screen, TransitionState};
use deadlib_platform::input::RawKeyboardEvent;
use deadlib_present::actors::Actor;
use deadsync_config::theme::ThemeFlag;
use deadsync_input::InputEvent;
use deadsync_theme::main_menu::{MainMenuFrame, MainMenuPhase, MenuBackgroundMovie, ThemeMainMenu};
use deadsync_theme_ddr_extreme::PulseCircuitTheme;
use deadsync_theme_openitg::OpenItgTheme;
use deadsync_theme_simply_love::{
    SimplyLoveEffect, SimplyLoveInputResult, views::SimplyLoveVisualPolicyView,
};

type Policy = SimplyLoveVisualPolicyView;
type Runtime = crate::main_menu::MainMenuRuntimeView;

pub(crate) struct InstalledMenu {
    flag: ThemeFlag,
    title: Box<dyn TitlePresentation>,
}

impl InstalledMenu {
    pub(crate) fn new(flag: ThemeFlag, color_index: i32) -> Self {
        let title = match flag {
            ThemeFlag::SimplyLove => mount::<ActiveTheme>(color_index, |policy| policy),
            ThemeFlag::OpenItg => mount::<OpenItgTheme>(color_index, |_| ()),
            ThemeFlag::PulseCircuit => mount::<PulseCircuitTheme>(color_index, |_| ()),
        };
        Self { flag, title }
    }

    pub(crate) const fn theme_flag(&self) -> ThemeFlag {
        self.flag
    }

    pub(crate) fn background_movie(&self) -> Option<MenuBackgroundMovie> {
        self.title.background_movie()
    }

    pub(crate) fn entry_transition(&self) -> (Vec<Actor>, f32) {
        self.title.entry_transition()
    }

    pub(crate) fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32) {
        self.title.exit_transition(target)
    }

    pub(crate) fn actor_entry(&self) -> TransitionState {
        self.title
            .entry_duration()
            .map_or(TransitionState::Idle, |duration| {
                TransitionState::ActorsFadeIn {
                    elapsed: 0.0,
                    duration,
                }
            })
    }

    pub(crate) fn actor_exit(&self, target: Screen) -> TransitionState {
        TransitionState::ActorsFadeOut {
            elapsed: 0.0,
            duration: self.title.exit_duration(),
            target,
        }
    }

    pub(crate) fn color_index(&self) -> i32 {
        self.title.color_index()
    }

    pub(crate) fn set_color_index(&mut self, color_index: i32) {
        self.title.set_color_index(color_index);
    }

    pub(crate) fn reset(&mut self) {
        self.title.reset();
    }

    pub(crate) fn sync_runtime(&mut self, view: Runtime) {
        self.title.sync_runtime(view);
    }

    pub(crate) fn update(&mut self, dt: f32, phase: MainMenuPhase) -> SimplyLoveEffect {
        self.title.update(dt, phase)
    }

    pub(crate) fn handle_input(&mut self, input: &InputEvent) -> SimplyLoveEffect {
        self.title.input(input)
    }

    pub(crate) fn handle_raw_key(&mut self, key: &RawKeyboardEvent) -> SimplyLoveInputResult {
        self.title.raw_key(key)
    }

    pub(crate) fn push_actors(
        &self,
        actors: &mut Vec<Actor>,
        frame: MainMenuFrame<'_, Screen, Policy>,
    ) {
        self.title.push_actors(actors, frame);
    }

    pub(crate) fn push_exit_overlay(
        &self,
        actors: &mut Vec<Actor>,
        transition: &TransitionState,
        policy: Policy,
    ) {
        self.title.push_exit_overlay(actors, transition, policy);
    }
}

trait TitlePresentation {
    fn background_movie(&self) -> Option<MenuBackgroundMovie>;
    fn entry_transition(&self) -> (Vec<Actor>, f32);
    fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32);
    fn entry_duration(&self) -> Option<f32>;
    fn exit_duration(&self) -> f32;
    fn color_index(&self) -> i32;
    fn set_color_index(&mut self, index: i32);
    fn reset(&mut self);
    fn sync_runtime(&mut self, view: Runtime);
    fn update(&mut self, dt: f32, phase: MainMenuPhase) -> SimplyLoveEffect;
    fn input(&mut self, input: &InputEvent) -> SimplyLoveEffect;
    fn raw_key(&mut self, key: &RawKeyboardEvent) -> SimplyLoveInputResult;
    fn push_actors(&self, actors: &mut Vec<Actor>, frame: MainMenuFrame<'_, Screen, Policy>);
    fn push_exit_overlay(
        &self,
        actors: &mut Vec<Actor>,
        transition: &TransitionState,
        policy: Policy,
    );
}

struct MountedTitle<T: ThemeMainMenu> {
    state: T::MenuState,
    policy: fn(Policy) -> T::MenuVisualPolicy,
}

fn mount<T>(index: i32, policy: fn(Policy) -> T::MenuVisualPolicy) -> Box<dyn TitlePresentation>
where
    T: ThemeMainMenu<
            Screen = Screen,
            Actor = Actor,
            MenuRuntimeView = Runtime,
            MenuInput = InputEvent,
            MenuRawKey = RawKeyboardEvent,
        > + 'static,
    T::MenuState: 'static,
    for<'a> T::EntryContext<'a>: Default,
{
    Box::new(MountedTitle::<T> {
        state: T::init_menu(index),
        policy,
    })
}

impl<T> TitlePresentation for MountedTitle<T>
where
    T: ThemeMainMenu<
            Screen = Screen,
            Actor = Actor,
            MenuRuntimeView = Runtime,
            MenuInput = InputEvent,
            MenuRawKey = RawKeyboardEvent,
        >,
    for<'a> T::EntryContext<'a>: Default,
{
    fn background_movie(&self) -> Option<MenuBackgroundMovie> {
        T::MENU_BACKGROUND_MOVIE
    }

    fn entry_transition(&self) -> (Vec<Actor>, f32) {
        T::in_transition(Screen::Menu, Default::default())
    }

    fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32) {
        T::out_transition(Screen::Menu, target)
    }

    fn entry_duration(&self) -> Option<f32> {
        T::actor_entry_duration(Screen::Menu)
    }

    fn exit_duration(&self) -> f32 {
        T::actor_exit_duration(Screen::Menu)
    }

    fn color_index(&self) -> i32 {
        T::menu_color_index(&self.state)
    }

    fn set_color_index(&mut self, index: i32) {
        T::set_menu_color_index(&mut self.state, index);
    }

    fn reset(&mut self) {
        T::reset_menu(&mut self.state);
    }

    fn sync_runtime(&mut self, view: Runtime) {
        T::sync_menu_runtime(&mut self.state, view);
    }

    fn update(&mut self, dt: f32, phase: MainMenuPhase) -> SimplyLoveEffect {
        T::update_menu(&mut self.state, dt, phase)
    }

    fn input(&mut self, input: &InputEvent) -> SimplyLoveEffect {
        T::handle_menu_input(&mut self.state, input)
    }

    fn raw_key(&mut self, key: &RawKeyboardEvent) -> SimplyLoveInputResult {
        T::handle_menu_raw_key(&mut self.state, key)
    }

    fn push_actors(&self, actors: &mut Vec<Actor>, frame: MainMenuFrame<'_, Screen, Policy>) {
        T::push_menu_actors(
            actors,
            &self.state,
            MainMenuFrame {
                update_banner_tag: frame.update_banner_tag,
                background_key: frame.background_key,
                alpha_multiplier: frame.alpha_multiplier,
                transition: frame.transition,
                visual_policy: (self.policy)(frame.visual_policy),
            },
        );
    }

    fn push_exit_overlay(
        &self,
        actors: &mut Vec<Actor>,
        transition: &TransitionState,
        policy: Policy,
    ) {
        T::push_menu_exit_overlay(actors, &self.state, transition, (self.policy)(policy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use deadsync_input::VirtualAction;
    use deadsync_theme::{ThemeEffect, UiCue};
    use std::time::Instant;

    #[test]
    fn installed_title_menus_request_profile_selection_from_p2() {
        for flag in ThemeFlag::CHOICES {
            let mut menu = InstalledMenu::new(flag, 2);
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
            menu.handle_input(&start).append_to(&mut effects);
            assert!(matches!(
                effects.as_slice(),
                [
                    ThemeEffect::UiCue(UiCue::Confirm),
                    ThemeEffect::Navigate(Screen::SelectProfile)
                ]
            ));
        }
    }
}
