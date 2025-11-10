use std::time::Instant;
use std::collections::HashMap;

use once_cell::sync::Lazy;
use std::sync::Mutex;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use configparser::ini::Ini;

use crate::core::gamepad::{PadEvent, PadDir, PadButton, FaceBtn};

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

#[derive(Default)]
pub struct InputState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

pub fn init_state() -> InputState {
    InputState::default()
}

pub fn handle_keyboard_input(event: &KeyEvent, state: &mut InputState) {
    if let PhysicalKey::Code(code) = event.physical_key {
        let is_pressed = event.state == ElementState::Pressed;
        let target = match code {
            KeyCode::ArrowUp | KeyCode::KeyW => Some(&mut state.up),
            KeyCode::ArrowDown | KeyCode::KeyS => Some(&mut state.down),
            KeyCode::ArrowLeft | KeyCode::KeyA => Some(&mut state.left),
            KeyCode::ArrowRight | KeyCode::KeyD => Some(&mut state.right),
            _ => None,
        };
        if let Some(slot) = target {
            *slot = is_pressed;
        }
    }
}

#[inline(always)]
pub fn lane_from_keycode(code: KeyCode) -> Option<Lane> {
    match code {
        KeyCode::ArrowLeft | KeyCode::KeyD => Some(Lane::Left),
        KeyCode::ArrowDown | KeyCode::KeyF => Some(Lane::Down),
        KeyCode::ArrowUp | KeyCode::KeyJ => Some(Lane::Up),
        KeyCode::ArrowRight | KeyCode::KeyK => Some(Lane::Right),
        _ => None,
    }
}

/* ------------------------ Virtual Keymap system ------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VirtualAction {
    P1_Up,
    P1_Down,
    P1_Left,
    P1_Right,
    P1_Start,
    P1_Back,
    P1_MenuUp,
    P1_MenuDown,
    P1_MenuLeft,
    P1_MenuRight,
    P1_Select,
    P1_Operator,
    P1_Restart,
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
    km.bind(A::P1_Up,    &[
        InputBinding::Key(KeyCode::ArrowUp), InputBinding::Key(KeyCode::KeyW),
        InputBinding::PadDir(PadDir::Up),
    ]);
    km.bind(A::P1_Down,  &[
        InputBinding::Key(KeyCode::ArrowDown), InputBinding::Key(KeyCode::KeyS),
        InputBinding::PadDir(PadDir::Down),
    ]);
    km.bind(A::P1_Left,  &[
        InputBinding::Key(KeyCode::ArrowLeft), InputBinding::Key(KeyCode::KeyA),
        InputBinding::PadDir(PadDir::Left),
    ]);
    km.bind(A::P1_Right, &[
        InputBinding::Key(KeyCode::ArrowRight), InputBinding::Key(KeyCode::KeyD),
        InputBinding::PadDir(PadDir::Right),
    ]);
    km.bind(A::P1_Start, &[
        InputBinding::Key(KeyCode::Enter), InputBinding::PadButton(PadButton::Confirm)
    ]);
    km.bind(A::P1_Back, &[
        InputBinding::Key(KeyCode::Escape), InputBinding::PadButton(PadButton::Back)
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

// ---- INI parsing / writing for [keymaps] ----

const SECTION: &str = "keymaps"; // [keymaps]

#[inline(always)]
fn parse_action_key(k: &str) -> Option<VirtualAction> {
    use VirtualAction::*;
    match k {
        "P1_Up" => Some(P1_Up),
        "P1_Down" => Some(P1_Down),
        "P1_Left" => Some(P1_Left),
        "P1_Right" => Some(P1_Right),
        "P1_Start" => Some(P1_Start),
        "P1_Back" => Some(P1_Back),
        "P1_MenuUp" => Some(P1_MenuUp),
        "P1_MenuDown" => Some(P1_MenuDown),
        "P1_MenuLeft" => Some(P1_MenuLeft),
        "P1_MenuRight" => Some(P1_MenuRight),
        "P1_Select" => Some(P1_Select),
        "P1_Operator" => Some(P1_Operator),
        "P1_Restart" => Some(P1_Restart),
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
    if let Some(section) = conf.get_map_ref().get(SECTION) {
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
    out.push_str("[keymaps]\n");

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

    push_row("P1_Back", km.map.get(&A::P1_Back));
    push_row("P1_Down", km.map.get(&A::P1_Down));
    push_row("P1_Left", km.map.get(&A::P1_Left));
    push_row("P1_Right", km.map.get(&A::P1_Right));
    push_row("P1_Start", km.map.get(&A::P1_Start));
    push_row("P1_Up", km.map.get(&A::P1_Up));
    // Do not emit Menu* aliases into the INI by default
    push_row("P1_Select", km.map.get(&A::P1_Select));
    push_row("P1_Operator", km.map.get(&A::P1_Operator));
    push_row("P1_Restart", km.map.get(&A::P1_Restart));
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
        VirtualAction::P1_Left | VirtualAction::P1_MenuLeft => Some(Lane::Left),
        VirtualAction::P1_Down | VirtualAction::P1_MenuDown => Some(Lane::Down),
        VirtualAction::P1_Up   | VirtualAction::P1_MenuUp   => Some(Lane::Up),
        VirtualAction::P1_Right| VirtualAction::P1_MenuRight=> Some(Lane::Right),
        _ => None,
    }
}
