use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::lights::{
    ButtonLight, CabinetLight, Mode as LightMode, Player as LightPlayer, State as LightState,
};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ScreenAction};

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const MANUAL_RETURN_SECONDS: f32 = 20.0;

const ROOT_Y_OFFSET: f32 = -70.0;
const CABINET_ZOOM: f32 = 0.2;
const PAD_FRAME_Y: f32 = 210.0;
const PAD_ZOOM: f32 = 0.55;
const P1_PAD_X: f32 = -135.0;
const P2_PAD_X: f32 = 135.0;

const CABINET_TEX: &str = "test_lights/cabinet ITG2.png";
const PAD_TEX: &str = "test_lights/dance.png";
const PANEL_HIGHLIGHT_TEX: &str = "test_lights/highlight.png";

#[derive(Clone, Copy)]
struct CabinetHighlight {
    light: CabinetLight,
    x: f32,
    y: f32,
    zoom: f32,
    texture: &'static str,
}

const CABINET_HIGHLIGHTS: [CabinetHighlight; 6] = [
    CabinetHighlight {
        light: CabinetLight::MarqueeUpperLeft,
        x: -278.0,
        y: -587.0,
        zoom: 0.6,
        texture: "test_lights/red.png",
    },
    CabinetHighlight {
        light: CabinetLight::MarqueeUpperRight,
        x: 278.0,
        y: -587.0,
        zoom: 0.6,
        texture: "test_lights/blue.png",
    },
    CabinetHighlight {
        light: CabinetLight::MarqueeLowerLeft,
        x: -278.0,
        y: -409.0,
        zoom: 0.6,
        texture: "test_lights/white.png",
    },
    CabinetHighlight {
        light: CabinetLight::MarqueeLowerRight,
        x: 278.0,
        y: -409.0,
        zoom: 0.6,
        texture: "test_lights/pink.png",
    },
    CabinetHighlight {
        light: CabinetLight::BassLeft,
        x: -230.0,
        y: 433.0,
        zoom: 0.6,
        texture: "test_lights/bass light (blue).png",
    },
    CabinetHighlight {
        light: CabinetLight::BassRight,
        x: 230.0,
        y: 433.0,
        zoom: 0.6,
        texture: "test_lights/bass light (blue).png",
    },
];

#[derive(Clone, Copy)]
struct PanelHighlight {
    button: ButtonLight,
    x: f32,
    y: f32,
}

const PANEL_HIGHLIGHTS: [PanelHighlight; 4] = [
    PanelHighlight {
        button: ButtonLight::Up,
        x: 0.0,
        y: -84.0,
    },
    PanelHighlight {
        button: ButtonLight::Left,
        x: -84.0,
        y: 0.0,
    },
    PanelHighlight {
        button: ButtonLight::Right,
        x: 84.0,
        y: 0.0,
    },
    PanelHighlight {
        button: ButtonLight::Down,
        x: 0.0,
        y: 84.0,
    },
];

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    manual_elapsed: f32,
    manual_active: bool,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        manual_elapsed: 0.0,
        manual_active: false,
    }
}

pub fn on_enter(state: &mut State) {
    state.manual_elapsed = 0.0;
    state.manual_active = false;
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    if !state.manual_active {
        return None;
    }
    state.manual_elapsed += dt.max(0.0);
    if state.manual_elapsed < MANUAL_RETURN_SECONDS {
        return None;
    }
    state.manual_elapsed = 0.0;
    state.manual_active = false;
    Some(ScreenAction::TestLightsSetAuto)
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back => ScreenAction::Navigate(Screen::Options),
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            set_manual(state);
            ScreenAction::TestLightsStepCabinet(-1)
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            set_manual(state);
            ScreenAction::TestLightsStepCabinet(1)
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            set_manual(state);
            ScreenAction::TestLightsStepButton(-1)
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            set_manual(state);
            ScreenAction::TestLightsStepButton(1)
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

pub fn get_actors(
    state: &State,
    lights: LightState,
    mode: LightMode,
    alpha_mul: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(44);
    let screen_w = screen_width();
    let screen_h = screen_height();
    let root_x = screen_center_x();
    let root_y = screen_center_y() + ROOT_Y_OFFSET;

    actors.extend(state.bg.build(visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul,
    }));

    actors.push(act!(sprite(CABINET_TEX):
        align(0.5, 0.5):
        xy(root_x, root_y):
        zoom(CABINET_ZOOM):
        diffuse(1.0, 1.0, 1.0, 0.92 * alpha_mul):
        z(20)
    ));

    for highlight in CABINET_HIGHLIGHTS {
        if !lights.cabinet(highlight.light) {
            continue;
        }
        actors.push(act!(sprite(highlight.texture):
            align(0.5, 0.5):
            xy(
                root_x + highlight.x * CABINET_ZOOM,
                root_y + highlight.y * CABINET_ZOOM
            ):
            zoom(highlight.zoom * CABINET_ZOOM):
            diffuse(1.0, 1.0, 1.0, alpha_mul):
            z(30)
        ));
    }

    push_pad(
        &mut actors,
        lights,
        LightPlayer::P1,
        root_x,
        root_y,
        alpha_mul,
    );
    push_pad(
        &mut actors,
        lights,
        LightPlayer::P2,
        root_x,
        root_y,
        alpha_mul,
    );
    push_labels(
        &mut actors,
        lights,
        mode,
        state.active_color_index,
        alpha_mul,
    );

    actors.push(act!(quad:
        align(0.0, 1.0):
        xy(0.0, screen_h):
        zoomto(screen_w, 40.0):
        diffuse(0.0, 0.0, 0.0, 0.52 * alpha_mul):
        z(80)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(tr("ScreenTestLights", "Controls")):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_h - 20.0):
        zoom(0.62):
        maxwidth(screen_w * 0.9):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.74 * alpha_mul):
        z(90)
    ));

    actors
}

fn set_manual(state: &mut State) {
    state.manual_active = true;
    state.manual_elapsed = 0.0;
}

fn push_pad(
    actors: &mut Vec<Actor>,
    lights: LightState,
    player: LightPlayer,
    root_x: f32,
    root_y: f32,
    alpha_mul: f32,
) {
    let side_x = match player {
        LightPlayer::P1 => P1_PAD_X,
        LightPlayer::P2 => P2_PAD_X,
    };
    let origin_x = root_x + side_x * PAD_ZOOM;
    let origin_y = root_y + PAD_FRAME_Y;

    actors.push(act!(sprite(PAD_TEX):
        align(0.5, 0.5):
        xy(origin_x, origin_y):
        zoom(PAD_ZOOM):
        diffuse(1.0, 1.0, 1.0, 0.95 * alpha_mul):
        z(40)
    ));

    for highlight in PANEL_HIGHLIGHTS {
        if !lights.button(player, highlight.button) {
            continue;
        }
        actors.push(act!(sprite(PANEL_HIGHLIGHT_TEX):
            align(0.5, 0.5):
            xy(
                root_x + (side_x + highlight.x) * PAD_ZOOM,
                origin_y + highlight.y * PAD_ZOOM
            ):
            zoom(PAD_ZOOM):
            diffuse(1.0, 1.0, 1.0, alpha_mul):
            z(50)
        ));
    }

    let start_on = lights.button(player, ButtonLight::Start);
    let start_alpha = if start_on { 0.96 } else { 0.28 } * alpha_mul;
    let label = match player {
        LightPlayer::P1 => "P1 START",
        LightPlayer::P2 => "P2 START",
    };
    actors.push(act!(text:
        font("miso"):
        settext(label):
        align(0.5, 0.5):
        xy(origin_x, origin_y - 88.0):
        zoom(0.42):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, start_alpha):
        strokecolor(0.0, 0.0, 0.0, 0.72 * alpha_mul):
        shadowlength(1.0):
        z(55)
    ));
}

fn push_labels(
    actors: &mut Vec<Actor>,
    lights: LightState,
    mode: LightMode,
    active_color_index: i32,
    alpha_mul: f32,
) {
    let screen_w = screen_width();
    let accent = color::DECORATIVE_RGBA
        [active_color_index.rem_euclid(color::DECORATIVE_RGBA.len() as i32) as usize];
    let mode_text = match mode {
        LightMode::TestManualCycle => tr("ScreenTestLights", "ManualCycle"),
        _ => tr("ScreenTestLights", "AutoCycle"),
    };
    let cabinet = tr("ScreenTestLights", cabinet_name(lights).unwrap_or("None"));
    let pad = active_button_text(lights);
    let info_x = (screen_center_x() + 245.0).min(screen_w - 210.0);
    let title = tr("ScreenTestLights", "HeaderText");
    let mode_line = tr_fmt(
        "ScreenTestLights",
        "ModeLine",
        &[("mode", mode_text.as_ref())],
    );
    let cabinet_line = tr_fmt(
        "ScreenTestLights",
        "CabinetLine",
        &[("cabinet", cabinet.as_ref())],
    );
    let pad_line = tr_fmt("ScreenTestLights", "PadLine", &[("pad", pad.as_str())]);

    actors.push(act!(text:
        font("miso"):
        settext(title):
        align(0.5, 0.5):
        xy(screen_center_x(), 28.0):
        zoom(1.0):
        maxwidth(screen_w * 0.72):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.96 * alpha_mul):
        strokecolor(accent[0], accent[1], accent[2], 0.8 * alpha_mul):
        shadowlength(1.0):
        z(85)
    ));

    let rows = [mode_line, cabinet_line, pad_line];
    for (idx, text) in rows.into_iter().enumerate() {
        actors.push(act!(text:
            font("miso"):
            settext(text):
            align(0.5, 0.5):
            xy(info_x, 92.0 + idx as f32 * 28.0):
            zoom(0.66):
            maxwidth(188.0):
            horizalign(left):
            diffuse(1.0, 1.0, 1.0, 0.86 * alpha_mul):
            strokecolor(0.0, 0.0, 0.0, 0.75 * alpha_mul):
            shadowlength(1.0):
            z(85)
        ));
    }
}

fn cabinet_name(lights: LightState) -> Option<&'static str> {
    for light in [
        CabinetLight::MarqueeUpperLeft,
        CabinetLight::MarqueeUpperRight,
        CabinetLight::MarqueeLowerLeft,
        CabinetLight::MarqueeLowerRight,
        CabinetLight::BassLeft,
        CabinetLight::BassRight,
    ] {
        if lights.cabinet(light) {
            return Some(match light {
                CabinetLight::MarqueeUpperLeft => "MarqueeUpLeft",
                CabinetLight::MarqueeUpperRight => "MarqueeUpRight",
                CabinetLight::MarqueeLowerLeft => "MarqueeLrLeft",
                CabinetLight::MarqueeLowerRight => "MarqueeLrRight",
                CabinetLight::BassLeft => "BassLeft",
                CabinetLight::BassRight => "BassRight",
            });
        }
    }
    None
}

fn active_button_text(lights: LightState) -> String {
    for player in [LightPlayer::P1, LightPlayer::P2] {
        for button in [
            ButtonLight::Left,
            ButtonLight::Down,
            ButtonLight::Up,
            ButtonLight::Right,
            ButtonLight::Start,
        ] {
            if lights.button(player, button) {
                return format!("{} {}", player_name(player), button_name(button));
            }
        }
    }
    tr("ScreenTestLights", "None").to_string()
}

const fn player_name(player: LightPlayer) -> &'static str {
    match player {
        LightPlayer::P1 => "P1",
        LightPlayer::P2 => "P2",
    }
}

const fn button_name(button: ButtonLight) -> &'static str {
    match button {
        ButtonLight::Left => "Left",
        ButtonLight::Down => "Down",
        ButtonLight::Up => "Up",
        ButtonLight::Right => "Right",
        ButtonLight::Start => "Start",
    }
}
