pub fn quantize_centi_i32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    (value * 100.0).round() as i32
}

pub fn quantize_centi_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * 100.0).round() as u32
}

pub fn mod_percent_key(level: f32) -> i16 {
    clamp_rounded_i16(level * 100.0)
}

pub const fn clamp_rounded_i16(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value.round() as i16
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
