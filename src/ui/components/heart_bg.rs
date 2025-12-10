use crate::act;
use crate::core::space::*;
use crate::ui::actors::Actor;
use crate::ui::color;
use image;
use std::time::Instant;

// Shared start time so all heart backgrounds (menu, select color, options, etc.)
// stay phase-locked and visually continuous across screens.
static GLOBAL_T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

// ---- tint/placement (from theme) ----
const COLOR_ADD: [i32; 10] = [-1, 0, 0, -1, -1, -1, 0, 0, 0, 0];
const DIFFUSE_ALPHA: [f32; 10] = [0.05, 0.2, 0.1, 0.1, 0.1, 0.1, 0.1, 0.05, 0.1, 0.1];
const XY: [f32; 10] = [
    0.0, 40.0, 80.0, 120.0, 200.0, 280.0, 360.0, 400.0, 480.0, 560.0,
];

// reinterpret “UV velocities” as screen px/sec scale
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

pub struct State {
    pub t0: Instant,
    base_w: f32,
    base_h: f32,
    variants: [usize; 10],
    tex_key: &'static str, // <-- sprite key (must match texture manager)
}

pub struct Params {
    pub active_color_index: i32,
    pub backdrop_rgba: [f32; 4],
    /// Multiplies the per-layer heart alpha (for cross-fades). 1.0 = unchanged.
    pub alpha_mul: f32,
}

impl State {
    pub fn new() -> Self {
        Self::with_texture("heart.png")
    }

    pub fn with_texture(tex_key: &'static str) -> Self {
        // Cache: tex_key -> (w,h). Keeps everything local; no new top-level imports.
        static CACHE: std::sync::OnceLock<
            std::sync::Mutex<std::collections::HashMap<&'static str, (u32, u32)>>,
        > = std::sync::OnceLock::new();
        let map = CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));

        #[inline(always)]
        fn fallback_dims() -> (u32, u32) {
            (668, 566)
        }

        let (w_px, h_px) = {
            // Hit cache
            if let Some(&wh) = map.lock().unwrap().get(tex_key) {
                wh
            } else {
                // Miss: query once, then stash
                let full_path = format!("assets/graphics/{}", tex_key);
                let wh = image::image_dimensions(&full_path).unwrap_or(fallback_dims());
                map.lock().unwrap().insert(tex_key, wh);
                wh
            }
        };

        let variants = [0, 1, 2, 0, 1, 0, 2, 0, 1, 2]; // normal,big,small pattern
        Self {
            t0: *GLOBAL_T0.get_or_init(Instant::now),
            base_w: w_px as f32,
            base_h: h_px as f32,
            variants,
            tex_key,
        }
    }

    pub fn build(&self, params: Params) -> Vec<Actor> {
        let mut actors: Vec<Actor> = Vec::with_capacity(64);

        // backdrop
        let w = screen_width();
        let h = screen_height();
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(params.backdrop_rgba[0], params.backdrop_rgba[1], params.backdrop_rgba[2], params.backdrop_rgba[3]):
            z(-100)
        ));

        // aspect
        let aspect = self.base_h / self.base_w;

        // original widths (approx):
        const BW_BIG: f32 = 668.0;
        const BW_NORMAL: f32 = 543.0;
        const BW_SMALL: f32 = 400.0;

        let scale_k = (self.base_w * 0.6) / BW_BIG;
        let var_w = [BW_NORMAL * scale_k, BW_BIG * scale_k, BW_SMALL * scale_k]; // [normal, big, small]
        let var_h = [var_w[0] * aspect, var_w[1] * aspect, var_w[2] * aspect];

        // motion
        let speed_scale_px = w.max(h) * 1.3;
        let t = self.t0.elapsed().as_secs_f32();

        const PHI: f32 = 0.618_034;

        for i in 0..10 {
            let variant = self.variants[i];
            let heart_w = var_w[variant];
            let heart_h = var_h[variant];
            let half_w = heart_w * 0.5;
            let half_h = heart_h * 0.5;

            let mut rgba = color::decorative_rgba(params.active_color_index + COLOR_ADD[i]);
            rgba[3] = DIFFUSE_ALPHA[i] * params.alpha_mul;

            let vx_px = -2.0 * UV_VEL[i][0] * speed_scale_px;
            let vy_px = -2.0 * UV_VEL[i][1] * speed_scale_px;

            let start_x = (XY[i] + (i as f32) * (w / 10.0)) % w;
            let start_y = (XY[i] * 0.5 + (i as f32) * (h / 10.0) * PHI) % h;

            let x_raw = start_x + vx_px * t;
            let y_raw = start_y + vy_px * t;

            let x0 = x_raw.rem_euclid(w);
            let y0 = y_raw.rem_euclid(h);

            let mut x_offsets = [0.0f32; 3];
            let mut y_offsets = [0.0f32; 3];
            let mut nx = 1usize;
            let mut ny = 1usize;

            if x0 < half_w {
                x_offsets[nx] = w;
                nx += 1;
            }
            if x0 > w - half_w {
                x_offsets[nx] = -w;
                nx += 1;
            }
            if y0 < half_h {
                y_offsets[ny] = h;
                ny += 1;
            }
            if y0 > h - half_h {
                y_offsets[ny] = -h;
                ny += 1;
            }

            for xi in 0..nx {
                for yi in 0..ny {
                    let x = x0 + x_offsets[xi];
                    let y = y0 + y_offsets[yi];

                    actors.push(act!(sprite(self.tex_key):  // <-- use the key, not a path
                        align(0.5, 0.5):
                        xy(x, y):
                        zoomto(heart_w, heart_h):
                        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                        z(-99)
                    ));
                }
            }
        }

        actors
    }
}
