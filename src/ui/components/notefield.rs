use crate::act;
use crate::assets;
use crate::core::space::*;
use crate::game::gameplay::active_hold_is_engaged;
use crate::game::gameplay::{
    COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION, ComboMilestoneKind,
    HOLD_JUDGMENT_TOTAL_DURATION, MAX_COLS, MINE_EXPLOSION_DURATION, RECEPTOR_GLOW_DURATION,
    RECEPTOR_Y_OFFSET_FROM_CENTER, RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
};
use crate::game::judgment::{JudgeGrade, TimingWindow};
use crate::game::note::{HoldResult, NoteType};
use crate::game::parsing::noteskin::NUM_QUANTIZATIONS;
use crate::game::{gameplay::State, profile, scroll::ScrollSpeedSetting};
use crate::ui::actors::Actor;
use crate::ui::color;
use cgmath::{Deg, Matrix4, Point3, Vector3};
use rssp::streams::StreamSegment;
use std::array::from_fn;

// --- CONSTANTS ---

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Match Simply Love's on-screen arrow height
const TARGET_EXPLOSION_PIXEL_SIZE: f32 = 125.0; // Simply Love tap explosions top out around 125px tall
const HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER: f32 = -90.0; // Mirrors Simply Love metrics for hold judgments
const HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR: f32 =
    HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER - RECEPTOR_Y_OFFSET_FROM_CENTER;
const TAP_JUDGMENT_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback JudgmentTransformCommand
const COMBO_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback ComboTransformCommand (non-centered)
const LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT: f32 = 140.0; // Each frame in Love 1x2 (doubleres).png is 140px tall
const HOLD_JUDGMENT_FINAL_HEIGHT: f32 = 32.0; // Matches Simply Love's final on-screen size
const HOLD_JUDGMENT_INITIAL_HEIGHT: f32 = HOLD_JUDGMENT_FINAL_HEIGHT * 0.8; // Mirrors 0.4->0.5 zoom ramp in metrics
const HOLD_JUDGMENT_FINAL_ZOOM: f32 =
    HOLD_JUDGMENT_FINAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const HOLD_JUDGMENT_INITIAL_ZOOM: f32 =
    HOLD_JUDGMENT_INITIAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const ERROR_BAR_JUDGMENT_HEIGHT: f32 = 40.0; // SL: judgmentHeight in SL-Layout.lua
const ERROR_BAR_OFFSET_FROM_JUDGMENT: f32 = ERROR_BAR_JUDGMENT_HEIGHT * 0.5 + 5.0; // SL: top/bottom +/-25px

const ERROR_BAR_WIDTH_COLORFUL: f32 = 160.0;
const ERROR_BAR_HEIGHT_COLORFUL: f32 = 10.0;
const ERROR_BAR_WIDTH_MONOCHROME: f32 = 240.0;
const ERROR_BAR_TICK_WIDTH: f32 = 2.0;
const ERROR_BAR_TICK_DUR_COLORFUL: f32 = 0.5;
const ERROR_BAR_TICK_DUR_MONOCHROME: f32 = 0.75;
const ERROR_BAR_SEG_ALPHA_BASE: f32 = 0.3;
const ERROR_BAR_MONO_BG_ALPHA: f32 = 0.5;
const ERROR_BAR_LINE_ALPHA: f32 = 0.3;
const ERROR_BAR_LINES_FADE_START_S: f32 = 2.5;
const ERROR_BAR_LINES_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_HOLD_S: f32 = 2.0;

const ERROR_BAR_COLORFUL_TICK_RGBA: [f32; 4] = color::rgba_hex("#b20000");

// Visual Feedback
const SHOW_COMBO_AT: u32 = 4; // From Simply Love metrics

// Z-order layers for key gameplay visuals (higher draws on top)
const Z_RECEPTOR: i32 = 100;
const Z_HOLD_BODY: i32 = 110;
const Z_HOLD_CAP: i32 = 110;
const Z_HOLD_EXPLOSION: i32 = 120;
const Z_HOLD_GLOW: i32 = 130;
const Z_MINE_EXPLOSION: i32 = 101;
const Z_TAP_NOTE: i32 = 140;
const MINE_CORE_SIZE_RATIO: f32 = 0.45;
const MINE_FILL_LAYERS: usize = 32;
const Z_MEASURE_LINES: i32 = 80;

#[derive(Clone, Copy, Debug)]
pub enum FieldPlacement {
    P1,
    P2,
}

#[derive(Clone, Debug)]
struct MineFillState {
    layers: [[f32; 4]; MINE_FILL_LAYERS],
}

fn mine_fill_state(colors: &[[f32; 4]], beat: f32) -> Option<MineFillState> {
    if colors.is_empty() {
        return None;
    }

    let phase = beat.rem_euclid(1.0);
    let len = colors.len();

    let idx_float = phase * len as f32;
    let idx = (idx_float.floor() as usize) % len;

    let layers = from_fn(|layer| {
        let offset = layer % len;
        let sample_index = (idx + len - offset) % len;
        let mut color = colors[sample_index];
        color[3] = 1.0;
        color
    });

    Some(MineFillState { layers })
}

#[inline(always)]
fn sm_scale(v: f32, in0: f32, in1: f32, out0: f32, out1: f32) -> f32 {
    let denom = in1 - in0;
    if denom.abs() < 1e-6 {
        return out1;
    }
    ((v - in0) / denom).mul_add(out1 - out0, out0)
}

#[inline(always)]
fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline(always)]
const fn timing_window_from_num(n: usize) -> TimingWindow {
    match n {
        0 => TimingWindow::W0,
        1 => TimingWindow::W1,
        2 => TimingWindow::W2,
        3 => TimingWindow::W3,
        4 => TimingWindow::W4,
        _ => TimingWindow::W5,
    }
}

#[inline(always)]
fn error_bar_color_for_window(window: TimingWindow, show_fa_plus_window: bool) -> [f32; 4] {
    match window {
        TimingWindow::W0 => color::JUDGMENT_RGBA[0],
        TimingWindow::W1 => {
            if show_fa_plus_window {
                color::JUDGMENT_FA_PLUS_WHITE_RGBA
            } else {
                color::JUDGMENT_RGBA[0]
            }
        }
        TimingWindow::W2 => color::JUDGMENT_RGBA[1],
        TimingWindow::W3 => color::JUDGMENT_RGBA[2],
        TimingWindow::W4 => color::JUDGMENT_RGBA[3],
        TimingWindow::W5 => color::JUDGMENT_RGBA[4],
    }
}

#[inline(always)]
fn error_bar_tick_alpha(age: f32, dur: f32, multi_tick: bool) -> f32 {
    if !age.is_finite() || age < 0.0 {
        return 0.0;
    }
    if multi_tick {
        if age < 0.03 {
            1.0
        } else if age < dur {
            1.0 - (age - 0.03) / (dur - 0.03).max(0.000_001)
        } else {
            0.0
        }
    } else if age < dur {
        1.0
    } else {
        0.0
    }
}

#[inline(always)]
fn error_bar_flash_alpha(now: f32, started_at: Option<f32>, dur: f32) -> f32 {
    let Some(t0) = started_at else {
        return ERROR_BAR_SEG_ALPHA_BASE;
    };
    let age = now - t0;
    if !age.is_finite() || age < 0.0 || age >= dur {
        return ERROR_BAR_SEG_ALPHA_BASE;
    }
    let t = (age / dur).clamp(0.0, 1.0);
    1.0 - (1.0 - ERROR_BAR_SEG_ALPHA_BASE) * t
}

#[inline(always)]
fn error_bar_trim_max_window_ix(trim: profile::ErrorBarTrim) -> usize {
    match trim {
        profile::ErrorBarTrim::Off => 4,       // W5
        profile::ErrorBarTrim::Great => 2,     // W3
        profile::ErrorBarTrim::Excellent => 1, // W2
    }
}

#[inline(always)]
fn error_bar_boundaries_s(
    windows_s: [f32; 5],
    w0_s: Option<f32>,
    show_fa_plus_window: bool,
    trim: profile::ErrorBarTrim,
) -> ([f32; 6], usize) {
    let mut out = [0.0_f32; 6];
    let mut len: usize = 0;
    let base_end = error_bar_trim_max_window_ix(trim) + 1; // 1..=5
    for wi in 1..=base_end {
        if show_fa_plus_window && wi == 1 {
            if let Some(w0) = w0_s
                && len < out.len()
            {
                out[len] = w0;
                len += 1;
            }
            if len < out.len() {
                out[len] = windows_s[0];
                len += 1;
            }
        } else if len < out.len() {
            out[len] = windows_s[wi - 1];
            len += 1;
        }
    }
    (out, len)
}

#[derive(Clone, Copy, Debug)]
struct ZmodLayoutYs {
    measure_counter_y: Option<f32>,
    subtractive_scoring_y: f32,
}

#[inline(always)]
fn zmod_layout_ys(profile: &crate::game::profile::Profile, judgment_y: f32) -> ZmodLayoutYs {
    let mut top_y = judgment_y - ERROR_BAR_JUDGMENT_HEIGHT * 0.5;
    let mut bottom_y = judgment_y + ERROR_BAR_JUDGMENT_HEIGHT * 0.5;

    // Zmod SL-Layout.lua: hasErrorBar checks multiple flags; deadsync models this as one enum.
    if profile.error_bar != crate::game::profile::ErrorBarStyle::None {
        if matches!(
            profile.judgment_graphic,
            crate::game::profile::JudgmentGraphic::None
        ) {
            // Error bar replaces judgment; no top/bottom adjustment.
        } else if profile.error_bar_up {
            top_y -= 15.0;
        } else {
            bottom_y += 15.0;
        }
    }

    let mut measure_counter_y = None;
    let has_measure_counter = profile.measure_counter != crate::game::profile::MeasureCounter::None;
    if has_measure_counter {
        if profile.measure_counter_up {
            let mut y = top_y - 8.0;
            top_y -= 20.0;
            if profile.broken_run {
                y -= 16.0;
            }
            measure_counter_y = Some(y);
        } else {
            measure_counter_y = Some(bottom_y + 8.0);
            bottom_y += 21.0;
        }
    }

    let subtractive_scoring_y = if has_measure_counter && profile.measure_counter_up {
        bottom_y + 8.0
    } else {
        top_y - 8.0
    };

    ZmodLayoutYs {
        measure_counter_y,
        subtractive_scoring_y,
    }
}

fn zmod_measure_counter_text(
    curr_beat_floor: f32,
    curr_measure: f32,
    segs: &[StreamSegment],
    stream_index_unshifted: usize,
    is_lookahead: bool,
    lookahead: u8,
    multiplier: f32,
) -> String {
    if segs.is_empty() {
        return String::new();
    }

    let mut stream_index = stream_index_unshifted as isize;
    let beat_div4 = curr_beat_floor / 4.0;

    if curr_measure < 0.0 {
        if !is_lookahead {
            let first = segs[0];
            if !first.is_break {
                let v = ((beat_div4 * -1.0) + (1.0 * multiplier)).floor() as i32;
                return format!("({v})");
            }
            let len = (first.end - first.start) as i32;
            let v_unscaled = (beat_div4 * -1.0).floor() as i32 + 1 + len;
            let v = ((v_unscaled as f32) * multiplier).floor() as i32;
            return format!("({v})");
        }
        if !segs[0].is_break {
            stream_index -= 1;
        }
    }

    let Some(seg) = stream_index
        .try_into()
        .ok()
        .and_then(|i: usize| segs.get(i).copied())
    else {
        return String::new();
    };

    let segment_start = seg.start as f32;
    let segment_end = seg.end as f32;
    let seg_len = ((segment_end - segment_start) * multiplier).floor() as i32;
    let curr_count = (((beat_div4 - segment_start) * multiplier).floor() as i32) + 1;

    if seg.is_break {
        if lookahead == 0 {
            return String::new();
        }
        if is_lookahead {
            format!("({seg_len})")
        } else {
            let remaining = seg_len - curr_count + 1;
            format!("({remaining})")
        }
    } else if !is_lookahead && curr_count != 0 {
        format!("{curr_count}/{seg_len}")
    } else {
        seg_len.to_string()
    }
}

fn zmod_broken_run_end(segs: &[StreamSegment], start_index: usize) -> (usize, bool) {
    let Some(first) = segs.get(start_index).copied() else {
        return (0, false);
    };
    if first.is_break {
        return (first.end, false);
    }

    let last_index = segs.len().saturating_sub(1);
    let mut end = first.end;
    let mut broken = false;

    for i in (start_index + 1)..segs.len() {
        let seg = segs[i];
        let len = seg.end - seg.start;
        if seg.is_break {
            if len < 4 && i != last_index {
                end += len;
                broken = true;
                continue;
            }
            break;
        }

        broken = true;
        end += len;
        if !segs[i - 1].is_break {
            end += 1;
        }
    }

    (end, broken)
}

fn zmod_broken_run_segment(segs: &[StreamSegment], curr_measure: f32) -> Option<(usize, usize, bool)> {
    for (i, seg) in segs.iter().copied().enumerate() {
        if seg.is_break {
            if curr_measure < seg.end as f32 {
                return Some((i, seg.end, false));
            }
            continue;
        }
        let (end, broken) = zmod_broken_run_end(segs, i);
        if curr_measure < end as f32 {
            return Some((i, end, broken));
        }
    }
    None
}

fn zmod_run_timer_index(segs: &[StreamSegment], curr_measure: f32) -> Option<usize> {
    for (i, seg) in segs.iter().copied().enumerate() {
        let len = (seg.end - seg.start) as f32;
        let curr_count = (curr_measure - seg.start as f32).ceil();
        if curr_count <= len {
            return Some(i);
        }
    }
    None
}

fn zmod_run_timer_fmt(seconds: i32, minute_threshold: i32) -> String {
    let seconds = seconds.max(0);
    if seconds < 10 {
        format!("0.0{seconds}")
    } else if seconds > minute_threshold {
        let minutes = seconds / 60;
        let secs = seconds % 60;
        format!("{minutes}.{secs:02}")
    } else {
        format!("0.{seconds}")
    }
}

#[inline(always)]
fn rage_frustum(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32) -> Matrix4<f32> {
    let a = (r + l) / (r - l);
    let bb = (t + b) / (t - b);
    let c = -(zf + zn) / (zf - zn);
    let d = -(2.0 * zf * zn) / (zf - zn);
    // Match ITGmania's RageDisplay::GetFrustumMatrix (OpenGL-style frustum matrix).
    //
    // Note: cgmath::Matrix4::new takes elements in column-major order.
    Matrix4::new(
        // column 0
        2.0 * zn / (r - l),
        0.0,
        0.0,
        0.0,
        // column 1
        0.0,
        2.0 * zn / (t - b),
        0.0,
        0.0,
        // column 2
        a,
        bb,
        c,
        -1.0,
        // column 3
        0.0,
        0.0,
        d,
        0.0,
    )
}

fn notefield_view_proj(
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
    center_y: f32,
    tilt: f32,
    skew: f32,
    reverse: bool,
) -> Option<Matrix4<f32>> {
    if !screen_w.is_finite() || !screen_h.is_finite() || screen_w <= 0.0 || screen_h <= 0.0 {
        return None;
    }

    let half_w = 0.5 * screen_w;
    let half_h = 0.5 * screen_h;

    // ITGmania: Player::PushPlayerMatrix -> LoadMenuPerspective(45, w, h, vanish_x, center_y)
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

    // Match RageDisplay::LoadMenuPerspective exactly (ITGmania).
    let mut vp_x = sm_scale(vanish_x, 0.0, screen_w, screen_w, 0.0);
    let mut vp_y = sm_scale(vanish_y, 0.0, screen_h, screen_h, 0.0);
    vp_x -= half_w;
    vp_y -= half_h;
    let l = (vp_x - half_w) / dist;
    let r = (vp_x + half_w) / dist;
    let b = (vp_y + half_h) / dist;
    let t = (vp_y - half_h) / dist;
    let proj = rage_frustum(l, r, b, t, near, far);

    let eye = Point3::new(-vp_x + half_w, -vp_y + half_h, dist);
    let at = Point3::new(-vp_x + half_w, -vp_y + half_h, 0.0);
    let view = Matrix4::look_at_rh(eye, at, Vector3::unit_y());

    // ITGmania: PlayerNoteFieldPositioner applies tilt/zoom/y_offset on the NoteField actor.
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
    // Screen y-down to world y-up.
    let y_offset_world = -y_offset_screen;

    let pivot_x = playfield_center_x - half_w;
    let pivot_y = half_h - center_y;
    // Convert our world coords (centered, y-up) back into the SM-style screen
    // coords (top-left, y-down) expected by the menu perspective camera.
    let world_to_screen = Matrix4::new(
        1.0, 0.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        half_w, half_h, 0.0, 1.0,
    );
    let field = Matrix4::from_translation(Vector3::new(0.0, y_offset_world, 0.0))
        * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
        * Matrix4::from_angle_x(Deg(tilt_deg))
        * Matrix4::from_nonuniform_scale(tilt_scale, tilt_scale, 1.0)
        * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0));

    Some((proj * view) * world_to_screen * field)
}

pub fn build(
    state: &State,
    profile: &profile::Profile,
    placement: FieldPlacement,
) -> (Vec<Actor>, f32) {
    let mut actors = Vec::new();
    let mut hud_actors: Vec<Actor> = Vec::new();
    let hold_judgment_texture: Option<&str> = match profile.hold_judgment_graphic {
        profile::HoldJudgmentGraphic::Love => Some("hold_judgements/Love 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::Mute => Some("hold_judgements/mute 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::ITG2 => Some("hold_judgements/ITG2 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::None => None,
    };

    // --- Playfield Positioning (1:1 with Simply Love) ---
    // In P2-only single-player, we still have a single player runtime (index 0),
    // but need to place the notefield on the P2 side of the screen.
    let player_idx = if state.num_players == 1 {
        0
    } else {
        match placement {
            FieldPlacement::P1 => 0,
            FieldPlacement::P2 => 1,
        }
    };
    if player_idx >= state.num_players {
        return (Vec::new(), screen_center_x());
    }
    // Use the cached field_zoom from gameplay state so visual layout and
    // scroll math share the exact same scaling as gameplay.
    let field_zoom = state.field_zoom[player_idx];
    let scroll_speed = state.scroll_speed[player_idx];
    let col_start = player_idx * state.cols_per_player;
    let col_end = (col_start + state.cols_per_player)
        .min(state.num_cols)
        .min(MAX_COLS);
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return (Vec::new(), screen_center_x());
    }
    let p = &state.players[player_idx];

    // NoteFieldOffsetX is stored as a non-negative magnitude; for a single P1-style field,
    // apply the player-side sign flip used by Simply Love (P1=-, P2=+).
    let offset_sign = match placement {
        FieldPlacement::P1 => -1.0,
        FieldPlacement::P2 => 1.0,
    };
    let notefield_offset_x = offset_sign * (profile.note_field_offset_x.clamp(0, 50) as f32);
    let notefield_offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let base_playfield_center_x = if state.num_players == 2 {
        match placement {
            FieldPlacement::P1 => screen_center_x() - (clamped_width * 0.25),
            FieldPlacement::P2 => screen_center_x() + (clamped_width * 0.25),
        }
    } else if state.cols_per_player > 4 {
        screen_center_x()
    } else {
        match placement {
            FieldPlacement::P1 => screen_center_x() - (clamped_width * 0.25),
            FieldPlacement::P2 => screen_center_x() + (clamped_width * 0.25),
        }
    };
    let playfield_center_x = base_playfield_center_x + notefield_offset_x;
    let receptor_y_normal = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER + notefield_offset_y;
    let receptor_y_reverse =
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE + notefield_offset_y;

    let is_centered = profile
        .scroll_option
        .contains(profile::ScrollOption::Centered);
    let receptor_y_centered = screen_center_y() + notefield_offset_y;
    let column_dirs: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return 1.0;
        }
        state.column_scroll_dirs[col_start + i]
    });
    let column_receptor_ys: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return receptor_y_normal;
        }
        if is_centered {
            receptor_y_centered
        } else if column_dirs[i] >= 0.0 {
            receptor_y_normal
        } else {
            receptor_y_reverse
        }
    });

    let elapsed_screen = state.total_elapsed_in_screen;
    let reverse_scroll = state.reverse_scroll[player_idx];
    let judgment_y = if is_centered {
        receptor_y_centered + 95.0
    } else if reverse_scroll {
        screen_center_y() + TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y
    } else {
        screen_center_y() - TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y
    };

    if let Some(ns) = &state.noteskin[player_idx] {
        let timing = &state.timing_players[player_idx];
        let target_arrow_px = TARGET_ARROW_PIXEL_SIZE * field_zoom;
        let target_explosion_px = TARGET_EXPLOSION_PIXEL_SIZE * field_zoom;
        let scale_sprite = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || target_arrow_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_arrow_px / height;
                [width * scale, target_arrow_px]
            }
        };
        let scale_explosion = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || target_explosion_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_explosion_px / height;
                [width * scale, target_explosion_px]
            }
        };
        let current_time = state.current_music_time_visible[player_idx];
        let current_beat = state.current_beat_visible[player_idx];
        // Precompute per-frame values used for converting beat/time to Y positions
        let (rate, cmod_pps_opt, curr_disp_beat, beatmod_multiplier) = match scroll_speed {
            ScrollSpeedSetting::CMod(c_bpm) => {
                let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                    state.music_rate
                } else {
                    1.0
                };
                (rate, Some(pps), 0.0, 0.0)
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let curr_disp = timing.get_displayed_beat(state.current_beat_visible[player_idx]);
                let speed_multiplier = timing
                    .get_speed_multiplier(state.current_beat_visible[player_idx], current_time);
                let player_multiplier =
                    scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
                let final_multiplier = player_multiplier * speed_multiplier;
                (1.0, None, curr_disp, final_multiplier)
            }
        };
        // For dynamic values (e.g., last_held_beat while letting go), fall back to timing for that beat.
        // Direction and receptor row are per-lane: upwards lanes anchor to the normal receptor row,
        // downwards lanes anchor to the reverse row.
        let compute_lane_y_dynamic = |beat: f32, receptor_y_lane: f32, dir: f32| -> f32 {
            let dir = if dir >= 0.0 { 1.0 } else { -1.0 };
            match scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    let note_time_chart = timing.get_time_for_beat(beat);
                    let time_diff_real = (note_time_chart - current_time) / rate;
                    receptor_y_lane + dir * time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp_beat = timing.get_displayed_beat(beat);
                    let beat_diff_disp = note_disp_beat - curr_disp_beat;
                    receptor_y_lane
                        + dir
                            * (beat_diff_disp
                                * ScrollSpeedSetting::ARROW_SPACING
                                * field_zoom
                                * beatmod_multiplier)
                }
            }
        };

        // Measure Lines (Zmod parity: NoteField:SetBeatBarsAlpha)
        if !matches!(profile.measure_lines, crate::game::profile::MeasureLines::Off) {
            let (alpha_measure, alpha_quarter, alpha_eighth) = match profile.measure_lines {
                crate::game::profile::MeasureLines::Off => (0.0, 0.0, 0.0),
                crate::game::profile::MeasureLines::Measure => (0.75, 0.0, 0.0),
                crate::game::profile::MeasureLines::Quarter => (0.75, 0.5, 0.0),
                crate::game::profile::MeasureLines::Eighth => (0.75, 0.5, 0.125),
            };

            let mut pos_min_x: f32 = f32::INFINITY;
            let mut pos_max_x: f32 = f32::NEG_INFINITY;
            let mut pos_receptor_y: f32 = 0.0;
            let mut pos_any = false;

            let mut neg_min_x: f32 = f32::INFINITY;
            let mut neg_max_x: f32 = f32::NEG_INFINITY;
            let mut neg_receptor_y: f32 = 0.0;
            let mut neg_any = false;

            for i in 0..num_cols {
                let x = ns.column_xs[i] as f32;
                if column_dirs[i] >= 0.0 {
                    if !pos_any {
                        pos_any = true;
                        pos_receptor_y = column_receptor_ys[i];
                        pos_min_x = x;
                        pos_max_x = x;
                    } else {
                        pos_min_x = pos_min_x.min(x);
                        pos_max_x = pos_max_x.max(x);
                    }
                } else if !neg_any {
                    neg_any = true;
                    neg_receptor_y = column_receptor_ys[i];
                    neg_min_x = x;
                    neg_max_x = x;
                } else {
                    neg_min_x = neg_min_x.min(x);
                    neg_max_x = neg_max_x.max(x);
                }
            }

            let beat_units_start = (current_beat * 2.0).floor() as i64;
            let thickness = (2.0 * field_zoom).max(1.0);
            let y_min = -400.0;
            let y_max = screen_height() + 400.0;

            let mut draw_group = |min_x: f32, max_x: f32, receptor_y: f32, dir: f32| {
                let center_x_offset = 0.5 * (min_x + max_x) * field_zoom;
                let w = ((max_x - min_x) + ScrollSpeedSetting::ARROW_SPACING) * field_zoom;
                if !w.is_finite() || w <= 0.0 {
                    return;
                }

                let x_center = playfield_center_x + center_x_offset;

                // Walk backward from current beat.
                let mut u = beat_units_start;
                let mut iters = 0;
                while iters < 2000 {
                    let alpha = if u.rem_euclid(8) == 0 {
                        alpha_measure
                    } else if u.rem_euclid(2) == 0 {
                        alpha_quarter
                    } else {
                        alpha_eighth
                    };

                    let beat = (u as f32) * 0.5;
                    let y = compute_lane_y_dynamic(beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y < y_min) || (dir < 0.0 && y > y_max) {
                        break;
                    }
                    if alpha > 0.0 && y >= y_min && y <= y_max {
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x_center, y):
                            zoomto(w, thickness):
                            diffuse(1.0, 1.0, 1.0, alpha):
                            z(Z_MEASURE_LINES)
                        ));
                    }
                    u -= 1;
                    iters += 1;
                }

                // Walk forward from next half-beat to avoid duplicating the start line.
                let mut u = beat_units_start + 1;
                let mut iters = 0;
                while iters < 2000 {
                    let alpha = if u.rem_euclid(8) == 0 {
                        alpha_measure
                    } else if u.rem_euclid(2) == 0 {
                        alpha_quarter
                    } else {
                        alpha_eighth
                    };

                    let beat = (u as f32) * 0.5;
                    let y = compute_lane_y_dynamic(beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y > y_max) || (dir < 0.0 && y < y_min) {
                        break;
                    }
                    if alpha > 0.0 && y >= y_min && y <= y_max {
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x_center, y):
                            zoomto(w, thickness):
                            diffuse(1.0, 1.0, 1.0, alpha):
                            z(Z_MEASURE_LINES)
                        ));
                    }
                    u += 1;
                    iters += 1;
                }
            };

            if pos_any {
                draw_group(pos_min_x, pos_max_x, pos_receptor_y, 1.0);
            }
            if neg_any {
                draw_group(neg_min_x, neg_max_x, neg_receptor_y, -1.0);
            }
        }

        let mine_explosion_size = {
            let base = assets::texture_dims("hit_mine_explosion.png")
                .map(|meta| [meta.w.max(1) as f32, meta.h.max(1) as f32])
                .unwrap_or([TARGET_EXPLOSION_PIXEL_SIZE, TARGET_EXPLOSION_PIXEL_SIZE]);
            if base[1] <= 0.0 {
                base
            } else {
                let scale = TARGET_EXPLOSION_PIXEL_SIZE / base[1];
                [base[0] * scale, TARGET_EXPLOSION_PIXEL_SIZE]
            }
        };
        // Receptors + glow
        for i in 0..num_cols {
            let col = col_start + i;
            let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
            let receptor_y_lane = column_receptor_ys[i];
            if !profile.hide_targets {
                let bop_timer = state.receptor_bop_timers[col];
                let bop_zoom = if bop_timer > 0.0 {
                    let t = (0.11 - bop_timer) / 0.11;
                    0.75 + (1.0 - 0.75) * t
                } else {
                    1.0
                };
                let receptor_slot = &ns.receptor_off[i];
                let receptor_frame =
                    receptor_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                let receptor_uv = receptor_slot.uv_for_frame(receptor_frame);
                let receptor_size = scale_sprite(receptor_slot.size());
                let receptor_color = ns.receptor_pulse.color_for_beat(current_beat);
                actors.push(act!(sprite(receptor_slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset, receptor_y_lane):
                    zoomto(receptor_size[0], receptor_size[1]):
                    zoom(bop_zoom):
                    diffuse(
                        receptor_color[0],
                        receptor_color[1],
                        receptor_color[2],
                        receptor_color[3]
                    ):
                    rotationz(-receptor_slot.def.rotation_deg as f32):
                    customtexturerect(
                        receptor_uv[0],
                        receptor_uv[1],
                        receptor_uv[2],
                        receptor_uv[3]
                    ):
                    z(Z_RECEPTOR)
                ));
            }
            if let Some(hold_slot) = state.active_holds[col]
                .as_ref()
                .filter(|active| active_hold_is_engaged(active))
                .and_then(|active| {
                    let note_type = &state.notes[active.note_index].note_type;
                    let visuals = if matches!(note_type, NoteType::Roll) {
                        &ns.roll
                    } else {
                        &ns.hold
                    };
                    visuals.explosion.as_ref().or(ns.hold.explosion.as_ref())
                })
            {
                let hold_uv = hold_slot.uv_for_frame(0);
                let hold_size = scale_explosion(hold_slot.size());
                let receptor_rotation = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg as f32)
                    .unwrap_or(0.0);
                let base_rotation = hold_slot.def.rotation_deg as f32;
                let final_rotation = base_rotation + receptor_rotation;
                actors.push(act!(sprite(hold_slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset, receptor_y_lane):
                    zoomto(hold_size[0], hold_size[1]):
                    rotationz(-final_rotation):
                    customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                    blend(normal):
                    z(Z_HOLD_EXPLOSION)
                ));
            }
            if !profile.hide_targets {
                let glow_timer = state.receptor_glow_timers[col];
                if glow_timer > 0.0
                    && let Some(glow_slot) = ns.receptor_glow.get(i).and_then(|slot| slot.as_ref())
                {
                    let glow_frame =
                        glow_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                    let glow_uv = glow_slot.uv_for_frame(glow_frame);
                    let glow_size = glow_slot.size();
                    let alpha = (glow_timer / RECEPTOR_GLOW_DURATION).powf(0.75);
                    actors.push(act!(sprite(glow_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset, receptor_y_lane):
                        zoomto(glow_size[0] as f32, glow_size[1] as f32):
                        rotationz(-glow_slot.def.rotation_deg as f32):
                        customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                        diffuse(1.0, 1.0, 1.0, alpha):
                        blend(add):
                        z(Z_HOLD_GLOW)
                    ));
                }
            }
        }
        // Tap explosions
        if !profile.hide_combo_explosions {
            for i in 0..num_cols {
                let col = col_start + i;
                if let Some(active) = state.tap_explosions[col].as_ref()
                    && let Some(explosion) = ns.tap_explosions.get(&active.window)
                {
                    let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
                    let receptor_y_lane = column_receptor_ys[i];
                    let anim_time = active.elapsed;
                    let slot = &explosion.slot;
                    let beat_for_anim = if slot.source.is_beat_based() {
                        (state.current_beat - active.start_beat).max(0.0)
                    } else {
                        state.current_beat
                    };
                    let frame = slot.frame_index(anim_time, beat_for_anim);
                    let uv = slot.uv_for_frame(frame);
                    let size = scale_explosion(slot.size());
                    let visual = explosion.animation.state_at(active.elapsed);
                    let rotation_deg = ns
                        .receptor_off
                        .get(i)
                        .map(|slot| slot.def.rotation_deg)
                        .unwrap_or(0);
                    actors.push(act!(sprite(slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset, receptor_y_lane):
                        zoomto(size[0], size[1]):
                        zoom(visual.zoom):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        diffuse(
                            visual.diffuse[0],
                            visual.diffuse[1],
                            visual.diffuse[2],
                            visual.diffuse[3]
                        ):
                        rotationz(-(rotation_deg as f32)):
                        blend(normal):
                        z(101)
                    ));
                    let glow = visual.glow;
                    let glow_strength =
                        glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset, receptor_y_lane):
                            zoomto(size[0], size[1]):
                            zoom(visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            rotationz(-(rotation_deg as f32)):
                            blend(add):
                            z(101)
                        ));
                    }
                }
            }
        }
        // Mine explosions
        for i in 0..num_cols {
            let col = col_start + i;
            if let Some(active) = state.mine_explosions[col].as_ref() {
                let duration = MINE_EXPLOSION_DURATION.max(f32::EPSILON);
                let progress = (active.elapsed / duration).clamp(0.0, 1.0);
                let alpha = if progress < 0.5 {
                    1.0
                } else {
                    1.0 - ((progress - 0.5) / 0.5)
                }
                .clamp(0.0, 1.0);
                if alpha <= f32::EPSILON {
                    continue;
                }
                let rotation_progress = 180.0 * progress;
                let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
                let receptor_y_lane = column_receptor_ys[i];
                let base_rotation = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg as f32)
                    .unwrap_or(0.0);
                let final_rotation = base_rotation + rotation_progress;
                actors.push(act!(sprite("hit_mine_explosion.png"):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset, receptor_y_lane):
                    zoomto(mine_explosion_size[0], mine_explosion_size[1]):
                    rotationz(-final_rotation):
                    diffuse(1.0, 1.0, 1.0, alpha):
                    blend(add):
                    z(Z_MINE_EXPLOSION)
                ));
            }
        }
        // Only consider notes that are currently in or near the lookahead window.
        let notes_len = state.notes.len();
        let (note_start, note_end) = state.note_ranges[player_idx];
        let min_visible_index = state.arrows[col_start..col_end]
            .iter()
            .filter_map(|v| v.first())
            .map(|a| a.note_index)
            .min()
            .unwrap_or(note_start);
        let max_visible_index = state.note_spawn_cursor[player_idx]
            .clamp(note_start, note_end)
            .min(notes_len);
        let extra_hold_indices = state
            .active_holds
            .iter()
            .filter_map(|a| a.as_ref().map(|h| h.note_index))
            .chain(state.decaying_hold_indices.iter().copied())
            .filter(|&idx| {
                idx >= note_start
                    && idx < note_end
                    && (idx < min_visible_index || idx >= max_visible_index)
            });

        // Render holds in the visible window, plus any active/decaying holds outside it.
        // This avoids per-frame allocations and hashing for deduping.
        for note_index in (min_visible_index..max_visible_index).chain(extra_hold_indices) {
            let note = &state.notes[note_index];
            if note.column < col_start || note.column >= col_end {
                continue;
            }
            let local_col = note.column - col_start;
            if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let Some(hold) = &note.hold else {
                continue;
            };
            if matches!(hold.result, Some(HoldResult::Held)) {
                continue;
            }

            // Prepare static/dynamic Y positions for the hold body
            // Head Y: dynamic if actively held or let go, otherwise static cache
            let mut head_beat = note.beat;
            let is_head_dynamic =
                hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo);

            if is_head_dynamic {
                head_beat = hold.last_held_beat.clamp(note.beat, hold.end_beat);
            }

            let col_dir = column_dirs[local_col];
            let dir = col_dir;
            let lane_receptor_y = column_receptor_ys[local_col];

            // Compute Y positions: O(1) via cache for static parts, dynamic for moving head
            let head_y = if is_head_dynamic {
                compute_lane_y_dynamic(head_beat, lane_receptor_y, dir)
            } else {
                match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = state.note_time_cache[note_index];
                        let time_diff_real = (note_time_chart - current_time) / rate;
                        lane_receptor_y + dir * time_diff_real * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = state.note_display_beat_cache[note_index];
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        lane_receptor_y
                            + dir
                                * (beat_diff_disp
                                    * ScrollSpeedSetting::ARROW_SPACING
                                    * field_zoom
                                    * beatmod_multiplier)
                    }
                }
            };

            let tail_y = match scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    // Use cached end time for O(1) lookup
                    let note_end_time_chart = state.hold_end_time_cache[note_index].unwrap_or(0.0);
                    let time_diff_real = (note_end_time_chart - current_time) / rate;
                    lane_receptor_y + dir * time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    // Use cached end display beat for O(1) lookup
                    let note_end_disp_beat =
                        state.hold_end_display_beat_cache[note_index].unwrap_or(0.0);
                    let beat_diff_disp = note_end_disp_beat - curr_disp_beat;
                    lane_receptor_y
                        + dir
                            * (beat_diff_disp
                                * ScrollSpeedSetting::ARROW_SPACING
                                * field_zoom
                                * beatmod_multiplier)
                }
            };

            let head_is_top = head_y <= tail_y;
            let mut top = head_y.min(tail_y);
            let mut bottom = head_y.max(tail_y);
            if bottom < -200.0 || top > screen_height() + 200.0 {
                continue;
            }
            top = top.max(-400.0);
            bottom = bottom.min(screen_height() + 400.0);
            if bottom <= top {
                continue;
            }
            let col_x_offset = ns.column_xs[local_col] as f32 * field_zoom;

            let active_state = state.active_holds[note.column]
                .as_ref()
                .filter(|h| h.note_index == note_index);
            let engaged = active_state.map(active_hold_is_engaged).unwrap_or(false);
            let use_active = active_state
                .map(|h| h.is_pressed && !h.let_go)
                .unwrap_or(false);
            let let_go_gray = ns.hold_let_go_gray_percent.clamp(0.0, 1.0);
            let hold_life = hold.life.clamp(0.0, 1.0);
            let hold_color_scale = let_go_gray + (1.0 - let_go_gray) * hold_life;
            let hold_diffuse = [hold_color_scale, hold_color_scale, hold_color_scale, 1.0];
            if engaged {
                if head_is_top {
                    top = top.max(lane_receptor_y);
                } else {
                    bottom = bottom.min(lane_receptor_y);
                }
            }
            if bottom <= top {
                continue;
            }
            let visuals = if matches!(note.note_type, NoteType::Roll) {
                &ns.roll
            } else {
                &ns.hold
            };
            let tail_slot = if use_active {
                visuals
                    .bottomcap_active
                    .as_ref()
                    .or(visuals.bottomcap_inactive.as_ref())
            } else {
                visuals
                    .bottomcap_inactive
                    .as_ref()
                    .or(visuals.bottomcap_active.as_ref())
            };
            // Prepare clipped body extents that respect the tail cap on the side
            // where the tail visually exists. For normal orientation (head above
            // tail), we clip the body against the tail cap at the bottom. For
            // reverse orientation (head below tail), we clip the body against the
            // tail cap at the top.
            let mut body_top = top;
            let mut body_bottom = bottom;
            if let Some(cap_slot) = tail_slot {
                let cap_size = scale_sprite(cap_slot.size());
                let cap_height = cap_size[1];
                if cap_height > f32::EPSILON {
                    // Keep the body from poking through the bottom cap, but allow
                    // a tiny overlap so the seam stays hidden like ITGmania.
                    if head_is_top {
                        // Tail visually at the bottom; trim the body bottom.
                        let cap_top = tail_y - cap_height * 0.5;
                        body_bottom = body_bottom.min(cap_top + 1.0);
                    } else {
                        // Tail visually at the top; trim the body top.
                        let cap_bottom = tail_y + cap_height * 0.5;
                        body_top = body_top.max(cap_bottom - 1.0);
                    }
                }
            }
            // Track the actual drawn body extents to decide whether the tail cap
            // should be rendered (prevents floating caps when no body segments were drawn).
            let mut rendered_body_top: Option<f32> = None;
            let mut rendered_body_bottom: Option<f32> = None;
            if body_bottom > body_top
                && let Some(body_slot) = if use_active {
                    visuals
                        .body_active
                        .as_ref()
                        .or(visuals.body_inactive.as_ref())
                } else {
                    visuals
                        .body_inactive
                        .as_ref()
                        .or(visuals.body_active.as_ref())
                }
            {
                let texture_size = body_slot.size();
                let texture_width = texture_size[0].max(1) as f32;
                let texture_height = texture_size[1].max(1) as f32;
                if texture_width > f32::EPSILON && texture_height > f32::EPSILON {
                    let body_width = TARGET_ARROW_PIXEL_SIZE * field_zoom;
                    let scale = body_width / texture_width;
                    let segment_height = (texture_height * scale).max(f32::EPSILON);
                    let body_uv = body_slot.uv_for_frame(0);
                    let u0 = body_uv[0];
                    let u1 = body_uv[2];
                    let v_top = body_uv[1];
                    let v_bottom = body_uv[3];
                    let v_range = v_bottom - v_top;
                    let natural_top = if head_is_top { head_y } else { tail_y };
                    let natural_bottom = if head_is_top { tail_y } else { head_y };
                    let hold_length = (natural_bottom - natural_top).abs();
                    const SEGMENT_PHASE_EPS: f32 = 1e-4;
                    let max_segments = 2048;
                    let lane_reverse = col_dir < 0.0;
                    let receptor = lane_receptor_y;

                    // Unified segmentation path for both normal and reverse scroll.
                    // For reverse scroll, we work in "forward space" by mirroring coordinates,
                    // run the same segmentation logic, then mirror back to screen space.

                    // Transform to "forward space" if reverse scroll (mirror around receptor)
                    let (eff_head_y, eff_tail_y, eff_body_top, eff_body_bottom) = if lane_reverse {
                        (
                            2.0 * receptor - head_y,
                            2.0 * receptor - tail_y,
                            2.0 * receptor - body_bottom,
                            2.0 * receptor - body_top,
                        )
                    } else {
                        (head_y, tail_y, body_top, body_bottom)
                    };

                    let eff_head_is_top = eff_head_y <= eff_tail_y;
                    let eff_natural_top = if eff_head_is_top {
                        eff_head_y
                    } else {
                        eff_tail_y
                    };
                    let eff_natural_bottom = if eff_head_is_top {
                        eff_tail_y
                    } else {
                        eff_head_y
                    };

                    // Skip if hold has no effective length
                    if hold_length > f32::EPSILON {
                        // Calculate visible distances in forward space
                        let visible_top_distance = if eff_head_is_top {
                            (eff_body_top - eff_natural_top).clamp(0.0, hold_length)
                        } else {
                            (eff_natural_bottom - eff_body_top).clamp(0.0, hold_length)
                        };
                        let visible_bottom_distance = if eff_head_is_top {
                            (eff_body_bottom - eff_natural_top).clamp(0.0, hold_length)
                        } else {
                            (eff_natural_bottom - eff_body_bottom).clamp(0.0, hold_length)
                        };

                        // Phase offset: shifts fractional remainder to first segment so the
                        // final segment aligns with the tail cap. Only applies when head is on top.
                        let phase_offset = if eff_head_is_top {
                            let total_phase = hold_length / segment_height;
                            if total_phase >= 1.0 + SEGMENT_PHASE_EPS {
                                let fractional = total_phase.fract();
                                if fractional > SEGMENT_PHASE_EPS
                                    && (1.0 - fractional) > SEGMENT_PHASE_EPS
                                {
                                    1.0 - fractional
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };

                        let mut phase = visible_top_distance / segment_height + phase_offset;
                        let phase_end_adjusted =
                            visible_bottom_distance / segment_height + phase_offset;
                        let mut emitted = 0;

                        while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                            && emitted < max_segments
                        {
                            let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                            if next_phase - phase < SEGMENT_PHASE_EPS {
                                next_phase = phase_end_adjusted;
                            }
                            if next_phase - phase < SEGMENT_PHASE_EPS {
                                break;
                            }

                            let distance_start = (phase - phase_offset) * segment_height;
                            let distance_end = (next_phase - phase_offset) * segment_height;
                            let y_start = eff_natural_top + distance_start;
                            let y_end = eff_natural_top + distance_end;
                            let segment_top_eff = y_start.max(eff_body_top);
                            let segment_bottom_eff = y_end.min(eff_body_bottom);

                            if segment_bottom_eff - segment_top_eff <= f32::EPSILON {
                                phase = next_phase;
                                continue;
                            }

                            // UV calculations
                            let base_floor = phase.floor();
                            let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                            let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                            let mut v0 = v_top + v_range * start_fraction;
                            let mut v1 = v_top + v_range * end_fraction;

                            let segment_size_eff = segment_bottom_eff - segment_top_eff;
                            let portion = (segment_size_eff / segment_height).clamp(0.0, 1.0);

                            let is_last_segment = (eff_body_bottom - segment_bottom_eff).abs()
                                <= 0.5
                                || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                            if is_last_segment {
                                if v_range >= 0.0 {
                                    v1 = v_bottom;
                                    v0 = v_bottom - v_range.abs() * portion;
                                } else {
                                    v1 = v_bottom;
                                    v0 = v_bottom + v_range.abs() * portion;
                                }
                            }

                            // Transform back to screen space if reverse scroll
                            let (
                                segment_center_screen,
                                segment_size_screen,
                                seg_top_screen,
                                seg_bottom_screen,
                            ) = if lane_reverse {
                                let top_scr = 2.0 * receptor - segment_bottom_eff;
                                let bottom_scr = 2.0 * receptor - segment_top_eff;
                                (
                                    (top_scr + bottom_scr) * 0.5,
                                    bottom_scr - top_scr,
                                    top_scr,
                                    bottom_scr,
                                )
                            } else {
                                (
                                    (segment_top_eff + segment_bottom_eff) * 0.5,
                                    segment_size_eff,
                                    segment_top_eff,
                                    segment_bottom_eff,
                                )
                            };

                            let rotation = if lane_reverse { 180.0 } else { 0.0 };

                            // Track rendered bounds in screen space
                            rendered_body_top = Some(match rendered_body_top {
                                None => seg_top_screen,
                                Some(v) => v.min(seg_top_screen),
                            });
                            rendered_body_bottom = Some(match rendered_body_bottom {
                                None => seg_bottom_screen,
                                Some(v) => v.max(seg_bottom_screen),
                            });

                            actors.push(act!(sprite(body_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(playfield_center_x + col_x_offset, segment_center_screen):
                                zoomto(body_width, segment_size_screen):
                                rotationz(rotation):
                                customtexturerect(u0, v0, u1, v1):
                                diffuse(
                                    hold_diffuse[0],
                                    hold_diffuse[1],
                                    hold_diffuse[2],
                                    hold_diffuse[3]
                                ):
                                z(Z_HOLD_BODY)
                            ));

                            phase = next_phase;
                            emitted += 1;
                        }
                    }
                }
            }
            if let Some(cap_slot) = tail_slot {
                let tail_position = tail_y;
                if tail_position > -400.0 && tail_position < screen_height() + 400.0 {
                    let cap_uv = cap_slot.uv_for_frame(0);
                    let cap_size = scale_sprite(cap_slot.size());
                    let cap_width = cap_size[0];
                    let mut cap_height = cap_size[1];
                    let mut cap_center = tail_position;
                    let u0 = cap_uv[0];
                    let u1 = cap_uv[2];
                    let mut v0 = cap_uv[1];
                    let mut v1 = cap_uv[3];
                    // Only draw the tail cap if the rendered body actually reaches
                    // the cap side. This prevents floating caps when no body segments
                    // were drawn near the tail due to scroll gimmicks.
                    let (rt, rb) = match (rendered_body_top, rendered_body_bottom) {
                        (Some(t), Some(b)) if b > t + 0.5 => (t, b),
                        _ => {
                            continue;
                        }
                    };
                    let cap_adjacent_ok = if head_is_top {
                        // Tail visually below; ensure the drawn body bottom is near the tail.
                        let dist = tail_y - rb;
                        dist >= -2.0 && dist <= cap_height + 2.0
                    } else {
                        // Tail visually above; ensure the drawn body top is near the tail.
                        let dist = rt - tail_y;
                        dist >= -2.0 && dist <= cap_height + 2.0
                    };
                    if !cap_adjacent_ok {
                        continue;
                    }
                    if cap_height > f32::EPSILON {
                        let mut cap_top = cap_center - cap_height * 0.5;
                        let mut cap_bottom = cap_center + cap_height * 0.5;
                        let v_span = v1 - v0;
                        if head_is_top {
                            let head_limit = top;
                            if head_limit > cap_top {
                                let trimmed = (head_limit - cap_top).clamp(0.0, cap_height);
                                if trimmed >= cap_height - f32::EPSILON {
                                    cap_height = 0.0;
                                } else if trimmed > f32::EPSILON {
                                    let fraction = trimmed / cap_height;
                                    v0 += v_span * fraction;
                                    cap_top += trimmed;
                                    cap_center = (cap_top + cap_bottom) * 0.5;
                                    cap_height = cap_bottom - cap_top;
                                }
                            }
                        } else {
                            let head_limit = bottom;
                            if head_limit < cap_bottom {
                                let trimmed = (cap_bottom - head_limit).clamp(0.0, cap_height);
                                if trimmed >= cap_height - f32::EPSILON {
                                    cap_height = 0.0;
                                } else if trimmed > f32::EPSILON {
                                    let fraction = trimmed / cap_height;
                                    v1 -= v_span * fraction;
                                    cap_bottom -= trimmed;
                                    cap_center = (cap_top + cap_bottom) * 0.5;
                                    cap_height = cap_bottom - cap_top;
                                }
                            }
                        }
                    }
                    if cap_height > f32::EPSILON {
                        actors.push(act!(sprite(cap_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset, cap_center):
                            zoomto(cap_width, cap_height):
                            customtexturerect(u0, v0, u1, v1):
                            diffuse(
                                hold_diffuse[0],
                                hold_diffuse[1],
                                hold_diffuse[2],
                                hold_diffuse[3]
                            ):
                            rotationz(if col_dir < 0.0 { 180.0 } else { 0.0 }):
                            z(Z_HOLD_CAP)
                        ));
                    }
                }
            }
            if (hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo))
                && head_y >= lane_receptor_y - state.draw_distance_after_targets
                && head_y <= lane_receptor_y + state.draw_distance_before_targets
            {
                let note_idx = local_col * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                if let Some(note_slot) = ns.notes.get(note_idx) {
                    let frame = note_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                    let uv = note_slot.uv_for_frame(frame);
                    let size = scale_sprite(note_slot.size());
                    actors.push(act!(sprite(note_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset, head_y):
                        zoomto(size[0], size[1]):
                        rotationz(-note_slot.def.rotation_deg as f32):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        diffuse(
                            hold_diffuse[0],
                            hold_diffuse[1],
                            hold_diffuse[2],
                            hold_diffuse[3]
                        ):
                        z(Z_TAP_NOTE)
                    ));
                }
            }
        }
        // Active arrows
        for col_idx in 0..num_cols {
            let col = col_start + col_idx;
            let column_arrows = &state.arrows[col];
            let dir = column_dirs[col_idx];
            let receptor_y_lane = column_receptor_ys[col_idx];
            for arrow in column_arrows {
                // Use cached per-note timing to avoid per-frame timing queries
                let y_pos = match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = state.note_time_cache[arrow.note_index];
                        let time_diff_real = (current_time - note_time_chart) / rate;
                        receptor_y_lane - dir * time_diff_real * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        receptor_y_lane
                            + dir
                                * beat_diff_disp
                                * ScrollSpeedSetting::ARROW_SPACING
                                * field_zoom
                                * beatmod_multiplier
                    }
                };
                let delta = (y_pos - receptor_y_lane) * dir;
                if delta < -state.draw_distance_after_targets
                    || delta > state.draw_distance_before_targets
                {
                    continue;
                }
                let col_x_offset = ns.column_xs[col_idx] as f32 * field_zoom;
                if matches!(arrow.note_type, NoteType::Mine) {
                    let fill_slot = ns.mines.get(col_idx).and_then(|slot| slot.as_ref());
                    let frame_slot = ns.mine_frames.get(col_idx).and_then(|slot| slot.as_ref());
                    if fill_slot.is_none() && frame_slot.is_none() {
                        continue;
                    }
                    let base_rotation = fill_slot
                        .map(|slot| -slot.def.rotation_deg as f32)
                        .or_else(|| frame_slot.map(|slot| -slot.def.rotation_deg as f32))
                        .unwrap_or(0.0);
                    let time = state.total_elapsed_in_screen;
                    let beat = current_beat;
                    let circle_reference = frame_slot
                        .map(|slot| scale_sprite(slot.size()))
                        .or_else(|| fill_slot.map(|slot| scale_sprite(slot.size())))
                        .unwrap_or([
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
                        ]);
                    if let Some(slot) = fill_slot {
                        let fill_gradient = ns
                            .mine_fill_gradients
                            .get(col_idx)
                            .and_then(|colors| colors.as_deref());
                        if let Some(fill_state) =
                            fill_gradient.and_then(|colors| mine_fill_state(colors, current_beat))
                        {
                            let width = circle_reference[0] * MINE_CORE_SIZE_RATIO;
                            let height = circle_reference[1] * MINE_CORE_SIZE_RATIO;
                            for layer_idx in (0..MINE_FILL_LAYERS).rev() {
                                let color = fill_state.layers[layer_idx];
                                let scale = (layer_idx as f32 + 1.0) / MINE_FILL_LAYERS as f32;
                                let layer_width = width * scale;
                                let layer_height = height * scale;
                                if layer_width <= 0.0 || layer_height <= 0.0 {
                                    continue;
                                }
                                actors.push(act!(sprite("circle.png"):
                                    align(0.5, 0.5):
                                    xy(playfield_center_x + col_x_offset, y_pos):
                                    zoomto(layer_width, layer_height):
                                    diffuse(color[0], color[1], color[2], 1.0):
                                    z(Z_TAP_NOTE - 2)
                                ));
                            }
                        } else {
                            let frame = slot.frame_index(time, beat);
                            let uv = slot.uv_for_frame(frame);
                            let size = scale_sprite(slot.size());
                            let width = size[0];
                            let height = size[1];
                            let rotation = base_rotation - time * 45.0;
                            actors.push(act!(sprite(slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(playfield_center_x + col_x_offset, y_pos):
                                zoomto(width, height):
                                rotationz(rotation):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(1.0, 1.0, 1.0, 0.9):
                                z(Z_TAP_NOTE - 1)
                            ));
                        }
                    }
                    if let Some(slot) = frame_slot {
                        let frame = slot.frame_index(time, beat);
                        let uv = slot.uv_for_frame(frame);
                        let size = scale_sprite(slot.size());
                        let rotation = base_rotation + time * 120.0;
                        actors.push(act!(sprite(slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset, y_pos):
                            zoomto(size[0], size[1]):
                            rotationz(rotation):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            z(Z_TAP_NOTE)
                        ));
                    }
                    continue;
                }
                let note = &state.notes[arrow.note_index];
                let note_idx = col_idx * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                if let Some(note_slot) = ns.notes.get(note_idx) {
                    let note_frame =
                        note_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                    let note_uv = note_slot.uv_for_frame(note_frame);
                    let note_size = scale_sprite(note_slot.size());
                    actors.push(act!(sprite(note_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset, y_pos):
                        zoomto(note_size[0], note_size[1]):
                        rotationz(-note_slot.def.rotation_deg as f32):
                        customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                        z(Z_TAP_NOTE)
                    ));
                }
            }
        }
    }
    // Combo Milestone Explosions (100 / 1000 combo)
    if !profile.hide_combo && !profile.hide_combo_explosions && !p.combo_milestones.is_empty() {
        let combo_center_x = playfield_center_x;
        let combo_center_y = if state.reverse_scroll[player_idx] {
            screen_center_y() - COMBO_OFFSET_FROM_CENTER
        } else {
            screen_center_y() + COMBO_OFFSET_FROM_CENTER
        } + notefield_offset_y;
        let player_color = state.player_color;
        let ease_out_quad = |t: f32| -> f32 {
            let t = t.clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(2)
        };
        for milestone in &p.combo_milestones {
            match milestone.kind {
                ComboMilestoneKind::Hundred => {
                    let elapsed = milestone.elapsed;
                    let explosion_duration = 0.5_f32;
                    if elapsed <= explosion_duration {
                        let progress = (elapsed / explosion_duration).clamp(0.0, 1.0);
                        let zoom = 2.0 - progress;
                        let alpha = (0.5 * (1.0 - progress)).max(0.0);
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let rotation = 90.0 * direction * progress;
                            hud_actors.push(act!(sprite("combo_explosion.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(zoom):
                                rotationz(rotation):
                                diffuse(1.0, 1.0, 1.0, alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                    if elapsed <= COMBO_HUNDRED_MILESTONE_DURATION {
                        let progress = (elapsed / COMBO_HUNDRED_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let eased = ease_out_quad(progress);
                        let zoom = 0.25 + (2.0 - 0.25) * eased;
                        let alpha = (0.6 * (1.0 - eased)).max(0.0);
                        let rotation = 10.0 + (0.0 - 10.0) * eased;
                        hud_actors.push(act!(sprite("combo_100milestone_splode.png"):
                            align(0.5, 0.5):
                            xy(combo_center_x, combo_center_y):
                            zoom(zoom):
                            rotationz(rotation):
                            diffuse(player_color[0], player_color[1], player_color[2], alpha):
                            blend(add):
                            z(89)
                        ));
                        let mini_duration = 0.4_f32;
                        if elapsed <= mini_duration {
                            let mini_progress = (elapsed / mini_duration).clamp(0.0, 1.0);
                            let mini_zoom = 0.25 + (1.8 - 0.25) * mini_progress;
                            let mini_alpha = (1.0 - mini_progress).max(0.0);
                            let mini_rotation = 10.0 + (0.0 - 10.0) * mini_progress;
                            hud_actors.push(act!(sprite("combo_100milestone_minisplode.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(mini_zoom):
                                rotationz(mini_rotation):
                                diffuse(player_color[0], player_color[1], player_color[2], mini_alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
                ComboMilestoneKind::Thousand => {
                    let elapsed = milestone.elapsed;
                    if elapsed <= COMBO_THOUSAND_MILESTONE_DURATION {
                        let progress =
                            (elapsed / COMBO_THOUSAND_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let zoom = 0.25 + (3.0 - 0.25) * progress;
                        let alpha = (0.7 * (1.0 - progress)).max(0.0);
                        let x_offset = 100.0 * progress;
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let final_x = combo_center_x + x_offset * direction;
                            hud_actors.push(act!(sprite("combo_1000milestone_swoosh.png"):
                                align(0.5, 0.5):
                                xy(final_x, combo_center_y):
                                zoom(zoom):
                                zoomx(zoom * direction):
                                diffuse(player_color[0], player_color[1], player_color[2], alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
            }
        }
    }
    // Combo
    if !profile.hide_combo {
        if p.miss_combo >= SHOW_COMBO_AT {
            let combo_y = if is_centered {
                receptor_y_centered + 155.0
            } else if state.reverse_scroll[player_idx] {
                screen_center_y() - COMBO_OFFSET_FROM_CENTER + notefield_offset_y
            } else {
                screen_center_y() + COMBO_OFFSET_FROM_CENTER + notefield_offset_y
            };
            let miss_combo_font_name = match profile.combo_font {
                crate::game::profile::ComboFont::Wendy => Some("wendy_combo"),
                crate::game::profile::ComboFont::ArialRounded => Some("combo_arial_rounded"),
                crate::game::profile::ComboFont::Asap => Some("combo_asap"),
                crate::game::profile::ComboFont::BebasNeue => Some("combo_bebas_neue"),
                crate::game::profile::ComboFont::SourceCode => Some("combo_source_code"),
                crate::game::profile::ComboFont::Work => Some("combo_work"),
                crate::game::profile::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
                crate::game::profile::ComboFont::None => None,
            };
            if let Some(font_name) = miss_combo_font_name {
                hud_actors.push(act!(text:
                    font(font_name): settext(p.miss_combo.to_string()):
                    align(0.5, 0.5): xy(playfield_center_x, combo_y):
                    zoom(0.75): horizalign(center): shadowlength(1.0):
                    diffuse(1.0, 0.0, 0.0, 1.0):
                    z(90)
                ));
            }
        } else if p.combo >= SHOW_COMBO_AT {
            let combo_y = if is_centered {
                receptor_y_centered + 155.0
            } else if state.reverse_scroll[player_idx] {
                screen_center_y() - COMBO_OFFSET_FROM_CENTER + notefield_offset_y
            } else {
                screen_center_y() + COMBO_OFFSET_FROM_CENTER + notefield_offset_y
            };
            let (color1, color2) = if let Some(fc_grade) = &p.full_combo_grade {
                match fc_grade {
                    JudgeGrade::Fantastic => {
                        (color::rgba_hex("#C8FFFF"), color::rgba_hex("#6BF0FF"))
                    }
                    JudgeGrade::Excellent => {
                        (color::rgba_hex("#FDFFC9"), color::rgba_hex("#FDDB85"))
                    }
                    JudgeGrade::Great => (color::rgba_hex("#C9FFC9"), color::rgba_hex("#94FEC1")),
                    _ => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
                }
            } else {
                ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0])
            };
            let effect_period = 0.8;
            let t = (state.total_elapsed_in_screen / effect_period).fract();
            let anim_t = ((t * 2.0 * std::f32::consts::PI).sin() + 1.0) / 2.0;
            let final_color = [
                color1[0] + (color2[0] - color1[0]) * anim_t,
                color1[1] + (color2[1] - color1[1]) * anim_t,
                color1[2] + (color2[2] - color1[2]) * anim_t,
                1.0,
            ];
            let combo_font_name = match profile.combo_font {
                crate::game::profile::ComboFont::Wendy => Some("wendy_combo"),
                crate::game::profile::ComboFont::ArialRounded => Some("combo_arial_rounded"),
                crate::game::profile::ComboFont::Asap => Some("combo_asap"),
                crate::game::profile::ComboFont::BebasNeue => Some("combo_bebas_neue"),
                crate::game::profile::ComboFont::SourceCode => Some("combo_source_code"),
                crate::game::profile::ComboFont::Work => Some("combo_work"),
                crate::game::profile::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
                crate::game::profile::ComboFont::None => None,
            };
            if let Some(font_name) = combo_font_name {
                hud_actors.push(act!(text:
                    font(font_name): settext(p.combo.to_string()):
                    align(0.5, 0.5): xy(playfield_center_x, combo_y):
                    zoom(0.75): horizalign(center): shadowlength(1.0):
                    diffuse(final_color[0], final_color[1], final_color[2], final_color[3]):
                    z(90)
                ));
            }
        }
    }

    // Error Bar (Simply Love parity)
    if profile.error_bar != crate::game::profile::ErrorBarStyle::None {
        let (error_bar_y, error_bar_max_h) = if matches!(
            profile.judgment_graphic,
            crate::game::profile::JudgmentGraphic::None
        ) {
            (judgment_y, 30.0_f32)
        } else if profile.error_bar_up {
            (judgment_y - ERROR_BAR_OFFSET_FROM_JUDGMENT, 10.0_f32)
        } else {
            (judgment_y + ERROR_BAR_OFFSET_FROM_JUDGMENT, 10.0_f32)
        };

        match profile.error_bar {
            crate::game::profile::ErrorBarStyle::Monochrome => {
                let bar_h = error_bar_max_h;
                let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                    (ERROR_BAR_WIDTH_MONOCHROME * 0.5) / max_offset_s
                } else {
                    0.0
                };
                let (bounds_s, bounds_len) = error_bar_boundaries_s(
                    state.timing_profile.windows_s,
                    state.timing_profile.fa_plus_window_s,
                    profile.show_fa_plus_window,
                    profile.error_bar_trim,
                );

                let bg_alpha = if matches!(
                    profile.background_filter,
                    crate::game::profile::BackgroundFilter::Off
                ) {
                    ERROR_BAR_MONO_BG_ALPHA
                } else {
                    0.0
                };
                if bg_alpha > 0.0 {
                    hud_actors.push(act!(quad:
                        align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                        zoomto(ERROR_BAR_WIDTH_MONOCHROME + 2.0, bar_h + 2.0):
                        diffuse(0.0, 0.0, 0.0, bg_alpha):
                        z(180)
                    ));
                }

                hud_actors.push(act!(quad:
                    align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                    zoomto(2.0, bar_h):
                    diffuse(0.5, 0.5, 0.5, 1.0):
                    z(181)
                ));

                let line_alpha = if elapsed_screen < ERROR_BAR_LINES_FADE_START_S {
                    0.0
                } else if elapsed_screen < ERROR_BAR_LINES_FADE_START_S + ERROR_BAR_LINES_FADE_DUR_S
                {
                    let t = (elapsed_screen - ERROR_BAR_LINES_FADE_START_S)
                        / ERROR_BAR_LINES_FADE_DUR_S;
                    ERROR_BAR_LINE_ALPHA * smoothstep01(t)
                } else {
                    ERROR_BAR_LINE_ALPHA
                };
                if line_alpha > 0.0 && wscale.is_finite() && wscale > 0.0 {
                    for i in 0..bounds_len {
                        let offset = bounds_s[i] * wscale;
                        if !offset.is_finite() {
                            continue;
                        }
                        for sx in [-1.0_f32, 1.0_f32] {
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + sx * offset, error_bar_y):
                                zoomto(1.0, bar_h):
                                diffuse(1.0, 1.0, 1.0, line_alpha):
                                z(182)
                            ));
                        }
                    }
                }

                let label_fade_out_start_s = ERROR_BAR_LABEL_FADE_DUR_S + ERROR_BAR_LABEL_HOLD_S;
                let label_alpha = if elapsed_screen < ERROR_BAR_LABEL_FADE_DUR_S {
                    smoothstep01(elapsed_screen / ERROR_BAR_LABEL_FADE_DUR_S)
                } else if elapsed_screen < label_fade_out_start_s {
                    1.0
                } else if elapsed_screen < label_fade_out_start_s + ERROR_BAR_LABEL_FADE_DUR_S {
                    1.0 - smoothstep01(
                        (elapsed_screen - label_fade_out_start_s) / ERROR_BAR_LABEL_FADE_DUR_S,
                    )
                } else {
                    0.0
                };
                if label_alpha > 0.0 {
                    let x_off = ERROR_BAR_WIDTH_MONOCHROME * 0.25;
                    hud_actors.push(act!(text:
                        font("game"): settext("Early"):
                        align(0.5, 0.5): xy(playfield_center_x - x_off, error_bar_y):
                        zoom(0.7): diffuse(1.0, 1.0, 1.0, label_alpha):
                        z(184)
                    ));
                    hud_actors.push(act!(text:
                        font("game"): settext("Late"):
                        align(0.5, 0.5): xy(playfield_center_x + x_off, error_bar_y):
                        zoom(0.7): diffuse(1.0, 1.0, 1.0, label_alpha):
                        z(184)
                    ));
                }

                if wscale.is_finite() && wscale > 0.0 {
                    let multi_tick = profile.error_bar_multi_tick;
                    for tick_opt in &p.error_bar_mono_ticks {
                        let Some(tick) = tick_opt else {
                            continue;
                        };
                        let alpha = error_bar_tick_alpha(
                            elapsed_screen - tick.started_at,
                            ERROR_BAR_TICK_DUR_MONOCHROME,
                            multi_tick,
                        );
                        if alpha <= 0.0 {
                            continue;
                        }
                        let x = tick.offset_s * wscale;
                        if !x.is_finite() {
                            continue;
                        }
                        let c =
                            error_bar_color_for_window(tick.window, profile.show_fa_plus_window);
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, bar_h):
                            diffuse(c[0], c[1], c[2], alpha):
                            z(183)
                        ));
                    }
                }
            }
            crate::game::profile::ErrorBarStyle::Colorful => {
                let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                    (ERROR_BAR_WIDTH_COLORFUL * 0.5) / max_offset_s
                } else {
                    0.0
                };
                let (bounds_s, bounds_len) = error_bar_boundaries_s(
                    state.timing_profile.windows_s,
                    state.timing_profile.fa_plus_window_s,
                    profile.show_fa_plus_window,
                    profile.error_bar_trim,
                );

                let bar_visible = p
                    .error_bar_color_bar_started_at
                    .map(|t0| {
                        let age = elapsed_screen - t0;
                        age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL
                    })
                    .unwrap_or(false);

                if bar_visible && wscale.is_finite() && wscale > 0.0 {
                    hud_actors.push(act!(quad:
                        align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                        zoomto(ERROR_BAR_WIDTH_COLORFUL + 4.0, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                        diffuse(0.0, 0.0, 0.0, 1.0):
                        z(180)
                    ));

                    let base = if profile.show_fa_plus_window {
                        0usize
                    } else {
                        1usize
                    };
                    let mut lastx = 0.0_f32;
                    for i in 0..bounds_len {
                        let x = bounds_s[i] * wscale;
                        let width = x - lastx;
                        if !x.is_finite() || !width.is_finite() || width <= 0.0 {
                            lastx = x;
                            continue;
                        }
                        let window_num = base + i;
                        let window = timing_window_from_num(window_num);
                        let wi = window_num.min(5);
                        let c = error_bar_color_for_window(window, profile.show_fa_plus_window);
                        let early_a = error_bar_flash_alpha(
                            elapsed_screen,
                            p.error_bar_color_flash_early[wi],
                            ERROR_BAR_TICK_DUR_COLORFUL,
                        );
                        let late_a = error_bar_flash_alpha(
                            elapsed_screen,
                            p.error_bar_color_flash_late[wi],
                            ERROR_BAR_TICK_DUR_COLORFUL,
                        );

                        let cx_early = -0.5 * (lastx + x);
                        let cx_late = 0.5 * (lastx + x);
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x + cx_early, error_bar_y):
                            zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                            diffuse(c[0], c[1], c[2], early_a):
                            z(181)
                        ));
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x + cx_late, error_bar_y):
                            zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                            diffuse(c[0], c[1], c[2], late_a):
                            z(181)
                        ));

                        lastx = x;
                    }
                }

                if wscale.is_finite() && wscale > 0.0 {
                    let multi_tick = profile.error_bar_multi_tick;
                    for tick_opt in &p.error_bar_color_ticks {
                        let Some(tick) = tick_opt else {
                            continue;
                        };
                        let alpha = error_bar_tick_alpha(
                            elapsed_screen - tick.started_at,
                            ERROR_BAR_TICK_DUR_COLORFUL,
                            multi_tick,
                        );
                        if alpha <= 0.0 {
                            continue;
                        }
                        let x = tick.offset_s * wscale;
                        if !x.is_finite() {
                            continue;
                        }
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(182)
                        ));
                    }
                }
            }
            crate::game::profile::ErrorBarStyle::Text => {
                if let Some(text) = p.error_bar_text {
                    let age = elapsed_screen - text.started_at;
                    if age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL {
                        let x = if text.early { -40.0 } else { 40.0 };
                        let s = if text.early { "EARLY" } else { "LATE" };
                        hud_actors.push(act!(text:
                            font("wendy"): settext(s):
                            align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                            zoom(0.25): shadowlength(1.0):
                            diffuse(1.0, 1.0, 1.0, 1.0):
                            z(184)
                        ));
                    }
                }
            }
            crate::game::profile::ErrorBarStyle::None => {}
        }
    }

    // Measure Counter / Measure Breakdown (Zmod parity)
    if profile.measure_counter != crate::game::profile::MeasureCounter::None {
        let segs: &[StreamSegment] = &state.measure_counter_segments[player_idx];
        if !segs.is_empty() {
            let layout = zmod_layout_ys(profile, judgment_y);
            let lookahead: u8 = profile.measure_counter_lookahead.min(4);
            let multiplier = profile.measure_counter.multiplier();

            let mc_font_name = match profile.combo_font {
                crate::game::profile::ComboFont::Wendy | crate::game::profile::ComboFont::WendyCursed => {
                    "wendy"
                }
                crate::game::profile::ComboFont::ArialRounded => "combo_arial_rounded",
                crate::game::profile::ComboFont::Asap => "combo_asap",
                crate::game::profile::ComboFont::BebasNeue => "combo_bebas_neue",
                crate::game::profile::ComboFont::SourceCode => "combo_source_code",
                crate::game::profile::ComboFont::Work => "combo_work",
                crate::game::profile::ComboFont::None => "wendy",
            };

            let beat_floor = state.current_beat_visible[player_idx].floor();
            let curr_measure = beat_floor / 4.0;
            let base_index = segs
                .iter()
                .position(|s| curr_measure < s.end as f32)
                .unwrap_or(segs.len());

            let mut column_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
            if profile.measure_counter_left {
                column_width *= 4.0 / 3.0;
            }

            if let Some(measure_counter_y) = layout.measure_counter_y {
                for j in (0..=lookahead).rev() {
                    let seg_index_unshifted = base_index + j as usize;
                    if seg_index_unshifted >= segs.len() {
                        continue;
                    }

                    let is_lookahead = j != 0;
                    let text = zmod_measure_counter_text(
                        beat_floor,
                        curr_measure,
                        segs,
                        seg_index_unshifted,
                        is_lookahead,
                        lookahead,
                        multiplier,
                    );
                    if text.is_empty() {
                        continue;
                    }

                    let seg_unshifted = segs[seg_index_unshifted];
                    let rgba = if seg_unshifted.is_break {
                        if is_lookahead {
                            [0.4, 0.4, 0.4, 1.0]
                        } else {
                            [0.5, 0.5, 0.5, 1.0]
                        }
                    } else if is_lookahead {
                        [0.45, 0.45, 0.45, 1.0]
                    } else if text.contains('/') {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        [0.5, 0.5, 0.5, 1.0]
                    };

                    let zoom = 0.35 - 0.05 * (j as f32);
                    let mut x = playfield_center_x;
                    let mut y = measure_counter_y;

                    if profile.measure_counter_vert {
                        y += 20.0 * (j as f32);
                    } else {
                        let denom = if lookahead == 0 { 1.0 } else { lookahead as f32 };
                        x += (column_width / denom) * 2.0 * (j as f32);
                    }
                    if profile.measure_counter_left {
                        x -= column_width;
                    }

                    hud_actors.push(act!(text:
                        font(mc_font_name): settext(text):
                        align(0.5, 0.5): xy(x, y):
                        zoom(zoom): horizalign(center): shadowlength(1.0):
                        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                        z(85)
                    ));
                }

                // Broken Run Total (Zmod BrokenRunCounter.lua)
                if profile.broken_run
                    && let Some((broken_index, broken_end, is_broken)) =
                        zmod_broken_run_segment(segs, curr_measure)
                {
                    let seg0 = segs[broken_index];
                    if !seg0.is_break && is_broken {
                        let curr_count =
                            (curr_measure - (seg0.start as f32)).floor() as i32 + 1;
                        let len = (broken_end - seg0.start) as i32;
                        let text = if curr_measure < 0.0 {
                            // BrokenRunCounter.lua special-cases negative time.
                            let first = segs[0];
                            if !first.is_break {
                                let v = (curr_measure * -1.0).floor() as i32 + 1;
                                format!("({v})")
                            } else {
                                let first_len = (first.end - first.start) as i32;
                                let v = (curr_measure * -1.0).floor() as i32 + 1 + first_len;
                                format!("({v})")
                            }
                        } else if curr_count != 0 {
                            format!("{curr_count}/{len}")
                        } else {
                            len.to_string()
                        };

                        if text.contains('/') {
                            let mut x = playfield_center_x;
                            let mut y = measure_counter_y + 15.0;
                            if profile.measure_counter_vert {
                                y -= 15.0;
                                x += column_width * (4.0 / 3.0);
                            }
                            if profile.measure_counter_left {
                                x -= column_width;
                            }

                            hud_actors.push(act!(text:
                                font(mc_font_name): settext(text):
                                align(0.5, 0.5): xy(x, y):
                                zoom(0.35): horizalign(center): shadowlength(1.0):
                                diffuse(1.0, 1.0, 1.0, 0.7):
                                z(85)
                            ));
                        }
                    }
                }
            }

            // Run Timer (Zmod RunTimer.lua: TimerMode=Time only)
            if profile.run_timer
                && let Some(stream_index) = zmod_run_timer_index(segs, curr_measure)
            {
                let seg = segs[stream_index];
                if !seg.is_break {
                    let cur_bps = state.timing.get_bpm_for_beat(state.current_beat) / 60.0;
                    let rate = state.music_rate;
                    if cur_bps.is_finite() && cur_bps > 0.0 && rate.is_finite() && rate > 0.0 {
                        let measure_seconds = 4.0 / (cur_bps * rate);
                        let curr_time = state.current_beat / (cur_bps * rate);

                        let seg_len_s =
                            (((seg.end - seg.start) as f32) * measure_seconds).ceil() as i32;
                        let total = zmod_run_timer_fmt(seg_len_s, 60);

                        let remaining_s =
                            (((seg.end as f32) * measure_seconds) - curr_time).ceil() as i32;
                        let remaining_s = remaining_s.max(0);

                        let text = if remaining_s > seg_len_s {
                            total
                        } else if remaining_s < 1 {
                            "0.00 ".to_string()
                        } else {
                            let rem = zmod_run_timer_fmt(remaining_s, 59);
                            format!("{rem} ")
                        };

                        let active = text.contains(' ');
                        let rgba = if active {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [0.5, 0.5, 0.5, 1.0]
                        };

                        let mut x = playfield_center_x;
                        if profile.measure_counter_left {
                            x -= column_width;
                        }
                        let y = layout.subtractive_scoring_y;

                        hud_actors.push(act!(text:
                            font(mc_font_name): settext(text):
                            align(0.5, 0.5): xy(x, y):
                            zoom(0.35): horizalign(center): shadowlength(1.0):
                            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                            z(85)
                        ));
                    }
                }
            }
        }
    }

    // Judgment Sprite (tap judgments)
    if let Some(render_info) = &p.last_judgment {
        if matches!(profile.judgment_graphic, profile::JudgmentGraphic::None) {
            // Player chose to hide tap judgment graphics.
            // Still keep life/score effects; only suppress the visual sprite.
        } else {
            let judgment = &render_info.judgment;
            let elapsed = render_info.judged_at.elapsed().as_secs_f32();
            if elapsed < 0.9 {
                let zoom = if elapsed < 0.1 {
                    let t = elapsed / 0.1;
                    let ease_t = 1.0 - (1.0 - t).powi(2);
                    0.8 + (0.75 - 0.8) * ease_t
                } else if elapsed < 0.7 {
                    0.75
                } else {
                    let t = (elapsed - 0.7) / 0.2;
                    let ease_t = t.powi(2);
                    0.75 * (1.0 - ease_t)
                };
                let offset_sec = judgment.time_error_ms / 1000.0;
                let use_fa_plus_window = profile.show_fa_plus_window;
                // Map JudgeGrade + TimingWindow to a row index in the 7-row sheet:
                //  row 0: FA+ Fantastic (W0)
                //  row 1: regular Fantastic (W1)
                //  row 2..6: Excellent..Miss, matching our existing layout.
                let frame_row = match judgment.grade {
                    JudgeGrade::Fantastic => {
                        if use_fa_plus_window {
                            match judgment.window {
                                Some(TimingWindow::W0) => 0,
                                _ => 1,
                            }
                        } else {
                            0
                        }
                    }
                    JudgeGrade::Excellent => 2,
                    JudgeGrade::Great => 3,
                    JudgeGrade::Decent => 4,
                    JudgeGrade::WayOff => 5,
                    JudgeGrade::Miss => 6,
                };
                let frame_offset = if offset_sec < 0.0 { 0 } else { 1 };
                let columns = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Censored => 1,
                    _ => 2,
                };
                let col_index = if columns > 1 { frame_offset } else { 0 };
                let linear_index = (frame_row * columns + col_index) as u32;
                let judgment_texture = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Bebas => "judgements/Bebas 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Censored => "judgements/Censored 1x7 (doubleres).png",
                    profile::JudgmentGraphic::Chromatic => {
                        "judgements/Chromatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Code => "judgements/Code 2x7 (doubleres).png",
                    profile::JudgmentGraphic::ComicSans => {
                        "judgements/Comic Sans 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Emoticon => "judgements/Emoticon 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Focus => "judgements/Focus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Grammar => "judgements/Grammar 2x7 (doubleres).png",
                    profile::JudgmentGraphic::GrooveNights => {
                        "judgements/GrooveNights 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::ITG2 => "judgements/ITG2 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Love => "judgements/Love 2x7 (doubleres).png",
                    profile::JudgmentGraphic::LoveChroma => {
                        "judgements/Love Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Miso => "judgements/Miso 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Papyrus => "judgements/Papyrus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Rainbowmatic => {
                        "judgements/Rainbowmatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Roboto => "judgements/Roboto 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Shift => "judgements/Shift 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Tactics => "judgements/Tactics 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Wendy => "judgements/Wendy 2x7 (doubleres).png",
                    profile::JudgmentGraphic::WendyChroma => {
                        "judgements/Wendy Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::None => {
                        unreachable!("JudgmentGraphic::None is filtered above")
                    }
                };
                let rot_deg = if profile.judgment_tilt && judgment.grade != JudgeGrade::Miss {
                    let abs_sec = offset_sec.abs().min(0.050);
                    let dir = if offset_sec < 0.0 { -1.0 } else { 1.0 };
                    dir * abs_sec * 300.0 * profile.tilt_multiplier
                } else {
                    0.0
                };
                hud_actors.push(act!(sprite(judgment_texture):
                    align(0.5, 0.5): xy(playfield_center_x, judgment_y):
                    z(200): rotationz(rot_deg): zoomtoheight(76.0): setstate(linear_index): zoom(zoom)
                ));
            }
        }
    }
    for i in 0..num_cols {
        let col = col_start + i;
        let Some(render_info) = state.hold_judgments[col].as_ref() else {
            continue;
        };
        let elapsed = render_info.triggered_at.elapsed().as_secs_f32();
        if elapsed >= HOLD_JUDGMENT_TOTAL_DURATION {
            continue;
        }
        // Hold judgments scale with Mini/Tiny in ITGmania as pow(0.5, mini+tiny), clamped to 1.0.
        let mini_for_holds = ((profile.mini_percent as f32).clamp(-100.0, 150.0) / 100.0).max(0.0);
        let hold_judgment_zoom_mod = 0.5_f32.powf(mini_for_holds).min(1.0);
        let zoom = if elapsed < 0.3 {
            let progress = (elapsed / 0.3).clamp(0.0, 1.0);
            (HOLD_JUDGMENT_INITIAL_ZOOM
                + progress * (HOLD_JUDGMENT_FINAL_ZOOM - HOLD_JUDGMENT_INITIAL_ZOOM))
                * hold_judgment_zoom_mod
        } else {
            HOLD_JUDGMENT_FINAL_ZOOM * hold_judgment_zoom_mod
        };
        let frame_index = match render_info.result {
            HoldResult::Held => 0,
            HoldResult::LetGo => 1,
        } as u32;
        if let Some(texture) = hold_judgment_texture {
            let dir = column_dirs[i];
            let receptor_y_lane = column_receptor_ys[i];
            let hold_judgment_y = if dir >= 0.0 {
                // Non-reverse lane: match Simply Love's baseline offset below receptors.
                receptor_y_lane + HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR
            } else {
                // Reverse lane: mirror around the receptor so the hold judgment
                // appears just above the receptors instead of near screen center.
                receptor_y_lane - HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR
            };
            let column_offset = state.noteskin[player_idx]
                .as_ref()
                .and_then(|ns| ns.column_xs.get(i))
                .map(|&x| x as f32)
                .unwrap_or_else(|| ((i as f32) - 1.5) * TARGET_ARROW_PIXEL_SIZE * field_zoom);
            hud_actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(playfield_center_x + column_offset, hold_judgment_y):
                z(195):
                setstate(frame_index):
                zoom(zoom):
                diffusealpha(1.0)
            ));
        }
    }

    let (tilt, skew) = profile.perspective.tilt_skew();
    if (tilt != 0.0 || skew != 0.0) && !actors.is_empty() {
        let center_y = 0.5 * (receptor_y_normal + receptor_y_reverse);
        let reverse = column_dirs[0] < 0.0;
        if let Some(view_proj) = notefield_view_proj(
            screen_width(),
            screen_height(),
            playfield_center_x,
            center_y,
            tilt,
            skew,
            reverse,
        ) {
            actors = vec![Actor::Camera {
                view_proj,
                children: actors,
            }];
        }
    }

    if hud_actors.is_empty() {
        return (actors, playfield_center_x);
    }
    let mut out: Vec<Actor> = Vec::with_capacity(hud_actors.len() + actors.len());
    out.extend(hud_actors);
    out.extend(actors);
    (out, playfield_center_x)
}
