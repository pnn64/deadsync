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
pub(crate) struct NoteDepthFrameCache {
    elapsed: f32,
    screen_height: f32,
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
    pub stealth_col: f32,
    pub stealth_type: bool,
    pub stealth_past_receptors: bool,
    pub blink: f32,
    pub random_vanish: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct VisualEffectParams {
    pub tan_bumpy: f32,
    pub tan_bumpy_offset: f32,
    pub tan_bumpy_period: f32,
    pub local_col: usize,
    pub cosecant: bool,
    pub drunk_z: f32,
    pub drunk_z_offset: f32,
    pub drunk_z_speed: f32,
    pub drunk_z_period: f32,
    pub tan_drunk_z: f32,
    pub tan_drunk_z_offset: f32,
    pub tan_drunk_z_speed: f32,
    pub tan_drunk_z_period: f32,
    pub bumpy: f32,
    pub tiny: f32,
    pub pulse_inner: f32,
    pub pulse_outer: f32,
    pub pulse_offset: f32,
    pub pulse_period: f32,
    pub shrink_linear: f32,
    pub shrink_mult: f32,
    pub confusion: f32,
    pub confusion_offset: f32,
    pub confusion_x: f32,
    pub confusion_y: f32,
    pub confusion_x_offset: f32,
    pub confusion_y_offset: f32,
    pub dizzy: f32,
    pub dizzy_holds: bool,
    pub twirl: f32,
    pub parabola_z: f32,
    pub attenuate_z: f32,
    pub col_x: f32,
    pub beat_z: f32,
    pub beat_z_offset: f32,
    pub beat_z_mult: f32,
    pub beat_z_period: f32,
    pub bounce_z: f32,
    pub bounce_z_offset: f32,
    pub bounce_z_period: f32,
    pub digital_z: f32,
    pub digital_z_offset: f32,
    pub digital_z_period: f32,
    pub digital_z_steps: f32,
    pub tornado_z: f32,
    pub tornado_z_offset: f32,
    pub tornado_z_period: f32,
    pub sawtooth_z: f32,
    pub sawtooth_z_period: f32,
    pub tornado_z_bounds: TornadoBounds,
    pub square_z: f32,
    pub zigzag_z: f32,
    pub square_z_offset: f32,
    pub zigzag_z_offset: f32,
    pub square_z_period: f32,
    pub zigzag_z_period: f32,
    pub rotate_z: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LaneNoteTransformCache {
    tan_bumpy: [f32; 3],
    local_col: usize,
    cosecant: bool,
    drunk_z: DrunkWaveParams,
    tan_drunk_z: DrunkWaveParams,
    bumpy_amplitude: f32,
    parabola_z: f32,
    attenuate_z: f32,
    col_x: f32,
    beat_z: f32,
    beat_z_factor: f32,
    beat_z_period: f32,
    bounce_z: f32,
    bounce_z_offset: f32,
    bounce_z_period: f32,
    digital_z: f32,
    digital_z_offset: f32,
    digital_z_period: f32,
    digital_z_steps: f32,
    tornado_z: f32,
    tornado_z_offset: f32,
    tornado_z_period: f32,
    sawtooth_z: f32,
    sawtooth_z_period: f32,
    tornado_z_bounds: TornadoBounds,
    tornado_z_angle: f32,
    square_z: f32,
    zigzag_z: f32,
    square_z_offset: f32,
    zigzag_z_offset: f32,
    square_z_period: f32,
    zigzag_z_period: f32,
    tiny_zoom: f32,
    pulse_active: bool,
    pulse_constant: bool,
    pulse_inner_zoom: f32,
    pulse_outer_scale: f32,
    pulse_offset: f32,
    pulse_divisor: f32,
    shrink_linear: f32,
    shrink_mult: f32,
    identity_rotation: bool,
    static_rotation_z: Option<f32>,
    rotation_base_z: f32,
    pub(crate) confusion_rotation_x_deg: f32,
    pub(crate) confusion_rotation_y_deg: f32,
    song_beat: f32,
    dizzy: f32,
    dizzy_holds: bool,
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
    stealth_col: f32,
    stealth_type: bool,
    stealth_past_receptors: bool,
    blink_adjust: f32,
    random_vanish_active: bool,
    random_vanish: f32,
    combined_fade_low_y: f32,
    combined_fade_high_y: f32,
    fade_low_alpha: f32,
    fade_high_alpha: f32,
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
    HiddenOnlyUnbounded,
    SuddenOnlyUnbounded,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AccelYParams {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub wave_period: f32,
    pub parabola_y: f32,
    pub expand: f32,
    pub expand_period: f32,
    pub boomerang: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AccelYCache {
    boost_height_offset: f32,
    wave_divisor: f32,
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
    BoostBoomerangOnly,
    BrakeBoomerangOnly,
    WaveBoomerangOnly,
    BoostExpandOnly,
    BrakeExpandOnly,
    BoomerangExpandOnly,
    BoostBoomerangExpandOnly,
    BrakeBoomerangExpandOnly,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NoteXParams {
    pub bumpy_x: f32,
    pub bumpy_x_offset: f32,
    pub bumpy_x_period: f32,
    pub tan_bumpy_x: f32,
    pub tan_bumpy_x_offset: f32,
    pub tan_bumpy_x_period: f32,
    pub cosecant: bool,
    pub tan_drunk: f32,
    pub tan_drunk_offset: f32,
    pub tan_drunk_speed: f32,
    pub tan_drunk_period: f32,
    pub screen_height: f32,
    pub flip: f32,
    pub invert: f32,
    pub tornado: f32,
    pub tornado_period: f32,
    pub tornado_offset: f32,
    pub bounce: f32,
    pub sawtooth: f32,
    pub sawtooth_period: f32,
    pub bounce_period: f32,
    pub bounce_offset: f32,
    pub drunk: f32,
    pub drunk_offset: f32,
    pub drunk_speed: f32,
    pub drunk_period: f32,
    pub beat: f32,
    pub beat_period: f32,
    pub parabola_x: f32,
    pub attenuate_x: f32,
    pub square: f32,
    pub digital: f32,
    pub zigzag: f32,
    pub zigzag_offset: f32,
    pub zigzag_period: f32,
    pub square_offset: f32,
    pub digital_offset: f32,
    pub digital_steps: f32,
    pub square_period: f32,
    pub digital_period: f32,
    pub xmode: f32,
    pub player_p2: bool,
    pub double_style: bool,
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
    (value * 100.0).round() as i32
}

#[must_use]
pub fn quantize_centi_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * 100.0).round() as u32
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
    value.round() as i16
}

pub(crate) fn beat_factor(song_beat: f32, offset: f32, mult: f32) -> f32 {
    if !song_beat.is_finite() || !offset.is_finite() || !mult.is_finite() {
        return 0.0;
    }
    let accel_time = 0.2_f32;
    let total_time = 0.5_f32;
    let mut beat = (song_beat + accel_time + offset) * (mult + 1.0);
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
        let t = beat / accel_time;
        t * t
    } else {
        // Preserve the multiply-before-divide order of native SCALE.
        let t = (beat - accel_time) * -1.0 / (total_time - accel_time) + 1.0;
        1.0 - (1.0 - t) * (1.0 - t)
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
    !(accel.boost != 0.0
        || accel.brake != 0.0
        || accel.wave != 0.0
        || accel.parabola_y != 0.0
        || accel.expand != 0.0
        || accel.boomerang != 0.0)
}

pub(crate) fn accel_y_cache(seconds: f32, effect_height: f32, accel: AccelYParams) -> AccelYCache {
    let expand_scale = if accel.expand != 0.0 {
        let multiplier = sm_scale(
            (seconds * EXPAND_MULTIPLIER_FREQUENCY * (accel.expand_period + 1.0)).cos(),
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
        accel.parabola_y != 0.0,
        accel.boost != 0.0,
        accel.brake != 0.0,
        accel.wave != 0.0,
        accel.expand != 0.0,
        accel.boomerang != 0.0,
    ) {
        (false, true, false, false, false, false) => AccelYPath::BoostOnly,
        (false, false, true, false, false, false) => AccelYPath::BrakeOnly,
        (false, false, false, false, true, false) => AccelYPath::ExpandOnly,
        (false, false, false, true, false, false) => AccelYPath::WaveOnly,
        (false, false, false, false, false, true) => AccelYPath::BoomerangOnly,
        (false, true, false, false, false, true) => AccelYPath::BoostBoomerangOnly,
        (false, false, true, false, false, true) => AccelYPath::BrakeBoomerangOnly,
        (false, false, false, true, false, true) => AccelYPath::WaveBoomerangOnly,
        (false, true, false, false, true, false) => AccelYPath::BoostExpandOnly,
        (false, false, true, false, true, false) => AccelYPath::BrakeExpandOnly,
        (false, false, false, false, true, true) => AccelYPath::BoomerangExpandOnly,
        (false, true, false, false, true, true) => AccelYPath::BoostBoomerangExpandOnly,
        (false, false, true, false, true, true) => AccelYPath::BrakeBoomerangExpandOnly,
        _ => AccelYPath::General,
    };
    AccelYCache {
        boost_height_offset: effect_height / 1.2,
        wave_divisor: (accel.wave_period * WAVE_MOD_HEIGHT) + WAVE_MOD_HEIGHT,
        expand_scale,
        path,
    }
}
pub(crate) fn note_depth_frame_cache(
    offset: f32,
    period: f32,
    elapsed: f32,
    screen_height: f32,
) -> NoteDepthFrameCache {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    NoteDepthFrameCache {
        elapsed,
        screen_height,
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
                .mul_add((raw_y / cache.wave_divisor).sin(), raw_y);
            return (y, true);
        }
        AccelYPath::BoomerangOnly => {
            let before_peak = raw_y < screen_height * 0.75;
            let y = 1.5f32.mul_add(raw_y, -raw_y * raw_y / screen_height);
            return (y, before_peak);
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
                .mul_add((raw_y / cache.wave_divisor).sin(), raw_y);
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
    // Native GetYOffset adds each adjustment from the original travel,
    // then applies boomerang and speed. Sequential deformation changes the
    // result when Wave, Boost, Brake or ParabolaY are combined.
    let raw_y = y;
    let mut adjust = 0.0;
    if accel.boost != 0.0 {
        let new_y = raw_y * 1.5 / ((raw_y + cache.boost_height_offset) / effect_height);
        adjust += (accel.boost * (new_y - raw_y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
    }
    if accel.brake != 0.0 {
        let new_y = raw_y * sm_scale(raw_y, 0.0, effect_height, 0.0, 1.0);
        adjust += (accel.brake * (new_y - raw_y)).clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
    }
    if accel.wave != 0.0 {
        adjust += accel.wave * WAVE_MOD_MAGNITUDE * (raw_y / cache.wave_divisor).sin();
    }
    if accel.parabola_y != 0.0 {
        adjust += accel.parabola_y * (raw_y / ARROW_EFFECT_PIXEL_SIZE) * (raw_y / ARROW_EFFECT_PIXEL_SIZE);
    }
    y += adjust;
    let mut before_boomerang_peak = true;
    if accel.boomerang != 0.0 {
        let peak_at_y = screen_height * 0.75;
        before_boomerang_peak = y < peak_at_y;
        y = 1.5f32.mul_add(y, -y * y / screen_height);
    }
    if accel.expand != 0.0 {
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
pub(crate) fn digital_wave_offset(
    y: f32,
    amount: f32,
    offset: f32,
    period: f32,
    steps: f32,
) -> f32 {
    if amount == 0.0 || !amount.is_finite() {
        return 0.0;
    }
    // ArrowEffects::CalculateDigitalAngle/GetXPos quantizes the sine wave
    // with C++ round (half away from zero), before Tiny scales lane spacing.
    let angle = std::f32::consts::PI * (y + offset)
        / (ARROW_EFFECT_PIXEL_SIZE + period * ARROW_EFFECT_PIXEL_SIZE);
    amount * ARROW_EFFECT_PIXEL_SIZE * 0.5 * ((steps + 1.0) * angle.sin()).round() / (steps + 1.0)
}

pub(crate) fn triangle_wave_offset(y: f32, amount: f32, offset: f32, period: f32) -> f32 {
    if amount == 0.0 || !amount.is_finite() {
        return 0.0;
    }
    // ArrowEffects::GetXPos/GetZPos and RageTriangle wrap negative phases before
    // evaluating the three linear parts of the triangle wave.
    let angle = std::f32::consts::PI
        * (1.0 / (period + 1.0))
        * ((y + 100.0 * offset) / ARROW_EFFECT_PIXEL_SIZE);
    let mut phase = angle % std::f32::consts::TAU;
    if phase < 0.0 {
        phase += std::f32::consts::TAU;
    }
    let phase = f64::from(phase * (1.0 / std::f32::consts::PI));
    let wave = if phase < 0.5 {
        phase * 2.0
    } else if phase < 1.5 {
        1.0 - (phase - 0.5) * 2.0
    } else {
        -4.0 + phase * 2.0
    };
    amount * ARROW_EFFECT_PIXEL_SIZE / 2.0 * wave as f32
}

// ArrowEffects::GetXPos/GetZPos and RageMath::RageSquare. The 0.01
// transition prevents hold flicker at the receptor; negative fmod results
// must be corrected before deciding the sign. A period of -1 deliberately
// preserves native IEEE division: its non-finite phase selects +1.
pub(crate) fn square_wave_offset(y: f32, amount: f32, offset: f32, period: f32) -> f32 {
    if amount == 0.0 || !amount.is_finite() {
        return 0.0;
    }
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    let angle = std::f32::consts::PI * (y + offset)
        / (ARROW_EFFECT_PIXEL_SIZE + period * ARROW_EFFECT_PIXEL_SIZE);
    let mut phase = angle % std::f32::consts::TAU;
    if phase < 0.01 {
        phase += std::f32::consts::TAU;
    }
    amount
        * ARROW_EFFECT_PIXEL_SIZE
        * 0.5
        * if phase >= std::f32::consts::PI {
            -1.0
        } else {
            1.0
        }
}

pub(crate) fn note_world_z_cached(
    y: f32,
    frame_cache: NoteDepthFrameCache,
    lane_cache: LaneNoteTransformCache,
) -> f32 {
    // Keep native GetZPos addition order. Tiny affects X and zoom, never Z.
    let mut z = 0.0;
    if lane_cache.tornado_z != 0.0 {
        let bounds = lane_cache.tornado_z_bounds;
        let radians = lane_cache.tornado_z_angle
            + (y + lane_cache.tornado_z_offset) * (lane_cache.tornado_z_period * 6.0 + 6.0)
                / frame_cache.screen_height;
        let adjusted = (radians.cos() + 1.0) * (bounds.max_x - bounds.min_x) / 2.0 + bounds.min_x;
        z += (adjusted - lane_cache.col_x) * lane_cache.tornado_z;
    }
    if lane_cache.bumpy_amplitude != 0.0 {
        let angle = 100.0f32.mul_add(frame_cache.offset, y) / frame_cache.divisor;
        z += lane_cache.bumpy_amplitude * angle.sin();
    }
    z += bumpy_wave_offset(y, lane_cache.tan_bumpy, true, lane_cache.cosecant);
    z += triangle_wave_offset(
        y,
        lane_cache.zigzag_z,
        lane_cache.zigzag_z_offset,
        lane_cache.zigzag_z_period,
    );
    z += sawtooth_wave_offset(y, lane_cache.sawtooth_z, lane_cache.sawtooth_z_period);
    if lane_cache.parabola_z != 0.0 {
        z += lane_cache.parabola_z * (y / ARROW_EFFECT_PIXEL_SIZE) * (y / ARROW_EFFECT_PIXEL_SIZE);
    }
    z += attenuate_offset(y, lane_cache.col_x, lane_cache.attenuate_z);
    z += drunk_wave_offset(
        lane_cache.local_col,
        y,
        frame_cache.elapsed,
        frame_cache.screen_height,
        lane_cache.drunk_z,
        false,
        false,
    );
    z += drunk_wave_offset(
        lane_cache.local_col,
        y,
        frame_cache.elapsed,
        frame_cache.screen_height,
        lane_cache.tan_drunk_z,
        true,
        lane_cache.cosecant,
    );
    z += beat_wave_offset(
        y,
        lane_cache.beat_z_factor,
        lane_cache.beat_z,
        lane_cache.beat_z_period,
    );
    z += digital_wave_offset(
        y,
        lane_cache.digital_z,
        lane_cache.digital_z_offset,
        lane_cache.digital_z_period,
        lane_cache.digital_z_steps,
    );
    z += square_wave_offset(
        y,
        lane_cache.square_z,
        lane_cache.square_z_offset,
        lane_cache.square_z_period,
    );
    z += bounce_wave_offset(
        y,
        [
            lane_cache.bounce_z,
            lane_cache.bounce_z_offset,
            lane_cache.bounce_z_period,
        ],
    );
    z
}

// GetXPos and GetZPos use floor, so negative travel wraps toward positive one.
fn sawtooth_wave_offset(y: f32, amount: f32, period: f32) -> f32 {
    if amount == 0.0 || !amount.is_finite() {
        return 0.0;
    }
    let phase = (0.5 / (period + 1.0) * y) / ARROW_EFFECT_PIXEL_SIZE;
    amount * ARROW_EFFECT_PIXEL_SIZE * (phase - phase.floor())
}

pub(crate) fn itg_actor_rotation_z(deg: f32) -> f32 {
    -deg
}

// Native receptor rotation adds the radian offset, then the visible-beat spin.
fn confusion_rotation_deg(beat: f32, strength: f32, offset: f32) -> f32 {
    let base = if offset.is_finite() {
        offset * 180.0 / std::f32::consts::PI
    } else {
        0.0
    };
    let spin = if beat.is_finite() && strength.is_finite() {
        (beat * strength) % std::f32::consts::TAU
    } else {
        0.0
    };
    base + spin * (-180.0 / std::f32::consts::PI)
}

// Roll uses travel before Reverse, Tipsy, and MoveY and excludes hold caps.
pub(crate) fn visual_note_rotation_x(y_offset: f32, roll: f32) -> f32 {
    if roll == 0.0 || !roll.is_finite() {
        0.0
    } else {
        roll * y_offset / 2.0
    }
}

// ArrowEffects::GetRotationY uses the pre-reverse, post-acceleration Y offset.
pub(crate) fn visual_note_rotation_y(y_offset: f32, twirl: f32) -> f32 {
    if twirl == 0.0 || !twirl.is_finite() {
        0.0
    } else {
        twirl * y_offset / 2.0
    }
}

pub(crate) fn visual_hold_body_needs_z_buffer(params: VisualEffectParams) -> bool {
    signed_effect_active(params.bumpy)
        || (params.twirl.is_finite() && params.twirl != 0.0)
        || (params.parabola_z.is_finite() && params.parabola_z != 0.0)
        || (params.attenuate_z.is_finite() && params.attenuate_z != 0.0)
        || (params.beat_z.is_finite() && params.beat_z != 0.0)
        || (params.zigzag_z.is_finite() && params.zigzag_z != 0.0)
        || (params.square_z.is_finite() && params.square_z != 0.0)
        || (params.bounce_z.is_finite() && params.bounce_z != 0.0)
        || (params.digital_z.is_finite() && params.digital_z != 0.0)
        || (params.sawtooth_z.is_finite() && params.sawtooth_z != 0.0)
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
        .all(|v| v.abs() <= f32::EPSILON)
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

#[cfg(test)]
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

#[cfg(test)]
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
        tan_bumpy: [
            params.tan_bumpy,
            params.tan_bumpy_offset,
            params.tan_bumpy_period,
        ],
        local_col: params.local_col,
        cosecant: params.cosecant,
        drunk_z: DrunkWaveParams {
            amount: params.drunk_z,
            offset: params.drunk_z_offset,
            speed: params.drunk_z_speed,
            period: params.drunk_z_period,
        },
        tan_drunk_z: DrunkWaveParams {
            amount: params.tan_drunk_z,
            offset: params.tan_drunk_z_offset,
            speed: params.tan_drunk_z_speed,
            period: params.tan_drunk_z_period,
        },
        bumpy_amplitude,
        parabola_z: if params.parabola_z.is_finite() {
            params.parabola_z
        } else {
            0.0
        },
        square_z: params.square_z,
        attenuate_z: params.attenuate_z,
        col_x: params.col_x,
        beat_z: params.beat_z,
        beat_z_factor: beat_factor(song_beat, params.beat_z_offset, params.beat_z_mult),
        beat_z_period: params.beat_z_period,
        bounce_z: params.bounce_z,
        bounce_z_offset: params.bounce_z_offset,
        bounce_z_period: params.bounce_z_period,
        digital_z: params.digital_z,
        digital_z_offset: params.digital_z_offset,
        digital_z_period: params.digital_z_period,
        digital_z_steps: params.digital_z_steps,
        tornado_z: params.tornado_z,
        tornado_z_offset: params.tornado_z_offset,
        tornado_z_period: params.tornado_z_period,
        sawtooth_z: params.sawtooth_z,
        sawtooth_z_period: params.sawtooth_z_period,
        tornado_z_bounds: params.tornado_z_bounds,
        tornado_z_angle: if params.tornado_z != 0.0 {
            let bounds = params.tornado_z_bounds;
            ((params.col_x - bounds.min_x) * 2.0 / (bounds.max_x - bounds.min_x) - 1.0).acos()
        } else {
            0.0
        },
        zigzag_z: params.zigzag_z,
        square_z_offset: params.square_z_offset,
        zigzag_z_offset: params.zigzag_z_offset,
        square_z_period: params.square_z_period,
        zigzag_z_period: params.zigzag_z_period,
        tiny_zoom: visual_tiny_zoom(params),
        pulse_active,
        pulse_constant: pulse_active && pulse_outer == 0.0,
        pulse_inner_zoom: visual_pulse_inner_zoom(params),
        pulse_outer_scale: pulse_outer * 0.5,
        pulse_offset,
        pulse_divisor: mod_divisor(0.4 * ARROW_EFFECT_PIXEL_SIZE * (1.0 + pulse_period)),
        shrink_linear: params.shrink_linear,
        shrink_mult: params.shrink_mult,
        identity_rotation,
        static_rotation_z,
        rotation_base_z,
        confusion_rotation_x_deg: confusion_rotation_deg(
            song_beat,
            params.confusion_x,
            params.confusion_x_offset,
        ),
        confusion_rotation_y_deg: confusion_rotation_deg(
            song_beat,
            params.confusion_y,
            params.confusion_y_offset,
        ),
        song_beat,
        dizzy: params.dizzy,
        dizzy_holds: params.dizzy_holds,
    }
}

pub(crate) fn visual_arrow_effect_zoom_cached(
    y: f32,
    cache: LaneNoteTransformCache,
    field_zoom: f32,
) -> f32 {
    // Native GetZoom starts with field zoom, then Pulse, ShrinkMult,
    // ShrinkLinear and Tiny. Linear is additive and may produce signed zoom.
    let mut zoom = field_zoom;
    if cache.pulse_active {
        let pulse = if cache.pulse_constant && y.is_finite() {
            cache.pulse_inner_zoom
        } else {
            ((y + 100.0 * cache.pulse_offset) / cache.pulse_divisor).sin() * cache.pulse_outer_scale
                + cache.pulse_inner_zoom
        };
        zoom *= pulse;
    }
    if y >= 0.0 {
        if cache.shrink_mult != 0.0 {
            zoom *= 1.0 / (1.0 + y * (cache.shrink_mult / 100.0));
        }
        if cache.shrink_linear != 0.0 {
            zoom += y * (0.5 * cache.shrink_linear / ARROW_EFFECT_PIXEL_SIZE);
        }
    }
    zoom * cache.tiny_zoom
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

pub(crate) fn visual_hold_head_rotation_z_cached(
    note_beat: f32,
    cache: LaneNoteTransformCache,
) -> f32 {
    if cache.dizzy_holds {
        visual_note_rotation_z_cached(note_beat, cache)
    } else if cache.identity_rotation {
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
            shrink_linear: visual.shrink_linear,
            shrink_mult: visual.shrink_mult,
            confusion: visual.confusion,
            confusion_x: visual.confusion_x,
            confusion_y: visual.confusion_y,
            confusion_x_offset: visual.confusion_x_offset,
            confusion_y_offset: visual.confusion_y_offset,
            confusion_offset: visual.confusion_offset,
            dizzy: visual.dizzy,
            dizzy_holds: visual.dizzy_holds,
            twirl: visual.twirl,
            parabola_z: visual.parabola_z,
            attenuate_z: visual.attenuate_z,
            beat_z: visual.beat_z,
            beat_z_offset: visual.beat_z_offset,
            beat_z_mult: visual.beat_z_mult,
            beat_z_period: visual.beat_z_period,
            bounce_z: visual.bounce_z,
            bounce_z_offset: visual.bounce_z_offset,
            bounce_z_period: visual.bounce_z_period,
            digital_z: visual.digital_z,
            digital_z_offset: visual.digital_z_offset,
            digital_z_period: visual.digital_z_period,
            digital_z_steps: visual.digital_z_steps,
            tornado_z: visual.tornado_z,
            tornado_z_offset: visual.tornado_z_offset,
            tornado_z_period: visual.tornado_z_period,
            sawtooth_z: visual.sawtooth_z,
            sawtooth_z_period: visual.sawtooth_z_period,
            tornado_z_bounds: TornadoBounds::default(),
            local_col,
            col_x: 0.0,
            cosecant: visual.cosecant,
            drunk_z: visual.drunk_z,
            drunk_z_offset: visual.drunk_z_offset,
            drunk_z_speed: visual.drunk_z_speed,
            drunk_z_period: visual.drunk_z_period,
            tan_drunk_z: visual.tan_drunk_z,
            tan_drunk_z_offset: visual.tan_drunk_z_offset,
            tan_drunk_z_speed: visual.tan_drunk_z_speed,
            tan_drunk_z_period: visual.tan_drunk_z_period,
            square_z: visual.square_z,
            zigzag_z: visual.zigzag_z,
            square_z_offset: visual.square_z_offset,
            zigzag_z_offset: visual.zigzag_z_offset,
            square_z_period: visual.square_z_period,
            zigzag_z_period: visual.zigzag_z_period,
            bumpy: visual.bumpy,
            tan_bumpy: visual.tan_bumpy,
            tan_bumpy_offset: visual.tan_bumpy_offset,
            tan_bumpy_period: visual.tan_bumpy_period,
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
        let tipsy = tipsy_y_extra(
            local_col,
            arrow_effect_time_s,
            visual.tipsy,
            visual.tipsy_offset,
            visual.tipsy_speed,
        );
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

pub(crate) fn compute_tornado_z_bounds(col_offsets: &[f32], out: &mut [TornadoBounds]) {
    for (col, bounds) in out.iter_mut().take(col_offsets.len()).enumerate() {
        let start = col.saturating_sub(3);
        let end = (col + 3).min(col_offsets.len() - 1);
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN_POSITIVE;
        for &x in &col_offsets[start..=end] {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
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

pub(crate) fn tipsy_y_extra(
    local_col: usize,
    elapsed: f32,
    tipsy: f32,
    offset: f32,
    speed: f32,
) -> f32 {
    if !signed_effect_active(tipsy) {
        return 0.0;
    }
    let col = local_col as f32;
    // ArrowEffects::UpdateTipsy: offset scales column phase, speed scales time.
    let angle = elapsed * (speed * TIPSY_TIMER_FREQUENCY + TIPSY_TIMER_FREQUENCY)
        + col * (offset * TIPSY_COLUMN_FREQUENCY + TIPSY_COLUMN_FREQUENCY);
    tipsy * angle.cos() * ARROW_EFFECT_PIXEL_SIZE * TIPSY_ARROW_MAGNITUDE
}

pub(crate) fn beat_wave_offset(y: f32, beat_factor: f32, beat: f32, period: f32) -> f32 {
    if beat == 0.0 || !beat.is_finite() {
        return 0.0;
    }
    let shift = beat_factor
        * (y / (period * BEAT_OFFSET_HEIGHT + BEAT_OFFSET_HEIGHT)
            + std::f32::consts::PI / BEAT_PI_HEIGHT)
            .sin();
    beat * shift
}

#[derive(Clone, Copy, Debug, Default)]
struct DrunkWaveParams {
    amount: f32,
    offset: f32,
    speed: f32,
    period: f32,
}

// ArrowEffects::CalculateDrunkAngle/GetXPos/GetZPos. Both axes use the
// fallback metrics 0.2, 10 and 0.5; travel precedes Reverse/Tipsy/MoveY.
// Keep native IEEE behavior at tangent/cosecant poles; no amplitude clamp.
fn drunk_wave_offset(
    local_col: usize,
    y: f32,
    elapsed: f32,
    screen_height: f32,
    params: DrunkWaveParams,
    tangent: bool,
    cosecant: bool,
) -> f32 {
    if params.amount == 0.0 || !params.amount.is_finite() {
        return 0.0;
    }
    let angle = elapsed * (1.0 + params.speed)
        + local_col as f32 * (params.offset * DRUNK_COLUMN_FREQUENCY + DRUNK_COLUMN_FREQUENCY)
        + y * (params.period * DRUNK_OFFSET_FREQUENCY + DRUNK_OFFSET_FREQUENCY) / screen_height;
    let wave = if !tangent {
        angle.cos()
    } else if cosecant {
        1.0 / angle.sin()
    } else {
        angle.tan()
    };
    params.amount * (wave * ARROW_EFFECT_PIXEL_SIZE * DRUNK_ARROW_MAGNITUDE)
}

// ArrowEffects::CalculateBumpyAngle/GetXPos/GetZPos. Horizontal Bumpy
// precedes Tiny spacing; tangent depth remains unscaled. Preserve native
// IEEE division at period -1 and tangent/cosecant poles.
fn bumpy_wave_offset(y: f32, params: [f32; 3], tangent: bool, cosecant: bool) -> f32 {
    let [amount, offset, period] = params;
    if amount == 0.0 || !amount.is_finite() {
        return 0.0;
    }
    let angle = (y + 100.0 * offset) / (period * 16.0 + 16.0);
    let wave = if !tangent {
        angle.sin()
    } else if cosecant {
        1.0 / angle.sin()
    } else {
        angle.tan()
    };
    amount * 40.0 * wave
}

pub(crate) fn drunk_x_extra(local_col: usize, y: f32, elapsed: f32, params: NoteXParams) -> f32 {
    drunk_wave_offset(
        local_col,
        y,
        elapsed,
        params.screen_height,
        DrunkWaveParams {
            amount: params.drunk,
            offset: params.drunk_offset,
            speed: params.drunk_speed,
            period: params.drunk_period,
        },
        false,
        false,
    )
}

fn tan_drunk_x_extra(local_col: usize, y: f32, elapsed: f32, params: NoteXParams) -> f32 {
    drunk_wave_offset(
        local_col,
        y,
        elapsed,
        params.screen_height,
        DrunkWaveParams {
            amount: params.tan_drunk,
            offset: params.tan_drunk_offset,
            speed: params.tan_drunk_speed,
            period: params.tan_drunk_period,
        },
        true,
        params.cosecant,
    )
}

pub(crate) fn tornado_x_extra(
    y: f32,
    base_x: f32,
    bounds: TornadoBounds,
    screen_height: f32,
    [tornado, offset, period]: [f32; 3],
) -> f32 {
    if !signed_effect_active(tornado) {
        return 0.0;
    }
    let position_between = sm_scale(base_x, bounds.min_x, bounds.max_x, -1.0, 1.0).clamp(-1.0, 1.0);
    let radians = position_between.acos()
        + (y + offset) * (period * TORNADO_X_OFFSET_FREQUENCY + TORNADO_X_OFFSET_FREQUENCY)
            / screen_height;
    let adjusted = sm_scale(radians.cos(), -1.0, 1.0, bounds.min_x, bounds.max_x);
    (adjusted - base_x) * tornado
}

#[inline(always)]
fn tornado_x_extra_cached(
    y: f32,
    base_x: f32,
    bounds: TornadoBounds,
    screen_height: f32,
    [tornado, offset, period]: [f32; 3],
    cache: TornadoLaneCache,
) -> f32 {
    let radians = cache.base_angle
        + (y + offset) * (period * TORNADO_X_OFFSET_FREQUENCY + TORNADO_X_OFFSET_FREQUENCY)
            / screen_height;
    let adjusted = sm_scale(radians.cos(), -1.0, 1.0, bounds.min_x, bounds.max_x);
    (adjusted - base_x) * tornado
}

// ArrowEffects::GetXPos/GetZPos use std::sin (not RageFastSin) and a 60px period.
// Preserve IEEE behavior when the native period denominator is zero.
fn bounce_wave_offset(y: f32, [amount, offset, period]: [f32; 3]) -> f32 {
    if amount == 0.0 || !amount.is_finite() {
        return 0.0;
    }
    let wave = ((y + offset) / (60.0 + period * 60.0)).sin().abs();
    amount * ARROW_EFFECT_PIXEL_SIZE * 0.5 * wave
}

// ArrowEffects::GetXPos: doubles split at floor(columns / 2); singles
// use the native player number, including a lone P2 field.
fn xmode_x_extra(local_col: usize, y: f32, num_cols: usize, params: NoteXParams) -> f32 {
    if !params.xmode.is_finite() || params.xmode == 0.0 {
        return 0.0;
    }
    let negative = if params.double_style {
        local_col >= num_cols / 2
    } else {
        params.player_p2
    };
    params.xmode * if negative { -y } else { y }
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
            [params.tornado, params.tornado_offset, params.tornado_period],
        );
    }
    out += bumpy_wave_offset(
        y,
        [params.bumpy_x, params.bumpy_x_offset, params.bumpy_x_period],
        false,
        false,
    );
    out += bumpy_wave_offset(
        y,
        [
            params.tan_bumpy_x,
            params.tan_bumpy_x_offset,
            params.tan_bumpy_x_period,
        ],
        true,
        params.cosecant,
    );
    if params.drunk != 0.0 {
        out += drunk_x_extra(local_col, y, elapsed, params);
    }
    if params.tan_drunk != 0.0 {
        out += tan_drunk_x_extra(local_col, y, elapsed, params);
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
    if (params.beat.is_finite() && params.beat != 0.0) {
        out += beat_wave_offset(y, beat_factor_value, params.beat, params.beat_period);
    }
    out += triangle_wave_offset(
        y, params.zigzag, params.zigzag_offset, params.zigzag_period,
    );
    // ArrowEffects::GetXPos adds the squared travel offset before Tiny spacing.
    out += sawtooth_wave_offset(y, params.sawtooth, params.sawtooth_period);
    if params.parabola_x.is_finite() && params.parabola_x != 0.0 {
        out += params.parabola_x * (y / ARROW_EFFECT_PIXEL_SIZE) * (y / ARROW_EFFECT_PIXEL_SIZE);
    }
    out += attenuate_offset(y, base_x, params.attenuate_x);
    out += digital_wave_offset(
        y,
        params.digital,
        params.digital_offset,
        params.digital_period,
        params.digital_steps,
    );
    out += square_wave_offset(y, params.square, params.square_offset, params.square_period);
    out += bounce_wave_offset(
        y,
        [params.bounce, params.bounce_offset, params.bounce_period],
    );
    out += xmode_x_extra(local_col, y, col_offsets.len(), params);
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
            || {
                tornado_x_extra(
                    y,
                    base_x,
                    bounds,
                    params.screen_height,
                    [params.tornado, params.tornado_offset, params.tornado_period],
                )
            },
            |&cache| {
                tornado_x_extra_cached(
                    y,
                    base_x,
                    bounds,
                    params.screen_height,
                    [params.tornado, params.tornado_offset, params.tornado_period],
                    cache,
                )
            },
        );
    }
    extra += bumpy_wave_offset(
        y,
        [params.bumpy_x, params.bumpy_x_offset, params.bumpy_x_period],
        false,
        false,
    );
    extra += bumpy_wave_offset(
        y,
        [
            params.tan_bumpy_x,
            params.tan_bumpy_x_offset,
            params.tan_bumpy_x_period,
        ],
        true,
        params.cosecant,
    );
    if params.drunk != 0.0 {
        extra += drunk_x_extra(local_col, y, elapsed, params);
    }
    if params.tan_drunk != 0.0 {
        extra += tan_drunk_x_extra(local_col, y, elapsed, params);
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
    if (params.beat.is_finite() && params.beat != 0.0) {
        extra += beat_wave_offset(y, beat_factor_value, params.beat, params.beat_period);
    }
    extra += triangle_wave_offset(
        y, params.zigzag, params.zigzag_offset, params.zigzag_period,
    );
    // ArrowEffects::GetXPos adds the squared travel offset before Tiny spacing.
    extra += sawtooth_wave_offset(y, params.sawtooth, params.sawtooth_period);
    if params.parabola_x.is_finite() && params.parabola_x != 0.0 {
        extra += params.parabola_x * (y / ARROW_EFFECT_PIXEL_SIZE) * (y / ARROW_EFFECT_PIXEL_SIZE);
    }
    extra += attenuate_offset(y, base_x, params.attenuate_x);
    extra += digital_wave_offset(
        y,
        params.digital,
        params.digital_offset,
        params.digital_period,
        params.digital_steps,
    );
    extra += square_wave_offset(y, params.square, params.square_offset, params.square_period);
    extra += bounce_wave_offset(
        y,
        [params.bounce, params.bounce_offset, params.bounce_period],
    );
    extra += xmode_x_extra(local_col, y, col_offsets.len(), params);
    let base = base_x + extra;
    base * tiny_scale + move_x_cache.get(local_col).copied().unwrap_or(0.0)
}

pub(crate) fn fill_static_note_x_offsets(
    num_cols: usize,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    move_x_offsets: &[f32],
    params: NoteXParams,
    tiny_scale: f32,
    out: &mut [f32],
) -> bool {
    if (params.sawtooth.is_finite() && params.sawtooth != 0.0)
        || signed_effect_active(params.tornado)
        || (params.bounce.is_finite() && params.bounce != 0.0)
        || params.bumpy_x != 0.0
        || params.tan_bumpy_x != 0.0
        || params.drunk != 0.0
        || params.tan_drunk != 0.0
        || (params.beat.is_finite() && params.beat != 0.0)
        || (params.parabola_x.is_finite() && params.parabola_x != 0.0)
        || (params.attenuate_x.is_finite() && params.attenuate_x != 0.0)
        || (params.xmode.is_finite() && params.xmode != 0.0)
        || (params.digital.is_finite() && params.digital != 0.0)
        || (params.zigzag.is_finite() && params.zigzag != 0.0)
        || (params.square.is_finite() && params.square != 0.0)
    {
        return false;
    }
    let columns = num_cols.min(out.len());
    for (local_col, offset) in out.iter_mut().take(columns).enumerate() {
        *offset = note_x_offset_cached(
            local_col,
            0.0,
            0.0,
            0.0,
            col_offsets,
            invert,
            tornado,
            &[],
            move_x_offsets,
            params,
            tiny_scale,
        );
    }
    true
}
#[inline(always)]
pub(crate) fn appearance_note_alpha_is_identity(params: NoteAlphaParams) -> bool {
    params.hidden == 0.0
        && params.sudden == 0.0
        && params.stealth == 0.0
        && params.stealth_col == 0.0
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
            stealth_col: 0.0,
            stealth_type: params.stealth_type,
            stealth_past_receptors: params.stealth_past_receptors,
            blink_adjust: 0.0,
            random_vanish_active: false,
            random_vanish: 0.0,
            combined_fade_low_y: 0.0,
            combined_fade_high_y: 0.0,
            fade_low_alpha: 1.0,
            fade_high_alpha: 1.0,
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
    let stealth_active = (!params.stealth.is_nan() && params.stealth != 0.0)
        || (!params.stealth_col.is_nan() && params.stealth_col != 0.0);
    let blink_active = params.blink > f32::EPSILON;
    let random_vanish_active = params.random_vanish > f32::EPSILON;
    let path = match (
        hidden_active,
        sudden_active,
        stealth_active,
        blink_active,
        random_vanish_active,
    ) {
        (true, false, false, false, false) => {
            if hidden_bounds_finite && hidden_denom.abs() >= 1e-6 {
                AppearancePath::HiddenOnly
            } else {
                AppearancePath::HiddenOnlyUnbounded
            }
        }
        (false, true, false, false, false) => {
            if sudden_bounds_finite && sudden_denom.abs() >= 1e-6 {
                AppearancePath::SuddenOnly
            } else {
                AppearancePath::SuddenOnlyUnbounded
            }
        }
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
    // Reuse the endpoint slots for single fades as well as combined fades.
    // Inactive modifiers must stay excluded from single-effect arithmetic.
    let (fade_low_alpha, fade_high_alpha) = match path {
        AppearancePath::HiddenOnly | AppearancePath::HiddenOnlyUnbounded => (
            (1.0 + params.hidden.mul_add(-1.0, 0.0)).clamp(0.0, 1.0),
            (1.0 + params.hidden.mul_add(0.0, 0.0)).clamp(0.0, 1.0),
        ),
        AppearancePath::SuddenOnly | AppearancePath::SuddenOnlyUnbounded => (
            (1.0 + params.sudden.mul_add(0.0, 0.0)).clamp(0.0, 1.0),
            (1.0 + params.sudden.mul_add(-1.0, 0.0)).clamp(0.0, 1.0),
        ),
        _ => {
            let mut combined_fade_low_adjust = 0.0;
            combined_fade_low_adjust = params.hidden.mul_add(-1.0, combined_fade_low_adjust);
            combined_fade_low_adjust = params.sudden.mul_add(0.0, combined_fade_low_adjust);
            combined_fade_low_adjust -= params.stealth;
            combined_fade_low_adjust -= params.stealth_col;
            combined_fade_low_adjust += blink_adjust;
            let mut combined_fade_high_adjust = 0.0;
            combined_fade_high_adjust = params.hidden.mul_add(0.0, combined_fade_high_adjust);
            combined_fade_high_adjust = params.sudden.mul_add(-1.0, combined_fade_high_adjust);
            combined_fade_high_adjust -= params.stealth;
            combined_fade_high_adjust -= params.stealth_col;
            combined_fade_high_adjust += blink_adjust;
            (
                (1.0 + combined_fade_low_adjust).clamp(0.0, 1.0),
                (1.0 + combined_fade_high_adjust).clamp(0.0, 1.0),
            )
        }
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
        stealth_col: params.stealth_col,
        stealth_type: params.stealth_type,
        stealth_past_receptors: params.stealth_past_receptors,
        blink_adjust,
        random_vanish_active,
        random_vanish: params.random_vanish,
        combined_fade_low_y: hidden_end.min(sudden_end),
        combined_fade_high_y: hidden_start.max(sudden_start),
        fade_low_alpha,
        fade_high_alpha,
    }
}

#[inline(always)]
pub(crate) fn appearance_note_alpha_glow_cached(
    y: f32,
    y_offset: f32,
    cache: &NoteAppearanceCache,
) -> (f32, f32) {
    let percent_visible = appearance_note_alpha_cached(y, y_offset, cache);
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
pub(crate) fn appearance_note_alpha_cached(
    y_without_reverse: f32,
    y_offset: f32,
    cache: &NoteAppearanceCache,
) -> f32 {
    // Native StealthType excludes Tipsy from fade and receptor tests.
    // RandomVanish continues to use the position including Tipsy.
    // MoveY affects placement only and is excluded from both coordinates.
    let y = if cache.stealth_type {
        y_offset
    } else {
        y_without_reverse
    };
    if cache.identity || (y < 0.0 && !cache.stealth_past_receptors) {
        return 1.0;
    }
    match cache.path {
        AppearancePath::HiddenOnly => {
            if y <= cache.hidden_end {
                return cache.fade_low_alpha;
            }
            if y >= cache.hidden_start {
                return cache.fade_high_alpha;
            }
            let scaled = ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0);
            let visible_adjust = cache.hidden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenOnly => {
            if y <= cache.sudden_end {
                return cache.fade_low_alpha;
            }
            if y >= cache.sudden_start {
                return cache.fade_high_alpha;
            }
            let scaled = ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0);
            let visible_adjust = cache.sudden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenOnlyUnbounded => {
            let scaled = hidden_fade_scaled_bounded(y, cache);
            let visible_adjust = cache.hidden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenOnlyUnbounded => {
            let scaled = sudden_fade_scaled_bounded(y, cache);
            let visible_adjust = cache.sudden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::StealthOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust -= cache.stealth;
            visible_adjust -= cache.stealth_col;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::BlinkOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.fade_high_alpha;
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
            visible_adjust -= cache.stealth_col;
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
            visible_adjust -= cache.stealth_col;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenStealthOnly => {
            let mut visible_adjust = 0.0;
            let scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            visible_adjust -= cache.stealth_col;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenStealthOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.fade_high_alpha;
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
            visible_adjust -= cache.stealth_col;
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
                return cache.fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.fade_high_alpha;
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
            visible_adjust -= cache.stealth_col;
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
            visible_adjust -= cache.stealth_col;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenStealthBlinkOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.fade_high_alpha;
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
            visible_adjust -= cache.stealth_col;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::General => {}
    }
    appearance_note_alpha_general(y, y_without_reverse, cache)
}

#[inline(always)]
fn appearance_note_alpha_general(
    y: f32,
    y_without_reverse: f32,
    cache: &NoteAppearanceCache,
) -> f32 {
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
        visible_adjust -= cache.stealth_col;
    }
    visible_adjust += cache.blink_adjust;
    if cache.random_vanish_active {
        let dist = (y_without_reverse - cache.center_line).abs();
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

// ArrowEffects' X/Y/Z attenuation uses the unmodified style column offset.
pub(crate) fn attenuate_offset(y: f32, col_x: f32, amount: f32) -> f32 {
    if amount == 0.0 || !amount.is_finite() || !col_x.is_finite() {
        0.0
    } else {
        amount
            * (y / ARROW_EFFECT_PIXEL_SIZE)
            * (y / ARROW_EFFECT_PIXEL_SIZE)
            * (col_x / ARROW_EFFECT_PIXEL_SIZE)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_matches_native_phase_and_travel() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/expand-motion.json"
        ))
        .expect("full native GetYOffset and phase update block");
        let vectors = native["vectors"].as_array().expect("native Expand vectors");
        assert_eq!(vectors.len(), 6480);
        let mut phases = [0.0; 12];
        let mut steps = 0;
        for v in vectors {
            let value = |key: &str| v[key].as_f64().expect("native input") as f32;
            let seq = value("sequence") as usize;
            let flags = value("flags") as usize;
            if value("strength") == -0.5 && value("raw") == -128.0 && value("speed") == 0.5 {
                let phase = &mut phases[seq * 4 + flags];
                *phase = deadsync_gameplay::advance_expand_phase(
                    *phase,
                    value("delta"),
                    value("period"),
                    flags & 1 != 0,
                    flags & 2 != 0,
                );
                assert!(
                    (*phase - value("seconds")).abs() < 0.00001,
                    "phase {v}; actual={phase}"
                );
                steps += 1;
            }
            let accel = AccelYParams {
                expand: value("strength"),
                expand_period: value("period"),
                ..Default::default()
            };
            let actual = apply_accel_y_with_peak_cached(
                value("raw"),
                480.0,
                480.0,
                accel,
                accel_y_cache(value("seconds"), 480.0, accel),
            )
            .0 * value("speed");
            assert!(
                (actual - value("y")).abs() < 0.0001,
                "travel {v}; actual={actual}"
            );
        }
        assert_eq!(steps, 144);
    }

    #[test]
    fn shrink_matches_native_zoom() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/shrink-motion.json"
        ))
        .expect("unchanged full native GetZoom/GetZoomVariable/GetPulseInner");
        let vectors = native["vectors"].as_array().expect("native zooms");
        assert_eq!(vectors.len(), 2835);
        for vector in vectors {
            let value = |key: &str| vector[key].as_f64().expect("native input") as f32;
            let cache = lane_note_transform_cache(
                0.0,
                VisualEffectParams {
                    shrink_linear: value("linear"),
                    shrink_mult: value("mult"),
                    pulse_inner: value("inner"),
                    pulse_outer: value("outer"),
                    pulse_offset: value("offset"),
                    pulse_period: value("period"),
                    tiny: value("tiny") + value("lane_tiny"),
                    ..Default::default()
                },
            );
            for (travel, key) in [(value("travel"), "zoom"), (0.0, "receptor_zoom")] {
                let actual = visual_arrow_effect_zoom_cached(travel, cache, value("field"));
                if let Some(expected) = vector[key].as_f64() {
                    assert!(
                        (actual - expected as f32).abs() < 0.0001,
                        "{key}: {vector}; actual={actual}"
                    );
                } else {
                    match vector[key].as_str().expect("IEEE state") {
                        "nan" => assert!(actual.is_nan(), "{vector}; actual={actual}"),
                        "inf" => assert_eq!(actual, f32::INFINITY, "{vector}"),
                        "-inf" => assert_eq!(actual, f32::NEG_INFINITY, "{vector}"),
                        _ => panic!("invalid native IEEE state"),
                    }
                }
            }
        }
    }

    #[test]
    fn z_waves_match_native_positions() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/z-wave-motion.json"
        ))
        .expect("unchanged native GetZPos/NeedZBuffer, Sawtooth and bounds");
        let vectors = native["vectors"].as_array().expect("native vectors");
        assert_eq!(vectors.len(), 9720);
        for vector in vectors {
            let value = |key: &str| vector[key].as_f64().expect("native float") as f32;
            let count = vector["columns"].as_u64().expect("columns") as usize;
            let col = vector["col"].as_u64().expect("column") as usize;
            let mut columns = [0.0; 8];
            for (i, x) in columns[..count].iter_mut().enumerate() {
                *x = (i as f32 - (count - 1) as f32 * 0.5) * 64.0 * value("zoom");
            }
            let mut bounds = [TornadoBounds::default(); 8];
            compute_tornado_z_bounds(&columns[..count], &mut bounds[..count]);
            let params = VisualEffectParams {
                bounce_z: value("bounce_z"),
                bounce_z_offset: value("bounce_z_offset"),
                bounce_z_period: value("bounce_z_period"),
                digital_z: value("digital_z"),
                digital_z_offset: value("digital_z_offset"),
                digital_z_period: value("digital_z_period"),
                digital_z_steps: value("digital_z_steps"),
                tornado_z: value("tornado_z"),
                tornado_z_offset: value("tornado_z_offset"),
                tornado_z_period: value("tornado_z_period"),
                sawtooth_z: value("sawtooth_z"),
                sawtooth_z_period: value("sawtooth_z_period"),
                col_x: columns[col],
                tornado_z_bounds: bounds[col],
                ..Default::default()
            };
            let cache = lane_note_transform_cache(0.0, params);
            let frame = note_depth_frame_cache(0.0, 0.0, 0.0, 480.0);
            for (travel, key) in [(value("travel"), "z"), (0.0, "receptor_z")] {
                let actual = note_world_z_cached(travel, frame, cache);
                assert!(
                    (actual - value(key)).abs() < 0.0001,
                    "{key}: {vector}; actual={actual}"
                );
            }
            let x_params = NoteXParams {
                sawtooth: value("sawtooth"),
                sawtooth_period: value("sawtooth_period"),
                ..Default::default()
            };
            for (travel, key) in [(value("travel"), "x"), (0.0, "receptor_x")] {
                let direct = note_x_offset(
                    col,
                    travel,
                    0.0,
                    0.0,
                    &columns[..count],
                    &[0.0; 8],
                    &[TornadoBounds::default(); 8],
                    &[0.0; 8],
                    x_params,
                    value("tiny"),
                );
                let cached = note_x_offset_cached(
                    col,
                    travel,
                    0.0,
                    0.0,
                    &columns[..count],
                    &[0.0; 8],
                    &[TornadoBounds::default(); 8],
                    &[TornadoLaneCache::default(); 8],
                    &[0.0; 8],
                    x_params,
                    tiny_spacing_scale(value("tiny")),
                );
                for actual in [direct, cached] {
                    assert!(
                        (actual - value(key)).abs() < 0.0001,
                        "{key}: {vector}; actual={actual}"
                    );
                }
            }
            assert_eq!(
                visual_hold_body_needs_z_buffer(params),
                vector["depth"].as_bool().expect("native depth gate"),
                "{vector}"
            );
        }
    }

    #[test]
    fn confusion_spin_matches_native_rotations() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/confusion-spin-rotation.json"
        ))
        .expect("independently compiled native X/Y rotation functions");
        let vectors = native["vectors"].as_array().expect("native rotations");
        assert_eq!(vectors.len(), 672);
        for vector in vectors {
            let value = |key: &str| vector[key].as_f64().expect("native float") as f32;
            let cache = lane_note_transform_cache(
                value("beat"),
                VisualEffectParams {
                    confusion_x: value("strength_x"),
                    confusion_y: value("strength_y"),
                    confusion_x_offset: value("offset_x"),
                    confusion_y_offset: value("offset_y"),
                    ..Default::default()
                },
            );
            let x = cache.confusion_rotation_x_deg
                + if vector["hold_cap"].as_bool().expect("cap gate") {
                    0.0
                } else {
                    visual_note_rotation_x(value("travel"), value("roll"))
                };
            let y = cache.confusion_rotation_y_deg
                + visual_note_rotation_y(value("travel"), value("twirl"));
            for (actual, key) in [
                (cache.confusion_rotation_x_deg, "receptor_x"),
                (cache.confusion_rotation_y_deg, "receptor_y"),
                (x, "note_x"),
                (y, "note_y"),
            ] {
                assert!(
                    (actual - value(key)).abs() < 0.0001,
                    "{key}: {vector}; actual={actual}"
                );
            }
        }
    }

    #[test]
    fn attenuation_matches_native_vectors() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/attenuate-motion.json"
        ))
        .expect("independently compiled native attenuation blocks");
        let vectors = native["vectors"].as_array().expect("native vectors");
        assert_eq!(vectors.len(), 980);
        for vector in vectors {
            let value = |key: &str| vector[key].as_f64().expect("native float") as f32;
            let travel = value("travel");
            let col = value("col_x");
            let x_params = NoteXParams {
                attenuate_x: value("amount_x"),
                ..Default::default()
            };
            let x = note_x_offset(
                0,
                travel,
                0.0,
                0.0,
                &[col],
                &[0.0],
                &[TornadoBounds::default()],
                &[0.0],
                x_params,
                value("tiny"),
            );
            let cached_x = note_x_offset_cached(
                0,
                travel,
                0.0,
                0.0,
                &[col],
                &[0.0],
                &[TornadoBounds::default()],
                &[TornadoLaneCache::default()],
                &[0.0],
                x_params,
                tiny_spacing_scale(value("tiny")),
            );
            let y = value("direction") * travel
                + value("lane_offset")
                + attenuate_offset(travel, col, value("amount_y"));
            let z_params = VisualEffectParams {
                attenuate_z: value("amount_z"),
                col_x: col,
                ..Default::default()
            };
            let z = note_world_z_cached(
                travel,
                note_depth_frame_cache(0.0, 0.0, 0.0, 480.0),
                lane_note_transform_cache(0.0, z_params),
            );
            for (actual, key) in [(x, "x"), (cached_x, "x"), (y, "y"), (z, "z")] {
                assert!(
                    (actual - value(key)).abs() < 0.0001,
                    "{key}: {vector}; actual={actual}"
                );
            }
            assert_eq!(
                visual_hold_body_needs_z_buffer(z_params),
                value("amount_z") != 0.0
            );
        }
    }

    #[test]
    fn beat_family_matches_native_vectors() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/beat-family-motion.json"
        ))
        .expect("independently compiled native Beat blocks");
        let vectors = native["vectors"].as_array().expect("native vectors");
        assert_eq!(vectors.len(), 2400);
        for vector in vectors {
            let value = |key: &str| vector[key].as_f64().expect("native float") as f32;
            let travel = value("travel");
            let col = value("col_x");
            let x_params = NoteXParams {
                beat: value("amount_x"),
                beat_period: value("period_x"),
                ..Default::default()
            };
            let factor = beat_factor(value("beat"), value("offset_x"), value("mult_x"));
            let x = note_x_offset(
                0,
                travel,
                factor,
                0.0,
                &[col],
                &[0.0],
                &[TornadoBounds::default()],
                &[0.0],
                x_params,
                value("tiny"),
            );
            let cached_x = note_x_offset_cached(
                0,
                travel,
                factor,
                0.0,
                &[col],
                &[0.0],
                &[TornadoBounds::default()],
                &[TornadoLaneCache::default()],
                &[0.0],
                x_params,
                tiny_spacing_scale(value("tiny")),
            );
            let y = value("direction") * travel
                + value("lane_offset")
                + beat_wave_offset(
                    travel,
                    beat_factor(value("beat"), value("offset_y"), value("mult_y")),
                    value("amount_y"),
                    value("period_y"),
                );
            let z_params = VisualEffectParams {
                beat_z: value("amount_z"),
                beat_z_offset: value("offset_z"),
                beat_z_mult: value("mult_z"),
                beat_z_period: value("period_z"),
                col_x: col,
                ..Default::default()
            };
            let z = note_world_z_cached(
                travel,
                note_depth_frame_cache(0.0, 0.0, 0.0, 480.0),
                lane_note_transform_cache(value("beat"), z_params),
            );
            for (actual, key) in [(x, "x"), (cached_x, "x"), (y, "y"), (z, "z")] {
                assert!(
                    (actual - value(key)).abs() < 0.0001,
                    "{key}: {vector}; actual={actual}"
                );
            }
            for axis in ["x", "y", "z"] {
                let actual = beat_factor(
                    value("beat"),
                    value(&format!("offset_{axis}")),
                    value(&format!("mult_{axis}")),
                );
                assert!(
                    (actual - value(&format!("factor_{axis}"))).abs() < 0.0001,
                    "factor {axis}: {vector}; actual={actual}"
                );
            }
            assert_eq!(
                visual_hold_body_needs_z_buffer(z_params),
                value("amount_z") != 0.0
            );
        }
    }

    #[test]
    fn parabola_y_and_accels_match_native_travel() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/parabola-y-motion.json"
        )).expect("independently compiled native additive acceleration vectors");
        let vectors = native["vectors"].as_array().expect("native vectors");
        assert_eq!(vectors.len(), 78);
        for vector in vectors {
            let value = |key: &str| vector[key].as_f64().expect("native float") as f32;
            let accel = AccelYParams {
                parabola_y: value("amount"), boost: value("boost"), brake: value("brake"),
                wave: value("wave"), wave_period: value("period"), boomerang: value("boomerang"),
                ..AccelYParams::default()
            };
            let cache = accel_y_cache(0.0, 480.0, accel);
            let raw = value("travel");
            let expected = value("y");
            let actual = apply_accel_y_cached(raw, 480.0, 480.0, accel, cache);
            assert!((actual - expected).abs() < 0.0005, "{vector}: actual={actual}");
            if raw >= 0.0 {
                let general = apply_accel_y_general(raw, 480.0, 480.0, accel, cache).0;
                assert!((general - expected).abs() < 0.0005, "{vector}: general={general}");
            }
        }
    }

    #[test]
    fn wave_period_matches_native_travel() {
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/wave-period-motion.json"
        )).expect("independently compiled native Wave vectors");
        let vectors = native["vectors"].as_array().expect("native vectors");
        assert_eq!(vectors.len(), 80);
        for vector in vectors {
            let value = |key: &str| vector[key].as_f64().expect("native float") as f32;
            let accel = AccelYParams {
                wave: value("amount"), wave_period: value("period"),
                ..AccelYParams::default()
            };
            let cache = accel_y_cache(0.0, 480.0, accel);
            let raw = value("travel");
            let expected = value("y");
            let optimized = apply_accel_y_cached(raw, 480.0, 480.0, accel, cache);
            let general = apply_accel_y_general(raw, 480.0, 480.0, accel, cache).0;
            assert!((optimized - expected).abs() < 0.00005, "{vector}: optimized={optimized}");
            assert!((general - expected).abs() < 0.00005, "{vector}: general={general}");
            assert_eq!(accel_y_is_identity(accel), accel.wave == 0.0);
        }
    }
}
