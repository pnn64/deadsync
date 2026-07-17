use deadlib_present::{color, space::screen_center_x};
use deadsync_profile as profile_data;

const ARROW_BREAKDOWN_RGBA: [[f32; 4]; 8] = [
    [1.0, 0.0, 0.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [1.0, 1.0, 0.0, 1.0],
    color::rgba_hex("#B54DFF"),
    color::rgba_hex("#FF8A00"),
    color::rgba_hex("#00D7FF"),
    [1.0, 1.0, 1.0, 1.0],
];

#[inline(always)]
pub(crate) fn arrow_breakdown_rgba(col_idx: usize) -> [f32; 4] {
    ARROW_BREAKDOWN_RGBA
        .get(col_idx)
        .copied()
        .unwrap_or([1.0; 4])
}

#[inline(always)]
pub(crate) fn arrow_code_rgba(direction_code: u8) -> [f32; 4] {
    direction_code
        .checked_sub(1)
        .map_or([1.0; 4], |code| arrow_breakdown_rgba(code as usize))
}

#[inline(always)]
pub(crate) fn pane_origin_x(controller: profile_data::PlayerSide) -> f32 {
    match controller {
        profile_data::PlayerSide::P1 => screen_center_x() - 155.0,
        profile_data::PlayerSide::P2 => screen_center_x() + 155.0,
    }
}

#[inline(always)]
pub(crate) fn pane3_origin_x(controller: profile_data::PlayerSide, num_cols: usize) -> f32 {
    let origin = pane_origin_x(controller);
    if num_cols == 8 && controller == profile_data::PlayerSide::P2 {
        origin - 310.0
    } else {
        origin
    }
}

pub(crate) const fn eval_style_alpha(
    transparent: bool,
    opaque_alpha: f32,
    transparent_alpha: f32,
) -> f32 {
    if transparent {
        transparent_alpha
    } else {
        opaque_alpha
    }
}
