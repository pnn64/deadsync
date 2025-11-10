use std::time::Instant;
use std::collections::HashMap;

use once_cell::sync::Lazy;
use std::sync::Mutex;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use configparser::ini::Ini;

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
        if active_id.is_none() { *active_id = Some(id); }

        // Ignore input events from non-active gamepads.
        if Some(id) != *active_id { continue; }

        match event {
            EventType::ButtonPressed(btn, _) => {
                match btn {
                    // Face buttons → Face events
                    Button::South => out.push(PadEvent::Face { btn: FaceBtn::SouthA, pressed: true }),
                    Button::East  => out.push(PadEvent::Face { btn: FaceBtn::EastB,  pressed: true }),
                    Button::West  => out.push(PadEvent::Face { btn: FaceBtn::WestX,  pressed: true }),
                    Button::North => {
                        out.push(PadEvent::Face { btn: FaceBtn::NorthY, pressed: true });
                        if want_f7 { out.push(PadEvent::Button { btn: PadButton::F7, pressed: true }); }
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
                        if want_f7 { out.push(PadEvent::Button { btn: PadButton::F7, pressed: false }); }
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
}

#[derive(Clone, Debug, Default)]
pub struct Keymap { map: HashMap<VirtualAction, Vec<InputBinding>> }

static KEYMAP: Lazy<Mutex<Keymap>> = Lazy::new(|| Mutex::new(default_keymap()));

#[inline(always)]
pub fn get_keymap() -> Keymap { KEYMAP.lock().unwrap().clone() }

#[inline(always)]
pub fn set_keymap(new_map: Keymap) { *KEYMAP.lock().unwrap() = new_map; }

fn default_keymap() -> Keymap {
    let mut km = Keymap { map: HashMap::with_capacity(16) };
    use VirtualAction as A;
    km.bind(A::p1_up,    &[
        InputBinding::Key(KeyCode::ArrowUp), InputBinding::Key(KeyCode::KeyW),
    ]);
    km.bind(A::p1_down,  &[
        InputBinding::Key(KeyCode::ArrowDown), InputBinding::Key(KeyCode::KeyS),
    ]);
    km.bind(A::p1_left,  &[
        InputBinding::Key(KeyCode::ArrowLeft), InputBinding::Key(KeyCode::KeyA),
    ]);
    km.bind(A::p1_right, &[
        InputBinding::Key(KeyCode::ArrowRight), InputBinding::Key(KeyCode::KeyD),
    ]);
    km.bind(A::p1_start, &[
        InputBinding::Key(KeyCode::Enter)
    ]);
    km.bind(A::p1_back, &[
        InputBinding::Key(KeyCode::Escape)
    ]);

    // Do not bind Menu* aliases by default to avoid duplicate events
    km
}

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
        match ev {
            PadEvent::Dir { dir, pressed } => {
                for (act, binds) in &self.map {
                    for b in binds { if *b == InputBinding::PadDir(*dir) { out.push((*act, *pressed)); break; } }
                }
            }
            PadEvent::Button { btn, pressed } => {
                for (act, binds) in &self.map {
                    for b in binds { if *b == InputBinding::PadButton(*btn) { out.push((*act, *pressed)); break; } }
                }
            }
            PadEvent::Face { btn, pressed } => {
                for (act, binds) in &self.map {
                    for b in binds { if *b == InputBinding::Face(*btn) { out.push((*act, *pressed)); break; } }
                }
            }
        }
        out
    }
}

// ---- INI parsing / writing for [Keymaps] ----

const SECTION: &str = "Keymaps"; // [Keymaps]

#[inline(always)]
fn parse_action_key(k: &str) -> Option<VirtualAction> {
    use VirtualAction::*;
    match k {
        "P1_Up" => Some(p1_up),
        "P1_Down" => Some(p1_down),
        "P1_Left" => Some(p1_left),
        "P1_Right" => Some(p1_right),
        "P1_Start" => Some(p1_start),
        "P1_Back" => Some(p1_back),
        "P1_MenuUp" => Some(p1_menu_up),
        "P1_MenuDown" => Some(p1_menu_down),
        "P1_MenuLeft" => Some(p1_menu_left),
        "P1_MenuRight" => Some(p1_menu_right),
        "P1_Select" => Some(p1_select),
        "P1_Operator" => Some(p1_operator),
        "P1_Restart" => Some(p1_restart),
        _ => None,
    }
}

#[inline(always)]
fn parse_binding_token(tok: &str) -> Option<InputBinding> {
    // Preferred transparent forms:
    //  - KeyCode::<Variant>
    //  - PadDir::Up/Down/Left/Right
    //  - PadButton::Confirm/Back/F7
    //  - FaceBtn::SouthA/WestX/EastB/NorthY

    if let Some(rest) = tok.strip_prefix("KeyCode::") {
        let code = match rest {
            // Special keys
            "Enter" => KeyCode::Enter,
            "Escape" => KeyCode::Escape,
            "ArrowUp" => KeyCode::ArrowUp,
            "ArrowDown" => KeyCode::ArrowDown,
            "ArrowLeft" => KeyCode::ArrowLeft,
            "ArrowRight" => KeyCode::ArrowRight,
            // Letter keys
            "KeyA" => KeyCode::KeyA, "KeyB" => KeyCode::KeyB, "KeyC" => KeyCode::KeyC, "KeyD" => KeyCode::KeyD,
            "KeyE" => KeyCode::KeyE, "KeyF" => KeyCode::KeyF, "KeyG" => KeyCode::KeyG, "KeyH" => KeyCode::KeyH,
            "KeyI" => KeyCode::KeyI, "KeyJ" => KeyCode::KeyJ, "KeyK" => KeyCode::KeyK, "KeyL" => KeyCode::KeyL,
            "KeyM" => KeyCode::KeyM, "KeyN" => KeyCode::KeyN, "KeyO" => KeyCode::KeyO, "KeyP" => KeyCode::KeyP,
            "KeyQ" => KeyCode::KeyQ, "KeyR" => KeyCode::KeyR, "KeyS" => KeyCode::KeyS, "KeyT" => KeyCode::KeyT,
            "KeyU" => KeyCode::KeyU, "KeyV" => KeyCode::KeyV, "KeyW" => KeyCode::KeyW, "KeyX" => KeyCode::KeyX,
            "KeyY" => KeyCode::KeyY, "KeyZ" => KeyCode::KeyZ,
            _ => return None,
        };
        return Some(InputBinding::Key(code));
    }

    if let Some(rest) = tok.strip_prefix("PadDir::") {
        return Some(InputBinding::PadDir(match rest {
            "Up" => PadDir::Up,
            "Down" => PadDir::Down,
            "Left" => PadDir::Left,
            "Right" => PadDir::Right,
            _ => return None,
        }));
    }
    if let Some(rest) = tok.strip_prefix("PadButton::") {
        return Some(InputBinding::PadButton(match rest {
            "Confirm" => PadButton::Confirm,
            "Back" => PadButton::Back,
            "F7" => PadButton::F7,
            _ => return None,
        }));
    }
    if let Some(rest) = tok.strip_prefix("FaceBtn::") {
        return Some(InputBinding::Face(match rest {
            "SouthA" => FaceBtn::SouthA,
            "WestX" => FaceBtn::WestX,
            "EastB" => FaceBtn::EastB,
            "NorthY" => FaceBtn::NorthY,
            _ => return None,
        }));
    }

    None
}

pub fn load_keymap_from_ini(conf: &Ini) -> Keymap {
    let mut km = default_keymap();
    // Accept both [Keymaps] (preferred) and legacy [keymaps]
    if let Some(section) = conf
        .get_map_ref()
        .get(SECTION)
        .or_else(|| conf.get_map_ref().get("keymaps"))
    {
        for (k, v_opt) in section {
            if let Some(action) = parse_action_key(k) {
                let mut bindings = Vec::new();
                let spec = v_opt.as_deref().unwrap_or("").trim();
                if !spec.is_empty() {
                    for tok in spec.split(|c| c == ':' || c == ';') {
                        if let Some(b) = parse_binding_token(tok.trim()) { bindings.push(b); }
                    }
                }
                km.map.insert(action, bindings);
            }
        }
    }
    km
}

pub fn keymap_to_ini_section_string(km: &Keymap) -> String {
    use VirtualAction as A;
    let mut out = String::new();
    out.push_str("[Keymaps]\n");

    let emit = |b: &InputBinding| -> &'static str {
        match *b {
            InputBinding::Key(KeyCode::Enter) => "KeyCode::Enter",
            InputBinding::Key(KeyCode::Escape) => "KeyCode::Escape",
            InputBinding::Key(KeyCode::ArrowUp) => "KeyCode::ArrowUp",
            InputBinding::Key(KeyCode::ArrowDown) => "KeyCode::ArrowDown",
            InputBinding::Key(KeyCode::ArrowLeft) => "KeyCode::ArrowLeft",
            InputBinding::Key(KeyCode::ArrowRight) => "KeyCode::ArrowRight",
            InputBinding::Key(KeyCode::KeyA) => "KeyCode::KeyA",
            InputBinding::Key(KeyCode::KeyS) => "KeyCode::KeyS",
            InputBinding::Key(KeyCode::KeyD) => "KeyCode::KeyD",
            InputBinding::Key(KeyCode::KeyW) => "KeyCode::KeyW",
            _ => "",
        }
    };

    let mut push_row = |name: &str, v: Option<&Vec<InputBinding>>| {
        let mut parts: Vec<&str> = Vec::new();
        if let Some(list) = v {
            for b in list {
                match b {
                    InputBinding::Key(_) => { let t = emit(b); if !t.is_empty() { parts.push(t); } }
                    InputBinding::PadDir(PadDir::Up) => parts.push("PadDir::Up"),
                    InputBinding::PadDir(PadDir::Down) => parts.push("PadDir::Down"),
                    InputBinding::PadDir(PadDir::Left) => parts.push("PadDir::Left"),
                    InputBinding::PadDir(PadDir::Right) => parts.push("PadDir::Right"),
                    InputBinding::PadButton(PadButton::Confirm) => parts.push("PadButton::Confirm"),
                    InputBinding::PadButton(PadButton::Back) => parts.push("PadButton::Back"),
                    InputBinding::PadButton(PadButton::F7) => parts.push("PadButton::F7"),
                    InputBinding::Face(FaceBtn::SouthA) => parts.push("FaceBtn::SouthA"),
                    InputBinding::Face(FaceBtn::WestX) => parts.push("FaceBtn::WestX"),
                    InputBinding::Face(FaceBtn::EastB) => parts.push("FaceBtn::EastB"),
                    InputBinding::Face(FaceBtn::NorthY) => parts.push("FaceBtn::NorthY"),
                }
            }
        }
        out.push_str(&format!("{}={}\n", name, parts.join(";")));
    };

    // Emit keys in strict alphabetical order
    push_row("P1_Back", km.map.get(&A::p1_back));
    push_row("P1_Down", km.map.get(&A::p1_down));
    push_row("P1_Left", km.map.get(&A::p1_left));
    push_row("P1_Operator", km.map.get(&A::p1_operator));
    push_row("P1_Restart", km.map.get(&A::p1_restart));
    push_row("P1_Right", km.map.get(&A::p1_right));
    push_row("P1_Select", km.map.get(&A::p1_select));
    push_row("P1_Start", km.map.get(&A::p1_start));
    push_row("P1_Up", km.map.get(&A::p1_up));
    // Do not emit Menu* aliases into the INI by default
    out.push('\n');
    out
}

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
    let actions = km.actions_for_key_event(ev);
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
    let actions = km.actions_for_pad_event(ev);
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
        VirtualAction::p1_left | VirtualAction::p1_menu_left => Some(Lane::Left),
        VirtualAction::p1_down | VirtualAction::p1_menu_down => Some(Lane::Down),
        VirtualAction::p1_up   | VirtualAction::p1_menu_up   => Some(Lane::Up),
        VirtualAction::p1_right| VirtualAction::p1_menu_right=> Some(Lane::Right),
        _ => None,
    }
}
