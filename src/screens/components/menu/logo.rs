use crate::act;
use crate::assets;
use deadsync_present::actors::Actor;
use deadsync_present::space::screen_center_x;
use std::sync::atomic::{AtomicU32, Ordering};

// Cached logo aspect ratio (w/h). The logo texture never changes size at
// runtime, so it is resolved once and reused. A stored value of 0 means "not
// yet resolved"; the 1x1 fallback is never cached.
static LOGO_ASPECT_BITS: AtomicU32 = AtomicU32::new(0);

fn logo_aspect() -> f32 {
    let cached = f32::from_bits(LOGO_ASPECT_BITS.load(Ordering::Relaxed));
    if cached > 0.0 {
        return cached;
    }
    let Some(dims) = assets::texture_dims("logo.png") else {
        return 1.0;
    };
    if dims.h == 0 {
        return 1.0;
    }
    let aspect = dims.w as f32 / dims.h as f32;
    // Only memoize a real measurement, not the 1x1 placeholder dimensions that
    // the asset system may report before the texture is loaded.
    if dims.w > 1 || dims.h > 1 {
        LOGO_ASPECT_BITS.store(aspect.to_bits(), Ordering::Relaxed);
    }
    aspect
}

/// Parameters to tweak the layout easily.
#[derive(Clone, Copy, Debug)]
pub struct LogoParams {
    pub target_h: f32,
    pub top_margin: f32,
    /// Positive values move the banner *up* inside the logo.
    pub banner_y_offset_inside: f32,
}

impl Default for LogoParams {
    fn default() -> Self {
        Self {
            target_h: 238.0,
            top_margin: 102.0,
            banner_y_offset_inside: 0.0,
        }
    }
}

/// Build the “banner inside logo” stack with the actor DSL.
/// Returns a `Vec<Actor>` to be included in a screen's actor list.
pub fn build_logo(params: LogoParams) -> Vec<Actor> {
    let mut out = Vec::with_capacity(2);
    push_logo(&mut out, params, 1.0);
    out
}

/// Append the “banner inside logo” stack directly into `out`, scaling each
/// sprite's alpha by `alpha_multiplier`.
pub fn push_logo(out: &mut Vec<Actor>, params: LogoParams, alpha_multiplier: f32) {
    // Resolve the logo's display size from its (cached) native aspect ratio.
    let logo_aspect = logo_aspect();

    // Calculate the final display width of the logo based on the target height and true aspect ratio.
    let logo_h = params.target_h;
    let logo_w = logo_h * logo_aspect;

    // Center both components horizontally.
    let center_x = screen_center_x();
    let logo_top_y = params.top_margin;
    // The dance banner will be centered vertically within the logo's final height.
    let dance_center_y = 0.5f32.mul_add(logo_h, logo_top_y) - params.banner_y_offset_inside;

    out.reserve(2);
    // The dance banner's width is constrained to the logo's width.
    // `zoomtowidth` will automatically calculate its height while preserving its aspect ratio.
    out.push(act!(sprite("dance.png"):
        align(0.5, 0.5):
        xy(center_x, dance_center_y):
        zoomtowidth(logo_w):
        diffuse(1.0, 1.0, 1.0, alpha_multiplier)
    ));
    // The logo's height is set directly.
    // `zoomtoheight` will automatically calculate its width while preserving its aspect ratio.
    out.push(act!(sprite("logo.png"):
        align(0.5, 0.0):
        xy(center_x, logo_top_y):
        zoomtoheight(logo_h):
        diffuse(1.0, 1.0, 1.0, alpha_multiplier)
    ));
}

/// Convenience: build with default params.
pub fn build_logo_default() -> Vec<Actor> {
    build_logo(LogoParams::default())
}

/// Convenience: append with default params and an alpha multiplier.
pub fn push_logo_default(out: &mut Vec<Actor>, alpha_multiplier: f32) {
    push_logo(out, LogoParams::default(), alpha_multiplier);
}
