use crate::act;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::*;
use crate::game::profile::{self, ActiveProfile};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{self, Actor};
use crate::ui::color;
use crate::ui::components::screen_bar::{ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::components::{heart_bg, screen_bar};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ------------------------------ layout ------------------------------- */
const ROW_H: f32 = 35.0;
const ROWS_VISIBLE: i32 = 9;
const PANEL_W_4_3: f32 = 520.0;
const PANEL_W_16_9: f32 = 720.0;
const PANEL_H: f32 = 250.0;
const PANEL_BORDER: f32 = 2.0;
const PANEL_PAD: f32 = 14.0;
const LIST_W_FRAC: f32 = 0.44;

#[derive(Clone)]
struct Choice {
    kind: ActiveProfile,
    display_name: String,
}

pub struct State {
    pub active_color_index: i32,
    selected_index: usize,
    choices: Vec<Choice>,
    bg: heart_bg::State,
}

fn build_choices() -> Vec<Choice> {
    let mut out = Vec::new();
    out.push(Choice {
        kind: ActiveProfile::Guest,
        display_name: "[ GUEST ]".to_string(),
    });
    for p in profile::scan_local_profiles() {
        out.push(Choice {
            kind: ActiveProfile::Local { id: p.id },
            display_name: p.display_name,
        });
    }
    out
}

pub fn init() -> State {
    let choices = build_choices();
    let active = profile::get_active_profile();

    let mut selected_index = 0usize;
    if let ActiveProfile::Local { id } = active {
        if let Some(i) = choices.iter().position(|c| match &c.kind {
            ActiveProfile::Local { id: cid } => cid == &id,
            ActiveProfile::Guest => false,
        }) {
            selected_index = i;
        }
    }

    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selected_index,
        choices,
        bg: heart_bg::State::new(),
    }
}

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

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if state.selected_index > 0 {
                state.selected_index -= 1;
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if state.selected_index + 1 < state.choices.len() {
                state.selected_index += 1;
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p1_start => {
            audio::play_sfx("assets/sounds/start.ogg");
            let choice = state
                .choices
                .get(state.selected_index)
                .map(|c| c.kind.clone())
                .unwrap_or(ActiveProfile::Guest);
            ScreenAction::SelectProfile(choice)
        }
        VirtualAction::p1_back | VirtualAction::p1_select => ScreenAction::Navigate(Screen::Menu),
        _ => ScreenAction::None,
    }
}

fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
        Actor::Frame {
            background,
            children,
            ..
        } => {
            if let Some(actors::Background::Color(c)) = background {
                c[3] *= alpha;
            }
            for child in children {
                apply_alpha_to_actor(child, alpha);
            }
        }
        Actor::Shadow { color, child, .. } => {
            color[3] *= alpha;
            apply_alpha_to_actor(child, alpha);
        }
    }
}

pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(128);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui: Vec<Actor> = Vec::new();

    let fg = [1.0, 1.0, 1.0, 1.0];

    ui.push(screen_bar::build(ScreenBarParams {
        title: "SELECT PROFILE",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: fg,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
    }));
    ui.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: fg,
        left_text: None,
        center_text: None,
        right_text: Some("PRESS START"),
        left_avatar: None,
    }));

    let panel_w = widescale(PANEL_W_4_3, PANEL_W_16_9);
    let panel_h = PANEL_H;
    let cx = screen_center_x();
    let cy = screen_center_y();
    let panel_x0 = cx - panel_w * 0.5;
    let panel_y0 = cy - panel_h * 0.5;

    let mut panel_color = color::decorative_rgba(state.active_color_index);
    panel_color[3] *= 0.85;
    let border_alpha = 0.75;

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(panel_x0 - PANEL_BORDER, panel_y0 - PANEL_BORDER):
        zoomto(panel_w + PANEL_BORDER * 2.0, panel_h + PANEL_BORDER * 2.0):
        diffuse(0.0, 0.0, 0.0, border_alpha):
        z(100)
    ));
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(panel_x0, panel_y0):
        zoomto(panel_w, panel_h):
        diffuse(panel_color[0], panel_color[1], panel_color[2], panel_color[3]):
        z(101)
    ));

    let list_w = panel_w * LIST_W_FRAC;
    let list_x0 = panel_x0 + PANEL_PAD;
    let info_x0 = panel_x0 + list_w + PANEL_PAD * 2.0;
    let highlight_w = list_w - PANEL_PAD * 2.0;
    let highlight_h = ROW_H;

    ui.push(act!(quad:
        align(0.0, 0.5):
        xy(list_x0, cy):
        zoomto(highlight_w, highlight_h):
        diffuse(0.0, 0.0, 0.0, 0.45):
        z(102)
    ));

    let rows_half = ROWS_VISIBLE / 2;
    for d in -rows_half..=rows_half {
        let idx_i = state.selected_index as i32 + d;
        if idx_i < 0 || idx_i >= state.choices.len() as i32 {
            continue;
        }
        let choice = &state.choices[idx_i as usize];
        let y = cy + d as f32 * ROW_H;

        let a = 1.0 - (d.abs() as f32 / (rows_half as f32 + 1.0));
        let mut text_color = [1.0, 1.0, 1.0, 0.35 + 0.65 * a];
        if d == 0 {
            text_color = color::menu_selected_rgba(state.active_color_index);
        }

        ui.push(act!(text:
            align(0.0, 0.5):
            xy(list_x0 + 6.0, y):
            font("miso"):
            zoom(0.92):
            settext(choice.display_name.clone()):
            diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
            z(103)
        ));
    }

    let selected_name = state
        .choices
        .get(state.selected_index)
        .map(|c| c.display_name.as_str())
        .unwrap_or("[ GUEST ]");

    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_x0, panel_y0 + PANEL_PAD):
        font("wendy"):
        zoom(0.9):
        settext("PROFILE"):
        diffuse(1.0, 1.0, 1.0, 0.65):
        z(103)
    ));
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_x0, panel_y0 + PANEL_PAD + 22.0):
        font("wendy"):
        zoom(1.15):
        settext(selected_name.to_string()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(103)
    ));
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_x0, panel_y0 + panel_h - PANEL_PAD - 18.0):
        font("miso"):
        zoom(0.75):
        settext("Use ◄ ► to choose, then press START"):
        diffuse(1.0, 1.0, 1.0, 0.75):
        z(103)
    ));

    for mut a in ui {
        apply_alpha_to_actor(&mut a, alpha_multiplier);
        actors.push(a);
    }

    actors
}
