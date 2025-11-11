use std::time::Instant;
use std::collections::HashMap;

use once_cell::sync::Lazy;
use std::sync::Mutex;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

// Gamepad (gilrs)
use gilrs::{Axis, Button, Event, EventType, GamepadId, Gilrs};

/* ------------------------ Gamepad types + poll ------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadDir { Up, Down, Left, Right }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadButton { Confirm, Back, F7 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceBtn { SouthA, EastB, WestX, NorthY }

#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    Dir { id: GamepadId, dir: PadDir, pressed: bool },
    Button { id: GamepadId, btn: PadButton, pressed: bool },
    Face { id: GamepadId, btn: FaceBtn, pressed: bool },
}

#[derive(Debug)]
pub enum GpSystemEvent {
    Connected { name: String, id: GamepadId },
    Disconnected { name: String, id: GamepadId },
}

#[derive(Default, Clone, Copy)]
struct PerPadState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,

    dpad_up: bool,
    dpad_down: bool,
    dpad_left: bool,
    dpad_right: bool,

    lx: f32,
    ly: f32,
}

#[derive(Default, Clone)]
pub struct GamepadState {
    states: HashMap<GamepadId, PerPadState>,
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
#[inline(always)]
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
                if active_id.is_none() { *active_id = Some(id); }
                continue; // Don't process this event as an input.
            }
            EventType::Disconnected => {
                let name = gilrs.gamepad(id).name().to_string();
                sys_out.push(GpSystemEvent::Disconnected { name, id });
                // Release any buttons/dirs for this device and drop its state.
                if let Some(ps) = state.states.remove(&id) {
                    if ps.up    { out.push(PadEvent::Dir { id, dir: PadDir::Up,    pressed: false }); }
                    if ps.down  { out.push(PadEvent::Dir { id, dir: PadDir::Down,  pressed: false }); }
                    if ps.left  { out.push(PadEvent::Dir { id, dir: PadDir::Left,  pressed: false }); }
                    if ps.right { out.push(PadEvent::Dir { id, dir: PadDir::Right, pressed: false }); }
                }
                if Some(id) == *active_id { *active_id = None; }
                continue; // Don't process this event as an input.
            }
            _ => {}
        }

        // --- Input Events (Buttons/Axes) ---
        // Multi-device: do not filter by active_id. Maintain per-device state.
        if active_id.is_none() { *active_id = Some(id); }

        let ps = state.states.entry(id).or_insert_with(PerPadState::default);

        match event {
            EventType::ButtonPressed(btn, _) => {
                match btn {
                    // Face buttons → Face events
                    Button::South => out.push(PadEvent::Face { id, btn: FaceBtn::SouthA, pressed: true }),
                    Button::East  => out.push(PadEvent::Face { id, btn: FaceBtn::EastB,  pressed: true }),
                    Button::West  => out.push(PadEvent::Face { id, btn: FaceBtn::WestX,  pressed: true }),
                    Button::North => {
                        out.push(PadEvent::Face { id, btn: FaceBtn::NorthY, pressed: true });
                        if want_f7 { out.push(PadEvent::Button { id, btn: PadButton::F7, pressed: true }); }
                    }

                    // Confirm = Start ONLY (so A can be used as Down lane)
                    Button::Start => out.push(PadEvent::Button { id, btn: PadButton::Confirm, pressed: true }),

                    // Back = View/Select (NOT B)
                    Button::Select => out.push(PadEvent::Button { id, btn: PadButton::Back, pressed: true }),

                    // D-Pad raw state (edges emitted below)
                    Button::DPadUp    => { ps.dpad_up    = true; }
                    Button::DPadDown  => { ps.dpad_down  = true; }
                    Button::DPadLeft  => { ps.dpad_left  = true; }
                    Button::DPadRight => { ps.dpad_right = true; }
                    _ => {}
                }
            }

            EventType::ButtonReleased(btn, _) => {
                match btn {
                    Button::South => out.push(PadEvent::Face { id, btn: FaceBtn::SouthA, pressed: false }),
                    Button::East  => out.push(PadEvent::Face { id, btn: FaceBtn::EastB,  pressed: false }),
                    Button::West  => out.push(PadEvent::Face { id, btn: FaceBtn::WestX,  pressed: false }),
                    Button::North => {
                        out.push(PadEvent::Face { id, btn: FaceBtn::NorthY, pressed: false });
                        if want_f7 { out.push(PadEvent::Button { id, btn: PadButton::F7, pressed: false }); }
                    }

                    // Confirm = Start ONLY
                    Button::Start => out.push(PadEvent::Button { id, btn: PadButton::Confirm, pressed: false }),
                    // Back = View/Select
                    Button::Select => out.push(PadEvent::Button { id, btn: PadButton::Back, pressed: false }),

                    Button::DPadUp    => { ps.dpad_up    = false; }
                    Button::DPadDown  => { ps.dpad_down  = false; }
                    Button::DPadLeft  => { ps.dpad_left  = false; }
                    Button::DPadRight => { ps.dpad_right = false; }
                    _ => {}
                }
            }

            EventType::AxisChanged(axis, value, _) => {
                match axis {
                    Axis::LeftStickX => ps.lx = value,
                    Axis::LeftStickY => ps.ly = value,
                    _ => {}
                }
            }

            _ => {}
        }

        // Emit edge transitions for combined D-Pad OR left stick (per-device).
        let (su, sd, sl, sr) = stick_to_dirs(ps.lx, ps.ly);
        let want_up    = ps.dpad_up    || su;
        let want_down  = ps.dpad_down  || sd;
        let want_left  = ps.dpad_left  || sl;
        let want_right = ps.dpad_right || sr;

        if want_up != ps.up {
            out.push(PadEvent::Dir { id, dir: PadDir::Up, pressed: want_up });
            ps.up = want_up;
        }
        if want_down != ps.down {
            out.push(PadEvent::Dir { id, dir: PadDir::Down, pressed: want_down });
            ps.down = want_down;
        }
        if want_left != ps.left {
            out.push(PadEvent::Dir { id, dir: PadDir::Left, pressed: want_left });
            ps.left = want_left;
        }
        if want_right != ps.right {
            out.push(PadEvent::Dir { id, dir: PadDir::Right, pressed: want_right });
            ps.right = want_right;
        }
    }

    (out, sys_out)
}

#[inline(always)]
pub fn try_init() -> Option<Gilrs> { Gilrs::new().ok() }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Lane {
    Left = 0,
    Down = 1,
    Up = 2,
    Right = 3,
}

impl Lane {
    #[inline(always)]
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Keyboard,
    Gamepad,
}

#[derive(Clone, Copy, Debug)]
pub struct InputEdge {
    pub lane: Lane,
    pub pressed: bool,
    pub source: InputSource,
    pub timestamp: Instant,
}

// Removed legacy per-key state helpers in favor of virtual action mapping.

/* ------------------------ Virtual Keymap system ------------------------ */

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VirtualAction {
    p1_up,
    p1_down,
    p1_left,
    p1_right,
    p1_start,
    p1_back,
    p1_menu_up,
    p1_menu_down,
    p1_menu_left,
    p1_menu_right,
    p1_select,
    p1_operator,
    p1_restart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputBinding {
    Key(KeyCode),
    PadDir(PadDir),
    PadButton(PadButton),
    Face(FaceBtn),
    PadDirOn { device: usize, dir: PadDir },
    PadButtonOn { device: usize, btn: PadButton },
    FaceOn { device: usize, btn: FaceBtn },
}

#[derive(Clone, Debug, Default)]
pub struct Keymap { map: HashMap<VirtualAction, Vec<InputBinding>> }

static KEYMAP: Lazy<Mutex<Keymap>> = Lazy::new(|| Mutex::new(Keymap::default()));

#[inline(always)]
pub fn get_keymap() -> Keymap { KEYMAP.lock().unwrap().clone() }

#[inline(always)]
pub fn set_keymap(new_map: Keymap) { *KEYMAP.lock().unwrap() = new_map; }

// Defaults are provided by config.rs; keep this module free of config.

impl Keymap {
    #[inline(always)]
    pub fn bind(&mut self, action: VirtualAction, inputs: &[InputBinding]) {
        self.map.insert(action, inputs.to_vec());
    }

    #[inline(always)]
    pub fn actions_for_key_event(&self, ev: &KeyEvent) -> Vec<(VirtualAction, bool)> {
        let mut out = Vec::with_capacity(2);
        let pressed = ev.state == ElementState::Pressed;
        let PhysicalKey::Code(code) = ev.physical_key else { return out; };
        for (act, binds) in &self.map {
            for b in binds { if *b == InputBinding::Key(code) { out.push((*act, pressed)); break; } }
        }
        out
    }

    #[inline(always)]
    pub fn actions_for_pad_event(&self, ev: &PadEvent) -> Vec<(VirtualAction, bool)> {
        let mut out = Vec::with_capacity(2);
        match *ev {
            PadEvent::Dir { id, dir, pressed } => {
                let dev = usize::from(id);
                for (act, binds) in &self.map {
                    for b in binds {
                        match *b {
                            InputBinding::PadDir(d) if d == dir => { out.push((*act, pressed)); break; }
                            InputBinding::PadDirOn { device, dir: d } if d == dir && device == dev => { out.push((*act, pressed)); break; }
                            _ => {}
                        }
                    }
                }
            }
            PadEvent::Button { id, btn, pressed } => {
                let dev = usize::from(id);
                for (act, binds) in &self.map {
                    for b in binds {
                        match *b {
                            InputBinding::PadButton(b0) if b0 == btn => { out.push((*act, pressed)); break; }
                            InputBinding::PadButtonOn { device, btn: b0 } if b0 == btn && device == dev => { out.push((*act, pressed)); break; }
                            _ => {}
                        }
                    }
                }
            }
            PadEvent::Face { id, btn, pressed } => {
                let dev = usize::from(id);
                for (act, binds) in &self.map {
                    for b in binds {
                        match *b {
                            InputBinding::Face(b0) if b0 == btn => { out.push((*act, pressed)); break; }
                            InputBinding::FaceOn { device, btn: b0 } if b0 == btn && device == dev => { out.push((*act, pressed)); break; }
                            _ => {}
                        }
                    }
                }
            }
        }
        out
    }
}

// INI parsing and default emission moved to config.rs

/* ------------------------- Normalized input events ------------------------- */

#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    pub action: VirtualAction,
    pub pressed: bool,
    pub source: InputSource,
    pub timestamp: Instant,
}

#[inline(always)]
pub fn map_key_event(ev: &KeyEvent) -> Vec<InputEvent> {
    let mut out = Vec::with_capacity(2);
    // Ignore OS key auto-repeat for pressed events (prevents resetting hold timers)
    if ev.state == ElementState::Pressed && ev.repeat {
        return out;
    }
    let km = get_keymap();
    let mut actions = km.actions_for_key_event(ev);
    dedup_menu_variants(&mut actions);
    if actions.is_empty() { return out; }
    let timestamp = Instant::now();
    for (act, pressed) in actions {
        out.push(InputEvent { action: act, pressed, source: InputSource::Keyboard, timestamp });
    }
    out
}

#[inline(always)]
pub fn map_pad_event(ev: &PadEvent) -> Vec<InputEvent> {
    let mut out = Vec::with_capacity(2);
    let km = get_keymap();
    let mut actions = km.actions_for_pad_event(ev);
    dedup_menu_variants(&mut actions);
    if actions.is_empty() { return out; }
    let timestamp = Instant::now();
    for (act, pressed) in actions {
        out.push(InputEvent { action: act, pressed, source: InputSource::Gamepad, timestamp });
    }
    out
}

#[inline(always)]
pub fn lane_from_action(act: VirtualAction) -> Option<Lane> {
    match act {
        VirtualAction::p1_left  => Some(Lane::Left),
        VirtualAction::p1_down  => Some(Lane::Down),
        VirtualAction::p1_up    => Some(Lane::Up),
        VirtualAction::p1_right => Some(Lane::Right),
        _ => None,
    }
}

#[inline(always)]
fn dedup_menu_variants(actions: &mut Vec<(VirtualAction, bool)>) {
    use VirtualAction as A;
    // If both menu and non-menu variants for the same direction are present with the same
    // pressed state, drop the menu variant to avoid double-triggering navigation.
    let snapshot = actions.clone();
    let has = |a: A, p: bool| snapshot.iter().any(|(act, pr)| *act == a && *pr == p);
    actions.retain(|(act, pr)| match *act {
        A::p1_menu_up    => !(has(A::p1_up,    *pr)),
        A::p1_menu_down  => !(has(A::p1_down,  *pr)),
        A::p1_menu_left  => !(has(A::p1_left,  *pr)),
        A::p1_menu_right => !(has(A::p1_right, *pr)),
        _ => true,
    });
}
