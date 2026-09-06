use super::{Audio, frames_to_hns, query_device_periods_hns, waveformat};

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
    let (default_period_hns, _) = query_device_periods_hns(audio_client)?;
    let buffer_duration_hns = preferred_buffer_frames
        .filter(|frames| *frames > 0)
        .map_or_else(
            || default_period_hns.max(0),
            |frames| frames_to_hns(frames, waveformat(format).nSamplesPerSec),
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
