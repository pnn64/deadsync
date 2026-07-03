use super::{BiasCfg, get};
use deadsync_config::null_or_die::{
    clamp_null_or_die_magic_offset_ms, clamp_null_or_die_positive_ms,
};

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
