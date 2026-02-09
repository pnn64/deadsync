use crate::act;
use crate::core::input::{InputEvent, PadEvent, VirtualAction};
use crate::core::space::{screen_height, screen_width};
use crate::screens::components::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::screens::components::{heart_bg, screen_bar, test_input};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    test_input: test_input::State,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        test_input: test_input::State::default(),
    }
}

/* ------------------------------- update ------------------------------- */

pub const fn update(_state: &mut State, _dt: f32) {
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

    test_input::apply_virtual_input(&mut state.test_input, ev);
    ScreenAction::None
}

/// Raw pad events are used to approximate Simply Love's "unmapped" device list.
pub fn handle_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    test_input::apply_raw_pad_event(&mut state.test_input, pad_event);
}

/* ------------------------------- drawing ------------------------------- */

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

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
        right_avatar: None,
        fg_color: FG,
    }));

    actors.extend(test_input::build_test_input_screen_content(&state.test_input));
    actors
}
