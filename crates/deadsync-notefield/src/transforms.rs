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
    hidden_start: f32,
    hidden_denom: f32,
    hidden_degenerate: bool,
    sudden_active: bool,
    sudden: f32,
    sudden_start: f32,
    sudden_denom: f32,
    sudden_degenerate: bool,
    stealth_active: bool,
    stealth: f32,
    blink_adjust: f32,
    random_vanish_active: bool,
    random_vanish: f32,
}

#[derive(Clone, Copy, Debug)]
enum AppearancePath {
    General,
    HiddenOnly,
    SuddenOnly,
    StealthOnly,
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

pub(crate) fn accel_y_cache(elapsed: f32, accel: AccelYParams) -> AccelYCache {
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
        _ => AccelYPath::General,
    };
    AccelYCache { expand_scale, path }
}

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn bumpy_angle(y: f32, offset: f32, period: f32) -> f32 {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    let divisor = mod_divisor(period.mul_add(BUMPY_Z_ANGLE_DIVISOR, BUMPY_Z_ANGLE_DIVISOR));
    100.0f32.mul_add(offset, y) / divisor
}

pub(crate) fn bumpy_frame_cache(offset: f32, period: f32) -> BumpyFrameCache {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    BumpyFrameCache {
        offset,
        divisor: mod_divisor(period.mul_add(BUMPY_Z_ANGLE_DIVISOR, BUMPY_Z_ANGLE_DIVISOR)),
    }
}

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn apply_accel_y_with_peak(
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

#[cfg(test)]
pub(crate) fn apply_accel_y(
    raw_y: f32,
    elapsed: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
) -> f32 {
    apply_accel_y_with_peak(raw_y, elapsed, effect_height, screen_height, accel).0
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
            let new_y = raw_y * 1.5 / ((raw_y + effect_height / 1.2) / effect_height);
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
            let boosted = raw_y * 1.5 / ((raw_y + effect_height / 1.2) / effect_height);
            let boost_adjust =
                (accel.boost * (boosted - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
            let boosted_y = raw_y + boost_adjust;
            let scale = sm_scale(boosted_y, 0.0, effect_height, 0.0, 1.0);
            let braked = boosted_y * scale;
            let brake_adjust = (accel.brake * (braked - boosted_y))
                .clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
            return (boosted_y + brake_adjust, true);
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

#[cfg(feature = "bench-support")]
fn apply_accel_y_with_peak_cached_reference(
    raw_y: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
    cache: AccelYCache,
) -> (f32, bool) {
    if raw_y < 0.0 {
        return (raw_y, true);
    }
    apply_accel_y_general(raw_y, effect_height, screen_height, accel, cache)
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

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn note_world_z_for_bumpy(y: f32, bumpy: f32, offset: f32, period: f32) -> f32 {
    if bumpy.abs() <= f32::EPSILON || !bumpy.is_finite() {
        return 0.0;
    }
    bumpy * BUMPY_Z_MAGNITUDE * bumpy_angle(y, offset, period).sin()
}

#[cfg(feature = "bench-support")]
pub(crate) fn note_world_z_for_bumpy_frame_cached(
    y: f32,
    bumpy: f32,
    cache: BumpyFrameCache,
) -> f32 {
    if bumpy.abs() <= f32::EPSILON || !bumpy.is_finite() {
        return 0.0;
    }
    let angle = 100.0f32.mul_add(cache.offset, y) / cache.divisor;
    bumpy * BUMPY_Z_MAGNITUDE * angle.sin()
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
        itg_actor_rotation_z(params.rotate_z) - visual_confusion_rotation_deg(song_beat, params);
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

#[cfg(feature = "bench-support")]
fn visual_arrow_effect_zoom_cached_reference(y: f32, cache: LaneNoteTransformCache) -> f32 {
    if cache.pulse_active {
        let pulse = (100.0f32.mul_add(cache.pulse_offset, y) / cache.pulse_divisor)
            .sin()
            .mul_add(cache.pulse_outer_scale, cache.pulse_inner_zoom);
        cache.tiny_zoom * pulse
    } else {
        cache.tiny_zoom
    }
}

pub(crate) fn visual_confusion_rotation_deg(song_beat: f32, params: VisualEffectParams) -> f32 {
    song_beat
        .mul_add(params.confusion, params.confusion_offset)
        .rem_euclid(std::f32::consts::TAU)
        * (-180.0 / std::f32::consts::PI)
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

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn visual_note_rotation_z(
    note_beat: f32,
    song_beat: f32,
    _is_hold_head: bool,
    params: VisualEffectParams,
) -> f32 {
    if params.rotate_z == 0.0
        && params.confusion == 0.0
        && params.confusion_offset == 0.0
        && params.dizzy == 0.0
    {
        return 0.0;
    }
    visual_note_rotation_z_full(note_beat, song_beat, params)
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

#[cfg(feature = "bench-support")]
fn visual_note_rotation_z_cached_reference(note_beat: f32, cache: LaneNoteTransformCache) -> f32 {
    if cache.identity_rotation {
        return 0.0;
    }
    if note_beat.is_finite()
        && let Some(rotation) = cache.static_rotation_z
    {
        return rotation;
    }
    cache.rotation_base_z
        + ((note_beat - cache.song_beat) * cache.dizzy) % std::f32::consts::TAU
            * (-180.0 / std::f32::consts::PI)
}

#[inline(always)]
fn visual_note_rotation_z_full(note_beat: f32, song_beat: f32, params: VisualEffectParams) -> f32 {
    itg_actor_rotation_z(params.rotate_z) - visual_confusion_rotation_deg(song_beat, params)
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

#[cfg(test)]
mod common_note_transform_tests {
    use super::*;

    #[test]
    fn identity_appearance_fast_path_matches_the_full_formula() {
        let identity = NoteAlphaParams {
            hidden_offset: f32::NAN,
            sudden_offset: f32::NAN,
            ..NoteAlphaParams::default()
        };
        for y in [0.0, 32.0, 160.0, 640.0] {
            let full = appearance_note_alpha_full(y, 12.5, 0.35, identity);
            let fast = appearance_note_alpha(y, 12.5, 0.35, identity);
            assert_eq!(fast.to_bits(), full.to_bits());
        }
        assert_eq!(
            appearance_note_alpha(-1.0, 12.5, 0.35, identity).to_bits(),
            1.0_f32.to_bits()
        );

        let active = NoteAlphaParams {
            hidden: 0.7,
            hidden_offset: 0.2,
            sudden: 0.4,
            sudden_offset: -0.1,
            stealth: 0.15,
            blink: 0.1,
            random_vanish: 0.3,
        };
        for y in [0.0, 64.0, 160.0, 320.0] {
            assert_eq!(
                appearance_note_alpha(y, 3.25, 0.2, active).to_bits(),
                appearance_note_alpha_full(y, 3.25, 0.2, active).to_bits()
            );
        }
    }

    #[test]
    fn identity_appearance_detection_preserves_alpha_and_glow_output() {
        let identities = [
            NoteAlphaParams::default(),
            NoteAlphaParams {
                hidden: -0.0,
                sudden: -0.0,
                stealth: -0.0,
                blink: -0.0,
                random_vanish: -0.0,
                hidden_offset: f32::NAN,
                sudden_offset: f32::INFINITY,
            },
        ];
        for params in identities {
            assert!(appearance_note_alpha_is_identity(params));
            for y in [-1.0, 0.0, 160.0, f32::NAN] {
                let percent = appearance_note_alpha(y, 3.25, 0.2, params);
                assert_eq!(percent.to_bits(), 1.0_f32.to_bits());
                assert_eq!(
                    appearance_note_actor_alpha_from_alpha(percent).to_bits(),
                    1.0_f32.to_bits(),
                );
                assert_eq!(
                    appearance_note_glow_from_alpha(percent).to_bits(),
                    0.0_f32.to_bits(),
                );
            }
        }

        for params in [
            NoteAlphaParams {
                hidden: f32::EPSILON,
                ..NoteAlphaParams::default()
            },
            NoteAlphaParams {
                blink: f32::NAN,
                ..NoteAlphaParams::default()
            },
            NoteAlphaParams {
                random_vanish: -0.25,
                ..NoteAlphaParams::default()
            },
        ] {
            assert!(!appearance_note_alpha_is_identity(params));
        }
    }

    #[test]
    fn cached_appearance_fades_match_reference_alpha_and_glow_behavior() {
        let cases = [
            NoteAlphaParams::default(),
            NoteAlphaParams {
                hidden: 0.7,
                hidden_offset: 0.2,
                sudden: 0.4,
                sudden_offset: -0.1,
                stealth: 0.15,
                blink: 0.1,
                random_vanish: 0.3,
            },
            NoteAlphaParams {
                hidden: f32::NAN,
                sudden: -0.25,
                ..NoteAlphaParams::default()
            },
        ];
        for params in cases {
            let cache = note_appearance_cache(3.25, 0.2, params);
            for y in [-1.0, 0.0, 64.0, 160.0, 320.0, f32::NAN] {
                let cached = appearance_note_alpha_glow_cached(y, &cache);
                assert_eq!(
                    cached.0.to_bits(),
                    appearance_note_actor_alpha(y, 3.25, 0.2, params).to_bits(),
                );
                assert_eq!(
                    cached.1.to_bits(),
                    appearance_note_glow(y, 3.25, 0.2, params).to_bits(),
                );
            }
        }
    }

    #[test]
    fn single_appearance_paths_match_reference_alpha_and_glow_behavior() {
        let cases = [
            NoteAlphaParams {
                hidden: 0.7,
                hidden_offset: 0.2,
                ..NoteAlphaParams::default()
            },
            NoteAlphaParams {
                sudden: 0.4,
                sudden_offset: -0.1,
                ..NoteAlphaParams::default()
            },
            NoteAlphaParams {
                stealth: 0.15,
                ..NoteAlphaParams::default()
            },
        ];
        for params in cases {
            for elapsed in [0.0, 0.125, 3.25, 81.75] {
                for mini in [-0.5, 0.0, 0.2, 1.5] {
                    let cache = note_appearance_cache(elapsed, mini, params);
                    for y in [
                        -64.0,
                        -0.0,
                        0.0,
                        64.0,
                        160.0,
                        320.0,
                        640.0,
                        f32::INFINITY,
                        f32::NAN,
                    ] {
                        let cached = appearance_note_alpha_glow_cached(y, &cache);
                        assert_eq!(
                            cached.0.to_bits(),
                            appearance_note_actor_alpha(y, elapsed, mini, params).to_bits(),
                        );
                        assert_eq!(
                            cached.1.to_bits(),
                            appearance_note_glow(y, elapsed, mini, params).to_bits(),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cached_expand_scale_matches_per_note_reference_math() {
        let accel = AccelYParams {
            boost: 0.35,
            brake: 0.2,
            wave: 0.4,
            expand: 0.75,
            boomerang: 0.3,
        };
        for elapsed in [0.0, 0.125, 3.25, 81.75] {
            let cache = accel_y_cache(elapsed, accel);
            for raw_y in [-64.0, 0.0, 32.0, 160.0, 384.0, 640.0] {
                let reference = apply_accel_y_with_peak(raw_y, elapsed, 480.0, 720.0, accel);
                let cached = apply_accel_y_with_peak_cached(raw_y, 480.0, 720.0, accel, cache);
                assert_eq!(cached.0.to_bits(), reference.0.to_bits());
                assert_eq!(cached.1, reference.1);
            }
        }
    }

    #[test]
    fn single_acceleration_paths_match_per_note_reference_math() {
        let cases = [
            AccelYParams {
                boost: 0.35,
                ..AccelYParams::default()
            },
            AccelYParams {
                brake: 0.2,
                ..AccelYParams::default()
            },
            AccelYParams {
                expand: 0.75,
                ..AccelYParams::default()
            },
        ];
        for accel in cases {
            for elapsed in [0.0, 0.125, 3.25, 81.75] {
                let cache = accel_y_cache(elapsed, accel);
                for raw_y in [
                    -64.0,
                    -0.0,
                    0.0,
                    32.0,
                    160.0,
                    384.0,
                    640.0,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    let reference = apply_accel_y_with_peak(raw_y, elapsed, 480.0, 720.0, accel);
                    let cached = apply_accel_y_with_peak_cached(raw_y, 480.0, 720.0, accel, cache);
                    assert_eq!(cached.0.to_bits(), reference.0.to_bits());
                    assert_eq!(cached.1, reference.1);
                }
            }
        }
    }

    #[test]
    fn wave_boomerang_and_boost_brake_paths_match_per_note_reference_math() {
        let cases = [
            AccelYParams {
                wave: 0.4,
                ..AccelYParams::default()
            },
            AccelYParams {
                boomerang: 0.3,
                ..AccelYParams::default()
            },
            AccelYParams {
                boost: 0.35,
                brake: 0.2,
                ..AccelYParams::default()
            },
        ];
        for accel in cases {
            for elapsed in [0.0, 0.125, 3.25, 81.75] {
                let cache = accel_y_cache(elapsed, accel);
                for raw_y in [
                    -64.0,
                    -0.0,
                    0.0,
                    32.0,
                    160.0,
                    384.0,
                    640.0,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    let reference = apply_accel_y_with_peak(raw_y, elapsed, 480.0, 720.0, accel);
                    let cached = apply_accel_y_with_peak_cached(raw_y, 480.0, 720.0, accel, cache);
                    assert_eq!(cached.0.to_bits(), reference.0.to_bits());
                    assert_eq!(cached.1, reference.1);
                }
            }
        }
    }

    #[test]
    fn cached_tornado_angles_match_per_note_reference_geometry() {
        let col_offsets = [-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0];
        let mut bounds = [TornadoBounds::default(); 8];
        compute_tornado_bounds(&col_offsets, &mut bounds);
        let mut caches = [TornadoLaneCache::default(); 8];
        compute_tornado_lane_caches(&col_offsets, &bounds, 0.8, &mut caches);
        let invert = [17.0, 11.0, 5.0, 2.0, -2.0, -5.0, -11.0, -17.0];
        let move_x = [0.0, 0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7];
        let mut move_x_cache = [0.0; 8];
        fill_move_col_extras(&move_x, &mut move_x_cache);
        let params = NoteXParams {
            screen_height: 720.0,
            flip: 0.25,
            invert: 0.4,
            tornado: 0.8,
            drunk: 0.2,
            beat: 0.3,
        };
        for local_col in 0..col_offsets.len() {
            for y in [-80.0, 0.0, 96.0, 320.0, 768.0] {
                let reference = note_x_offset(
                    local_col,
                    y,
                    -0.35,
                    4.25,
                    &col_offsets,
                    &invert,
                    &bounds,
                    &move_x,
                    params,
                    0.2,
                );
                let cached = note_x_offset_cached(
                    local_col,
                    y,
                    -0.35,
                    4.25,
                    &col_offsets,
                    &invert,
                    &bounds,
                    &caches,
                    &move_x_cache,
                    params,
                    tiny_spacing_scale(0.2),
                );
                assert_eq!(cached.to_bits(), reference.to_bits());
            }
        }
    }

    #[test]
    fn cached_tiny_scale_matches_per_note_reference_spacing() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let zero = [0.0; 4];
        let bounds = [TornadoBounds::default(); 4];
        let tornado_caches = [TornadoLaneCache::default(); 4];
        for tiny in [-0.5, 0.0, 0.2, 0.75, f32::NAN] {
            let tiny_scale = tiny_spacing_scale(tiny);
            for local_col in 0..col_offsets.len() {
                let reference = note_x_offset(
                    local_col,
                    96.0,
                    0.0,
                    2.5,
                    &col_offsets,
                    &zero,
                    &bounds,
                    &zero,
                    NoteXParams::default(),
                    tiny,
                );
                let cached = note_x_offset_cached(
                    local_col,
                    96.0,
                    0.0,
                    2.5,
                    &col_offsets,
                    &zero,
                    &bounds,
                    &tornado_caches,
                    &zero,
                    NoteXParams::default(),
                    tiny_scale,
                );
                assert_eq!(cached.to_bits(), reference.to_bits());
            }
        }
    }

    #[test]
    fn cached_bumpy_geometry_matches_per_note_reference_math() {
        for (offset, period) in [
            (0.0, 0.0),
            (0.35, 0.75),
            (-1.25, -0.5),
            (f32::NAN, f32::NAN),
        ] {
            let cache = bumpy_frame_cache(offset, period);
            for bumpy in [0.0, 0.6, -0.4, f32::NAN] {
                let lane_cache = lane_note_transform_cache(
                    0.0,
                    VisualEffectParams {
                        bumpy,
                        ..VisualEffectParams::default()
                    },
                );
                for y in [-128.0, 0.0, 96.0, 512.0] {
                    let reference = note_world_z_for_bumpy(y, bumpy, offset, period);
                    let cached = note_world_z_for_bumpy_cached(y, cache, lane_cache);
                    assert_eq!(cached.to_bits(), reference.to_bits());
                }
            }
        }
    }

    #[test]
    fn cached_move_offsets_match_per_note_reference_math() {
        let values = [0.0, 0.1, -0.2, f32::NAN, 0.4, -0.5];
        let mut cache = [0.0; 8];
        fill_move_col_extras(&values, &mut cache);
        for local_col in 0..cache.len() {
            assert_eq!(
                cache[local_col].to_bits(),
                move_col_extra(&values, local_col).to_bits()
            );
        }
    }

    #[test]
    fn identity_rotation_fast_path_matches_the_full_formula() {
        let identity = VisualEffectParams::default();
        for (note_beat, song_beat) in [(0.0, 0.0), (4.25, 3.5), (-1.0, 128.75)] {
            assert_eq!(
                visual_note_rotation_z(note_beat, song_beat, false, identity).to_bits(),
                visual_note_rotation_z_full(note_beat, song_beat, identity).to_bits()
            );
        }

        let active = VisualEffectParams {
            confusion: 0.4,
            confusion_offset: -0.2,
            dizzy: 0.7,
            rotate_z: 15.0,
            ..VisualEffectParams::default()
        };
        assert_eq!(
            visual_note_rotation_z(8.5, 6.25, false, active).to_bits(),
            visual_note_rotation_z_full(8.5, 6.25, active).to_bits()
        );
    }

    #[test]
    fn cached_pulse_geometry_matches_per_note_reference_math() {
        let song_beat = 6.25;
        let cases = [
            VisualEffectParams::default(),
            VisualEffectParams {
                tiny: 0.25,
                confusion_offset: 0.4,
                ..VisualEffectParams::default()
            },
            VisualEffectParams {
                tiny: 0.25,
                pulse_inner: 0.4,
                ..VisualEffectParams::default()
            },
            VisualEffectParams {
                tiny: -0.15,
                pulse_inner: 0.2,
                pulse_outer: 0.35,
                pulse_offset: -0.1,
                pulse_period: 0.4,
                confusion: 0.2,
                dizzy: 0.5,
                rotate_z: 15.0,
                ..VisualEffectParams::default()
            },
            VisualEffectParams {
                pulse_inner: f32::NAN,
                pulse_outer: 0.35,
                pulse_offset: f32::INFINITY,
                pulse_period: f32::NAN,
                ..VisualEffectParams::default()
            },
        ];
        for params in cases {
            let cache = lane_note_transform_cache(song_beat, params);
            for y in [-128.0, 0.0, 256.0, 640.0, f32::NAN] {
                assert_eq!(
                    visual_arrow_effect_zoom_cached(y, cache).to_bits(),
                    visual_arrow_effect_zoom(y, params).to_bits(),
                );
            }
        }
    }

    #[test]
    fn cached_dynamic_rotation_matches_per_note_reference_math() {
        let song_beat = 6.25;
        let cases = [
            VisualEffectParams::default(),
            VisualEffectParams {
                confusion: 0.2,
                confusion_offset: 0.4,
                rotate_z: 15.0,
                ..VisualEffectParams::default()
            },
            VisualEffectParams {
                confusion: 0.2,
                confusion_offset: -0.1,
                dizzy: 0.5,
                rotate_z: 15.0,
                ..VisualEffectParams::default()
            },
            VisualEffectParams {
                confusion: f32::NAN,
                ..VisualEffectParams::default()
            },
        ];
        for params in cases {
            let cache = lane_note_transform_cache(song_beat, params);
            for note_beat in [
                -100.0,
                -1.0,
                0.0,
                6.25,
                12.5,
                100.0,
                f32::INFINITY,
                f32::NAN,
            ] {
                assert_eq!(
                    visual_note_rotation_z_cached(note_beat, cache).to_bits(),
                    visual_note_rotation_z(note_beat, song_beat, false, params).to_bits(),
                );
            }
        }
    }

    #[test]
    fn bounded_dizzy_wrap_matches_remainder() {
        for radians in [
            -std::f32::consts::TAU * 8.0,
            -std::f32::consts::TAU,
            -1.0,
            -0.0,
            0.0,
            1.0,
            std::f32::consts::TAU,
            std::f32::consts::TAU * 8.0,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NAN,
        ] {
            assert_eq!(
                wrap_dizzy_radians(radians).to_bits(),
                (radians % std::f32::consts::TAU).to_bits(),
                "radians={radians}"
            );
        }
    }

    #[test]
    fn static_lane_x_cache_matches_canonical_placement_and_rejects_motion() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [192.0, 64.0, -64.0, -192.0];
        let tornado = [TornadoBounds {
            min_x: -96.0,
            max_x: 96.0,
        }; 4];
        let move_x = [4.0, -2.0, 3.0, -5.0];
        let params = NoteXParams {
            screen_height: 480.0,
            flip: 0.4,
            invert: 0.25,
            ..NoteXParams::default()
        };
        let mut cached = [f32::NAN; 4];
        assert!(fill_static_note_x_offsets(
            4,
            &col_offsets,
            &invert,
            &tornado,
            &move_x,
            params,
            0.15,
            &mut cached,
        ));
        for local_col in 0..4 {
            for y in [-128.0, 0.0, 256.0, 640.0] {
                let canonical = note_x_offset(
                    local_col,
                    y,
                    12.0,
                    3.5,
                    &col_offsets,
                    &invert,
                    &tornado,
                    &move_x,
                    params,
                    0.15,
                );
                assert_eq!(cached[local_col].to_bits(), canonical.to_bits());
            }
        }

        let dynamic = NoteXParams {
            drunk: 0.5,
            ..params
        };
        let before = cached;
        assert!(!fill_static_note_x_offsets(
            4,
            &col_offsets,
            &invert,
            &tornado,
            &move_x,
            dynamic,
            0.15,
            &mut cached,
        ));
        assert_eq!(cached.map(f32::to_bits), before.map(f32::to_bits));
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

#[cfg(test)]
pub(crate) fn appearance_note_alpha(
    y: f32,
    elapsed: f32,
    mini: f32,
    params: NoteAlphaParams,
) -> f32 {
    if y < 0.0 {
        return 1.0;
    }
    if appearance_note_alpha_is_identity(params) {
        return 1.0;
    }
    appearance_note_alpha_full(y, elapsed, mini, params)
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
            hidden_start: 0.0,
            hidden_denom: 0.0,
            hidden_degenerate: false,
            sudden_active: false,
            sudden: 0.0,
            sudden_start: 0.0,
            sudden_denom: 0.0,
            sudden_degenerate: false,
            stealth_active: false,
            stealth: 0.0,
            blink_adjust: 0.0,
            random_vanish_active: false,
            random_vanish: 0.0,
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
    let hidden_active = params.hidden > f32::EPSILON;
    let sudden_active = params.sudden > f32::EPSILON;
    let stealth_active = params.stealth > f32::EPSILON;
    let blink_active = params.blink > f32::EPSILON;
    let random_vanish_active = params.random_vanish > f32::EPSILON;
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
        _ => AppearancePath::General,
    };
    NoteAppearanceCache {
        identity: false,
        path,
        center_line,
        hidden_active,
        hidden: params.hidden,
        hidden_start,
        hidden_denom,
        hidden_degenerate: hidden_denom.abs() < 1e-6,
        sudden_active,
        sudden: params.sudden,
        sudden_start,
        sudden_denom,
        sudden_degenerate: sudden_denom.abs() < 1e-6,
        stealth_active,
        stealth: params.stealth,
        blink_adjust,
        random_vanish_active,
        random_vanish: params.random_vanish,
    }
}

#[inline(always)]
pub(crate) fn appearance_note_alpha_glow_cached(y: f32, cache: &NoteAppearanceCache) -> (f32, f32) {
    if cache.identity || y < 0.0 {
        return (1.0, 0.0);
    }
    let percent_visible = appearance_note_alpha_from_cache(y, cache);
    (
        appearance_note_actor_alpha_from_alpha(percent_visible),
        appearance_note_glow_from_alpha(percent_visible),
    )
}

#[inline(always)]
fn appearance_note_alpha_from_cache(y: f32, cache: &NoteAppearanceCache) -> f32 {
    match cache.path {
        AppearancePath::HiddenOnly => {
            let scaled = if cache.hidden_degenerate {
                -1.0
            } else {
                ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
            };
            let visible_adjust = cache.hidden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenOnly => {
            let scaled = if cache.sudden_degenerate {
                0.0
            } else {
                ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
            };
            let visible_adjust = cache.sudden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::StealthOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust -= cache.stealth;
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

#[cfg(feature = "bench-support")]
fn appearance_note_alpha_from_cache_reference(y: f32, cache: &NoteAppearanceCache) -> f32 {
    appearance_note_alpha_general(y, cache)
}

#[inline(always)]
#[cfg(test)]
fn appearance_note_alpha_full(y: f32, elapsed: f32, mini: f32, params: NoteAlphaParams) -> f32 {
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

    let mut visible_adjust = 0.0;
    if params.hidden > f32::EPSILON {
        visible_adjust = params.hidden.mul_add(
            sm_scale(y, hidden_start, hidden_end, 0.0, -1.0).clamp(-1.0, 0.0),
            visible_adjust,
        );
    }
    if params.sudden > f32::EPSILON {
        visible_adjust = params.sudden.mul_add(
            sm_scale(y, sudden_start, sudden_end, -1.0, 0.0).clamp(-1.0, 0.0),
            visible_adjust,
        );
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

#[cfg(test)]
pub(crate) fn appearance_note_glow(
    y: f32,
    elapsed: f32,
    mini: f32,
    params: NoteAlphaParams,
) -> f32 {
    let percent_visible = appearance_note_alpha(y, elapsed, mini, params);
    appearance_note_glow_from_alpha(percent_visible)
}

#[cfg(test)]
pub(crate) fn appearance_note_actor_alpha(
    y: f32,
    elapsed: f32,
    mini: f32,
    params: NoteAlphaParams,
) -> f32 {
    appearance_note_actor_alpha_from_alpha(appearance_note_alpha(y, elapsed, mini, params))
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod transform_cache_bench_support {
    use std::hint::black_box;

    use super::*;

    #[derive(Clone, Copy)]
    struct PreviousAppearanceCache {
        center_line: f32,
        hidden_end: f32,
        hidden_start: f32,
        sudden_end: f32,
        sudden_start: f32,
        blink_adjust: f32,
    }

    fn previous_appearance_cache(
        elapsed: f32,
        mini: f32,
        params: NoteAlphaParams,
    ) -> PreviousAppearanceCache {
        let zoom = mini.mul_add(-0.5, 1.0).abs().max(0.01);
        let center_line = CENTER_LINE_Y / zoom;
        let hidden_sudden = params.hidden * params.sudden;
        let hidden_end = FADE_DIST_Y
            .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, -1.0, -1.25), center_line)
            + center_line * params.hidden_offset;
        let hidden_start = FADE_DIST_Y
            .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 0.0, -0.25), center_line)
            + center_line * params.hidden_offset;
        let sudden_end = FADE_DIST_Y
            .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 0.0, 0.25), center_line)
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
        PreviousAppearanceCache {
            center_line,
            hidden_end,
            hidden_start,
            sudden_end,
            sudden_start,
            blink_adjust,
        }
    }

    #[inline(always)]
    fn previous_appearance_alpha(
        y: f32,
        params: NoteAlphaParams,
        cache: PreviousAppearanceCache,
    ) -> f32 {
        let mut visible_adjust = 0.0;
        if params.hidden > f32::EPSILON {
            visible_adjust = params.hidden.mul_add(
                sm_scale(y, cache.hidden_start, cache.hidden_end, 0.0, -1.0).clamp(-1.0, 0.0),
                visible_adjust,
            );
        }
        if params.sudden > f32::EPSILON {
            visible_adjust = params.sudden.mul_add(
                sm_scale(y, cache.sudden_start, cache.sudden_end, -1.0, 0.0).clamp(-1.0, 0.0),
                visible_adjust,
            );
        }
        if params.stealth > f32::EPSILON {
            visible_adjust -= params.stealth;
        }
        visible_adjust += cache.blink_adjust;
        if params.random_vanish > f32::EPSILON {
            let dist = (y - cache.center_line).abs();
            visible_adjust += sm_scale(dist, 80.0, 160.0, -1.0, 0.0) * params.random_vanish;
        }
        (1.0 + visible_adjust).clamp(0.0, 1.0)
    }

    fn appearance_path_old(evaluations: usize, params: NoteAlphaParams) -> u64 {
        let cache = black_box(note_appearance_cache(
            black_box(3.25),
            black_box(0.2),
            params,
        ));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 640) as f32);
            let alpha = appearance_note_alpha_from_cache_reference(y, black_box(&cache));
            let actor = appearance_note_actor_alpha_from_alpha(alpha);
            let glow = appearance_note_glow_from_alpha(alpha);
            checksum = checksum
                .wrapping_add(u64::from(actor.to_bits()))
                .rotate_left(11)
                ^ u64::from(glow.to_bits());
        }
        checksum
    }

    fn appearance_path_new(evaluations: usize, params: NoteAlphaParams) -> u64 {
        let cache = black_box(note_appearance_cache(
            black_box(3.25),
            black_box(0.2),
            params,
        ));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 640) as f32);
            let alpha = appearance_note_alpha_from_cache(y, black_box(&cache));
            let actor = appearance_note_actor_alpha_from_alpha(alpha);
            let glow = appearance_note_glow_from_alpha(alpha);
            checksum = checksum
                .wrapping_add(u64::from(actor.to_bits()))
                .rotate_left(11)
                ^ u64::from(glow.to_bits());
        }
        checksum
    }

    #[must_use]
    pub fn hidden_only_appearance_old(evaluations: usize) -> u64 {
        appearance_path_old(
            evaluations,
            NoteAlphaParams {
                hidden: 0.7,
                hidden_offset: 0.2,
                ..NoteAlphaParams::default()
            },
        )
    }

    #[must_use]
    pub fn hidden_only_appearance_new(evaluations: usize) -> u64 {
        appearance_path_new(
            evaluations,
            NoteAlphaParams {
                hidden: 0.7,
                hidden_offset: 0.2,
                ..NoteAlphaParams::default()
            },
        )
    }

    #[must_use]
    pub fn sudden_only_appearance_old(evaluations: usize) -> u64 {
        appearance_path_old(
            evaluations,
            NoteAlphaParams {
                sudden: 0.4,
                sudden_offset: -0.1,
                ..NoteAlphaParams::default()
            },
        )
    }

    #[must_use]
    pub fn sudden_only_appearance_new(evaluations: usize) -> u64 {
        appearance_path_new(
            evaluations,
            NoteAlphaParams {
                sudden: 0.4,
                sudden_offset: -0.1,
                ..NoteAlphaParams::default()
            },
        )
    }

    #[must_use]
    pub fn stealth_only_appearance_old(evaluations: usize) -> u64 {
        appearance_path_old(
            evaluations,
            NoteAlphaParams {
                stealth: 0.15,
                ..NoteAlphaParams::default()
            },
        )
    }

    #[must_use]
    pub fn stealth_only_appearance_new(evaluations: usize) -> u64 {
        appearance_path_new(
            evaluations,
            NoteAlphaParams {
                stealth: 0.15,
                ..NoteAlphaParams::default()
            },
        )
    }

    fn accel_path_old(evaluations: usize, accel: AccelYParams) -> u64 {
        let accel = black_box(accel);
        let cache = black_box(accel_y_cache(black_box(37.25), accel));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let raw_y = black_box((index % 1024) as f32 - 96.0);
            let (y, before_peak) = apply_accel_y_with_peak_cached_reference(
                raw_y,
                480.0,
                720.0,
                black_box(accel),
                black_box(cache),
            );
            checksum = checksum
                .wrapping_add(u64::from(y.to_bits()))
                .rotate_left(u32::from(before_peak));
        }
        checksum
    }

    fn accel_path_new(evaluations: usize, accel: AccelYParams) -> u64 {
        let accel = black_box(accel);
        let cache = black_box(accel_y_cache(black_box(37.25), accel));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let raw_y = black_box((index % 1024) as f32 - 96.0);
            let (y, before_peak) = apply_accel_y_with_peak_cached(
                raw_y,
                480.0,
                720.0,
                black_box(accel),
                black_box(cache),
            );
            checksum = checksum
                .wrapping_add(u64::from(y.to_bits()))
                .rotate_left(u32::from(before_peak));
        }
        checksum
    }

    #[must_use]
    pub fn boost_only_old(evaluations: usize) -> u64 {
        accel_path_old(
            evaluations,
            AccelYParams {
                boost: 0.35,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn boost_only_new(evaluations: usize) -> u64 {
        accel_path_new(
            evaluations,
            AccelYParams {
                boost: 0.35,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn brake_only_old(evaluations: usize) -> u64 {
        accel_path_old(
            evaluations,
            AccelYParams {
                brake: 0.2,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn brake_only_new(evaluations: usize) -> u64 {
        accel_path_new(
            evaluations,
            AccelYParams {
                brake: 0.2,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn expand_only_old(evaluations: usize) -> u64 {
        accel_path_old(
            evaluations,
            AccelYParams {
                expand: 0.75,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn expand_only_new(evaluations: usize) -> u64 {
        accel_path_new(
            evaluations,
            AccelYParams {
                expand: 0.75,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn wave_only_old(evaluations: usize) -> u64 {
        accel_path_old(
            evaluations,
            AccelYParams {
                wave: 0.4,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn wave_only_new(evaluations: usize) -> u64 {
        accel_path_new(
            evaluations,
            AccelYParams {
                wave: 0.4,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn boomerang_only_old(evaluations: usize) -> u64 {
        accel_path_old(
            evaluations,
            AccelYParams {
                boomerang: 0.3,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn boomerang_only_new(evaluations: usize) -> u64 {
        accel_path_new(
            evaluations,
            AccelYParams {
                boomerang: 0.3,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn boost_brake_old(evaluations: usize) -> u64 {
        accel_path_old(
            evaluations,
            AccelYParams {
                boost: 0.35,
                brake: 0.2,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn boost_brake_new(evaluations: usize) -> u64 {
        accel_path_new(
            evaluations,
            AccelYParams {
                boost: 0.35,
                brake: 0.2,
                ..AccelYParams::default()
            },
        )
    }

    #[must_use]
    pub fn expand_old(evaluations: usize) -> u64 {
        let accel = AccelYParams {
            boost: 0.35,
            brake: 0.2,
            wave: 0.4,
            expand: 0.75,
            boomerang: 0.3,
        };
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let raw_y = black_box((index % 1024) as f32 - 96.0);
            let (y, before_peak) =
                apply_accel_y_with_peak(raw_y, black_box(37.25), 480.0, 720.0, accel);
            checksum = checksum
                .wrapping_add(u64::from(y.to_bits()))
                .rotate_left(u32::from(before_peak));
        }
        checksum
    }

    #[must_use]
    pub fn expand_new(evaluations: usize) -> u64 {
        let accel = AccelYParams {
            boost: 0.35,
            brake: 0.2,
            wave: 0.4,
            expand: 0.75,
            boomerang: 0.3,
        };
        let cache = accel_y_cache(black_box(37.25), accel);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let raw_y = black_box((index % 1024) as f32 - 96.0);
            let (y, before_peak) =
                apply_accel_y_with_peak_cached(raw_y, 480.0, 720.0, accel, cache);
            checksum = checksum
                .wrapping_add(u64::from(y.to_bits()))
                .rotate_left(u32::from(before_peak));
        }
        checksum
    }

    #[must_use]
    pub fn appearance_old(evaluations: usize) -> u64 {
        let params = NoteAlphaParams {
            hidden: 0.7,
            hidden_offset: 0.2,
            sudden: 0.4,
            sudden_offset: -0.1,
            stealth: 0.15,
            blink: 0.1,
            random_vanish: 0.3,
        };
        let cache = previous_appearance_cache(black_box(3.25), black_box(0.2), params);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 640) as f32 - 32.0);
            let alpha = previous_appearance_alpha(y, black_box(params), black_box(cache));
            let actor = appearance_note_actor_alpha_from_alpha(alpha);
            let glow = appearance_note_glow_from_alpha(alpha);
            checksum = checksum
                .wrapping_add(u64::from(actor.to_bits()))
                .rotate_left(11)
                ^ u64::from(glow.to_bits());
        }
        checksum
    }

    #[must_use]
    pub fn appearance_new(evaluations: usize) -> u64 {
        let params = NoteAlphaParams {
            hidden: 0.7,
            hidden_offset: 0.2,
            sudden: 0.4,
            sudden_offset: -0.1,
            stealth: 0.15,
            blink: 0.1,
            random_vanish: 0.3,
        };
        let cache = note_appearance_cache(black_box(3.25), black_box(0.2), params);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 640) as f32 - 32.0);
            let alpha = appearance_note_alpha_from_cache(y, &cache);
            let actor = appearance_note_actor_alpha_from_alpha(alpha);
            let glow = appearance_note_glow_from_alpha(alpha);
            checksum = checksum
                .wrapping_add(u64::from(actor.to_bits()))
                .rotate_left(11)
                ^ u64::from(glow.to_bits());
        }
        checksum
    }

    fn pulse_params() -> VisualEffectParams {
        VisualEffectParams {
            tiny: 0.25,
            pulse_inner: 0.2,
            pulse_outer: 0.65,
            pulse_offset: -0.15,
            pulse_period: 0.4,
            ..VisualEffectParams::default()
        }
    }

    #[must_use]
    pub fn pulse_old(evaluations: usize) -> u64 {
        let params = pulse_params();
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let zoom = visual_arrow_effect_zoom(y, black_box(params));
            checksum = checksum
                .wrapping_add(u64::from(zoom.to_bits()))
                .rotate_left(9);
        }
        checksum
    }

    #[must_use]
    pub fn pulse_new(evaluations: usize) -> u64 {
        let cache = lane_note_transform_cache(black_box(17.25), black_box(pulse_params()));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let zoom = visual_arrow_effect_zoom_cached(y, cache);
            checksum = checksum
                .wrapping_add(u64::from(zoom.to_bits()))
                .rotate_left(9);
        }
        checksum
    }

    fn inner_pulse_params() -> VisualEffectParams {
        VisualEffectParams {
            tiny: 0.25,
            pulse_inner: 0.4,
            ..VisualEffectParams::default()
        }
    }

    #[must_use]
    pub fn inner_pulse_old(evaluations: usize) -> u64 {
        let cache = lane_note_transform_cache(black_box(17.25), black_box(inner_pulse_params()));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let zoom = visual_arrow_effect_zoom_cached_reference(y, cache);
            checksum = checksum
                .wrapping_add(u64::from(zoom.to_bits()))
                .rotate_left(9);
        }
        checksum
    }

    #[must_use]
    pub fn inner_pulse_new(evaluations: usize) -> u64 {
        let cache = lane_note_transform_cache(black_box(17.25), black_box(inner_pulse_params()));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let zoom = visual_arrow_effect_zoom_cached(y, cache);
            checksum = checksum
                .wrapping_add(u64::from(zoom.to_bits()))
                .rotate_left(9);
        }
        checksum
    }

    fn rotation_params() -> VisualEffectParams {
        VisualEffectParams {
            confusion: 0.35,
            confusion_offset: -0.2,
            dizzy: 0.7,
            rotate_z: 15.0,
            ..VisualEffectParams::default()
        }
    }

    #[must_use]
    pub fn rotation_old(evaluations: usize) -> u64 {
        let params = rotation_params();
        let song_beat = 17.25;
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let note_beat = black_box((index % 768) as f32 * 0.125 - 24.0);
            let rotation =
                visual_note_rotation_z(note_beat, black_box(song_beat), false, black_box(params));
            checksum = checksum
                .wrapping_add(u64::from(rotation.to_bits()))
                .rotate_left(15);
        }
        checksum
    }

    #[must_use]
    pub fn rotation_new(evaluations: usize) -> u64 {
        let cache = lane_note_transform_cache(black_box(17.25), black_box(rotation_params()));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let note_beat = black_box((index % 768) as f32 * 0.125 - 24.0);
            let rotation = visual_note_rotation_z_cached(note_beat, cache);
            checksum = checksum
                .wrapping_add(u64::from(rotation.to_bits()))
                .rotate_left(15);
        }
        checksum
    }

    #[must_use]
    pub fn bounded_dizzy_old(evaluations: usize) -> u64 {
        let song_beat = black_box(17.25_f32);
        let cache = lane_note_transform_cache(song_beat, black_box(rotation_params()));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let note_beat = black_box(song_beat + ((index & 63) as f32 - 32.0) * 0.125);
            let rotation = visual_note_rotation_z_cached_reference(note_beat, cache);
            checksum = checksum
                .wrapping_add(u64::from(rotation.to_bits()))
                .rotate_left(15);
        }
        checksum
    }

    #[must_use]
    pub fn bounded_dizzy_new(evaluations: usize) -> u64 {
        let song_beat = black_box(17.25_f32);
        let cache = lane_note_transform_cache(song_beat, black_box(rotation_params()));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let note_beat = black_box(song_beat + ((index & 63) as f32 - 32.0) * 0.125);
            let rotation = visual_note_rotation_z_cached(note_beat, cache);
            checksum = checksum
                .wrapping_add(u64::from(rotation.to_bits()))
                .rotate_left(15);
        }
        checksum
    }

    fn tornado_inputs() -> ([f32; 8], [TornadoBounds; 8]) {
        let col_offsets = [-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0];
        let mut bounds = [TornadoBounds::default(); 8];
        compute_tornado_bounds(&col_offsets, &mut bounds);
        (col_offsets, bounds)
    }

    #[must_use]
    pub fn tornado_old(evaluations: usize) -> u64 {
        let (col_offsets, bounds) = tornado_inputs();
        let zero = [0.0; 8];
        let params = NoteXParams {
            screen_height: 720.0,
            tornado: 0.8,
            ..NoteXParams::default()
        };
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let local_col = index & 7;
            let y = black_box((index % 896) as f32 - 96.0);
            let x = note_x_offset(
                local_col,
                y,
                0.0,
                0.0,
                &col_offsets,
                &zero,
                &bounds,
                &zero,
                params,
                0.0,
            );
            checksum = checksum.wrapping_add(u64::from(x.to_bits())).rotate_left(7);
        }
        checksum
    }

    #[must_use]
    pub fn tornado_new(evaluations: usize) -> u64 {
        let (col_offsets, bounds) = tornado_inputs();
        let zero = [0.0; 8];
        let mut caches = [TornadoLaneCache::default(); 8];
        compute_tornado_lane_caches(&col_offsets, &bounds, 0.8, &mut caches);
        let params = NoteXParams {
            screen_height: 720.0,
            tornado: 0.8,
            ..NoteXParams::default()
        };
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let local_col = index & 7;
            let y = black_box((index % 896) as f32 - 96.0);
            let x = note_x_offset_cached(
                local_col,
                y,
                0.0,
                0.0,
                &col_offsets,
                &zero,
                &bounds,
                &caches,
                &zero,
                params,
                tiny_spacing_scale(0.0),
            );
            checksum = checksum.wrapping_add(u64::from(x.to_bits())).rotate_left(7);
        }
        checksum
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod lane_invariant_cache_bench_support {
    use std::hint::black_box;

    use super::*;

    #[must_use]
    pub fn tiny_old(evaluations: usize) -> u64 {
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let base_x = black_box((index % 768) as f32 - 384.0);
            let x = base_x * tiny_spacing_scale(black_box(0.65));
            checksum = checksum.wrapping_add(u64::from(x.to_bits())).rotate_left(5);
        }
        checksum
    }

    #[must_use]
    pub fn tiny_new(evaluations: usize) -> u64 {
        let scale = tiny_spacing_scale(black_box(0.65));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let base_x = black_box((index % 768) as f32 - 384.0);
            let x = base_x * scale;
            checksum = checksum.wrapping_add(u64::from(x.to_bits())).rotate_left(5);
        }
        checksum
    }

    #[must_use]
    pub fn bumpy_old(evaluations: usize) -> u64 {
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let z = note_world_z_for_bumpy(y, 0.7, black_box(0.35), black_box(0.75));
            checksum = checksum
                .wrapping_add(u64::from(z.to_bits()))
                .rotate_left(11);
        }
        checksum
    }

    #[must_use]
    pub fn bumpy_new(evaluations: usize) -> u64 {
        let cache = bumpy_frame_cache(black_box(0.35), black_box(0.75));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let z = note_world_z_for_bumpy_frame_cached(y, 0.7, cache);
            checksum = checksum
                .wrapping_add(u64::from(z.to_bits()))
                .rotate_left(11);
        }
        checksum
    }

    #[must_use]
    pub fn bumpy_lane_old(evaluations: usize) -> u64 {
        let cache = bumpy_frame_cache(black_box(0.35), black_box(0.75));
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let z = note_world_z_for_bumpy_frame_cached(y, black_box(0.0), cache);
            checksum = checksum
                .wrapping_add(u64::from(z.to_bits()))
                .rotate_left(11);
        }
        checksum
    }

    #[must_use]
    pub fn bumpy_lane_new(evaluations: usize) -> u64 {
        let frame_cache = bumpy_frame_cache(black_box(0.35), black_box(0.75));
        let lane_cache = lane_note_transform_cache(
            0.0,
            black_box(VisualEffectParams {
                bumpy: 0.0,
                ..VisualEffectParams::default()
            }),
        );
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let y = black_box((index % 960) as f32 - 160.0);
            let z = note_world_z_for_bumpy_cached(y, frame_cache, lane_cache);
            checksum = checksum
                .wrapping_add(u64::from(z.to_bits()))
                .rotate_left(11);
        }
        checksum
    }

    #[must_use]
    pub fn move_old(evaluations: usize) -> u64 {
        let values = black_box([0.0, 0.1, -0.2, 0.3, f32::NAN, 0.5, -0.6, 0.7]);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let x = move_col_extra(&values, index & 7);
            checksum = checksum
                .wrapping_add(u64::from(x.to_bits()))
                .rotate_left(13);
        }
        checksum
    }

    #[must_use]
    pub fn move_new(evaluations: usize) -> u64 {
        let values = black_box([0.0, 0.1, -0.2, 0.3, f32::NAN, 0.5, -0.6, 0.7]);
        let mut cache = [0.0; 8];
        fill_move_col_extras(&values, &mut cache);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let x = cache[index & 7];
            checksum = checksum
                .wrapping_add(u64::from(x.to_bits()))
                .rotate_left(13);
        }
        checksum
    }
}
