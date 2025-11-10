use crate::act;
use crate::core::space::*;
use crate::screens::{Screen, ScreenAction};
use crate::core::input::{VirtualAction, InputEvent};
use crate::ui::actors::Actor;
use crate::ui::components::heart_bg;
use crate::ui::color;

/* ----------------------- timing & layout ----------------------- */

/* Show ONLY the hearts bg some time before any other animation starts */
const PRE_ROLL: f32 = 1.25;

/* arrows (matches the simple SM-like splash) */
const ARROW_COUNT: usize = 7;
const ARROW_SPACING: f32 = 50.0;
const ARROW_BASE_DELAY: f32 = 0.20;
const ARROW_STEP_DELAY: f32 = 0.10;
const ARROW_FADE_IN: f32  = 0.75;
const ARROW_FADE_OUT: f32 = 0.75;

/* black bar behind arrows */
const BAR_TARGET_H: f32 = 128.0;
const ARROW_BG_Z: f32   = 106.0; // above hearts, below arrows

/* “squish” bar timings (center line -> open -> close) */
const SQUISH_START_DELAY: f32 = 0.50;   // after PRE_ROLL
const SQUISH_IN_DURATION: f32 = 0.35;   // 1.0 -> 0.0
pub const BAR_SQUISH_DURATION: f32 = 0.35;

/* ----------------------- auto-advance ----------------------- */
#[inline(always)]
fn arrows_finished_at() -> f32 {
    // PRE_ROLL + unsquish end + last arrow fade in/out + tiny pad
    let unsquish_end = SQUISH_START_DELAY + SQUISH_IN_DURATION;
    let last_delay = ARROW_BASE_DELAY + ARROW_STEP_DELAY * (ARROW_COUNT as f32);
    PRE_ROLL + unsquish_end + last_delay + ARROW_FADE_IN + ARROW_FADE_OUT + 0.05
}

#[inline(always)]
fn maxf(a: f32, b: f32) -> f32 { if a > b { a } else { b } }
#[inline(always)]
fn remaining(from_time: f32, now: f32) -> f32 { maxf(from_time - now, 0.0) }

/* ---------------------------- state ---------------------------- */

#[derive(PartialEq, Eq)]
enum InitPhase {
    Playing,
    FadingOut,
}

pub struct State {
    elapsed: f32,
    phase: InitPhase,
    pub active_color_index: i32,
    bg: heart_bg::State,
}

pub fn init() -> State {
    State {
        elapsed: 0.0,
        phase: InitPhase::Playing,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
    }
}

/* -------------------------- input -> nav ----------------------- */

pub fn handle_input(_: &mut State, ev: &InputEvent) -> ScreenAction {
    if ev.pressed {
        match ev.action {
            VirtualAction::p1_start | VirtualAction::p1_back => return ScreenAction::Navigate(Screen::Menu),
            _ => {}
        }
    }
    ScreenAction::None
}

/* ---------------------------- update --------------------------- */

pub fn update(state: &mut State, dt: f32) -> ScreenAction {
    state.elapsed += dt;

    if state.phase == InitPhase::Playing && state.elapsed >= arrows_finished_at() {
        state.phase = InitPhase::FadingOut;
        state.elapsed = arrows_finished_at();
    }

    if state.phase == InitPhase::FadingOut {
        let fade_elapsed = state.elapsed - arrows_finished_at();
        if fade_elapsed >= BAR_SQUISH_DURATION {
            return ScreenAction::Navigate(Screen::Menu);
        }
    }
    ScreenAction::None
}

/* --------------------------- drawing helpers --------------------------- */

pub fn build_squish_bar(progress: f32) -> Actor {
    let w  = screen_width();
    let cy = screen_center_y();

    let t = progress.clamp(0.0, 1.0);
    let crop = 0.5 * t;

    act!(quad:
        align(0.5, 0.5):
        xy(0.5 * w, cy):
        zoomto(w, BAR_TARGET_H):
        diffuse(0.0, 0.0, 0.0, 1.0):
        croptop(crop): cropbottom(crop):
        z(105.0)
    )
}

/* Backdrop that starts its animation immediately WHEN ADDED (no initial sleep). */
fn build_arrows_backdrop_now() -> Actor {
    let w  = screen_width();
    let cy = screen_center_y();

    act!(quad:
        align(0.5, 0.5):
        xy(0.5 * w, cy):
        zoomto(w, 0.0):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(ARROW_BG_Z):

        /* IN: grow to 128px tall and reach 0.9 alpha */
        accelerate(0.30): zoomto(w, BAR_TARGET_H): diffusealpha(0.90):

        /* hold while arrows do their fade in/out */
        sleep(2.10):

        /* OUT: collapse back to 0 height */
        accelerate(0.30): zoomto(w, 0.0):
        linear(0.0): visible(false)
    )
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.5, 0.5):
        xy(0.5 * screen_width(), screen_center_y()):
        zoomto(screen_width(), BAR_TARGET_H):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1200.0):
        croptop(0.0): cropbottom(0.0):
        linear(0.35): croptop(0.5): cropbottom(0.5)
    );
    (vec![actor], 0.35)
}

/* --------------------------- combined build --------------------------- */

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(32 + ARROW_COUNT);

    /* 1) HEART BACKGROUND — visible immediately */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    /* If we’re still in pre-roll, stop here: no squish/backdrop/arrows yet. */
    if state.elapsed < PRE_ROLL {
        return actors;
    }

    /* 2) SQUISH BAR — drive by timeline that starts after PRE_ROLL */
    let t_anim = state.elapsed - PRE_ROLL;

    let progress = if state.phase == InitPhase::FadingOut {
        let fade_elapsed = state.elapsed - arrows_finished_at();
        (fade_elapsed / BAR_SQUISH_DURATION).clamp(0.0, 1.0)
    } else if t_anim < SQUISH_START_DELAY {
        1.0
    } else if t_anim < SQUISH_START_DELAY + SQUISH_IN_DURATION {
        1.0 - ((t_anim - SQUISH_START_DELAY) / SQUISH_IN_DURATION)
    } else {
        0.0
    };
    actors.push(build_squish_bar(progress));

    /* 2.5) ARROW BACKDROP — only add once unsquish has completed */
    let unsquish_end = SQUISH_START_DELAY + SQUISH_IN_DURATION;
    if t_anim >= unsquish_end {
        actors.push(build_arrows_backdrop_now());
    }

    /* 3) RAINBOW ARROWS — their sleeps are computed as “remaining time from now” */
    let cx = screen_center_x();
    let cy = screen_center_y();

    for i in 1..=ARROW_COUNT {
        let x = (i as f32 - 4.0) * ARROW_SPACING;

        // absolute start for arrow i (global time)
        let arrow_start_time = PRE_ROLL + unsquish_end + ARROW_BASE_DELAY + ARROW_STEP_DELAY * (i as f32);
        // convert to remaining time from *current* elapsed so late frames still work perfectly
        let delay_from_now = remaining(arrow_start_time, state.elapsed);

        let tint = color::decorative_rgba(state.active_color_index - i as i32 - 4);

        actors.push(act!(sprite("init_arrow.png"):
            align(0.5, 0.5):
            xy(cx + x, cy):
            z(110.0):
            zoom(0.1):
            diffuse(tint[0], tint[1], tint[2], 0.0):
            sleep(delay_from_now):
            linear(ARROW_FADE_IN):  alpha(1.0):
            linear(ARROW_FADE_OUT): alpha(0.0):
            linear(0.0): visible(false)
        ));
    }

    actors
}
