use super::super::{
    OutputBackendReady, OutputTelemetryClock, OutputTimingQuality, QueuedSfx, RenderState,
    internal, note_output_clock_fallback, publish_output_timing, publish_output_timing_quality,
};
use crate::core::host_time::now_nanos;
use libloading::Library;
use log::{info, warn};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_SAMPLE_S16LE: c_int = 3;
const PULSE_FALLBACK_BUFFER_FRAMES: u32 = 2048;

#[repr(C)]
struct PaSimple {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PaSampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PaBufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

type PaSimpleNewFn = unsafe extern "C" fn(
    server: *const c_char,
    name: *const c_char,
    dir: c_int,
    dev: *const c_char,
    stream_name: *const c_char,
    sample_spec: *const PaSampleSpec,
    channel_map: *const c_void,
    attr: *const PaBufferAttr,
    error: *mut c_int,
) -> *mut PaSimple;
type PaSimpleFreeFn = unsafe extern "C" fn(stream: *mut PaSimple);
type PaSimpleWriteFn = unsafe extern "C" fn(
    stream: *mut PaSimple,
    data: *const c_void,
    bytes: usize,
    error: *mut c_int,
) -> c_int;
type PaSimpleDrainFn = unsafe extern "C" fn(stream: *mut PaSimple, error: *mut c_int) -> c_int;
type PaSimpleGetLatencyFn = unsafe extern "C" fn(stream: *mut PaSimple, error: *mut c_int) -> u64;
type PaStrErrorFn = unsafe extern "C" fn(error: c_int) -> *const c_char;

struct PulseApi {
    _simple: Library,
    _pulse: Library,
    pa_simple_new: PaSimpleNewFn,
    pa_simple_free: PaSimpleFreeFn,
    pa_simple_write: PaSimpleWriteFn,
    pa_simple_drain: PaSimpleDrainFn,
    pa_simple_get_latency: PaSimpleGetLatencyFn,
    pa_strerror: PaStrErrorFn,
}

static PULSE_API: OnceLock<Result<PulseApi, String>> = OnceLock::new();

pub(crate) fn is_available() -> bool {
    pulse_api().is_ok()
}

fn pulse_api() -> Result<&'static PulseApi, String> {
    match PULSE_API.get_or_init(load_pulse_api) {
        Ok(api) => Ok(api),
        Err(err) => Err(err.clone()),
    }
}

fn load_pulse_api() -> Result<PulseApi, String> {
    let simple = load_library(&["libpulse-simple.so.0", "libpulse-simple.so"])?;
    let pulse = load_library(&["libpulse.so.0", "libpulse.so"])?;
    Ok(PulseApi {
        // SAFETY: the loaded shared object stays owned by the `PulseApi` struct for
        // at least as long as these copied function pointers are used.
        pa_simple_new: unsafe { load_symbol(&simple, b"pa_simple_new\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        pa_simple_free: unsafe { load_symbol(&simple, b"pa_simple_free\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        pa_simple_write: unsafe { load_symbol(&simple, b"pa_simple_write\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        pa_simple_drain: unsafe { load_symbol(&simple, b"pa_simple_drain\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        pa_simple_get_latency: unsafe { load_symbol(&simple, b"pa_simple_get_latency\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        pa_strerror: unsafe { load_symbol(&pulse, b"pa_strerror\0")? },
        _simple: simple,
        _pulse: pulse,
    })
}

fn load_library(names: &[&str]) -> Result<Library, String> {
    let mut last_err = None;
    for name in names {
        // SAFETY: loading a shared object is the intended `libloading` API here;
        // we keep the returned handle alive for the full lifetime of any symbols
        // resolved from it.
        match unsafe { Library::new(*name) } {
            Ok(lib) => return Ok(lib),
            Err(err) => last_err = Some(format!("{name}: {err}")),
        }
    }
    Err(last_err.unwrap_or_else(|| "no candidate library names were provided".to_string()))
}

unsafe fn load_symbol<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
    // SAFETY: the caller chooses `T` to match the actual symbol signature, and
    // `lib` remains alive after the copied function pointer is returned.
    unsafe { lib.get::<T>(name) }
        .map(|sym| *sym)
        .map_err(|err| {
            format!(
                "{}: {err}",
                String::from_utf8_lossy(name).trim_end_matches('\0')
            )
        })
}

pub(crate) struct PulseOutputPrep {
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    buffer_frames: u32,
}

impl PulseOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: "pulse-shared",
            requested_output_mode: crate::config::AudioOutputMode::Shared,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::Monotonic,
            timing_quality: OutputTimingQuality::Trusted,
        }
    }
}

pub(crate) struct PulseOutputStream {
    stop_flag: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for PulseOutputStream {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn prepare(
    requested_device_name: Option<String>,
    sample_rate_hz: u32,
    channels: usize,
) -> Result<PulseOutputPrep, String> {
    let sample_rate_hz = sample_rate_hz.max(1);
    let channels = channels.clamp(1, 32);
    let period_frames = suggested_period_frames(sample_rate_hz);
    let buffer_frames = period_frames.saturating_mul(4).max(period_frames);
    let device_name = match requested_device_name {
        Some(name) if !name.is_empty() => {
            format!("PulseAudio default sink (requested '{name}' unsupported)")
        }
        _ => "PulseAudio default sink".to_string(),
    };
    Ok(PulseOutputPrep {
        device_name,
        sample_rate_hz,
        channels,
        period_frames,
        buffer_frames,
    })
}

pub(crate) fn start(
    prep: PulseOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<PulseOutputStream, String> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    let thread = thread::Builder::new()
        .name("pulse_out".to_string())
        .spawn(move || render_thread(prep, music_ring, sfx_receiver, stop_flag_thread, ready_tx))
        .map_err(|e| format!("failed to spawn PulseAudio render thread: {e}"))?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(PulseOutputStream {
            stop_flag,
            thread: Some(thread),
        }),
        Ok(Err(err)) => {
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
            Err("PulseAudio render thread exited during startup".to_string())
        }
    }
}

struct PulseConnection {
    api: &'static PulseApi,
    raw: *mut PaSimple,
}

impl PulseConnection {
    fn open(prep: &PulseOutputPrep) -> Result<Self, String> {
        let api = pulse_api()?;
        let app_name = CString::new("deadsync").unwrap();
        let stream_name = CString::new("Gameplay").unwrap();
        let sample_spec = PaSampleSpec {
            format: PA_SAMPLE_S16LE,
            rate: prep.sample_rate_hz,
            channels: prep.channels as u8,
        };
        let buffer_attr = PaBufferAttr {
            maxlength: u32::MAX,
            tlength: frames_to_bytes(prep.buffer_frames, prep.channels),
            prebuf: u32::MAX,
            minreq: frames_to_bytes(prep.period_frames, prep.channels),
            fragsize: u32::MAX,
        };
        let mut error = 0;
        // SAFETY: all pointers passed to PulseAudio come from stack locals or
        // owned `CString`s that remain alive for the duration of the call, and the
        // sample/buffer descriptors are fully initialized.
        let raw = unsafe {
            (api.pa_simple_new)(
                ptr::null(),
                app_name.as_ptr(),
                PA_STREAM_PLAYBACK,
                ptr::null(),
                stream_name.as_ptr(),
                &sample_spec,
                ptr::null(),
                &buffer_attr,
                &mut error,
            )
        };
        if raw.is_null() {
            return Err(format!(
                "failed to open PulseAudio playback stream: {}",
                pulse_error(api, error)
            ));
        }
        Ok(Self { api, raw })
    }

    #[inline(always)]
    fn write_i16(&self, data: &[i16]) -> Result<(), String> {
        let mut error = 0;
        // SAFETY: `self.raw` is a live PulseAudio stream owned by this
        // connection, and `data` is a valid contiguous i16 slice whose byte length
        // we pass explicitly.
        let rc = unsafe {
            (self.api.pa_simple_write)(
                self.raw,
                data.as_ptr().cast::<c_void>(),
                std::mem::size_of_val(data),
                &mut error,
            )
        };
        if rc < 0 {
            return Err(format!(
                "PulseAudio write failed: {}",
                pulse_error(self.api, error)
            ));
        }
        Ok(())
    }

    #[inline(always)]
    fn drain(&self) {
        let mut error = 0;
        // SAFETY: `self.raw` remains a valid PulseAudio stream until `Drop`, and
        // `error` points to writable stack storage for the call.
        let _ = unsafe { (self.api.pa_simple_drain)(self.raw, &mut error) };
    }

    #[inline(always)]
    fn latency_nanos(&self) -> Result<u64, String> {
        let mut error = 0;
        // SAFETY: `self.raw` remains a valid PulseAudio stream until `Drop`, and
        // `error` points to writable stack storage for the call.
        let latency_usec = unsafe { (self.api.pa_simple_get_latency)(self.raw, &mut error) };
        if latency_usec == u64::MAX {
            return Err(pulse_error(self.api, error));
        }
        Ok(usec_to_nanos(latency_usec))
    }
}

impl Drop for PulseConnection {
    fn drop(&mut self) {
        // SAFETY: `raw` is allocated by `pa_simple_new` and owned uniquely by this
        // connection, so freeing it once in `Drop` is correct.
        unsafe { (self.api.pa_simple_free)(self.raw) };
    }
}

#[derive(Clone, Copy)]
struct PulseTiming {
    playback_host_nanos: u64,
    latency_frames: u32,
    latency_ns: u64,
    timing_quality: OutputTimingQuality,
}

struct PulseClockHealth {
    warned_fallback: bool,
}

impl PulseClockHealth {
    #[inline(always)]
    const fn new() -> Self {
        Self {
            warned_fallback: false,
        }
    }
}

fn render_thread(
    prep: PulseOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_flag: Arc<AtomicBool>,
    ready_tx: Sender<Result<(), String>>,
) {
    if let Err(err) = render_thread_inner(prep, music_ring, sfx_receiver, &stop_flag, &ready_tx) {
        let _ = ready_tx.send(Err(err));
    }
}

fn render_thread_inner(
    prep: PulseOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_flag: &AtomicBool,
    ready_tx: &Sender<Result<(), String>>,
) -> Result<(), String> {
    let stream = PulseConnection::open(&prep)?;
    let period_ns = frames_to_nanos(prep.sample_rate_hz, prep.period_frames);
    let buffer_ns = frames_to_nanos(prep.sample_rate_hz, prep.buffer_frames);
    info!("PulseAudio '{}' using shared output.", prep.device_name);
    publish_output_timing(
        prep.sample_rate_hz,
        period_ns,
        buffer_ns,
        prep.buffer_frames,
        prep.buffer_frames,
        prep.buffer_frames,
        buffer_ns,
    );
    if ready_tx.send(Ok(())).is_err() {
        return Ok(());
    }

    let mut render = RenderState::new(music_ring, sfx_receiver, prep.channels);
    let mut mix = vec![0i16; prep.period_frames as usize * prep.channels];
    let mut clock_health = PulseClockHealth::new();
    while !stop_flag.load(Ordering::Relaxed) {
        let timing_before = playback_timing(&stream, prep.sample_rate_hz, &mut clock_health);
        render.render_i16_host_nanos(&mut mix, timing_before.playback_host_nanos);
        stream.write_i16(&mix)?;
        let timing_after = playback_timing(&stream, prep.sample_rate_hz, &mut clock_health);
        publish_output_timing_quality(worst_quality(
            timing_before.timing_quality,
            timing_after.timing_quality,
        ));
        publish_output_timing(
            prep.sample_rate_hz,
            period_ns,
            buffer_ns,
            prep.buffer_frames,
            timing_after.latency_frames,
            timing_after.latency_frames,
            timing_after.latency_ns,
        );
    }
    stream.drain();
    Ok(())
}

#[inline(always)]
fn playback_timing(
    stream: &PulseConnection,
    sample_rate_hz: u32,
    clock_health: &mut PulseClockHealth,
) -> PulseTiming {
    let now = now_nanos();
    let Ok(latency_ns) = stream.latency_nanos() else {
        note_output_clock_fallback();
        if !clock_health.warned_fallback {
            warn!("PulseAudio latency query failed; falling back to host-time anchors.");
            clock_health.warned_fallback = true;
        }
        return PulseTiming {
            playback_host_nanos: now,
            latency_frames: 0,
            latency_ns: 0,
            timing_quality: OutputTimingQuality::Fallback,
        };
    };
    clock_health.warned_fallback = false;
    PulseTiming {
        playback_host_nanos: now.saturating_add(latency_ns),
        latency_frames: nanos_to_frames(sample_rate_hz, latency_ns),
        latency_ns,
        timing_quality: OutputTimingQuality::Trusted,
    }
}

#[inline(always)]
fn pulse_error(api: &PulseApi, error: c_int) -> String {
    // SAFETY: `pa_strerror` returns either null or a valid NUL-terminated static
    // PulseAudio error string for the duration of the call.
    let ptr = unsafe { (api.pa_strerror)(error) };
    if ptr.is_null() {
        return format!("PulseAudio error {error}");
    }
    // SAFETY: the pointer was checked for null above and PulseAudio documents it
    // as a valid C string.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[inline(always)]
fn frames_to_bytes(frames: u32, channels: usize) -> u32 {
    let bytes = u64::from(frames)
        .saturating_mul(channels.max(1) as u64)
        .saturating_mul(std::mem::size_of::<i16>() as u64);
    bytes.min((u32::MAX - 1) as u64) as u32
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    if sample_rate_hz == 0 || frames == 0 {
        return 0;
    }
    ((u128::from(frames) * 1_000_000_000u128) / u128::from(sample_rate_hz))
        .min((u64::MAX - 1) as u128) as u64
}

#[inline(always)]
fn nanos_to_frames(sample_rate_hz: u32, nanos: u64) -> u32 {
    if sample_rate_hz == 0 || nanos == 0 {
        return 0;
    }
    ((u128::from(sample_rate_hz) * u128::from(nanos)) / 1_000_000_000u128)
        .min((u32::MAX - 1) as u128) as u32
}

#[inline(always)]
fn usec_to_nanos(usec: u64) -> u64 {
    usec.saturating_mul(1_000)
}

#[inline(always)]
const fn worst_quality(a: OutputTimingQuality, b: OutputTimingQuality) -> OutputTimingQuality {
    if (a as u8) >= (b as u8) { a } else { b }
}

#[inline(always)]
fn suggested_period_frames(sample_rate_hz: u32) -> u32 {
    let frames = sample_rate_hz.max(1) / 200;
    frames.clamp(128, PULSE_FALLBACK_BUFFER_FRAMES)
}
