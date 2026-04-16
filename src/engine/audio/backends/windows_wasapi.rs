use super::super::{
    OutputBackendReady, OutputTelemetryClock, OutputTimingQuality, QueuedSfx, RenderState,
    internal, publish_output_timing,
};
use crate::engine::windows_rt::{ThreadRole, boost_current_thread};
use log::{error, warn};
use std::mem::size_of;
use std::slice;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};
use windows::Win32::Devices::FunctionDiscovery;
use windows::Win32::Foundation::{self, CloseHandle, HANDLE, WAIT_FAILED};
use windows::Win32::Media::{Audio, KernelStreaming, Multimedia};
use windows::Win32::System::Com::StructuredStorage;
use windows::Win32::System::{Com, Threading, Variant};
use windows::core::{PCWSTR, PWSTR};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WasapiAccessMode {
    Shared,
    Exclusive,
}

impl WasapiAccessMode {
    #[inline(always)]
    const fn backend_name(self) -> &'static str {
        match self {
            Self::Shared => "wasapi-shared",
            Self::Exclusive => "wasapi-exclusive",
        }
    }
}

#[derive(Clone, Copy)]
enum WasapiSampleFormat {
    I16,
    F32,
}

impl WasapiSampleFormat {
    #[inline(always)]
    const fn sample_size(self) -> usize {
        match self {
            Self::I16 => size_of::<i16>(),
            Self::F32 => size_of::<f32>(),
        }
    }
}

pub(crate) struct WasapiOutputPrep {
    device_id: Option<String>,
    device_name: String,
    format: Vec<u8>,
    sample_rate_hz: u32,
    channels: usize,
    bytes_per_frame: u16,
    sample_format: WasapiSampleFormat,
    mode: WasapiAccessMode,
}

impl WasapiOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: self.mode.backend_name(),
            requested_output_mode: match self.mode {
                WasapiAccessMode::Shared => crate::config::AudioOutputMode::Shared,
                WasapiAccessMode::Exclusive => crate::config::AudioOutputMode::Exclusive,
            },
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::DeviceQpc,
            timing_quality: OutputTimingQuality::Trusted,
        }
    }
}

pub(crate) struct WasapiOutputStream {
    thread: Option<JoinHandle<()>>,
    stop_event: HANDLE,
}

impl Drop for WasapiOutputStream {
    fn drop(&mut self) {
        // SAFETY: `stop_event` is a live manual-reset event handle owned by this
        // stream until drop. Signaling it is the shutdown path for the render
        // thread.
        unsafe {
            let _ = Threading::SetEvent(self.stop_event);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // SAFETY: `stop_event` is still owned by this stream and is closed exactly
        // once here after the render thread has exited.
        unsafe {
            let _ = CloseHandle(self.stop_event);
        }
    }
}

pub(crate) struct WasapiOutputDevice {
    pub id: String,
    pub name: String,
    pub sample_rates_hz: Vec<u32>,
    pub mix_rate_hz: u32,
    pub channels: usize,
    pub is_default: bool,
}

pub(crate) fn enumerate_output_devices() -> Result<Vec<WasapiOutputDevice>, String> {
    let _com = ComGuard::new()?;
    let enumerator = create_device_enumerator()?;
    // SAFETY: COM is initialized for this thread, `enumerator` is a live COM
    // object, and the returned endpoint/device interfaces are owned values.
    let default_device_id = unsafe {
        enumerator
            .GetDefaultAudioEndpoint(Audio::eRender, Audio::eConsole)
            .ok()
            .and_then(|device| device_id_string(&device).ok())
    };
    // SAFETY: COM is initialized for this thread and `enumerator` is a live COM
    // object. The collection object owns the returned endpoint list.
    let collection = unsafe {
        enumerator
            .EnumAudioEndpoints(Audio::eRender, Audio::DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("failed to enumerate WASAPI output devices: {e}"))?
    };
    // SAFETY: `collection` is a live COM object returned above, and the out value
    // is managed internally by the windows crate.
    let count = unsafe {
        collection
            .GetCount()
            .map_err(|e| format!("failed to query WASAPI output device count: {e}"))?
    };
    let mut devices = Vec::with_capacity(count as usize);
    for index in 0..count {
        // SAFETY: `index` is within `0..count`, and `collection` remains live for
        // the duration of the call.
        let device = unsafe {
            collection
                .Item(index)
                .map_err(|e| format!("failed to open WASAPI output device {index}: {e}"))?
        };
        let id = device_id_string(&device)?;
        let name = device_friendly_name(&device).unwrap_or_else(|err| {
            warn!("failed to query WASAPI device name for '{id}': {err}");
            id.clone()
        });
        let (mix_rate_hz, channels) = device_mix_format(&device).unwrap_or_else(|err| {
            warn!("failed to probe WASAPI mix format for '{name}': {err}");
            (48_000, 2)
        });
        devices.push(WasapiOutputDevice {
            is_default: default_device_id.as_deref() == Some(id.as_str()),
            id,
            name,
            sample_rates_hz: vec![mix_rate_hz],
            mix_rate_hz,
            channels,
        });
    }
    if !devices.iter().any(|device| device.is_default)
        && let Some(device) = devices.first_mut()
    {
        device.is_default = true;
    }
    Ok(devices)
}

pub(crate) fn prepare(
    device_id: Option<String>,
    device_name: String,
    requested_rate_hz: Option<u32>,
    mode: WasapiAccessMode,
) -> Result<WasapiOutputPrep, String> {
    let _com = ComGuard::new()?;
    let device = open_output_device(device_id.as_deref())?;
    let audio_client = build_audio_client(&device)?;
    let mix_format = get_mix_format_bytes(&audio_client)?;
    let mut chosen_format = mix_format.clone();
    if let Some(rate_hz) = requested_rate_hz.filter(|rate| *rate > 0) {
        set_waveformat_sample_rate(&mut chosen_format, rate_hz);
        match mode {
            WasapiAccessMode::Shared => {
                if let Err(err) = initialize_shared(&audio_client, &chosen_format) {
                    warn!(
                        "WASAPI shared sample rate override {} Hz rejected for '{}': {err}. Using mix format.",
                        rate_hz, device_name
                    );
                    chosen_format = mix_format;
                }
            }
            WasapiAccessMode::Exclusive => {
                validate_exclusive_format(&audio_client, &chosen_format, &device_name)?;
            }
        }
    } else {
        match mode {
            WasapiAccessMode::Shared => initialize_shared(&audio_client, &chosen_format)?,
            WasapiAccessMode::Exclusive => {
                validate_exclusive_format(&audio_client, &chosen_format, &device_name)?
            }
        }
    }

    let sample_format = sample_format_from_waveformat(&chosen_format)
        .ok_or_else(|| format!("unsupported WASAPI mix format for '{}'", device_name))?;
    let sample_rate_hz = waveformat(&chosen_format).nSamplesPerSec;
    let channels = waveformat(&chosen_format).nChannels as usize;
    let bytes_per_frame = waveformat(&chosen_format).nBlockAlign;
    Ok(WasapiOutputPrep {
        device_id,
        device_name,
        format: chosen_format,
        sample_rate_hz,
        channels,
        bytes_per_frame,
        sample_format,
        mode,
    })
}

pub(crate) fn start(
    prep: WasapiOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<WasapiOutputStream, String> {
    // SAFETY: creating an unnamed auto-reset event requires no borrowed Rust
    // memory; the returned handle is owned by the caller and closed on all exit
    // paths below.
    let stop_event = unsafe { Threading::CreateEventW(None, false, false, PCWSTR::null()) }
        .map_err(|e| format!("failed to create WASAPI stop event: {e}"))?;
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    let stop_event_thread = stop_event.0 as isize;
    let thread = thread::Builder::new()
        .name("wasapi_out".to_string())
        .spawn(move || {
            render_thread(
                prep,
                music_ring,
                sfx_receiver,
                HANDLE(stop_event_thread as *mut _),
                ready_tx,
            )
        })
        .map_err(|e| {
            // SAFETY: thread spawn failed, so ownership of `stop_event` never left
            // this function and it must be closed here.
            unsafe {
                let _ = CloseHandle(stop_event);
            }
            format!("failed to spawn WASAPI render thread: {e}")
        })?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(WasapiOutputStream {
            thread: Some(thread),
            stop_event,
        }),
        Ok(Err(err)) => {
            let _ = thread.join();
            // SAFETY: startup failed, so this function still owns `stop_event` and
            // closes it before returning the error.
            unsafe {
                let _ = CloseHandle(stop_event);
            }
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
            // SAFETY: the render thread exited before taking over steady-state
            // ownership, so this function closes the event handle on the error path.
            unsafe {
                let _ = CloseHandle(stop_event);
            }
            Err("WASAPI render thread exited during startup".to_string())
        }
    }
}

fn render_thread(
    prep: WasapiOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_event: HANDLE,
    ready_tx: Sender<Result<(), String>>,
) {
    let _thread_policy = boost_current_thread(ThreadRole::AudioRender);
    if let Err(err) = render_thread_inner(prep, music_ring, sfx_receiver, stop_event, &ready_tx)
        && ready_tx.send(Err(err.clone())).is_err()
    {
        error!("WASAPI render thread failed: {err}");
    }
}

fn render_thread_inner(
    prep: WasapiOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_event: HANDLE,
    ready_tx: &Sender<Result<(), String>>,
) -> Result<(), String> {
    let _com = ComGuard::new()?;
    let device = open_output_device(prep.device_id.as_deref())?;
    let audio_client = build_audio_client(&device)?;
    initialize_client(&audio_client, &prep)?;
    // SAFETY: creating an unnamed auto-reset event requires no borrowed Rust
    // memory; the returned handle is owned within this function and closed on all
    // exit paths below.
    let event = unsafe { Threading::CreateEventW(None, false, false, PCWSTR::null()) }
        .map_err(|e| format!("failed to create WASAPI event handle: {e}"))?;
    // SAFETY: `event` is a live event handle created above, and `audio_client` is
    // a live initialized WASAPI client that accepts an event callback handle.
    unsafe {
        audio_client
            .SetEventHandle(event)
            .map_err(|e| format!("failed to set WASAPI event handle: {e}"))?;
    }
    // SAFETY: `audio_client` is initialized and alive, so requesting its render
    // service yields a live COM interface owned by this function.
    let render_client = unsafe {
        audio_client
            .GetService::<Audio::IAudioRenderClient>()
            .map_err(|e| format!("failed to acquire WASAPI render client: {e}"))?
    };
    // SAFETY: `audio_client` is initialized and alive, so requesting its clock
    // service yields a live COM interface owned by this function.
    let audio_clock = unsafe {
        audio_client
            .GetService::<Audio::IAudioClock>()
            .map_err(|e| format!("failed to acquire WASAPI audio clock: {e}"))?
    };
    let device_period_ns = match query_device_periods_hns(&audio_client) {
        Ok((default_period_hns, min_period_hns)) => reference_time_to_nanos(
            selected_device_period_hns(prep.mode, default_period_hns, min_period_hns),
        ),
        Err(err) => {
            warn!(
                "failed to query WASAPI device period for '{}': {err}",
                prep.device_name
            );
            0
        }
    };
    let stream_latency_ns = match query_stream_latency_ns(&audio_client) {
        Ok(latency_ns) => latency_ns,
        Err(err) => {
            warn!(
                "failed to query WASAPI stream latency for '{}': {err}",
                prep.device_name
            );
            0
        }
    };
    // SAFETY: `audio_client` is initialized and alive, and `GetBufferSize` only
    // writes to caller-managed stack locals through the windows bindings.
    let max_frames_in_buffer = unsafe {
        audio_client
            .GetBufferSize()
            .map_err(|e| format!("failed to query WASAPI buffer size: {e}"))?
    };

    let mut render = RenderState::new(music_ring, sfx_receiver, prep.channels);
    write_frames(
        &audio_clock,
        &render_client,
        &mut render,
        &prep,
        max_frames_in_buffer,
        max_frames_in_buffer,
    )?;
    publish_output_timing(
        prep.sample_rate_hz,
        device_period_ns,
        stream_latency_ns,
        max_frames_in_buffer,
        max_frames_in_buffer,
        max_frames_in_buffer,
        estimated_output_delay_ns(
            prep.sample_rate_hz,
            max_frames_in_buffer,
            device_period_ns,
            stream_latency_ns,
        ),
    );
    // SAFETY: `audio_client` is fully initialized and primed with one buffer fill
    // above, so starting the stream is valid here.
    unsafe {
        audio_client
            .Start()
            .map_err(|e| format!("failed to start WASAPI output: {e}"))?;
    }
    if ready_tx.send(Ok(())).is_err() {
        // SAFETY: startup aborts before handing control back to the caller, so we
        // stop the stream if it started and close the local event handle here.
        unsafe {
            let _ = audio_client.Stop();
            let _ = CloseHandle(event);
        }
        return Ok(());
    }

    let handles = [stop_event, event];
    let result = loop {
        // SAFETY: both handles in `handles` are valid event handles that remain
        // alive for the duration of this wait call.
        let wait = unsafe {
            Threading::WaitForMultipleObjectsEx(&handles, false, Threading::INFINITE, false)
        };
        if wait == WAIT_FAILED {
            // SAFETY: `GetLastError` reads the thread-local Windows error state and
            // takes no borrowed Rust memory.
            let err = unsafe { Foundation::GetLastError() };
            break Err(format!("WaitForMultipleObjectsEx failed: {err:?}"));
        }
        let idx = wait.0.saturating_sub(Foundation::WAIT_OBJECT_0.0) as usize;
        if idx == 0 {
            break Ok(());
        }
        // SAFETY: `audio_client` remains alive and started while the render loop
        // runs, so querying current padding is valid here.
        let padding = unsafe {
            audio_client
                .GetCurrentPadding()
                .map_err(|e| format!("failed to query WASAPI padding: {e}"))
        }?;
        let frames_available = max_frames_in_buffer.saturating_sub(padding);
        publish_output_timing(
            prep.sample_rate_hz,
            device_period_ns,
            stream_latency_ns,
            max_frames_in_buffer,
            padding,
            padding,
            estimated_output_delay_ns(
                prep.sample_rate_hz,
                padding,
                device_period_ns,
                stream_latency_ns,
            ),
        );
        if frames_available == 0 {
            continue;
        }
        write_frames(
            &audio_clock,
            &render_client,
            &mut render,
            &prep,
            frames_available,
            frames_available,
        )?;
    };

    // SAFETY: shutdown runs while `audio_client` and `event` are still owned by
    // this function; stopping the client and closing the local event handle are
    // the correct teardown steps.
    unsafe {
        let _ = audio_client.Stop();
        let _ = CloseHandle(event);
    }
    result
}

fn write_frames(
    audio_clock: &Audio::IAudioClock,
    render_client: &Audio::IAudioRenderClient,
    render: &mut RenderState,
    prep: &WasapiOutputPrep,
    frames_available: u32,
    playback_delay_frames: u32,
) -> Result<(), String> {
    if frames_available == 0 {
        return Ok(());
    }
    // SAFETY: `render_client` owns the buffer returned by `GetBuffer` for exactly
    // `frames_available` frames. We reinterpret that memory according to the
    // negotiated sample format and release the same frame count before returning.
    unsafe {
        let buffer = render_client
            .GetBuffer(frames_available)
            .map_err(|e| format!("failed to get WASAPI output buffer: {e}"))?;
        let samples = frames_available as usize * prep.bytes_per_frame as usize
            / prep.sample_format.sample_size();
        let anchor_nanos = playback_anchor_nanos_after_frames(
            audio_clock,
            prep.sample_rate_hz,
            playback_delay_frames,
        )?;
        match prep.sample_format {
            WasapiSampleFormat::I16 => {
                let out = slice::from_raw_parts_mut(buffer as *mut i16, samples);
                render.render_i16_qpc(out, anchor_nanos);
            }
            WasapiSampleFormat::F32 => {
                let out = slice::from_raw_parts_mut(buffer as *mut f32, samples);
                render.render_f32_qpc(out, anchor_nanos);
            }
        }
        render_client
            .ReleaseBuffer(frames_available, 0)
            .map_err(|e| format!("failed to release WASAPI output buffer: {e}"))?;
    }
    Ok(())
}

#[inline(always)]
fn playback_anchor_nanos_after_frames(
    audio_clock: &Audio::IAudioClock,
    sample_rate_hz: u32,
    frames: u32,
) -> Result<u64, String> {
    let mut _position = 0u64;
    let mut qpc_position = 0u64;
    // SAFETY: `audio_clock` is a live WASAPI clock interface, and both out
    // pointers reference writable stack locals for the duration of the call.
    unsafe {
        audio_clock
            .GetPosition(&mut _position, Some(&mut qpc_position))
            .map_err(|e| format!("failed to query WASAPI audio clock position: {e}"))?;
    }
    Ok(qpc_position
        .saturating_mul(100)
        .saturating_add(frames_to_nanos(sample_rate_hz, frames))
        .min(u64::MAX - 1))
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    if sample_rate_hz == 0 || frames == 0 {
        return 0;
    }
    (u64::from(frames) * 1_000_000_000) / u64::from(sample_rate_hz)
}

#[inline(always)]
fn reference_time_to_nanos(hns: i64) -> u64 {
    hns.max(0) as u64 * 100
}

#[inline(always)]
fn query_stream_latency_ns(audio_client: &Audio::IAudioClient) -> Result<u64, String> {
    // SAFETY: `audio_client` is a live initialized WASAPI client, and
    // `GetStreamLatency` returns a plain scalar value through the windows bindings.
    unsafe {
        audio_client
            .GetStreamLatency()
            .map(reference_time_to_nanos)
            .map_err(|e| format!("GetStreamLatency failed: {e}"))
    }
}

#[inline(always)]
fn estimated_output_delay_ns(
    sample_rate_hz: u32,
    queued_frames: u32,
    device_period_ns: u64,
    stream_latency_ns: u64,
) -> u64 {
    let queue_delay_ns = frames_to_nanos(sample_rate_hz, queued_frames);
    let downstream_ns = if stream_latency_ns != 0 {
        stream_latency_ns
    } else {
        device_period_ns
    };
    queue_delay_ns.saturating_add(downstream_ns)
}

fn initialize_client(
    audio_client: &Audio::IAudioClient,
    prep: &WasapiOutputPrep,
) -> Result<(), String> {
    match prep.mode {
        WasapiAccessMode::Shared => initialize_shared(audio_client, &prep.format),
        WasapiAccessMode::Exclusive => initialize_exclusive(audio_client, &prep.format),
    }
}

fn initialize_exclusive(audio_client: &Audio::IAudioClient, format: &[u8]) -> Result<(), String> {
    let (default_period_hns, min_period_hns) = query_device_periods_hns(audio_client)?;
    let period_hns = selected_device_period_hns(
        WasapiAccessMode::Exclusive,
        default_period_hns,
        min_period_hns,
    );
    // SAFETY: `audio_client` is a live COM interface, and `format` points to a
    // valid `WAVEFORMATEX`/`WAVEFORMATEXTENSIBLE` byte buffer owned by the caller.
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

fn validate_exclusive_format(
    audio_client: &Audio::IAudioClient,
    format: &[u8],
    device_name: &str,
) -> Result<(), String> {
    // SAFETY: `audio_client` is a live COM interface, and `format` points to a
    // valid waveform description buffer for the duration of the call.
    let status = unsafe {
        audio_client.IsFormatSupported(Audio::AUDCLNT_SHAREMODE_EXCLUSIVE, waveformat(format), None)
    };
    if status.is_ok() {
        return Ok(());
    }
    if status == Audio::AUDCLNT_E_UNSUPPORTED_FORMAT {
        let wave = waveformat(format);
        let sample_rate_hz = wave.nSamplesPerSec;
        let channels = wave.nChannels;
        let bits_per_sample = wave.wBitsPerSample;
        return Err(format!(
            "WASAPI exclusive format not supported for '{}': {} Hz, {} ch, {} bits",
            device_name, sample_rate_hz, channels, bits_per_sample
        ));
    }
    Err(format!(
        "WASAPI exclusive IsFormatSupported failed for '{}': {status:?}",
        device_name
    ))
}

#[inline(always)]
fn query_device_periods_hns(audio_client: &Audio::IAudioClient) -> Result<(i64, i64), String> {
    let mut default_period = 0i64;
    let mut min_period = 0i64;
    // SAFETY: `audio_client` is a live COM interface, and both out pointers refer
    // to writable stack locals for the duration of the call.
    unsafe {
        audio_client
            .GetDevicePeriod(Some(&mut default_period), Some(&mut min_period))
            .map_err(|e| format!("GetDevicePeriod failed: {e}"))?;
    }
    Ok((default_period, min_period))
}

#[inline(always)]
fn selected_device_period_hns(mode: WasapiAccessMode, default_hns: i64, min_hns: i64) -> i64 {
    match mode {
        WasapiAccessMode::Shared => default_hns.max(0),
        WasapiAccessMode::Exclusive => {
            let preferred = if min_hns > 0 { min_hns } else { default_hns };
            preferred.max(0)
        }
    }
}

struct ComGuard;

impl ComGuard {
    fn new() -> Result<Self, String> {
        // SAFETY: this thread owns its COM initialization state. We initialize it
        // once for multithreaded use and balance it with `CoUninitialize` in Drop.
        unsafe {
            Com::CoInitializeEx(None, Com::COINIT_MULTITHREADED)
                .ok()
                .map_err(|e| format!("failed to initialize COM for WASAPI: {e}"))?;
        }
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        // SAFETY: this balances the successful `CoInitializeEx` call in `new()`
        // for the same thread.
        unsafe {
            Com::CoUninitialize();
        }
    }
}

fn create_device_enumerator() -> Result<Audio::IMMDeviceEnumerator, String> {
    // SAFETY: COM is initialized on this thread, and `CoCreateInstance` returns a
    // COM interface object whose lifetime is managed by the windows crate.
    unsafe {
        Com::CoCreateInstance::<_, Audio::IMMDeviceEnumerator>(
            &Audio::MMDeviceEnumerator,
            None,
            Com::CLSCTX_ALL,
        )
        .map_err(|e| format!("failed to create WASAPI device enumerator: {e}"))
    }
}

fn open_output_device(device_id: Option<&str>) -> Result<Audio::IMMDevice, String> {
    // SAFETY: COM is initialized on this thread. Any UTF-16 buffer we build for
    // `device_id` stays alive for the duration of the call.
    unsafe {
        let enumerator = create_device_enumerator()?;
        match device_id {
            Some(device_id) => {
                let wide = wide_null(device_id);
                enumerator
                    .GetDevice(PCWSTR(wide.as_ptr()))
                    .map_err(|e| format!("failed to open WASAPI output device '{device_id}': {e}"))
            }
            None => enumerator
                .GetDefaultAudioEndpoint(Audio::eRender, Audio::eConsole)
                .map_err(|e| format!("failed to open default WASAPI output device: {e}")),
        }
    }
}

fn device_id_string(device: &Audio::IMMDevice) -> Result<String, String> {
    // SAFETY: `device` is a live COM object. `GetId` returns CoTaskMem-allocated
    // memory that we free exactly once with `CoTaskMemFree` after converting it.
    unsafe {
        let id = device
            .GetId()
            .map_err(|e| format!("failed to query WASAPI device id: {e}"))?;
        let text = pwstr_to_string(id);
        Com::CoTaskMemFree(Some(id.0.cast()));
        text.map_err(|e| format!("failed to decode WASAPI device id: {e}"))
    }
}

fn device_friendly_name(device: &Audio::IMMDevice) -> Result<String, String> {
    // SAFETY: `device` and the property store it returns are live COM objects.
    // `PropVariantClear` is called exactly once to release any owned variant
    // storage before returning.
    unsafe {
        let store = device
            .OpenPropertyStore(Com::STGM_READ)
            .map_err(|e| format!("OpenPropertyStore failed: {e}"))?;
        let mut value = store
            .GetValue(&FunctionDiscovery::PKEY_Device_FriendlyName)
            .map_err(|e| format!("GetValue(PKEY_Device_FriendlyName) failed: {e}"))?;
        let name = propvariant_lpwstr(&value)
            .ok_or_else(|| "device friendly name was not a UTF-16 string".to_string());
        let _ = StructuredStorage::PropVariantClear(&mut value);
        name
    }
}

fn device_mix_format(device: &Audio::IMMDevice) -> Result<(u32, usize), String> {
    let audio_client = build_audio_client(device)?;
    let mix_format = get_mix_format_bytes(&audio_client)?;
    let wave = waveformat(&mix_format);
    Ok((wave.nSamplesPerSec.max(1), wave.nChannels.max(1) as usize))
}

fn build_audio_client(device: &Audio::IMMDevice) -> Result<Audio::IAudioClient, String> {
    // SAFETY: `device` is a live WASAPI endpoint COM object, and activation
    // returns an owned `IAudioClient` interface.
    unsafe {
        device
            .Activate::<Audio::IAudioClient>(Com::CLSCTX_ALL, None)
            .map_err(|e| format!("failed to activate WASAPI audio client: {e}"))
    }
}

fn initialize_shared(audio_client: &Audio::IAudioClient, format: &[u8]) -> Result<(), String> {
    // SAFETY: `audio_client` is a live COM interface, and `format` points to a
    // valid `WAVEFORMATEX`/`WAVEFORMATEXTENSIBLE` byte buffer owned by the caller.
    unsafe {
        audio_client
            .Initialize(
                Audio::AUDCLNT_SHAREMODE_SHARED,
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                0,
                0,
                waveformat(format),
                None,
            )
            .map_err(|e| format!("failed to initialize WASAPI shared stream: {e}"))
    }
}

fn get_mix_format_bytes(audio_client: &Audio::IAudioClient) -> Result<Vec<u8>, String> {
    // SAFETY: `audio_client` is a live COM interface. `GetMixFormat` returns a
    // CoTaskMem-allocated waveform structure that we copy into a Rust `Vec<u8>`
    // and then free exactly once with `CoTaskMemFree`.
    unsafe {
        let mix = audio_client
            .GetMixFormat()
            .map_err(|e| format!("failed to query WASAPI mix format: {e}"))?;
        let len = size_of::<Audio::WAVEFORMATEX>() + usize::from((*mix).cbSize);
        let bytes = slice::from_raw_parts(mix as *const u8, len).to_vec();
        Com::CoTaskMemFree(Some(mix as *mut _));
        Ok(bytes)
    }
}

#[inline(always)]
fn waveformat(bytes: &[u8]) -> &Audio::WAVEFORMATEX {
    // SAFETY: all callers pass byte buffers originating from WASAPI waveform
    // structures, so the prefix is a valid `WAVEFORMATEX` for the lifetime of the
    // slice borrow.
    unsafe { &*(bytes.as_ptr() as *const Audio::WAVEFORMATEX) }
}

#[inline(always)]
fn waveformat_mut(bytes: &mut [u8]) -> &mut Audio::WAVEFORMATEX {
    // SAFETY: all callers pass mutable byte buffers containing a writable WASAPI
    // waveform structure, so the prefix may be viewed as `WAVEFORMATEX`.
    unsafe { &mut *(bytes.as_mut_ptr() as *mut Audio::WAVEFORMATEX) }
}

fn set_waveformat_sample_rate(bytes: &mut [u8], rate_hz: u32) {
    let wave = waveformat_mut(bytes);
    wave.nSamplesPerSec = rate_hz;
    wave.nAvgBytesPerSec = rate_hz.saturating_mul(u32::from(wave.nBlockAlign));
}

fn sample_format_from_waveformat(bytes: &[u8]) -> Option<WasapiSampleFormat> {
    let wave = waveformat(bytes);
    match (wave.wFormatTag as u32, wave.wBitsPerSample) {
        (Audio::WAVE_FORMAT_PCM, 16) => Some(WasapiSampleFormat::I16),
        (Multimedia::WAVE_FORMAT_IEEE_FLOAT, 32) => Some(WasapiSampleFormat::F32),
        (tag, bits) if tag == KernelStreaming::WAVE_FORMAT_EXTENSIBLE => {
            // SAFETY: this branch is only taken for `WAVE_FORMAT_EXTENSIBLE`, so
            // the buffer contains a `WAVEFORMATEXTENSIBLE` header after the shared
            // `WAVEFORMATEX` prefix.
            let ext = unsafe { &*(bytes.as_ptr() as *const Audio::WAVEFORMATEXTENSIBLE) };
            // SAFETY: `SubFormat` may be only naturally aligned inside the byte
            // buffer, so we read it with `read_unaligned`.
            let sub = unsafe { std::ptr::addr_of!(ext.SubFormat).read_unaligned() };
            if guid_eq(&sub, &KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM) && bits == 16 {
                Some(WasapiSampleFormat::I16)
            } else if guid_eq(&sub, &Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT) && bits == 32 {
                Some(WasapiSampleFormat::F32)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[inline(always)]
fn guid_eq(a: &windows::core::GUID, b: &windows::core::GUID) -> bool {
    (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
}

fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn pwstr_to_string(value: PWSTR) -> Result<String, std::string::FromUtf16Error> {
    // SAFETY: callers only pass live Windows-owned UTF-16 strings and the windows
    // crate performs the length scan/conversion before the backing allocation is
    // freed by the caller.
    unsafe { value.to_string() }
}

fn propvariant_lpwstr(value: &StructuredStorage::PROPVARIANT) -> Option<String> {
    // SAFETY: `value` is a live `PROPVARIANT` owned by the caller. We inspect its
    // tagged union only after checking `vt`, and convert the contained pointer only
    // when it is non-null.
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        if inner.vt != Variant::VT_LPWSTR {
            return None;
        }
        let text = inner.Anonymous.pwszVal;
        if text.is_null() {
            return None;
        }
        text.to_string().ok()
    }
}
