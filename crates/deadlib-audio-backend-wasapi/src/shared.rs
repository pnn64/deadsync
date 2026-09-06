use super::{Audio, frames_to_hns, hns_to_frames, query_device_periods_hns, waveformat};

pub(super) fn validate(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<Vec<u8>, String> {
    initialize(audio_client, format, preferred_buffer_frames)?;
    Ok(format.to_vec())
}

pub(super) fn initialize(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<(), String> {
    let (default_period_hns, min_period_hns) = query_device_periods_hns(audio_client)?;
    let buffer_duration_hns = preferred_buffer_frames
        .filter(|frames| *frames > 0)
        .map_or_else(
            || default_period_hns.max(0),
            |frames| frames_to_hns(frames, waveformat(format).nSamplesPerSec),
        );

    let min_period_frames = hns_to_frames(min_period_hns, waveformat(format).nSamplesPerSec);
    let default_period_frames =
        hns_to_frames(default_period_hns, waveformat(format).nSamplesPerSec);
    log::info!(
        "WASAPI shared periods: \
        min {min_period_frames}, \
        default {default_period_frames}"
    );
    // SAFETY: `audio_client` is live and `format` points to a valid waveform
    // buffer owned by the caller.
    unsafe {
        audio_client
            .Initialize(
                Audio::AUDCLNT_SHAREMODE_SHARED,
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                buffer_duration_hns,
                0,
                waveformat(format),
                None,
            )
            .map_err(|e| format!("failed to initialize WASAPI shared stream: {e}"))
    }
}
