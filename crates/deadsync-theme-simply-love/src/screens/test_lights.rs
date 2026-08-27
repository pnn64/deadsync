use crate::act;
use crate::assets::i18n::{self, tr, tr_fmt};
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ThemeEffect};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_theme::views::LightsTestView;
use std::cell::RefCell;
use std::sync::Arc;

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
enum LightPlayer {
    P1,
    P2,
}

impl LightPlayer {
    const fn ix(self) -> usize {
        match self {
            Self::P1 => 0,
            Self::P2 => 1,
        }
    }
}

#[derive(Clone, Copy)]
enum CabinetLight {
    MarqueeUpperLeft,
    MarqueeUpperRight,
    MarqueeLowerLeft,
    MarqueeLowerRight,
    BassLeft,
    BassRight,
}

impl CabinetLight {
    const fn ix(self) -> usize {
        match self {
            Self::MarqueeUpperLeft => 0,
            Self::MarqueeUpperRight => 1,
            Self::MarqueeLowerLeft => 2,
            Self::MarqueeLowerRight => 3,
            Self::BassLeft => 4,
            Self::BassRight => 5,
        }
    }
}

#[derive(Clone, Copy)]
enum ButtonLight {
    Left,
    Down,
    Up,
    Right,
    Start,
    Select,
}

impl ButtonLight {
    const fn ix(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Down => 1,
            Self::Up => 2,
            Self::Right => 3,
            Self::Start => 4,
            Self::Select => 5,
        }
    }
}

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
    text: RefCell<LightsText>,
}

#[must_use]
pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        manual_elapsed: 0.0,
        manual_active: false,
        text: RefCell::new(LightsText::build(LightsTestView::default())),
    }
}

pub const fn on_enter(state: &mut State) {
    state.manual_elapsed = 0.0;
    state.manual_active = false;
}

pub fn update(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    if !state.manual_active {
        return None;
    }
    state.manual_elapsed += dt.max(0.0);
    if state.manual_elapsed < MANUAL_RETURN_SECONDS {
        return None;
    }
    state.manual_elapsed = 0.0;
    state.manual_active = false;
    Some(ThemeEffect::Runtime(
        crate::SimplyLoveRuntimeRequest::Hardware(crate::SimplyLoveHardwareRequest::TestLightsAuto),
    ))
}

pub const fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if !ev.pressed {
        return ThemeEffect::None;
    }

    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back => ThemeEffect::Navigate(Screen::Options),
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            set_manual(state);
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Hardware(
                crate::SimplyLoveHardwareRequest::StepTestCabinet(-1),
            ))
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            set_manual(state);
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Hardware(
                crate::SimplyLoveHardwareRequest::StepTestCabinet(1),
            ))
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            set_manual(state);
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Hardware(
                crate::SimplyLoveHardwareRequest::StepTestButton(-1),
            ))
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            set_manual(state);
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Hardware(
                crate::SimplyLoveHardwareRequest::StepTestButton(1),
            ))
        }
        _ => ThemeEffect::None,
    }
}

#[must_use]
pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

#[must_use]
pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    lights: LightsTestView,
    alpha_mul: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(44);
    let screen_w = screen_width();
    let screen_h = screen_height();
    let root_x = screen_center_x();
    let root_y = screen_center_y() + ROOT_Y_OFFSET;

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul,
            visual_policy,
        },
    );

    actors.push(act!(sprite(CABINET_TEX):
        align(0.5, 0.5):
        xy(root_x, root_y):
        zoom(CABINET_ZOOM):
        diffuse(1.0, 1.0, 1.0, 0.92 * alpha_mul):
        z(20)
    ));

    for highlight in CABINET_HIGHLIGHTS {
        if !lights.cabinet[highlight.light.ix()] {
            continue;
        }
        actors.push(act!(sprite(highlight.texture):
            align(0.5, 0.5):
            xy(
                highlight.x.mul_add(CABINET_ZOOM, root_x),
                highlight.y.mul_add(CABINET_ZOOM, root_y)
            ):
            zoom(highlight.zoom * CABINET_ZOOM):
            diffuse(1.0, 1.0, 1.0, alpha_mul):
            z(30)
        ));
    }

    push_pad(actors, lights, LightPlayer::P1, root_x, root_y, alpha_mul);
    push_pad(actors, lights, LightPlayer::P2, root_x, root_y, alpha_mul);
    let mut text = state.text.borrow_mut();
    text.sync(lights);
    push_labels(actors, &text, state.active_color_index, alpha_mul);

    actors.push(act!(quad:
        align(0.0, 1.0):
        xy(0.0, screen_h):
        zoomto(screen_w, 40.0):
        diffuse(0.0, 0.0, 0.0, 0.52 * alpha_mul):
        z(80)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(Arc::clone(&text.controls)):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_h - 20.0):
        zoom(0.62):
        maxwidth(screen_w * 0.9):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.74 * alpha_mul):
        z(90)
    ));
}

pub fn get_actors(state: &State, lights: LightsTestView, alpha_mul: f32) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(44);
    push_actors(
        &mut actors,
        state,
        lights,
        alpha_mul,
        crate::views::SimplyLoveVisualPolicyView::default(),
    );
    actors
}

const fn set_manual(state: &mut State) {
    state.manual_active = true;
    state.manual_elapsed = 0.0;
}

fn push_pad(
    actors: &mut Vec<Actor>,
    lights: LightsTestView,
    player: LightPlayer,
    root_x: f32,
    root_y: f32,
    alpha_mul: f32,
) {
    let side_x = match player {
        LightPlayer::P1 => P1_PAD_X,
        LightPlayer::P2 => P2_PAD_X,
    };
    let origin_x = side_x.mul_add(PAD_ZOOM, root_x);
    let origin_y = root_y + PAD_FRAME_Y;

    actors.push(act!(sprite(PAD_TEX):
        align(0.5, 0.5):
        xy(origin_x, origin_y):
        zoom(PAD_ZOOM):
        diffuse(1.0, 1.0, 1.0, 0.95 * alpha_mul):
        z(40)
    ));

    for highlight in PANEL_HIGHLIGHTS {
        if !lights.buttons[player.ix()][highlight.button.ix()] {
            continue;
        }
        actors.push(act!(sprite(PANEL_HIGHLIGHT_TEX):
            align(0.5, 0.5):
            xy(
                (side_x + highlight.x).mul_add(PAD_ZOOM, root_x),
                highlight.y.mul_add(PAD_ZOOM, origin_y)
            ):
            zoom(PAD_ZOOM):
            diffuse(1.0, 1.0, 1.0, alpha_mul):
            z(50)
        ));
    }

    let start_on = lights.buttons[player.ix()][ButtonLight::Start.ix()];
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
    text: &LightsText,
    active_color_index: i32,
    alpha_mul: f32,
) {
    let screen_w = screen_width();
    let accent = color::DECORATIVE_RGBA
        [active_color_index.rem_euclid(color::DECORATIVE_RGBA.len() as i32) as usize];
    let info_x = (screen_center_x() + 245.0).min(screen_w - 210.0);

    actors.push(act!(text:
        font("miso"):
        settext(Arc::clone(&text.title)):
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

    for (idx, row) in text.rows.iter().enumerate() {
        actors.push(act!(text:
            font("miso"):
            settext(Arc::clone(row)):
            align(0.5, 0.5):
            xy(info_x, (idx as f32).mul_add(28.0, 92.0)):
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

fn cabinet_name(lights: LightsTestView) -> Option<&'static str> {
    for light in [
        CabinetLight::MarqueeUpperLeft,
        CabinetLight::MarqueeUpperRight,
        CabinetLight::MarqueeLowerLeft,
        CabinetLight::MarqueeLowerRight,
        CabinetLight::BassLeft,
        CabinetLight::BassRight,
    ] {
        if lights.cabinet[light.ix()] {
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

fn active_button_text(lights: LightsTestView) -> Arc<str> {
    for player in [LightPlayer::P1, LightPlayer::P2] {
        for button in [
            ButtonLight::Left,
            ButtonLight::Down,
            ButtonLight::Up,
            ButtonLight::Right,
            ButtonLight::Start,
            ButtonLight::Select,
        ] {
            if lights.buttons[player.ix()][button.ix()] {
                return Arc::from(format!("{} {}", player_name(player), button_name(button)));
            }
        }
    }
    tr("ScreenTestLights", "None")
}

/// Screen-lifetime actor-ready text retained by the game thread.
///
/// Owner/thread model: the Test Lights state on the single game thread, with
/// `RefCell` only because actor construction receives `&State`. Lifetime and
/// capacity: one fixed five-string entry for the screen. Warmup: screen
/// initialization. Miss behavior: a hardware-light state or language revision
/// change rebuilds the entry on that diagnostic-screen frame; gameplay never
/// touches this screen. Eviction/destruction: whole-entry replacement and
/// normal screen teardown. The fixed domain needs no pruning or counters;
/// worst-case refresh work is a bounded 6-by-12 scan and three formats.
struct LightsText {
    i18n_revision: u64,
    lights: LightsTestView,
    title: Arc<str>,
    rows: [Arc<str>; 3],
    controls: Arc<str>,
}

impl LightsText {
    fn build(lights: LightsTestView) -> Self {
        let mode = if lights.manual_cycle {
            tr("ScreenTestLights", "ManualCycle")
        } else {
            tr("ScreenTestLights", "AutoCycle")
        };
        let cabinet = tr("ScreenTestLights", cabinet_name(lights).unwrap_or("None"));
        let pad = active_button_text(lights);
        Self {
            i18n_revision: i18n::revision(),
            lights,
            title: tr("ScreenTestLights", "HeaderText"),
            rows: [
                tr_fmt("ScreenTestLights", "ModeLine", &[("mode", mode.as_ref())]),
                tr_fmt(
                    "ScreenTestLights",
                    "CabinetLine",
                    &[("cabinet", cabinet.as_ref())],
                ),
                tr_fmt("ScreenTestLights", "PadLine", &[("pad", pad.as_ref())]),
            ],
            controls: tr("ScreenTestLights", "Controls"),
        }
    }

    #[inline]
    fn sync(&mut self, lights: LightsTestView) {
        if self.lights != lights || self.i18n_revision != i18n::revision() {
            *self = Self::build(lights);
        }
    }

    #[cfg(any(test, feature = "bench-support"))]
    fn checksum(&self) -> u64 {
        [
            &self.title,
            &self.rows[0],
            &self.rows[1],
            &self.rows[2],
            &self.controls,
        ]
        .into_iter()
        .fold(0, |checksum, text| {
            text.bytes()
                .fold(checksum ^ text.len() as u64, |hash, byte| {
                    hash.rotate_left(5) ^ u64::from(byte)
                })
        })
    }
}

#[cfg(any(test, feature = "bench-support"))]
pub struct LightsTextBenchmark {
    lights: LightsTestView,
    text: LightsText,
}

#[cfg(any(test, feature = "bench-support"))]
impl LightsTextBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let lights = LightsTestView {
            cabinet: [false, true, false, false, false, false],
            buttons: [
                [false, false, false, false, false, false],
                [false, false, true, false, false, false],
            ],
            manual_cycle: true,
        };
        Self {
            lights,
            text: LightsText::build(lights),
        }
    }

    #[must_use]
    pub fn legacy_frame(&self) -> u64 {
        LightsText::build(self.lights).checksum()
    }

    pub fn current_frame(&mut self) -> u64 {
        self.text.sync(self.lights);
        let values = [
            Arc::clone(&self.text.title),
            Arc::clone(&self.text.rows[0]),
            Arc::clone(&self.text.rows[1]),
            Arc::clone(&self.text.rows[2]),
            Arc::clone(&self.text.controls),
        ];
        values.into_iter().fold(0, |checksum, text| {
            text.bytes()
                .fold(checksum ^ text.len() as u64, |hash, byte| {
                    hash.rotate_left(5) ^ u64::from(byte)
                })
        })
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for LightsTextBenchmark {
    fn default() -> Self {
        Self::new()
    }
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
        ButtonLight::Select => "Select",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_lights_text_matches_immediate_text_for_all_inputs() {
        let mut retained = LightsText::build(LightsTestView::default());
        let mut cases = vec![LightsTestView::default()];
        for cabinet in 0..6 {
            let mut lights = LightsTestView::default();
            lights.cabinet[cabinet] = true;
            lights.manual_cycle = cabinet % 2 == 0;
            cases.push(lights);
        }
        for player in 0..2 {
            for button in 0..6 {
                let mut lights = LightsTestView::default();
                lights.buttons[player][button] = true;
                lights.manual_cycle = button % 2 == 1;
                cases.push(lights);
            }
        }

        for lights in cases {
            retained.sync(lights);
            assert_eq!(retained.checksum(), LightsText::build(lights).checksum());
        }
    }

    #[test]
    fn stable_lights_benchmark_matches_immediate_builder() {
        let mut benchmark = LightsTextBenchmark::new();
        assert_eq!(benchmark.legacy_frame(), benchmark.current_frame());
    }
}
