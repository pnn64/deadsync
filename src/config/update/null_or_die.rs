use super::*;

pub fn update_null_or_die_sync_graph(mode: SyncGraphMode) {
    update_config_value(mode, |cfg| &mut cfg.null_or_die_sync_graph);
}

pub fn update_null_or_die_confidence_percent(value: u8) {
    let value = clamp_null_or_die_confidence_percent(value);
    update_config_value(value, |cfg| &mut cfg.null_or_die_confidence_percent);
}

pub fn update_null_or_die_pack_sync_threads(threads: u8) {
    update_config_value(threads, |cfg| &mut cfg.null_or_die_pack_sync_threads);
}

pub fn update_null_or_die_fingerprint_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    update_config_f64(value, |cfg| &mut cfg.null_or_die_fingerprint_ms);
}

pub fn update_null_or_die_window_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    update_config_f64(value, |cfg| &mut cfg.null_or_die_window_ms);
}

pub fn update_null_or_die_step_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    update_config_f64(value, |cfg| &mut cfg.null_or_die_step_ms);
}

pub fn update_null_or_die_magic_offset_ms(value: f64) {
    let value = clamp_null_or_die_magic_offset_ms(value);
    update_config_f64(value, |cfg| &mut cfg.null_or_die_magic_offset_ms);
}

pub fn update_null_or_die_kernel_target(value: KernelTarget) {
    update_config_value(value, |cfg| &mut cfg.null_or_die_kernel_target);
}

pub fn update_null_or_die_kernel_type(value: BiasKernel) {
    update_config_value(value, |cfg| &mut cfg.null_or_die_kernel_type);
}

pub fn update_null_or_die_full_spectrogram(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.null_or_die_full_spectrogram);
}
