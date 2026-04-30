use super::technique_bg;
use crate::act;
use crate::assets::visual_styles;
use crate::config::{self, VisualStyle};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use std::sync::atomic::{AtomicU32, Ordering};

// Shared UI elapsed clock advanced by `app` using post-Tab-acceleration dt so
// menu backgrounds stay phase-locked across screens while still honoring
// fast/slow/paused menu animation controls.
static GLOBAL_ELAPSED_BITS: AtomicU32 = AtomicU32::new(0.0_f32.to_bits());

const COLOR_ADD: [i32; 10] = [-1, 0, 0, -1, -1, -1, 0, 0, 0, 0];
const DIFFUSE_ALPHA: [f32; 10] = [0.05, 0.2, 0.1, 0.1, 0.1, 0.1, 0.1, 0.05, 0.1, 0.1];
const XY: [f32; 10] = [
    0.0, 40.0, 80.0, 120.0, 200.0, 280.0, 360.0, 400.0, 480.0, 560.0,
];
const UV_VEL: [[f32; 2]; 10] = [
    [0.03, 0.01],
    [0.03, 0.02],
    [0.03, 0.01],
    [0.02, 0.02],
    [0.03, 0.03],
    [0.02, 0.02],
    [0.03, 0.01],
    [-0.03, 0.01],
    [0.05, 0.03],
    [0.03, 0.04],
];
const SHARED_BG_ZOOM: f32 = 1.3;
const SHARED_BG_UV_SPAN: f32 = 1.0;

#[derive(Clone, Copy)]
struct HeartsState;

#[derive(Clone)]
pub struct State {
    hearts: HeartsState,
    technique: technique_bg::State,
}

pub struct Params {
    pub active_color_index: i32,
    pub backdrop_rgba: [f32; 4],
    pub alpha_mul: f32,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for HeartsState {
    fn default() -> Self {
        Self::new()
    }
}

impl HeartsState {
    const fn new() -> Self {
        Self
    }

    fn build_at_elapsed(&self, params: &Params, elapsed_s: f32) -> Vec<Actor> {
        let mut actors = Vec::with_capacity(11);
        let w = screen_width();
        let h = screen_height();
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
            z(-100)
        ));

        for i in 0..10 {
            let mut rgba = color::decorative_rgba(params.active_color_index + COLOR_ADD[i]);
            rgba[3] = DIFFUSE_ALPHA[i] * params.alpha_mul;
            let uv = scrolled_uv_rect(UV_VEL[i], elapsed_s);

            push_shared_bg(&mut actors, XY[i], XY[i], rgba, uv);
        }

        actors
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            hearts: HeartsState::new(),
            technique: technique_bg::State::new(),
        }
    }

    pub fn build(&self, params: Params) -> Vec<Actor> {
        self.build_at_elapsed(params, global_elapsed_s())
    }

    pub fn build_at_elapsed(&self, params: Params, elapsed_s: f32) -> Vec<Actor> {
        let style = visual_style();
        if matches!(style, VisualStyle::Technique)
            && let Some(actors) = self.technique.build_at_elapsed(
                params.active_color_index,
                params.backdrop_rgba,
                params.alpha_mul,
                elapsed_s,
            )
        {
            return actors;
        }
        if matches!(style, VisualStyle::Srpg9) {
            return build_srpg9_static(&params);
        }
        self.hearts.build_at_elapsed(&params, elapsed_s)
    }
}

fn push_shared_bg(out: &mut Vec<Actor>, x: f32, y: f32, rgba: [f32; 4], uv: [f32; 4]) {
    out.push(act!(sprite(visual_styles::shared_background_texture_key()):
        xy(x, y):
        zoom(SHARED_BG_ZOOM):
        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(-99)
    ));
}

fn build_srpg9_static(params: &Params) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(3);
    let w = screen_width();
    let h = screen_height();
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
        z(-100)
    ));

    let mut tint = color::decorative_rgba(params.active_color_index);
    tint[0] = (tint[0] * 3.0).min(1.0);
    tint[1] = (tint[1] * 3.0).min(1.0);
    tint[2] = (tint[2] * 3.0).min(1.0);
    tint[3] = params.alpha_mul;
    actors.push(act!(sprite(visual_styles::shared_background_texture_key()):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_center_y()):
        setsize((h * 16.0 / 9.0).max(w), h):
        diffuse(tint[0], tint[1], tint[2], tint[3]):
        z(-99)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.5 * params.alpha_mul):
        z(-98)
    ));
    actors
}

#[inline(always)]
fn scrolled_uv_rect(velocity: [f32; 2], elapsed_s: f32) -> [f32; 4] {
    let u0 = (velocity[0] * elapsed_s).rem_euclid(1.0);
    let v0 = (velocity[1] * elapsed_s).rem_euclid(1.0);
    [u0, v0, u0 + SHARED_BG_UV_SPAN, v0 + SHARED_BG_UV_SPAN]
}

fn visual_style() -> VisualStyle {
    std::panic::catch_unwind(|| config::get().visual_style).unwrap_or(VisualStyle::Hearts)
}

#[inline]
pub fn tick_global(dt: f32) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let _ = GLOBAL_ELAPSED_BITS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
        let elapsed = f32::from_bits(bits);
        let next = elapsed + dt;
        Some(if next.is_finite() {
            next.max(0.0).to_bits()
        } else {
            bits
        })
    });
}

#[inline]
fn global_elapsed_s() -> f32 {
    f32::from_bits(GLOBAL_ELAPSED_BITS.load(Ordering::Relaxed))
}

#[cfg(test)]
fn set_global_elapsed_for_test(elapsed_s: f32) {
    GLOBAL_ELAPSED_BITS.store(elapsed_s.max(0.0).to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-3;

    fn params() -> Params {
        Params {
            active_color_index: 3,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
        }
    }

    fn first_bg_sprite(actors: &[Actor]) -> ([f32; 2], [f32; 4]) {
        let Some(Actor::Sprite {
            offset,
            source,
            uv_rect,
            ..
        }) = actors.get(1)
        else {
            panic!("missing first background sprite");
        };
        assert_eq!(
            source.texture_key(),
            Some(visual_styles::for_style(VisualStyle::Hearts).shared_background)
        );
        (
            *offset,
            uv_rect.expect("shared background should scroll UVs"),
        )
    }

    #[test]
    fn build_reads_shared_elapsed_clock() {
        set_global_elapsed_for_test(2.5);
        let state = HeartsState::new();
        let shared = first_bg_sprite(&state.build_at_elapsed(&params(), global_elapsed_s()));
        let explicit = first_bg_sprite(&state.build_at_elapsed(&params(), 2.5));
        assert!(
            (shared.0[0] - explicit.0[0]).abs() < EPS
                && (shared.0[1] - explicit.0[1]).abs() < EPS
                && shared
                    .1
                    .iter()
                    .zip(explicit.1)
                    .all(|(a, b)| (*a - b).abs() < EPS),
            "shared={shared:?} explicit={explicit:?}"
        );
    }

    #[test]
    fn tick_global_accumulates_positive_dt() {
        set_global_elapsed_for_test(1.0);
        tick_global(0.5);
        assert!(
            (global_elapsed_s() - 1.5).abs() < EPS,
            "got {}",
            global_elapsed_s()
        );
        tick_global(0.0);
        assert!(
            (global_elapsed_s() - 1.5).abs() < EPS,
            "got {}",
            global_elapsed_s()
        );
        tick_global(-0.25);
        assert!(
            (global_elapsed_s() - 1.5).abs() < EPS,
            "got {}",
            global_elapsed_s()
        );
    }
}
