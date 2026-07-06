use crate::app_config::Config;
use crate::bools::parse_u8_bool_or_default;
use crate::defaults::{
    DEFAULT_NULL_OR_DIE_CONFIDENCE_PERCENT, DEFAULT_NULL_OR_DIE_FINGERPRINT_MS,
    DEFAULT_NULL_OR_DIE_FULL_SPECTROGRAM, DEFAULT_NULL_OR_DIE_MAGIC_OFFSET_MS,
    DEFAULT_NULL_OR_DIE_PACK_SYNC_THREADS, DEFAULT_NULL_OR_DIE_STEP_MS,
    DEFAULT_NULL_OR_DIE_WINDOW_MS,
};
use crate::ini::SimpleIni;
use crate::numbers::parse_auto_threads_u8;
use crate::theme::SyncGraphMode;
use crate::writer::{push_bool, push_line};
use null_or_die::{BiasCfg, BiasKernel, KernelTarget};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NullOrDieOptions {
    pub sync_graph: SyncGraphMode,
    pub confidence_percent: u8,
    pub pack_sync_threads: u8,
    pub fingerprint_ms: f64,
    pub window_ms: f64,
    pub step_ms: f64,
    pub magic_offset_ms: f64,
    pub kernel_target: KernelTarget,
    pub kernel_type: BiasKernel,
    pub full_spectrogram: bool,
}

impl Default for NullOrDieOptions {
    fn default() -> Self {
        Self {
            sync_graph: SyncGraphMode::PostKernelFingerprint,
            confidence_percent: DEFAULT_NULL_OR_DIE_CONFIDENCE_PERCENT,
            pack_sync_threads: DEFAULT_NULL_OR_DIE_PACK_SYNC_THREADS,
            fingerprint_ms: DEFAULT_NULL_OR_DIE_FINGERPRINT_MS,
            window_ms: DEFAULT_NULL_OR_DIE_WINDOW_MS,
            step_ms: DEFAULT_NULL_OR_DIE_STEP_MS,
            magic_offset_ms: DEFAULT_NULL_OR_DIE_MAGIC_OFFSET_MS,
            kernel_target: KernelTarget::Digest,
            kernel_type: BiasKernel::Rising,
            full_spectrogram: DEFAULT_NULL_OR_DIE_FULL_SPECTROGRAM,
        }
    }
}

pub fn load_null_or_die_options(conf: &SimpleIni, default: NullOrDieOptions) -> NullOrDieOptions {
    NullOrDieOptions {
        sync_graph: conf
            .get("Options", "NullOrDieSyncGraph")
            .and_then(|value| SyncGraphMode::from_str(&value).ok())
            .unwrap_or(default.sync_graph),
        confidence_percent: conf
            .get("Options", "NullOrDieConfidencePercent")
            .and_then(|value| value.parse::<u8>().ok())
            .map(clamp_null_or_die_confidence_percent)
            .unwrap_or(default.confidence_percent),
        pack_sync_threads: conf
            .get("Options", "PackSyncThreads")
            .and_then(|value| parse_auto_threads_u8(&value))
            .unwrap_or(default.pack_sync_threads),
        fingerprint_ms: conf
            .get("Options", "NullOrDieFingerprintMs")
            .and_then(|value| value.parse::<f64>().ok())
            .map(clamp_null_or_die_positive_ms)
            .unwrap_or(default.fingerprint_ms),
        window_ms: conf
            .get("Options", "NullOrDieWindowMs")
            .and_then(|value| value.parse::<f64>().ok())
            .map(clamp_null_or_die_positive_ms)
            .unwrap_or(default.window_ms),
        step_ms: conf
            .get("Options", "NullOrDieStepMs")
            .and_then(|value| value.parse::<f64>().ok())
            .map(clamp_null_or_die_positive_ms)
            .unwrap_or(default.step_ms),
        magic_offset_ms: conf
            .get("Options", "NullOrDieMagicOffsetMs")
            .and_then(|value| value.parse::<f64>().ok())
            .map(clamp_null_or_die_magic_offset_ms)
            .unwrap_or(default.magic_offset_ms),
        kernel_target: conf
            .get("Options", "NullOrDieKernelTarget")
            .and_then(|value| parse_null_or_die_kernel_target(&value))
            .unwrap_or(default.kernel_target),
        kernel_type: conf
            .get("Options", "NullOrDieKernelType")
            .and_then(|value| parse_null_or_die_kernel_type(&value))
            .unwrap_or(default.kernel_type),
        full_spectrogram: parse_u8_bool_or_default(
            conf.get("Options", "NullOrDieFullSpectrogram").as_deref(),
            default.full_spectrogram,
        ),
    }
}

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

pub fn push_null_or_die_option_lines(content: &mut String, options: NullOrDieOptions) {
    push_line(content, "NullOrDieSyncGraph", options.sync_graph.as_str());
    push_line(
        content,
        "NullOrDieConfidencePercent",
        clamp_null_or_die_confidence_percent(options.confidence_percent),
    );
    push_line(content, "PackSyncThreads", options.pack_sync_threads);
    push_line(
        content,
        "NullOrDieFingerprintMs",
        format!(
            "{:.1}",
            clamp_null_or_die_positive_ms(options.fingerprint_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieWindowMs",
        format!("{:.1}", clamp_null_or_die_positive_ms(options.window_ms)),
    );
    push_line(
        content,
        "NullOrDieStepMs",
        format!("{:.1}", clamp_null_or_die_positive_ms(options.step_ms)),
    );
    push_line(
        content,
        "NullOrDieMagicOffsetMs",
        format!(
            "{:.1}",
            clamp_null_or_die_magic_offset_ms(options.magic_offset_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieKernelTarget",
        null_or_die_kernel_target_str(options.kernel_target),
    );
    push_line(
        content,
        "NullOrDieKernelType",
        null_or_die_kernel_type_str(options.kernel_type),
    );
    push_bool(
        content,
        "NullOrDieFullSpectrogram",
        options.full_spectrogram,
    );
}

pub fn null_or_die_bias_cfg(options: NullOrDieOptions) -> BiasCfg {
    BiasCfg {
        fingerprint_ms: clamp_null_or_die_positive_ms(options.fingerprint_ms),
        window_ms: clamp_null_or_die_positive_ms(options.window_ms),
        step_ms: clamp_null_or_die_positive_ms(options.step_ms),
        magic_offset_ms: clamp_null_or_die_magic_offset_ms(options.magic_offset_ms),
        kernel_target: options.kernel_target,
        kernel_type: options.kernel_type,
        _full_spectrogram: options.full_spectrogram,
    }
}

pub fn null_or_die_options_from_config(cfg: Config) -> NullOrDieOptions {
    NullOrDieOptions {
        sync_graph: cfg.null_or_die_sync_graph,
        confidence_percent: cfg.null_or_die_confidence_percent,
        pack_sync_threads: cfg.null_or_die_pack_sync_threads,
        fingerprint_ms: cfg.null_or_die_fingerprint_ms,
        window_ms: cfg.null_or_die_window_ms,
        step_ms: cfg.null_or_die_step_ms,
        magic_offset_ms: cfg.null_or_die_magic_offset_ms,
        kernel_target: cfg.null_or_die_kernel_target,
        kernel_type: cfg.null_or_die_kernel_type,
        full_spectrogram: cfg.null_or_die_full_spectrogram,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_options() -> NullOrDieOptions {
        NullOrDieOptions {
            sync_graph: SyncGraphMode::Frequency,
            confidence_percent: 80,
            pack_sync_threads: 0,
            fingerprint_ms: 12.0,
            window_ms: 8.0,
            step_ms: 1.0,
            magic_offset_ms: 0.0,
            kernel_target: KernelTarget::Digest,
            kernel_type: BiasKernel::Rising,
            full_spectrogram: false,
        }
    }

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

    #[test]
    fn loads_null_or_die_options_from_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            NullOrDieSyncGraph=PostKernel
            NullOrDieConfidencePercent=200
            PackSyncThreads=4
            NullOrDieFingerprintMs=10.05
            NullOrDieWindowMs=0
            NullOrDieStepMs=250
            NullOrDieMagicOffsetMs=-250
            NullOrDieKernelTarget=Accumulator
            NullOrDieKernelType=Loudest
            NullOrDieFullSpectrogram=1
            "#,
        );

        let loaded = load_null_or_die_options(&conf, default_options());

        assert_eq!(loaded.sync_graph, SyncGraphMode::PostKernelFingerprint);
        assert_eq!(loaded.confidence_percent, 100);
        assert_eq!(loaded.pack_sync_threads, 4);
        assert_tenths_eq(loaded.fingerprint_ms, 101);
        assert_tenths_eq(loaded.window_ms, 1);
        assert_tenths_eq(loaded.step_ms, 1000);
        assert_tenths_eq(loaded.magic_offset_ms, -1000);
        assert_eq!(loaded.kernel_target, KernelTarget::Accumulator);
        assert_eq!(loaded.kernel_type, BiasKernel::Loudest);
        assert!(loaded.full_spectrogram);
    }

    #[test]
    fn load_null_or_die_options_keeps_defaults_for_bad_values() {
        let default = default_options();
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            NullOrDieSyncGraph=bad
            NullOrDieConfidencePercent=bad
            PackSyncThreads=bad
            NullOrDieFingerprintMs=bad
            NullOrDieWindowMs=bad
            NullOrDieStepMs=bad
            NullOrDieMagicOffsetMs=bad
            NullOrDieKernelTarget=bad
            NullOrDieKernelType=bad
            NullOrDieFullSpectrogram=bad
            "#,
        );

        assert_eq!(load_null_or_die_options(&conf, default), default);
    }

    #[test]
    fn writes_null_or_die_option_lines() {
        let mut content = String::new();

        push_null_or_die_option_lines(
            &mut content,
            NullOrDieOptions {
                sync_graph: SyncGraphMode::PostKernelFingerprint,
                confidence_percent: 250,
                pack_sync_threads: 4,
                fingerprint_ms: 10.05,
                window_ms: 0.0,
                step_ms: 250.0,
                magic_offset_ms: -250.0,
                kernel_target: KernelTarget::Accumulator,
                kernel_type: BiasKernel::Loudest,
                full_spectrogram: true,
            },
        );

        assert_eq!(
            content,
            "NullOrDieSyncGraph=PostKernelFingerprint\n\
NullOrDieConfidencePercent=100\n\
PackSyncThreads=4\n\
NullOrDieFingerprintMs=10.1\n\
NullOrDieWindowMs=0.1\n\
NullOrDieStepMs=100.0\n\
NullOrDieMagicOffsetMs=-100.0\n\
NullOrDieKernelTarget=Accumulator\n\
NullOrDieKernelType=Loudest\n\
NullOrDieFullSpectrogram=1\n"
        );
    }

    #[test]
    fn bias_cfg_clamps_runtime_values() {
        let cfg = null_or_die_bias_cfg(NullOrDieOptions {
            sync_graph: SyncGraphMode::Frequency,
            confidence_percent: 80,
            pack_sync_threads: 0,
            fingerprint_ms: 0.0,
            window_ms: 250.0,
            step_ms: 10.05,
            magic_offset_ms: -250.0,
            kernel_target: KernelTarget::Accumulator,
            kernel_type: BiasKernel::Loudest,
            full_spectrogram: true,
        });

        assert_tenths_eq(cfg.fingerprint_ms, 1);
        assert_tenths_eq(cfg.window_ms, 1000);
        assert_tenths_eq(cfg.step_ms, 101);
        assert_tenths_eq(cfg.magic_offset_ms, -1000);
        assert_eq!(cfg.kernel_target, KernelTarget::Accumulator);
        assert_eq!(cfg.kernel_type, BiasKernel::Loudest);
        assert!(cfg._full_spectrogram);
    }
}
