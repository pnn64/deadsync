use super::super::{
    OutputBackendReady, OutputTelemetryClock, OutputTimingQuality, QueuedSfx, RenderState,
    note_output_clock_fallback, publish_output_timing, publish_output_timing_quality,
};
use crate::engine::audio::internal;
use crate::engine::host_time::now_nanos;
use coreaudio::audio_unit::audio_format::LinearPcmFlags;
use coreaudio::audio_unit::macos_helpers::{
    audio_unit_from_device_id, get_audio_device_ids_for_scope, get_available_sample_rates,
    get_default_device_id, get_device_id_from_name, get_device_name,
};
use coreaudio::audio_unit::render_callback::{self, data};
use coreaudio::audio_unit::{AudioUnit, Element, SampleFormat, Scope, StreamFormat};
use log::{info, warn};
use mach2::mach_time::{mach_absolute_time, mach_timebase_info, mach_timebase_info_data_t};
use objc2_core_audio::{
    AudioDeviceID, AudioObjectGetPropertyData, AudioObjectPropertyAddress,
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyDeviceUID,
    kAudioDevicePropertyNominalSampleRate, kAudioHardwareNoError, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal,
};
use objc2_core_foundation::{CFRetained, CFString};
use std::mem::size_of;
use std::ptr::{NonNull, null};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

pub(crate) struct CoreAudioOutputPrep {
    audio_unit: AudioUnit,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    buffer_frames: u32,
}

impl CoreAudioOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: "coreaudio-shared",
            requested_output_mode: crate::config::AudioOutputMode::Shared,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::HostTime,
            timing_quality: OutputTimingQuality::Trusted,
        }
    }
}

pub(crate) struct CoreAudioOutputStream {
    audio_unit: Option<AudioUnit>,
}

impl Drop for CoreAudioOutputStream {
    fn drop(&mut self) {
        if let Some(audio_unit) = self.audio_unit.as_mut() {
            let _ = audio_unit.stop();
        }
    }
}

pub(crate) struct CoreAudioOutputDevice {
    pub uid: String,
    pub name: String,
    pub sample_rates_hz: Vec<u32>,
    pub default_rate_hz: u32,
    pub channels: usize,
    pub is_default: bool,
}

const COMMON_SAMPLE_RATES_HZ: [u32; 11] = [
    48_000, 44_100, 96_000, 88_200, 192_000, 176_400, 32_000, 22_050, 16_000, 11_025, 384_000,
];

pub(crate) fn enumerate_output_devices() -> Vec<CoreAudioOutputDevice> {
    let default_device_id = get_default_device_id(false);
    let Ok(device_ids) = get_audio_device_ids_for_scope(Scope::Output) else {
        warn!("failed to enumerate CoreAudio output devices at startup");
        return Vec::new();
    };
    let mut devices = Vec::with_capacity(device_ids.len());
    for device_id in device_ids {
        let uid = match device_uid(device_id) {
            Ok(uid) => uid,
            Err(err) => {
                warn!("skipping CoreAudio device {device_id}: {err}");
                continue;
            }
        };
        let name =
            get_device_name(device_id).unwrap_or_else(|_| format!("CoreAudio Device {device_id}"));
        let (sample_rates_hz, default_rate_hz, channels) = match probe_output_device(device_id) {
            Ok(probed) => probed,
            Err(err) => {
                warn!("failed to probe CoreAudio device '{name}': {err}");
                (vec![48_000], 48_000, 2)
            }
        };
        devices.push(CoreAudioOutputDevice {
            uid,
            name,
            sample_rates_hz,
            default_rate_hz,
            channels,
            is_default: default_device_id == Some(device_id),
        });
    }
    if !devices.iter().any(|device| device.is_default) {
        if let Some(device) = devices.first_mut() {
            device.is_default = true;
        }
    }
    devices
}

#[derive(Clone, Copy)]
struct CoreAudioHostClock {
    numer: u32,
    denom: u32,
    offset_nanos: i128,
}

impl CoreAudioHostClock {
    fn calibrate() -> Result<Self, String> {
        let mut info = mach_timebase_info_data_t { numer: 0, denom: 0 };
        // SAFETY: `mach_timebase_info` writes into the provided stack local and
        // does not retain the pointer after returning.
        let status = unsafe { mach_timebase_info(&mut info) };
        if status != 0 || info.denom == 0 {
            return Err(format!(
                "failed to query mach timebase info (status={status}, denom={}).",
                info.denom
            ));
        }
        let host_before = now_nanos();
        // SAFETY: `mach_absolute_time` reads the current monotonic host clock and
        // takes no pointers or borrowed Rust data.
        let mach_now = unsafe { mach_absolute_time() };
        let host_after = now_nanos();
        let host_mid =
            host_before / 2 + host_after / 2 + ((host_before & 1) + (host_after & 1)) / 2;
        let mach_nanos = scale_mach_time(mach_now, info.numer, info.denom);
        Ok(Self {
            numer: info.numer,
            denom: info.denom,
            offset_nanos: i128::from(mach_nanos) - i128::from(host_mid),
        })
    }

    #[inline(always)]
    fn callback_nanos(self, mach_time: u64) -> (u64, OutputTimingQuality) {
        if mach_time == 0 {
            note_output_clock_fallback();
            return (now_nanos(), OutputTimingQuality::Fallback);
        }
        let mach_nanos = scale_mach_time(mach_time, self.numer, self.denom);
        let host_nanos = i128::from(mach_nanos) - self.offset_nanos;
        let clamped = host_nanos.clamp(0, i128::from(u64::MAX)) as u64;
        (clamped, OutputTimingQuality::Trusted)
    }
}

struct ConfiguredUnit {
    sample_rate_hz: u32,
    channels: usize,
    buffer_frames: u32,
}

pub(crate) fn prepare(
    device_uid: Option<String>,
    device_name: String,
    requested_rate_hz: Option<u32>,
    channels: usize,
) -> Result<CoreAudioOutputPrep, String> {
    let device_id = select_device_id(device_uid.as_deref(), &device_name)?;
    let resolved_name = get_device_name(device_id).unwrap_or(device_name);
    let sample_rate_hz = requested_rate_hz
        .filter(|rate| *rate > 0)
        .map_or_else(|| device_nominal_sample_rate_hz(device_id), Ok)?;
    let mut audio_unit = audio_unit_from_device_id(device_id, false)
        .map_err(|e| format!("failed to create CoreAudio HAL output unit: {e}"))?;
    let actual = configure_output_unit(&mut audio_unit, sample_rate_hz, channels)?;
    Ok(CoreAudioOutputPrep {
        audio_unit,
        device_name: resolved_name,
        sample_rate_hz: actual.sample_rate_hz,
        channels: actual.channels,
        buffer_frames: actual.buffer_frames,
    })
}

pub(crate) fn start(
    mut prep: CoreAudioOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<CoreAudioOutputStream, String> {
    let host_clock = CoreAudioHostClock::calibrate()?;
    let mut render = RenderState::new(music_ring, sfx_receiver, prep.channels);
    let sample_rate_hz = prep.sample_rate_hz;
    let buffer_frames = prep.buffer_frames.max(1);
    let device_name = prep.device_name.clone();

    type Args = render_callback::Args<data::Interleaved<f32>>;
    prep.audio_unit
        .set_render_callback(move |args: Args| {
            let (anchor_nanos, quality) = host_clock.callback_nanos(args.time_stamp.mHostTime);
            render.render_f32_host_nanos(args.data.buffer, anchor_nanos);
            let period_frames = args.num_frames.max(1) as u32;
            let latency_frames = buffer_frames.max(period_frames);
            let period_ns = frames_to_nanos(sample_rate_hz, period_frames);
            let latency_ns = frames_to_nanos(sample_rate_hz, latency_frames);
            publish_output_timing(
                sample_rate_hz,
                period_ns,
                latency_ns,
                buffer_frames.max(period_frames),
                0,
                latency_frames,
                latency_ns,
            );
            publish_output_timing_quality(quality);
            Ok(())
        })
        .map_err(|e| format!("failed to set CoreAudio render callback: {e}"))?;

    prep.audio_unit
        .start()
        .map_err(|e| format!("failed to start CoreAudio output unit: {e}"))?;
    let buffer_ns = frames_to_nanos(sample_rate_hz, buffer_frames);
    publish_output_timing(
        sample_rate_hz,
        buffer_ns,
        buffer_ns,
        buffer_frames,
        0,
        buffer_frames,
        buffer_ns,
    );
    publish_output_timing_quality(OutputTimingQuality::Trusted);
    info!(
        "CoreAudio '{}' using native shared output at {} Hz, {} ch.",
        device_name, sample_rate_hz, prep.channels
    );
    Ok(CoreAudioOutputStream {
        audio_unit: Some(prep.audio_unit),
    })
}

fn select_device_id(device_uid: Option<&str>, device_name: &str) -> Result<AudioDeviceID, String> {
    if let Some(uid) = device_uid {
        return device_id_from_uid(uid);
    }
    if let Some(device_id) = get_device_id_from_name(device_name, false) {
        return Ok(device_id);
    }
    get_default_device_id(false).ok_or_else(|| "no default CoreAudio output device".to_string())
}

fn probe_output_device(device_id: AudioDeviceID) -> Result<(Vec<u32>, u32, usize), String> {
    let default_rate_hz = device_nominal_sample_rate_hz(device_id)?;
    let audio_unit = audio_unit_from_device_id(device_id, false)
        .map_err(|e| format!("failed to create HAL output unit: {e}"))?;
    let actual = audio_unit
        .output_stream_format()
        .map_err(|e| format!("failed to query output stream format: {e}"))?;
    Ok((
        supported_sample_rates_hz(device_id, default_rate_hz),
        default_rate_hz,
        actual.channels.max(1) as usize,
    ))
}

fn device_nominal_sample_rate_hz(device_id: AudioDeviceID) -> Result<u32, String> {
    let property_address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyNominalSampleRate,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut sample_rate_hz = 0f64;
    let mut data_size = size_of::<f64>() as u32;
    // SAFETY: the property address is fully initialized, `data_size` matches
    // the output buffer, and CoreAudio writes the sample rate into the stack
    // local `sample_rate_hz` without retaining the pointer.
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            NonNull::from(&property_address),
            0,
            null(),
            NonNull::from(&mut data_size),
            NonNull::from(&mut sample_rate_hz).cast(),
        )
    };
    if status != kAudioHardwareNoError {
        return Err(format!(
            "failed to query CoreAudio nominal sample rate (status={status})"
        ));
    }
    if !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
        return Err(format!(
            "CoreAudio nominal sample rate was invalid ({sample_rate_hz})."
        ));
    }
    Ok(sample_rate_hz.round().clamp(1.0, u32::MAX as f64) as u32)
}

fn supported_sample_rates_hz(device_id: AudioDeviceID, default_rate_hz: u32) -> Vec<u32> {
    let mut sample_rates_hz = Vec::with_capacity(COMMON_SAMPLE_RATES_HZ.len() + 3);
    if default_rate_hz > 0 {
        sample_rates_hz.push(default_rate_hz);
    }
    let ranges = match get_available_sample_rates(device_id) {
        Ok(ranges) => ranges,
        Err(err) => {
            warn!("failed to query CoreAudio sample rates for device {device_id}: {err}");
            sample_rates_hz.sort_unstable();
            sample_rates_hz.dedup();
            return sample_rates_hz;
        }
    };
    let supported_ranges: Vec<_> = ranges
        .into_iter()
        .map(|range| (range.mMinimum, range.mMaximum))
        .collect();
    for &(min_rate_hz, max_rate_hz) in &supported_ranges {
        push_sample_rate_hz(&mut sample_rates_hz, min_rate_hz);
        push_sample_rate_hz(&mut sample_rates_hz, max_rate_hz);
    }
    for sample_rate_hz in COMMON_SAMPLE_RATES_HZ {
        let rate = f64::from(sample_rate_hz);
        if supported_ranges.iter().any(|&(min_rate_hz, max_rate_hz)| {
            rate + 0.5 >= min_rate_hz && rate - 0.5 <= max_rate_hz
        }) {
            sample_rates_hz.push(sample_rate_hz);
        }
    }
    sample_rates_hz.sort_unstable();
    sample_rates_hz.dedup();
    sample_rates_hz
}

fn push_sample_rate_hz(sample_rates_hz: &mut Vec<u32>, sample_rate_hz: f64) {
    if !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
        return;
    }
    sample_rates_hz.push(sample_rate_hz.round().clamp(1.0, u32::MAX as f64) as u32);
}

fn configure_output_unit(
    audio_unit: &mut AudioUnit,
    sample_rate_hz: u32,
    channels: usize,
) -> Result<ConfiguredUnit, String> {
    let stream_format = StreamFormat {
        sample_rate: f64::from(sample_rate_hz.max(1)),
        sample_format: SampleFormat::F32,
        flags: LinearPcmFlags::IS_FLOAT | LinearPcmFlags::IS_PACKED,
        channels: channels.max(1) as u32,
    };
    audio_unit
        .set_stream_format(stream_format, Scope::Input, Element::Output)
        .map_err(|e| format!("failed to configure CoreAudio stream format: {e}"))?;
    let actual = audio_unit
        .output_stream_format()
        .map_err(|e| format!("failed to query CoreAudio stream format: {e}"))?;
    if actual.sample_format != SampleFormat::F32 {
        return Err(format!(
            "CoreAudio stream format mismatch: expected f32 callback data, got {:?}.",
            actual.sample_format
        ));
    }
    let buffer_frames: u32 = audio_unit
        .get_property(
            kAudioDevicePropertyBufferFrameSize,
            Scope::Global,
            Element::Output,
        )
        .map_err(|e| format!("failed to query CoreAudio buffer frame size: {e}"))?;
    Ok(ConfiguredUnit {
        sample_rate_hz: actual.sample_rate.max(1.0).round() as u32,
        channels: actual.channels.max(1) as usize,
        buffer_frames: buffer_frames.max(1),
    })
}

fn device_id_from_uid(uid: &str) -> Result<AudioDeviceID, String> {
    let devices = get_audio_device_ids_for_scope(Scope::Output)
        .map_err(|e| format!("failed to enumerate CoreAudio output devices: {e}"))?;
    for device_id in devices {
        if device_uid(device_id)?.as_str() == uid {
            return Ok(device_id);
        }
    }
    Err(format!("no CoreAudio output device matched UID '{uid}'"))
}

fn device_uid(device_id: AudioDeviceID) -> Result<String, String> {
    let property_address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyDeviceUID,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut uid: *mut CFString = std::ptr::null_mut();
    let mut data_size = size_of::<*mut CFString>() as u32;
    // SAFETY: the property address is fully initialized, `data_size` points to
    // writable stack storage sized for a `CFString` pointer, and `uid` points to
    // writable stack storage receiving the retained CoreFoundation object.
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            NonNull::from(&property_address),
            0,
            null(),
            NonNull::from(&mut data_size),
            NonNull::from(&mut uid).cast(),
        )
    };
    if status != kAudioHardwareNoError {
        return Err(format!(
            "failed to query CoreAudio device UID (status={status})"
        ));
    }
    let uid = NonNull::new(uid).ok_or_else(|| "CoreAudio device UID was null".to_string())?;
    // SAFETY: on success CoreAudio returns an owned `CFStringRef` in `uid`, so
    // transferring it into `CFRetained` gives Rust responsibility for releasing
    // that reference.
    let uid = unsafe { CFRetained::<CFString>::from_raw(uid) };
    Ok(uid.to_string())
}

#[inline(always)]
fn scale_mach_time(mach_time: u64, numer: u32, denom: u32) -> u64 {
    ((u128::from(mach_time) * u128::from(numer)) / u128::from(denom)).min(u128::from(u64::MAX))
        as u64
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    (u64::from(frames) * 1_000_000_000) / u64::from(sample_rate_hz)
}
