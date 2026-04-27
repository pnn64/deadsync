use super::*;

#[inline(always)]
pub(super) fn format_ms(value: i32) -> String {
    // Positive values omit a '+' and compact to the Simply Love "Nms" style.
    format!("{value}ms")
}

#[inline(always)]
pub(super) fn format_percent(value: i32) -> String {
    format!("{value}%")
}

#[inline(always)]
pub(super) fn format_tenths_ms(value_tenths: i32) -> String {
    format!("{:.1} ms", value_tenths as f64 / 10.0)
}

#[inline(always)]
pub(super) fn adjust_ms_value(value: &mut i32, delta: isize, min: i32, max: i32) -> bool {
    let new_value = (*value + delta as i32).clamp(min, max);
    if new_value == *value {
        false
    } else {
        *value = new_value;
        true
    }
}

#[inline(always)]
pub(super) fn adjust_tenths_value(value: &mut i32, delta: isize, min: i32, max: i32) -> bool {
    let new_value = (*value + delta as i32).clamp(min, max);
    if new_value == *value {
        false
    } else {
        *value = new_value;
        true
    }
}

#[inline(always)]
pub(super) fn tenths_from_f64(value: f64) -> i32 {
    let scaled = value * 10.0;
    let nudge = scaled.signum() * scaled.abs().max(1.0) * f64::EPSILON * 16.0;
    (scaled + nudge).round() as i32
}

#[inline(always)]
pub(super) fn f64_from_tenths(value: i32) -> f64 {
    value as f64 / 10.0
}
