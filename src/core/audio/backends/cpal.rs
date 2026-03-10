use super::super::{
    OutputBackend, OutputBackendReady, OutputDeviceInfo, OutputDeviceProbe, OutputTelemetryClock,
    QueuedSfx, RenderState, internal, output_playback_anchor,
};
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{debug, error, warn};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::time::Instant;

#[derive(Clone)]
pub(crate) struct CpalBackendLaunch {
    pub device: cpal::Device,
    pub device_name: String,
    pub sample_format: SampleFormat,
    pub stream_config: cpal::StreamConfig,
    pub output_mode: crate::config::AudioOutputMode,
}

#[inline(always)]
pub(crate) fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string())
}

#[inline(always)]
pub(crate) fn device_id_string(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.1)
}

fn sample_rates_from_ranges(ranges: &[(u32, u32)], default_rate_hz: u32) -> Vec<u32> {
    const COMMON_SAMPLE_RATES: [u32; 11] = [
        11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000,
    ];
    let mut rates = Vec::with_capacity(COMMON_SAMPLE_RATES.len() + 4);
    if default_rate_hz > 0 {
        rates.push(default_rate_hz);
    }
    for &hz in &COMMON_SAMPLE_RATES {
        if ranges.iter().any(|&(min, max)| hz >= min && hz <= max) {
            rates.push(hz);
        }
    }
    for &(min, max) in ranges {
        rates.push(min);
        rates.push(max);
    }
    rates.sort_unstable();
    rates.dedup();
    rates
}

pub(crate) fn collect_supported_sample_rates(device: &cpal::Device) -> Vec<u32> {
    let default_rate_hz = device
        .default_output_config()
        .map(|cfg| cfg.sample_rate())
        .unwrap_or(0);
    let mut ranges = Vec::new();
    match device.supported_output_configs() {
        Ok(configs) => {
            for cfg_range in configs {
                let min = cfg_range.min_sample_rate();
                let max = cfg_range.max_sample_rate();
                ranges.push((min.min(max), max.max(min)));
            }
        }
        Err(_) => {
            if default_rate_hz > 0 {
                return vec![default_rate_hz];
            }
            return Vec::new();
        }
    }
    let mut rates = sample_rates_from_ranges(&ranges, default_rate_hz);
    if rates.is_empty() && default_rate_hz > 0 {
        rates.push(default_rate_hz);
    }
    rates
}

pub(crate) fn enumerate_output_device_probes(
    host: &cpal::Host,
    default_device_name: &str,
) -> Vec<OutputDeviceProbe> {
    let mut probes = Vec::new();
    match host.output_devices() {
        Ok(devices) => {
            debug!("Enumerating audio output devices for host {:?}:", host.id());
            for (idx, dev) in devices.enumerate() {
                let name = device_name(&dev);
                let is_default = name == default_device_name;
                let tag = if is_default { " (default)" } else { "" };
                #[cfg(all(unix, not(target_os = "macos")))]
                let alsa_pcm_id = device_id_string(&dev);
                #[cfg(windows)]
                let wasapi_id = device_id_string(&dev);
                debug!("  Device {idx}: '{name}'{tag}");
                let sample_rates_hz = match dev.supported_output_configs() {
                    Ok(configs) => {
                        let mut ranges = Vec::new();
                        for cfg_range in configs {
                            let min = cfg_range.min_sample_rate();
                            let max = cfg_range.max_sample_rate();
                            let channels = cfg_range.channels();
                            let fmt = cfg_range.sample_format();
                            debug!("    - {fmt:?}, {channels} ch, {min}..{max} Hz");
                            ranges.push((min.min(max), max.max(min)));
                        }
                        let default_rate_hz = dev
                            .default_output_config()
                            .map(|cfg| cfg.sample_rate())
                            .unwrap_or(0);
                        sample_rates_from_ranges(&ranges, default_rate_hz)
                    }
                    Err(e) => {
                        warn!("    ! Failed to query supported output configs: {e}");
                        collect_supported_sample_rates(&dev)
                    }
                };
                probes.push(OutputDeviceProbe {
                    device: dev,
                    info: OutputDeviceInfo {
                        name,
                        is_default,
                        sample_rates_hz,
                    },
                    #[cfg(all(unix, not(target_os = "macos")))]
                    alsa_pcm_id,
                    #[cfg(windows)]
                    wasapi_id,
                });
            }
        }
        Err(e) => {
            warn!("Failed to enumerate audio output devices: {e}");
        }
    }
    probes
}

pub(crate) fn start_output(
    launch: CpalBackendLaunch,
    music_ring: Arc<internal::SpscRingI16>,
    fallback_from_native: bool,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    let device_channels = launch.stream_config.channels as usize;
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
    let mut render = RenderState::new(music_ring, sfx_receiver, device_channels);
    let stream = match launch.sample_format {
        SampleFormat::I16 => launch
            .device
            .build_output_stream(
                &launch.stream_config,
                move |out: &mut [i16], info| {
                    render.render_i16(out, output_playback_anchor(Instant::now(), info));
                },
                |err| error!("Audio stream error: {err}"),
                None,
            )
            .map_err(|e| format!("failed to build CPAL i16 output stream: {e}"))?,
        SampleFormat::U16 => launch
            .device
            .build_output_stream(
                &launch.stream_config,
                move |out: &mut [u16], info| {
                    render.render_u16(out, output_playback_anchor(Instant::now(), info));
                },
                |err| error!("Audio stream error: {err}"),
                None,
            )
            .map_err(|e| format!("failed to build CPAL u16 output stream: {e}"))?,
        SampleFormat::F32 => launch
            .device
            .build_output_stream(
                &launch.stream_config,
                move |out: &mut [f32], info| {
                    render.render_f32(out, output_playback_anchor(Instant::now(), info));
                },
                |err| error!("Audio stream error: {err}"),
                None,
            )
            .map_err(|e| format!("failed to build CPAL f32 output stream: {e}"))?,
        other => return Err(format!("unsupported CPAL sample format: {other:?}")),
    };
    stream
        .play()
        .map_err(|e| format!("failed to play CPAL output stream: {e}"))?;
    Ok((
        OutputBackend::Cpal(stream),
        OutputBackendReady {
            device_sample_rate: launch.stream_config.sample_rate,
            device_channels,
            device_name: launch.device_name,
            backend_name: "cpal",
            requested_output_mode: launch.output_mode,
            fallback_from_native,
            timing_clock: OutputTelemetryClock::Callback,
        },
        sfx_sender,
    ))
}
