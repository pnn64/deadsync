#![cfg(target_os = "linux")]

use deadlib_audio_core::{
    AudioOutputMode, CallbackClockSource, CallbackInfo, OutputBackendReady, OutputBufferMut,
    OutputTelemetryClock, OutputTimingQuality, RenderState, SfxReceiver, note_output_underrun,
    publish_output_timing, publish_output_timing_quality, report_audio_render_callback,
};
use deadlib_platform::host_time::now_nanos;
use libloading::Library;
use log::{info, warn};
use std::ffi::{CStr, c_char, c_void};
use std::mem;
use std::ptr::NonNull;
use std::sync::OnceLock;

pub struct PipeWireOutputPrep {
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
}

#[repr(C)]
struct PipeWireApiRaw {
    pw_init: usize,
    pw_main_loop_new: usize,
    pw_main_loop_get_loop: usize,
    pw_main_loop_destroy: usize,
    pw_context_new: usize,
    pw_context_connect: usize,
    pw_context_destroy: usize,
    pw_core_disconnect: usize,
    pw_thread_loop_new: usize,
    pw_thread_loop_get_loop: usize,
    pw_thread_loop_destroy: usize,
    pw_thread_loop_start: usize,
    pw_thread_loop_stop: usize,
    pw_thread_loop_lock: usize,
    pw_thread_loop_unlock: usize,
    pw_thread_loop_timed_wait: usize,
    pw_thread_loop_signal: usize,
    pw_properties_new: usize,
    pw_properties_set: usize,
    pw_stream_new_simple: usize,
    pw_stream_destroy: usize,
    pw_stream_connect: usize,
    pw_stream_dequeue_buffer: usize,
    pw_stream_queue_buffer: usize,
}

struct PipeWireApi {
    raw: PipeWireApiRaw,
    _library: Library,
}

enum PipeWireStreamRaw {}

type RenderCallback = unsafe extern "C" fn(
    data: *mut c_void,
    buffer: *mut u8,
    capacity: u32,
    sample_rate_hz: u32,
    channels: u32,
) -> u32;
type ErrorCallback = unsafe extern "C" fn(data: *mut c_void, error: *const c_char);

unsafe extern "C" {
    fn ds_pipewire_init(api: *const PipeWireApiRaw);
    fn ds_pipewire_probe(api: *const PipeWireApiRaw) -> bool;
    fn ds_pipewire_stream_start(
        api: *const PipeWireApiRaw,
        sample_rate_hz: u32,
        channels: u32,
        render: RenderCallback,
        report_error: ErrorCallback,
        callback_data: *mut c_void,
        error: *mut c_char,
        error_capacity: usize,
    ) -> *mut PipeWireStreamRaw;
    fn ds_pipewire_stream_destroy(stream: *mut PipeWireStreamRaw);
}

static PIPEWIRE_API: OnceLock<Result<PipeWireApi, String>> = OnceLock::new();
static PIPEWIRE_AVAILABLE: OnceLock<bool> = OnceLock::new();

pub fn is_available() -> bool {
    *PIPEWIRE_AVAILABLE.get_or_init(|| {
        let Ok(api) = pipewire_api() else {
            return false;
        };
        // SAFETY: `api.raw` contains addresses resolved from the live PipeWire
        // shared object retained by `api._library`.
        unsafe { ds_pipewire_probe(&api.raw) }
    })
}

fn pipewire_api() -> Result<&'static PipeWireApi, String> {
    match PIPEWIRE_API.get_or_init(load_pipewire_api) {
        Ok(api) => Ok(api),
        Err(err) => Err(err.clone()),
    }
}

fn load_pipewire_api() -> Result<PipeWireApi, String> {
    let library = load_library(&["libpipewire-0.3.so.0", "libpipewire-0.3.so"])?;
    let raw = PipeWireApiRaw {
        // SAFETY: each name is a PipeWire C API function. The C bridge casts
        // these addresses back to the signatures declared by the same headers
        // that were used to compile it.
        pw_init: unsafe { load_symbol(&library, b"pw_init\0")? },
        pw_main_loop_new: unsafe { load_symbol(&library, b"pw_main_loop_new\0")? },
        pw_main_loop_get_loop: unsafe { load_symbol(&library, b"pw_main_loop_get_loop\0")? },
        pw_main_loop_destroy: unsafe { load_symbol(&library, b"pw_main_loop_destroy\0")? },
        pw_context_new: unsafe { load_symbol(&library, b"pw_context_new\0")? },
        pw_context_connect: unsafe { load_symbol(&library, b"pw_context_connect\0")? },
        pw_context_destroy: unsafe { load_symbol(&library, b"pw_context_destroy\0")? },
        pw_core_disconnect: unsafe { load_symbol(&library, b"pw_core_disconnect\0")? },
        pw_thread_loop_new: unsafe { load_symbol(&library, b"pw_thread_loop_new\0")? },
        pw_thread_loop_get_loop: unsafe { load_symbol(&library, b"pw_thread_loop_get_loop\0")? },
        pw_thread_loop_destroy: unsafe { load_symbol(&library, b"pw_thread_loop_destroy\0")? },
        pw_thread_loop_start: unsafe { load_symbol(&library, b"pw_thread_loop_start\0")? },
        pw_thread_loop_stop: unsafe { load_symbol(&library, b"pw_thread_loop_stop\0")? },
        pw_thread_loop_lock: unsafe { load_symbol(&library, b"pw_thread_loop_lock\0")? },
        pw_thread_loop_unlock: unsafe { load_symbol(&library, b"pw_thread_loop_unlock\0")? },
        pw_thread_loop_timed_wait: unsafe {
            load_symbol(&library, b"pw_thread_loop_timed_wait\0")?
        },
        pw_thread_loop_signal: unsafe { load_symbol(&library, b"pw_thread_loop_signal\0")? },
        pw_properties_new: unsafe { load_symbol(&library, b"pw_properties_new\0")? },
        pw_properties_set: unsafe { load_symbol(&library, b"pw_properties_set\0")? },
        pw_stream_new_simple: unsafe { load_symbol(&library, b"pw_stream_new_simple\0")? },
        pw_stream_destroy: unsafe { load_symbol(&library, b"pw_stream_destroy\0")? },
        pw_stream_connect: unsafe { load_symbol(&library, b"pw_stream_connect\0")? },
        pw_stream_dequeue_buffer: unsafe { load_symbol(&library, b"pw_stream_dequeue_buffer\0")? },
        pw_stream_queue_buffer: unsafe { load_symbol(&library, b"pw_stream_queue_buffer\0")? },
    };
    let api = PipeWireApi {
        raw,
        _library: library,
    };
    // SAFETY: all required function addresses were resolved above, and the
    // library is already stored in `api` so it cannot unload after init.
    unsafe { ds_pipewire_init(&api.raw) };
    Ok(api)
}

fn load_library(names: &[&str]) -> Result<Library, String> {
    let mut last_err = None;
    for name in names {
        // SAFETY: the handle is retained in `PipeWireApi` for the process-long
        // lifetime of every copied function address.
        match unsafe { Library::new(*name) } {
            Ok(library) => return Ok(library),
            Err(err) => last_err = Some(format!("{name}: {err}")),
        }
    }
    Err(last_err.unwrap_or_else(|| "no PipeWire library names were provided".to_string()))
}

// SAFETY: `name` must identify a function in `library`. The returned address
// may only be called with that function's actual C signature while the library
// remains loaded.
unsafe fn load_symbol(library: &Library, name: &[u8]) -> Result<usize, String> {
    // SAFETY: the caller supplies a PipeWire function name, and this generic
    // no-argument type is used only to copy its address, never to call it.
    unsafe { library.get::<unsafe extern "C" fn()>(name) }
        .map(|symbol| *symbol as usize)
        .map_err(|err| {
            format!(
                "{}: {err}",
                String::from_utf8_lossy(name).trim_end_matches('\0')
            )
        })
}

impl PipeWireOutputPrep {
    pub fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: "pipewire-shared",
            requested_output_mode: AudioOutputMode::Shared,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::Monotonic,
            timing_quality: OutputTimingQuality::Trusted,
        }
    }
}

pub struct PipeWireOutputStream {
    raw: NonNull<PipeWireStreamRaw>,
    callback_state: *mut CallbackState,
}

impl Drop for PipeWireOutputStream {
    fn drop(&mut self) {
        // SAFETY: `raw` is owned by this stream and destruction stops all C
        // callbacks before the Rust callback state is reclaimed.
        unsafe { ds_pipewire_stream_destroy(self.raw.as_ptr()) };
        if !self.callback_state.is_null() {
            // SAFETY: this pointer came from `Box::into_raw` in `start` and the
            // C bridge no longer retains it after stream destruction.
            unsafe { drop(Box::from_raw(self.callback_state)) };
            self.callback_state = std::ptr::null_mut();
        }
    }
}

struct CallbackState {
    render: RenderState,
    sfx_receiver: SfxReceiver,
}

impl CallbackState {
    fn render_into(&mut self, data: &mut [u8], sample_rate_hz: u32, channels: usize) -> usize {
        let sample_rate_hz = sample_rate_hz.max(1);
        let channels = channels.max(1);
        let stride = channels.saturating_mul(mem::size_of::<f32>());
        if stride == 0 {
            return 0;
        }
        let frames = data.len() / stride;
        let samples = frames.saturating_mul(channels);
        let bytes = samples.saturating_mul(mem::size_of::<f32>());
        if data.as_ptr().align_offset(mem::align_of::<f32>()) != 0 {
            data[..bytes].fill(0);
            note_output_underrun(now_nanos(), log::log_enabled!(log::Level::Trace));
            return bytes;
        }
        // SAFETY: alignment was checked above, `samples` was derived from the
        // available byte length, and PipeWire exclusively lends this mapped
        // buffer for the duration of the process callback.
        let output =
            unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr().cast::<f32>(), samples) };
        let anchor_nanos = now_nanos();
        let result = self.render.render(
            OutputBufferMut::F32(output),
            CallbackInfo {
                anchor_nanos,
                clock: CallbackClockSource::Instant,
            },
            self.sfx_receiver.try_iter(),
        );
        report_audio_render_callback(result, log::log_enabled!(log::Level::Trace), now_nanos);
        let period_ns = frames_to_nanos(sample_rate_hz, frames as u32);
        publish_output_timing(
            sample_rate_hz,
            period_ns,
            period_ns,
            frames as u32,
            0,
            frames as u32,
            period_ns,
        );
        publish_output_timing_quality(OutputTimingQuality::Trusted);
        bytes
    }
}

unsafe extern "C" fn render_callback(
    data: *mut c_void,
    buffer: *mut u8,
    capacity: u32,
    sample_rate_hz: u32,
    channels: u32,
) -> u32 {
    if data.is_null() || buffer.is_null() {
        return 0;
    }
    // SAFETY: the bridge receives this pointer from `Box::into_raw` and only
    // invokes callbacks while the box remains owned by the output stream.
    let state = unsafe { &mut *data.cast::<CallbackState>() };
    // SAFETY: PipeWire provides a writable mapped buffer of exactly `capacity`
    // bytes for this process callback.
    let output = unsafe { std::slice::from_raw_parts_mut(buffer, capacity as usize) };
    state
        .render_into(output, sample_rate_hz, channels as usize)
        .min(u32::MAX as usize) as u32
}

unsafe extern "C" fn error_callback(_data: *mut c_void, error: *const c_char) {
    if error.is_null() {
        warn!("PipeWire stream entered an unknown error state.");
        return;
    }
    // SAFETY: the bridge passes either PipeWire's callback-local error string or
    // its own NUL-terminated error buffer for the duration of this call.
    let error = unsafe { CStr::from_ptr(error) }.to_string_lossy();
    warn!("PipeWire stream error: {error}");
}

pub fn prepare(
    requested_device_name: Option<String>,
    sample_rate_hz: u32,
    channels: usize,
) -> Result<PipeWireOutputPrep, String> {
    pipewire_api().map_err(|err| format!("PipeWire backend unavailable: {err}"))?;
    let device_name = match requested_device_name {
        Some(name) if !name.is_empty() => {
            format!("PipeWire default sink (requested '{name}' unsupported)")
        }
        _ => "PipeWire default sink".to_string(),
    };
    Ok(PipeWireOutputPrep {
        device_name,
        sample_rate_hz: sample_rate_hz.max(1),
        channels: channels.clamp(1, 32),
    })
}

pub fn start(
    prep: PipeWireOutputPrep,
    render: RenderState,
    sfx_receiver: SfxReceiver,
) -> Result<PipeWireOutputStream, String> {
    let api = pipewire_api().map_err(|err| format!("PipeWire backend unavailable: {err}"))?;
    let callback_state = Box::into_raw(Box::new(CallbackState {
        render,
        sfx_receiver,
    }));
    let mut error = [0 as c_char; 512];
    // SAFETY: the API table points into the live library, the callback state is
    // heap-stable, and both callbacks obey the bridge's C ABI contracts.
    let raw = unsafe {
        ds_pipewire_stream_start(
            &api.raw,
            prep.sample_rate_hz,
            prep.channels as u32,
            render_callback,
            error_callback,
            callback_state.cast::<c_void>(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    let Some(raw) = NonNull::new(raw) else {
        // SAFETY: stream creation failed, so the bridge cannot retain or call
        // this callback state after returning.
        unsafe { drop(Box::from_raw(callback_state)) };
        // SAFETY: the bridge always leaves this fixed-size buffer NUL-terminated.
        let message = unsafe { CStr::from_ptr(error.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        return Err(if message.is_empty() {
            "failed to start PipeWire stream".to_string()
        } else {
            message
        });
    };

    info!(
        "PipeWire '{}' using {} Hz, {} ch shared output.",
        prep.device_name, prep.sample_rate_hz, prep.channels
    );
    publish_output_timing(prep.sample_rate_hz, 0, 0, 0, 0, 0, 0);
    publish_output_timing_quality(OutputTimingQuality::Trusted);
    Ok(PipeWireOutputStream {
        raw,
        callback_state,
    })
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    (u64::from(frames) * 1_000_000_000) / u64::from(sample_rate_hz.max(1))
}

#[cfg(test)]
mod tests {
    use super::is_available;

    #[test]
    fn availability_probe_is_stable_without_reconnecting() {
        let available = is_available();
        assert_eq!(is_available(), available);
    }
}
