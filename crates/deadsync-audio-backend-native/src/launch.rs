use deadsync_audio::{AudioOutputMode, OutputDeviceInfo};

#[cfg(target_os = "linux")]
use deadsync_audio::LinuxAudioBackend;
#[cfg(windows)]
use deadsync_audio::{
    AudioRenderMaps, InitConfig, OutputBackendReady, QueuedSfx, ring as internal,
};
#[cfg(windows)]
use log::{debug, info, warn};
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::sync::mpsc::{SyncSender, sync_channel};

pub const SFX_QUEUE_CAP: usize = 128;

#[derive(Clone, Debug)]
pub struct OutputDeviceProbe {
    pub info: OutputDeviceInfo,
    #[cfg(target_os = "freebsd")]
    pub freebsd_dsp_path: Option<String>,
}

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

#[cfg(windows)]
pub fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, NativeBackendLaunch) {
    let devices = match crate::windows_wasapi::enumerate_output_devices() {
        Ok(devices) => devices,
        Err(err) => {
            warn!("Failed to enumerate WASAPI output devices at startup: {err}");
            Vec::new()
        }
    };
    if devices.is_empty() {
        warn!(
            "No WASAPI output devices were enumerated at startup; native audio will use the system default device."
        );
    }
    let device_probes: Vec<_> = devices
        .iter()
        .map(|device| OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: device.name.clone(),
                is_default: device.is_default,
                sample_rates_hz: device.sample_rates_hz.clone(),
            },
        })
        .collect();
    let output_mode = cfg.output_mode;
    let requested_rate_hz = cfg.sample_rate_hz;
    let default_device = devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first());
    let requested_device = cfg
        .output_device_index
        .and_then(|idx| devices.get(idx as usize));
    if let Some(requested_idx) = cfg.output_device_index {
        if let Some(device) = requested_device {
            info!(
                "Audio output device override selected: index {} '{}'.",
                requested_idx, device.name
            );
        } else {
            warn!(
                "Audio output device override index {} not found; using default device.",
                requested_idx
            );
        }
    }
    let selected_device = requested_device.or(default_device);
    let device_name = selected_device
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "Default Audio Device".to_string());
    let device_id = selected_device.map(|device| device.id.clone());
    let native_sample_rate_hz = selected_device.map_or(48_000, |device| device.mix_rate_hz);
    let native_channels = selected_device.map_or(2, |device| device.channels);
    debug!(
        "Audio device: '{}' (native={} Hz, channels={}).",
        device_name, native_sample_rate_hz, native_channels
    );
    debug!(
        "Audio output stream config: {} Hz request, mode={} (WASAPI native path).",
        requested_rate_hz.unwrap_or(native_sample_rate_hz),
        output_mode.as_str()
    );
    (
        device_probes,
        NativeBackendLaunch {
            wasapi: Some(WasapiBackendHint {
                device_id,
                device_name,
                requested_rate_hz,
                output_mode,
            }),
        },
    )
}

#[cfg(windows)]
pub fn start_wasapi_backend(
    wasapi: WasapiBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
    render_maps: AudioRenderMaps,
) -> Result<
    (
        crate::windows_wasapi::WasapiOutputStream,
        OutputBackendReady,
        SyncSender<QueuedSfx>,
    ),
    String,
> {
    let access_mode = match wasapi.output_mode {
        AudioOutputMode::Exclusive => crate::windows_wasapi::WasapiAccessMode::Exclusive,
        AudioOutputMode::Auto | AudioOutputMode::Shared => {
            crate::windows_wasapi::WasapiAccessMode::Shared
        }
    };
    let prep = crate::windows_wasapi::prepare(
        wasapi.device_id.clone(),
        wasapi.device_name.clone(),
        wasapi.requested_rate_hz,
        access_mode,
    )
    .map_err(|err| {
        format!(
            "failed to prepare native WASAPI output for '{}': {err}",
            wasapi.device_name
        )
    })?;
    let mut ready = prep.ready();
    ready.requested_output_mode = wasapi.output_mode;
    let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
    let stream = crate::windows_wasapi::start(prep, music_ring, sfx_receiver, render_maps)
        .map_err(|err| {
            format!(
                "failed to start native WASAPI output for '{}': {err}",
                wasapi.device_name
            )
        })?;
    Ok((stream, ready, sfx_sender))
}
