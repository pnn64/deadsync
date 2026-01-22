use crate::act;
use crate::assets;
use crate::core::space::screen_center_x;
use crate::ui::actors::Actor;

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
    // Get logo's native dimensions from the asset system, with a safe fallback.
    let logo_dims = assets::texture_dims("logo.png").unwrap_or(assets::TexMeta { w: 1, h: 1 });
    let logo_aspect = if logo_dims.h > 0 {
        logo_dims.w as f32 / logo_dims.h as f32
    } else {
        1.0
    };

    // Calculate the final display width of the logo based on the target height and true aspect ratio.
    let logo_h = params.target_h;
    let logo_w = logo_h * logo_aspect;

    // Center both components horizontally.
    let center_x = screen_center_x();
    let logo_top_y = params.top_margin;
    // The dance banner will be centered vertically within the logo's final height.
    let dance_center_y = 0.5f32.mul_add(logo_h, logo_top_y) - params.banner_y_offset_inside;

    vec![
        // The dance banner's width is constrained to the logo's width.
        // `zoomtowidth` will automatically calculate its height while preserving its aspect ratio.
        act!(sprite("dance.png"):
            align(0.5, 0.5):
            xy(center_x, dance_center_y):
            zoomtowidth(logo_w)
        ),
        // The logo's height is set directly.
        // `zoomtoheight` will automatically calculate its width while preserving its aspect ratio.
        act!(sprite("logo.png"):
            align(0.5, 0.0):
            xy(center_x, logo_top_y):
            zoomtoheight(logo_h)
        ),
    ]
}

/// Convenience: build with default params.
pub fn build_logo_default() -> Vec<Actor> {
    build_logo(LogoParams::default())
}
