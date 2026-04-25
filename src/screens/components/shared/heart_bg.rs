use super::technique_bg;
use crate::act;
use crate::config::{self, MenuBackgroundStyle};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_height, screen_width};
use std::sync::Arc;
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
const VARIANTS: [usize; 10] = [0, 1, 2, 0, 1, 0, 2, 0, 1, 2];
const DEFAULT_DIMS: (f32, f32) = (668.0, 566.0);
const BW_BIG: f32 = 668.0;
const BW_NORMAL: f32 = 543.0;
const BW_SMALL: f32 = 400.0;
const PHI: f32 = 0.618_034;

#[derive(Clone)]
struct HeartsState {
    tex_key: Arc<str>,
    variant_size: [[f32; 2]; 3],
}

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
    fn new() -> Self {
        Self::with_texture("heart.png")
    }

    fn with_texture(tex_key: &'static str) -> Self {
        let (w, h) = DEFAULT_DIMS;
        let aspect = h / w;
        let scale_k = (w * 0.6) / BW_BIG;
        let var_w = [BW_NORMAL * scale_k, BW_BIG * scale_k, BW_SMALL * scale_k];
        let variant_size = [
            [var_w[0], var_w[0] * aspect],
            [var_w[1], var_w[1] * aspect],
            [var_w[2], var_w[2] * aspect],
        ];

        Self {
            tex_key: Arc::<str>::from(tex_key),
            variant_size,
        }
    }

    fn build_at_elapsed(&self, params: &Params, elapsed_s: f32) -> Vec<Actor> {
        let mut actors = Vec::with_capacity(41);
        let w = screen_width();
        let h = screen_height();
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
            z(-100)
        ));

        let speed_scale_px = w.max(h) * 1.3;
        for i in 0..10 {
            let variant = VARIANTS[i];
            let [heart_w, heart_h] = self.variant_size[variant];
            let half_w = heart_w * 0.5;
            let half_h = heart_h * 0.5;
            let mut rgba = color::decorative_rgba(params.active_color_index + COLOR_ADD[i]);
            rgba[3] = DIFFUSE_ALPHA[i] * params.alpha_mul;

            let vx_px = -2.0 * UV_VEL[i][0] * speed_scale_px;
            let vy_px = -2.0 * UV_VEL[i][1] * speed_scale_px;
            let start_x = (i as f32).mul_add(w * 0.1, XY[i]) % w;
            let start_y = XY[i].mul_add(0.5, (i as f32) * (h * 0.1) * PHI) % h;
            let x0 = (start_x + vx_px * elapsed_s).rem_euclid(w);
            let y0 = (start_y + vy_px * elapsed_s).rem_euclid(h);

            let wrap_x = if x0 < half_w {
                Some(x0 + w)
            } else if x0 > w - half_w {
                Some(x0 - w)
            } else {
                None
            };
            let wrap_y = if y0 < half_h {
                Some(y0 + h)
            } else if y0 > h - half_h {
                Some(y0 - h)
            } else {
                None
            };

            push_heart(&mut actors, &self.tex_key, x0, y0, heart_w, heart_h, rgba);
            if let Some(wx) = wrap_x {
                push_heart(&mut actors, &self.tex_key, wx, y0, heart_w, heart_h, rgba);
            }
            if let Some(wy) = wrap_y {
                push_heart(&mut actors, &self.tex_key, x0, wy, heart_w, heart_h, rgba);
            }
            if let (Some(wx), Some(wy)) = (wrap_x, wrap_y) {
                push_heart(&mut actors, &self.tex_key, wx, wy, heart_w, heart_h, rgba);
            }
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

    pub fn with_texture(tex_key: &'static str) -> Self {
        Self {
            hearts: HeartsState::with_texture(tex_key),
            technique: technique_bg::State::new(),
        }
    }

    pub fn build(&self, params: Params) -> Vec<Actor> {
        self.build_at_elapsed(params, global_elapsed_s())
    }

    pub fn build_at_elapsed(&self, params: Params, elapsed_s: f32) -> Vec<Actor> {
        if matches!(menu_background_style(), MenuBackgroundStyle::Technique)
            && let Some(actors) = self.technique.build_at_elapsed(
                params.active_color_index,
                params.backdrop_rgba,
                params.alpha_mul,
                elapsed_s,
            )
        {
            return actors;
        }
        self.hearts.build_at_elapsed(&params, elapsed_s)
    }
}

fn push_heart(
    out: &mut Vec<Actor>,
    tex_key: &Arc<str>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rgba: [f32; 4],
) {
    out.push(act!(sprite(tex_key.as_ref()):
        align(0.5, 0.5):
        xy(x, y):
        zoomto(w, h):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(-99)
    ));
}

fn menu_background_style() -> MenuBackgroundStyle {
    std::panic::catch_unwind(|| config::get().menu_background_style)
        .unwrap_or(MenuBackgroundStyle::Hearts)
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

    fn first_heart_xy(actors: &[Actor]) -> [f32; 2] {
        let Some(Actor::Sprite { offset, source, .. }) = actors.get(1) else {
            panic!("missing first heart sprite");
        };
        assert_eq!(source.texture_key(), Some("heart.png"));
        *offset
    }

    #[test]
    fn build_reads_shared_elapsed_clock() {
        set_global_elapsed_for_test(2.5);
        let state = HeartsState::new();
        let shared_xy = first_heart_xy(&state.build_at_elapsed(&params(), global_elapsed_s()));
        let explicit_xy = first_heart_xy(&state.build_at_elapsed(&params(), 2.5));
        assert!(
            (shared_xy[0] - explicit_xy[0]).abs() < EPS
                && (shared_xy[1] - explicit_xy[1]).abs() < EPS,
            "shared={shared_xy:?} explicit={explicit_xy:?}"
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
