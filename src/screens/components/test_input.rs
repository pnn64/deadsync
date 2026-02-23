use crate::act;
use crate::core::input::{InputEvent, PadDir, PadEvent, VirtualAction, with_keymap};
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::ui::actors::Actor;
use std::collections::HashMap;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

const UNMAPPED_AXIS_HELD_THRESHOLD: f32 = 0.5;
const SORT_MENU_DIM_ALPHA: f32 = 0.875;
const SORT_MENU_CLOSE_HINT: &str = "Press &START; to dismiss.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogicalButton {
    Up,
    Down,
    Left,
    Right,
    MenuLeft,
    MenuRight,
    Start,
    Select,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayerSlot {
    P1,
    P2,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    buttons_held: HashMap<(PlayerSlot, LogicalButton), bool>,
    unmapped: UnmappedTracker,
}

#[derive(Clone, Debug, Default)]
struct UnmappedTracker {
    held: HashMap<UnmappedKey, bool>,
    axis_value: HashMap<UnmappedKey, f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum UnmappedKey {
    Dir { dev: usize, dir: PadDir },
    RawButton { dev: usize, code_u32: u32 },
    RawAxis { dev: usize, code_u32: u32 },
    Keyboard { code: KeyCode },
}

impl UnmappedTracker {
    #[inline(always)]
    fn set(&mut self, key: UnmappedKey, pressed: bool) {
        self.held.insert(key, pressed);
    }

    #[inline(always)]
    fn set_axis(&mut self, key: UnmappedKey, value: f32) {
        self.axis_value.insert(key, value);
        self.held
            .insert(key, value.abs() >= UNMAPPED_AXIS_HELD_THRESHOLD);
    }

    #[inline(always)]
    fn active_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (k, pressed) in &self.held {
            if !*pressed {
                continue;
            }
            let line = match *k {
                UnmappedKey::Dir { dev, dir } => format!("Gamepad {dev}: Dir::{dir:?}"),
                UnmappedKey::RawButton { dev, code_u32 } => {
                    format!("Gamepad {dev}: RawButton [0x{code_u32:08X}]")
                }
                UnmappedKey::RawAxis { dev, code_u32 } => {
                    let value = self.axis_value.get(k).copied().unwrap_or(0.0);
                    format!("Gamepad {dev}: RawAxis [0x{code_u32:08X}] ({value:.3})")
                }
                UnmappedKey::Keyboard { code } => format!("Keyboard: KeyCode::{code:?}"),
            };
            out.push(format!("{line} (not mapped)"));
        }
        out.sort();
        out
    }
}

#[inline(always)]
pub fn clear(state: &mut State) {
    *state = State::default();
}

const fn player_from_action(act: VirtualAction) -> Option<PlayerSlot> {
    use VirtualAction::{
        p1_down, p1_left, p1_menu_left, p1_menu_right, p1_right, p1_select, p1_start, p1_up,
        p2_down, p2_left, p2_menu_left, p2_menu_right, p2_right, p2_select, p2_start, p2_up,
    };
    match act {
        p1_up | p1_down | p1_left | p1_right | p1_menu_left | p1_menu_right | p1_start
        | p1_select => Some(PlayerSlot::P1),
        p2_up | p2_down | p2_left | p2_right | p2_menu_left | p2_menu_right | p2_start
        | p2_select => Some(PlayerSlot::P2),
        _ => None,
    }
}

const fn logical_button_from_action(act: VirtualAction) -> Option<LogicalButton> {
    use VirtualAction::{
        p1_down, p1_left, p1_menu_left, p1_menu_right, p1_right, p1_select, p1_start, p1_up,
        p2_down, p2_left, p2_menu_left, p2_menu_right, p2_right, p2_select, p2_start, p2_up,
    };
    match act {
        p1_up | p2_up => Some(LogicalButton::Up),
        p1_down | p2_down => Some(LogicalButton::Down),
        p1_left | p2_left => Some(LogicalButton::Left),
        p1_right | p2_right => Some(LogicalButton::Right),
        p1_menu_left | p2_menu_left => Some(LogicalButton::MenuLeft),
        p1_menu_right | p2_menu_right => Some(LogicalButton::MenuRight),
        p1_start | p2_start => Some(LogicalButton::Start),
        p1_select | p2_select => Some(LogicalButton::Select),
        _ => None,
    }
}

pub fn apply_virtual_input(state: &mut State, ev: &InputEvent) {
    if let Some(player) = player_from_action(ev.action)
        && let Some(btn) = logical_button_from_action(ev.action)
    {
        state.buttons_held.insert((player, btn), ev.pressed);
    }
}

pub fn apply_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    use crate::core::input::PadEvent as PE;

    let (key, pressed_opt, axis_value_opt) = match pad_event {
        PE::Dir {
            id, dir, pressed, ..
        } => {
            let dev = usize::from(*id);
            (UnmappedKey::Dir { dev, dir: *dir }, Some(*pressed), None)
        }
        PE::RawButton {
            id, code, pressed, ..
        } => {
            let dev = usize::from(*id);
            (
                UnmappedKey::RawButton {
                    dev,
                    code_u32: code.into_u32(),
                },
                Some(*pressed),
                None,
            )
        }
        PE::RawAxis {
            id, code, value, ..
        } => {
            let dev = usize::from(*id);
            (
                UnmappedKey::RawAxis {
                    dev,
                    code_u32: code.into_u32(),
                },
                None,
                Some(*value),
            )
        }
    };

    let mapped = with_keymap(|km| !km.actions_for_pad_event(pad_event).is_empty());
    if mapped {
        return;
    }

    if let Some(pressed) = pressed_opt {
        state.unmapped.set(key, pressed);
        return;
    }
    if let Some(value) = axis_value_opt {
        state.unmapped.set_axis(key, value);
    }
}

pub fn apply_raw_key_event(state: &mut State, key_event: &KeyEvent) {
    let PhysicalKey::Code(code) = key_event.physical_key else {
        return;
    };
    if key_event.repeat {
        return;
    }
    let mapped = with_keymap(|km| !km.actions_for_key_event(key_event).is_empty());
    if mapped {
        return;
    }
    let pressed = key_event.state == ElementState::Pressed;
    state.unmapped.set(UnmappedKey::Keyboard { code }, pressed);
}

#[inline(always)]
fn held_alpha(state: &State, slot: PlayerSlot, button: LogicalButton) -> f32 {
    if *state.buttons_held.get(&(slot, button)).unwrap_or(&false) {
        1.0
    } else {
        0.0
    }
}

fn push_pad(
    actors: &mut Vec<Actor>,
    state: &State,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    show_menu_buttons: bool,
    show_player_label: bool,
    z: f32,
) {
    let arrow_h_offset = 67.0_f32;
    let arrow_v_offset = 68.0_f32;
    let buttons_y = pad_y + 160.0;
    let start_y = pad_y + 146.0;
    let select_y = pad_y + 175.0;
    let menu_y = pad_y + 160.0;
    let menu_x_offset = 37.0_f32;

    actors.push(act!(sprite("test_input/dance.png"):
        align(0.5, 0.5):
        xy(pad_x, pad_y):
        zoom(0.8):
        z(z)
    ));

    if show_player_label {
        let label = match slot {
            PlayerSlot::P1 => "Player 1",
            PlayerSlot::P2 => "Player 2",
        };
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(pad_x, pad_y - 130.0):
            zoom(0.7):
            font("wendy"):
            settext(label):
            horizalign(center):
            z(z + 1.0)
        ));
    }

    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x, pad_y - arrow_v_offset):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Up)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x, pad_y + arrow_v_offset):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Down)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x - arrow_h_offset, pad_y):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Left)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x + arrow_h_offset, pad_y):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Right)):
        z(z + 1.0)
    ));

    if !show_menu_buttons {
        return;
    }

    actors.push(act!(sprite("test_input/buttons.png"):
        align(0.5, 0.5):
        xy(pad_x, buttons_y):
        zoom(0.5):
        z(z)
    ));
    actors.push(act!(sprite("test_input/highlightgreen.png"):
        align(0.5, 0.5):
        xy(pad_x, start_y):
        zoom(0.5):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Start)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlightred.png"):
        align(0.5, 0.5):
        xy(pad_x, select_y):
        zoom(0.5):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Select)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlightarrow.png"):
        align(0.5, 0.5):
        xy(pad_x - menu_x_offset, menu_y):
        zoom(0.5):
        rotationz(180.0):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::MenuLeft)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlightarrow.png"):
        align(0.5, 0.5):
        xy(pad_x + menu_x_offset, menu_y):
        zoom(0.5):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::MenuRight)):
        z(z + 1.0)
    ));
}

pub fn build_test_input_screen_content(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(48);
    let cx = screen_center_x();
    let cy = screen_center_y() - 20.0;
    let pad_spacing = 150.0;

    push_pad(
        &mut actors,
        state,
        PlayerSlot::P1,
        cx - pad_spacing,
        cy,
        true,
        true,
        20.0,
    );
    push_pad(
        &mut actors,
        state,
        PlayerSlot::P2,
        cx + pad_spacing,
        cy,
        true,
        true,
        20.0,
    );

    let lines = state.unmapped.active_lines();
    if !lines.is_empty() {
        let start_y = cy + 112.0;
        let line_h = 16.0;
        for (i, line) in lines.iter().enumerate() {
            actors.push(act!(text:
                font("miso"):
                settext(line.clone()):
                align(0.5, 0.0):
                xy(cx, (i as f32).mul_add(line_h, start_y)):
                zoom(0.8):
                horizalign(center):
                z(30)
            ));
        }
    }

    actors.push(act!(text:
        font("miso"):
        settext("Hold &BACK; to return to Options."):
        align(0.5, 0.0):
        xy(cx, screen_height() - 40.0):
        zoom(0.8):
        horizalign(center):
        z(30)
    ));

    actors
}

pub fn build_select_music_overlay(
    state: &State,
    show_p1: bool,
    show_p2: bool,
    pad_spacing: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(24);
    let cx = screen_center_x();
    // SL parity: overlay/TestInput.lua places pad AF at y = _screen.cy + 50, then
    // _modules/TestInput Pad/default.lua places the pad art at y = -80 inside that AF.
    // Net visual pad center is _screen.cy - 30.
    let cy = screen_center_y() - 30.0;

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, SORT_MENU_DIM_ALPHA):
        z(1450)
    ));

    if show_p1 {
        push_pad(
            &mut actors,
            state,
            PlayerSlot::P1,
            cx - pad_spacing,
            cy,
            false,
            false,
            1451.0,
        );
    }
    if show_p2 {
        push_pad(
            &mut actors,
            state,
            PlayerSlot::P2,
            cx + pad_spacing,
            cy,
            false,
            false,
            1451.0,
        );
    }

    actors.push(act!(text:
        font("miso"):
        settext(SORT_MENU_CLOSE_HINT):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 120.0):
        zoom(1.1):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1453):
        horizalign(center)
    ));

    actors
}
