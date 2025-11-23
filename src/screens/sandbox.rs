use crate::act;
use crate::core::space::*;
use crate::screens::{Screen, ScreenAction};
use crate::core::input::{VirtualAction, InputEvent};
use crate::ui::actors::Actor;
// Keyboard input is handled centrally via the virtual dispatcher in app.rs
use std::collections::VecDeque;
use std::time::{Instant};
use crate::core::input::PadEvent;
use winit::event::{KeyEvent, ElementState};

/* ---------------------------- constants ---------------------------- */

const INPUT_LOG_MAX_ITEMS: usize = 10;
const INPUT_LOG_FADE_DURATION: f32 = 5.0; // seconds to fade out

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

pub struct State {
    pub elapsed: f32,
    pub last_inputs: VecDeque<(String, Instant)>,
}

pub fn init() -> State {
    State {
        elapsed: 0.0,
        last_inputs: VecDeque::with_capacity(INPUT_LOG_MAX_ITEMS),
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

pub fn update(state: &mut State, dt: f32) {
    state.elapsed += dt;
    state.last_inputs.retain(|(_, timestamp)| timestamp.elapsed().as_secs_f32() < INPUT_LOG_FADE_DURATION);
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0): z(1100):
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

pub fn handle_raw_key_event(state: &mut State, key_event: &KeyEvent) -> ScreenAction {
    if key_event.state == ElementState::Pressed && !key_event.repeat
        && let winit::keyboard::PhysicalKey::Code(code) = key_event.physical_key {
            // F4 or Escape navigates back to Menu
            if matches!(code, winit::keyboard::KeyCode::F4 | winit::keyboard::KeyCode::Escape) {
                return ScreenAction::Navigate(Screen::Menu);
            }
            let key_str = format!("Keyboard: KeyCode::{:?}", code);
            state.last_inputs.push_front((key_str, Instant::now()));
            if state.last_inputs.len() > INPUT_LOG_MAX_ITEMS {
                state.last_inputs.pop_back();
            }
        }
    ScreenAction::None
}

pub fn handle_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    // We only care about press events for the log
    let pressed = match pad_event {
        PadEvent::Dir { pressed, .. } => *pressed,
        PadEvent::Button { pressed, .. } => *pressed,
        PadEvent::Face { pressed, .. } => *pressed,
    };

    if pressed {
        // Include controller ID in the display for clarity when multiple are connected.
        let pad_str = match pad_event {
            PadEvent::Dir { id, dir, pressed } => {
                format!("Gamepad {}: Dir {{ dir: {:?}, pressed: {} }}", usize::from(*id), dir, pressed)
            }
            PadEvent::Button { id, btn, pressed } => {
                format!("Gamepad {}: Button {{ btn: {:?}, pressed: {} }}", usize::from(*id), btn, pressed)
            }
            PadEvent::Face { id, btn, pressed } => {
                format!("Gamepad {}: Face {{ btn: {:?}, pressed: {} }}", usize::from(*id), btn, pressed)
            }
        };
        state.last_inputs.push_front((pad_str, Instant::now()));
        if state.last_inputs.len() > INPUT_LOG_MAX_ITEMS {
            state.last_inputs.pop_back();
        }
    }
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(10 + INPUT_LOG_MAX_ITEMS);

    actors.push(act!(text:
        align(0.5, 0.0): xy(screen_center_x(), 20.0):
        zoomtoheight(15.0): font("wendy"): settext("Actor System & Input Sandbox"): horizalign(center)
    ));
    actors.push(act!(text:
        align(0.5, 0.0): xy(screen_center_x(), 60.0):
        zoomtoheight(15.0): font("miso"): settext("Press ESC or F4 to return to Menu"): horizalign(center)
    ));

    // Display the input log
    let start_y = 100.0;
    let line_height = 20.0;
    for (i, (text, timestamp)) in state.last_inputs.iter().enumerate() {
        let age = timestamp.elapsed().as_secs_f32();
        let alpha = 1.0 - (age / INPUT_LOG_FADE_DURATION).clamp(0.0, 1.0);

        actors.push(act!(text:
            font("miso"):
            settext(text.clone()):
            align(0.5, 0.0):
            xy(screen_center_x(), start_y + (i as f32 * line_height)):
            zoom(0.8):
            horizalign(center):
            diffusealpha(alpha):
            z(200)
        ));
    }

    actors
}

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ScreenAction {
    if ev.pressed
        && ev.action == VirtualAction::p1_back { return ScreenAction::Navigate(Screen::Menu) }
    ScreenAction::None
}
