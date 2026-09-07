use super::{Audio, waveformat};
use windows::core::Interface;

pub(super) fn validate(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<(Vec<u8>, u32), String> {
    let buffer_size = select_buffer_size(audio_client, format, preferred_buffer_frames)?;
    initialize(audio_client, format, buffer_size)?;
    Ok((format.to_vec(), buffer_size))
}

pub(super) fn initialize(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    buffer_size: u32,
) -> Result<(), String> {
    let client3 = audio_client
        .cast::<Audio::IAudioClient3>()
        .map_err(|e| format!("IAudioClient3 interface not available: {e}"))?;

    // SAFETY: `client3` is live and `pformat` points to a valid waveform buffer.
    unsafe {
        client3
            .InitializeSharedAudioStream(
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                buffer_size,
                waveformat(format),
                None,
            )
            .map_err(|e| format!("IAudioClient3::InitializeSharedAudioStream failed: {e}"))?;
    }

    Ok(())
}

fn select_buffer_size(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<u32, String> {
    let client3 = audio_client
        .cast::<Audio::IAudioClient3>()
        .map_err(|e| format!("IAudioClient3 interface not available: {e}"))?;
    let mut default_period_frames = 0u32;
    let mut fundamental_period_frames = 0u32;
    let mut min_period_frames = 0u32;
    let mut max_period_frames = 0u32;

    // SAFETY: `client3` is live and the period variables are writable outputs.
    unsafe {
        client3
            .GetSharedModeEnginePeriod(
                waveformat(format),
                &mut default_period_frames,
                &mut fundamental_period_frames,
                &mut min_period_frames,
                &mut max_period_frames,
            )
            .map_err(|e| format!("IAudioClient3::GetSharedModeEnginePeriod failed: {e}"))?;
    }

    let mut period_frames = preferred_buffer_frames
        .filter(|frames| *frames > 0)
        .unwrap_or(min_period_frames)
        .clamp(min_period_frames, max_period_frames);

    // Ensure the period frames align with the fundamental period.
    if fundamental_period_frames > 0 && period_frames != min_period_frames {
        period_frames = period_frames
            .next_multiple_of(fundamental_period_frames)
            .min(max_period_frames);
    }

    log::info!(
        "WASAPI shared low-latency periods: \
        min {min_period_frames}, \
        default {default_period_frames}, \
        max {max_period_frames}, \
        fundamental {fundamental_period_frames}, \
        preferred {preferred_buffer_frames:?}, \
        chosen {period_frames}"
    );

    Ok(period_frames)
}
