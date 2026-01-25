use crate::act;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_width, screen_height, widescale, screen_center_x, screen_center_y};
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
// Simply Love: ScreenSelectStyle underlay/choice.lua
const CHOICE_CHOSEN_ZOOM_OUT_DURATION: f32 = 0.415;
const CHOICE_NOT_CHOSEN_FADE_DELAY: f32 = 0.1;
const CHOICE_NOT_CHOSEN_FADE_DURATION: f32 = 0.2;
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
    const fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Single,
            1 => Self::Versus,
            _ => Self::Double,
        }
    }

    #[inline(always)]
    const fn label(self) -> &'static str {
        match self {
            Self::Single => "1 Player",
            Self::Versus => "2 Players",
            Self::Double => "Double",
        }
    }
}

pub struct State {
    pub active_color_index: i32,
    pub selected_index: usize,
    choice_zooms: [f32; CHOICE_COUNT],
    exit_requested: bool,
    exit_chosen_anim: bool,
    exit_target: Option<Screen>,
    bg: heart_bg::State,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selected_index: 0,
        choice_zooms: [CHOICE_ZOOM_UNFOCUSED; CHOICE_COUNT],
        exit_requested: false,
        exit_chosen_anim: false,
        exit_target: None,
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
    if state.exit_requested {
        if let Some(target) = state.exit_target
            && exit_anim_t(state.exit_chosen_anim) >= CHOICE_CHOSEN_ZOOM_OUT_DURATION {
                state.exit_target = None;
                return Some(ScreenAction::Navigate(target));
            }
        return None;
    }
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
    if state.exit_requested {
        return ScreenAction::None;
    }

    let nav = match crate::game::profile::get_session_player_side() {
        crate::game::profile::PlayerSide::P2 => match ev.action {
            VirtualAction::p2_left | VirtualAction::p2_menu_left => Some(-1),
            VirtualAction::p2_right | VirtualAction::p2_menu_right => Some(1),
            VirtualAction::p2_start => Some(0),
            VirtualAction::p2_back => Some(9),
            _ => None,
        },
        crate::game::profile::PlayerSide::P1 => match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => Some(-1),
            VirtualAction::p1_right | VirtualAction::p1_menu_right => Some(1),
            VirtualAction::p1_start => Some(0),
            VirtualAction::p1_back => Some(9),
            _ => None,
        },
    };

    match nav {
        Some(-1) => {
            state.selected_index = (state.selected_index + CHOICE_COUNT - 1) % CHOICE_COUNT;
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        Some(1) => {
            state.selected_index = (state.selected_index + 1) % CHOICE_COUNT;
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        Some(0) => {
            state.exit_requested = true;
            state.exit_chosen_anim = true;
            state.exit_target = Some(Screen::SelectMusic);
            let _ = exit_anim_t(true);
            let choice = Choice::from_index(state.selected_index);
            crate::game::profile::set_session_play_style(match choice {
                Choice::Single => crate::game::profile::PlayStyle::Single,
                Choice::Versus => crate::game::profile::PlayStyle::Versus,
                Choice::Double => crate::game::profile::PlayStyle::Double,
            });
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::None
        }
        Some(9) => {
            state.exit_requested = true;
            state.exit_chosen_anim = false;
            state.exit_target = None;
            ScreenAction::Navigate(Screen::Menu)
        }
        _ => ScreenAction::None,
    }
}

#[inline(always)]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

#[inline(always)]
fn not_chosen_alpha(exit_t: f32) -> f32 {
    if exit_t <= CHOICE_NOT_CHOSEN_FADE_DELAY {
        return 1.0;
    }
    let t = (exit_t - CHOICE_NOT_CHOSEN_FADE_DELAY) / CHOICE_NOT_CHOSEN_FADE_DURATION;
    1.0 - smoothstep(t)
}

#[inline(always)]
fn exit_anim_t(exiting: bool) -> f32 {
    if !exiting {
        return 0.0;
    }

    use crate::ui::{anim, runtime};
    static STEPS: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();
    let dur = CHOICE_CHOSEN_ZOOM_OUT_DURATION.max(0.0);
    let steps = STEPS.get_or_init(|| vec![anim::linear(dur).x(dur).build()]);

    let mut init = anim::TweenState::default();
    init.x = 0.0;
    let sid = runtime::site_id(file!(), line!(), column!(), 0x5353544C45584954u64); // "SSTLEXIT"
    runtime::materialize(sid, init, steps).x.max(0.0)
}

fn push_pad_tiles(
    out: &mut Vec<Actor>,
    base_x: f32,
    base_y: f32,
    zoom: f32,
    alpha_mul: f32,
    used_rgba: [f32; 4],
    unused_rgba: [f32; 4],
) {
    let tile_zoom = widescale(PAD_TILE_ZOOM_4_3, PAD_TILE_ZOOM_16_9) * zoom;
    let tile_step = PAD_TILE_NATIVE_SIZE * tile_zoom;

    for row in 0..3 {
        for col in 0..3 {
            let idx = row * 3 + col;
            let mut tint = if DANCE_PAD_LAYOUT[idx] {
                used_rgba
            } else {
                unused_rgba
            };
            tint[3] *= alpha_mul;

            let x = tile_step.mul_add(col as f32 - 1.0, base_x);
            let y = tile_step.mul_add(row as f32 - 2.0, base_y);

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
    let exit_t = exit_anim_t(state.exit_chosen_anim);
    let (chosen_p, other_alpha) = if state.exit_chosen_anim {
        (
            crate::ui::anim::bouncebegin_p(exit_t / CHOICE_CHOSEN_ZOOM_OUT_DURATION),
            not_chosen_alpha(exit_t),
        )
    } else {
        (0.0, 1.0)
    };

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
        right_avatar: None,
    }));

    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P1);
    let p2_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P2);
    let p1_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P1);
    let p2_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P2);

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
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
        let (zoom, alpha) = if state.exit_chosen_anim {
            if i == state.selected_index {
                (CHOICE_ZOOM_FOCUSED * (1.0 - chosen_p), 1.0)
            } else {
                (CHOICE_ZOOM_UNFOCUSED, other_alpha)
            }
        } else {
            (state.choice_zooms[i], 1.0)
        };

        match choice {
            Choice::Single => {
                let used = color::decorative_rgba(state.active_color_index);
                push_pad_tiles(&mut actors, x, cy, zoom, alpha, used, PAD_UNUSED_RGBA);
            }
            Choice::Versus => {
                let left = color::decorative_rgba(state.active_color_index - 1);
                let right = color::decorative_rgba(state.active_color_index + 2);
                let off = dual_pad_off * zoom;
                push_pad_tiles(&mut actors, x - off, cy, zoom, alpha, left, PAD_UNUSED_RGBA);
                push_pad_tiles(&mut actors, x + off, cy, zoom, alpha, right, PAD_UNUSED_RGBA);
            }
            Choice::Double => {
                let used = color::decorative_rgba(state.active_color_index + 1);
                let off = dual_pad_off * zoom;
                push_pad_tiles(&mut actors, x - off, cy, zoom, alpha, used, PAD_UNUSED_RGBA);
                push_pad_tiles(&mut actors, x + off, cy, zoom, alpha, used, PAD_UNUSED_RGBA);
            }
        }

        let label_y = 37.0f32.mul_add(zoom, cy);
        actors.push(act!(text:
            align(0.5, 0.0):
            xy(x, label_y):
            zoom(0.5 * zoom):
            z(1):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, alpha):
            font("wendy"): settext(choice.label()): horizalign(center)
        ));
    }

    actors
}
