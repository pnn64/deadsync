use super::*;

pub fn update_master_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.master_volume == vol {
            return;
        }
        cfg.master_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_music_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.music_volume == vol {
            return;
        }
        cfg.music_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_menu_music(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.menu_music == enabled {
            return;
        }
        cfg.menu_music = enabled;
    }
    save_without_keymaps();
}

pub fn update_software_renderer_threads(threads: u8) {
    {
        let mut cfg = lock_config();
        if cfg.software_renderer_threads == threads {
            return;
        }
        cfg.software_renderer_threads = threads;
    }
    save_without_keymaps();
}

pub fn update_sfx_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.sfx_volume == vol {
            return;
        }
        cfg.sfx_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_assist_tick_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.assist_tick_volume == vol {
            return;
        }
        cfg.assist_tick_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_audio_sample_rate(rate: Option<u32>) {
    {
        let mut cfg = lock_config();
        if cfg.audio_sample_rate_hz == rate {
            return;
        }
        cfg.audio_sample_rate_hz = rate;
    }
    save_without_keymaps();
}

pub fn update_audio_output_device(index: Option<u16>) {
    {
        let mut cfg = lock_config();
        if cfg.audio_output_device_index == index {
            return;
        }
        cfg.audio_output_device_index = index;
    }
    save_without_keymaps();
}

pub fn update_audio_output_mode(mode: AudioOutputMode) {
    {
        let mut cfg = lock_config();
        if cfg.audio_output_mode == mode {
            return;
        }
        cfg.audio_output_mode = mode;
    }
    save_without_keymaps();
}

#[cfg(target_os = "linux")]
pub fn update_linux_audio_backend(backend: LinuxAudioBackend) {
    {
        let mut cfg = lock_config();
        if cfg.linux_audio_backend == backend {
            return;
        }
        cfg.linux_audio_backend = backend;
    }
    save_without_keymaps();
}

pub fn update_mine_hit_sound(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.mine_hit_sound == enabled {
            return;
        }
        cfg.mine_hit_sound = enabled;
    }
    save_without_keymaps();
}

pub fn update_music_wheel_switch_speed(speed: u8) {
    let speed = speed.max(1);
    {
        let mut cfg = lock_config();
        if cfg.music_wheel_switch_speed == speed {
            return;
        }
        cfg.music_wheel_switch_speed = speed;
    }
    save_without_keymaps();
}

pub fn update_translated_titles(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.translated_titles == enabled {
            return;
        }
        cfg.translated_titles = enabled;
    }
    save_without_keymaps();
}

pub fn update_rate_mod_preserves_pitch(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.rate_mod_preserves_pitch == enabled {
            return;
        }
        cfg.rate_mod_preserves_pitch = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_replaygain(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_replaygain == enabled {
            return;
        }
        cfg.enable_replaygain = enabled;
    }
    crate::engine::audio::on_replaygain_setting_changed(enabled);
    save_without_keymaps();
}
