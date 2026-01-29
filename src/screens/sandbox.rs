use crate::act;
use crate::core::input::{GpSystemEvent, InputEvent, PadEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_height, screen_width};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
// Keyboard input is handled centrally via the virtual dispatcher in app.rs
use std::collections::VecDeque;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};

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
    state
        .last_inputs
        .retain(|(_, timestamp)| timestamp.elapsed().as_secs_f32() < INPUT_LOG_FADE_DURATION);
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
    if key_event.state == ElementState::Pressed
        && !key_event.repeat
        && let winit::keyboard::PhysicalKey::Code(code) = key_event.physical_key
    {
        // F4 or Escape navigates back to Menu
        if matches!(
            code,
            winit::keyboard::KeyCode::F4 | winit::keyboard::KeyCode::Escape
        ) {
            return ScreenAction::Navigate(Screen::Menu);
        }
        let key_str = format!("Keyboard: KeyCode::{code:?}");
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
        PadEvent::RawButton { pressed, .. } => *pressed,
        PadEvent::RawAxis { .. } => false,
    };

    if pressed {
        // Include controller ID in the display for clarity when multiple are connected.
        let pad_str = match pad_event {
            PadEvent::Dir {
                id, dir, pressed, ..
            } => {
                format!(
                    "Gamepad {}: Dir {{ dir: {:?}, pressed: {} }}",
                    usize::from(*id),
                    dir,
                    pressed
                )
            }
            PadEvent::RawButton {
                id,
                code,
                uuid,
                value,
                pressed,
                ..
            } => {
                let dev = usize::from(*id);
                let code_u32 = code.into_u32();
                let uuid_hex: String = uuid.iter().map(|b| format!("{b:02X}")).collect();
                format!(
                    "Gamepad {dev} [uuid={uuid_hex}]: RAW BTN {{ PadCode[0x{code_u32:08X}], value: {value:.3}, pressed: {pressed} }}",
                )
            }
            PadEvent::RawAxis {
                id,
                code,
                uuid,
                value,
                ..
            } => {
                let dev = usize::from(*id);
                let code_u32 = code.into_u32();
                let uuid_hex: String = uuid.iter().map(|b| format!("{b:02X}")).collect();
                format!(
                    "Gamepad {dev} [uuid={uuid_hex}]: RAW AXIS {{ PadCode[0x{code_u32:08X}], value: {value:.3} }}",
                )
            }
        };
        state.last_inputs.push_front((pad_str, Instant::now()));
        if state.last_inputs.len() > INPUT_LOG_MAX_ITEMS {
            state.last_inputs.pop_back();
        }
    }
}

pub fn handle_gamepad_system_event(state: &mut State, ev: &GpSystemEvent) {
    let now = Instant::now();
    let msg = match ev {
        GpSystemEvent::StartupComplete => "[SYS] Gamepad startup complete".to_string(),
        GpSystemEvent::Connected {
            name,
            id,
            vendor_id,
            product_id,
            backend,
            initial,
        } => {
            let dev = usize::from(*id);
            let vid = vendor_id
                .map(|v| format!("0x{v:04X}"))
                .unwrap_or_else(|| "n/a".to_string());
            let pid = product_id
                .map(|p| format!("0x{p:04X}"))
                .unwrap_or_else(|| "n/a".to_string());
            format!(
                "[SYS] Gamepad {dev} CONNECTED: \"{name}\" vid={vid} pid={pid} backend={backend:?} initial={initial}",
            )
        }
        GpSystemEvent::Disconnected {
            name, id, initial, ..
        } => {
            let dev = usize::from(*id);
            format!("[SYS] Gamepad {dev} DISCONNECTED: \"{name}\" initial={initial}")
        }
    };
    state.last_inputs.push_front((msg, now));
    if state.last_inputs.len() > INPUT_LOG_MAX_ITEMS {
        state.last_inputs.pop_back();
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
            xy(screen_center_x(), (i as f32).mul_add(line_height, start_y)):
            zoom(0.8):
            horizalign(center):
            diffusealpha(alpha):
            z(200)
        ));
    }

    actors
}

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ScreenAction {
    if ev.pressed && ev.action == VirtualAction::p1_back {
        return ScreenAction::Navigate(Screen::Menu);
    }
    ScreenAction::None
}
