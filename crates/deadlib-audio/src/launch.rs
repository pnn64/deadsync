use deadlib_audio_core::{
    AudioOutputMode, AudioStreamHandle, MixControls, OutputBackendReady, OutputDeviceInfo,
    RenderState, SfxReceiver, SfxSender, music_transport, sfx_transport,
};

#[cfg(target_os = "linux")]
use deadlib_audio_backend_alsa as linux_alsa;
#[cfg(target_os = "macos")]
use deadlib_audio_backend_coreaudio as macos_coreaudio;
#[cfg(target_os = "freebsd")]
use deadlib_audio_backend_freebsd_pcm as freebsd_pcm;
#[cfg(target_os = "linux")]
use deadlib_audio_backend_jack as linux_jack;
#[cfg(target_os = "linux")]
#[cfg(feature = "pipewire-audio")]
use deadlib_audio_backend_pipewire as linux_pipewire;
#[cfg(target_os = "linux")]
use deadlib_audio_backend_pulse as linux_pulse;
#[cfg(windows)]
use deadlib_audio_backend_wasapi as windows_wasapi;
use log::{debug, info, warn};
use std::str::FromStr;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitConfig {
    pub output_device_index: Option<u16>,
    pub output_mode: AudioOutputMode,
    #[cfg(target_os = "linux")]
    pub linux_backend: LinuxAudioBackend,
    pub sample_rate_hz: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxAudioBackend {
    Auto,
    PipeWire,
    PulseAudio,
    Jack,
    Alsa,
}

impl LinuxAudioBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::PipeWire => "PipeWire",
            Self::PulseAudio => "PulseAudio",
            Self::Jack => "JACK",
            Self::Alsa => "ALSA",
        }
    }
}

impl FromStr for LinuxAudioBackend {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "pipewire" | "pipe-wire" | "pw" => Ok(Self::PipeWire),
            "pulseaudio" | "pulse" => Ok(Self::PulseAudio),
            "jack" => Ok(Self::Jack),
            "alsa" => Ok(Self::Alsa),
            _ => Err(()),
        }
    }
}

pub const SFX_QUEUE_CAP: usize = 128;

fn output_transport(
    device_channels: usize,
    controls: &Arc<MixControls>,
) -> (AudioStreamHandle, RenderState, SfxSender, SfxReceiver) {
    let (stream_handle, render_handle) = music_transport(device_channels);
    let (sfx_sender, sfx_receiver) = sfx_transport(SFX_QUEUE_CAP);
    (
        stream_handle,
        RenderState::new(render_handle, Arc::clone(controls), device_channels),
        sfx_sender,
        sfx_receiver,
    )
}

#[derive(Clone, Debug)]
struct OutputDeviceProbe {
    pub info: OutputDeviceInfo,
    #[cfg(target_os = "freebsd")]
    pub freebsd_dsp_path: Option<String>,
}

#[cfg(windows)]
#[derive(Clone, Debug)]
struct WasapiBackendHint {
    pub device_id: Option<String>,
    pub device_name: String,
    pub requested_rate_hz: Option<u32>,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
struct AlsaBackendHint {
    pub pcm_id: Option<String>,
    pub device_name: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
struct JackBackendHint {
    pub requested_device_name: Option<String>,
    pub requested_rate_hz: Option<u32>,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(feature = "pipewire-audio")]
#[derive(Clone, Debug)]
struct PipeWireBackendHint {
    pub requested_device_name: Option<String>,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
struct PulseBackendHint {
    pub requested_device_name: Option<String>,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct CoreAudioBackendHint {
    pub device_uid: Option<String>,
    pub device_name: String,
    pub requested_rate_hz: Option<u32>,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[cfg(target_os = "freebsd")]
#[derive(Clone, Debug)]
struct FreeBsdPcmBackendHint {
    pub dsp_path: Option<String>,
    pub device_name: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub output_mode: AudioOutputMode,
}

#[derive(Clone, Debug)]
struct NativeBackendLaunch {
    #[cfg(target_os = "linux")]
    pub explicit_device_requested: bool,
    #[cfg(target_os = "linux")]
    pub linux_backend: LinuxAudioBackend,
    #[cfg(target_os = "linux")]
    pub alsa: Option<AlsaBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "linux")]
    pub jack: Option<JackBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(feature = "pipewire-audio")]
    pub pipewire: Option<PipeWireBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "linux")]
    pub pulse: Option<PulseBackendHint>,
    #[cfg(target_os = "macos")]
    pub coreaudio: Option<CoreAudioBackendHint>,
    #[cfg(target_os = "freebsd")]
    pub freebsd_pcm: Option<FreeBsdPcmBackendHint>,
    #[cfg(windows)]
    pub wasapi: Option<WasapiBackendHint>,
}

enum NativeOutputBackend {
    #[cfg(target_os = "linux")]
    Alsa {
        _stream: linux_alsa::AlsaOutputStream,
    },
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "linux")]
    Jack {
        _stream: linux_jack::JackOutputStream,
    },
    #[cfg(target_os = "linux")]
    #[cfg(feature = "pipewire-audio")]
    PipeWire {
        _stream: linux_pipewire::PipeWireOutputStream,
    },
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "linux")]
    Pulse {
        _stream: linux_pulse::PulseOutputStream,
    },
    #[cfg(target_os = "macos")]
    CoreAudio {
        _stream: macos_coreaudio::CoreAudioOutputStream,
    },
    #[cfg(target_os = "freebsd")]
    FreeBsdPcm {
        _stream: freebsd_pcm::FreeBsdPcmOutputStream,
    },
    #[cfg(windows)]
    Wasapi {
        _stream: windows_wasapi::WasapiOutputStream,
    },
}

/// Opaque result of device probing and backend selection.
///
/// Preparing a plan performs discovery on the caller thread. Opening it moves
/// all backend-specific state into this crate and starts exactly one output
/// session.
pub struct OutputPlan {
    devices: Vec<OutputDeviceInfo>,
    launch: NativeBackendLaunch,
    controls: Arc<MixControls>,
}

impl OutputPlan {
    pub fn devices(&self) -> &[OutputDeviceInfo] {
        &self.devices
    }

    pub fn open(self) -> Result<OpenedOutput, String> {
        let (backend, ready, sfx_sender, stream_handle) =
            start_output_backend(self.launch, self.controls)?;
        Ok(OpenedOutput {
            session: OutputSession { _backend: backend },
            ready,
            sfx_sender,
            stream_handle,
        })
    }
}

/// Live native output ownership. Dropping it stops and joins the backend.
pub struct OutputSession {
    _backend: NativeOutputBackend,
}

pub struct OpenedOutput {
    session: OutputSession,
    ready: OutputBackendReady,
    sfx_sender: SfxSender,
    stream_handle: AudioStreamHandle,
}

impl OpenedOutput {
    pub fn into_parts(
        self,
    ) -> (
        OutputSession,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ) {
        (
            self.session,
            self.ready,
            self.sfx_sender,
            self.stream_handle,
        )
    }
}

pub fn prepare_output(cfg: &InitConfig, controls: Arc<MixControls>) -> OutputPlan {
    let (probes, launch) = build_audio_launch(cfg);
    OutputPlan {
        devices: probes.into_iter().map(|probe| probe.info).collect(),
        launch,
        controls,
    }
}

#[cfg(target_os = "linux")]
pub fn available_linux_backends() -> Vec<LinuxAudioBackend> {
    let mut backends = Vec::with_capacity(5);
    backends.push(LinuxAudioBackend::Auto);
    #[cfg(feature = "pipewire-audio")]
    backends.push(LinuxAudioBackend::PipeWire);
    #[cfg(target_os = "linux")]
    if linux_pulse::is_available() {
        backends.push(LinuxAudioBackend::PulseAudio);
    }
    backends.push(LinuxAudioBackend::Alsa);
    #[cfg(target_os = "linux")]
    if linux_jack::is_available() {
        backends.push(LinuxAudioBackend::Jack);
    }
    backends
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn linux_default_output_device(
    devices: &[linux_alsa::AlsaOutputDevice],
) -> Option<&linux_alsa::AlsaOutputDevice> {
    devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first())
}

#[cfg(target_os = "linux")]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, NativeBackendLaunch) {
    let alsa_devices = linux_alsa::enumerate_output_devices();
    if alsa_devices.is_empty() {
        warn!(
            "No ALSA playback devices were enumerated at startup; Linux audio will rely on backend defaults."
        );
    }
    let device_probes: Vec<_> = alsa_devices
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
    let linux_backend = cfg.linux_backend;
    let default_device = linux_default_output_device(&alsa_devices);
    let requested_device = cfg
        .output_device_index
        .and_then(|idx| alsa_devices.get(idx as usize));
    let explicit_device_requested = requested_device.is_some();
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
    let selected_device = requested_device.or_else(|| {
        (matches!(output_mode, AudioOutputMode::Exclusive)
            && matches!(
                linux_backend,
                LinuxAudioBackend::Auto | LinuxAudioBackend::Alsa
            ))
        .then_some(default_device)
        .flatten()
    });
    let (device_name, alsa_pcm_id) = if let Some(device) = selected_device {
        if !explicit_device_requested {
            info!(
                "Audio output device auto-selected for ALSA exclusive mode: '{}' ({})",
                device.name, device.pcm_id
            );
        }
        (device.name.clone(), Some(device.pcm_id.clone()))
    } else {
        ("Default Audio Device".to_string(), None)
    };
    let fallback_device = selected_device.or(default_device);
    let native_sample_rate_hz = cfg
        .sample_rate_hz
        .unwrap_or_else(|| fallback_device.map_or(48_000, |device| device.default_rate_hz));
    let native_channels = fallback_device.map_or(2, |device| device.channels);
    debug!(
        "Audio device: '{}' (native={} Hz, channels={}).",
        device_name, native_sample_rate_hz, native_channels
    );
    debug!(
        "Audio output stream config: {} Hz, {} ch, mode={} (Linux native path).",
        native_sample_rate_hz,
        native_channels,
        output_mode.as_str()
    );
    (
        device_probes,
        NativeBackendLaunch {
            explicit_device_requested,
            linux_backend,
            alsa: Some(AlsaBackendHint {
                pcm_id: alsa_pcm_id,
                device_name: device_name.clone(),
                sample_rate_hz: native_sample_rate_hz,
                channels: native_channels,
                output_mode,
            }),
            #[cfg(target_os = "linux")]
            jack: Some(JackBackendHint {
                requested_device_name: explicit_device_requested.then_some(device_name.clone()),
                requested_rate_hz: cfg.sample_rate_hz,
                output_mode,
            }),
            #[cfg(feature = "pipewire-audio")]
            pipewire: Some(PipeWireBackendHint {
                requested_device_name: explicit_device_requested.then_some(device_name.clone()),
                sample_rate_hz: native_sample_rate_hz,
                channels: native_channels,
                output_mode,
            }),
            #[cfg(target_os = "linux")]
            pulse: Some(PulseBackendHint {
                requested_device_name: explicit_device_requested.then_some(device_name),
                sample_rate_hz: native_sample_rate_hz,
                channels: native_channels,
                output_mode,
            }),
        },
    )
}

#[cfg(target_os = "macos")]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, NativeBackendLaunch) {
    let devices = macos_coreaudio::enumerate_output_devices();
    if devices.is_empty() {
        warn!(
            "No CoreAudio output devices were enumerated at startup; native audio will use the system default device."
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
    let device_uid = selected_device.map(|device| device.uid.clone());
    let requested_rate_hz = cfg.sample_rate_hz;
    let native_sample_rate_hz = requested_rate_hz
        .unwrap_or_else(|| selected_device.map_or(48_000, |device| device.default_rate_hz));
    let native_channels = selected_device.map_or(2, |device| device.channels);
    debug!(
        "Audio device: '{}' (native={} Hz, channels={}).",
        device_name, native_sample_rate_hz, native_channels
    );
    debug!(
        "Audio output stream config: {} Hz, {} ch, mode={} (CoreAudio native path).",
        native_sample_rate_hz,
        native_channels,
        output_mode.as_str()
    );
    (
        device_probes,
        NativeBackendLaunch {
            coreaudio: Some(CoreAudioBackendHint {
                device_uid,
                device_name,
                requested_rate_hz,
                channels: native_channels,
                output_mode,
            }),
        },
    )
}

#[cfg(target_os = "freebsd")]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, NativeBackendLaunch) {
    let mut device_probes: Vec<_> = freebsd_pcm::enumerate_output_devices()
        .into_iter()
        .map(|dev| OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: dev.name,
                is_default: dev.is_default,
                sample_rates_hz: Vec::new(),
            },
            freebsd_dsp_path: Some(dev.path),
        })
        .collect();
    let output_mode = cfg.output_mode;
    let mut device_name = device_probes
        .iter()
        .find(|probe| probe.info.is_default)
        .map(|probe| probe.info.name.clone())
        .unwrap_or_else(|| "FreeBSD PCM default".to_string());
    let mut dsp_path = device_probes
        .iter()
        .find(|probe| probe.info.is_default)
        .and_then(|probe| probe.freebsd_dsp_path.clone());
    if let Some(requested_idx) = cfg.output_device_index {
        if let Some(probe) = device_probes.get(requested_idx as usize) {
            device_name = probe.info.name.clone();
            dsp_path = probe.freebsd_dsp_path.clone();
            info!(
                "Audio output device override selected: index {} '{}'.",
                requested_idx, device_name
            );
        } else {
            warn!(
                "Audio output device override index {} not found; using default device.",
                requested_idx
            );
        }
    }
    if device_probes.is_empty() {
        warn!(
            "No FreeBSD PCM devices were enumerated at startup; native audio will still try /dev/dsp."
        );
        device_probes.push(OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: "FreeBSD PCM (/dev/dsp)".to_string(),
                is_default: true,
                sample_rates_hz: Vec::new(),
            },
            freebsd_dsp_path: Some("/dev/dsp".to_string()),
        });
        if dsp_path.is_none() {
            dsp_path = Some("/dev/dsp".to_string());
            device_name = "FreeBSD PCM (/dev/dsp)".to_string();
        }
    }
    let sample_rate_hz = cfg.sample_rate_hz.unwrap_or(48_000).max(1);
    debug!(
        "FreeBSD PCM device '{}' selected at {} Hz, 2 ch, mode={}.",
        device_name,
        sample_rate_hz,
        output_mode.as_str()
    );
    (
        device_probes,
        NativeBackendLaunch {
            #[cfg(target_os = "linux")]
            explicit_device_requested: false,
            #[cfg(target_os = "linux")]
            linux_backend: cfg.linux_backend,
            #[cfg(target_os = "linux")]
            alsa: None,
            #[cfg(target_os = "linux")]
            #[cfg(target_os = "linux")]
            jack: None,
            #[cfg(target_os = "linux")]
            #[cfg(feature = "pipewire-audio")]
            pipewire: None,
            #[cfg(target_os = "linux")]
            #[cfg(target_os = "linux")]
            pulse: None,
            #[cfg(target_os = "macos")]
            coreaudio: None,
            freebsd_pcm: Some(FreeBsdPcmBackendHint {
                dsp_path,
                device_name,
                sample_rate_hz,
                channels: 2,
                output_mode,
            }),
            #[cfg(windows)]
            wasapi: None,
        },
    )
}

#[cfg(windows)]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, NativeBackendLaunch) {
    let devices = match windows_wasapi::enumerate_output_devices() {
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

#[cfg(target_os = "linux")]
fn start_linux_alsa_backend(
    alsa: AlsaBackendHint,
    controls: &Arc<MixControls>,
) -> Result<
    (
        NativeOutputBackend,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    let access_mode = match alsa.output_mode {
        AudioOutputMode::Exclusive => linux_alsa::AlsaAccessMode::Exclusive,
        AudioOutputMode::Auto | AudioOutputMode::Shared => linux_alsa::AlsaAccessMode::Shared,
    };
    let prep = linux_alsa::prepare(
        alsa.pcm_id.clone(),
        alsa.device_name.clone(),
        alsa.sample_rate_hz,
        alsa.channels,
        access_mode,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = alsa.output_mode;
    let (stream_handle, render, sfx_sender, sfx_receiver) =
        output_transport(ready.device_channels, controls);
    let stream = linux_alsa::start(prep, render, sfx_receiver)?;
    Ok((
        NativeOutputBackend::Alsa { _stream: stream },
        ready,
        sfx_sender,
        stream_handle,
    ))
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
fn start_linux_jack_backend(
    jack: JackBackendHint,
    controls: &Arc<MixControls>,
) -> Result<
    (
        NativeOutputBackend,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    if matches!(jack.output_mode, AudioOutputMode::Exclusive) {
        return Err("JACK does not expose a separate exclusive output mode.".to_string());
    }
    let prep = linux_jack::prepare(jack.requested_device_name.clone(), jack.requested_rate_hz)?;
    let mut ready = prep.ready();
    ready.requested_output_mode = jack.output_mode;
    let (stream_handle, render, sfx_sender, sfx_receiver) =
        output_transport(ready.device_channels, controls);
    let stream = linux_jack::start(prep, render, sfx_receiver)?;
    Ok((
        NativeOutputBackend::Jack { _stream: stream },
        ready,
        sfx_sender,
        stream_handle,
    ))
}

#[cfg(target_os = "linux")]
#[cfg(feature = "pipewire-audio")]
fn start_linux_pipewire_backend(
    pipewire: PipeWireBackendHint,
    controls: &Arc<MixControls>,
) -> Result<
    (
        NativeOutputBackend,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    if matches!(pipewire.output_mode, AudioOutputMode::Exclusive) {
        return Err("PipeWire does not support a separate exclusive output mode.".to_string());
    }
    if let Some(name) = &pipewire.requested_device_name {
        warn!(
            "PipeWire backend ignores explicit Sound Device selection '{}'; using the default PipeWire sink.",
            name
        );
    }
    let prep = linux_pipewire::prepare(
        pipewire.requested_device_name.clone(),
        pipewire.sample_rate_hz,
        pipewire.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = pipewire.output_mode;
    let (stream_handle, render, sfx_sender, sfx_receiver) =
        output_transport(ready.device_channels, controls);
    let stream = linux_pipewire::start(prep, render, sfx_receiver)?;
    Ok((
        NativeOutputBackend::PipeWire { _stream: stream },
        ready,
        sfx_sender,
        stream_handle,
    ))
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
fn start_linux_pulse_backend(
    pulse: PulseBackendHint,
    controls: &Arc<MixControls>,
) -> Result<
    (
        NativeOutputBackend,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    if matches!(pulse.output_mode, AudioOutputMode::Exclusive) {
        return Err("PulseAudio does not support exclusive output.".to_string());
    }
    if let Some(name) = &pulse.requested_device_name {
        warn!(
            "PulseAudio backend ignores explicit Sound Device selection '{}'; using the default PulseAudio sink.",
            name
        );
    }
    let prep = linux_pulse::prepare(
        pulse.requested_device_name.clone(),
        pulse.sample_rate_hz,
        pulse.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = pulse.output_mode;
    let (stream_handle, render, sfx_sender, sfx_receiver) =
        output_transport(ready.device_channels, controls);
    let stream = linux_pulse::start(prep, render, sfx_receiver)?;
    Ok((
        NativeOutputBackend::Pulse { _stream: stream },
        ready,
        sfx_sender,
        stream_handle,
    ))
}

#[cfg(target_os = "freebsd")]
fn start_freebsd_pcm_backend(
    pcm: FreeBsdPcmBackendHint,
    controls: &Arc<MixControls>,
) -> Result<
    (
        NativeOutputBackend,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    if matches!(pcm.output_mode, AudioOutputMode::Exclusive) {
        return Err("FreeBSD PCM exclusive output is not implemented yet.".to_string());
    }
    let prep = freebsd_pcm::prepare(
        pcm.dsp_path.clone(),
        pcm.device_name.clone(),
        pcm.sample_rate_hz,
        pcm.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = pcm.output_mode;
    let (stream_handle, render, sfx_sender, sfx_receiver) =
        output_transport(ready.device_channels, controls);
    let stream = freebsd_pcm::start(prep, render, sfx_receiver)?;
    Ok((
        NativeOutputBackend::FreeBsdPcm { _stream: stream },
        ready,
        sfx_sender,
        stream_handle,
    ))
}

#[cfg(target_os = "macos")]
fn start_macos_coreaudio_backend(
    coreaudio: CoreAudioBackendHint,
    controls: &Arc<MixControls>,
) -> Result<
    (
        NativeOutputBackend,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    if matches!(coreaudio.output_mode, AudioOutputMode::Exclusive) {
        return Err("CoreAudio exclusive output is not implemented yet.".to_string());
    }
    let prep = macos_coreaudio::prepare(
        coreaudio.device_uid.clone(),
        coreaudio.device_name.clone(),
        coreaudio.requested_rate_hz,
        coreaudio.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = coreaudio.output_mode;
    let (stream_handle, render, sfx_sender, sfx_receiver) =
        output_transport(ready.device_channels, controls);
    let stream = macos_coreaudio::start(prep, render, sfx_receiver)?;
    Ok((
        NativeOutputBackend::CoreAudio { _stream: stream },
        ready,
        sfx_sender,
        stream_handle,
    ))
}

fn start_output_backend(
    launch: NativeBackendLaunch,
    controls: Arc<MixControls>,
) -> Result<
    (
        NativeOutputBackend,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    let NativeBackendLaunch {
        #[cfg(target_os = "linux")]
        explicit_device_requested,
        #[cfg(target_os = "linux")]
        linux_backend,
        #[cfg(target_os = "linux")]
        alsa,
        #[cfg(target_os = "linux")]
        #[cfg(target_os = "linux")]
        jack,
        #[cfg(target_os = "linux")]
        #[cfg(feature = "pipewire-audio")]
        pipewire,
        #[cfg(target_os = "linux")]
        #[cfg(target_os = "linux")]
        pulse,
        #[cfg(target_os = "macos")]
        coreaudio,
        #[cfg(target_os = "freebsd")]
        freebsd_pcm,
        #[cfg(windows)]
        wasapi,
    } = launch;
    #[cfg(target_os = "linux")]
    let requested_output_mode = alsa
        .as_ref()
        .map(|hint| hint.output_mode)
        .or({
            #[cfg(target_os = "linux")]
            #[cfg(feature = "pipewire-audio")]
            {
                pipewire.as_ref().map(|hint| hint.output_mode)
            }
            #[cfg(not(all(target_os = "linux", feature = "pipewire-audio")))]
            {
                None
            }
        })
        .or({
            #[cfg(target_os = "linux")]
            #[cfg(target_os = "linux")]
            {
                jack.as_ref().map(|hint| hint.output_mode)
            }
            #[cfg(not(target_os = "linux"))]
            {
                None
            }
        })
        .or({
            #[cfg(target_os = "linux")]
            #[cfg(target_os = "linux")]
            {
                pulse.as_ref().map(|hint| hint.output_mode)
            }
            #[cfg(not(target_os = "linux"))]
            {
                None
            }
        })
        .unwrap_or(AudioOutputMode::Auto);
    #[cfg(target_os = "linux")]
    match linux_backend {
        LinuxAudioBackend::Alsa => {
            let Some(alsa) = alsa else {
                return Err("Linux ALSA backend hint unavailable.".to_string());
            };
            start_linux_alsa_backend(alsa, &controls)
        }
        LinuxAudioBackend::Jack => {
            #[cfg(target_os = "linux")]
            {
                let Some(jack) = jack else {
                    return Err("JACK backend hint unavailable.".to_string());
                };
                start_linux_jack_backend(jack, &controls)
            }
            #[cfg(not(target_os = "linux"))]
            {
                Err("JACK backend support was not built into this binary.".to_string())
            }
        }
        LinuxAudioBackend::PipeWire => {
            #[cfg(feature = "pipewire-audio")]
            {
                let Some(pipewire) = pipewire else {
                    return Err("PipeWire backend hint unavailable.".to_string());
                };
                return start_linux_pipewire_backend(pipewire, &controls);
            }
            #[cfg(not(feature = "pipewire-audio"))]
            {
                Err("PipeWire backend support was not built into this binary.".to_string())
            }
        }
        LinuxAudioBackend::PulseAudio => {
            #[cfg(target_os = "linux")]
            {
                let Some(pulse) = pulse else {
                    return Err("PulseAudio backend hint unavailable.".to_string());
                };
                start_linux_pulse_backend(pulse, &controls)
            }
            #[cfg(not(target_os = "linux"))]
            {
                return Err(
                    "PulseAudio backend support was not built into this binary.".to_string()
                );
            }
        }
        LinuxAudioBackend::Auto => {
            if matches!(requested_output_mode, AudioOutputMode::Exclusive) {
                let Some(alsa) = alsa else {
                    return Err(
                        "Linux ALSA backend hint unavailable for exclusive output.".to_string()
                    );
                };
                return start_linux_alsa_backend(alsa, &controls);
            }
            if explicit_device_requested {
                let Some(alsa) = alsa else {
                    return Err(
                        "Linux ALSA backend hint unavailable for the selected Sound Device."
                            .to_string(),
                    );
                };
                return start_linux_alsa_backend(alsa, &controls).map_err(|err| {
                    format!(
                        "failed to start native ALSA output for the selected Sound Device: {err}"
                    )
                });
            }
            #[cfg(feature = "pipewire-audio")]
            if let Some(pipewire) = pipewire {
                match start_linux_pipewire_backend(pipewire, &controls) {
                    Ok(output) => return Ok(output),
                    Err(err) => {
                        warn!(
                            "Failed to start native PipeWire output: {err}. Falling back to PulseAudio/ALSA."
                        );
                    }
                }
            }
            #[cfg(target_os = "linux")]
            if linux_pulse::is_available()
                && let Some(pulse) = pulse
            {
                match start_linux_pulse_backend(pulse, &controls) {
                    Ok(output) => return Ok(output),
                    Err(err) => {
                        warn!(
                            "Failed to start native PulseAudio output: {err}. Falling back to ALSA/JACK."
                        );
                    }
                }
            }
            if let Some(alsa) = alsa {
                match start_linux_alsa_backend(alsa, &controls) {
                    Ok(output) => return Ok(output),
                    Err(err) => {
                        #[cfg(target_os = "linux")]
                        if linux_jack::is_available()
                            && let Some(jack) = jack
                        {
                            match start_linux_jack_backend(jack, &controls) {
                                Ok(output) => return Ok(output),
                                Err(jack_err) => {
                                    return Err(format!(
                                        "failed to start native ALSA output: {err}; JACK fallback also failed: {jack_err}"
                                    ));
                                }
                            }
                        }
                        return Err(format!("failed to start native ALSA output: {err}"));
                    }
                }
            }
            Err("no native Linux audio backend hint is available.".to_string())
        }
    }
    #[cfg(target_os = "freebsd")]
    if let Some(pcm) = freebsd_pcm {
        return start_freebsd_pcm_backend(pcm.clone(), &controls)
            .map_err(|err| format!("failed to start native FreeBSD PCM output: {err}"));
    }

    #[cfg(target_os = "macos")]
    if let Some(coreaudio) = coreaudio {
        return start_macos_coreaudio_backend(coreaudio.clone(), &controls).map_err(|err| {
            format!(
                "failed to start native CoreAudio output for '{}': {err}",
                coreaudio.device_name
            )
        });
    }

    #[cfg(windows)]
    if let Some(wasapi) = wasapi {
        let (stream, ready, sfx_sender, stream_handle) = start_wasapi_backend(wasapi, &controls)?;
        return Ok((
            NativeOutputBackend::Wasapi { _stream: stream },
            ready,
            sfx_sender,
            stream_handle,
        ));
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("no native audio backend hint is available on this platform build.".to_string())
    }
}

#[cfg(windows)]
fn start_wasapi_backend(
    wasapi: WasapiBackendHint,
    controls: &Arc<MixControls>,
) -> Result<
    (
        windows_wasapi::WasapiOutputStream,
        OutputBackendReady,
        SfxSender,
        AudioStreamHandle,
    ),
    String,
> {
    let access_mode = match wasapi.output_mode {
        AudioOutputMode::Exclusive => windows_wasapi::WasapiAccessMode::Exclusive,
        AudioOutputMode::Auto | AudioOutputMode::Shared => windows_wasapi::WasapiAccessMode::Shared,
    };
    let prep = windows_wasapi::prepare(
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
    let (stream_handle, render, sfx_sender, sfx_receiver) =
        output_transport(ready.device_channels, controls);
    let stream = windows_wasapi::start(prep, render, sfx_receiver).map_err(|err| {
        format!(
            "failed to start native WASAPI output for '{}': {err}",
            wasapi.device_name
        )
    })?;
    Ok((stream, ready, sfx_sender, stream_handle))
}

#[cfg(test)]
mod tests {
    use super::LinuxAudioBackend;
    use std::str::FromStr;

    #[test]
    fn linux_backend_config_is_owned_by_the_native_facade() {
        for (text, expected) in [
            ("auto", LinuxAudioBackend::Auto),
            ("pipe-wire", LinuxAudioBackend::PipeWire),
            ("pulse", LinuxAudioBackend::PulseAudio),
            ("jack", LinuxAudioBackend::Jack),
            ("alsa", LinuxAudioBackend::Alsa),
        ] {
            assert_eq!(LinuxAudioBackend::from_str(text), Ok(expected));
            assert_ne!(expected.as_str(), "");
        }
    }
}
