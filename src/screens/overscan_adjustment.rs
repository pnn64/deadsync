use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, current_machine_font_key_for_text};
use crate::config;
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ScreenAction};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space;
use deadlib_present::space::{screen_center_x, screen_height, screen_width};
use deadsync_input::{InputEvent, RawKeyboardEvent};
use deadsync_screens::overscan::{
    Action as OverscanAction, Adjustment, Field, State as OverscanState, Values,
};
use winit::keyboard::KeyCode;

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

const GUIDE_THICKNESS: f32 = 2.0;
const RED: [f32; 3] = [0.92, 0.25, 0.25];
const BLUE: [f32; 3] = [0.35, 0.6, 1.0];

struct FieldInfo {
    field: Field,
    /// i18n key (under `[ScreenOverscanAdjustment]`) for the field's label.
    label_key: &'static str,
    /// Key that increases the value (shown first in the hint).
    inc_key: &'static str,
    /// Key that decreases the value.
    dec_key: &'static str,
    /// Guide colour (red = vertical extent, blue = horizontal extent).
    color: [f32; 3],
}

const FIELDS: [FieldInfo; deadsync_screens::overscan::FIELD_COUNT] = [
    FieldInfo {
        field: Field::AddHeight,
        label_key: "AddHeight",
        inc_key: "w",
        dec_key: "s",
        color: RED,
    },
    FieldInfo {
        field: Field::AddWidth,
        label_key: "AddWidth",
        inc_key: "d",
        dec_key: "a",
        color: BLUE,
    },
    FieldInfo {
        field: Field::TranslateX,
        label_key: "TranslateX",
        inc_key: "l",
        dec_key: "j",
        color: BLUE,
    },
    FieldInfo {
        field: Field::TranslateY,
        label_key: "TranslateY",
        inc_key: "k",
        dec_key: "i",
        color: RED,
    },
];

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    edit: OverscanState,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        edit: OverscanState::new(Values::default()),
    }
}

pub fn on_enter(state: &mut State) {
    let cfg = config::get();
    let values = Values {
        add_height: cfg.center_image_add_height,
        add_width: cfg.center_image_add_width,
        translate_x: cfg.center_image_translate_x,
        translate_y: cfg.center_image_translate_y,
    };
    state.edit.reset(values);
    apply_preview(values);
}

pub fn update(_state: &mut State, _dt: f32) -> Option<ScreenAction> {
    None
}

/// Raw keyboard handling for the per-field key bindings. Returns `true`
/// when the key was consumed so it does not also fire as a virtual menu action
/// (W/A/S/D overlap the P1 pad directions).
pub fn handle_raw_key_event(state: &mut State, ev: &RawKeyboardEvent) -> bool {
    let adjustment = match ev.code {
        KeyCode::KeyW => Adjustment::new(Field::AddHeight, 1),
        KeyCode::KeyS => Adjustment::new(Field::AddHeight, -1),
        KeyCode::KeyD => Adjustment::new(Field::AddWidth, 1),
        KeyCode::KeyA => Adjustment::new(Field::AddWidth, -1),
        KeyCode::KeyL => Adjustment::new(Field::TranslateX, 1),
        KeyCode::KeyJ => Adjustment::new(Field::TranslateX, -1),
        KeyCode::KeyK => Adjustment::new(Field::TranslateY, 1),
        KeyCode::KeyI => Adjustment::new(Field::TranslateY, -1),
        _ => return false,
    };
    if ev.pressed {
        apply_preview(deadsync_screens::overscan::apply_adjustment(
            &mut state.edit,
            adjustment,
        ));
    }
    true
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    match deadsync_screens::overscan::handle_input(&mut state.edit, ev) {
        OverscanAction::None => ScreenAction::None,
        OverscanAction::Preview(values) => {
            apply_preview(values);
            ScreenAction::None
        }
        OverscanAction::Commit(values) => {
            config::update_overscan(
                values.translate_x,
                values.translate_y,
                values.add_width,
                values.add_height,
            );
            ScreenAction::Navigate(Screen::Options)
        }
        OverscanAction::Cancel(values) => {
            apply_preview(values);
            ScreenAction::Navigate(Screen::Options)
        }
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
        let selected = field.field == state.edit.selected();
        let row_alpha = if selected { 1.0 } else { 0.7 } * alpha_mul;
        let prefix = if selected { "> " } else { "  " };
        let text = format!(
            "{prefix}{} ({}/{}): {}",
            tr("ScreenOverscanAdjustment", field.label_key),
            field.inc_key,
            field.dec_key,
            state.edit.values().get(field.field)
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

/// Push the current working values to the live render mirror (no disk write).
fn apply_preview(values: Values) {
    space::set_overscan(
        values.translate_x,
        values.translate_y,
        values.add_width,
        values.add_height,
    );
}
