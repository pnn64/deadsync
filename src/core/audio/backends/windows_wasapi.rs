use super::super::{
    OutputBackendReady, QueuedSfx, RenderState, internal, playback_anchor_after_frames,
};
use log::{error, warn};
use std::mem::size_of;
use std::slice;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{self, CloseHandle, HANDLE, WAIT_FAILED};
use windows::Win32::Media::{Audio, KernelStreaming, Multimedia};
use windows::Win32::System::Com;
use windows::Win32::System::Threading;
use windows::core::PCWSTR;

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
}

impl WasapiOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: "wasapi-shared",
        }
    }
}

pub(crate) struct WasapiOutputStream {
    thread: Option<JoinHandle<()>>,
    stop_event: HANDLE,
}

impl Drop for WasapiOutputStream {
    fn drop(&mut self) {
        unsafe {
            let _ = Threading::SetEvent(self.stop_event);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        unsafe {
            let _ = CloseHandle(self.stop_event);
        }
    }
}

pub(crate) fn prepare(
    device_id: Option<String>,
    device_name: String,
    requested_rate_hz: Option<u32>,
) -> Result<WasapiOutputPrep, String> {
    let _com = ComGuard::new()?;
    let device = open_output_device(device_id.as_deref())?;
    let audio_client = build_audio_client(&device)?;
    let mix_format = get_mix_format_bytes(&audio_client)?;
    let mut chosen_format = mix_format.clone();
    if let Some(rate_hz) = requested_rate_hz.filter(|rate| *rate > 0) {
        set_waveformat_sample_rate(&mut chosen_format, rate_hz);
        if let Err(err) = initialize_shared(&audio_client, &chosen_format) {
            warn!(
                "WASAPI shared sample rate override {} Hz rejected for '{}': {err}. Using mix format.",
                rate_hz, device_name
            );
            chosen_format = mix_format;
        }
    } else {
        initialize_shared(&audio_client, &chosen_format)?;
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
    })
}

pub(crate) fn start(
    prep: WasapiOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<WasapiOutputStream, String> {
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
            unsafe {
                let _ = CloseHandle(stop_event);
            }
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
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
    if let Err(err) = render_thread_inner(prep, music_ring, sfx_receiver, stop_event, &ready_tx) {
        if ready_tx.send(Err(err.clone())).is_err() {
            error!("WASAPI render thread failed: {err}");
        }
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
    initialize_shared(&audio_client, &prep.format)?;
    let event = unsafe { Threading::CreateEventW(None, false, false, PCWSTR::null()) }
        .map_err(|e| format!("failed to create WASAPI event handle: {e}"))?;
    unsafe {
        audio_client
            .SetEventHandle(event)
            .map_err(|e| format!("failed to set WASAPI event handle: {e}"))?;
    }
    let render_client = unsafe {
        audio_client
            .GetService::<Audio::IAudioRenderClient>()
            .map_err(|e| format!("failed to acquire WASAPI render client: {e}"))?
    };
    let max_frames_in_buffer = unsafe {
        audio_client
            .GetBufferSize()
            .map_err(|e| format!("failed to query WASAPI buffer size: {e}"))?
    };

    let mut render = RenderState::new(music_ring, sfx_receiver, prep.channels);
    write_frames(
        &render_client,
        &mut render,
        &prep,
        max_frames_in_buffer,
        max_frames_in_buffer,
    )?;
    unsafe {
        audio_client
            .Start()
            .map_err(|e| format!("failed to start WASAPI output: {e}"))?;
    }
    if ready_tx.send(Ok(())).is_err() {
        unsafe {
            let _ = audio_client.Stop();
            let _ = CloseHandle(event);
        }
        return Ok(());
    }

    let handles = [stop_event, event];
    let result = loop {
        let wait = unsafe {
            Threading::WaitForMultipleObjectsEx(&handles, false, Threading::INFINITE, false)
        };
        if wait == WAIT_FAILED {
            let err = unsafe { Foundation::GetLastError() };
            break Err(format!("WaitForMultipleObjectsEx failed: {err:?}"));
        }
        let idx = wait.0.saturating_sub(Foundation::WAIT_OBJECT_0.0) as usize;
        if idx == 0 {
            break Ok(());
        }
        let padding = unsafe {
            audio_client
                .GetCurrentPadding()
                .map_err(|e| format!("failed to query WASAPI padding: {e}"))
        }?;
        let frames_available = max_frames_in_buffer.saturating_sub(padding);
        if frames_available == 0 {
            continue;
        }
        write_frames(
            &render_client,
            &mut render,
            &prep,
            frames_available,
            frames_available,
        )?;
    };

    unsafe {
        let _ = audio_client.Stop();
        let _ = CloseHandle(event);
    }
    result
}

fn write_frames(
    render_client: &Audio::IAudioRenderClient,
    render: &mut RenderState,
    prep: &WasapiOutputPrep,
    frames_available: u32,
    playback_delay_frames: u32,
) -> Result<(), String> {
    if frames_available == 0 {
        return Ok(());
    }
    unsafe {
        let buffer = render_client
            .GetBuffer(frames_available)
            .map_err(|e| format!("failed to get WASAPI output buffer: {e}"))?;
        let samples = frames_available as usize * prep.bytes_per_frame as usize
            / prep.sample_format.sample_size();
        let anchor_at = playback_anchor_after_frames(
            std::time::Instant::now(),
            prep.sample_rate_hz,
            playback_delay_frames,
        );
        match prep.sample_format {
            WasapiSampleFormat::I16 => {
                let out = slice::from_raw_parts_mut(buffer as *mut i16, samples);
                render.render_i16(out, anchor_at);
            }
            WasapiSampleFormat::F32 => {
                let out = slice::from_raw_parts_mut(buffer as *mut f32, samples);
                render.render_f32(out, anchor_at);
            }
        }
        render_client
            .ReleaseBuffer(frames_available, 0)
            .map_err(|e| format!("failed to release WASAPI output buffer: {e}"))?;
    }
    Ok(())
}

struct ComGuard;

impl ComGuard {
    fn new() -> Result<Self, String> {
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
        unsafe {
            Com::CoUninitialize();
        }
    }
}

fn open_output_device(device_id: Option<&str>) -> Result<Audio::IMMDevice, String> {
    unsafe {
        let enumerator = Com::CoCreateInstance::<_, Audio::IMMDeviceEnumerator>(
            &Audio::MMDeviceEnumerator,
            None,
            Com::CLSCTX_ALL,
        )
        .map_err(|e| format!("failed to create WASAPI device enumerator: {e}"))?;
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

fn build_audio_client(device: &Audio::IMMDevice) -> Result<Audio::IAudioClient, String> {
    unsafe {
        device
            .Activate::<Audio::IAudioClient>(Com::CLSCTX_ALL, None)
            .map_err(|e| format!("failed to activate WASAPI audio client: {e}"))
    }
}

fn initialize_shared(audio_client: &Audio::IAudioClient, format: &[u8]) -> Result<(), String> {
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
    unsafe { &*(bytes.as_ptr() as *const Audio::WAVEFORMATEX) }
}

#[inline(always)]
fn waveformat_mut(bytes: &mut [u8]) -> &mut Audio::WAVEFORMATEX {
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
            let ext = unsafe { &*(bytes.as_ptr() as *const Audio::WAVEFORMATEXTENSIBLE) };
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
