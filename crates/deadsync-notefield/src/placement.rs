use deadsync_core::input::MAX_COLS;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_theme::NotefieldStyle;
use glam::{Mat4 as Matrix4, Vec3 as Vector3};
use std::array::from_fn;

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
pub(crate) struct HudLayoutOffsets {
    pub judgment_extra_y: f32,
    pub combo_extra_y: f32,
    pub error_bar_extra_y: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HudLayoutParams {
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldLayoutRequest {
    pub style: NotefieldStyle,
    pub placement: FieldPlacement,
    pub num_players: usize,
    pub single_style: bool,
    pub double_style: bool,
    pub center_one_player: bool,
    pub screen_width: f32,
    pub screen_center_x: f32,
    pub screen_center_y: f32,
    pub num_cols: usize,
    pub field_zoom: f32,
    pub notefield_offset_x: f32,
    pub notefield_offset_y: f32,
    pub receptor_y_override: Option<f32>,
    pub center_receptors_y: bool,
    pub centered_scroll: f32,
    pub column_reverse_percent: [f32; MAX_COLS],
    pub column_dirs: [f32; MAX_COLS],
    pub song_lua_column_y_offsets: [f32; MAX_COLS],
    pub judgment_offset_x: f32,
    pub combo_offset_x: f32,
    pub error_bar_offset_x: f32,
    pub hud_offsets: HudLayoutOffsets,
    pub hud_params: HudLayoutParams,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldLayout {
    pub playfield_center_x: f32,
    pub layout_center_x: f32,
    pub notefield_offset_x: f32,
    pub notefield_offset_y: f32,
    pub receptor_y_normal: f32,
    pub receptor_y_reverse: f32,
    pub receptor_y_centered: f32,
    pub centered_percent: f32,
    pub column_reverse_percent: [f32; MAX_COLS],
    pub column_dirs: [f32; MAX_COLS],
    pub column_receptor_ys: [f32; MAX_COLS],
    pub hud_reverse: bool,
    pub judgment_x: f32,
    pub combo_x: f32,
    pub error_bar_x: f32,
    pub hud_layout: HudLayoutYs,
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

pub(crate) fn player_metric_y(
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

pub(crate) fn notefield_view_proj(
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
    let view = glam::camera::rh::view::look_at_mat4(eye, at, Vector3::Y);

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

pub(crate) fn combo_actor_zoom(mini: f32) -> f32 {
    if !mini.is_finite() || mini <= 0.0 {
        1.0
    } else {
        0.5_f32.powf(mini)
    }
}

pub(crate) fn effective_mini_value(
    mini_percent: f32,
    fallback_mini_percent: f32,
    big_effect: f32,
) -> f32 {
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

pub(crate) fn average_error_bar_mini_scale(mini: f32) -> f32 {
    (1.1 - 0.545 * mini).max(0.0)
}

pub(crate) fn hud_y(
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

pub(crate) fn zmod_layout_ys(
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

pub(crate) fn hud_layout_ys(
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

fn field_receptor_y(
    reverse_percent: f32,
    centered_percent: f32,
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
) -> f32 {
    let reverse_y = normal_y + (reverse_y - normal_y) * reverse_percent.clamp(0.0, 1.0);
    (centered_y - reverse_y).mul_add(centered_percent, reverse_y)
}

pub(crate) fn field_layout(request: FieldLayoutRequest) -> FieldLayout {
    let style = request.style;
    let clamped_width = request
        .screen_width
        .clamp(style.layout_width_min, style.layout_width_max);
    let side_sign = match request.placement {
        FieldPlacement::P1 => -1.0,
        FieldPlacement::P2 => 1.0,
    };
    let centered_one_side =
        request.num_players == 1 && request.single_style && request.center_one_player;
    let centered_both_sides = request.num_players == 1 && request.double_style;
    let base_playfield_center_x = if centered_both_sides || centered_one_side {
        request.screen_center_x
    } else {
        request.screen_center_x + side_sign * clamped_width * style.side_center_x_ratio
    };
    let notefield_offset_x = side_sign * request.notefield_offset_x;
    let playfield_center_x = base_playfield_center_x + notefield_offset_x;
    let layout_center_x = if request.num_players == 1 && (centered_both_sides || centered_one_side)
    {
        request.screen_center_x
    } else {
        playfield_center_x
    };

    let receptor_y_override = request
        .receptor_y_override
        .map(|y| y + request.notefield_offset_y);
    let receptor_y_centered =
        receptor_y_override.unwrap_or(request.screen_center_y + request.notefield_offset_y);
    let receptor_y_normal = receptor_y_override.unwrap_or({
        if request.center_receptors_y {
            receptor_y_centered
        } else {
            request.screen_center_y + style.receptor_normal_y + request.notefield_offset_y
        }
    });
    let receptor_y_reverse = receptor_y_override.unwrap_or({
        if request.center_receptors_y {
            receptor_y_centered
        } else {
            request.screen_center_y + style.receptor_reverse_y + request.notefield_offset_y
        }
    });
    let centered_percent = if request.receptor_y_override.is_some() || request.center_receptors_y {
        1.0
    } else {
        request.centered_scroll
    };
    let column_reverse_percent = from_fn(|i| {
        if i < request.num_cols {
            request.column_reverse_percent[i]
        } else {
            0.0
        }
    });
    let column_dirs = from_fn(|i| {
        if i < request.num_cols {
            request.column_dirs[i]
        } else {
            1.0
        }
    });
    let column_receptor_ys = from_fn(|i| {
        if i >= request.num_cols {
            return receptor_y_normal;
        }
        field_receptor_y(
            column_reverse_percent[i],
            centered_percent,
            receptor_y_normal,
            receptor_y_reverse,
            receptor_y_centered,
        ) + request.song_lua_column_y_offsets[i] * request.field_zoom
    });

    let hud_reverse = column_reverse_percent[0] >= 0.999_9;
    let judgment_y = hud_y(
        request.screen_center_y + style.judgment_normal_y + request.notefield_offset_y,
        request.screen_center_y + style.judgment_reverse_y + request.notefield_offset_y,
        receptor_y_centered + style.judgment_centered_y,
        hud_reverse,
        centered_percent,
    );
    let combo_y = hud_y(
        request.screen_center_y + style.combo_normal_y + request.notefield_offset_y,
        request.screen_center_y + style.combo_reverse_y + request.notefield_offset_y,
        receptor_y_centered + style.combo_centered_y,
        hud_reverse,
        centered_percent,
    );
    let hud_layout = hud_layout_ys(
        judgment_y,
        combo_y,
        hud_reverse,
        request.hud_offsets,
        request.hud_params,
    );

    FieldLayout {
        playfield_center_x,
        layout_center_x,
        notefield_offset_x,
        notefield_offset_y: request.notefield_offset_y,
        receptor_y_normal,
        receptor_y_reverse,
        receptor_y_centered,
        centered_percent,
        column_reverse_percent,
        column_dirs,
        column_receptor_ys,
        hud_reverse,
        judgment_x: playfield_center_x + request.judgment_offset_x,
        combo_x: playfield_center_x + request.combo_offset_x,
        error_bar_x: playfield_center_x + request.error_bar_offset_x,
        hud_layout,
    }
}

pub(crate) fn default_column_x(local_col: usize, num_cols: usize) -> f32 {
    (local_col as f32 - (num_cols.saturating_sub(1) as f32 * 0.5)) * 64.0
}

pub(crate) trait LaneColumnX {
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

pub(crate) fn fill_lane_col_offsets<T: Copy + LaneColumnX>(
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

#[cfg(test)]
mod tests {
    // Decimal components in these visual fixtures are authored RGB values.
    #![allow(clippy::approx_constant)]

    use super::{
        FieldLayoutRequest, FieldPlacement, HudLayoutOffsets, HudLayoutParams,
        LayoutMiniIndicatorPosition, ZmodLayoutParams, field_layout,
    };
    use deadsync_core::input::MAX_COLS;
    use deadsync_theme::{
        ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle,
        CounterHudStyle, ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle,
        MiniIndicatorStyle, NotefieldActorStyle, NotefieldStyle, ReceptorStyle,
    };

    fn style() -> NotefieldStyle {
        NotefieldStyle {
            layout_width_min: 640.0,
            layout_width_max: 854.0,
            side_center_x_ratio: 0.25,
            receptor_normal_y: -125.0,
            receptor_reverse_y: 145.0,
            receptor: ReceptorStyle {
                target_z: 100,
                press_glow_z: 105,
                hold_explosion_z: 145,
            },
            actors: NotefieldActorStyle {
                hold_body_z: 110,
                hold_cap_z: 110,
                hold_glow_z: 111,
                tap_explosion_z: 150,
                mine_explosion_z: 101,
                note_z: 140,
                mine_core_size_ratio: 0.45,
            },
            judgment_normal_y: -30.0,
            judgment_reverse_y: 30.0,
            judgment_centered_y: 95.0,
            combo_normal_y: 30.0,
            combo_reverse_y: -30.0,
            combo_centered_y: 155.0,
            judgment_height: 40.0,
            error_bar_offset_y: 25.0,
            measure_line_overscan_y: 400.0,
            measure_line_z: 80,
            measure_cue_scroll_color: [0.824, 0.706, 0.549],
            measure_cue_bpm_color: [1.0, 1.0, 0.0],
            measure_cue_delay_color: [1.0, 0.45, 0.75],
            measure_cue_stop_color: [1.0, 0.0, 0.0],
            measure_cue_alpha: 0.7,
            edit_measure_number_font: "miso",
            column_cue: ColumnCueStyle {
                top_y: 80.0,
                reverse_anchor_y: 304.0,
                crossover_height_trim: 270.0,
                body_fade: 0.333,
                base_alpha: 0.12,
                normal_color: [0.3, 1.0, 1.0],
                mine_color: [1.0, 0.0, 0.0],
                countdown_normal_y: 160.0,
                countdown_reverse_y: 340.0,
                countdown_color: [1.0, 1.0, 1.0],
                countdown_zoom: 0.5,
                body_z: 90,
                countdown_z: 200,
            },
            column_flash: ColumnFlashStyle {
                default_layout: ColumnFlashLayoutStyle {
                    top_y: 80.0,
                    height_trim: 0.0,
                    reverse_trim: 0.0,
                    fade: 0.333,
                },
                compact_layout: ColumnFlashLayoutStyle {
                    top_y: 70.0,
                    height_trim: 270.0,
                    reverse_trim: 30.0,
                    fade: 0.2,
                },
                reverse_anchor_y: 304.0,
                normal_alpha: 0.66,
                dimmed_alpha: 0.3,
                miss_color: [1.0, 0.0, 0.0],
                decent_color: [0.70, 0.36, 1.0],
                way_off_color: [0.788, 0.522, 0.369],
                great_color: [0.4, 0.788, 0.333],
                excellent_color: [0.886, 0.612, 0.094],
                fantastic_color: [1.0, 1.0, 1.0],
                fantastic_blue_color: [0.129, 0.8, 0.91],
                z: 91,
            },
            counter_hud: CounterHudStyle {
                text_z: 85,
                shadow_len: 1.0,
                base_zoom: 0.35,
                lookahead_zoom_step: 0.05,
                vertical_step_y: 20.0,
                left_column_scale: 4.0 / 3.0,
                horizontal_span: 2.0,
                break_lookahead_color: [0.4, 0.4, 0.4, 1.0],
                break_current_color: [0.5, 0.5, 0.5, 1.0],
                stream_lookahead_color: [0.45, 0.45, 0.45, 1.0],
                ratio_color: [1.0, 1.0, 1.0, 1.0],
                total_color: [0.5, 0.5, 0.5, 1.0],
                broken_y_offset: 15.0,
                broken_vertical_y_offset: -15.0,
                broken_vertical_x_scale: 4.0 / 3.0,
                broken_color: [1.0, 1.0, 1.0, 0.7],
                run_active_color: [1.0, 1.0, 1.0, 1.0],
                run_inactive_color: [0.5, 0.5, 0.5, 1.0],
            },
            mini_indicator: MiniIndicatorStyle {
                column_offset: 1.0,
                under_up_x_offset: -45.0,
                unanchored_x_offset: -12.0,
                failed_color: [0.5, 0.5, 0.5],
                shadow_len: 1.0,
                text_z: 85,
            },
            judgment_feedback: JudgmentFeedbackStyle {
                tap_front_z: 200,
                tap_back_z: 95,
                split_overlay_alpha: 0.5,
                held_miss_normal_y: -50.0,
                held_miss_reverse_y: 110.0,
                held_miss_z: 196,
                hold_normal_y: -90.0,
                hold_reverse_y: 90.0,
                hold_z: 195,
                hold_initial_zoom: 25.6 / 140.0,
                hold_final_zoom: 32.0 / 140.0,
            },
            combo_feedback: ComboFeedbackStyle {
                threshold: 4,
                milestone_z: 89,
                number_z: 90,
                number_zoom: 0.75,
                shadow_len: 1.0,
                miss_color: [1.0, 0.0, 0.0, 1.0],
                burst_duration: 0.5,
                burst_start_zoom: 2.0,
                burst_end_zoom: 1.0,
                burst_start_alpha: 0.5,
                burst_rotation_deg: 90.0,
                hundred_start_zoom: 0.25,
                hundred_end_zoom: 2.0,
                hundred_start_alpha: 0.6,
                hundred_start_rotation_deg: 10.0,
                mini_duration: 0.4,
                mini_start_zoom: 0.25,
                mini_end_zoom: 1.8,
                mini_start_alpha: 1.0,
                mini_start_rotation_deg: 10.0,
                thousand_start_zoom: 0.25,
                thousand_end_zoom: 3.0,
                thousand_start_alpha: 0.7,
                thousand_x_travel: 100.0,
            },
            error_bar: ErrorBarStyle {
                colorful_width: 160.0,
                colorful_height: 10.0,
                average_width: 325.0,
                average_height: 7.0,
                monochrome_width: 240.0,
                tick_width: 2.0,
                colorful_border_size: 4.0,
                average_tick_padding: 4.0,
                monochrome_border_size: 2.0,
                monochrome_center_width: 2.0,
                monochrome_line_width: 1.0,
                colorful_tick_duration: 0.5,
                monochrome_tick_duration: 0.75,
                average_tick_extra_height: 75.0,
                monochrome_background_alpha: 0.5,
                line_alpha: 0.3,
                lines_fade_start: 2.5,
                lines_fade_duration: 0.5,
                label_fade_duration: 0.5,
                label_hold: 2.0,
                label_x_ratio: 0.25,
                label_zoom: 0.7,
                center_tick_width: 1.0,
                highlight_inactive_alpha: 0.3,
                offset_indicator_duration: 0.5,
                offset_indicator_gap: 6.0,
                offset_indicator_zoom: 0.25,
                offset_indicator_shadow_len: 1.0,
                long_average_tick_duration: 0.5,
                long_average_tick_extra_height: 65.0,
                long_average_tick_width: 1.0,
                text_duration: 0.5,
                text_x_offset: 40.0,
                text_zoom: 0.25,
                text_shadow_len: 1.0,
                background_color: [0.0, 0.0, 0.0, 1.0],
                monochrome_center_color: [0.5, 0.5, 0.5, 1.0],
                monochrome_line_color: [1.0, 1.0, 1.0, 1.0],
                label_color: [1.0, 1.0, 1.0, 1.0],
                colorful_tick_color: [0.698, 0.0, 0.0, 1.0],
                average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
                long_average_tick_color: [0.0, 0.0, 1.0, 1.0],
                text_early_color: [0.024, 0.416, 0.957, 1.0],
                text_late_color: [1.0, 0.353, 0.306, 1.0],
                text_scaled_early_color: [0.0, 0.318, 0.859, 1.0],
                text_scaled_late_color: [1.0, 0.086, 0.02, 1.0],
                palette: ErrorBarPalette {
                    fantastic_blue: [0.129, 0.8, 0.91, 1.0],
                    fa_plus_white: [1.0, 1.0, 1.0, 1.0],
                    excellent: [0.886, 0.612, 0.094, 1.0],
                    great: [0.4, 0.788, 0.333, 1.0],
                    decent: [0.706, 0.361, 1.0, 1.0],
                    way_off: [0.788, 0.522, 0.369, 1.0],
                },
                label_font: "game",
                offset_indicator_font: "wendy",
                text_font: "wendy",
                early_label: "Early",
                late_label: "Late",
                front_layers: ErrorBarLayers {
                    background: 180,
                    band: 181,
                    line: 182,
                    tick: 183,
                    text: 184,
                },
                back_layers: ErrorBarLayers {
                    background: 86,
                    band: 87,
                    line: 88,
                    tick: 89,
                    text: 90,
                },
                average_z: 88,
            },
        }
    }

    fn request() -> FieldLayoutRequest {
        FieldLayoutRequest {
            style: style(),
            placement: FieldPlacement::P1,
            num_players: 1,
            single_style: true,
            double_style: false,
            center_one_player: false,
            screen_width: 854.0,
            screen_center_x: 427.0,
            screen_center_y: 240.0,
            num_cols: 4,
            field_zoom: 1.0,
            notefield_offset_x: 0.0,
            notefield_offset_y: 0.0,
            receptor_y_override: None,
            center_receptors_y: false,
            centered_scroll: 0.0,
            column_reverse_percent: [0.0; MAX_COLS],
            column_dirs: [1.0; MAX_COLS],
            song_lua_column_y_offsets: [0.0; MAX_COLS],
            judgment_offset_x: 0.0,
            combo_offset_x: 0.0,
            error_bar_offset_x: 0.0,
            hud_offsets: HudLayoutOffsets::default(),
            hud_params: HudLayoutParams {
                zmod: ZmodLayoutParams {
                    judgment_height: 40.0,
                    has_error_bar: false,
                    has_judgment_texture: false,
                    error_bar_up: false,
                    has_measure_counter: false,
                    measure_counter_up: false,
                    broken_run: false,
                    mini_indicator_position: LayoutMiniIndicatorPosition::Default,
                },
                has_judgment_texture: false,
                error_bar_up: false,
                error_bar_offset: 25.0,
            },
        }
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn places_p1_and_p2_fields_with_signed_offsets() {
        let mut p1_request = request();
        p1_request.num_players = 2;
        p1_request.notefield_offset_x = 20.0;
        let p1 = field_layout(p1_request);
        assert_near(p1.playfield_center_x, 193.5);
        assert_near(p1.layout_center_x, 193.5);
        assert_near(p1.notefield_offset_x, -20.0);

        let mut p2_request = p1_request;
        p2_request.placement = FieldPlacement::P2;
        let p2 = field_layout(p2_request);
        assert_near(p2.playfield_center_x, 660.5);
        assert_near(p2.layout_center_x, 660.5);
        assert_near(p2.notefield_offset_x, 20.0);
    }

    #[test]
    fn centered_single_and_double_layouts_ignore_offset_for_layout_center() {
        let mut single_request = request();
        single_request.center_one_player = true;
        single_request.notefield_offset_x = 20.0;
        let single = field_layout(single_request);
        assert_near(single.playfield_center_x, 407.0);
        assert_near(single.layout_center_x, 427.0);

        let mut non_single_request = single_request;
        non_single_request.single_style = false;
        let non_single = field_layout(non_single_request);
        assert_near(non_single.playfield_center_x, 193.5);
        assert_near(non_single.layout_center_x, 193.5);

        let mut double_request = single_request;
        double_request.single_style = false;
        double_request.center_one_player = false;
        double_request.double_style = true;
        double_request.placement = FieldPlacement::P2;
        let double = field_layout(double_request);
        assert_near(double.playfield_center_x, 447.0);
        assert_near(double.layout_center_x, 427.0);
    }

    #[test]
    fn side_centers_clamp_logical_screen_width() {
        let mut narrow_request = request();
        narrow_request.screen_width = 500.0;
        narrow_request.screen_center_x = 250.0;
        assert_near(field_layout(narrow_request).playfield_center_x, 90.0);

        let mut wide_request = request();
        wide_request.screen_width = 1000.0;
        wide_request.screen_center_x = 500.0;
        wide_request.placement = FieldPlacement::P2;
        assert_near(field_layout(wide_request).playfield_center_x, 713.5);
    }

    #[test]
    fn lays_out_normal_reverse_centered_and_overridden_receptors() {
        let mut request = request();
        request.notefield_offset_y = 10.0;
        request.column_reverse_percent[1] = 0.5;
        request.column_reverse_percent[2] = 1.0;
        let layout = field_layout(request);
        assert_near(layout.receptor_y_normal, 125.0);
        assert_near(layout.receptor_y_reverse, 395.0);
        assert_near(layout.receptor_y_centered, 250.0);
        assert_near(layout.column_receptor_ys[0], 125.0);
        assert_near(layout.column_receptor_ys[1], 260.0);
        assert_near(layout.column_receptor_ys[2], 395.0);

        request.centered_scroll = 2.0;
        let overshot = field_layout(request);
        assert_near(overshot.column_receptor_ys[0], 375.0);
        assert_near(overshot.hud_layout.judgment_y, 470.0);

        request.centered_scroll = 0.0;
        request.center_receptors_y = true;
        let centered = field_layout(request);
        assert_eq!(centered.centered_percent, 1.0);
        assert!(
            centered.column_receptor_ys[..4]
                .iter()
                .all(|y| (*y - 250.0).abs() <= 0.001)
        );

        request.center_receptors_y = false;
        request.receptor_y_override = Some(100.0);
        let overridden = field_layout(request);
        assert_eq!(overridden.centered_percent, 1.0);
        assert!(
            overridden.column_receptor_ys[..4]
                .iter()
                .all(|y| (*y - 110.0).abs() <= 0.001)
        );
    }

    #[test]
    fn applies_song_lua_column_y_offsets_after_field_zoom() {
        let mut request = request();
        request.num_cols = 2;
        request.field_zoom = 1.5;
        request.column_dirs[0] = -1.0;
        request.song_lua_column_y_offsets[0] = 8.0;
        request.song_lua_column_y_offsets[2] = 100.0;
        let layout = field_layout(request);

        assert_near(layout.column_receptor_ys[0], 127.0);
        assert_eq!(layout.column_dirs[0], -1.0);
        assert_eq!(layout.column_dirs[2], 1.0);
        assert_near(layout.column_receptor_ys[2], 115.0);
    }

    #[test]
    fn applies_hud_and_error_bar_offsets_from_the_request() {
        let mut request = request();
        request.judgment_offset_x = 12.0;
        request.combo_offset_x = -8.0;
        request.error_bar_offset_x = 4.0;
        request.hud_offsets = HudLayoutOffsets {
            judgment_extra_y: 7.0,
            combo_extra_y: -9.0,
            error_bar_extra_y: 11.0,
        };
        let layout = field_layout(request);

        assert_near(layout.judgment_x, 225.5);
        assert_near(layout.combo_x, 205.5);
        assert_near(layout.error_bar_x, 217.5);
        assert_near(layout.hud_layout.judgment_y, 217.0);
        assert_near(layout.hud_layout.zmod_layout.combo_y, 261.0);
        assert_near(layout.hud_layout.error_bar_y, 221.0);

        request.column_reverse_percent[0] = 1.0;
        let reverse = field_layout(request);
        assert!(reverse.hud_reverse);
        assert_near(reverse.hud_layout.judgment_y, 277.0);
        assert_near(reverse.hud_layout.zmod_layout.combo_y, 201.0);
    }
}
