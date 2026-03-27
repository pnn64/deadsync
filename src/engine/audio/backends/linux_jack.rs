use super::super::{
    OutputBackendReady, OutputTelemetryClock, OutputTimingQuality, QueuedSfx, RenderState,
    internal, publish_output_timing, publish_output_timing_quality,
};
use crate::engine::host_time::now_nanos;
use libloading::Library;
use log::{info, warn};
use std::ffi::{c_char, c_int, c_uint, c_void};
use std::ptr;
use std::slice;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::mpsc::Receiver;

const JACK_DEFAULT_AUDIO_TYPE: &[u8] = b"32 bit float mono audio\0";
const JACK_CLIENT_NAME: &[u8] = b"deadsync\0";
const JACK_PROBE_CLIENT_NAME: &[u8] = b"deadsync_probe\0";
const JACK_LEFT_PORT_NAME: &[u8] = b"out_l\0";
const JACK_RIGHT_PORT_NAME: &[u8] = b"out_r\0";
const JACK_NO_START_SERVER: c_int = 1;
const JACK_PORT_IS_INPUT: libc::c_ulong = 1;
const JACK_PORT_IS_OUTPUT: libc::c_ulong = 2;
const JACK_PORT_IS_PHYSICAL: libc::c_ulong = 4;
type JackNFrames = c_uint;
type JackStatus = c_int;

#[repr(C)]
struct JackClientRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct JackPortRaw {
    _private: [u8; 0],
}

// SAFETY: These function-pointer types model JACK's C ABI exactly. Callers must only invoke them
// with live JACK handles, valid callback/user-data pointers, and any required NUL-terminated
// strings exactly as the JACK API specifies.
type JackClientOpenFn = unsafe extern "C" fn(
    client_name: *const c_char,
    options: c_int,
    status: *mut JackStatus,
    ...
) -> *mut JackClientRaw;
// SAFETY: Same FFI contract as above; the passed client handle must come from JACK.
type JackClientCloseFn = unsafe extern "C" fn(client: *mut JackClientRaw) -> c_int;
// SAFETY: Same FFI contract as above; the passed client handle must come from JACK.
type JackActivateFn = unsafe extern "C" fn(client: *mut JackClientRaw) -> c_int;
// SAFETY: Same FFI contract as above; the passed client handle must come from JACK.
type JackDeactivateFn = unsafe extern "C" fn(client: *mut JackClientRaw) -> c_int;
// SAFETY: Same FFI contract as above; the callback and user-data pointer must stay valid for as
// long as JACK may invoke the process callback.
type JackSetProcessCallbackFn = unsafe extern "C" fn(
    client: *mut JackClientRaw,
    process_callback: unsafe extern "C" fn(JackNFrames, *mut c_void) -> c_int,
    arg: *mut c_void,
) -> c_int;
// SAFETY: Same FFI contract as above; the passed client handle must come from JACK.
type JackGetSampleRateFn = unsafe extern "C" fn(client: *mut JackClientRaw) -> JackNFrames;
// SAFETY: Same FFI contract as above; the passed client handle must come from JACK.
type JackGetBufferSizeFn = unsafe extern "C" fn(client: *mut JackClientRaw) -> JackNFrames;
// SAFETY: Same FFI contract as above; the passed client handle must come from JACK and returned
// pointers are JACK-owned until released with `jack_free`.
type JackGetPortsFn = unsafe extern "C" fn(
    client: *mut JackClientRaw,
    port_name_pattern: *const c_char,
    type_name_pattern: *const c_char,
    flags: libc::c_ulong,
) -> *mut *const c_char;
// SAFETY: Same FFI contract as above; the client and port-name pointers must all be valid JACK
// objects / NUL-terminated strings.
type JackConnectFn = unsafe extern "C" fn(
    client: *mut JackClientRaw,
    source_port: *const c_char,
    destination_port: *const c_char,
) -> c_int;
// SAFETY: Same FFI contract as above; the client handle and string pointers must be valid for the
// duration of the call.
type JackPortRegisterFn = unsafe extern "C" fn(
    client: *mut JackClientRaw,
    port_name: *const c_char,
    port_type: *const c_char,
    flags: libc::c_ulong,
    buffer_size: libc::c_ulong,
) -> *mut JackPortRaw;
// SAFETY: Same FFI contract as above; the client and port handles must come from JACK.
type JackPortUnregisterFn =
    unsafe extern "C" fn(client: *mut JackClientRaw, port: *mut JackPortRaw) -> c_int;
// SAFETY: Same FFI contract as above; the passed port handle must come from JACK.
type JackPortNameFn = unsafe extern "C" fn(port: *const JackPortRaw) -> *const c_char;
// SAFETY: Same FFI contract as above; the returned pointer is only valid for the current JACK
// process callback and for exactly `nframes` samples.
type JackPortGetBufferFn =
    unsafe extern "C" fn(port: *mut JackPortRaw, nframes: JackNFrames) -> *mut c_void;
// SAFETY: Same FFI contract as above; the pointer must be one JACK allocated and expects freed.
type JackFreeFn = unsafe extern "C" fn(ptr: *mut c_void);

struct JackApi {
    _lib: Library,
    jack_client_open: JackClientOpenFn,
    jack_client_close: JackClientCloseFn,
    jack_activate: JackActivateFn,
    jack_deactivate: JackDeactivateFn,
    jack_set_process_callback: JackSetProcessCallbackFn,
    jack_get_sample_rate: JackGetSampleRateFn,
    jack_get_buffer_size: JackGetBufferSizeFn,
    jack_get_ports: JackGetPortsFn,
    jack_connect: JackConnectFn,
    jack_port_register: JackPortRegisterFn,
    jack_port_unregister: JackPortUnregisterFn,
    jack_port_name: JackPortNameFn,
    jack_port_get_buffer: JackPortGetBufferFn,
    jack_free: JackFreeFn,
}

static JACK_API: OnceLock<Result<JackApi, String>> = OnceLock::new();

pub(crate) fn is_available() -> bool {
    let Ok(api) = jack_api() else {
        return false;
    };
    let mut status = 0;
    // SAFETY: the client name is a static NUL-terminated byte string, `status`
    // points to writable stack storage, and the optional server name vararg is a
    // null pointer as required by JACK.
    let client = unsafe {
        (api.jack_client_open)(
            JACK_PROBE_CLIENT_NAME.as_ptr().cast::<c_char>(),
            JACK_NO_START_SERVER,
            &mut status,
            ptr::null::<c_char>(),
        )
    };
    if client.is_null() {
        return false;
    }
    // SAFETY: `client` came from `jack_client_open` above and is still owned by
    // this probe path, so closing it here is the correct cleanup.
    unsafe { (api.jack_client_close)(client) };
    true
}

fn jack_api() -> Result<&'static JackApi, String> {
    match JACK_API.get_or_init(load_jack_api) {
        Ok(api) => Ok(api),
        Err(err) => Err(err.clone()),
    }
}

fn load_jack_api() -> Result<JackApi, String> {
    let lib = load_library(&["libjack.so.0", "libjack.so"])?;
    Ok(JackApi {
        // SAFETY: the loaded shared object stays owned by the `JackApi` struct for
        // at least as long as these copied function pointers are used.
        jack_client_open: unsafe { load_symbol(&lib, b"jack_client_open\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_client_close: unsafe { load_symbol(&lib, b"jack_client_close\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_activate: unsafe { load_symbol(&lib, b"jack_activate\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_deactivate: unsafe { load_symbol(&lib, b"jack_deactivate\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_set_process_callback: unsafe { load_symbol(&lib, b"jack_set_process_callback\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_get_sample_rate: unsafe { load_symbol(&lib, b"jack_get_sample_rate\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_get_buffer_size: unsafe { load_symbol(&lib, b"jack_get_buffer_size\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_get_ports: unsafe { load_symbol(&lib, b"jack_get_ports\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_connect: unsafe { load_symbol(&lib, b"jack_connect\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_port_register: unsafe { load_symbol(&lib, b"jack_port_register\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_port_unregister: unsafe { load_symbol(&lib, b"jack_port_unregister\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_port_name: unsafe { load_symbol(&lib, b"jack_port_name\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_port_get_buffer: unsafe { load_symbol(&lib, b"jack_port_get_buffer\0")? },
        // SAFETY: same lifetime reasoning as above for the symbol resolution.
        jack_free: unsafe { load_symbol(&lib, b"jack_free\0")? },
        _lib: lib,
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

// SAFETY: The caller must choose `T` to match the actual symbol signature exported by `lib`.
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

pub(crate) struct JackOutputPrep {
    client: JackClient,
    device_name: String,
    sample_rate_hz: u32,
    period_frames: u32,
}

impl JackOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: 2,
            device_name: self.device_name.clone(),
            backend_name: "jack-shared",
            requested_output_mode: crate::config::AudioOutputMode::Shared,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::Monotonic,
            timing_quality: OutputTimingQuality::Trusted,
        }
    }
}

pub(crate) struct JackOutputStream {
    client: JackClient,
    callback_state: *mut JackCallbackState,
}

impl Drop for JackOutputStream {
    fn drop(&mut self) {
        self.client.deactivate();
        if !self.callback_state.is_null() {
            // SAFETY: `callback_state` came from `Box::into_raw` in `start()` and
            // ownership returns to Rust exactly once here after JACK is deactivated.
            unsafe { drop(Box::from_raw(self.callback_state)) };
            self.callback_state = ptr::null_mut();
        }
    }
}

struct JackClient {
    api: &'static JackApi,
    raw: *mut JackClientRaw,
    port_l: *mut JackPortRaw,
    port_r: *mut JackPortRaw,
    activated: bool,
}

impl JackClient {
    #[inline(always)]
    fn deactivate(&mut self) {
        if self.activated && !self.raw.is_null() {
            // SAFETY: `raw` is the live JACK client owned by this struct, and
            // `deactivate` is only called while that client is still valid.
            unsafe { (self.api.jack_deactivate)(self.raw) };
            self.activated = false;
        }
    }
}

impl Drop for JackClient {
    fn drop(&mut self) {
        self.deactivate();
        if !self.raw.is_null() {
            // SAFETY: `port_l`, `port_r`, and `raw` are all owned by this client.
            // Each non-null port is unregistered at most once before the client is
            // closed exactly once here.
            unsafe {
                if !self.port_r.is_null() {
                    (self.api.jack_port_unregister)(self.raw, self.port_r);
                }
                if !self.port_l.is_null() {
                    (self.api.jack_port_unregister)(self.raw, self.port_l);
                }
                (self.api.jack_client_close)(self.raw);
            }
        }
    }
}

struct JackCallbackState {
    api: &'static JackApi,
    render: RenderState,
    port_l: *mut JackPortRaw,
    port_r: *mut JackPortRaw,
    sample_rate_hz: u32,
    latency_frames: u32,
    interleaved: Vec<f32>,
}

impl JackCallbackState {
    #[inline(always)]
    fn process(&mut self, nframes: u32) {
        let frames = nframes as usize;
        let samples = frames.saturating_mul(2);
        if self.interleaved.len() != samples {
            self.interleaved.resize(samples, 0.0);
        }
        let anchor_nanos = now_nanos();
        self.render
            .render_f32_host_nanos(&mut self.interleaved, anchor_nanos);
        // SAFETY: JACK owns the port buffers for exactly this callback invocation,
        // and `port_buffer` returns slices over those frame-local buffers.
        let left = unsafe { port_buffer(self.api, self.port_l, nframes) };
        // SAFETY: same callback-local buffer lifetime reasoning as above.
        let right = unsafe { port_buffer(self.api, self.port_r, nframes) };
        for i in 0..frames {
            left[i] = self.interleaved[i * 2];
            right[i] = self.interleaved[i * 2 + 1];
        }
        let period_ns = frames_to_nanos(self.sample_rate_hz, nframes);
        let latency_ns = frames_to_nanos(self.sample_rate_hz, self.latency_frames);
        publish_output_timing(
            self.sample_rate_hz,
            period_ns,
            latency_ns,
            self.latency_frames.max(nframes),
            0,
            self.latency_frames,
            latency_ns,
        );
        publish_output_timing_quality(OutputTimingQuality::Trusted);
    }
}

pub(crate) fn prepare(
    requested_device_name: Option<String>,
    requested_rate_hz: Option<u32>,
) -> Result<JackOutputPrep, String> {
    if let Some(name) = &requested_device_name {
        warn!(
            "JACK backend ignores explicit Sound Device selection '{}'; using JACK system playback.",
            name
        );
    }
    let api = jack_api()?;
    let mut status = 0;
    // SAFETY: the client name is a static NUL-terminated byte string, `status`
    // points to writable stack storage, and the optional server name vararg is a
    // null pointer as required by JACK.
    let client = unsafe {
        (api.jack_client_open)(
            JACK_CLIENT_NAME.as_ptr().cast::<c_char>(),
            JACK_NO_START_SERVER,
            &mut status,
            ptr::null::<c_char>(),
        )
    };
    if client.is_null() {
        return Err("Couldn't connect to a running JACK server.".to_string());
    }
    // SAFETY: `client` is the live JACK client returned above, and all string
    // arguments are static NUL-terminated byte strings.
    let port_l = unsafe {
        (api.jack_port_register)(
            client,
            JACK_LEFT_PORT_NAME.as_ptr().cast::<c_char>(),
            JACK_DEFAULT_AUDIO_TYPE.as_ptr().cast::<c_char>(),
            JACK_PORT_IS_OUTPUT,
            0,
        )
    };
    if port_l.is_null() {
        // SAFETY: `client` is still uniquely owned by this function on this error
        // path and must be closed here.
        unsafe { (api.jack_client_close)(client) };
        return Err("Couldn't create JACK output port 'out_l'.".to_string());
    }
    // SAFETY: `client` is the live JACK client returned above, and all string
    // arguments are static NUL-terminated byte strings.
    let port_r = unsafe {
        (api.jack_port_register)(
            client,
            JACK_RIGHT_PORT_NAME.as_ptr().cast::<c_char>(),
            JACK_DEFAULT_AUDIO_TYPE.as_ptr().cast::<c_char>(),
            JACK_PORT_IS_OUTPUT,
            0,
        )
    };
    if port_r.is_null() {
        // SAFETY: both `client` and `port_l` are still owned by this function on
        // this error path, so unregistering the first port and closing the client
        // here is correct cleanup.
        unsafe {
            (api.jack_port_unregister)(client, port_l);
            (api.jack_client_close)(client);
        }
        return Err("Couldn't create JACK output port 'out_r'.".to_string());
    }
    // SAFETY: `client` is a live JACK client, and these query functions only read
    // client state and return scalar values.
    let sample_rate_hz = unsafe { (api.jack_get_sample_rate)(client) as u32 }.max(1);
    // SAFETY: same client-state query reasoning as above.
    let period_frames = unsafe { (api.jack_get_buffer_size)(client) as u32 }.max(1);
    if let Some(requested_rate_hz) = requested_rate_hz
        && requested_rate_hz > 0
        && requested_rate_hz != sample_rate_hz
    {
        warn!(
            "JACK server is running at {} Hz; ignoring requested {} Hz sample rate.",
            sample_rate_hz, requested_rate_hz
        );
    }
    Ok(JackOutputPrep {
        client: JackClient {
            api,
            raw: client,
            port_l,
            port_r,
            activated: false,
        },
        device_name: "JACK system playback".to_string(),
        sample_rate_hz,
        period_frames,
    })
}

pub(crate) fn start(
    prep: JackOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<JackOutputStream, String> {
    let JackOutputPrep {
        mut client,
        device_name,
        sample_rate_hz,
        period_frames,
    } = prep;
    let callback_state = Box::new(JackCallbackState {
        api: client.api,
        render: RenderState::new(music_ring, sfx_receiver, 2),
        port_l: client.port_l,
        port_r: client.port_r,
        sample_rate_hz,
        latency_frames: period_frames,
        interleaved: Vec::new(),
    });
    let callback_state = Box::into_raw(callback_state);
    // SAFETY: `client.raw` is a live JACK client, `jack_process_callback` matches
    // the expected ABI, and `callback_state` is a stable heap pointer kept alive
    // until stream shutdown.
    if unsafe {
        (client.api.jack_set_process_callback)(
            client.raw,
            jack_process_callback,
            callback_state.cast::<c_void>(),
        )
    } != 0
    {
        // SAFETY: callback registration failed, so JACK will never use this raw
        // pointer and ownership returns to Rust immediately.
        unsafe { drop(Box::from_raw(callback_state)) };
        return Err("Couldn't set JACK process callback.".to_string());
    }
    // SAFETY: `client.raw` is a live JACK client with its process callback already
    // installed, so activating it is valid here.
    if unsafe { (client.api.jack_activate)(client.raw) } != 0 {
        // SAFETY: activation failed before steady-state use, so ownership of the
        // callback state pointer returns to Rust immediately.
        unsafe { drop(Box::from_raw(callback_state)) };
        return Err("Couldn't activate JACK client.".to_string());
    }
    client.activated = true;
    if let Err(err) = connect_physical_playback(&client) {
        warn!("JACK backend left ports unconnected: {err}");
    }
    info!(
        "JACK '{}' active at {} Hz, {} frames/cycle.",
        device_name, sample_rate_hz, period_frames
    );
    Ok(JackOutputStream {
        client,
        callback_state,
    })
}

// SAFETY: JACK invokes this callback with the exact user-data pointer registered in
// `jack_set_process_callback`, and the callback must not outlive that allocation.
unsafe extern "C" fn jack_process_callback(nframes: JackNFrames, arg: *mut c_void) -> c_int {
    let state = arg.cast::<JackCallbackState>();
    if state.is_null() {
        return 0;
    }
    // SAFETY: JACK calls this callback with the same `callback_state` pointer
    // registered in `start()`, and that allocation stays alive until stream drop.
    let state = unsafe { &mut *state };
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.process(nframes)))
        // SAFETY: on panic we are still inside JACK's callback and the two port
        // buffers are valid for this invocation, so zeroing them is the least-bad
        // recovery path before returning to JACK.
        .map_err(|_| unsafe {
            zero_port(state.api, state.port_l, nframes);
            zero_port(state.api, state.port_r, nframes);
        });
    0
}

fn connect_physical_playback(client: &JackClient) -> Result<(), String> {
    // SAFETY: `client.raw` is a live JACK client, the type filter string is a
    // static NUL-terminated byte string, and JACK returns a null-terminated array
    // that must later be freed with `jack_free`.
    let ports = unsafe {
        (client.api.jack_get_ports)(
            client.raw,
            ptr::null(),
            JACK_DEFAULT_AUDIO_TYPE.as_ptr().cast::<c_char>(),
            JACK_PORT_IS_PHYSICAL | JACK_PORT_IS_INPUT,
        )
    };
    if ports.is_null() {
        return Err("Couldn't enumerate JACK playback ports.".to_string());
    }
    // SAFETY: `ports` points to a JACK-owned null-terminated array of C strings as
    // returned above, so reading the first element is valid before we free it.
    let port_out_l = unsafe { *ports };
    if port_out_l.is_null() {
        // SAFETY: `ports` came from `jack_get_ports` above and must be freed with
        // `jack_free` on every exit path after allocation succeeds.
        unsafe { (client.api.jack_free)(ports.cast::<c_void>()) };
        return Err("No physical JACK playback ports were found.".to_string());
    }
    // SAFETY: reading the second pointer in the null-terminated list is valid; if
    // it is null we intentionally mirror the left port for mono sinks.
    let port_out_r = unsafe {
        let second = *ports.add(1);
        if second.is_null() { port_out_l } else { second }
    };
    // SAFETY: `client.raw` is live, `jack_port_name` returns JACK-owned C strings
    // for our registered ports, and `port_out_l` comes from the live ports list.
    let left_connect = unsafe {
        (client.api.jack_connect)(
            client.raw,
            (client.api.jack_port_name)(client.port_l),
            port_out_l,
        )
    };
    // SAFETY: same reasoning as above for the right-channel connection.
    let right_connect = unsafe {
        (client.api.jack_connect)(
            client.raw,
            (client.api.jack_port_name)(client.port_r),
            port_out_r,
        )
    };
    // SAFETY: `ports` came from `jack_get_ports` above and is freed exactly once
    // here after we finish reading from the list.
    unsafe { (client.api.jack_free)(ports.cast::<c_void>()) };
    if left_connect != 0 || right_connect != 0 {
        return Err("Couldn't autoconnect JACK output ports to physical sinks.".to_string());
    }
    Ok(())
}

#[inline(always)]
// SAFETY: Callers must only request a port buffer during the active JACK process callback for the
// matching port and frame count returned by JACK.
unsafe fn port_buffer(
    api: &JackApi,
    port: *mut JackPortRaw,
    nframes: JackNFrames,
) -> &'static mut [f32] {
    // SAFETY: JACK guarantees that `jack_port_get_buffer` returns a writable audio
    // buffer for exactly `nframes` samples for the duration of the current process
    // callback. Callers only invoke this inside that callback.
    unsafe {
        slice::from_raw_parts_mut(
            (api.jack_port_get_buffer)(port, nframes).cast::<f32>(),
            nframes as usize,
        )
    }
}

#[inline(always)]
// SAFETY: Callers must only invoke this during the active JACK process callback for the matching
// port and frame count returned by JACK.
unsafe fn zero_port(api: &JackApi, port: *mut JackPortRaw, nframes: JackNFrames) {
    // SAFETY: callers only use this during the JACK process callback when the port
    // buffer for `nframes` frames is valid and writable.
    unsafe { port_buffer(api, port, nframes).fill(0.0) };
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    ((frames as u128) * 1_000_000_000u128 / sample_rate_hz.max(1) as u128) as u64
}
