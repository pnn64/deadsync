use super::*;
use deadsync_config::audio::{clamp_audio_volume_percent, clamp_music_wheel_switch_speed};

pub fn update_master_volume(volume: u8) {
    let vol = clamp_audio_volume_percent(volume);
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
    let vol = clamp_audio_volume_percent(volume);
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
    update_config_value(enabled, |cfg| &mut cfg.menu_music);
}

pub fn update_software_renderer_threads(threads: u8) {
    update_config_value(threads, |cfg| &mut cfg.software_renderer_threads);
}

pub fn update_sfx_volume(volume: u8) {
    let vol = clamp_audio_volume_percent(volume);
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
    let vol = clamp_audio_volume_percent(volume);
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
    update_config_value(rate, |cfg| &mut cfg.audio_sample_rate_hz);
}

pub fn update_audio_output_device(index: Option<u16>) {
    update_config_value(index, |cfg| &mut cfg.audio_output_device_index);
}

pub fn update_audio_output_mode(mode: AudioOutputMode) {
    update_config_value(mode, |cfg| &mut cfg.audio_output_mode);
}

#[cfg(target_os = "linux")]
pub fn update_linux_audio_backend(backend: LinuxAudioBackend) {
    update_config_value(backend, |cfg| &mut cfg.linux_audio_backend);
}

pub fn update_mine_hit_sound(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.mine_hit_sound);
}

pub fn update_music_wheel_switch_speed(speed: u8) {
    let speed = clamp_music_wheel_switch_speed(speed);
    update_config_value(speed, |cfg| &mut cfg.music_wheel_switch_speed);
}

pub fn update_translated_titles(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.translated_titles);
}

pub fn update_rate_mod_preserves_pitch(enabled: bool) {
    if update_config_value(enabled, |cfg| &mut cfg.rate_mod_preserves_pitch) {
        deadsync_audio_stream::set_preserve_pitch_enabled(enabled);
    }
}

pub fn update_enable_replaygain(enabled: bool) {
    if update_config_value(enabled, |cfg| &mut cfg.enable_replaygain) {
        deadsync_audio_stream::set_replaygain_enabled(enabled);
    }
}
