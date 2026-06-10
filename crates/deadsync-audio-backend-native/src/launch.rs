use deadsync_audio::AudioOutputMode;

#[cfg(target_os = "linux")]
use deadsync_audio::LinuxAudioBackend;

pub const SFX_QUEUE_CAP: usize = 128;

#[cfg(windows)]
#[derive(Clone, Debug)]
pub struct WasapiBackendHint {
    pub device_id: Option<String>,
    pub device_name: String,
    pub requested_rate_hz: Option<u32>,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
pub struct AlsaBackendHint {
    pub pcm_id: Option<String>,
    pub device_name: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_jack_audio)]
#[derive(Clone, Debug)]
pub struct JackBackendHint {
    pub requested_device_name: Option<String>,
    pub requested_rate_hz: Option<u32>,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_pipewire_audio)]
#[derive(Clone, Debug)]
pub struct PipeWireBackendHint {
    pub requested_device_name: Option<String>,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_pulse_audio)]
#[derive(Clone, Debug)]
pub struct PulseBackendHint {
    pub requested_device_name: Option<String>,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
pub struct CoreAudioBackendHint {
    pub device_uid: Option<String>,
    pub device_name: String,
    pub requested_rate_hz: Option<u32>,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "freebsd")]
#[derive(Clone, Debug)]
pub struct FreeBsdPcmBackendHint {
    pub dsp_path: Option<String>,
    pub device_name: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[derive(Clone, Debug)]
pub struct NativeBackendLaunch {
    #[cfg(target_os = "linux")]
    pub explicit_device_requested: bool,
    #[cfg(target_os = "linux")]
    pub linux_backend: LinuxAudioBackend,
    #[cfg(target_os = "linux")]
    pub alsa: Option<AlsaBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(has_jack_audio)]
    pub jack: Option<JackBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(has_pipewire_audio)]
    pub pipewire: Option<PipeWireBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(has_pulse_audio)]
    pub pulse: Option<PulseBackendHint>,
    #[cfg(target_os = "macos")]
    pub coreaudio: Option<CoreAudioBackendHint>,
    #[cfg(target_os = "freebsd")]
    pub freebsd_pcm: Option<FreeBsdPcmBackendHint>,
    #[cfg(windows)]
    pub wasapi: Option<WasapiBackendHint>,
}
