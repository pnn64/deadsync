use log::info;

use super::{
    Audio, frames_to_hns, hns_to_frames, query_device_periods_hns, selected_device_period_hns,
    waveformat, waveformat_mut,
};

pub(super) fn validate(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    device_name: &str,
    preferred_buffer_frames: Option<u32>,
) -> Result<(Vec<u8>, u32), String> {
    let mut chosen_format = format.to_vec();
    if !is_format_supported(audio_client, &chosen_format)? {
        let wave = waveformat_mut(&mut chosen_format);
        wave.wFormatTag = Audio::WAVE_FORMAT_PCM as u16;
        wave.cbSize = 0;
        wave.wBitsPerSample = 16;
        wave.nBlockAlign = wave.nChannels.saturating_mul(2);
        wave.nAvgBytesPerSec = wave
            .nSamplesPerSec
            .saturating_mul(u32::from(wave.nBlockAlign));

        if !is_format_supported(audio_client, &chosen_format)? {
            return Err(unsupported_format_error(format, device_name));
        }
        info!(
            "WASAPI Exclusive: falling back to PCM-16 for device '{}'",
            device_name
        );
    }

    resolve_buffer_size(audio_client, &chosen_format, preferred_buffer_frames)
        .map(|buffer_size| (chosen_format, buffer_size))
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
    buffer_size: u32,
) -> Result<(), String> {
    // SAFETY: `audio_client` is live and `format` points to a valid waveform
    // buffer owned by the caller.
    unsafe {
        audio_client
            .Initialize(
                Audio::AUDCLNT_SHAREMODE_EXCLUSIVE,
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                i64::from(buffer_size),
                i64::from(buffer_size),
                waveformat(format),
                None,
            )
            .map_err(|e| format!("failed to initialize WASAPI exclusive stream: {e}"))
    }
}

fn resolve_buffer_size(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<u32, String> {
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
        )
        .max(min_period_hns);

    let min_period_frames = hns_to_frames(min_period_hns, sample_rate);
    let default_period_frames = hns_to_frames(default_period_hns, sample_rate);
    let buffer_duration_frames = hns_to_frames(period_hns, sample_rate);

    log::info!(
        "WASAPI exclusive: \
        min frames {min_period_frames}, \
        default frames {default_period_frames}, \
        preferred frames {preferred_buffer_frames:?}, \
        chosen frames {buffer_duration_frames}, \
        chosen hns {period_hns}"
    );
    // SAFETY: `audio_client` is live and `format` points to a valid waveform
    // buffer owned by the caller.
    match unsafe {
        audio_client.Initialize(
            Audio::AUDCLNT_SHAREMODE_EXCLUSIVE,
            Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            period_hns,
            period_hns,
            waveformat(format),
            None,
        )
    } {
        Ok(()) => {
            log::info!(
                "WASAPI exclusive buffer size resolution 
                succeeded using {period_hns} HNS."
            );
            Ok(period_hns.min(u32::MAX as i64) as u32)
        }
        Err(error) if error.code() == Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
            // The caller discards this client after resolution and creates a fresh
            // client with the driver-selected aligned frame count.
            let aligned_buffer_size = unsafe {
                audio_client.GetBufferSize().map_err(|e| {
                    format!(
                        "failed to query aligned WASAPI exclusive buffer size after Initialize: {e}"
                    )
                })?
            };
            log::info!(
                "WASAPI exclusive failed to use chosen frames, using driver-selected buffer size: \
                period {aligned_buffer_size}"
            );

            Ok(aligned_buffer_size)
            //Ok(frames_to_hns(aligned_buffer_size, sample_rate).min(u32::MAX as i64) as u32)
        }
        Err(error) => Err(format!(
            "failed to initialize WASAPI exclusive stream: {error}"
        )),
    }
}
