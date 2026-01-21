use crate::act;
use crate::core::input::{InputEvent, PadEvent, VirtualAction, get_keymap};
use crate::core::space::*;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::components::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::components::{heart_bg, screen_bar};
use std::collections::HashMap;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/// Logical buttons we visualize on the pad/menu HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogicalButton {
    // Dance pad directions (P1/P2 share layout; highlight per side).
    Up,
    Down,
    Left,
    Right,
    // Menu buttons (center cluster below pad).
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
pub struct PadVisualState {
    pub buttons_held: HashMap<(PlayerSlot, LogicalButton), bool>,
}

#[derive(Clone, Debug, Default)]
pub struct UnmappedTracker {
    /// Map of a human-readable device element label → whether it is currently held.
    held: HashMap<String, bool>,
}

impl UnmappedTracker {
    #[inline(always)]
    pub fn set(&mut self, key: String, pressed: bool) {
        if pressed {
            self.held.insert(key, true);
        } else {
            self.held.insert(key, false);
        }
    }

    #[inline(always)]
    pub fn active_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (k, v) in &self.held {
            if *v {
                out.push(format!("{k} (not mapped)"));
            }
        }
        out.sort();
        out
    }
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    pad_visual: PadVisualState,
    unmapped: UnmappedTracker,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        pad_visual: PadVisualState::default(),
        unmapped: UnmappedTracker::default(),
    }
}

/* ------------------------------- update ------------------------------- */

pub fn update(_state: &mut State, _dt: f32) {
    // No time-based animation yet; highlights are driven directly by input edges.
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

fn player_from_action(act: VirtualAction) -> Option<PlayerSlot> {
    use VirtualAction::*;
    match act {
        p1_up | p1_down | p1_left | p1_right | p1_menu_up | p1_menu_down | p1_menu_left
        | p1_menu_right | p1_start | p1_select | p1_back | p1_operator | p1_restart => {
            Some(PlayerSlot::P1)
        }
        p2_up | p2_down | p2_left | p2_right | p2_menu_up | p2_menu_down | p2_menu_left
        | p2_menu_right | p2_start | p2_select | p2_back | p2_operator | p2_restart => {
            Some(PlayerSlot::P2)
        }
    }
}

fn logical_button_from_action(act: VirtualAction) -> Option<LogicalButton> {
    use VirtualAction::*;
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

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    // Back or Start returns to Options, matching the operator menu behavior.
    if ev.pressed {
        match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back => {
                return ScreenAction::Navigate(Screen::Options);
            }
            VirtualAction::p1_start | VirtualAction::p2_start => {
                return ScreenAction::Navigate(Screen::Options);
            }
            _ => {}
        }
    }

    // Update pad highlight state from virtual actions (for mapped inputs).
    if let Some(player) = player_from_action(ev.action) {
        if let Some(btn) = logical_button_from_action(ev.action) {
            state
                .pad_visual
                .buttons_held
                .insert((player, btn), ev.pressed);
        }
    }

    ScreenAction::None
}

/// Raw pad events are used to approximate Simply Love's \"unmapped\" device list.
pub fn handle_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    use crate::core::input::PadEvent as PE;

    // Determine a stable, human-readable label for this device element.
    let (label, pressed_opt) = match pad_event {
        PE::Dir { id, dir, pressed, .. } => {
            let dev = usize::from(*id);
            (format!("Gamepad {}: Dir::{:?}", dev, dir), Some(*pressed))
        }
        PE::RawButton {
            id,
            code,
            pressed,
            ..
        } => {
            let dev = usize::from(*id);
            let code_u32 = code.into_u32();
            (
                format!("Gamepad {}: RawButton [0x{:08X}]", dev, code_u32),
                Some(*pressed),
            )
        }
        PE::RawAxis {
            id,
            code,
            value,
            ..
        } => {
            let dev = usize::from(*id);
            let code_u32 = code.into_u32();
            // Axis inputs are continuous; treat them as always \"pressed\" for display.
            (
                format!("Gamepad {}: RawAxis [0x{:08X}] ({:.3})", dev, code_u32, value),
                None,
            )
        }
    };

    // Use the same mapping logic as the main input system to decide whether this
    // element corresponds to any virtual action. If not, show it as \"not mapped\".
    let km = get_keymap();
    let mapped = !km.actions_for_pad_event(pad_event).is_empty();

    if !mapped {
        if let Some(pressed) = pressed_opt {
            state.unmapped.set(label, pressed);
        } else {
            // For axes, treat any movement as active.
            state.unmapped.set(label, true);
        }
    }
}

/* ------------------------------- drawing ------------------------------- */

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);

    /* -------------------------- HEART BACKGROUND -------------------------- */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    /* ------------------------------ TOP BAR ------------------------------- */
    const FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "TEST INPUT",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        fg_color: FG,
    }));

    /* --------------------------- PAD VISUALS --------------------------- */

    // Basic layout: two pads centered horizontally, unmapped list below.
    let cx = screen_center_x();
    let cy = screen_center_y() - 20.0;
    let pad_spacing = 150.0;

    // NOTE: The textures below assume you'll copy the Simply Love assets to:
    // - deadsync/assets/graphics/test_input/dance.png
    // - deadsync/assets/graphics/test_input/buttons.png
    // - deadsync/assets/graphics/test_input/highlight.png
    // - deadsync/assets/graphics/test_input/highlightgreen.png
    // - deadsync/assets/graphics/test_input/highlightred.png
    // - deadsync/assets/graphics/test_input/highlightarrow.png
    //
    // and optionally pump/techno variants later.

    // Draw pad backgrounds for P1 and P2 using the dance pad image for now.
    // Positions are chosen to mirror Simply Love's TestInput Pad offsets
    // relative to the pad center as closely as possible.
    let arrow_h_offset = 67.0_f32;
    let arrow_v_offset = 68.0_f32;
    let buttons_y = cy + 160.0;
    let start_y = cy + 146.0;
    let select_y = cy + 175.0;
    let menu_y = cy + 160.0;
    let menu_x_offset = 37.0_f32;

    for (slot, x_offset) in [
        (PlayerSlot::P1, -pad_spacing),
        (PlayerSlot::P2, pad_spacing),
    ] {
        let pad_x = cx + x_offset as f32;
        actors.push(act!(sprite("test_input/dance.png"):
            align(0.5, 0.5):
            xy(pad_x, cy):
            zoom(0.8)
        ));

        // Player label above pad.
        let label = match slot {
            PlayerSlot::P1 => "P1",
            PlayerSlot::P2 => "P2",
        };
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(pad_x, cy - 150.0):
            zoom(0.7):
            font("miso"):
            settext(label):
            horizalign(center)
        ));

        // Simple four-direction highlights roughly matching Simply Love layout.
        let held = &state.pad_visual.buttons_held;
        let alpha_up = if *held.get(&(slot, LogicalButton::Up)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        let alpha_down = if *held.get(&(slot, LogicalButton::Down)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        let alpha_left = if *held.get(&(slot, LogicalButton::Left)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        let alpha_right = if *held.get(&(slot, LogicalButton::Right)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };

        actors.push(act!(sprite("test_input/highlight.png"):
            align(0.5, 0.5):
            xy(pad_x, cy - arrow_v_offset):
            zoom(0.8):
            diffuse(1.0, 1.0, 1.0, alpha_up)
        ));
        actors.push(act!(sprite("test_input/highlight.png"):
            align(0.5, 0.5):
            xy(pad_x, cy + arrow_v_offset):
            zoom(0.8):
            diffuse(1.0, 1.0, 1.0, alpha_down)
        ));
        actors.push(act!(sprite("test_input/highlight.png"):
            align(0.5, 0.5):
            xy(pad_x - arrow_h_offset, cy):
            zoom(0.8):
            diffuse(1.0, 1.0, 1.0, alpha_left)
        ));
        actors.push(act!(sprite("test_input/highlight.png"):
            align(0.5, 0.5):
            xy(pad_x + arrow_h_offset, cy):
            zoom(0.8):
            diffuse(1.0, 1.0, 1.0, alpha_right)
        ));

        // Menu button cluster below pad (Start/Select/MenuLeft/MenuRight).
        let alpha_start = if *held.get(&(slot, LogicalButton::Start)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        let alpha_select = if *held.get(&(slot, LogicalButton::Select)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        let alpha_mleft = if *held.get(&(slot, LogicalButton::MenuLeft)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        let alpha_mright = if *held
            .get(&(slot, LogicalButton::MenuRight))
            .unwrap_or(&false)
        {
            1.0
        } else {
            0.0
        };

        // Buttons background sprite (mirrors Simply Love's buttons.png).
        actors.push(act!(sprite("test_input/buttons.png"):
            align(0.5, 0.5):
            xy(pad_x, buttons_y):
            zoom(0.5)
        ));

        // Start (green circle)
        actors.push(act!(sprite("test_input/highlightgreen.png"):
            align(0.5, 0.5):
            xy(pad_x, start_y):
            zoom(0.5):
            diffuse(1.0, 1.0, 1.0, alpha_start)
        ));
        // Select (red circle)
        actors.push(act!(sprite("test_input/highlightred.png"):
            align(0.5, 0.5):
            xy(pad_x, select_y):
            zoom(0.5):
            diffuse(1.0, 1.0, 1.0, alpha_select)
        ));
        // MenuLeft arrow
        actors.push(act!(sprite("test_input/highlightarrow.png"):
            align(0.5, 0.5):
            xy(pad_x - menu_x_offset, menu_y):
            zoom(0.5):
            rotationz(180.0):
            diffuse(1.0, 1.0, 1.0, alpha_mleft)
        ));
        // MenuRight arrow
        actors.push(act!(sprite("test_input/highlightarrow.png"):
            align(0.5, 0.5):
            xy(pad_x + menu_x_offset, menu_y):
            zoom(0.5):
            diffuse(1.0, 1.0, 1.0, alpha_mright)
        ));
    }

    /* ---------------------- Unmapped device list text --------------------- */

    let lines = state.unmapped.active_lines();
    if !lines.is_empty() {
        let start_y = cy + 210.0;
        let line_h = 16.0;
        for (i, line) in lines.iter().enumerate() {
            actors.push(act!(text:
                font("miso"):
                settext(line.clone()):
                align(0.5, 0.0):
                xy(cx, start_y + (i as f32 * line_h)):
                zoom(0.8):
                horizalign(center)
            ));
        }
    } else {
        actors.push(act!(text:
            font("miso"):
            settext("Press any button on your dance pad or gamepad to test input."):
            align(0.5, 0.0):
            xy(cx, cy + 150.0):
            zoom(0.8):
            horizalign(center)
        ));
    }

    // Footer hint
    actors.push(act!(text:
        font("miso"):
        settext("Press START or BACK to return to Options."):
        align(0.5, 0.0):
        xy(cx, screen_height() - 40.0):
        zoom(0.8):
        horizalign(center)
    ));

    actors
}
