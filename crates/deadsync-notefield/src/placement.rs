use deadsync_rules::scroll::ScrollSpeedSetting;
use glam::{Mat4 as Matrix4, Vec3 as Vector3};

use crate::transforms::sm_scale;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMiniIndicatorPosition {
    Default,
    UnderUpArrow,
}

#[derive(Clone, Copy, Debug)]
pub struct ZmodLayoutParams {
    pub judgment_height: f32,
    pub has_error_bar: bool,
    pub has_judgment_texture: bool,
    pub error_bar_up: bool,
    pub has_measure_counter: bool,
    pub measure_counter_up: bool,
    pub broken_run: bool,
    pub mini_indicator_position: LayoutMiniIndicatorPosition,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HudLayoutOffsets {
    pub judgment_extra_y: f32,
    pub combo_extra_y: f32,
    pub error_bar_extra_y: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HudLayoutParams {
    pub zmod: ZmodLayoutParams,
    pub has_judgment_texture: bool,
    pub error_bar_up: bool,
    pub error_bar_offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZmodLayoutYs {
    pub measure_counter_y: Option<f32>,
    pub subtractive_scoring_y: f32,
    pub subtractive_scoring_addx: f32,
    pub combo_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudLayoutYs {
    pub judgment_y: f32,
    pub error_bar_y: f32,
    pub error_bar_max_h: f32,
    pub zmod_layout: ZmodLayoutYs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldPlacement {
    P1,
    P2,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewOverride {
    pub field_zoom: Option<f32>,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub force_center_1player: bool,
    pub center_receptors_y: bool,
    pub receptor_y: Option<f32>,
    pub edit_beat_bars: bool,
    pub hide_combo: bool,
    pub hide_display_mods: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProxyCaptureRequests {
    pub note_field: bool,
    pub judgment: bool,
    pub combo: bool,
}

pub fn player_metric_y(
    center_y: f32,
    offset_y: f32,
    reverse: f32,
    normal_offset: f32,
    reverse_offset: f32,
) -> f32 {
    center_y + offset_y + normal_offset * (1.0 - reverse) + reverse_offset * reverse
}

fn rage_frustum(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        2.0 * zn / (r - l),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 * zn / (t - b),
        0.0,
        0.0,
        (r + l) / (r - l),
        (t + b) / (t - b),
        -(zf + zn) / (zf - zn),
        -1.0,
        0.0,
        0.0,
        -2.0 * zf * zn / (zf - zn),
        0.0,
    ])
}

pub fn notefield_view_proj(
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
    center_y: f32,
    tilt: f32,
    skew: f32,
    reverse: bool,
) -> Option<Matrix4> {
    if !screen_w.is_finite() || !screen_h.is_finite() || screen_w <= 0.0 || screen_h <= 0.0 {
        return None;
    }

    let half_w = 0.5 * screen_w;
    let half_h = 0.5 * screen_h;

    let fov_deg = 45.0_f32;
    let theta = (0.5 * fov_deg).to_radians();
    let tan_theta = theta.tan();
    if !tan_theta.is_finite() || tan_theta.abs() < 1e-6 {
        return None;
    }
    let dist = half_w / tan_theta;
    if !dist.is_finite() || dist <= 0.0 {
        return None;
    }

    let vanish_x = sm_scale(skew, 0.1, 1.0, playfield_center_x, half_w);
    let vanish_y = center_y;

    let near = 1.0_f32;
    let far = dist + 1000.0_f32;

    let mut vp_x = sm_scale(vanish_x, 0.0, screen_w, screen_w, 0.0);
    let mut vp_y = sm_scale(vanish_y, 0.0, screen_h, screen_h, 0.0);
    vp_x -= half_w;
    vp_y -= half_h;
    let l = (vp_x - half_w) / dist;
    let r = (vp_x + half_w) / dist;
    let b = (vp_y + half_h) / dist;
    let t = (vp_y - half_h) / dist;
    let proj = rage_frustum(l, r, b, t, near, far);

    let eye = Vector3::new(-vp_x + half_w, -vp_y + half_h, dist);
    let at = Vector3::new(-vp_x + half_w, -vp_y + half_h, 0.0);
    let view = Matrix4::look_at_rh(eye, at, Vector3::Y);

    let reverse_mult = if reverse { -1.0 } else { 1.0 };
    let tilt = tilt.clamp(-1.0, 1.0);
    let tilt_deg = (-30.0 * tilt) * reverse_mult;
    let tilt_abs = tilt.abs();
    let tilt_scale = 1.0 - 0.1 * tilt_abs;
    let y_offset_screen = if tilt > 0.0 {
        -45.0 * tilt
    } else {
        20.0 * tilt
    } * reverse_mult;
    let y_offset_world = -y_offset_screen;

    let pivot_x = playfield_center_x - half_w;
    let pivot_y = half_h - center_y;
    let world_to_screen = Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        half_w, half_h, 0.0, 1.0,
    ]);
    let field = Matrix4::from_translation(Vector3::new(0.0, y_offset_world, 0.0))
        * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
        * Matrix4::from_rotation_x(tilt_deg.to_radians())
        * Matrix4::from_scale(Vector3::new(tilt_scale, tilt_scale, 1.0))
        * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0));

    Some((proj * view) * world_to_screen * field)
}

pub fn combo_actor_zoom(mini: f32) -> f32 {
    if !mini.is_finite() || mini <= 0.0 {
        1.0
    } else {
        0.5_f32.powf(mini)
    }
}

pub fn effective_mini_value(mini_percent: f32, fallback_mini_percent: f32, big_effect: f32) -> f32 {
    let mut mini_percent = if mini_percent.is_finite() {
        mini_percent
    } else {
        fallback_mini_percent
    };
    if big_effect > f32::EPSILON {
        mini_percent -= 100.0;
    }
    mini_percent.clamp(-100.0, 150.0) / 100.0
}

pub fn average_error_bar_mini_scale(mini: f32) -> f32 {
    (1.1 - 0.545 * mini).max(0.0)
}

pub fn hud_y(
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
    reverse: bool,
    reverse_level: f32,
) -> f32 {
    if reverse {
        reverse_y + (centered_y - reverse_y) * reverse_level
    } else {
        normal_y + (centered_y - normal_y) * reverse_level
    }
}

pub fn zmod_layout_ys(
    judgment_y: f32,
    combo_y: f32,
    reverse: bool,
    params: ZmodLayoutParams,
) -> ZmodLayoutYs {
    let mut top_y = judgment_y - params.judgment_height * 0.5;
    let mut bottom_y = judgment_y + params.judgment_height * 0.5;

    if params.has_error_bar && params.has_judgment_texture {
        if params.error_bar_up {
            top_y -= 15.0;
        } else {
            bottom_y += 15.0;
        }
    }

    let mut measure_counter_y = None;
    if params.has_measure_counter {
        if params.measure_counter_up {
            let mut y = top_y - 8.0;
            top_y -= 20.0;
            if params.broken_run {
                y -= 16.0;
            }
            measure_counter_y = Some(y);
        } else {
            measure_counter_y = Some(bottom_y + 8.0);
            bottom_y += 21.0;
        }
    }

    let (subtractive_scoring_y, subtractive_scoring_addx) = match params.mini_indicator_position {
        LayoutMiniIndicatorPosition::Default => {
            if params.has_measure_counter && params.measure_counter_up {
                let y = bottom_y + 8.0;
                bottom_y += 16.0;
                (y, 0.0)
            } else {
                let y = top_y - 8.0;
                top_y -= 16.0;
                (y, 0.0)
            }
        }
        LayoutMiniIndicatorPosition::UnderUpArrow => {
            if params.has_measure_counter && params.measure_counter_up {
                let y = top_y + 16.0;
                top_y -= 16.0;
                (y, -60.0)
            } else {
                let y = top_y - 8.0;
                top_y -= 16.0;
                (y, 0.0)
            }
        }
    };
    let combo_y = if reverse {
        combo_y.min(top_y - 20.0)
    } else {
        combo_y.max(bottom_y + 20.0)
    };

    ZmodLayoutYs {
        measure_counter_y,
        subtractive_scoring_y,
        subtractive_scoring_addx,
        combo_y,
    }
}

pub fn hud_layout_ys(
    judgment_y: f32,
    combo_y: f32,
    reverse: bool,
    offsets: HudLayoutOffsets,
    params: HudLayoutParams,
) -> HudLayoutYs {
    let placed_judgment_y = judgment_y + offsets.judgment_extra_y;
    let mut zmod_layout = zmod_layout_ys(judgment_y, combo_y, reverse, params.zmod);
    zmod_layout.combo_y += offsets.combo_extra_y;
    let (error_bar_y, error_bar_max_h) = if !params.has_judgment_texture {
        (judgment_y + offsets.error_bar_extra_y, 30.0)
    } else if params.error_bar_up {
        (
            judgment_y - params.error_bar_offset + offsets.error_bar_extra_y,
            10.0,
        )
    } else {
        (
            judgment_y + params.error_bar_offset + offsets.error_bar_extra_y,
            10.0,
        )
    };
    HudLayoutYs {
        judgment_y: placed_judgment_y,
        error_bar_y,
        error_bar_max_h,
        zmod_layout,
    }
}

pub fn default_column_x(local_col: usize, num_cols: usize) -> f32 {
    (local_col as f32 - (num_cols.saturating_sub(1) as f32 * 0.5)) * 64.0
}

pub trait LaneColumnX {
    fn to_f32(self) -> f32;
}

impl LaneColumnX for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}

impl LaneColumnX for i32 {
    fn to_f32(self) -> f32 {
        self as f32
    }
}

pub fn fill_lane_col_offsets<T: Copy + LaneColumnX>(
    out: &mut [f32],
    noteskin_cols: Option<&[T]>,
    num_cols: usize,
    spacing: f32,
    zoom: f32,
) {
    for (i, dst) in out.iter_mut().take(num_cols).enumerate() {
        let col_x = noteskin_cols
            .and_then(|cols| cols.get(i).copied())
            .map_or_else(|| default_column_x(i, num_cols), LaneColumnX::to_f32);
        *dst = col_x * spacing * zoom;
    }
}
