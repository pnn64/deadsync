use crate::act;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::*;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::components::screen_bar::{
    AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::ui::components::{heart_bg, screen_bar};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ------------------------------ layout ------------------------------- */
const CHOICE_COUNT: usize = 3;
const CHOICE_ZOOM_UNFOCUSED: f32 = 0.5;
const CHOICE_ZOOM_FOCUSED: f32 = 1.0;
const CHOICE_ZOOM_TWEEN_DURATION: f32 = 0.125;
const PAD_TILE_NATIVE_SIZE: f32 = 64.0;
const PAD_TILE_ZOOM_4_3: f32 = 0.435;
const PAD_TILE_ZOOM_16_9: f32 = 0.525;
const PAD_DUAL_OFFSET_4_3: f32 = 42.0;
const PAD_DUAL_OFFSET_16_9: f32 = 51.0;
const CHOICE_X_OFFSET_4_3: f32 = 160.0;
const CHOICE_X_OFFSET_16_9: f32 = 214.0;
const CHOICE_Y_OFFSET_4_3: f32 = 0.0;
const CHOICE_Y_OFFSET_16_9: f32 = 10.0;

const PAD_UNUSED_RGBA: [f32; 4] = [0.2, 0.2, 0.2, 1.0];
const DANCE_PAD_LAYOUT: [bool; 9] = [false, true, false, true, false, true, false, true, false];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Single,
    Versus,
    Double,
}

impl Choice {
    #[inline(always)]
    const fn from_index(idx: usize) -> Choice {
        match idx {
            0 => Choice::Single,
            1 => Choice::Versus,
            _ => Choice::Double,
        }
    }

    #[inline(always)]
    const fn label(self) -> &'static str {
        match self {
            Choice::Single => "1 Player",
            Choice::Versus => "2 Players",
            Choice::Double => "Double",
        }
    }
}

pub struct State {
    pub active_color_index: i32,
    pub selected_index: usize,
    choice_zooms: [f32; CHOICE_COUNT],
    bg: heart_bg::State,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selected_index: 0,
        choice_zooms: [CHOICE_ZOOM_UNFOCUSED; CHOICE_COUNT],
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

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    let speed = (CHOICE_ZOOM_FOCUSED - CHOICE_ZOOM_UNFOCUSED) / CHOICE_ZOOM_TWEEN_DURATION;
    let max_step = speed * dt.max(0.0);

    for i in 0..CHOICE_COUNT {
        let target = if i == state.selected_index {
            CHOICE_ZOOM_FOCUSED
        } else {
            CHOICE_ZOOM_UNFOCUSED
        };
        let z = &mut state.choice_zooms[i];
        let delta = target - *z;
        if delta.abs() <= max_step {
            *z = target;
        } else {
            *z += delta.signum() * max_step;
        }
    }
    None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            state.selected_index = (state.selected_index + CHOICE_COUNT - 1) % CHOICE_COUNT;
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            state.selected_index = (state.selected_index + 1) % CHOICE_COUNT;
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        VirtualAction::p1_start => {
            let choice = Choice::from_index(state.selected_index);
            crate::game::profile::set_session_play_style(match choice {
                Choice::Single => crate::game::profile::PlayStyle::Single,
                Choice::Versus => crate::game::profile::PlayStyle::Versus,
                Choice::Double => crate::game::profile::PlayStyle::Double,
            });
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::Navigate(Screen::SelectMusic)
        }
        VirtualAction::p1_back => ScreenAction::Navigate(Screen::Menu),
        _ => ScreenAction::None,
    }
}

fn push_pad_tiles(
    out: &mut Vec<Actor>,
    base_x: f32,
    base_y: f32,
    zoom: f32,
    used_rgba: [f32; 4],
    unused_rgba: [f32; 4],
) {
    let tile_zoom = widescale(PAD_TILE_ZOOM_4_3, PAD_TILE_ZOOM_16_9) * zoom;
    let tile_step = PAD_TILE_NATIVE_SIZE * tile_zoom;

    for row in 0..3 {
        for col in 0..3 {
            let idx = row * 3 + col;
            let tint = if DANCE_PAD_LAYOUT[idx] {
                used_rgba
            } else {
                unused_rgba
            };

            let x = base_x + tile_step * (col as f32 - 1.0);
            let y = base_y + tile_step * (row as f32 - 2.0);

            out.push(act!(sprite("rounded-square.png"):
                xy(x, y):
                zoomto(PAD_TILE_NATIVE_SIZE, PAD_TILE_NATIVE_SIZE):
                zoom(tile_zoom):
                diffuse(tint[0], tint[1], tint[2], tint[3])
            ));
        }
    }
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(128);
    let profile = crate::game::profile::get();

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    actors.push(screen_bar::build(ScreenBarParams {
        title: "SELECT STYLE",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
    }));

    let footer_avatar = profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: Some(&profile.display_name),
        center_text: None,
        right_text: Some("PRESS START"),
        left_avatar: footer_avatar,
    }));

    let cx = screen_center_x();
    let cy = screen_center_y() + widescale(CHOICE_Y_OFFSET_4_3, CHOICE_Y_OFFSET_16_9);
    let choice_x_off = widescale(CHOICE_X_OFFSET_4_3, CHOICE_X_OFFSET_16_9);
    let dual_pad_off = widescale(PAD_DUAL_OFFSET_4_3, PAD_DUAL_OFFSET_16_9);

    for i in 0..CHOICE_COUNT {
        let choice = Choice::from_index(i);
        let x = match choice {
            Choice::Single => cx - choice_x_off,
            Choice::Versus => cx,
            Choice::Double => cx + choice_x_off,
        };
        let zoom = state.choice_zooms[i];

        match choice {
            Choice::Single => {
                let used = color::decorative_rgba(state.active_color_index);
                push_pad_tiles(&mut actors, x, cy, zoom, used, PAD_UNUSED_RGBA);
            }
            Choice::Versus => {
                let left = color::decorative_rgba(state.active_color_index - 1);
                let right = color::decorative_rgba(state.active_color_index + 2);
                let off = dual_pad_off * zoom;
                push_pad_tiles(&mut actors, x - off, cy, zoom, left, PAD_UNUSED_RGBA);
                push_pad_tiles(&mut actors, x + off, cy, zoom, right, PAD_UNUSED_RGBA);
            }
            Choice::Double => {
                let used = color::decorative_rgba(state.active_color_index + 1);
                let off = dual_pad_off * zoom;
                push_pad_tiles(&mut actors, x - off, cy, zoom, used, PAD_UNUSED_RGBA);
                push_pad_tiles(&mut actors, x + off, cy, zoom, used, PAD_UNUSED_RGBA);
            }
        }

        let label_y = cy + 37.0 * zoom;
        actors.push(act!(text:
            align(0.5, 0.0):
            xy(x, label_y):
            zoom(0.5 * zoom):
            z(1):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("wendy"): settext(choice.label()): horizalign(center)
        ));
    }

    actors
}
