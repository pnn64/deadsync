//! Format and period negotiation runs only when opening or recovering an endpoint.

use super::{
    Audio, Foundation, KernelStreaming, WasapiAccessMode, WasapiOutputPrep, build_audio_client,
    free_com_task_mem, initialize_shared, query_device_periods_hns, sample_format_from_waveformat,
    selected_device_period_hns, waveformat, waveformat_mut,
};
use log::{debug, warn};
use windows::core::Interface;

pub(super) struct InitializedClient {
    pub client: Audio::IAudioClient,
    pub period_frames: Option<u32>,
    pub path: &'static str,
}

pub(super) fn initialize_client(
    device: &Audio::IMMDevice,
    prep: &WasapiOutputPrep,
) -> Result<InitializedClient, String> {
    let client = build_audio_client(device)?;
    match prep.mode {
        WasapiAccessMode::Shared => {
            initialize_shared_client(client, &prep.format, || build_audio_client(device))
        }
        WasapiAccessMode::Exclusive => {
            initialize_exclusive(client, &prep.format, || build_audio_client(device))
        }
    }
}

fn initialize_shared_client(
    mut client: Audio::IAudioClient,
    format: &[u8],
    mut activate: impl FnMut() -> Result<Audio::IAudioClient, String>,
) -> Result<InitializedClient, String> {
    // QueryInterface is available on Windows 7; the newer interface is reached
    // through its COM vtable, without importing any Windows 10 DLL entry points.
    let attempt = match client.cast::<Audio::IAudioClient3>() {
        Ok(client3) => Some(initialize_low_period(&client3, format)),
        Err(err) if err.code() == Foundation::E_NOINTERFACE => None,
        Err(err) => Some(Err(format!("IAudioClient3 query failed: {err}"))),
    };
    match attempt {
        Some(Ok(period_frames)) => {
            return Ok(InitializedClient {
                client,
                period_frames: Some(period_frames),
                path: "IAudioClient3 shared",
            });
        }
        Some(Err(err)) => {
            warn!(
                "WASAPI low-period shared initialization unavailable: {err}. Using legacy shared mode."
            );
            // Failed initialization can leave a client unusable. All interface
            // references must be released before activating the fallback client.
            drop(client);
            client = activate()?;
        }
        None => debug!("WASAPI IAudioClient3 unavailable; using legacy shared mode."),
    }
    initialize_shared(&client, format)?;
    Ok(InitializedClient {
        client,
        period_frames: None,
        path: "IAudioClient shared (legacy)",
    })
}

fn initialize_low_period(client: &Audio::IAudioClient3, format: &[u8]) -> Result<u32, String> {
    let properties = Audio::AudioClientProperties {
        cbSize: size_of::<Audio::AudioClientProperties>() as u32,
        bIsOffload: false.into(),
        eCategory: Audio::AudioCategory_GameMedia,
        Options: Audio::AUDCLNT_STREAMOPTIONS_NONE,
    };
    let mut default = 0;
    let mut fundamental = 0;
    let mut min = 0;
    let mut max = 0;
    // SAFETY: this uninitialized client belongs to this thread. The property and
    // format pointers remain valid, and all out parameters are writable locals.
    unsafe {
        client
            .SetClientProperties(&properties)
            .map_err(|e| format!("SetClientProperties failed: {e}"))?;
        client
            .GetSharedModeEnginePeriod(
                waveformat(format),
                &mut default,
                &mut fundamental,
                &mut min,
                &mut max,
            )
            .map_err(|e| format!("GetSharedModeEnginePeriod failed: {e}"))?;
    }
    let period = minimum_period(fundamental, min, max).ok_or_else(|| {
        format!("invalid shared periods: fundamental={fundamental}, min={min}, max={max}")
    })?;
    // SAFETY: the period is within the reported bounds and is a multiple of the
    // fundamental period. The format is the one used for the period query.
    unsafe {
        client
            .InitializeSharedAudioStream(
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                period,
                waveformat(format),
                None,
            )
            .map_err(|e| format!("InitializeSharedAudioStream ({period} frames) failed: {e}"))?;
    }
    Ok(period)
}

fn minimum_period(fundamental: u32, min: u32, max: u32) -> Option<u32> {
    if fundamental == 0 || min == 0 || min > max {
        return None;
    }
    let period = min.div_ceil(fundamental).checked_mul(fundamental)?;
    (period <= max).then_some(period)
}

fn initialize_exclusive(
    mut client: Audio::IAudioClient,
    format: &[u8],
    mut activate: impl FnMut() -> Result<Audio::IAudioClient, String>,
) -> Result<InitializedClient, String> {
    let (default, min) = query_device_periods_hns(&client)?;
    let period = selected_device_period_hns(WasapiAccessMode::Exclusive, default, min);
    match initialize_exclusive_period(&client, format, period) {
        Ok(()) => {}
        Err(err) if err.code() == Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
            // Microsoft's documented alignment dance: query the failed client,
            // release it, then retry once with equal aligned duration/period.
            // SAFETY: GetBufferSize is explicitly valid after this HRESULT.
            let frames = unsafe { client.GetBufferSize() }
                .map_err(|e| format!("failed to query aligned WASAPI buffer size: {e}"))?;
            let period = aligned_period_hns(frames, waveformat(format).nSamplesPerSec)
                .ok_or_else(|| "invalid aligned WASAPI buffer size or sample rate".to_string())?;
            drop(client);
            client = activate()?;
            initialize_exclusive_period(&client, format, period).map_err(|e| {
                format!("failed to initialize aligned WASAPI exclusive stream: {e}")
            })?;
        }
        Err(err) => {
            return Err(format!(
                "failed to initialize WASAPI exclusive stream: {err}"
            ));
        }
    }
    Ok(InitializedClient {
        client,
        period_frames: None,
        path: "IAudioClient exclusive",
    })
}

fn initialize_exclusive_period(
    client: &Audio::IAudioClient,
    format: &[u8],
    period: i64,
) -> windows::core::Result<()> {
    // SAFETY: the uninitialized client and valid format are live for this call.
    // Exclusive event streams require equal nonzero duration and periodicity.
    unsafe {
        client.Initialize(
            Audio::AUDCLNT_SHAREMODE_EXCLUSIVE,
            Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            period,
            period,
            waveformat(format),
            None,
        )
    }
}

fn aligned_period_hns(frames: u32, rate: u32) -> Option<i64> {
    if frames == 0 || rate == 0 {
        return None;
    }
    // REFERENCE_TIME uses 100 ns units. Round to nearest, as in Microsoft's
    // alignment example; u64 also covers the largest u32 frame count.
    Some(((u64::from(frames) * 10_000_000 + u64::from(rate) / 2) / u64::from(rate)) as i64)
}

pub(super) fn validate_shared_format(
    client: &Audio::IAudioClient,
    format: &[u8],
) -> Result<(), String> {
    let mut closest = std::ptr::null_mut();
    // SAFETY: format is a live waveform descriptor, and closest receives either
    // null or a COM-task allocation which is freed exactly once below.
    let status = unsafe {
        client.IsFormatSupported(
            Audio::AUDCLNT_SHAREMODE_SHARED,
            waveformat(format),
            Some(&mut closest),
        )
    };
    if !closest.is_null() {
        // SAFETY: IsFormatSupported allocated this pointer with CoTaskMemAlloc.
        unsafe { free_com_task_mem(closest.cast()) };
    }
    if status == Foundation::S_OK {
        Ok(())
    } else {
        Err(format!("shared format not supported exactly: {status:?}"))
    }
}

pub(super) fn exclusive_format(
    client: &Audio::IAudioClient,
    format: Vec<u8>,
    device_name: &str,
) -> Result<Vec<u8>, String> {
    let rate = waveformat(&format).nSamplesPerSec;
    let channels = waveformat(&format).nChannels;
    select_exclusive_format(format, |candidate| {
        // SAFETY: candidate is a valid waveform descriptor. Exclusive queries
        // do not return a closest-match allocation.
        let status = unsafe {
            client.IsFormatSupported(Audio::AUDCLNT_SHAREMODE_EXCLUSIVE, waveformat(candidate), None)
        };
        if status == Foundation::S_OK {
            Ok(true)
        } else if status == Audio::AUDCLNT_E_UNSUPPORTED_FORMAT {
            Ok(false)
        } else {
            Err(format!("WASAPI exclusive IsFormatSupported failed: {status:?}"))
        }
    })?.ok_or_else(|| format!(
        "No supported WASAPI exclusive float32/PCM16 format for '{device_name}': {rate} Hz, {channels} ch"
    ))
}

fn select_exclusive_format(
    mut format: Vec<u8>,
    mut supported: impl FnMut(&[u8]) -> Result<bool, String>,
) -> Result<Option<Vec<u8>>, String> {
    if sample_format_from_waveformat(&format).is_some() && supported(&format)? {
        return Ok(Some(format));
    }
    // Shared mix formats are commonly float32 even on integer-only hardware.
    // Keep the rate/channel layout, using the mixer's existing PCM16 output.
    let wave = waveformat_mut(&mut format);
    wave.wBitsPerSample = 16;
    wave.nBlockAlign = wave
        .nChannels
        .checked_mul(2)
        .ok_or_else(|| "too many channels in WASAPI format".to_string())?;
    wave.nAvgBytesPerSec = wave
        .nSamplesPerSec
        .checked_mul(u32::from(wave.nBlockAlign))
        .ok_or_else(|| "WASAPI format byte rate overflow".to_string())?;
    if u32::from(wave.wFormatTag) == KernelStreaming::WAVE_FORMAT_EXTENSIBLE {
        // SAFETY: the extensible format is a packed structure copied from
        // GetMixFormat, with its channel mask and all extension bytes retained.
        let ext = unsafe { &mut *(format.as_mut_ptr().cast::<Audio::WAVEFORMATEXTENSIBLE>()) };
        ext.Samples.wValidBitsPerSample = 16;
        ext.SubFormat = KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM;
    } else {
        wave.wFormatTag = Audio::WAVE_FORMAT_PCM as u16;
        wave.cbSize = 0;
        format.truncate(size_of::<Audio::WAVEFORMATEX>());
    }
    if supported(&format)? {
        return Ok(Some(format));
    }
    // Some mono/stereo drivers accept PCM WAVEFORMATEX but reject the equivalent
    // extensible descriptor. Multichannel layouts must retain their channel mask.
    if waveformat(&format).nChannels <= 2
        && u32::from(waveformat(&format).wFormatTag) == KernelStreaming::WAVE_FORMAT_EXTENSIBLE
    {
        let wave = waveformat_mut(&mut format);
        wave.wFormatTag = Audio::WAVE_FORMAT_PCM as u16;
        wave.cbSize = 0;
        format.truncate(size_of::<Audio::WAVEFORMATEX>());
        if supported(&format)? {
            return Ok(Some(format));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "../tests/negotiation/mod.rs"]
mod tests;
