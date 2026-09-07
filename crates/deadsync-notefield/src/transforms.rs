use crate::style::*;
use deadsync_gameplay::VisualEffects;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TornadoBounds {
    pub min_x: f32,
    pub max_x: f32,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TornadoLaneCache {
    base_angle: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BumpyFrameCache {
    offset: f32,
    divisor: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NoteAlphaParams {
    pub hidden: f32,
    pub hidden_offset: f32,
    pub sudden: f32,
    pub sudden_offset: f32,
    pub stealth: f32,
    pub blink: f32,
    pub random_vanish: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct VisualEffectParams {
    pub bumpy: f32,
    pub tiny: f32,
    pub pulse_inner: f32,
    pub pulse_outer: f32,
    pub pulse_offset: f32,
    pub pulse_period: f32,
    pub confusion: f32,
    pub confusion_offset: f32,
    pub dizzy: f32,
    pub rotate_z: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LaneNoteTransformCache {
    bumpy_amplitude: f32,
    tiny_zoom: f32,
    pulse_active: bool,
    pulse_constant: bool,
    pulse_inner_zoom: f32,
    pulse_outer_scale: f32,
    pulse_offset: f32,
    pulse_divisor: f32,
    identity_rotation: bool,
    static_rotation_z: Option<f32>,
    rotation_base_z: f32,
    song_beat: f32,
    dizzy: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NoteAppearanceCache {
    identity: bool,
    path: AppearancePath,
    center_line: f32,
    hidden_active: bool,
    hidden: f32,
    hidden_end: f32,
    hidden_start: f32,
    hidden_denom: f32,
    hidden_degenerate: bool,
    hidden_bounds_finite: bool,
    sudden_active: bool,
    sudden: f32,
    sudden_end: f32,
    sudden_start: f32,
    sudden_denom: f32,
    sudden_degenerate: bool,
    sudden_bounds_finite: bool,
    stealth_active: bool,
    stealth: f32,
    blink_adjust: f32,
    random_vanish_active: bool,
    random_vanish: f32,
    combined_fade_low_y: f32,
    combined_fade_high_y: f32,
    combined_fade_low_alpha: f32,
    combined_fade_high_alpha: f32,
}

#[derive(Clone, Copy, Debug)]
enum AppearancePath {
    General,
    HiddenOnly,
    SuddenOnly,
    StealthOnly,
    BlinkOnly,
    HiddenSuddenOnly,
    StealthBlinkOnly,
    HiddenStealthOnly,
    SuddenStealthOnly,
    HiddenSuddenStealthOnly,
    HiddenBlinkOnly,
    SuddenBlinkOnly,
    HiddenSuddenBlinkOnly,
    HiddenStealthBlinkOnly,
    SuddenStealthBlinkOnly,
    HiddenSuddenStealthBlinkOnly,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AccelYParams {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub expand: f32,
    pub boomerang: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AccelYCache {
    boost_height_offset: f32,
    expand_scale: f32,
    path: AccelYPath,
}

#[derive(Clone, Copy, Debug)]
enum AccelYPath {
    General,
    BoostOnly,
    BrakeOnly,
    ExpandOnly,
    WaveOnly,
    BoomerangOnly,
    BoostBrakeOnly,
    BoostBoomerangOnly,
    BrakeBoomerangOnly,
    WaveBoomerangOnly,
    BoostExpandOnly,
    BrakeExpandOnly,
    BoomerangExpandOnly,
    BoostBrakeExpandOnly,
    BoostBoomerangExpandOnly,
    BrakeBoomerangExpandOnly,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NoteXParams {
    pub screen_height: f32,
    pub flip: f32,
    pub invert: f32,
    pub tornado: f32,
    pub drunk: f32,
    pub beat: f32,
}

pub(crate) fn sm_scale(v: f32, in0: f32, in1: f32, out0: f32, out1: f32) -> f32 {
    let denom = in1 - in0;
    if denom.abs() < 1e-6 {
        return out1;
    }
    ((v - in0) / denom).mul_add(out1 - out0, out0)
}

pub(crate) fn quantize_step(v: f32, step: f32) -> f32 {
    if !v.is_finite() || !step.is_finite() || step == 0.0 {
        0.0
    } else {
        (step.mul_add(0.5, v) / step).trunc() * step
    }
}

#[must_use]
pub fn quantize_centi_i32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    (value * 100.0)
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

#[must_use]
pub fn quantize_centi_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * 100.0).round().min(f64::from(u32::MAX)) as u32
}

#[must_use]
pub fn mod_percent_key(level: f32) -> i16 {
    clamp_rounded_i16(level * 100.0)
}

#[must_use]
pub const fn clamp_rounded_i16(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

pub(crate) fn beat_factor(song_beat: f32) -> f32 {
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
        (1.0 - t).mul_add(-(1.0 - t), 1.0)
    };
    if even_beat {
        factor *= -1.0;
    }
    factor * 20.0
}

pub(crate) fn mod_divisor(value: f32) -> f32 {
    if value.abs() > 0.001 {
        value
    } else if value.is_sign_negative() {
        -0.001
    } else {
        0.001
    }
}

pub(crate) fn signed_effect_active(value: f32) -> bool {
    value.is_finite() && value.abs() > f32::EPSILON
}

pub(crate) fn accel_y_is_identity(accel: AccelYParams) -> bool {
    !(accel.boost > f32::EPSILON
        || accel.brake > f32::EPSILON
        || accel.wave > f32::EPSILON
        || accel.expand > f32::EPSILON
        || accel.boomerang > f32::EPSILON)
}

pub(crate) fn accel_y_cache(elapsed: f32, effect_height: f32, accel: AccelYParams) -> AccelYCache {
    let expand_scale = if accel.expand > f32::EPSILON {
        let seconds = elapsed.rem_euclid((std::f32::consts::PI * 2.0).max(f32::EPSILON));
        let multiplier = sm_scale(
            (seconds * EXPAND_MULTIPLIER_FREQUENCY).cos(),
            EXPAND_MULTIPLIER_SCALE_FROM_LOW,
            EXPAND_MULTIPLIER_SCALE_FROM_HIGH,
            EXPAND_MULTIPLIER_SCALE_TO_LOW,
            EXPAND_MULTIPLIER_SCALE_TO_HIGH,
        );
        sm_scale(
            accel.expand,
            EXPAND_SPEED_SCALE_FROM_LOW,
            EXPAND_SPEED_SCALE_FROM_HIGH,
            EXPAND_SPEED_SCALE_TO_LOW,
            multiplier,
        )
    } else {
        1.0
    };
    let path = match (
        accel.boost > f32::EPSILON,
        accel.brake > f32::EPSILON,
        accel.wave > f32::EPSILON,
        accel.expand > f32::EPSILON,
        accel.boomerang > f32::EPSILON,
    ) {
        (true, false, false, false, false) => AccelYPath::BoostOnly,
        (false, true, false, false, false) => AccelYPath::BrakeOnly,
        (false, false, false, true, false) => AccelYPath::ExpandOnly,
        (false, false, true, false, false) => AccelYPath::WaveOnly,
        (false, false, false, false, true) => AccelYPath::BoomerangOnly,
        (true, true, false, false, false) => AccelYPath::BoostBrakeOnly,
        (true, false, false, false, true) => AccelYPath::BoostBoomerangOnly,
        (false, true, false, false, true) => AccelYPath::BrakeBoomerangOnly,
        (false, false, true, false, true) => AccelYPath::WaveBoomerangOnly,
        (true, false, false, true, false) => AccelYPath::BoostExpandOnly,
        (false, true, false, true, false) => AccelYPath::BrakeExpandOnly,
        (false, false, false, true, true) => AccelYPath::BoomerangExpandOnly,
        (true, true, false, true, false) => AccelYPath::BoostBrakeExpandOnly,
        (true, false, false, true, true) => AccelYPath::BoostBoomerangExpandOnly,
        (false, true, false, true, true) => AccelYPath::BrakeBoomerangExpandOnly,
        _ => AccelYPath::General,
    };
    AccelYCache {
        boost_height_offset: effect_height / 1.2,
        expand_scale,
        path,
    }
}
pub(crate) fn bumpy_frame_cache(offset: f32, period: f32) -> BumpyFrameCache {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    BumpyFrameCache {
        offset,
        divisor: mod_divisor(period.mul_add(BUMPY_Z_ANGLE_DIVISOR, BUMPY_Z_ANGLE_DIVISOR)),
    }
}
pub(crate) fn apply_accel_y_with_peak_cached(
    raw_y: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
    cache: AccelYCache,
) -> (f32, bool) {
    if raw_y < 0.0 {
        return (raw_y, true);
    }
    match cache.path {
        AccelYPath::BoostOnly => {
            let new_y = raw_y * 1.5 / ((raw_y + cache.boost_height_offset) / effect_height);
            let adjust =
                (accel.boost * (new_y - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
            return (raw_y + adjust, true);
        }
        AccelYPath::BrakeOnly => {
            let scale = sm_scale(raw_y, 0.0, effect_height, 0.0, 1.0);
            let new_y = raw_y * scale;
            let adjust =
                (accel.brake * (new_y - raw_y)).clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
            return (raw_y + adjust, true);
        }
        AccelYPath::ExpandOnly => return (raw_y * cache.expand_scale, true),
        AccelYPath::WaveOnly => {
            let y = (accel.wave * WAVE_MOD_MAGNITUDE)
                .mul_add((raw_y / WAVE_MOD_HEIGHT.mul_add(1.0, 0.0)).sin(), raw_y);
            return (y, true);
        }
        AccelYPath::BoomerangOnly => {
            let before_peak = raw_y < screen_height * 0.75;
            let y = 1.5f32.mul_add(raw_y, -raw_y * raw_y / screen_height);
            return (y, before_peak);
        }
        AccelYPath::BoostBrakeOnly => {
            let boosted = raw_y * 1.5 / ((raw_y + cache.boost_height_offset) / effect_height);
            let boost_adjust =
                (accel.boost * (boosted - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
            let boosted_y = raw_y + boost_adjust;
            let scale = sm_scale(boosted_y, 0.0, effect_height, 0.0, 1.0);
            let braked = boosted_y * scale;
            let brake_adjust = (accel.brake * (braked - boosted_y))
                .clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
            return (boosted_y + brake_adjust, true);
        }
        AccelYPath::BoostBoomerangOnly => {
            let boosted = raw_y * 1.5 / ((raw_y + cache.boost_height_offset) / effect_height);
            let boost_adjust =
                (accel.boost * (boosted - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
            let y = raw_y + boost_adjust;
            let before_peak = y < screen_height * 0.75;
            let y = 1.5f32.mul_add(y, -y * y / screen_height);
            return (y, before_peak);
        }
        AccelYPath::BrakeBoomerangOnly => {
            let scale = sm_scale(raw_y, 0.0, effect_height, 0.0, 1.0);
            let braked = raw_y * scale;
            let brake_adjust =
                (accel.brake * (braked - raw_y)).clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
            let y = raw_y + brake_adjust;
            let before_peak = y < screen_height * 0.75;
            let y = 1.5f32.mul_add(y, -y * y / screen_height);
            return (y, before_peak);
        }
        AccelYPath::WaveBoomerangOnly => {
            let y = (accel.wave * WAVE_MOD_MAGNITUDE)
                .mul_add((raw_y / WAVE_MOD_HEIGHT.mul_add(1.0, 0.0)).sin(), raw_y);
            let before_peak = y < screen_height * 0.75;
            let y = 1.5f32.mul_add(y, -y * y / screen_height);
            return (y, before_peak);
        }
        AccelYPath::BoostExpandOnly => {
            let boosted = raw_y * 1.5 / ((raw_y + cache.boost_height_offset) / effect_height);
            let boost_adjust =
                (accel.boost * (boosted - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
            let y = (raw_y + boost_adjust) * cache.expand_scale;
            return (y, true);
        }
        AccelYPath::BrakeExpandOnly => {
            let scale = sm_scale(raw_y, 0.0, effect_height, 0.0, 1.0);
            let braked = raw_y * scale;
            let brake_adjust =
                (accel.brake * (braked - raw_y)).clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
            let y = (raw_y + brake_adjust) * cache.expand_scale;
            return (y, true);
        }
        AccelYPath::BoomerangExpandOnly => {
            let before_peak = raw_y < screen_height * 0.75;
            let y = 1.5f32.mul_add(raw_y, -raw_y * raw_y / screen_height) * cache.expand_scale;
            return (y, before_peak);
        }
        AccelYPath::BoostBrakeExpandOnly => {
            let boosted = raw_y * 1.5 / ((raw_y + cache.boost_height_offset) / effect_height);
            let boost_adjust =
                (accel.boost * (boosted - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
            let boosted_y = raw_y + boost_adjust;
            let scale = sm_scale(boosted_y, 0.0, effect_height, 0.0, 1.0);
            let braked = boosted_y * scale;
            let brake_adjust = (accel.brake * (braked - boosted_y))
                .clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
            let y = (boosted_y + brake_adjust) * cache.expand_scale;
            return (y, true);
        }
        AccelYPath::BoostBoomerangExpandOnly => {
            let boosted = raw_y * 1.5 / ((raw_y + cache.boost_height_offset) / effect_height);
            let boost_adjust =
                (accel.boost * (boosted - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
            let y = raw_y + boost_adjust;
            let before_peak = y < screen_height * 0.75;
            let y = 1.5f32.mul_add(y, -y * y / screen_height) * cache.expand_scale;
            return (y, before_peak);
        }
        AccelYPath::BrakeBoomerangExpandOnly => {
            let scale = sm_scale(raw_y, 0.0, effect_height, 0.0, 1.0);
            let braked = raw_y * scale;
            let brake_adjust =
                (accel.brake * (braked - raw_y)).clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
            let y = raw_y + brake_adjust;
            let before_peak = y < screen_height * 0.75;
            let y = 1.5f32.mul_add(y, -y * y / screen_height) * cache.expand_scale;
            return (y, before_peak);
        }
        AccelYPath::General => {}
    }
    apply_accel_y_general(raw_y, effect_height, screen_height, accel, cache)
}

#[inline(always)]
fn apply_accel_y_general(
    mut y: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
    cache: AccelYCache,
) -> (f32, bool) {
    if accel.boost > f32::EPSILON {
        let new_y = y * 1.5 / ((y + cache.boost_height_offset) / effect_height);
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
        y = (accel.wave * WAVE_MOD_MAGNITUDE)
            .mul_add((y / WAVE_MOD_HEIGHT.mul_add(1.0, 0.0)).sin(), y);
    }
    let mut before_boomerang_peak = true;
    if accel.boomerang > f32::EPSILON {
        let peak_at_y = screen_height * 0.75;
        before_boomerang_peak = y < peak_at_y;
        y = 1.5f32.mul_add(y, -y * y / screen_height);
    }
    if accel.expand > f32::EPSILON {
        y *= cache.expand_scale;
    }
    (y, before_boomerang_peak)
}

pub(crate) fn apply_accel_y_cached(
    raw_y: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
    cache: AccelYCache,
) -> f32 {
    apply_accel_y_with_peak_cached(raw_y, effect_height, screen_height, accel, cache).0
}
pub(crate) fn note_world_z_for_bumpy_cached(
    y: f32,
    frame_cache: BumpyFrameCache,
    lane_cache: LaneNoteTransformCache,
) -> f32 {
    if lane_cache.bumpy_amplitude == 0.0 {
        return 0.0;
    }
    let angle = 100.0f32.mul_add(frame_cache.offset, y) / frame_cache.divisor;
    lane_cache.bumpy_amplitude * angle.sin()
}

pub(crate) fn itg_actor_rotation_z(deg: f32) -> f32 {
    -deg
}

pub(crate) fn visual_hold_body_needs_z_buffer(params: VisualEffectParams) -> bool {
    signed_effect_active(params.bumpy)
}

pub(crate) fn visual_use_legacy_hold_sprites(
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

pub(crate) fn visual_tiny_zoom(params: VisualEffectParams) -> f32 {
    if !params.tiny.is_finite() || params.tiny.abs() <= f32::EPSILON {
        1.0
    } else {
        0.5_f32.powf(params.tiny)
    }
}

pub(crate) fn visual_pulse_active(params: VisualEffectParams) -> bool {
    signed_effect_active(params.pulse_inner) || signed_effect_active(params.pulse_outer)
}

pub(crate) fn visual_pulse_inner_zoom(params: VisualEffectParams) -> f32 {
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

pub(crate) fn visual_pulse_zoom_for_y(y: f32, params: VisualEffectParams) -> f32 {
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
    (100.0f32.mul_add(offset, y) / divisor)
        .sin()
        .mul_add(outer * 0.5, visual_pulse_inner_zoom(params))
}

pub(crate) fn visual_arrow_effect_zoom(y: f32, params: VisualEffectParams) -> f32 {
    visual_tiny_zoom(params) * visual_pulse_zoom_for_y(y, params)
}

pub(crate) fn lane_note_transform_cache(
    song_beat: f32,
    params: VisualEffectParams,
) -> LaneNoteTransformCache {
    let bumpy_amplitude = if signed_effect_active(params.bumpy) {
        params.bumpy * BUMPY_Z_MAGNITUDE
    } else {
        0.0
    };
    let pulse_active = visual_pulse_active(params);
    let pulse_outer = if params.pulse_outer.is_finite() {
        params.pulse_outer
    } else {
        0.0
    };
    let pulse_offset = if params.pulse_offset.is_finite() {
        params.pulse_offset
    } else {
        0.0
    };
    let pulse_period = if params.pulse_period.is_finite() {
        params.pulse_period
    } else {
        0.0
    };
    let identity_rotation = params.rotate_z == 0.0
        && params.confusion == 0.0
        && params.confusion_offset == 0.0
        && params.dizzy == 0.0;
    let rotation_base_z =
        itg_actor_rotation_z(params.rotate_z) + visual_confusion_rotation_deg(song_beat, params);
    let static_rotation_z = if !identity_rotation && params.dizzy == 0.0 && song_beat.is_finite() {
        let rotation = visual_note_rotation_z_full(song_beat, song_beat, params);
        (rotation.is_finite() && rotation != 0.0).then_some(rotation)
    } else {
        None
    };
    LaneNoteTransformCache {
        bumpy_amplitude,
        tiny_zoom: visual_tiny_zoom(params),
        pulse_active,
        pulse_constant: pulse_active && pulse_outer == 0.0,
        pulse_inner_zoom: visual_pulse_inner_zoom(params),
        pulse_outer_scale: pulse_outer * 0.5,
        pulse_offset,
        pulse_divisor: mod_divisor(0.4 * ARROW_EFFECT_PIXEL_SIZE * (1.0 + pulse_period)),
        identity_rotation,
        static_rotation_z,
        rotation_base_z,
        song_beat,
        dizzy: params.dizzy,
    }
}

pub(crate) fn visual_arrow_effect_zoom_cached(y: f32, cache: LaneNoteTransformCache) -> f32 {
    if cache.pulse_active {
        if cache.pulse_constant && y.is_finite() {
            return cache.tiny_zoom * cache.pulse_inner_zoom;
        }
        let pulse = (100.0f32.mul_add(cache.pulse_offset, y) / cache.pulse_divisor)
            .sin()
            .mul_add(cache.pulse_outer_scale, cache.pulse_inner_zoom);
        cache.tiny_zoom * pulse
    } else {
        cache.tiny_zoom
    }
}

pub(crate) fn visual_confusion_rotation_deg(song_beat: f32, params: VisualEffectParams) -> f32 {
    // ArrowEffects uses +offset and -beat*confusion in screen coordinates.
    // Flat draws rotate in Y-up world coordinates, so negate the native angle.
    let spin = (song_beat * params.confusion) % std::f32::consts::TAU;
    (spin - params.confusion_offset) * (180.0 / std::f32::consts::PI)
}

pub(crate) fn visual_dizzy_rotation_deg(
    note_beat: f32,
    song_beat: f32,
    params: VisualEffectParams,
) -> f32 {
    ((note_beat - song_beat) * params.dizzy) % std::f32::consts::TAU
        * (-180.0 / std::f32::consts::PI)
}

#[inline(always)]
fn wrap_dizzy_radians(radians: f32) -> f32 {
    if radians > -std::f32::consts::TAU && radians < std::f32::consts::TAU {
        radians
    } else {
        radians % std::f32::consts::TAU
    }
}
pub(crate) fn visual_note_rotation_z_cached(note_beat: f32, cache: LaneNoteTransformCache) -> f32 {
    if cache.identity_rotation {
        return 0.0;
    }
    if note_beat.is_finite()
        && let Some(rotation) = cache.static_rotation_z
    {
        return rotation;
    }
    let radians = (note_beat - cache.song_beat) * cache.dizzy;
    let wrapped = wrap_dizzy_radians(radians);
    cache.rotation_base_z + wrapped * (-180.0 / std::f32::consts::PI)
}

pub(crate) fn visual_hold_head_rotation_z_cached(cache: LaneNoteTransformCache) -> f32 {
    if cache.identity_rotation {
        0.0
    } else {
        cache.rotation_base_z
    }
}

#[inline(always)]
fn visual_note_rotation_z_full(note_beat: f32, song_beat: f32, params: VisualEffectParams) -> f32 {
    itg_actor_rotation_z(params.rotate_z)
        + visual_confusion_rotation_deg(song_beat, params)
        + visual_dizzy_rotation_deg(note_beat, song_beat, params)
}

pub(crate) fn visual_effect_params_for_col(
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

pub(crate) fn gameplay_visual_effect_params(
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
            rotate_z: 0.0,
        },
        local_col,
        &visual.tiny_cols,
        &visual.confusion_offset_cols,
        &visual.bumpy_cols,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_gameplay_lane_effects(
    visual: &VisualEffects,
    arrow_effect_time_s: f32,
    num_cols: usize,
    effect_params: &mut [VisualEffectParams],
    lane_offsets: &mut [f32],
    tipsy_offsets: &mut [f32],
    move_y_offsets: &mut [f32],
) {
    let columns = num_cols
        .min(effect_params.len())
        .min(lane_offsets.len())
        .min(tipsy_offsets.len())
        .min(move_y_offsets.len());
    for local_col in 0..columns {
        effect_params[local_col] = gameplay_visual_effect_params(visual, local_col);
        let tipsy = tipsy_y_extra(local_col, arrow_effect_time_s, visual.tipsy);
        let move_y = move_col_extra(&visual.move_y_cols, local_col);
        lane_offsets[local_col] = tipsy + move_y;
        tipsy_offsets[local_col] = tipsy;
        move_y_offsets[local_col] = move_y;
    }
}

pub(crate) fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

pub(crate) fn compute_invert_distances(col_offsets: &[f32], out: &mut [f32]) {
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

pub(crate) fn compute_tornado_bounds(col_offsets: &[f32], out: &mut [TornadoBounds]) {
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

pub(crate) fn compute_tornado_lane_caches(
    col_offsets: &[f32],
    bounds: &[TornadoBounds],
    tornado: f32,
    out: &mut [TornadoLaneCache],
) {
    if !signed_effect_active(tornado) {
        return;
    }
    let columns = col_offsets.len().min(bounds.len()).min(out.len());
    for local_col in 0..columns {
        let base_x = col_offsets[local_col];
        let lane_bounds = bounds[local_col];
        let position_between =
            sm_scale(base_x, lane_bounds.min_x, lane_bounds.max_x, -1.0, 1.0).clamp(-1.0, 1.0);
        out[local_col] = TornadoLaneCache {
            base_angle: position_between.acos(),
        };
    }
}

#[inline(always)]
pub(crate) fn compute_active_note_geometry(
    visual: &VisualEffects,
    col_offsets: &[f32],
    invert: &mut [f32],
    tornado: &mut [TornadoBounds],
) {
    if signed_effect_active(visual.invert) {
        compute_invert_distances(col_offsets, invert);
    }
    if signed_effect_active(visual.tornado) {
        compute_tornado_bounds(col_offsets, tornado);
    }
}

pub(crate) fn tipsy_y_extra(local_col: usize, elapsed: f32, tipsy: f32) -> f32 {
    if !signed_effect_active(tipsy) {
        return 0.0;
    }
    let col = local_col as f32;
    let angle = col.mul_add(TIPSY_COLUMN_FREQUENCY, elapsed * TIPSY_TIMER_FREQUENCY);
    tipsy * angle.cos() * ARROW_EFFECT_PIXEL_SIZE * TIPSY_ARROW_MAGNITUDE
}

pub(crate) fn beat_x_extra(y: f32, beat_factor: f32, beat: f32) -> f32 {
    if !signed_effect_active(beat) {
        return 0.0;
    }
    let shift =
        beat_factor * (y / BEAT_OFFSET_HEIGHT + std::f32::consts::PI / BEAT_PI_HEIGHT).sin();
    beat * shift
}

pub(crate) fn drunk_x_extra(
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
    let angle =
        col.mul_add(DRUNK_COLUMN_FREQUENCY, elapsed) + y * DRUNK_OFFSET_FREQUENCY / screen_height;
    drunk * angle.cos() * ARROW_EFFECT_PIXEL_SIZE * DRUNK_ARROW_MAGNITUDE
}

pub(crate) fn tornado_x_extra(
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

#[inline(always)]
fn tornado_x_extra_cached(
    y: f32,
    base_x: f32,
    bounds: TornadoBounds,
    screen_height: f32,
    tornado: f32,
    cache: TornadoLaneCache,
) -> f32 {
    let radians = cache.base_angle + y * TORNADO_X_OFFSET_FREQUENCY / screen_height;
    let adjusted = sm_scale(radians.cos(), -1.0, 1.0, bounds.min_x, bounds.max_x);
    (adjusted - base_x) * tornado
}

pub(crate) fn note_x_extra(
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
        out = (mirrored - base_x).mul_add(params.flip, out);
    }
    if signed_effect_active(params.invert) {
        out = invert
            .get(local_col)
            .copied()
            .unwrap_or(0.0)
            .mul_add(params.invert, out);
    }
    if signed_effect_active(params.beat) {
        out += beat_x_extra(y, beat_factor_value, params.beat);
    }
    out
}

pub(crate) fn note_x_offset(
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn note_x_offset_cached(
    local_col: usize,
    y: f32,
    beat_factor_value: f32,
    elapsed: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    tornado_cache: &[TornadoLaneCache],
    move_x_cache: &[f32],
    params: NoteXParams,
    tiny_scale: f32,
) -> f32 {
    let base_x = col_offsets.get(local_col).copied().unwrap_or(0.0);
    let mut extra = 0.0;
    if signed_effect_active(params.tornado) {
        let bounds = tornado.get(local_col).copied().unwrap_or_default();
        extra += tornado_cache.get(local_col).map_or_else(
            || tornado_x_extra(y, base_x, bounds, params.screen_height, params.tornado),
            |&cache| {
                tornado_x_extra_cached(
                    y,
                    base_x,
                    bounds,
                    params.screen_height,
                    params.tornado,
                    cache,
                )
            },
        );
    }
    if signed_effect_active(params.drunk) {
        extra += drunk_x_extra(local_col, y, elapsed, params.screen_height, params.drunk);
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
        extra = (mirrored - base_x).mul_add(params.flip, extra);
    }
    if signed_effect_active(params.invert) {
        extra = invert
            .get(local_col)
            .copied()
            .unwrap_or(0.0)
            .mul_add(params.invert, extra);
    }
    if signed_effect_active(params.beat) {
        extra += beat_x_extra(y, beat_factor_value, params.beat);
    }
    let base = base_x + extra;
    base * tiny_scale + move_x_cache.get(local_col).copied().unwrap_or(0.0)
}

pub(crate) fn fill_static_note_x_offsets(
    num_cols: usize,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    move_x: &[f32],
    params: NoteXParams,
    tiny_zoom: f32,
    out: &mut [f32],
) -> bool {
    if signed_effect_active(params.tornado)
        || signed_effect_active(params.drunk)
        || signed_effect_active(params.beat)
    {
        return false;
    }
    let columns = num_cols.min(out.len());
    for (local_col, offset) in out.iter_mut().take(columns).enumerate() {
        *offset = note_x_offset(
            local_col,
            0.0,
            0.0,
            0.0,
            col_offsets,
            invert,
            tornado,
            move_x,
            params,
            tiny_zoom,
        );
    }
    true
}
#[inline(always)]
pub(crate) fn appearance_note_alpha_is_identity(params: NoteAlphaParams) -> bool {
    params.hidden == 0.0
        && params.sudden == 0.0
        && params.stealth == 0.0
        && params.blink == 0.0
        && params.random_vanish == 0.0
}

#[inline(always)]
pub(crate) fn note_appearance_cache(
    elapsed: f32,
    mini: f32,
    params: NoteAlphaParams,
) -> NoteAppearanceCache {
    if appearance_note_alpha_is_identity(params) {
        return NoteAppearanceCache {
            identity: true,
            path: AppearancePath::General,
            center_line: 0.0,
            hidden_active: false,
            hidden: 0.0,
            hidden_end: 0.0,
            hidden_start: 0.0,
            hidden_denom: 0.0,
            hidden_degenerate: false,
            hidden_bounds_finite: true,
            sudden_active: false,
            sudden: 0.0,
            sudden_end: 0.0,
            sudden_start: 0.0,
            sudden_denom: 0.0,
            sudden_degenerate: false,
            sudden_bounds_finite: true,
            stealth_active: false,
            stealth: 0.0,
            blink_adjust: 0.0,
            random_vanish_active: false,
            random_vanish: 0.0,
            combined_fade_low_y: 0.0,
            combined_fade_high_y: 0.0,
            combined_fade_low_alpha: 1.0,
            combined_fade_high_alpha: 1.0,
        };
    }
    let zoom = mini.mul_add(-0.5, 1.0).abs().max(0.01);
    let center_line = CENTER_LINE_Y / zoom;
    let hidden_sudden = params.hidden * params.sudden;
    let hidden_end = FADE_DIST_Y
        .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, -1.0, -1.25), center_line)
        + center_line * params.hidden_offset;
    let hidden_start = FADE_DIST_Y
        .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 0.0, -0.25), center_line)
        + center_line * params.hidden_offset;
    let sudden_end = FADE_DIST_Y.mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 0.0, 0.25), center_line)
        + center_line * params.sudden_offset;
    let sudden_start = FADE_DIST_Y
        .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 1.0, 1.25), center_line)
        + center_line * params.sudden_offset;
    let blink_adjust = if params.blink > f32::EPSILON {
        let blink = quantize_step((elapsed * 10.0).sin(), BLINK_MOD_FREQUENCY);
        sm_scale(blink, 0.0, 1.0, -1.0, 0.0)
    } else {
        0.0
    };
    let hidden_denom = hidden_end - hidden_start;
    let sudden_denom = sudden_end - sudden_start;
    let hidden_bounds_finite =
        hidden_end.is_finite() && hidden_start.is_finite() && hidden_denom.is_finite();
    let sudden_bounds_finite =
        sudden_end.is_finite() && sudden_start.is_finite() && sudden_denom.is_finite();
    let hidden_active = params.hidden > f32::EPSILON;
    let sudden_active = params.sudden > f32::EPSILON;
    let stealth_active = params.stealth > f32::EPSILON;
    let blink_active = params.blink > f32::EPSILON;
    let random_vanish_active = params.random_vanish > f32::EPSILON;
    let mut combined_fade_low_adjust = 0.0;
    combined_fade_low_adjust = params.hidden.mul_add(-1.0, combined_fade_low_adjust);
    combined_fade_low_adjust = params.sudden.mul_add(0.0, combined_fade_low_adjust);
    combined_fade_low_adjust -= params.stealth;
    combined_fade_low_adjust += blink_adjust;
    let mut combined_fade_high_adjust = 0.0;
    combined_fade_high_adjust = params.hidden.mul_add(0.0, combined_fade_high_adjust);
    combined_fade_high_adjust = params.sudden.mul_add(-1.0, combined_fade_high_adjust);
    combined_fade_high_adjust -= params.stealth;
    combined_fade_high_adjust += blink_adjust;
    let path = match (
        hidden_active,
        sudden_active,
        stealth_active,
        blink_active,
        random_vanish_active,
    ) {
        (true, false, false, false, false) => AppearancePath::HiddenOnly,
        (false, true, false, false, false) => AppearancePath::SuddenOnly,
        (false, false, true, false, false) => AppearancePath::StealthOnly,
        (false, false, false, true, false) => AppearancePath::BlinkOnly,
        (true, true, false, false, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenOnly
        }
        (false, false, true, true, false) => AppearancePath::StealthBlinkOnly,
        (true, false, true, false, false) => AppearancePath::HiddenStealthOnly,
        (false, true, true, false, false) => AppearancePath::SuddenStealthOnly,
        (true, true, true, false, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenStealthOnly
        }
        (true, false, false, true, false) => AppearancePath::HiddenBlinkOnly,
        (false, true, false, true, false) => AppearancePath::SuddenBlinkOnly,
        (true, true, false, true, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenBlinkOnly
        }
        (true, false, true, true, false) if hidden_bounds_finite && hidden_denom.abs() >= 1e-6 => {
            AppearancePath::HiddenStealthBlinkOnly
        }
        (false, true, true, true, false) if sudden_bounds_finite && sudden_denom.abs() >= 1e-6 => {
            AppearancePath::SuddenStealthBlinkOnly
        }
        (true, true, true, true, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenStealthBlinkOnly
        }
        _ => AppearancePath::General,
    };
    NoteAppearanceCache {
        identity: false,
        path,
        center_line,
        hidden_active,
        hidden: params.hidden,
        hidden_end,
        hidden_start,
        hidden_denom,
        hidden_degenerate: hidden_denom.abs() < 1e-6,
        hidden_bounds_finite,
        sudden_active,
        sudden: params.sudden,
        sudden_end,
        sudden_start,
        sudden_denom,
        sudden_degenerate: sudden_denom.abs() < 1e-6,
        sudden_bounds_finite,
        stealth_active,
        stealth: params.stealth,
        blink_adjust,
        random_vanish_active,
        random_vanish: params.random_vanish,
        combined_fade_low_y: hidden_end.min(sudden_end),
        combined_fade_high_y: hidden_start.max(sudden_start),
        combined_fade_low_alpha: (1.0 + combined_fade_low_adjust).clamp(0.0, 1.0),
        combined_fade_high_alpha: (1.0 + combined_fade_high_adjust).clamp(0.0, 1.0),
    }
}

#[inline(always)]
pub(crate) fn appearance_note_alpha_glow_cached(y: f32, cache: &NoteAppearanceCache) -> (f32, f32) {
    let percent_visible = appearance_note_alpha_cached(y, cache);
    (
        appearance_note_actor_alpha_from_alpha(percent_visible),
        appearance_note_glow_from_alpha(percent_visible),
    )
}

#[inline(always)]
fn hidden_fade_scaled_bounded(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if cache.hidden_degenerate {
        -1.0
    } else if !cache.hidden_bounds_finite {
        ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
    } else if y <= cache.hidden_end {
        -1.0
    } else if y >= cache.hidden_start {
        0.0
    } else {
        ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
    }
}

#[inline(always)]
fn sudden_fade_scaled_bounded(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if cache.sudden_degenerate {
        0.0
    } else if !cache.sudden_bounds_finite {
        ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
    } else if y <= cache.sudden_end {
        0.0
    } else if y >= cache.sudden_start {
        -1.0
    } else {
        ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
    }
}

#[inline(always)]
fn hidden_fade_scaled_finite(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if y <= cache.hidden_end {
        -1.0
    } else if y >= cache.hidden_start {
        0.0
    } else {
        ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
    }
}

#[inline(always)]
fn sudden_fade_scaled_finite(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if y <= cache.sudden_end {
        0.0
    } else if y >= cache.sudden_start {
        -1.0
    } else {
        ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
    }
}

#[inline(always)]
pub(crate) fn appearance_note_alpha_cached(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if cache.identity || y < 0.0 {
        return 1.0;
    }
    match cache.path {
        AppearancePath::HiddenOnly => {
            let scaled = hidden_fade_scaled_bounded(y, cache);
            let visible_adjust = cache.hidden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenOnly => {
            let scaled = sudden_fade_scaled_bounded(y, cache);
            let visible_adjust = cache.sudden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::StealthOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::BlinkOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::StealthBlinkOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenStealthOnly => {
            let mut visible_adjust = 0.0;
            let scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenStealthOnly => {
            let mut visible_adjust = 0.0;
            let scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenStealthOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenBlinkOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenStealthBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = hidden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenStealthBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = sudden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenStealthBlinkOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::General => {}
    }
    appearance_note_alpha_general(y, cache)
}

#[inline(always)]
fn appearance_note_alpha_general(y: f32, cache: &NoteAppearanceCache) -> f32 {
    let mut visible_adjust = 0.0;
    if cache.hidden_active {
        let scaled = if cache.hidden_degenerate {
            -1.0
        } else {
            ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
        };
        visible_adjust = cache
            .hidden
            .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
    }
    if cache.sudden_active {
        let scaled = if cache.sudden_degenerate {
            0.0
        } else {
            ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
        };
        visible_adjust = cache
            .sudden
            .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
    }
    if cache.stealth_active {
        visible_adjust -= cache.stealth;
    }
    visible_adjust += cache.blink_adjust;
    if cache.random_vanish_active {
        let dist = (y - cache.center_line).abs();
        visible_adjust += sm_scale(dist, 80.0, 160.0, -1.0, 0.0) * cache.random_vanish;
    }
    (1.0 + visible_adjust).clamp(0.0, 1.0)
}
#[inline(always)]
pub(crate) fn appearance_note_glow_from_alpha(percent_visible: f32) -> f32 {
    sm_scale((percent_visible - 0.5).abs(), 0.0, 0.5, 1.3, 0.0).max(0.0)
}

#[inline(always)]
pub(crate) fn appearance_note_actor_alpha_from_alpha(percent_visible: f32) -> f32 {
    if percent_visible > 0.5 { 1.0 } else { 0.0 }
}

pub(crate) fn appearance_needs_rows(appearance: NoteAlphaParams) -> bool {
    appearance.hidden > f32::EPSILON
        || appearance.sudden > f32::EPSILON
        || appearance.random_vanish > f32::EPSILON
}

pub(crate) fn tiny_spacing_scale(tiny: f32) -> f32 {
    if !tiny.is_finite() || tiny.abs() <= f32::EPSILON {
        1.0
    } else {
        0.5_f32.powf(tiny).min(1.0)
    }
}

pub(crate) fn move_col_extra(values: &[f32], local_col: usize) -> f32 {
    values
        .get(local_col)
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
        * ARROW_EFFECT_PIXEL_SIZE
}

pub(crate) fn fill_move_col_extras(values: &[f32], out: &mut [f32]) {
    for (local_col, extra) in out.iter_mut().enumerate() {
        *extra = move_col_extra(values, local_col);
    }
}
