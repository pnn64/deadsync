use gilrs::{Axis, Button, Event, EventType, GamepadId, Gilrs};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadDir { Up, Down, Left, Right }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadButton { Confirm, Back, F7 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceBtn { SouthA, EastB, WestX, NorthY }

#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    Dir { dir: PadDir, pressed: bool },
    Button { btn: PadButton, pressed: bool },
    Face { btn: FaceBtn, pressed: bool },
}

#[derive(Debug)]
pub enum GpSystemEvent {
    Connected { name: String, id: GamepadId },
    Disconnected { name: String, id: GamepadId },
}


#[derive(Default, Clone, Copy)]
pub struct GamepadState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,

    dpad_up: bool,
    dpad_down: bool,
    dpad_left: bool,
    dpad_right: bool,

    lx: f32,
    ly: f32,
}

#[inline(always)]
const fn deadzone() -> f32 { 0.35 }

#[inline(always)]
fn stick_to_dirs(x: f32, y: f32) -> (bool, bool, bool, bool) {
    let dz = deadzone();
    let left  = x <= -dz;
    let right = x >=  dz;
    let up    = y <= -dz;
    let down  = y >=  dz;
    (up, down, left, right)
}

/// Poll gilrs, keep a single active pad, and output high-level events.
/// No winit KeyEvent construction needed.
pub fn poll_and_collect(
    gilrs: &mut Gilrs,
    active_id: &mut Option<GamepadId>,
    state: &mut GamepadState,
    want_f7: bool,
) -> (Vec<PadEvent>, Vec<GpSystemEvent>) {
    let mut out = Vec::with_capacity(16);
    let mut sys_out = Vec::with_capacity(2);

    while let Some(Event { id, event, .. }) = gilrs.next_event() {
        // --- System Events (Connect/Disconnect) ---
        // These are processed for ANY gamepad, not just the active one.
        match event {
            EventType::Connected => {
                let name = gilrs.gamepad(id).name().to_string();
                sys_out.push(GpSystemEvent::Connected { name, id });
                if active_id.is_none() {
                    *active_id = Some(id);
                }
                continue; // Don't process this event as an input.
            }
            EventType::Disconnected => {
                let name = gilrs.gamepad(id).name().to_string();
                sys_out.push(GpSystemEvent::Disconnected { name, id });
                if Some(id) == *active_id {
                    *active_id = None;
                    // Release all buttons for the disconnected pad.
                    if state.up    { out.push(PadEvent::Dir { dir: PadDir::Up,    pressed: false }); }
                    if state.down  { out.push(PadEvent::Dir { dir: PadDir::Down,  pressed: false }); }
                    if state.left  { out.push(PadEvent::Dir { dir: PadDir::Left,  pressed: false }); }
                    if state.right { out.push(PadEvent::Dir { dir: PadDir::Right, pressed: false }); }
                    *state = GamepadState::default();
                }
                continue; // Don't process this event as an input.
            }
            _ => {}
        }
        
        // --- Input Events (Buttons/Axes) ---
        // From here on, we only care about the active gamepad.
        // If no pad is active, the first one to send an input event becomes active.
        if active_id.is_none() {
            *active_id = Some(id);
        }

        // Ignore input events from non-active gamepads.
        if Some(id) != *active_id {
            continue;
        }

        match event {
            EventType::ButtonPressed(btn, _) => {
                match btn {
                    // Face buttons → Face events
                    Button::South => out.push(PadEvent::Face { btn: FaceBtn::SouthA, pressed: true }),
                    Button::East  => out.push(PadEvent::Face { btn: FaceBtn::EastB,  pressed: true }),
                    Button::West  => out.push(PadEvent::Face { btn: FaceBtn::WestX,  pressed: true }),
                    Button::North => {
                        out.push(PadEvent::Face { btn: FaceBtn::NorthY, pressed: true });
                        if want_f7 {
                            out.push(PadEvent::Button { btn: PadButton::F7, pressed: true });
                        }
                    }

                    // Confirm = Start ONLY (so A can be used as Down lane)
                    Button::Start => out.push(PadEvent::Button { btn: PadButton::Confirm, pressed: true }),

                    // Back = View/Select (NOT B)
                    Button::Select => out.push(PadEvent::Button { btn: PadButton::Back, pressed: true }),

                    // D-Pad raw state (edges emitted below)
                    Button::DPadUp    => { state.dpad_up    = true; }
                    Button::DPadDown  => { state.dpad_down  = true; }
                    Button::DPadLeft  => { state.dpad_left  = true; }
                    Button::DPadRight => { state.dpad_right = true; }
                    _ => {}
                }
            }

            EventType::ButtonReleased(btn, _) => {
                match btn {
                    Button::South => out.push(PadEvent::Face { btn: FaceBtn::SouthA, pressed: false }),
                    Button::East  => out.push(PadEvent::Face { btn: FaceBtn::EastB,  pressed: false }),
                    Button::West  => out.push(PadEvent::Face { btn: FaceBtn::WestX,  pressed: false }),
                    Button::North => {
                        out.push(PadEvent::Face { btn: FaceBtn::NorthY, pressed: false });
                        if want_f7 {
                            out.push(PadEvent::Button { btn: PadButton::F7, pressed: false });
                        }
                    }

                    // Confirm = Start ONLY
                    Button::Start => out.push(PadEvent::Button { btn: PadButton::Confirm, pressed: false }),
                    // Back = View/Select
                    Button::Select => out.push(PadEvent::Button { btn: PadButton::Back, pressed: false }),

                    Button::DPadUp    => { state.dpad_up    = false; }
                    Button::DPadDown  => { state.dpad_down  = false; }
                    Button::DPadLeft  => { state.dpad_left  = false; }
                    Button::DPadRight => { state.dpad_right = false; }
                    _ => {}
                }
            }

            EventType::AxisChanged(axis, value, _) => {
                match axis {
                    Axis::LeftStickX => state.lx = value,
                    Axis::LeftStickY => state.ly = value,
                    _ => {}
                }
            }

            _ => {}
        }

        // Emit edge transitions for combined D-Pad OR left stick.
        let (su, sd, sl, sr) = stick_to_dirs(state.lx, state.ly);
        let want_up    = state.dpad_up    || su;
        let want_down  = state.dpad_down  || sd;
        let want_left  = state.dpad_left  || sl;
        let want_right = state.dpad_right || sr;

        if want_up != state.up {
            out.push(PadEvent::Dir { dir: PadDir::Up, pressed: want_up });
            state.up = want_up;
        }
        if want_down != state.down {
            out.push(PadEvent::Dir { dir: PadDir::Down, pressed: want_down });
            state.down = want_down;
        }
        if want_left != state.left {
            out.push(PadEvent::Dir { dir: PadDir::Left, pressed: want_left });
            state.left = want_left;
        }
        if want_right != state.right {
            out.push(PadEvent::Dir { dir: PadDir::Right, pressed: want_right });
            state.right = want_right;
        }
    }

    (out, sys_out)
}

#[inline(always)]
pub fn try_init() -> Option<Gilrs> { Gilrs::new().ok() }
