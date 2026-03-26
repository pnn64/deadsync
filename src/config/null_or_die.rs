use super::{BiasCfg, BiasKernel, KernelTarget, get};

pub(super) fn clamp_null_or_die_confidence_percent(value: u8) -> u8 {
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
pub(super) fn clamp_null_or_die_positive_ms(value: f64) -> f64 {
    if !value.is_finite() {
        return NULL_OR_DIE_POSITIVE_MS_MIN;
    }
    quantize_tenths(value.clamp(NULL_OR_DIE_POSITIVE_MS_MIN, NULL_OR_DIE_POSITIVE_MS_MAX))
}

#[inline(always)]
pub(super) fn clamp_null_or_die_magic_offset_ms(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    quantize_tenths(value.clamp(
        NULL_OR_DIE_MAGIC_OFFSET_MS_MIN,
        NULL_OR_DIE_MAGIC_OFFSET_MS_MAX,
    ))
}

#[inline(always)]
pub(super) fn null_or_die_kernel_target_str(target: KernelTarget) -> &'static str {
    match target {
        KernelTarget::Digest => "Digest",
        KernelTarget::Accumulator => "Accumulator",
    }
}

pub(super) fn parse_null_or_die_kernel_target(raw: &str) -> Option<KernelTarget> {
    let key = raw
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    match key.as_str() {
        "digest" => Some(KernelTarget::Digest),
        "accumulator" => Some(KernelTarget::Accumulator),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn null_or_die_kernel_type_str(kind: BiasKernel) -> &'static str {
    match kind {
        BiasKernel::Rising => "Rising",
        BiasKernel::Loudest => "Loudest",
    }
}

pub(super) fn parse_null_or_die_kernel_type(raw: &str) -> Option<BiasKernel> {
    let key = raw
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    match key.as_str() {
        "rising" => Some(BiasKernel::Rising),
        "loudest" => Some(BiasKernel::Loudest),
        _ => None,
    }
}

pub fn null_or_die_bias_cfg() -> BiasCfg {
    let cfg = get();
    BiasCfg {
        fingerprint_ms: clamp_null_or_die_positive_ms(cfg.null_or_die_fingerprint_ms),
        window_ms: clamp_null_or_die_positive_ms(cfg.null_or_die_window_ms),
        step_ms: clamp_null_or_die_positive_ms(cfg.null_or_die_step_ms),
        magic_offset_ms: clamp_null_or_die_magic_offset_ms(cfg.null_or_die_magic_offset_ms),
        kernel_target: cfg.null_or_die_kernel_target,
        kernel_type: cfg.null_or_die_kernel_type,
        _full_spectrogram: cfg.null_or_die_full_spectrogram,
    }
}
