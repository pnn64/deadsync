use crate::act;
use crate::core::input::{InputEvent, InputSource, PadEvent, VirtualAction, with_keymap};
use crate::core::space::{screen_height, screen_width};
use crate::screens::components::{heart_bg, test_input};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use winit::event::{ElementState, KeyEvent};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const BACK_HOLD_SECONDS: f32 = 0.33;

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    test_input: test_input::State,
    back_hold_active: bool,
    back_hold_secs: f32,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        test_input: test_input::State::default(),
        back_hold_active: false,
        back_hold_secs: 0.0,
    }
}

/* ------------------------------- update ------------------------------- */

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    if !state.back_hold_active {
        state.back_hold_secs = 0.0;
        return None;
    }
    state.back_hold_secs += dt;
    if state.back_hold_secs < BACK_HOLD_SECONDS {
        return None;
    }
    state.back_hold_active = false;
    state.back_hold_secs = 0.0;
    Some(ScreenAction::Navigate(Screen::Options))
}

/* ----------------------------- transitions ----------------------------- */

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

/* ------------------------------- input -------------------------------- */

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if ev.pressed
        && ev.source == InputSource::Gamepad
        && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back)
    {
        return ScreenAction::Navigate(Screen::Options);
    }
    test_input::apply_virtual_input(&mut state.test_input, ev);
    ScreenAction::None
}

pub fn handle_raw_key_event(state: &mut State, key_event: &KeyEvent) -> ScreenAction {
    test_input::apply_raw_key_event(&mut state.test_input, key_event);
    if key_event.state == ElementState::Pressed && key_event.repeat {
        return ScreenAction::None;
    }
    let is_back = with_keymap(|km| {
        km.actions_for_key_event(key_event)
            .into_iter()
            .any(|(act, _)| matches!(act, VirtualAction::p1_back | VirtualAction::p2_back))
    });
    if !is_back {
        return ScreenAction::None;
    }
    if key_event.state == ElementState::Pressed {
        if !state.back_hold_active {
            state.back_hold_active = true;
            state.back_hold_secs = 0.0;
        }
    } else {
        state.back_hold_active = false;
        state.back_hold_secs = 0.0;
    }
    ScreenAction::None
}

/// Raw pad events are used to approximate Simply Love's "unmapped" device list.
pub fn handle_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    test_input::apply_raw_pad_event(&mut state.test_input, pad_event);
}

/* ------------------------------- drawing ------------------------------- */

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(56);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    actors.extend(test_input::build_test_input_screen_content(
        &state.test_input,
    ));
    actors
}
