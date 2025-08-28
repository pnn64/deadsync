use crate::act;
use crate::core::space::globals::*;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::components::{heart_bg, screen_bar};
use crate::ui::components::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

// Native art size of heart.png (for aspect-correct sizing)
const HEART_NATIVE_W: f32 = 668.0;
const HEART_NATIVE_H: f32 = 566.0;
const HEART_ASPECT: f32 = HEART_NATIVE_W / HEART_NATIVE_H;

// Wheel tuning (baseline behavior)
const SCROLL_SPEED_SLOTS_PER_SEC: f32 = 5.0; // how fast the wheel slides
const ROT_PER_SLOT_DEG: f32 = 15.0;          // inward tilt amount (± per slot)
const ZOOM_CENTER: f32 = 1.05;               // center heart size
const EDGE_MIN_RATIO: f32 = 0.17;            // edge zoom = ZOOM_CENTER * EDGE_MIN_RATIO
const WHEEL_Z_BASE: i16 = 105;               // above BG, below bars

// Background cross-fade (to mimic Simply Love's slight delay)
const BG_FADE_DURATION: f32 = 0.20; // seconds, linear fade

// -----------------------------------------------------------------------------
// OPTIONAL PER-SLOT OVERRIDES (symmetric L/R, keyed by distance from center):
// -----------------------------------------------------------------------------
const DIST_OVERRIDES: &[(usize, f32)] = &[
    //(1, 12.0),
];

const ZOOM_MULT_OVERRIDES: &[(usize, f32)] = &[
    (1, 1.25), (2, 1.45), (3, 1.50), (4, 1.15)
];

#[inline(always)]
fn is_wide() -> bool {
    screen_width() / screen_height() >= 1.6 // ~16:10/16:9 and wider
}

/* -------------------------------- state -------------------------------- */

pub struct State {
    /// Which color in DECORATIVE_HEX is focused (and previewed in the bg)
    pub active_color_index: i32,
    /// Smooth wheel offset (in “slots”); eased toward active_color_index
    pub scroll: f32,
    bg: heart_bg::State,
    /// Background fade: from -> to over BG_FADE_DURATION
    pub bg_from_index: i32,
    pub bg_to_index: i32,
    pub bg_fade_t: f32, // [0, BG_FADE_DURATION] ; >= dur means finished
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        scroll: color::DEFAULT_COLOR_INDEX as f32,
        bg: heart_bg::State::new(),
        bg_from_index: color::DEFAULT_COLOR_INDEX,
        bg_to_index:   color::DEFAULT_COLOR_INDEX,
        bg_fade_t:     BG_FADE_DURATION, // start "finished"
    }
}

pub fn handle_key_press(state: &mut State, e: &KeyEvent) -> ScreenAction {
    // Only react to the initial key press; ignore OS key auto-repeat
    if e.state != ElementState::Pressed || e.repeat {
        return ScreenAction::None;
    }

    match e.physical_key {
        PhysicalKey::Code(KeyCode::ArrowRight) | PhysicalKey::Code(KeyCode::KeyD) => {
            state.active_color_index += 1;
            // start a new cross-fade from what's effectively on screen now
            let showing_now = if state.bg_fade_t < BG_FADE_DURATION {
                let a = (state.bg_fade_t / BG_FADE_DURATION).clamp(0.0, 1.0);
                if (1.0 - a) >= a { state.bg_from_index } else { state.bg_to_index }
            } else {
                state.bg_to_index
            };
            state.bg_from_index = showing_now;
            state.bg_to_index   = state.active_color_index;
            state.bg_fade_t     = 0.0;
            ScreenAction::None
        }
        PhysicalKey::Code(KeyCode::ArrowLeft) | PhysicalKey::Code(KeyCode::KeyA) => {
            state.active_color_index -= 1;
            let showing_now = if state.bg_fade_t < BG_FADE_DURATION {
                let a = (state.bg_fade_t / BG_FADE_DURATION).clamp(0.0, 1.0);
                if (1.0 - a) >= a { state.bg_from_index } else { state.bg_to_index }
            } else {
                state.bg_to_index
            };
            state.bg_from_index = showing_now;
            state.bg_to_index   = state.active_color_index;
            state.bg_fade_t     = 0.0;
            ScreenAction::None
        }
        PhysicalKey::Code(KeyCode::Enter) => ScreenAction::Navigate(Screen::Gameplay),
        PhysicalKey::Code(KeyCode::Escape) => ScreenAction::Navigate(Screen::Menu),
        _ => ScreenAction::None,
    }
}

/* ------------------------------- drawing ------------------------------- */

pub fn get_actors(state: &State, _: &crate::core::space::Metrics) -> Vec<Actor> {
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
        let alpha_to   = a;
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
        title_placement: ScreenBarTitlePlacement::Left,   // big title on the left
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,     // keep this None to avoid overlap with left title
        center_text: None,   // later: Some("01:23")
        right_text: None,    // later: Some("P1 • READY")
        fg_color: FG,
    }));
    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        left_text: None,
        center_text: None,
        right_text: Some("NOT PRESENT"),
        fg_color: FG,
    }));

    // 3) The bow of hearts (wheel) — smooth + inward tilt, no refade
    let wide = is_wide();
    let num_slots: i32 = if wide { 11 } else { 7 };
    let center_slot: i32 = num_slots / 2;
    let w_screen = screen_width();

    let x_spacing = w_screen / (num_slots as f32 - 1.0);

    let side_slots: usize = center_slot as usize;

    // (A) X-distance samples
    let mut x_samples: Vec<f32> = Vec::with_capacity(side_slots + 1);
    for k in 0..=side_slots {
        x_samples.push(k as f32 * x_spacing);
    }

    // (B) Zoom samples in log-space
    let max_off_all     = 0.5 * (num_slots as f32 - 1.0);
    let max_off_visible = (max_off_all - 1.0).max(1.0);
    let r               = EDGE_MIN_RATIO.powf(1.0 / max_off_visible);
    let ln_zc = ZOOM_CENTER.ln();
    let ln_r  = r.ln();

    let mut zoom_logs: Vec<f32> = Vec::with_capacity(side_slots + 1);
    for k in 0..=side_slots {
        let a = (k as f32).min(max_off_visible);
        zoom_logs.push(ln_zc + a * ln_r); // log(Z_k)
    }

    // --- Apply user overrides (symmetric for left/right) -----------------
    for &(k, add_px) in DIST_OVERRIDES {
        if k <= side_slots {
            x_samples[k] += add_px;
        }
    }
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
        let x = screen_center_x() + if o >= 0.0 { x_off } else { -x_off };

        // Y forms a gentle bow
        let y = 12.0 * o * o - 20.0;

        // inward tilt
        let rot_deg = -o * ROT_PER_SLOT_DEG;

        // Zoom via exponential sampling in log space
        let a_clamped = a.min(max_off_visible);
        let zoom = super::select_color::sample_exp_from_logs(&zoom_logs, a_clamped);

        // depth so near-center draws on top
        let z_layer = WHEEL_Z_BASE - (a.round() as i16);

        // correct aspect (don’t stretch tall)
        let base_h = 168.0; // overall heart height (tweak)
        let base_w = base_h * HEART_ASPECT;

        // Soft fade near edges so hearts slide on/off
        let start_fade  = (max_off_all - 1.0).max(0.0); // begin fade
        let end_fade    = max_off_all;                  // fully hidden
        let alpha = if a <= start_fade {
            1.0
        } else if a >= end_fade {
            0.0
        } else {
            let t = (a - start_fade) / (end_fade - start_fade);
            1.0 - t * t // ease-out
        };

        actors.push(act!(sprite("heart.png"):
            align(0.5, 0.5):
            xy(x, screen_center_y() + y):
            rotationz(rot_deg):
            z(z_layer):
            zoomto(base_w, base_h):
            zoom(zoom):
            diffuse(tint[0], tint[1], tint[2], alpha)
        ));
    }

    actors
}

/* ---------- tiny helpers for array-driven sampling (used above) ---------- */

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

#[inline(always)]
fn sample_linear(samples: &[f32], x: f32) -> f32 {
    if samples.is_empty() { return 0.0; }
    if x <= 0.0 { return samples[0]; }
    let max = (samples.len() - 1) as f32;
    if x >= max { return samples[samples.len() - 1]; }
    let i0 = x.floor() as usize;
    let t  = x - i0 as f32;
    lerp(samples[i0], samples[i0 + 1], t)
}

#[inline(always)]
fn sample_exp_from_logs(logs: &[f32], x: f32) -> f32 {
    if logs.is_empty() { return 0.0; }
    if x <= 0.0 { return logs[0].exp(); }
    let max = (logs.len() - 1) as f32;
    if x >= max { return logs[logs.len() - 1].exp(); }
    let i0 = x.floor() as usize;
    let t  = x - i0 as f32;
    (lerp(logs[i0], logs[i0 + 1], t)).exp()
}

/* ------------------------------- update ------------------------------- */

pub fn update(state: &mut State, dt: f32) {
    // glide scroll toward the selected slot
    let target = state.active_color_index as f32;
    let delta  = target - state.scroll;

    let max_step = SCROLL_SPEED_SLOTS_PER_SEC * dt;
    if delta.abs() <= max_step {
        state.scroll = target;                 // snap when close
    } else {
        state.scroll += delta.signum() * max_step;
    }

    // drive background cross-fade
    if state.bg_fade_t < BG_FADE_DURATION {
        state.bg_fade_t = (state.bg_fade_t + dt).min(BG_FADE_DURATION);
    }
}
