use crate::act;
use crate::core::space::{screen_width, screen_height};
use crate::ui::actors::Actor;
use crate::ui::color;
use std::time::Instant;
use std::sync::OnceLock;

// Shared start time for phase-locked animations across screens.
static GLOBAL_T0: OnceLock<Instant> = OnceLock::new();

// ---- Constants ----
const COLOR_ADD: [i32; 10] = [-1, 0, 0, -1, -1, -1, 0, 0, 0, 0];
const DIFFUSE_ALPHA: [f32; 10] = [0.05, 0.2, 0.1, 0.1, 0.1, 0.1, 0.1, 0.05, 0.1, 0.1];
const XY: [f32; 10] = [
    0.0, 40.0, 80.0, 120.0, 200.0, 280.0, 360.0, 400.0, 480.0, 560.0,
];

// UV velocities (screen px/sec scale)
const UV_VEL: [[f32; 2]; 10] = [
    [0.03, 0.01], [0.03, 0.02], [0.03, 0.01], [0.02, 0.02], [0.03, 0.03],
    [0.02, 0.02], [0.03, 0.01], [-0.03, 0.01], [0.05, 0.03], [0.03, 0.04],
];

const VARIANTS: [usize; 10] = [0, 1, 2, 0, 1, 0, 2, 0, 1, 2];
const DEFAULT_DIMS: (f32, f32) = (668.0, 566.0); // Standard heart.png dimensions
const BW_BIG: f32 = 668.0;
const BW_NORMAL: f32 = 543.0;
const BW_SMALL: f32 = 400.0;
const PHI: f32 = 0.618_034;

pub struct State {
    t0: Instant,
    tex_key: &'static str,
    base_w: f32,
    base_h: f32,
}

pub struct Params {
    pub active_color_index: i32,
    pub backdrop_rgba: [f32; 4],
    pub alpha_mul: f32,
}

impl State {
    pub fn new() -> Self {
        Self::with_texture("heart.png")
    }

    pub fn with_texture(tex_key: &'static str) -> Self {
        // Optimization: Removed image crate I/O and Mutex cache.
        // We assume standard assets. If dynamic sizing is strictly required later,
        // it should be passed in via arguments, not read from disk here.
        let (w, h) = if tex_key == "heart.png" { DEFAULT_DIMS } else { DEFAULT_DIMS };
        
        Self {
            t0: *GLOBAL_T0.get_or_init(Instant::now),
            tex_key,
            base_w: w,
            base_h: h,
        }
    }

    pub fn build(&self, params: Params) -> Vec<Actor> {
        // Pre-allocate for 1 background + 10 hearts (up to 4 clones each for wrapping)
        let mut actors: Vec<Actor> = Vec::with_capacity(41);

        let w = screen_width();
        let h = screen_height();

        // Backdrop
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
            z(-100)
        ));

        // Layout calcs
        let aspect = self.base_h / self.base_w;
        let scale_k = (self.base_w * 0.6) / BW_BIG;
        
        // Precompute variant sizes
        let var_w = [BW_NORMAL * scale_k, BW_BIG * scale_k, BW_SMALL * scale_k];
        let var_h = [var_w[0] * aspect, var_w[1] * aspect, var_w[2] * aspect];

        let speed_scale_px = w.max(h) * 1.3;
        let t = self.t0.elapsed().as_secs_f32();

        for i in 0..10 {
            let variant = VARIANTS[i];
            let heart_w = var_w[variant];
            let heart_h = var_h[variant];
            let half_w = heart_w * 0.5;
            let half_h = heart_h * 0.5;

            // Optimization: direct array access, no redundant lookups
            let mut rgba = color::decorative_rgba(params.active_color_index + COLOR_ADD[i]);
            rgba[3] = DIFFUSE_ALPHA[i] * params.alpha_mul;

            // Movement
            let vx_px = -2.0 * UV_VEL[i][0] * speed_scale_px;
            let vy_px = -2.0 * UV_VEL[i][1] * speed_scale_px;

            let start_x = (XY[i] + (i as f32) * (w * 0.1)) % w;
            let start_y = (XY[i] * 0.5 + (i as f32) * (h * 0.1) * PHI) % h;

            let x_raw = start_x + vx_px * t;
            let y_raw = start_y + vy_px * t;

            let x0 = x_raw.rem_euclid(w);
            let y0 = y_raw.rem_euclid(h);

            // Optimization: Flat wrap logic to avoid nested loops and array construction
            let wrap_x = if x0 < half_w { Some(x0 + w) } else if x0 > w - half_w { Some(x0 - w) } else { None };
            let wrap_y = if y0 < half_h { Some(y0 + h) } else if y0 > h - half_h { Some(y0 - h) } else { None };

            // Primary heart
            actors.push(act!(sprite(self.tex_key): align(0.5, 0.5): xy(x0, y0): zoomto(heart_w, heart_h): diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(-99)));

            // Horizontal wrap
            if let Some(wx) = wrap_x {
                actors.push(act!(sprite(self.tex_key): align(0.5, 0.5): xy(wx, y0): zoomto(heart_w, heart_h): diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(-99)));
            }

            // Vertical wrap
            if let Some(wy) = wrap_y {
                actors.push(act!(sprite(self.tex_key): align(0.5, 0.5): xy(x0, wy): zoomto(heart_w, heart_h): diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(-99)));
            }

            // Corner wrap
            if let (Some(wx), Some(wy)) = (wrap_x, wrap_y) {
                actors.push(act!(sprite(self.tex_key): align(0.5, 0.5): xy(wx, wy): zoomto(heart_w, heart_h): diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(-99)));
            }
        }

        actors
    }
}
