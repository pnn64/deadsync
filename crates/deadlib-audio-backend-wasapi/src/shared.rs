use super::{Audio, frames_to_hns, hns_to_frames, query_device_periods_hns, waveformat};

pub(super) fn validate(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<(Vec<u8>, u32), String> {
    let (default_period_hns, min_period_hns) = query_device_periods_hns(audio_client)?;
    let sample_rate = waveformat(format).nSamplesPerSec;
    let buffer_size = preferred_buffer_frames
        .filter(|frames| *frames > 0)
        .map_or_else(
            || default_period_hns.max(0),
            |frames| frames_to_hns(frames, sample_rate),
        );
    let buffer_size = buffer_size.min(u32::MAX as i64) as u32;

    let min_period_frames = hns_to_frames(min_period_hns, sample_rate);
    let default_period_frames = hns_to_frames(default_period_hns, sample_rate);
    let buffer_duration_frames = hns_to_frames(i64::from(buffer_size), sample_rate);
    log::info!(
        "WASAPI shared periods: \
        min {min_period_frames}, \
        default {default_period_frames}, \
        preferred {preferred_buffer_frames:?}, \
        chosen {buffer_duration_frames}"
    );

    initialize(audio_client, format, buffer_size)?;
    Ok((format.to_vec(), buffer_size))
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
                Audio::AUDCLNT_SHAREMODE_SHARED,
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                i64::from(buffer_size),
                0,
                waveformat(format),
                None,
            )
            .map_err(|e| format!("failed to initialize WASAPI shared stream: {e}"))
    }
}
