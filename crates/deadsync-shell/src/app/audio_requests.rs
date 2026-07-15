use deadsync_config::prelude as config;
use deadsync_theme::views::{AudioOptionsView, AudioOutputDeviceView};
use deadsync_theme::{AudioOutputModeChoice, AudioRequest, AudioVolumeTarget};

pub(super) fn options_view() -> AudioOptionsView {
    let cfg = config::get();
    let output_devices = if deadsync_audio_stream::is_initialized() {
        deadsync_audio_stream::startup_output_devices()
            .into_iter()
            .map(|device| AudioOutputDeviceView {
                name: device.name,
                is_default: device.is_default,
                sample_rates_hz: device.sample_rates_hz,
            })
            .collect()
    } else {
        Vec::new()
    };
    #[cfg(target_os = "linux")]
    let (available_backend_names, selected_backend_name) = (
        deadsync_audio_stream::available_linux_backends()
            .into_iter()
            .map(|backend| backend.as_str().to_owned())
            .collect(),
        cfg.linux_audio_backend.as_str().to_owned(),
    );
    #[cfg(not(target_os = "linux"))]
    let (available_backend_names, selected_backend_name) = (Vec::new(), String::new());

    AudioOptionsView {
        output_devices,
        available_backend_names,
        output_device: cfg.audio_output_device_index,
        output_mode: output_mode_choice(cfg.audio_output_mode),
        selected_backend_name,
        sample_rate_hz: cfg.audio_sample_rate_hz,
        preserve_pitch: cfg.rate_mod_preserves_pitch,
        replay_gain: cfg.enable_replaygain,
        master_volume: cfg.master_volume,
        music_volume: cfg.music_volume,
        sfx_volume: cfg.sfx_volume,
        assist_tick_volume: cfg.assist_tick_volume,
    }
}

const fn output_mode_choice(mode: config::AudioOutputMode) -> AudioOutputModeChoice {
    match mode {
        config::AudioOutputMode::Auto => AudioOutputModeChoice::Auto,
        config::AudioOutputMode::Shared => AudioOutputModeChoice::Shared,
        config::AudioOutputMode::Exclusive => AudioOutputModeChoice::Exclusive,
    }
}

const fn output_mode(choice: AudioOutputModeChoice) -> config::AudioOutputMode {
    match choice {
        AudioOutputModeChoice::Auto => config::AudioOutputMode::Auto,
        AudioOutputModeChoice::Shared => config::AudioOutputMode::Shared,
        AudioOutputModeChoice::Exclusive => config::AudioOutputMode::Exclusive,
    }
}

#[cfg(target_os = "linux")]
fn linux_backend(name: &str) -> config::LinuxAudioBackend {
    match name {
        "PipeWire" => config::LinuxAudioBackend::PipeWire,
        "PulseAudio" => config::LinuxAudioBackend::PulseAudio,
        "JACK" => config::LinuxAudioBackend::Jack,
        "ALSA" => config::LinuxAudioBackend::Alsa,
        _ => config::LinuxAudioBackend::Auto,
    }
}

pub(super) fn execute(request: AudioRequest) {
    match request {
        AudioRequest::PlaySfx(path) => deadsync_audio_stream::play_sfx(&path),
        AudioRequest::PlayMusic {
            path,
            cut,
            looping,
            rate,
        } => deadsync_audio_stream::play_music(
            path,
            deadsync_audio_stream::Cut {
                start_sec: cut.start_sec,
                length_sec: cut.length_sec,
                fade_in_sec: cut.fade_in_sec,
                fade_out_sec: cut.fade_out_sec,
            },
            looping,
            rate,
        ),
        AudioRequest::StopMusic => deadsync_audio_stream::stop_music(),
        AudioRequest::SetMusicRate(rate) => deadsync_audio_stream::set_music_rate(rate),
        AudioRequest::SetVolume { target, percent } => match target {
            AudioVolumeTarget::Master => config::update_master_volume(percent),
            AudioVolumeTarget::Music => config::update_music_volume(percent),
            AudioVolumeTarget::Sfx => config::update_sfx_volume(percent),
            AudioVolumeTarget::AssistTick => config::update_assist_tick_volume(percent),
        },
        AudioRequest::SetOutputDevice(device) => config::update_audio_output_device(device),
        AudioRequest::SetOutputMode(mode) => config::update_audio_output_mode(output_mode(mode)),
        AudioRequest::SetOutputBackend(name) => {
            #[cfg(target_os = "linux")]
            config::update_linux_audio_backend(linux_backend(&name));
            #[cfg(not(target_os = "linux"))]
            drop(name);
        }
        AudioRequest::SetSampleRate(rate) => config::update_audio_sample_rate(rate),
        AudioRequest::SetMineHitSound(enabled) => config::update_mine_hit_sound(enabled),
        AudioRequest::SetGlobalOffsetMillis(milliseconds) => {
            config::update_global_offset(milliseconds as f32 / 1000.0);
        }
        AudioRequest::SetPreservePitch(enabled) => {
            config::update_rate_mod_preserves_pitch(enabled);
        }
        AudioRequest::SetReplayGain(enabled) => config::update_enable_replaygain(enabled),
        AudioRequest::PrewarmReplayGain(paths) => deadsync_audio_replaygain::prewarm_paths(
            paths,
            deadsync_audio_replaygain::Priority::Background,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_output_modes_round_trip_through_shell_mapping() {
        for choice in [
            AudioOutputModeChoice::Auto,
            AudioOutputModeChoice::Shared,
            AudioOutputModeChoice::Exclusive,
        ] {
            assert_eq!(output_mode_choice(output_mode(choice)), choice);
        }
    }
}
