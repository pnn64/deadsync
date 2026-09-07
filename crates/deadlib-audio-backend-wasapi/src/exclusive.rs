use log::info;

use super::{
    Audio, frames_to_hns, hns_to_frames, query_device_periods_hns, selected_device_period_hns,
    waveformat, waveformat_mut,
};

pub(super) fn validate(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    device_name: &str,
) -> Result<Vec<u8>, String> {
    if is_format_supported(audio_client, format)? {
        return Ok(format.to_vec());
    }

    // fallback to PCM-16 by default
    let mut fallback = format.to_vec();
    let wave = waveformat_mut(&mut fallback);
    wave.wFormatTag = Audio::WAVE_FORMAT_PCM as u16;
    wave.cbSize = 0;
    wave.wBitsPerSample = 16;
    wave.nBlockAlign = wave.nChannels.saturating_mul(2);
    wave.nAvgBytesPerSec = wave
        .nSamplesPerSec
        .saturating_mul(u32::from(wave.nBlockAlign));

    if is_format_supported(audio_client, &fallback)? {
        info!(
            "WASAPI Exclusive: falling back to PCM-16 for device '{}'",
            device_name
        );
        return Ok(fallback);
    }
    Err(unsupported_format_error(format, device_name))
}

fn is_format_supported(audio_client: &Audio::IAudioClient, format: &[u8]) -> Result<bool, String> {
    // SAFETY: `audio_client` is live and `format` points to a valid waveform
    // buffer owned by the caller.
    let status = unsafe {
        audio_client.IsFormatSupported(Audio::AUDCLNT_SHAREMODE_EXCLUSIVE, waveformat(format), None)
    };
    if status.is_ok() {
        Ok(true)
    } else if status == Audio::AUDCLNT_E_UNSUPPORTED_FORMAT {
        Ok(false)
    } else {
        Err(format!(
            "WASAPI exclusive IsFormatSupported failed: {status:?}"
        ))
    }
}

fn unsupported_format_error(format: &[u8], device_name: &str) -> String {
    let wave = waveformat(format);
    let sample_rate_hz = wave.nSamplesPerSec;
    let channels = wave.nChannels;
    let bits_per_sample = wave.wBitsPerSample;
    format!(
        "WASAPI exclusive format not supported for '{device_name}': {} Hz, {} ch, {} bits",
        sample_rate_hz, channels, bits_per_sample
    )
}

pub(super) fn initialize(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<(), String> {
    let (default_period_hns, min_period_hns) = query_device_periods_hns(audio_client)?;
    let sample_rate = waveformat(format).nSamplesPerSec;
    let period_hns = preferred_buffer_frames
        .filter(|frames| *frames > 0)
        .map_or_else(
            || {
                selected_device_period_hns(
                    super::WasapiBackendMode::Exclusive,
                    default_period_hns,
                    min_period_hns,
                )
            },
            |frames| frames_to_hns(frames, sample_rate),
        );

    let min_period_frames = hns_to_frames(min_period_hns, sample_rate);
    let default_period_frames = hns_to_frames(default_period_hns, sample_rate);
    let buffer_duration_frames = hns_to_frames(period_hns, sample_rate);

    log::info!(
        "WASAPI exclusive periods: \
        min {min_period_frames}, \
        default {default_period_frames}, \
        preferred {preferred_buffer_frames:?}, \
        chosen {buffer_duration_frames}, \
        period {period_hns}"
    );
    // SAFETY: `audio_client` is live and `format` points to a valid waveform
    // buffer owned by the caller.
    unsafe {
        audio_client
            .Initialize(
                Audio::AUDCLNT_SHAREMODE_EXCLUSIVE,
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                period_hns,
                period_hns,
                waveformat(format),
                None,
            )
            .map_err(|e| format!("failed to initialize WASAPI exclusive stream: {e}"))
    }
}
