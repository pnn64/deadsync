use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, current_machine_font_key_for_text};
use crate::config;
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ScreenAction};
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_present::actors::Actor;
use deadsync_present::color;
use deadsync_present::space;
use deadsync_present::space::{screen_center_x, screen_height, screen_width};
use winit::keyboard::KeyCode;

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

const GUIDE_THICKNESS: f32 = 2.0;
const RED: [f32; 3] = [0.92, 0.25, 0.25];
const BLUE: [f32; 3] = [0.35, 0.6, 1.0];

/// Working values, ordered to match the rows shown on the overscan screen.
const FIELD_COUNT: usize = 4;
const IDX_ADD_HEIGHT: usize = 0;
const IDX_ADD_WIDTH: usize = 1;
const IDX_TRANSLATE_X: usize = 2;
const IDX_TRANSLATE_Y: usize = 3;

struct FieldInfo {
    /// i18n key (under `[ScreenOverscanAdjustment]`) for the field's label.
    label_key: &'static str,
    /// Key that increases the value (shown first in the hint).
    inc_key: &'static str,
    /// Key that decreases the value.
    dec_key: &'static str,
    /// Guide colour (red = vertical extent, blue = horizontal extent).
    color: [f32; 3],
}

const FIELDS: [FieldInfo; FIELD_COUNT] = [
    FieldInfo {
        label_key: "AddHeight",
        inc_key: "w",
        dec_key: "s",
        color: RED,
    },
    FieldInfo {
        label_key: "AddWidth",
        inc_key: "d",
        dec_key: "a",
        color: BLUE,
    },
    FieldInfo {
        label_key: "TranslateX",
        inc_key: "l",
        dec_key: "j",
        color: BLUE,
    },
    FieldInfo {
        label_key: "TranslateY",
        inc_key: "k",
        dec_key: "i",
        color: RED,
    },
];

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    /// Live working values: [add_height, add_width, translate_x, translate_y].
    values: [i32; FIELD_COUNT],
    /// Values present on entry, restored when the user cancels.
    initial: [i32; FIELD_COUNT],
    selected: usize,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        values: [0; FIELD_COUNT],
        initial: [0; FIELD_COUNT],
        selected: 0,
    }
}

pub fn on_enter(state: &mut State) {
    let cfg = config::get();
    state.values = [
        cfg.center_image_add_height,
        cfg.center_image_add_width,
        cfg.center_image_translate_x,
        cfg.center_image_translate_y,
    ];
    state.initial = state.values;
    state.selected = 0;
    apply_preview(state);
}

pub fn update(_state: &mut State, _dt: f32) -> Option<ScreenAction> {
    None
}

/// Raw keyboard handling for the per-field key bindings. Returns `true`
/// when the key was consumed so it does not also fire as a virtual menu action
/// (W/A/S/D overlap the P1 pad directions).
pub fn handle_raw_key_event(state: &mut State, ev: &RawKeyboardEvent) -> bool {
    if !ev.pressed {
        // Still consume key-up for our adjustment keys to keep behaviour tidy.
        return matches!(
            ev.code,
            KeyCode::KeyW
                | KeyCode::KeyS
                | KeyCode::KeyD
                | KeyCode::KeyA
                | KeyCode::KeyL
                | KeyCode::KeyJ
                | KeyCode::KeyK
                | KeyCode::KeyI
        );
    }
    let (field, delta) = match ev.code {
        KeyCode::KeyW => (IDX_ADD_HEIGHT, 1),
        KeyCode::KeyS => (IDX_ADD_HEIGHT, -1),
        KeyCode::KeyD => (IDX_ADD_WIDTH, 1),
        KeyCode::KeyA => (IDX_ADD_WIDTH, -1),
        KeyCode::KeyL => (IDX_TRANSLATE_X, 1),
        KeyCode::KeyJ => (IDX_TRANSLATE_X, -1),
        KeyCode::KeyK => (IDX_TRANSLATE_Y, 1),
        KeyCode::KeyI => (IDX_TRANSLATE_Y, -1),
        _ => return false,
    };
    state.selected = field;
    adjust(state, field, delta);
    true
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            // Commit working values to config (persists + keeps live preview).
            config::update_overscan(
                state.values[IDX_TRANSLATE_X],
                state.values[IDX_TRANSLATE_Y],
                state.values[IDX_ADD_WIDTH],
                state.values[IDX_ADD_HEIGHT],
            );
            ScreenAction::Navigate(Screen::Options)
        }
        VirtualAction::p1_back | VirtualAction::p2_back => {
            // Cancel: restore the values present on entry.
            state.values = state.initial;
            apply_preview(state);
            ScreenAction::Navigate(Screen::Options)
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            state.selected = (state.selected + FIELD_COUNT - 1) % FIELD_COUNT;
            ScreenAction::None
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            state.selected = (state.selected + 1) % FIELD_COUNT;
            ScreenAction::None
        }
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            adjust(state, state.selected, -1);
            ScreenAction::None
        }
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            adjust(state, state.selected, 1);
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

pub fn push_actors(actors: &mut Vec<Actor>, state: &State, alpha_mul: f32) {
    actors.reserve(24);
    let screen_w = screen_width();
    let screen_h = screen_height();

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul,
        },
    );

    // Edge guide lines (scale with the centering matrix). Red for the vertical
    // extent (top/bottom), blue for the horizontal extent (left/right).
    push_guides(actors, screen_w, screen_h, alpha_mul);

    // Title: machine Header font (Wendy) at the standard screen-title scale,
    // matching how every other screen renders its title.
    let title = tr("ScreenOverscanAdjustment", "HeaderText");
    let title_font = current_machine_font_key_for_text(FontRole::Header, &title);
    let title_scale = if space::is_wide() { 0.6 } else { 0.5 };
    actors.push(act!(text:
        font(title_font):
        settext(title):
        align(0.5, 0.5):
        xy(screen_center_x(), 28.0):
        zoom(title_scale):
        maxwidth(screen_w * 0.8):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.96 * alpha_mul):
        z(85)
    ));

    // Field rows.
    const ROW_SPACING: f32 = 42.0;
    let base_y = screen_h * 0.5 - ROW_SPACING * 1.5;
    for (idx, field) in FIELDS.iter().enumerate() {
        let selected = idx == state.selected;
        let row_alpha = if selected { 1.0 } else { 0.7 } * alpha_mul;
        let prefix = if selected { "> " } else { "  " };
        let text = format!(
            "{prefix}{} ({}/{}): {}",
            tr("ScreenOverscanAdjustment", field.label_key),
            field.inc_key,
            field.dec_key,
            state.values[idx]
        );
        actors.push(act!(text:
            font("miso"):
            settext(text):
            align(0.5, 0.5):
            xy(screen_center_x(), base_y + idx as f32 * ROW_SPACING):
            zoom(0.9):
            maxwidth(screen_w * 0.8):
            horizalign(center):
            diffuse(field.color[0], field.color[1], field.color[2], row_alpha):
            strokecolor(0.0, 0.0, 0.0, 0.75 * alpha_mul):
            shadowlength(1.0):
            z(86)
        ));
    }

    // Footer help.
    actors.push(act!(text:
        font("miso"):
        settext(tr("ScreenOverscanAdjustment", "Controls")):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_h - 22.0):
        zoom(0.74):
        maxwidth(screen_w * 0.92):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.74 * alpha_mul):
        z(90)
    ));
}

pub fn get_actors(state: &State, alpha_mul: f32) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(24);
    push_actors(&mut actors, state, alpha_mul);
    actors
}

fn push_guides(actors: &mut Vec<Actor>, screen_w: f32, screen_h: f32, alpha_mul: f32) {
    // Top (red).
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_w, GUIDE_THICKNESS):
        diffuse(RED[0], RED[1], RED[2], 0.95 * alpha_mul):
        z(80)
    ));
    // Bottom (red).
    actors.push(act!(quad:
        align(0.0, 1.0):
        xy(0.0, screen_h):
        zoomto(screen_w, GUIDE_THICKNESS):
        diffuse(RED[0], RED[1], RED[2], 0.95 * alpha_mul):
        z(80)
    ));
    // Left (blue).
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(GUIDE_THICKNESS, screen_h):
        diffuse(BLUE[0], BLUE[1], BLUE[2], 0.95 * alpha_mul):
        z(80)
    ));
    // Right (blue).
    actors.push(act!(quad:
        align(1.0, 0.0):
        xy(screen_w, 0.0):
        zoomto(GUIDE_THICKNESS, screen_h):
        diffuse(BLUE[0], BLUE[1], BLUE[2], 0.95 * alpha_mul):
        z(80)
    ));
}

fn adjust(state: &mut State, field: usize, delta: i32) {
    state.values[field] = state.values[field].saturating_add(delta);
    apply_preview(state);
}

/// Push the current working values to the live render mirror (no disk write).
fn apply_preview(state: &State) {
    space::set_overscan(
        state.values[IDX_TRANSLATE_X],
        state.values[IDX_TRANSLATE_Y],
        state.values[IDX_ADD_WIDTH],
        state.values[IDX_ADD_HEIGHT],
    );
}
