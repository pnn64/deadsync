use crate::act;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::*;
use crate::game::profile::{self, ActiveProfile};
use crate::game::scroll::ScrollSpeedSetting;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{self, Actor};
use crate::ui::color;
use crate::ui::components::screen_bar::{ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::components::{heart_bg, screen_bar};
use std::str::FromStr;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ------------------------------ layout ------------------------------- */
const ROW_H: f32 = 35.0;
const ROWS_VISIBLE: i32 = 9;
const FRAME_BASE_W: f32 = 200.0;
const FRAME_W_SCROLLER: f32 = FRAME_BASE_W * 1.1;
const FRAME_W_JOIN: f32 = FRAME_BASE_W * 0.9;
const FRAME_H: f32 = 214.0;
const FRAME_BORDER: f32 = 2.0;
const FRAME_CX_OFF: f32 = 150.0;

const INFO_W: f32 = FRAME_BASE_W * 0.475;
const INFO_X0_OFF: f32 = 15.5;
const INFO_PAD: f32 = 4.0;

const SCROLLER_W: f32 = FRAME_W_SCROLLER - INFO_W;
const SCROLLER_CX_OFF: f32 = -47.0;
const SCROLLER_TEXT_PAD_X: f32 = 6.0;

const PREVIEW_LABEL_H: f32 = 12.0;
const PREVIEW_VALUE_H: f32 = 16.0;

#[derive(Clone)]
struct Choice {
    kind: ActiveProfile,
    display_name: String,
    speed_mod: String,
}

pub struct State {
    pub active_color_index: i32,
    selected_index: usize,
    choices: Vec<Choice>,
    bg: heart_bg::State,
}

fn build_choices() -> Vec<Choice> {
    let mut out = Vec::new();

    let guest_speed_mod = format!("{}", crate::game::profile::Profile::default().scroll_speed);
    out.push(Choice {
        kind: ActiveProfile::Guest,
        display_name: "[ GUEST ]".to_string(),
        speed_mod: guest_speed_mod,
    });
    for p in profile::scan_local_profiles() {
        let mut speed_mod = String::new();
        let ini_path = std::path::Path::new("save/profiles")
            .join(&p.id)
            .join("profile.ini");
        let mut ini = crate::config::SimpleIni::new();
        if ini.load(&ini_path).is_ok()
            && let Some(raw) = ini.get("PlayerOptions", "ScrollSpeed")
        {
            let trimmed = raw.trim();
            speed_mod = if let Ok(setting) = ScrollSpeedSetting::from_str(trimmed) {
                format!("{}", setting)
            } else {
                trimmed.to_string()
            };
        }

        out.push(Choice {
            kind: ActiveProfile::Local { id: p.id },
            display_name: p.display_name,
            speed_mod,
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
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if state.selected_index > 0 {
                state.selected_index -= 1;
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
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

    let frame_h = FRAME_H;
    let cx = screen_center_x();
    let cy = screen_center_y();

    let frame_y0 = cy - frame_h * 0.5;

    let p1_cx = cx - FRAME_CX_OFF;
    let p2_cx = cx + FRAME_CX_OFF;

    let mut p1_color = color::decorative_rgba(state.active_color_index - 1);
    p1_color[3] *= 0.85;

    let border_alpha = 0.75;

    // P1 frame background
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p1_cx, cy):
        zoomto(FRAME_W_SCROLLER + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(0.0, 0.0, 0.0, border_alpha):
        z(100)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p1_cx, cy):
        zoomto(FRAME_W_SCROLLER, frame_h):
        diffuse(p1_color[0], p1_color[1], p1_color[2], p1_color[3]):
        z(101)
    ));

    // P1 info pane background
    let info_x0 = p1_cx + INFO_X0_OFF;
    let info_text_x = info_x0 + INFO_PAD * 1.25;
    let info_max_w = INFO_W - INFO_PAD * 2.5;

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(info_x0, frame_y0):
        zoomto(INFO_W, frame_h):
        diffuse(0.0, 0.0, 0.0, 0.5):
        z(102)
    ));

    // P2 join prompt (template only; not functional yet)
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p2_cx, cy):
        zoomto(FRAME_W_JOIN + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(0.0, 0.0, 0.0, border_alpha):
        z(100)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(p2_cx, cy):
        zoomto(FRAME_W_JOIN, frame_h):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(101)
    ));
    ui.push(act!(text:
        align(0.5, 0.5):
        xy(p2_cx, cy):
        font("miso"):
        zoomtoheight(18.0):
        maxwidth(FRAME_W_JOIN - 20.0):
        settext("Press START to join!"):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(103)
    ));

    // P1 scroller
    let scroller_cx = p1_cx + SCROLLER_CX_OFF;
    let scroller_x0 = scroller_cx - SCROLLER_W * 0.5;
    let highlight_h = ROW_H;

    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(scroller_cx, cy):
        zoomto(SCROLLER_W, highlight_h):
        diffuse(0.0, 0.0, 0.0, 0.5):
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
            xy(scroller_x0 + SCROLLER_TEXT_PAD_X, y):
            font("miso"):
            maxwidth(SCROLLER_W - SCROLLER_TEXT_PAD_X * 2.0):
            zoom(0.92):
            settext(choice.display_name.clone()):
            diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
            z(103)
        ));
    }

    let (selected_name, selected_speed) = state
        .choices
        .get(state.selected_index)
        .map(|c| (c.display_name.as_str(), c.speed_mod.as_str()))
        .unwrap_or(("[ GUEST ]", ""));

    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_text_x, frame_y0 + 10.0):
        font("miso"):
        zoomtoheight(PREVIEW_LABEL_H):
        settext("PROFILE"):
        diffuse(1.0, 1.0, 1.0, 0.65):
        z(103)
    ));
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_text_x, frame_y0 + 24.0):
        font("miso"):
        maxwidth(info_max_w):
        zoomtoheight(PREVIEW_VALUE_H):
        settext(selected_name.to_string()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(103)
    ));
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_text_x, frame_y0 + 52.0):
        font("miso"):
        zoomtoheight(PREVIEW_LABEL_H):
        settext("SPEED"):
        diffuse(1.0, 1.0, 1.0, 0.65):
        z(103)
    ));
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_text_x, frame_y0 + 66.0):
        font("miso"):
        maxwidth(info_max_w):
        zoomtoheight(PREVIEW_VALUE_H):
        settext(selected_speed.to_string()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(103)
    ));
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(info_text_x, frame_y0 + frame_h - 18.0):
        font("miso"):
        maxwidth(info_max_w):
        zoomtoheight(12.0):
        settext("Use ▲ ▼ to choose, then press START"):
        diffuse(1.0, 1.0, 1.0, 0.75):
        z(103)
    ));

    for mut a in ui {
        apply_alpha_to_actor(&mut a, alpha_multiplier);
        actors.push(a);
    }

    actors
}
