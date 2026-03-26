use super::*;

pub fn update_null_or_die_sync_graph(mode: SyncGraphMode) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_sync_graph == mode {
            return;
        }
        cfg.null_or_die_sync_graph = mode;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_confidence_percent(value: u8) {
    let value = clamp_null_or_die_confidence_percent(value);
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_confidence_percent == value {
            return;
        }
        cfg.null_or_die_confidence_percent = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_fingerprint_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_fingerprint_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_fingerprint_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_window_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_window_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_window_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_step_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_step_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_step_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_magic_offset_ms(value: f64) {
    let value = clamp_null_or_die_magic_offset_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_magic_offset_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_magic_offset_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_kernel_target(value: KernelTarget) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_kernel_target == value {
            return;
        }
        cfg.null_or_die_kernel_target = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_kernel_type(value: BiasKernel) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_kernel_type == value {
            return;
        }
        cfg.null_or_die_kernel_type = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_full_spectrogram(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_full_spectrogram == enabled {
            return;
        }
        cfg.null_or_die_full_spectrogram = enabled;
    }
    save_without_keymaps();
}
