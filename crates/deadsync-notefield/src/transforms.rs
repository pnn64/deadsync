use crate::style::*;

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
    if !v.is_finite() || !in0.is_finite() || !in1.is_finite() || (in1 - in0).abs() <= f32::EPSILON {
        return out1;
    }
    let t = ((v - in0) / (in1 - in0)).clamp(0.0, 1.0);
    out0 + (out1 - out0) * t
}

pub fn quantize_step(v: f32, step: f32) -> f32 {
    if !v.is_finite() || !step.is_finite() || step == 0.0 {
        0.0
    } else {
        (v / step).round() * step
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
    let beat = song_beat.floor();
    let frac = song_beat - beat;
    if frac > 0.25 {
        return 0.0;
    }
    (1.0 - frac * 4.0) * if beat as i32 % 2 == 0 { 1.0 } else { -1.0 }
}

pub fn mod_divisor(value: f32) -> f32 {
    if value == 0.0 {
        if value.is_sign_negative() {
            -0.001
        } else {
            0.001
        }
    } else {
        value
    }
}

pub fn bumpy_angle(y: f32, offset: f32, period: f32) -> f32 {
    (y / BUMPY_Z_ANGLE_DIVISOR)
        + offset.is_finite().then_some(offset).unwrap_or(0.0)
        + period.is_finite().then_some(period).unwrap_or(0.0)
}

pub fn apply_accel_y_with_peak(
    raw_y: f32,
    song_beat: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
) -> (f32, bool) {
    let mut y = raw_y;
    if accel.boost.is_finite() && accel.boost != 0.0 {
        y += accel.boost.clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP)
            * (raw_y / effect_height.max(1.0)).powi(2);
    }
    if accel.brake.is_finite() && accel.brake != 0.0 {
        y *= raw_y / effect_height.max(1.0);
    }
    if accel.wave.is_finite() && accel.wave != 0.0 {
        y += (raw_y / WAVE_MOD_HEIGHT).sin() * WAVE_MOD_MAGNITUDE * accel.wave;
    }
    if accel.expand.is_finite() && accel.expand != 0.0 {
        let phase = ((song_beat * EXPAND_MULTIPLIER_FREQUENCY).sin() + 1.0) * 0.5;
        let zoom = EXPAND_MULTIPLIER_SCALE_TO_LOW
            + (EXPAND_MULTIPLIER_SCALE_TO_HIGH - EXPAND_MULTIPLIER_SCALE_TO_LOW) * phase;
        y *= 1.0 + (zoom - 1.0) * accel.expand;
    }
    let mut peak_before = false;
    if accel.boomerang.is_finite() && accel.boomerang != 0.0 {
        let peak = screen_height * 0.5;
        peak_before = raw_y < peak;
        y = raw_y + (peak - raw_y).abs() * accel.boomerang;
    }
    (y, peak_before)
}

pub fn apply_accel_y(
    raw_y: f32,
    song_beat: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
) -> f32 {
    apply_accel_y_with_peak(raw_y, song_beat, effect_height, screen_height, accel).0
}

pub fn note_world_z_for_bumpy(y: f32, bumpy: f32, offset: f32, period: f32) -> f32 {
    if !bumpy.is_finite() || bumpy == 0.0 {
        return 0.0;
    }
    let magnitude = bumpy_angle(y, offset, period).sin().abs();
    let magnitude = if magnitude > 0.995 { 1.0 } else { magnitude };
    magnitude * BUMPY_Z_MAGNITUDE * bumpy.signum()
}

pub fn itg_actor_rotation_z(deg: f32) -> f32 {
    -deg
}

pub fn visual_hold_body_needs_z_buffer(params: VisualEffectParams) -> bool {
    params.bumpy.is_finite() && params.bumpy != 0.0
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
    0.5_f32.powf(params.tiny)
}

pub fn visual_pulse_active(params: VisualEffectParams) -> bool {
    params.pulse_outer != 0.0 || params.pulse_inner != 0.0
}

pub fn visual_pulse_inner_zoom(params: VisualEffectParams) -> f32 {
    (1.0 + params.pulse_inner).max(0.01)
}

pub fn visual_pulse_zoom_for_y(y: f32, params: VisualEffectParams) -> f32 {
    let phase = (y / (0.4 * ARROW_EFFECT_PIXEL_SIZE)).sin().max(0.0);
    1.0 + 0.5 * params.pulse_outer * phase
}

pub fn visual_arrow_effect_zoom(y: f32, params: VisualEffectParams) -> f32 {
    visual_tiny_zoom(params) * visual_pulse_zoom_for_y(y, params) * visual_pulse_inner_zoom(params)
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

pub fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn compute_invert_distances(col_offsets: &[f32], out: &mut [f32]) {
    if col_offsets.is_empty() {
        return;
    }
    for i in 0..out.len().min(col_offsets.len()) {
        out[i] = if i % 2 == 0 {
            col_offsets.get(i + 1).copied().unwrap_or(col_offsets[i]) - col_offsets[i]
        } else {
            col_offsets
                .get(i.wrapping_sub(1))
                .copied()
                .unwrap_or(col_offsets[i])
                - col_offsets[i]
        };
    }
}

pub fn compute_tornado_bounds(col_offsets: &[f32], out: &mut [TornadoBounds]) {
    for i in 0..out.len().min(col_offsets.len()) {
        let left = col_offsets
            .get(i.saturating_sub(2))
            .copied()
            .unwrap_or(col_offsets[0]);
        let right = col_offsets
            .get(i + 2)
            .copied()
            .unwrap_or_else(|| col_offsets[col_offsets.len() - 1]);
        out[i] = TornadoBounds {
            min_x: left.min(right),
            max_x: left.max(right),
        };
    }
}

pub fn tipsy_y_extra(local_col: usize, elapsed: f32, tipsy: f32) -> f32 {
    if tipsy == 0.0 {
        0.0
    } else {
        ((elapsed * TIPSY_TIMER_FREQUENCY) + local_col as f32 * TIPSY_COLUMN_FREQUENCY).cos()
            * ARROW_EFFECT_PIXEL_SIZE
            * TIPSY_ARROW_MAGNITUDE
            * tipsy
    }
}

pub fn beat_x_extra(y: f32, beat_factor: f32, beat: f32) -> f32 {
    if beat == 0.0 {
        0.0
    } else {
        (y / BEAT_PI_HEIGHT + std::f32::consts::FRAC_PI_2).sin() * beat_factor * beat
    }
}

pub fn drunk_x_extra(local_col: usize, y: f32, offset: f32, screen_height: f32, drunk: f32) -> f32 {
    if drunk == 0.0 {
        0.0
    } else {
        ((local_col as f32 * DRUNK_COLUMN_FREQUENCY)
            + y / screen_height.max(1.0) * DRUNK_OFFSET_FREQUENCY
            + offset)
            .cos()
            * ARROW_EFFECT_PIXEL_SIZE
            * DRUNK_ARROW_MAGNITUDE
            * drunk
    }
}

pub fn tornado_x_extra(
    y: f32,
    base_x: f32,
    bounds: TornadoBounds,
    screen_height: f32,
    tornado: f32,
) -> f32 {
    if tornado == 0.0 {
        return 0.0;
    }
    let width = (bounds.max_x - bounds.min_x).abs();
    let phase = (y / screen_height.max(1.0) * TORNADO_X_OFFSET_FREQUENCY).sin();
    let target = if phase >= 0.0 {
        bounds.max_x
    } else {
        bounds.min_x
    };
    (target - base_x) * phase.abs() * tornado + width * 0.0
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
    let flip = col_offsets.last().copied().unwrap_or(base_x)
        + col_offsets.first().copied().unwrap_or(base_x)
        - 2.0 * base_x;
    let invert_x = invert.get(local_col).copied().unwrap_or(0.0);
    let tornado_bounds = tornado.get(local_col).copied().unwrap_or_default();
    flip * params.flip
        + invert_x * params.invert
        + tornado_x_extra(
            y,
            base_x,
            tornado_bounds,
            params.screen_height,
            params.tornado,
        )
        + drunk_x_extra(local_col, y, elapsed, params.screen_height, params.drunk)
        + beat_x_extra(y, beat_factor_value, params.beat)
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

pub fn appearance_note_alpha(y: f32, elapsed: f32, _mini: f32, params: NoteAlphaParams) -> f32 {
    let mut alpha = 1.0 - params.stealth.clamp(0.0, 1.0);
    if params.hidden > 0.0 {
        alpha *= 1.0
            - smoothstep01((y - params.hidden_offset) / FADE_DIST_Y)
                * params.hidden.clamp(0.0, 1.0);
    }
    if params.sudden > 0.0 {
        alpha *=
            smoothstep01((y - CENTER_LINE_Y + params.sudden_offset * FADE_DIST_Y) / FADE_DIST_Y)
                * params.sudden.clamp(0.0, 1.0)
                + (1.0 - params.sudden.clamp(0.0, 1.0));
    }
    if params.blink != 0.0 && ((y * BLINK_MOD_FREQUENCY).sin() < 0.0) {
        alpha *= 1.0 - params.blink.clamp(0.0, 1.0);
    }
    if params.random_vanish != 0.0 && (elapsed + y).sin().abs() < params.random_vanish {
        alpha = 0.0;
    }
    alpha.clamp(0.0, 1.0)
}

pub fn appearance_note_glow(y: f32, elapsed: f32, mini: f32, params: NoteAlphaParams) -> f32 {
    if params.stealth > 0.0 {
        (params.stealth * 2.6).clamp(0.0, 1.0)
    } else {
        (1.0 - appearance_note_alpha(y, elapsed, mini, params)) * (1.0 - mini.clamp(0.0, 1.0) * 0.4)
    }
}

pub fn appearance_note_actor_alpha(
    y: f32,
    elapsed: f32,
    mini: f32,
    params: NoteAlphaParams,
) -> f32 {
    if appearance_note_alpha(y, elapsed, mini, params) <= 0.5 {
        0.0
    } else {
        1.0
    }
}

pub fn appearance_needs_rows(appearance: NoteAlphaParams) -> bool {
    appearance.hidden != 0.0 || appearance.sudden != 0.0 || appearance.random_vanish != 0.0
}

pub fn tiny_spacing_scale(tiny: f32) -> f32 {
    if !tiny.is_finite() || tiny <= 0.0 {
        1.0
    } else {
        0.5_f32.powf(tiny).clamp(0.01, 1.0)
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
