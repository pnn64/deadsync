use null_or_die::{BiasKernel, KernelTarget};

pub fn clamp_null_or_die_confidence_percent(value: u8) -> u8 {
    value.min(100)
}

const NULL_OR_DIE_POSITIVE_MS_MIN: f64 = 0.1;
const NULL_OR_DIE_POSITIVE_MS_MAX: f64 = 100.0;
const NULL_OR_DIE_MAGIC_OFFSET_MS_MIN: f64 = -100.0;
const NULL_OR_DIE_MAGIC_OFFSET_MS_MAX: f64 = 100.0;

#[inline(always)]
fn quantize_tenths(value: f64) -> f64 {
    let scaled = value * 10.0;
    // Nudge decimal half-steps across the IEEE-754 error margin so values like
    // 10.05 round to 10.1 instead of falling back to 10.0.
    let nudge = scaled.signum() * scaled.abs().max(1.0) * f64::EPSILON * 16.0;
    (scaled + nudge).round() / 10.0
}

#[inline(always)]
pub fn clamp_null_or_die_positive_ms(value: f64) -> f64 {
    if !value.is_finite() {
        return NULL_OR_DIE_POSITIVE_MS_MIN;
    }
    quantize_tenths(value.clamp(NULL_OR_DIE_POSITIVE_MS_MIN, NULL_OR_DIE_POSITIVE_MS_MAX))
}

#[inline(always)]
pub fn clamp_null_or_die_magic_offset_ms(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    quantize_tenths(value.clamp(
        NULL_OR_DIE_MAGIC_OFFSET_MS_MIN,
        NULL_OR_DIE_MAGIC_OFFSET_MS_MAX,
    ))
}

#[inline(always)]
pub const fn null_or_die_kernel_target_str(target: KernelTarget) -> &'static str {
    match target {
        KernelTarget::Digest => "Digest",
        KernelTarget::Accumulator => "Accumulator",
    }
}

pub fn parse_null_or_die_kernel_target(raw: &str) -> Option<KernelTarget> {
    let key = raw
        .trim()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    match key.as_str() {
        "digest" => Some(KernelTarget::Digest),
        "accumulator" => Some(KernelTarget::Accumulator),
        _ => None,
    }
}

#[inline(always)]
pub const fn null_or_die_kernel_target_choice_index(target: KernelTarget) -> usize {
    match target {
        KernelTarget::Digest => 0,
        KernelTarget::Accumulator => 1,
    }
}

#[inline(always)]
pub const fn null_or_die_kernel_target_from_choice(idx: usize) -> KernelTarget {
    match idx {
        1 => KernelTarget::Accumulator,
        _ => KernelTarget::Digest,
    }
}

#[inline(always)]
pub const fn null_or_die_kernel_type_str(kind: BiasKernel) -> &'static str {
    match kind {
        BiasKernel::Rising => "Rising",
        BiasKernel::Loudest => "Loudest",
    }
}

pub fn parse_null_or_die_kernel_type(raw: &str) -> Option<BiasKernel> {
    let key = raw
        .trim()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    match key.as_str() {
        "rising" => Some(BiasKernel::Rising),
        "loudest" => Some(BiasKernel::Loudest),
        _ => None,
    }
}

#[inline(always)]
pub const fn null_or_die_kernel_type_choice_index(kind: BiasKernel) -> usize {
    match kind {
        BiasKernel::Rising => 0,
        BiasKernel::Loudest => 1,
    }
}

#[inline(always)]
pub const fn null_or_die_kernel_type_from_choice(idx: usize) -> BiasKernel {
    match idx {
        1 => BiasKernel::Loudest,
        _ => BiasKernel::Rising,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_tenths_eq(value: f64, tenths: i32) {
        assert_eq!((value * 10.0).round() as i32, tenths);
    }

    #[test]
    fn confidence_caps_at_100() {
        assert_eq!(clamp_null_or_die_confidence_percent(0), 0);
        assert_eq!(clamp_null_or_die_confidence_percent(80), 80);
        assert_eq!(clamp_null_or_die_confidence_percent(120), 100);
    }

    #[test]
    fn positive_ms_uses_tenths() {
        assert_tenths_eq(clamp_null_or_die_positive_ms(0.0), 1);
        assert_tenths_eq(clamp_null_or_die_positive_ms(10.04), 100);
        assert_tenths_eq(clamp_null_or_die_positive_ms(10.05), 101);
        assert_tenths_eq(clamp_null_or_die_positive_ms(1000.0), 1000);
    }

    #[test]
    fn magic_offset_uses_tenths() {
        assert_tenths_eq(clamp_null_or_die_magic_offset_ms(-200.0), -1000);
        assert_tenths_eq(clamp_null_or_die_magic_offset_ms(0.04), 0);
        assert_tenths_eq(clamp_null_or_die_magic_offset_ms(0.05), 1);
        assert_tenths_eq(clamp_null_or_die_magic_offset_ms(200.0), 1000);
    }

    #[test]
    fn kernel_choices_match_options_order() {
        assert_eq!(
            null_or_die_kernel_target_choice_index(KernelTarget::Digest),
            0
        );
        assert_eq!(
            null_or_die_kernel_target_choice_index(KernelTarget::Accumulator),
            1
        );
        assert_eq!(
            null_or_die_kernel_target_from_choice(0),
            KernelTarget::Digest
        );
        assert_eq!(
            null_or_die_kernel_target_from_choice(1),
            KernelTarget::Accumulator
        );
        assert_eq!(
            null_or_die_kernel_target_from_choice(99),
            KernelTarget::Digest
        );

        assert_eq!(null_or_die_kernel_type_choice_index(BiasKernel::Rising), 0);
        assert_eq!(null_or_die_kernel_type_choice_index(BiasKernel::Loudest), 1);
        assert_eq!(null_or_die_kernel_type_from_choice(0), BiasKernel::Rising);
        assert_eq!(null_or_die_kernel_type_from_choice(1), BiasKernel::Loudest);
        assert_eq!(null_or_die_kernel_type_from_choice(99), BiasKernel::Rising);
    }
}
