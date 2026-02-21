use crate::act;
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::profile;
// Screen navigation handled in app.rs
use crate::screens::components::screen_bar::{
    AvatarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::{heart_bg, screen_bar};
use crate::ui::actors::Actor;
use crate::ui::color;
// Keyboard handling is centralized in app.rs via virtual actions
use crate::core::input::{InputEvent, VirtualAction};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

// Native art size of heart.png (for aspect-correct sizing)
const HEART_NATIVE_W: f32 = 668.0;
const HEART_NATIVE_H: f32 = 566.0;
const HEART_ASPECT: f32 = HEART_NATIVE_W / HEART_NATIVE_H;

// Wheel tuning (baseline behavior)
// Simply Love uses `finishtweening(); linear(0.2)` when a new scroll input arrives.
// This keeps rapid presses responsive by canceling in-flight scrolls instead of queueing.
const SCROLL_TWEEN_DURATION: f32 = 0.20;
// Simply Love wheel in/out details (ScreenSelectColor underlay.lua)
const WHEEL_FORM_DURATION: f32 = 0.20; // container `linear(0.2)` in transform()
const WHEEL_OFF_STAGGER: f32 = 0.04; // OffCommand: sleep(0.04 * index)
const WHEEL_OFF_FADE_DURATION: f32 = 0.20; // OffCommand: linear(0.2) diffusealpha(0)
const ROT_PER_SLOT_DEG: f32 = 15.0; // inward tilt amount (± per slot)
const ZOOM_CENTER: f32 = 1.05; // center heart size
const EDGE_MIN_RATIO: f32 = 0.17; // edge zoom = ZOOM_CENTER * EDGE_MIN_RATIO
const WHEEL_Z_BASE: i16 = 105; // above BG, below bars

// Background cross-fade (to mimic Simply Love's slight delay)
pub const BG_FADE_DURATION: f32 = 0.20; // seconds, linear fade

// -----------------------------------------------------------------------------
// OPTIONAL PER-SLOT OVERRIDES (symmetric L/R, keyed by distance from center):
// -----------------------------------------------------------------------------

const ZOOM_MULT_OVERRIDES: &[(usize, f32)] = &[(1, 1.25), (2, 1.45), (3, 1.50), (4, 1.15)];

#[inline(always)]
fn is_wide() -> bool {
    screen_width() / screen_height() >= 1.6 // ~16:10/16:9 and wider
}

/* -------------------------------- state -------------------------------- */

pub struct State {
    /// Which color in `DECORATIVE_RGBA` is focused (and previewed in the bg)
    pub active_color_index: i32,
    /// Smooth wheel offset (in “slots”); tweened toward `active_color_index`
    pub scroll: f32,
    scroll_from: f32,
    scroll_to: f32,
    scroll_t: f32, // [0, SCROLL_TWEEN_DURATION]
    exit_requested: bool,
    bg: heart_bg::State,
    /// Background fade: from -> to over `BG_FADE_DURATION`
    pub bg_from_index: i32,
    pub bg_to_index: i32,
    pub bg_fade_t: f32, // [0, BG_FADE_DURATION] ; >= dur means finished
}

pub fn init() -> State {
    let scroll = color::DEFAULT_COLOR_INDEX as f32;
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        scroll,
        scroll_from: scroll,
        scroll_to: scroll,
        scroll_t: SCROLL_TWEEN_DURATION, // start "finished"
        exit_requested: false,
        bg: heart_bg::State::new(),
        bg_from_index: color::DEFAULT_COLOR_INDEX,
        bg_to_index: color::DEFAULT_COLOR_INDEX,
        bg_fade_t: BG_FADE_DURATION, // start "finished"
    }
}

pub const fn snap_scroll_to_active(state: &mut State) {
    let s = state.active_color_index as f32;
    state.scroll = s;
    state.scroll_from = s;
    state.scroll_to = s;
    state.scroll_t = SCROLL_TWEEN_DURATION;
}

pub const fn on_enter(state: &mut State) {
    state.exit_requested = false;
}

pub fn exit_anim_duration() -> f32 {
    let num_slots = if is_wide() { 11 } else { 7 };
    WHEEL_OFF_STAGGER.mul_add(num_slots as f32, WHEEL_OFF_FADE_DURATION)
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

/* ------------------------------- drawing ------------------------------- */

/// Helper to recursively apply an alpha multiplier to an actor and its children.
fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
        Actor::Mesh { vertices, .. } => {
            let mut out: Vec<crate::core::gfx::MeshVertex> = Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                let mut c = v.color;
                c[3] *= alpha;
                out.push(crate::core::gfx::MeshVertex {
                    pos: v.pos,
                    color: c,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
        Actor::TexturedMesh { vertices, .. } => {
            let mut out: Vec<crate::core::gfx::TexturedMeshVertex> =
                Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                let mut c = v.color;
                c[3] *= alpha;
                out.push(crate::core::gfx::TexturedMeshVertex {
                    pos: v.pos,
                    uv: v.uv,
                    tex_matrix_scale: v.tex_matrix_scale,
                    color: c,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
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
        Actor::Camera { children, .. } => {
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

pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);

    // 1) Animated heart background with a short cross-fade between colors.
    let a = (state.bg_fade_t / BG_FADE_DURATION).clamp(0.0, 1.0);
    if a >= 1.0 || state.bg_from_index == state.bg_to_index {
        // No active fade: draw a single layer + normal backdrop
        actors.extend(state.bg.build(heart_bg::Params {
            active_color_index: state.bg_to_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
        }));
    } else {
        let alpha_from = 1.0 - a;
        let alpha_to = a;
        // Bottom: previous color + full backdrop
        actors.extend(state.bg.build(heart_bg::Params {
            active_color_index: state.bg_from_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: alpha_from,
        }));
        // Top: new color + NO backdrop (avoid double darkening)
        actors.extend(state.bg.build(heart_bg::Params {
            active_color_index: state.bg_to_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 0.0],
            alpha_mul: alpha_to,
        }));
    }

    // 2) Bars (top + bottom)
    const FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "SELECT A COLOR",
        title_placement: ScreenBarTitlePlacement::Left, // big title on the left
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,   // keep this None to avoid overlap with left title
        center_text: None, // later: Some("01:23")
        right_text: None,  // later: Some("P1 • READY")
        left_avatar: None,
        right_avatar: None,
        fg_color: FG,
    }));

    let p1_profile = profile::get_for_side(profile::PlayerSide::P1);
    let p2_profile = profile::get_for_side(profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    let p1_guest = profile::is_session_side_guest(profile::PlayerSide::P1);
    let p2_guest = profile::is_session_side_guest(profile::PlayerSide::P2);

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
    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
        fg_color: FG,
    }));

    let mut wheel_actors = Vec::new();

    // 3) The bow of hearts (wheel) — smooth + inward tilt, no refade
    let wide = is_wide();
    let num_slots: i32 = if wide { 11 } else { 7 };
    let center_slot: i32 = num_slots / 2;
    let w_screen = screen_width();
    let cx = screen_center_x();
    let cy = screen_center_y();

    #[inline(always)]
    fn wheel_form_p() -> f32 {
        use crate::ui::{anim, runtime};
        static STEPS: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();
        let steps = STEPS.get_or_init(|| vec![anim::linear(WHEEL_FORM_DURATION).x(1.0).build()]);

        let mut init = anim::TweenState::default();
        init.x = 0.0;
        let sid = runtime::site_id(file!(), line!(), column!(), 0x53434F4C464F524Du64); // "SCOLFORM"
        runtime::materialize(sid, init, steps).x.clamp(0.0, 1.0)
    }

    #[inline(always)]
    fn wheel_exit_t(wide: bool) -> f32 {
        use crate::ui::{anim, runtime};
        static STEPS_WIDE: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();
        static STEPS_NARROW: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();

        let num_slots = if wide { 11 } else { 7 };
        let dur = WHEEL_OFF_STAGGER.mul_add(num_slots as f32, WHEEL_OFF_FADE_DURATION);

        let steps = if wide {
            STEPS_WIDE.get_or_init(|| vec![anim::linear(dur).x(dur).build()])
        } else {
            STEPS_NARROW.get_or_init(|| vec![anim::linear(dur).x(dur).build()])
        };

        let mut init = anim::TweenState::default();
        init.x = 0.0;
        let sid = runtime::site_id(
            file!(),
            line!(),
            column!(),
            if wide {
                0x53434F4C45584954u64
            } else {
                0x53434F4C45584954u64 ^ 1
            }, // "SCOLEXIT"
        );
        runtime::materialize(sid, init, steps).x.max(0.0)
    }

    #[inline(always)]
    fn wheel_off_alpha(t: f32, index_1based: i32) -> f32 {
        let start = WHEEL_OFF_STAGGER * (index_1based as f32);
        if t <= start {
            return 1.0;
        }
        let u = (t - start) / WHEEL_OFF_FADE_DURATION;
        (1.0 - u).clamp(0.0, 1.0)
    }

    let form_p = wheel_form_p();
    let exit_t = if state.exit_requested {
        wheel_exit_t(wide)
    } else {
        0.0
    };

    let x_spacing = w_screen / (num_slots as f32 - 1.0);

    let side_slots: usize = center_slot as usize;

    // (A) X-distance samples
    let mut x_samples: Vec<f32> = Vec::with_capacity(side_slots + 1);
    for k in 0..=side_slots {
        x_samples.push(k as f32 * x_spacing);
    }

    // (B) Zoom samples in log-space
    let max_off_all = 0.5 * (num_slots as f32 - 1.0);
    let max_off_visible = (max_off_all - 1.0).max(1.0);
    let r = EDGE_MIN_RATIO.powf(1.0 / max_off_visible);
    let ln_zc = ZOOM_CENTER.ln();
    let ln_r = r.ln();

    let mut zoom_logs: Vec<f32> = Vec::with_capacity(side_slots + 1);
    for k in 0..=side_slots {
        let a = (k as f32).min(max_off_visible);
        zoom_logs.push(ln_zc + a * ln_r); // log(Z_k)
    }

    // --- Apply user overrides (symmetric for left/right) -----------------
    for &(k, mult) in ZOOM_MULT_OVERRIDES {
        if k <= side_slots && mult > 0.0 {
            zoom_logs[k] += mult.ln();
        }
    }
    // ---------------------------------------------------------------------

    // split scroll into integer + fractional parts (stable left/right motion)
    let base_i = state.scroll.floor() as i32;
    let frac = state.scroll - base_i as f32; // [0, 1)

    for slot in 0..num_slots {
        let offset_i = slot - center_slot; // integer slot offset

        // fractional offset used for position/zoom/rotation (smooth slide)
        let o = offset_i as f32 - frac;
        let a = o.abs();

        // palette color for this slot (stick to integer to avoid “color lerp” look)
        let tint = color::decorative_rgba(base_i + offset_i);

        // X centered via distance samples (sign from side)
        let x_off = super::select_color::sample_linear(&x_samples, a);
        let x_final = cx + if o >= 0.0 { x_off } else { -x_off };

        // Y forms a gentle bow
        let y_off_final = (12.0 * o).mul_add(o, -20.0);

        // inward tilt
        let rot_deg_final = -o * ROT_PER_SLOT_DEG;

        // Zoom via exponential sampling in log space
        let a_clamped = a.min(max_off_visible);
        let zoom_final = super::select_color::sample_exp_from_logs(&zoom_logs, a_clamped);

        // depth so near-center draws on top
        let z_layer = WHEEL_Z_BASE - (a.round() as i16);

        // correct aspect (don’t stretch tall)
        let base_h = 168.0; // overall heart height (tweak)
        let base_w = base_h * HEART_ASPECT;

        // Soft fade near edges so hearts slide on/off
        let start_fade = (max_off_all - 1.0).max(0.0); // begin fade
        let end_fade = max_off_all; // fully hidden
        let alpha = if a <= start_fade {
            1.0
        } else if a >= end_fade {
            0.0
        } else {
            let t = (a - start_fade) / (end_fade - start_fade);
            1.0 - t * t // ease-out
        };

        let off_alpha = if state.exit_requested {
            wheel_off_alpha(exit_t, slot + 1)
        } else {
            1.0
        };
        let alpha = alpha * off_alpha;

        // Enter: collapse from center into the bow (like SL wheel transform tween)
        let x = lerp(cx, x_final, form_p);
        let y = lerp(cy, cy + y_off_final, form_p);
        let rot_deg = lerp(0.0, rot_deg_final, form_p);
        let zoom = lerp(1.0, zoom_final, form_p);

        wheel_actors.push(act!(sprite("heart.png"):
            align(0.5, 0.5):
            xy(x, y):
            rotationz(rot_deg):
            z(z_layer):
            setsize(base_w, base_h):
            zoom(zoom):
            diffuse(tint[0], tint[1], tint[2], alpha)
        ));
    }

    for actor in &mut wheel_actors {
        apply_alpha_to_actor(actor, alpha_multiplier);
    }
    actors.extend(wheel_actors);

    actors
}

/* ---------- tiny helpers for array-driven sampling (used above) ---------- */

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn sample_linear(samples: &[f32], x: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    if x <= 0.0 {
        return samples[0];
    }
    let max = (samples.len() - 1) as f32;
    if x >= max {
        return samples[samples.len() - 1];
    }
    let i0 = x.floor() as usize;
    let t = x - i0 as f32;
    lerp(samples[i0], samples[i0 + 1], t)
}

#[inline(always)]
fn sample_exp_from_logs(logs: &[f32], x: f32) -> f32 {
    if logs.is_empty() {
        return 0.0;
    }
    if x <= 0.0 {
        return logs[0].exp();
    }
    let max = (logs.len() - 1) as f32;
    if x >= max {
        return logs[logs.len() - 1].exp();
    }
    let i0 = x.floor() as usize;
    let t = x - i0 as f32;
    (lerp(logs[i0], logs[i0 + 1], t)).exp()
}

/* ------------------------------- update ------------------------------- */

pub fn update(state: &mut State, dt: f32) {
    // Scroll tween (matches Simply Love's `linear(0.2)` behavior)
    if SCROLL_TWEEN_DURATION <= 0.0 {
        snap_scroll_to_active(state);
    } else if state.scroll_t < SCROLL_TWEEN_DURATION {
        state.scroll_t = (state.scroll_t + dt).min(SCROLL_TWEEN_DURATION);
        state.scroll = lerp(
            state.scroll_from,
            state.scroll_to,
            state.scroll_t / SCROLL_TWEEN_DURATION,
        );
    } else {
        state.scroll = state.scroll_to;
    }

    // drive background cross-fade
    if state.bg_fade_t < BG_FADE_DURATION {
        state.bg_fade_t = (state.bg_fade_t + dt).min(BG_FADE_DURATION);
    }
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
            let num_colors = color::DECORATIVE_RGBA.len() as i32;
            // Mimic SM's `finishtweening()` before starting a new scroll.
            state.scroll = state.scroll_to;
            state.scroll_from = state.scroll;
            state.active_color_index -= 1;
            state.scroll_to = state.active_color_index as f32;
            state.scroll_t = 0.0;
            crate::core::audio::play_sfx("assets/sounds/expand.ogg");
            crate::config::update_simply_love_color(
                state.active_color_index.rem_euclid(num_colors),
            );
            let showing_now = if state.bg_fade_t < BG_FADE_DURATION {
                let a = (state.bg_fade_t / BG_FADE_DURATION).clamp(0.0, 1.0);
                if (1.0 - a) >= a {
                    state.bg_from_index
                } else {
                    state.bg_to_index
                }
            } else {
                state.bg_to_index
            };
            state.bg_from_index = showing_now;
            state.bg_to_index = state.active_color_index;
            state.bg_fade_t = 0.0;
            ScreenAction::None
        }
        Some(1) => {
            let num_colors = color::DECORATIVE_RGBA.len() as i32;
            // Mimic SM's `finishtweening()` before starting a new scroll.
            state.scroll = state.scroll_to;
            state.scroll_from = state.scroll;
            state.active_color_index += 1;
            state.scroll_to = state.active_color_index as f32;
            state.scroll_t = 0.0;
            crate::core::audio::play_sfx("assets/sounds/expand.ogg");
            crate::config::update_simply_love_color(
                state.active_color_index.rem_euclid(num_colors),
            );
            let showing_now = if state.bg_fade_t < BG_FADE_DURATION {
                let a = (state.bg_fade_t / BG_FADE_DURATION).clamp(0.0, 1.0);
                if (1.0 - a) >= a {
                    state.bg_from_index
                } else {
                    state.bg_to_index
                }
            } else {
                state.bg_to_index
            };
            state.bg_from_index = showing_now;
            state.bg_to_index = state.active_color_index;
            state.bg_fade_t = 0.0;
            ScreenAction::None
        }
        Some(0) => {
            state.exit_requested = true;
            state.scroll = state.scroll_to;
            state.scroll_from = state.scroll;
            state.scroll_t = SCROLL_TWEEN_DURATION;
            crate::core::audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::Navigate(Screen::SelectStyle)
        }
        Some(9) => {
            state.exit_requested = true;
            state.scroll = state.scroll_to;
            state.scroll_from = state.scroll;
            state.scroll_t = SCROLL_TWEEN_DURATION;
            ScreenAction::Navigate(Screen::Menu)
        }
        _ => ScreenAction::None,
    }
}
