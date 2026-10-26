//! Frozen production reference from 6d1c4e5a1.
use crate::bools::parse_u8_bool_or_default;
use crate::ini::SimpleIni;
use crate::null_or_die::*;
use crate::numbers::parse_auto_threads_u8;
use crate::theme::SyncGraphMode;
use null_or_die::{BiasKernel, KernelTarget};
use std::{
    fmt::{Display, Write as _},
    str::FromStr,
};

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

fn format_null_or_die_magic_offset_ms(value: f64) -> String {
    let value = clamp_null_or_die_magic_offset_ms(value);
    let hundredths = (value * 100.0).round() as i32;
    if hundredths % 10 == 0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

pub fn push_null_or_die_option_lines(content: &mut String, options: NullOrDieOptions) {
    push_line(content, "NullOrDieSyncGraph", options.sync_graph.as_str());
    push_line(
        content,
        "NullOrDieGraphOrientation",
        null_or_die_graph_orientation_str(options.graph_orientation),
    );
    push_line(
        content,
        "NullOrDieGraphOrigin",
        null_or_die_graph_origin_str(options.graph_origin),
    );
    push_line(
        content,
        "NullOrDieConfidencePercent",
        clamp_null_or_die_confidence_percent(options.confidence_percent),
    );
    push_bool(content, "NullOrDieCacheResults", options.cache_results);
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
        format_null_or_die_magic_offset_ms(options.magic_offset_ms),
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

pub fn load_null_or_die_options(conf: &SimpleIni, default: NullOrDieOptions) -> NullOrDieOptions {
    NullOrDieOptions {
        sync_graph: conf
            .get("Options", "NullOrDieSyncGraph")
            .and_then(|value| SyncGraphMode::from_str(value).ok())
            .unwrap_or(default.sync_graph),
        graph_orientation: conf
            .get("Options", "NullOrDieGraphOrientation")
            .and_then(parse_null_or_die_graph_orientation)
            .unwrap_or(default.graph_orientation),
        graph_origin: conf
            .get("Options", "NullOrDieGraphOrigin")
            .and_then(parse_null_or_die_graph_origin)
            .unwrap_or(default.graph_origin),
        confidence_percent: conf
            .get("Options", "NullOrDieConfidencePercent")
            .and_then(|value| value.parse::<u8>().ok())
            .map(clamp_null_or_die_confidence_percent)
            .unwrap_or(default.confidence_percent),
        cache_results: parse_u8_bool_or_default(
            conf.get("Options", "NullOrDieCacheResults"),
            default.cache_results,
        ),
        pack_sync_threads: conf
            .get("Options", "PackSyncThreads")
            .and_then(parse_auto_threads_u8)
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
            .and_then(parse_null_or_die_kernel_target)
            .unwrap_or(default.kernel_target),
        kernel_type: conf
            .get("Options", "NullOrDieKernelType")
            .and_then(parse_null_or_die_kernel_type)
            .unwrap_or(default.kernel_type),
        full_spectrogram: parse_u8_bool_or_default(
            conf.get("Options", "NullOrDieFullSpectrogram"),
            default.full_spectrogram,
        ),
    }
}

pub fn push_line(content: &mut String, key: &str, value: impl Display) {
    writeln!(content, "{key}={value}").expect("writing into String cannot fail");
}

pub fn push_bool(content: &mut String, key: &str, enabled: bool) {
    push_line(content, key, if enabled { 1 } else { 0 });
}
