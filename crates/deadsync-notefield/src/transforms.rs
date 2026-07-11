use crate::style::*;
use deadsync_gameplay::VisualEffects;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TornadoBounds {
    pub min_x: f32,
    pub max_x: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoteAlphaParams {
    pub hidden: f32,
    pub hidden_offset: f32,
    pub sudden: f32,
    pub sudden_offset: f32,
    pub stealth: f32,
    pub blink: f32,
    pub random_vanish: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisualEffectParams {
    pub bumpy: f32,
    pub bumpy_offset: f32,
    pub bumpy_period: f32,
    pub tiny: f32,
    pub pulse_inner: f32,
    pub pulse_outer: f32,
    pub pulse_offset: f32,
    pub pulse_period: f32,
    pub confusion: f32,
    pub confusion_offset: f32,
    pub dizzy: f32,
    pub rotate_z: f32,
    pub beat: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccelYParams {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub expand: f32,
    pub boomerang: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoteXParams {
    pub screen_height: f32,
    pub flip: f32,
    pub invert: f32,
    pub tornado: f32,
    pub drunk: f32,
    pub beat: f32,
}

pub fn sm_scale(v: f32, in0: f32, in1: f32, out0: f32, out1: f32) -> f32 {
    let denom = in1 - in0;
    if denom.abs() < 1e-6 {
        return out1;
    }
    ((v - in0) / denom).mul_add(out1 - out0, out0)
}

pub fn quantize_step(v: f32, step: f32) -> f32 {
    if !v.is_finite() || !step.is_finite() || step == 0.0 {
        0.0
    } else {
        ((v + step * 0.5) / step).trunc() * step
    }
}

pub fn quantize_centi_i32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    (value * 100.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

pub fn quantize_centi_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * 100.0).round().min(u32::MAX as f64) as u32
}

pub fn mod_percent_key(level: f32) -> i16 {
    clamp_rounded_i16(level * 100.0)
}

pub fn clamp_rounded_i16(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

pub fn beat_factor(song_beat: f32) -> f32 {
    if !song_beat.is_finite() {
        return 0.0;
    }
    let accel_time = 0.2_f32;
    let total_time = 0.5_f32;
    let mut beat = song_beat + accel_time;
    let even_beat = (beat as i32 % 2) != 0;
    if beat < 0.0 {
        return 0.0;
    }
    beat -= beat.trunc();
    beat += 1.0;
    beat -= beat.trunc();
    if beat >= total_time {
        return 0.0;
    }
    let mut factor = if beat < accel_time {
        let t = sm_scale(beat, 0.0, accel_time, 0.0, 1.0);
        t * t
    } else {
        let t = sm_scale(beat, accel_time, total_time, 1.0, 0.0);
        1.0 - (1.0 - t) * (1.0 - t)
    };
    if even_beat {
        factor *= -1.0;
    }
    factor * 20.0
}

pub fn mod_divisor(value: f32) -> f32 {
    if value.abs() > 0.001 {
        value
    } else if value.is_sign_negative() {
        -0.001
    } else {
        0.001
    }
}

fn signed_effect_active(value: f32) -> bool {
    value.is_finite() && value.abs() > f32::EPSILON
}

pub fn bumpy_angle(y: f32, offset: f32, period: f32) -> f32 {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    let divisor = mod_divisor(period.mul_add(BUMPY_Z_ANGLE_DIVISOR, BUMPY_Z_ANGLE_DIVISOR));
    (y + 100.0 * offset) / divisor
}

pub fn apply_accel_y_with_peak(
    raw_y: f32,
    elapsed: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
) -> (f32, bool) {
    if raw_y < 0.0 {
        return (raw_y, true);
    }
    let mut y = raw_y;
    if accel.boost > f32::EPSILON {
        let new_y = y * 1.5 / ((y + effect_height / 1.2) / effect_height);
        let mut adjust = accel.boost * (new_y - y);
        adjust = adjust.clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
        y += adjust;
    }
    if accel.brake > f32::EPSILON {
        let scale = sm_scale(y, 0.0, effect_height, 0.0, 1.0);
        let new_y = y * scale;
        let mut adjust = accel.brake * (new_y - y);
        adjust = adjust.clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
        y += adjust;
    }
    if accel.wave > f32::EPSILON {
        y += accel.wave * WAVE_MOD_MAGNITUDE * (y / WAVE_MOD_HEIGHT.mul_add(1.0, 0.0)).sin();
    }
    let mut before_boomerang_peak = true;
    if accel.boomerang > f32::EPSILON {
        let peak_at_y = screen_height * 0.75;
        before_boomerang_peak = y < peak_at_y;
        y = (-y * y / screen_height) + 1.5 * y;
    }
    if accel.expand > f32::EPSILON {
        let seconds = elapsed.rem_euclid((std::f32::consts::PI * 2.0).max(f32::EPSILON));
        let multiplier = sm_scale(
            (seconds * EXPAND_MULTIPLIER_FREQUENCY).cos(),
            EXPAND_MULTIPLIER_SCALE_FROM_LOW,
            EXPAND_MULTIPLIER_SCALE_FROM_HIGH,
            EXPAND_MULTIPLIER_SCALE_TO_LOW,
            EXPAND_MULTIPLIER_SCALE_TO_HIGH,
        );
        y *= sm_scale(
            accel.expand,
            EXPAND_SPEED_SCALE_FROM_LOW,
            EXPAND_SPEED_SCALE_FROM_HIGH,
            EXPAND_SPEED_SCALE_TO_LOW,
            multiplier,
        );
    }
    (y, before_boomerang_peak)
}

pub fn apply_accel_y(
    raw_y: f32,
    elapsed: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
) -> f32 {
    apply_accel_y_with_peak(raw_y, elapsed, effect_height, screen_height, accel).0
}

pub fn note_world_z_for_bumpy(y: f32, bumpy: f32, offset: f32, period: f32) -> f32 {
    if bumpy.abs() <= f32::EPSILON || !bumpy.is_finite() {
        return 0.0;
    }
    bumpy * BUMPY_Z_MAGNITUDE * bumpy_angle(y, offset, period).sin()
}

pub fn itg_actor_rotation_z(deg: f32) -> f32 {
    -deg
}

pub fn visual_hold_body_needs_z_buffer(params: VisualEffectParams) -> bool {
    signed_effect_active(params.bumpy)
}

pub fn visual_use_legacy_hold_sprites(
    bumpy: f32,
    tiny: f32,
    pulse_outer: f32,
    pulse_inner: f32,
    arrow_effect: f32,
) -> bool {
    [bumpy, tiny, pulse_outer, pulse_inner, arrow_effect]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= f32::EPSILON)
}

pub fn visual_tiny_zoom(params: VisualEffectParams) -> f32 {
    if !params.tiny.is_finite() || params.tiny.abs() <= f32::EPSILON {
        1.0
    } else {
        0.5_f32.powf(params.tiny)
    }
}

pub fn visual_pulse_active(params: VisualEffectParams) -> bool {
    signed_effect_active(params.pulse_inner) || signed_effect_active(params.pulse_outer)
}

pub fn visual_pulse_inner_zoom(params: VisualEffectParams) -> f32 {
    if !visual_pulse_active(params) {
        return 1.0;
    }
    let inner = if params.pulse_inner.is_finite() {
        params.pulse_inner.mul_add(0.5, 1.0)
    } else {
        1.0
    };
    if inner.abs() <= f32::EPSILON {
        0.01
    } else {
        inner
    }
}

pub fn visual_pulse_zoom_for_y(y: f32, params: VisualEffectParams) -> f32 {
    if !visual_pulse_active(params) {
        return 1.0;
    }
    let outer = if params.pulse_outer.is_finite() {
        params.pulse_outer
    } else {
        0.0
    };
    let offset = if params.pulse_offset.is_finite() {
        params.pulse_offset
    } else {
        0.0
    };
    let period = if params.pulse_period.is_finite() {
        params.pulse_period
    } else {
        0.0
    };
    let divisor = mod_divisor(0.4 * ARROW_EFFECT_PIXEL_SIZE * (1.0 + period));
    ((y + 100.0 * offset) / divisor)
        .sin()
        .mul_add(outer * 0.5, visual_pulse_inner_zoom(params))
}

pub fn visual_arrow_effect_zoom(y: f32, params: VisualEffectParams) -> f32 {
    visual_tiny_zoom(params) * visual_pulse_zoom_for_y(y, params)
}

pub fn visual_confusion_rotation_deg(song_beat: f32, params: VisualEffectParams) -> f32 {
    (params.confusion_offset + song_beat * params.confusion).rem_euclid(std::f32::consts::TAU)
        * (-180.0 / std::f32::consts::PI)
}

pub fn visual_dizzy_rotation_deg(
    note_beat: f32,
    song_beat: f32,
    params: VisualEffectParams,
) -> f32 {
    ((note_beat - song_beat) * params.dizzy) % std::f32::consts::TAU
        * (-180.0 / std::f32::consts::PI)
}

pub fn visual_note_rotation_z(
    note_beat: f32,
    song_beat: f32,
    _is_hold_head: bool,
    params: VisualEffectParams,
) -> f32 {
    itg_actor_rotation_z(params.rotate_z) - visual_confusion_rotation_deg(song_beat, params)
        + visual_dizzy_rotation_deg(note_beat, song_beat, params)
}

pub fn visual_effect_params_for_col(
    mut params: VisualEffectParams,
    col: usize,
    tiny: &[f32],
    confusion_offset: &[f32],
    bumpy: &[f32],
) -> VisualEffectParams {
    if let Some(v) = tiny.get(col).copied().filter(|v| v.is_finite()) {
        params.tiny += v;
    }
    if let Some(v) = confusion_offset.get(col).copied().filter(|v| v.is_finite()) {
        params.confusion_offset += v;
    }
    if let Some(v) = bumpy.get(col).copied().filter(|v| v.is_finite()) {
        params.bumpy += v;
    }
    params
}

pub fn gameplay_visual_effect_params(
    visual: &VisualEffects,
    local_col: usize,
) -> VisualEffectParams {
    visual_effect_params_for_col(
        VisualEffectParams {
            tiny: visual.tiny,
            pulse_inner: visual.pulse_inner,
            pulse_outer: visual.pulse_outer,
            pulse_offset: visual.pulse_offset,
            pulse_period: visual.pulse_period,
            confusion: visual.confusion,
            confusion_offset: visual.confusion_offset,
            dizzy: visual.dizzy,
            bumpy: visual.bumpy,
            bumpy_offset: visual.bumpy_offset,
            bumpy_period: visual.bumpy_period,
            rotate_z: 0.0,
            beat: visual.beat,
        },
        local_col,
        &visual.tiny_cols,
        &visual.confusion_offset_cols,
        &visual.bumpy_cols,
    )
}

pub fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn compute_invert_distances(col_offsets: &[f32], out: &mut [f32]) {
    let num_cols = col_offsets.len();
    if num_cols == 0 {
        return;
    }
    let num_sides = if num_cols > 4 { 2 } else { 1 };
    let cols_per_side = (num_cols / num_sides).max(1);
    for i in 0..out.len().min(num_cols) {
        let side = i / cols_per_side;
        let on_side = i % cols_per_side;
        let left_mid = (cols_per_side - 1) / 2;
        let right_mid = cols_per_side.div_ceil(2);
        let (first, last) = if on_side <= left_mid {
            (0, left_mid)
        } else if on_side >= right_mid {
            (right_mid, cols_per_side - 1)
        } else {
            (on_side / 2, on_side / 2)
        };
        let new_on_side = if first == last {
            0
        } else {
            sm_scale(
                on_side as f32,
                first as f32,
                last as f32,
                last as f32,
                first as f32,
            )
            .round() as usize
        };
        let new_col = side * cols_per_side + new_on_side.min(num_cols.saturating_sub(1));
        out[i] = col_offsets[new_col] - col_offsets[i];
    }
}

pub fn compute_tornado_bounds(col_offsets: &[f32], out: &mut [TornadoBounds]) {
    let num_cols = col_offsets.len();
    let width = if num_cols > 4 { 2 } else { 3 };
    for (i, bounds) in out.iter_mut().take(num_cols).enumerate() {
        let start = i.saturating_sub(width);
        let end = (i + width).min(num_cols.saturating_sub(1));
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in &col_offsets[start..=end] {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
        }
        *bounds = TornadoBounds { min_x, max_x };
    }
}

pub fn tipsy_y_extra(local_col: usize, elapsed: f32, tipsy: f32) -> f32 {
    if !signed_effect_active(tipsy) {
        return 0.0;
    }
    let col = local_col as f32;
    let angle = elapsed * TIPSY_TIMER_FREQUENCY + col * TIPSY_COLUMN_FREQUENCY;
    tipsy * angle.cos() * ARROW_EFFECT_PIXEL_SIZE * TIPSY_ARROW_MAGNITUDE
}

pub fn beat_x_extra(y: f32, beat_factor: f32, beat: f32) -> f32 {
    if !signed_effect_active(beat) {
        return 0.0;
    }
    let shift =
        beat_factor * (y / BEAT_OFFSET_HEIGHT + std::f32::consts::PI / BEAT_PI_HEIGHT).sin();
    beat * shift
}

pub fn drunk_x_extra(
    local_col: usize,
    y: f32,
    elapsed: f32,
    screen_height: f32,
    drunk: f32,
) -> f32 {
    if !signed_effect_active(drunk) {
        return 0.0;
    }
    let col = local_col as f32;
    let angle = elapsed + col * DRUNK_COLUMN_FREQUENCY + y * DRUNK_OFFSET_FREQUENCY / screen_height;
    drunk * angle.cos() * ARROW_EFFECT_PIXEL_SIZE * DRUNK_ARROW_MAGNITUDE
}

pub fn tornado_x_extra(
    y: f32,
    base_x: f32,
    bounds: TornadoBounds,
    screen_height: f32,
    tornado: f32,
) -> f32 {
    if !signed_effect_active(tornado) {
        return 0.0;
    }
    let position_between = sm_scale(base_x, bounds.min_x, bounds.max_x, -1.0, 1.0).clamp(-1.0, 1.0);
    let radians = position_between.acos() + y * TORNADO_X_OFFSET_FREQUENCY / screen_height;
    let adjusted = sm_scale(radians.cos(), -1.0, 1.0, bounds.min_x, bounds.max_x);
    (adjusted - base_x) * tornado
}

pub fn note_x_extra(
    local_col: usize,
    y: f32,
    beat_factor_value: f32,
    elapsed: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    params: NoteXParams,
) -> f32 {
    let base_x = col_offsets.get(local_col).copied().unwrap_or(0.0);
    let mut out = 0.0;
    if signed_effect_active(params.tornado) {
        out += tornado_x_extra(
            y,
            base_x,
            tornado.get(local_col).copied().unwrap_or_default(),
            params.screen_height,
            params.tornado,
        );
    }
    if signed_effect_active(params.drunk) {
        out += drunk_x_extra(local_col, y, elapsed, params.screen_height, params.drunk);
    }
    if signed_effect_active(params.flip) {
        let mirrored = col_offsets
            .get(
                col_offsets
                    .len()
                    .saturating_sub(1)
                    .saturating_sub(local_col),
            )
            .copied()
            .unwrap_or(base_x);
        out += (mirrored - base_x) * params.flip;
    }
    if signed_effect_active(params.invert) {
        out += invert.get(local_col).copied().unwrap_or(0.0) * params.invert;
    }
    if signed_effect_active(params.beat) {
        out += beat_x_extra(y, beat_factor_value, params.beat);
    }
    out
}

pub fn note_x_offset(
    local_col: usize,
    y: f32,
    beat_factor_value: f32,
    elapsed: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    move_x: &[f32],
    params: NoteXParams,
    tiny_zoom: f32,
) -> f32 {
    let base = col_offsets.get(local_col).copied().unwrap_or(0.0)
        + note_x_extra(
            local_col,
            y,
            beat_factor_value,
            elapsed,
            col_offsets,
            invert,
            tornado,
            params,
        );
    base * tiny_spacing_scale(tiny_zoom) + move_col_extra(move_x, local_col)
}

pub fn appearance_note_alpha(y: f32, elapsed: f32, mini: f32, params: NoteAlphaParams) -> f32 {
    if y < 0.0 {
        return 1.0;
    }

    let zoom = (1.0 - mini * 0.5).abs().max(0.01);
    let center_line = CENTER_LINE_Y / zoom;
    let hidden_sudden = params.hidden * params.sudden;
    let hidden_end = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, -1.0, -1.25)
        + center_line * params.hidden_offset;
    let hidden_start = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, -0.25)
        + center_line * params.hidden_offset;
    let sudden_end = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, 0.25)
        + center_line * params.sudden_offset;
    let sudden_start = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 1.0, 1.25)
        + center_line * params.sudden_offset;

    let mut visible_adjust = 0.0;
    if params.hidden > f32::EPSILON {
        visible_adjust +=
            params.hidden * sm_scale(y, hidden_start, hidden_end, 0.0, -1.0).clamp(-1.0, 0.0);
    }
    if params.sudden > f32::EPSILON {
        visible_adjust +=
            params.sudden * sm_scale(y, sudden_start, sudden_end, -1.0, 0.0).clamp(-1.0, 0.0);
    }
    if params.stealth > f32::EPSILON {
        visible_adjust -= params.stealth;
    }
    if params.blink > f32::EPSILON {
        let blink = quantize_step((elapsed * 10.0).sin(), BLINK_MOD_FREQUENCY);
        visible_adjust += sm_scale(blink, 0.0, 1.0, -1.0, 0.0);
    }
    if params.random_vanish > f32::EPSILON {
        let dist = (y - center_line).abs();
        visible_adjust += sm_scale(dist, 80.0, 160.0, -1.0, 0.0) * params.random_vanish;
    }
    (1.0 + visible_adjust).clamp(0.0, 1.0)
}

pub fn appearance_note_glow(y: f32, elapsed: f32, mini: f32, params: NoteAlphaParams) -> f32 {
    let percent_visible = appearance_note_alpha(y, elapsed, mini, params);
    sm_scale((percent_visible - 0.5).abs(), 0.0, 0.5, 1.3, 0.0).max(0.0)
}

pub fn appearance_note_actor_alpha(
    y: f32,
    elapsed: f32,
    mini: f32,
    params: NoteAlphaParams,
) -> f32 {
    if appearance_note_alpha(y, elapsed, mini, params) > 0.5 {
        1.0
    } else {
        0.0
    }
}

pub fn appearance_needs_rows(appearance: NoteAlphaParams) -> bool {
    appearance.hidden > f32::EPSILON
        || appearance.sudden > f32::EPSILON
        || appearance.random_vanish > f32::EPSILON
}

pub fn tiny_spacing_scale(tiny: f32) -> f32 {
    if !tiny.is_finite() || tiny.abs() <= f32::EPSILON {
        1.0
    } else {
        0.5_f32.powf(tiny).min(1.0)
    }
}

pub fn move_col_extra(values: &[f32], local_col: usize) -> f32 {
    values
        .get(local_col)
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
        * ARROW_EFFECT_PIXEL_SIZE
}
