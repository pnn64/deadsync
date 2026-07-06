use super::{BiasCfg, get};
use deadsync_config::null_or_die::{NullOrDieOptions, null_or_die_bias_cfg as build_bias_cfg};

pub fn null_or_die_bias_cfg() -> BiasCfg {
    let cfg = get();
    build_bias_cfg(NullOrDieOptions {
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
    })
}
