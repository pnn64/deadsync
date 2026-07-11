use crate::screens::components::shared::{test_input, transitions, visual_style_bg};
use crate::screens::{Screen, ScreenAction};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadsync_core::input::InputSource;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, PadEvent, VirtualAction, with_keymap};
pub use deadsync_screens::input::{
    MenuLrChordTracker, ThreeKeyMenuAction, advance_hold_repeat, dedicated_blocks_arrow,
    menu_lr_both_held, menu_lr_side, reset_hold_repeat, track_menu_lr_chord,
};
/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const BACK_HOLD_SECONDS: f32 = 0.33;

#[inline(always)]
pub fn dedicated_three_key_nav_enabled() -> bool {
    let cfg = crate::config::get();
    cfg.three_key_navigation && cfg.only_dedicated_menu_buttons
}

pub fn three_key_menu_action(
    chord: &mut MenuLrChordTracker,
    ev: &InputEvent,
) -> Option<(deadsync_profile::PlayerSide, ThreeKeyMenuAction)> {
    deadsync_screens::input::three_key_menu_action(chord, ev, dedicated_three_key_nav_enabled())
}

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    test_input: test_input::State,
    back_hold_active: bool,
    back_hold_secs: f32,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
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
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
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

pub fn handle_raw_key_event(state: &mut State, key_event: &RawKeyboardEvent) -> ScreenAction {
    test_input::apply_raw_key_event(&mut state.test_input, key_event);
    if key_event.pressed && key_event.repeat {
        return ScreenAction::None;
    }
    let is_back = with_keymap(|km| {
        km.raw_key_event_has_action(key_event, |action| {
            matches!(action, VirtualAction::p1_back | VirtualAction::p2_back)
        })
    });
    if !is_back {
        return ScreenAction::None;
    }
    if key_event.pressed {
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

pub fn push_actors(actors: &mut Vec<Actor>, state: &State) {
    actors.reserve(56);

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
        },
    );

    actors.extend(test_input::build_test_input_screen_content(
        &state.test_input,
        state.active_color_index,
    ));
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(56);
    push_actors(&mut actors, state);
    actors
}
