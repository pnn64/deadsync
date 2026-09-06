use super::{Audio, waveformat};
use windows::core::Interface;

pub(super) fn validate(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<(), String> {
    initialize(audio_client, format, preferred_buffer_frames)
}

pub(super) fn initialize(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    preferred_buffer_frames: Option<u32>,
) -> Result<(), String> {
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
        .unwrap_or(default_period_frames)
        .clamp(min_period_frames, max_period_frames);
    if fundamental_period_frames > 0 {
        let remainder = period_frames % fundamental_period_frames;
        if remainder != 0 {
            period_frames = period_frames
                .saturating_add(fundamental_period_frames - remainder)
                .min(max_period_frames);
        }
    }

    // SAFETY: `client3` is live and `format` points to a valid waveform buffer.
    unsafe {
        client3
            .InitializeSharedAudioStream(
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                period_frames,
                waveformat(format),
                None,
            )
            .map_err(|e| format!("IAudioClient3::InitializeSharedAudioStream failed: {e}"))?;
    }
    Ok(())
}
