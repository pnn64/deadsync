mod backends;
pub(crate) mod decode;
mod resample;

use crate::config::dirs;
use crate::engine::host_time::instant_nanos;
#[cfg(windows)]
use crate::engine::windows_rt::current_qpc_nanos;
use log::{debug, info, warn};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

/* ============================== Public API ============================== */

#[derive(Clone, Copy, Debug)]
pub struct Cut {
    pub start_sec: f64,
    pub length_sec: f64,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
}
impl Default for Cut {
    fn default() -> Self {
        Self {
            start_sec: 0.0,
            length_sec: f64::INFINITY,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rates_hz: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitConfig {
    pub output_device_index: Option<u16>,
    pub output_mode: crate::config::AudioOutputMode,
    #[cfg(target_os = "linux")]
    pub linux_backend: crate::config::LinuxAudioBackend,
    pub sample_rate_hz: Option<u32>,
}

struct OutputDeviceProbe {
    info: OutputDeviceInfo,
    #[cfg(target_os = "freebsd")]
    freebsd_dsp_path: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum SfxLane {
    Effect,
    AssistTick,
}

#[derive(Clone)]
struct QueuedSfx {
    data: Arc<Vec<i16>>,
    lane: SfxLane,
}

// Commands to the audio engine
enum AudioCommand {
    PlaySfx(QueuedSfx),
    // Path, cut, looping, rate (1.0 = normal)
    PlayMusic(PathBuf, Cut, bool, f32),
    StopMusic,
    // Change rate of currently playing music without restarting
    SetMusicRate(f32),
}

// Global engine (initialized once)
static ENGINE_INIT_CFG: OnceLock<InitConfig> = OnceLock::new();
static ENGINE: std::sync::LazyLock<AudioEngine> =
    std::sync::LazyLock::new(|| init_engine_and_thread(engine_init_cfg()));

struct AudioEngine {
    command_sender: Sender<AudioCommand>,
    sfx_cache: Mutex<HashMap<String, Arc<Vec<i16>>>>,
    device_sample_rate: u32,
    device_channels: usize,
    startup_output_devices: Vec<OutputDeviceInfo>,
}

#[cfg(windows)]
#[derive(Clone)]
struct WasapiBackendHint {
    device_id: Option<String>,
    device_name: String,
    requested_rate_hz: Option<u32>,
    output_mode: crate::config::AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct AlsaBackendHint {
    pcm_id: Option<String>,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: crate::config::AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_jack_audio)]
#[derive(Clone)]
struct JackBackendHint {
    requested_device_name: Option<String>,
    requested_rate_hz: Option<u32>,
    output_mode: crate::config::AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_pipewire_audio)]
#[derive(Clone)]
struct PipeWireBackendHint {
    requested_device_name: Option<String>,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: crate::config::AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_pulse_audio)]
#[derive(Clone)]
struct PulseBackendHint {
    requested_device_name: Option<String>,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: crate::config::AudioOutputMode,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct CoreAudioBackendHint {
    device_uid: Option<String>,
    device_name: String,
    requested_rate_hz: Option<u32>,
    channels: usize,
    output_mode: crate::config::AudioOutputMode,
}

#[cfg(target_os = "freebsd")]
#[derive(Clone)]
struct FreeBsdPcmBackendHint {
    dsp_path: Option<String>,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: crate::config::AudioOutputMode,
}

#[derive(Clone)]
struct AudioThreadLaunch {
    #[cfg(target_os = "linux")]
    explicit_device_requested: bool,
    #[cfg(target_os = "linux")]
    linux_backend: crate::config::LinuxAudioBackend,
    #[cfg(target_os = "linux")]
    alsa: Option<AlsaBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(has_jack_audio)]
    jack: Option<JackBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(has_pipewire_audio)]
    pipewire: Option<PipeWireBackendHint>,
    #[cfg(target_os = "linux")]
    #[cfg(has_pulse_audio)]
    pulse: Option<PulseBackendHint>,
    #[cfg(target_os = "macos")]
    coreaudio: Option<CoreAudioBackendHint>,
    #[cfg(target_os = "freebsd")]
    freebsd_pcm: Option<FreeBsdPcmBackendHint>,
    #[cfg(windows)]
    wasapi: Option<WasapiBackendHint>,
}

#[derive(Clone, Debug)]
struct OutputBackendReady {
    device_sample_rate: u32,
    device_channels: usize,
    device_name: String,
    backend_name: &'static str,
    requested_output_mode: crate::config::AudioOutputMode,
    fallback_from_native: bool,
    timing_clock: OutputTelemetryClock,
    timing_quality: OutputTimingQuality,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputTelemetryBackend {
    Unknown = 0,
    #[cfg(target_os = "linux")]
    AlsaShared = 1,
    #[cfg(target_os = "linux")]
    AlsaExclusive = 2,
    #[cfg(windows)]
    WasapiShared = 1,
    #[cfg(windows)]
    WasapiExclusive = 2,
    #[cfg(target_os = "linux")]
    #[cfg(has_pulse_audio)]
    PulseAudioShared = 3,
    #[cfg(target_os = "freebsd")]
    FreeBsdPcm = 1,
    #[cfg(target_os = "linux")]
    #[cfg(has_jack_audio)]
    JackShared = 4,
    #[cfg(target_os = "macos")]
    CoreAudioShared = 1,
    #[cfg(target_os = "linux")]
    #[cfg(has_pipewire_audio)]
    PipeWireShared = 5,
}

impl OutputTelemetryBackend {
    #[inline(always)]
    fn from_backend_name(name: &'static str) -> Self {
        match name {
            #[cfg(target_os = "linux")]
            "alsa-shared" => Self::AlsaShared,
            #[cfg(target_os = "linux")]
            "alsa-exclusive" => Self::AlsaExclusive,
            #[cfg(windows)]
            "wasapi-shared" => Self::WasapiShared,
            #[cfg(windows)]
            "wasapi-exclusive" => Self::WasapiExclusive,
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            "pulse-shared" => Self::PulseAudioShared,
            #[cfg(target_os = "freebsd")]
            "freebsd-pcm" => Self::FreeBsdPcm,
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            "jack-shared" => Self::JackShared,
            #[cfg(target_os = "macos")]
            "coreaudio-shared" => Self::CoreAudioShared,
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            "pipewire-shared" => Self::PipeWireShared,
            _ => Self::Unknown,
        }
    }

    #[inline(always)]
    fn load() -> Self {
        match OUTPUT_TIMING_BACKEND.load(Ordering::Relaxed) {
            #[cfg(target_os = "linux")]
            1 => Self::AlsaShared,
            #[cfg(target_os = "linux")]
            2 => Self::AlsaExclusive,
            #[cfg(windows)]
            1 => Self::WasapiShared,
            #[cfg(windows)]
            2 => Self::WasapiExclusive,
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            3 => Self::PulseAudioShared,
            #[cfg(target_os = "freebsd")]
            1 => Self::FreeBsdPcm,
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            4 => Self::JackShared,
            #[cfg(target_os = "macos")]
            1 => Self::CoreAudioShared,
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            5 => Self::PipeWireShared,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for OutputTelemetryBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Unknown => "unknown",
            #[cfg(target_os = "linux")]
            Self::AlsaShared => "alsa-shared",
            #[cfg(target_os = "linux")]
            Self::AlsaExclusive => "alsa-exclusive",
            #[cfg(windows)]
            Self::WasapiShared => "wasapi-shared",
            #[cfg(windows)]
            Self::WasapiExclusive => "wasapi-exclusive",
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            Self::PulseAudioShared => "pulse-shared",
            #[cfg(target_os = "freebsd")]
            Self::FreeBsdPcm => "freebsd-pcm",
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            Self::JackShared => "jack-shared",
            #[cfg(target_os = "macos")]
            Self::CoreAudioShared => "coreaudio-shared",
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            Self::PipeWireShared => "pipewire-shared",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputTelemetryClock {
    Unknown = 0,
    Callback = 1,
    #[cfg(target_os = "macos")]
    HostTime = 2,
    #[cfg(all(unix, not(target_os = "macos")))]
    Monotonic = 2,
    #[cfg(all(unix, not(target_os = "macos")))]
    MonotonicRaw = 3,
    #[cfg(windows)]
    DeviceQpc = 4,
}

impl OutputTelemetryClock {
    #[inline(always)]
    fn load() -> Self {
        match OUTPUT_TIMING_CLOCK.load(Ordering::Relaxed) {
            1 => Self::Callback,
            #[cfg(target_os = "macos")]
            2 => Self::HostTime,
            #[cfg(all(unix, not(target_os = "macos")))]
            2 => Self::Monotonic,
            #[cfg(all(unix, not(target_os = "macos")))]
            3 => Self::MonotonicRaw,
            #[cfg(windows)]
            4 => Self::DeviceQpc,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for OutputTelemetryClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Unknown => "unknown",
            Self::Callback => "callback",
            #[cfg(target_os = "macos")]
            Self::HostTime => "host_time",
            #[cfg(all(unix, not(target_os = "macos")))]
            Self::Monotonic => "monotonic",
            #[cfg(all(unix, not(target_os = "macos")))]
            Self::MonotonicRaw => "monotonic_raw",
            #[cfg(windows)]
            Self::DeviceQpc => "device+qpc",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputTimingQuality {
    Unknown = 0,
    Trusted = 1,
    Degraded = 2,
    Fallback = 3,
}

impl OutputTimingQuality {
    #[inline(always)]
    fn load() -> Self {
        match OUTPUT_TIMING_QUALITY.load(Ordering::Relaxed) {
            1 => Self::Trusted,
            2 => Self::Degraded,
            3 => Self::Fallback,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for OutputTimingQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Unknown => "unknown",
            Self::Trusted => "trusted",
            Self::Degraded => "degraded",
            Self::Fallback => "fallback",
        };
        f.write_str(label)
    }
}

#[inline(always)]
const fn output_mode_bits(mode: crate::config::AudioOutputMode) -> u8 {
    match mode {
        crate::config::AudioOutputMode::Auto => 1,
        crate::config::AudioOutputMode::Shared => 2,
        crate::config::AudioOutputMode::Exclusive => 3,
    }
}

#[inline(always)]
const fn output_mode_from_bits(bits: u8) -> crate::config::AudioOutputMode {
    match bits {
        2 => crate::config::AudioOutputMode::Shared,
        3 => crate::config::AudioOutputMode::Exclusive,
        _ => crate::config::AudioOutputMode::Auto,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OutputTimingSnapshot {
    pub backend: OutputTelemetryBackend,
    pub requested_output_mode: crate::config::AudioOutputMode,
    pub fallback_from_native: bool,
    pub timing_clock: OutputTelemetryClock,
    pub timing_quality: OutputTimingQuality,
    pub sample_rate_hz: u32,
    pub device_period_ns: u64,
    pub stream_latency_ns: u64,
    pub buffer_frames: u32,
    pub padding_frames: u32,
    pub queued_frames: u32,
    pub estimated_output_delay_ns: u64,
    pub clock_fallback_count: u64,
    pub timing_sanity_failure_count: u64,
    pub underrun_count: u64,
}

impl OutputTimingSnapshot {
    #[inline(always)]
    pub const fn has_measurement(self) -> bool {
        !matches!(self.backend, OutputTelemetryBackend::Unknown)
            || self.device_period_ns != 0
            || self.stream_latency_ns != 0
            || self.buffer_frames != 0
            || self.padding_frames != 0
            || self.queued_frames != 0
            || self.estimated_output_delay_ns != 0
            || self.clock_fallback_count != 0
            || self.timing_sanity_failure_count != 0
            || self.underrun_count != 0
    }
}

const AUDIO_STUTTER_DIAG_EVENT_COUNT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StutterDiagAudioEventKind {
    Underrun = 1,
    CallbackGap = 2,
    TimingSanity = 3,
    ClockFallback = 4,
}

impl StutterDiagAudioEventKind {
    #[inline(always)]
    fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            1 => Some(Self::Underrun),
            2 => Some(Self::CallbackGap),
            3 => Some(Self::TimingSanity),
            4 => Some(Self::ClockFallback),
            _ => None,
        }
    }
}

impl std::fmt::Display for StutterDiagAudioEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Underrun => "underrun",
            Self::CallbackGap => "callback_gap",
            Self::TimingSanity => "timing_sanity",
            Self::ClockFallback => "clock_fallback",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StutterDiagAudioEvent {
    pub at_host_nanos: u64,
    pub kind: StutterDiagAudioEventKind,
    pub value_ns: u64,
    pub sample_rate_hz: u32,
    pub buffer_frames: u32,
    pub padding_frames: u32,
    pub queued_frames: u32,
    pub device_period_ns: u64,
    pub estimated_output_delay_ns: u64,
    pub timing_quality: OutputTimingQuality,
}

struct AudioDiagEventSlot {
    version: AtomicU64,
    at_host_nanos: AtomicU64,
    kind: AtomicU8,
    value_ns: AtomicU64,
    sample_rate_hz: AtomicU32,
    buffer_frames: AtomicU32,
    padding_frames: AtomicU32,
    queued_frames: AtomicU32,
    device_period_ns: AtomicU64,
    estimated_output_delay_ns: AtomicU64,
    timing_quality: AtomicU8,
}

impl AudioDiagEventSlot {
    #[inline(always)]
    fn new() -> Self {
        Self {
            version: AtomicU64::new(0),
            at_host_nanos: AtomicU64::new(0),
            kind: AtomicU8::new(0),
            value_ns: AtomicU64::new(0),
            sample_rate_hz: AtomicU32::new(0),
            buffer_frames: AtomicU32::new(0),
            padding_frames: AtomicU32::new(0),
            queued_frames: AtomicU32::new(0),
            device_period_ns: AtomicU64::new(0),
            estimated_output_delay_ns: AtomicU64::new(0),
            timing_quality: AtomicU8::new(OutputTimingQuality::Unknown as u8),
        }
    }

    fn load(&self) -> Option<(u64, StutterDiagAudioEvent)> {
        let version_start = self.version.load(Ordering::Acquire);
        if version_start == 0 || version_start & 1 != 0 {
            return None;
        }
        let event = StutterDiagAudioEvent {
            at_host_nanos: self.at_host_nanos.load(Ordering::Relaxed),
            kind: StutterDiagAudioEventKind::from_bits(self.kind.load(Ordering::Relaxed))?,
            value_ns: self.value_ns.load(Ordering::Relaxed),
            sample_rate_hz: self.sample_rate_hz.load(Ordering::Relaxed),
            buffer_frames: self.buffer_frames.load(Ordering::Relaxed),
            padding_frames: self.padding_frames.load(Ordering::Relaxed),
            queued_frames: self.queued_frames.load(Ordering::Relaxed),
            device_period_ns: self.device_period_ns.load(Ordering::Relaxed),
            estimated_output_delay_ns: self.estimated_output_delay_ns.load(Ordering::Relaxed),
            timing_quality: match self.timing_quality.load(Ordering::Relaxed) {
                1 => OutputTimingQuality::Trusted,
                2 => OutputTimingQuality::Degraded,
                3 => OutputTimingQuality::Fallback,
                _ => OutputTimingQuality::Unknown,
            },
        };
        let version_end = self.version.load(Ordering::Acquire);
        (version_start == version_end).then_some((version_end >> 1, event))
    }
}

/// A handle to a streaming music track.
struct MusicStream {
    thread: thread::JoinHandle<()>,
    stop_signal: Arc<AtomicBool>,
    rate_bits: Arc<AtomicU32>,
}

// Global playback position tracking for the current music stream.
// All counters are in *frames* at the device sample rate (not interleaved samples).
static MUSIC_TOTAL_FRAMES: AtomicU64 = AtomicU64::new(0);
static MUSIC_TRACK_START_FRAME: AtomicU64 = AtomicU64::new(0);
static MUSIC_TRACK_HAS_STARTED: AtomicBool = AtomicBool::new(false);
static MUSIC_TRACK_ACTIVE: AtomicBool = AtomicBool::new(false);
static MUSIC_MAP_GEN: AtomicU64 = AtomicU64::new(1);

// Last audio callback timing, used to interpolate the playback position
// between callback invocations so that the reported stream time is
// continuous instead of jumping in whole buffer increments.
static CALLBACK_CLOCK_SEQ: AtomicU64 = AtomicU64::new(0);
static CALLBACK_CLOCK_SOURCE: AtomicU8 = AtomicU8::new(CallbackClockSource::Instant as u8);
// Stored as elapsed nanos + 1 from the shared process host-clock epoch; 0 means "no callback yet".
static LAST_CALLBACK_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
static LAST_CALLBACK_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static LAST_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);
static AUDIO_TIMING_DIAG_ENABLED: OnceLock<bool> = OnceLock::new();
static AUDIO_TIMING_DIAG_LAST_SOURCE: AtomicU8 = AtomicU8::new(0);
static AUDIO_TIMING_DIAG_LAST_NANOS: AtomicU64 = AtomicU64::new(0);
static AUDIO_TIMING_DIAG_LAST_GAP_NS: AtomicU64 = AtomicU64::new(0);
static AUDIO_STUTTER_DIAG_EVENT_HEAD: AtomicU64 = AtomicU64::new(0);
static AUDIO_STUTTER_DIAG_EVENTS: std::sync::LazyLock<
    [AudioDiagEventSlot; AUDIO_STUTTER_DIAG_EVENT_COUNT],
> = std::sync::LazyLock::new(|| std::array::from_fn(|_| AudioDiagEventSlot::new()));
static OUTPUT_TIMING_BACKEND: AtomicU8 = AtomicU8::new(OutputTelemetryBackend::Unknown as u8);
static OUTPUT_TIMING_REQUESTED_MODE: AtomicU8 = AtomicU8::new(1);
static OUTPUT_TIMING_NATIVE_FALLBACK: AtomicBool = AtomicBool::new(false);
static OUTPUT_TIMING_CLOCK: AtomicU8 = AtomicU8::new(OutputTelemetryClock::Unknown as u8);
static OUTPUT_TIMING_QUALITY: AtomicU8 = AtomicU8::new(OutputTimingQuality::Unknown as u8);
static OUTPUT_TIMING_SAMPLE_RATE_HZ: AtomicU32 = AtomicU32::new(0);
static OUTPUT_TIMING_DEVICE_PERIOD_NS: AtomicU64 = AtomicU64::new(0);
static OUTPUT_TIMING_STREAM_LATENCY_NS: AtomicU64 = AtomicU64::new(0);
static OUTPUT_TIMING_BUFFER_FRAMES: AtomicU32 = AtomicU32::new(0);
static OUTPUT_TIMING_PADDING_FRAMES: AtomicU32 = AtomicU32::new(0);
static OUTPUT_TIMING_QUEUED_FRAMES: AtomicU32 = AtomicU32::new(0);
static OUTPUT_TIMING_EST_DELAY_NS: AtomicU64 = AtomicU64::new(0);
static OUTPUT_TIMING_CLOCK_FALLBACKS: AtomicU64 = AtomicU64::new(0);
static OUTPUT_TIMING_SANITY_FAILURES: AtomicU64 = AtomicU64::new(0);
static OUTPUT_TIMING_UNDERRUNS: AtomicU64 = AtomicU64::new(0);

const MUSIC_POS_MAP_BACKLOG_FRAMES: i64 = 80_000;
const NANOS_PER_SECOND: f64 = 1_000_000_000.0;

#[inline(always)]
fn music_nanos_from_seconds(seconds: f64) -> i64 {
    if !seconds.is_finite() {
        return 0;
    }
    let nanos = (seconds * NANOS_PER_SECOND).round();
    nanos.clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

#[derive(Clone, Copy, Debug)]
pub struct MusicStreamClockSnapshot {
    pub stream_seconds: f32,
    pub music_seconds: f32,
    pub music_nanos: i64,
    pub music_seconds_per_second: f32,
    pub has_music_mapping: bool,
    pub valid_at: Instant,
    // Host/QPC clock for `valid_at` when the backend publishes one; 0 means
    // the snapshot only has a local `Instant` anchor.
    pub valid_at_host_nanos: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum CallbackClockSource {
    Instant = 1,
    #[cfg(windows)]
    Qpc = 2,
}

impl CallbackClockSource {
    #[inline(always)]
    fn load() -> Self {
        match CALLBACK_CLOCK_SOURCE.load(Ordering::Relaxed) {
            #[cfg(windows)]
            2 => Self::Qpc,
            _ => Self::Instant,
        }
    }
}

#[inline(always)]
pub(crate) fn timing_diag_enabled() -> bool {
    *AUDIO_TIMING_DIAG_ENABLED.get_or_init(|| {
        let Ok(value) = std::env::var("DEADSYNC_AUDIO_TIMING_DIAG") else {
            return false;
        };
        let value = value.trim();
        !(value.is_empty()
            || value == "0"
            || value.eq_ignore_ascii_case("false")
            || value.eq_ignore_ascii_case("off")
            || value.eq_ignore_ascii_case("no"))
    })
}

#[inline(always)]
pub(crate) fn timing_diag_last_callback_gap_ns() -> u64 {
    AUDIO_TIMING_DIAG_LAST_GAP_NS.load(Ordering::Relaxed)
}

#[inline(always)]
fn stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

#[inline(always)]
fn stutter_diag_callback_gap_threshold_ns() -> u64 {
    let device_period_ns = OUTPUT_TIMING_DEVICE_PERIOD_NS.load(Ordering::Relaxed);
    if device_period_ns > 0 {
        return device_period_ns.saturating_mul(2).max(5_000_000);
    }
    let sample_rate_hz = OUTPUT_TIMING_SAMPLE_RATE_HZ.load(Ordering::Relaxed);
    let buffer_frames = OUTPUT_TIMING_BUFFER_FRAMES.load(Ordering::Relaxed);
    if sample_rate_hz > 0 && buffer_frames > 0 {
        let buffer_ns =
            (u64::from(buffer_frames) * 1_000_000_000).saturating_div(u64::from(sample_rate_hz));
        return buffer_ns.saturating_mul(2).max(5_000_000);
    }
    10_000_000
}

fn record_stutter_diag_event(
    kind: StutterDiagAudioEventKind,
    at_host_nanos: u64,
    value_ns: u64,
    timing_quality: OutputTimingQuality,
) {
    if !stutter_diag_enabled() {
        return;
    }
    let seq = AUDIO_STUTTER_DIAG_EVENT_HEAD.fetch_add(1, Ordering::Relaxed) + 1;
    let slot = &AUDIO_STUTTER_DIAG_EVENTS[(seq as usize - 1) % AUDIO_STUTTER_DIAG_EVENT_COUNT];
    slot.version.store((seq << 1) | 1, Ordering::Relaxed);
    slot.at_host_nanos.store(at_host_nanos, Ordering::Relaxed);
    slot.kind.store(kind as u8, Ordering::Relaxed);
    slot.value_ns.store(value_ns, Ordering::Relaxed);
    slot.sample_rate_hz.store(
        OUTPUT_TIMING_SAMPLE_RATE_HZ.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    slot.buffer_frames.store(
        OUTPUT_TIMING_BUFFER_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    slot.padding_frames.store(
        OUTPUT_TIMING_PADDING_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    slot.queued_frames.store(
        OUTPUT_TIMING_QUEUED_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    slot.device_period_ns.store(
        OUTPUT_TIMING_DEVICE_PERIOD_NS.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    slot.estimated_output_delay_ns.store(
        OUTPUT_TIMING_EST_DELAY_NS.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    slot.timing_quality
        .store(timing_quality as u8, Ordering::Relaxed);
    slot.version.store(seq << 1, Ordering::Release);
}

pub fn stutter_diag_trigger_seq() -> u64 {
    AUDIO_STUTTER_DIAG_EVENT_HEAD.load(Ordering::Acquire)
}

pub fn collect_stutter_diag_events(
    now_host_nanos: u64,
    window_ns: u64,
    out: &mut Vec<StutterDiagAudioEvent>,
) {
    let head = AUDIO_STUTTER_DIAG_EVENT_HEAD.load(Ordering::Acquire);
    let start = head.saturating_sub(AUDIO_STUTTER_DIAG_EVENT_COUNT as u64);
    for seq in (start + 1)..=head {
        let slot = &AUDIO_STUTTER_DIAG_EVENTS[(seq as usize - 1) % AUDIO_STUTTER_DIAG_EVENT_COUNT];
        let Some((loaded_seq, event)) = slot.load() else {
            continue;
        };
        if loaded_seq != seq || event.at_host_nanos == 0 {
            continue;
        }
        if now_host_nanos.saturating_sub(event.at_host_nanos) <= window_ns {
            out.push(event);
        }
    }
}

#[inline(always)]
fn note_timing_diag_callback_gap(anchor_nanos: u64, source: CallbackClockSource) {
    let timing_diag = timing_diag_enabled();
    let stutter_diag = stutter_diag_enabled();
    if anchor_nanos == 0 || (!timing_diag && !stutter_diag) {
        return;
    }
    let source_id = source as u8;
    let prev_source = AUDIO_TIMING_DIAG_LAST_SOURCE.swap(source_id, Ordering::Relaxed);
    let prev_nanos = if prev_source == source_id {
        AUDIO_TIMING_DIAG_LAST_NANOS.swap(anchor_nanos, Ordering::Relaxed)
    } else {
        AUDIO_TIMING_DIAG_LAST_NANOS.store(anchor_nanos, Ordering::Relaxed);
        0
    };
    if prev_nanos != 0 && anchor_nanos >= prev_nanos {
        let gap_ns = anchor_nanos - prev_nanos;
        AUDIO_TIMING_DIAG_LAST_GAP_NS.store(gap_ns, Ordering::Relaxed);
        if stutter_diag && gap_ns >= stutter_diag_callback_gap_threshold_ns() {
            record_stutter_diag_event(
                StutterDiagAudioEventKind::CallbackGap,
                anchor_nanos,
                gap_ns,
                OutputTimingQuality::load(),
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CallbackClockWindow {
    total_frames: u64,
    last_nanos: u64,
    last_base_frames: u64,
    last_callback_frames: u64,
    prev_nanos: u64,
    prev_base_frames: u64,
    prev_callback_frames: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct MusicMapSeg {
    stream_frame_start: i64,
    frames: i64,
    music_start_sec: f64,
    music_sec_per_frame: f64,
}

#[derive(Default)]
struct PlaybackPosMap {
    queue: VecDeque<MusicMapSeg>,
    backlog_frames: i64,
}

impl PlaybackPosMap {
    fn clear(&mut self) {
        self.queue.clear();
        self.backlog_frames = 0;
    }

    fn insert(&mut self, seg: MusicMapSeg) {
        if seg.frames <= 0
            || !seg.music_start_sec.is_finite()
            || !seg.music_sec_per_frame.is_finite()
        {
            return;
        }
        if let Some(last) = self.queue.back_mut() {
            let contiguous_stream = last.stream_frame_start + last.frames == seg.stream_frame_start;
            let ratio_match = (last.music_sec_per_frame - seg.music_sec_per_frame).abs() <= 1e-9;
            let expected_music_start =
                last.music_start_sec + last.music_sec_per_frame * last.frames as f64;
            let music_contiguous = (expected_music_start - seg.music_start_sec).abs()
                <= seg.music_sec_per_frame.abs().max(1e-9);
            if contiguous_stream && ratio_match && music_contiguous {
                last.frames += seg.frames;
                self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
                self.cleanup();
                return;
            }
        }
        self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
        self.queue.push_back(seg);
        self.cleanup();
    }

    fn cleanup(&mut self) {
        while self.backlog_frames > MUSIC_POS_MAP_BACKLOG_FRAMES {
            if let Some(front) = self.queue.pop_front() {
                self.backlog_frames = self.backlog_frames.saturating_sub(front.frames);
            } else {
                self.backlog_frames = 0;
                break;
            }
        }
    }

    fn search(&self, stream_frame: f64) -> Option<(f64, f64)> {
        if self.queue.is_empty() || !stream_frame.is_finite() {
            return None;
        }
        let mut closest = None;
        let mut closest_dist = f64::INFINITY;
        for seg in &self.queue {
            let start = seg.stream_frame_start as f64;
            let end = start + seg.frames as f64;
            if stream_frame >= start && stream_frame < end {
                let diff = stream_frame - start;
                return Some((
                    seg.music_start_sec + diff * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let start_dist = (stream_frame - start).abs();
            if start_dist < closest_dist {
                closest_dist = start_dist;
                closest = Some((
                    seg.music_start_sec + (stream_frame - start) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let end_music = seg.music_start_sec + seg.music_sec_per_frame * seg.frames as f64;
            let end_dist = (stream_frame - end).abs();
            if end_dist < closest_dist {
                closest_dist = end_dist;
                closest = Some((
                    end_music + (stream_frame - end) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
        }
        closest
    }
}

static QUEUED_MUSIC_MAP_SEGS: std::sync::LazyLock<Arc<internal::SpscRingMusicSeg>> =
    std::sync::LazyLock::new(|| internal::music_seg_ring_new(internal::MUSIC_SEG_RING_CAP));
static PLAYED_MUSIC_MAP_SEGS: std::sync::LazyLock<Arc<internal::SpscRingMusicSeg>> =
    std::sync::LazyLock::new(|| internal::music_seg_ring_new(internal::MUSIC_SEG_RING_CAP));
static PLAYBACK_POS_MAP: std::sync::LazyLock<Mutex<PlaybackPosMap>> =
    std::sync::LazyLock::new(|| Mutex::new(PlaybackPosMap::default()));

/* ============================ Public functions ============================ */

#[inline(always)]
fn engine_init_cfg() -> InitConfig {
    *ENGINE_INIT_CFG
        .get()
        .expect("engine::audio::init must be called before audio use")
}

#[inline(always)]
pub fn is_initialized() -> bool {
    ENGINE_INIT_CFG.get().is_some()
}

/// Initializes the audio engine. Must be called once at startup.
pub fn init(cfg: InitConfig) -> Result<(), String> {
    if let Some(existing) = ENGINE_INIT_CFG.get() {
        if *existing != cfg {
            return Err("audio engine already initialized with different config".to_string());
        }
    } else {
        let _ = ENGINE_INIT_CFG.set(cfg);
    }
    std::sync::LazyLock::force(&ENGINE);
    std::sync::LazyLock::force(&QUEUED_MUSIC_MAP_SEGS);
    std::sync::LazyLock::force(&PLAYED_MUSIC_MAP_SEGS);
    std::sync::LazyLock::force(&PLAYBACK_POS_MAP);
    Ok(())
}

pub fn startup_output_devices() -> Vec<OutputDeviceInfo> {
    ENGINE.startup_output_devices.clone()
}

#[cfg(target_os = "linux")]
pub fn available_linux_backends() -> Vec<crate::config::LinuxAudioBackend> {
    let mut backends = Vec::with_capacity(5);
    backends.push(crate::config::LinuxAudioBackend::Auto);
    #[cfg(has_pipewire_audio)]
    backends.push(crate::config::LinuxAudioBackend::PipeWire);
    #[cfg(has_pulse_audio)]
    if backends::linux_pulse::is_available() {
        backends.push(crate::config::LinuxAudioBackend::PulseAudio);
    }
    backends.push(crate::config::LinuxAudioBackend::Alsa);
    #[cfg(has_jack_audio)]
    if backends::linux_jack::is_available() {
        backends.push(crate::config::LinuxAudioBackend::Jack);
    }
    backends
}

/// Plays a sound effect from the given path (cached after first load).
pub fn play_sfx(path: &str) {
    play_sfx_on_lane(path, SfxLane::Effect);
}

/// Plays a gameplay assist tick that uses its own volume lane.
pub fn play_assist_tick(path: &str) {
    play_sfx_on_lane(path, SfxLane::AssistTick);
}

fn play_sfx_on_lane(path: &str, lane: SfxLane) {
    #[cfg(test)]
    if !is_initialized() {
        return;
    }

    let sound_data = {
        let mut cache = ENGINE.sfx_cache.lock().unwrap();
        if let Some(data) = cache.get(path) {
            data.clone()
        } else {
            let resolved = dirs::app_dirs().resolve_asset_path(path);
            let resolved_str = resolved.to_string_lossy();
            match resample::load_and_resample_sfx(&resolved_str) {
                Ok(data) => {
                    cache.insert(path.to_string(), data.clone());
                    debug!("Cached SFX: {path}");
                    data
                }
                Err(e) => {
                    warn!("Failed to load SFX '{path}': {e}");
                    return;
                }
            }
        }
    };
    let queued = QueuedSfx {
        data: sound_data,
        lane,
    };
    let _ = ENGINE.command_sender.send(AudioCommand::PlaySfx(queued));
}

/// Preloads a sound effect into cache without playing it.
pub fn preload_sfx(path: &str) {
    let mut cache = ENGINE.sfx_cache.lock().unwrap();
    if cache.contains_key(path) {
        return;
    }
    let resolved = dirs::app_dirs().resolve_asset_path(path);
    let resolved_str = resolved.to_string_lossy();
    match resample::load_and_resample_sfx(&resolved_str) {
        Ok(data) => {
            cache.insert(path.to_string(), data);
            debug!("Cached SFX: {path}");
        }
        Err(e) => {
            warn!("Failed to preload SFX '{path}': {e}");
        }
    }
}

#[inline(always)]
fn clear_music_pos_map() {
    internal::music_seg_ring_clear(&QUEUED_MUSIC_MAP_SEGS);
    internal::music_seg_ring_clear(&PLAYED_MUSIC_MAP_SEGS);
    PLAYBACK_POS_MAP.lock().unwrap().clear();
    MUSIC_MAP_GEN.fetch_add(1, Ordering::Release);
}

#[inline(always)]
fn reset_music_stream_clock() {
    // Reset immediately on the caller thread so async command handoff can't
    // leak the previous track's stream position into gameplay timing.
    let total = MUSIC_TOTAL_FRAMES.load(Ordering::Acquire);
    MUSIC_TRACK_START_FRAME.store(total, Ordering::Release);
    MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Release);
    MUSIC_TRACK_ACTIVE.store(false, Ordering::Release);
    clear_music_pos_map();
}

#[inline(always)]
fn callback_nanos_at(at: Instant) -> u64 {
    instant_nanos(at)
}

#[inline(always)]
fn current_callback_clock_nanos(valid_at: Instant, source: CallbackClockSource) -> Option<u64> {
    match source {
        CallbackClockSource::Instant => Some(callback_nanos_at(valid_at)),
        #[cfg(windows)]
        CallbackClockSource::Qpc => current_qpc_nanos(),
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "freebsd"))]
#[inline(always)]
fn f32_to_i16(sample: f32) -> i16 {
    let sample = sample.clamp(-1.0, 1.0);
    if sample >= 1.0 {
        i16::MAX
    } else if sample <= -1.0 {
        i16::MIN
    } else {
        (sample * (i16::MAX as f32 + 1.0)) as i16
    }
}

#[inline(always)]
fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / (i16::MAX as f32 + 1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        CallbackClockWindow, MusicMapSeg, PlaybackPosMap, stream_position_frames_from_window,
    };

    #[test]
    fn playback_pos_map_extrapolates_past_last_segment() {
        let mut map = PlaybackPosMap::default();
        map.insert(MusicMapSeg {
            stream_frame_start: 0,
            frames: 48_000,
            music_start_sec: 0.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });

        let (music_sec, sec_per_frame) = map.search(60_000.0).unwrap();
        assert!((music_sec - 1.25).abs() <= 1e-9, "music_sec={music_sec}");
        assert!(
            (sec_per_frame - (1.0 / 48_000.0)).abs() <= 1e-12,
            "sec_per_frame={sec_per_frame}"
        );
    }

    #[test]
    fn stream_clock_extrapolates_back_before_future_callback_anchor() {
        let frames = stream_position_frames_from_window(
            48_000,
            1_000,
            7_000_000,
            CallbackClockWindow {
                total_frames: 1_720,
                last_nanos: 15_000_001,
                last_base_frames: 1_480,
                last_callback_frames: 240,
                prev_nanos: 10_000_001,
                prev_base_frames: 1_240,
                prev_callback_frames: 240,
            },
        );

        assert!((frames - 96.0).abs() <= 1e-6, "frames={frames}");
    }
}

#[inline(always)]
fn stream_position_frames_from_callback(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    cb_nanos_plus_one: u64,
    base_frames: u64,
    buf_frames: u64,
) -> Option<f64> {
    if cb_nanos_plus_one == 0 {
        return None;
    }
    let cb_nanos = cb_nanos_plus_one.saturating_sub(1);
    if at_nanos < cb_nanos {
        return None;
    }
    let dt = (at_nanos.saturating_sub(cb_nanos) as f64) * 1e-9;
    let frames_since_cb = (dt * sample_rate as f64).clamp(0.0, buf_frames as f64);
    let frames_now = base_frames as f64 + frames_since_cb;
    Some((frames_now.max(start_frame as f64) - start_frame as f64).max(0.0))
}

#[inline(always)]
fn stream_position_frames_from_anchor_pair(
    start_frame: u64,
    at_nanos: u64,
    earlier_nanos_plus_one: u64,
    earlier_base_frames: u64,
    later_nanos_plus_one: u64,
    later_base_frames: u64,
) -> Option<f64> {
    if earlier_nanos_plus_one == 0 || later_nanos_plus_one == 0 {
        return None;
    }
    let earlier_nanos = earlier_nanos_plus_one.saturating_sub(1);
    let later_nanos = later_nanos_plus_one.saturating_sub(1);
    if later_nanos <= earlier_nanos || later_base_frames <= earlier_base_frames {
        return None;
    }
    let nanos_span = later_nanos.saturating_sub(earlier_nanos) as f64;
    if nanos_span <= 0.0 {
        return None;
    }
    let frames_per_ns = (later_base_frames - earlier_base_frames) as f64 / nanos_span;
    if !frames_per_ns.is_finite() || frames_per_ns <= 0.0 {
        return None;
    }
    let dt_ns = at_nanos as f64 - later_nanos as f64;
    let frames_now = later_base_frames as f64 + dt_ns * frames_per_ns;
    Some((frames_now.max(start_frame as f64) - start_frame as f64).max(0.0))
}

#[inline(always)]
fn begin_callback_clock_write() {
    CALLBACK_CLOCK_SEQ.fetch_add(1, Ordering::AcqRel);
}

#[inline(always)]
fn end_callback_clock_write() {
    CALLBACK_CLOCK_SEQ.fetch_add(1, Ordering::Release);
}

#[inline(always)]
fn publish_callback_window_start_nanos(
    total_before: u64,
    anchor_nanos: u64,
    source: CallbackClockSource,
) {
    note_timing_diag_callback_gap(anchor_nanos, source);
    begin_callback_clock_write();
    CALLBACK_CLOCK_SOURCE.store(source as u8, Ordering::Relaxed);
    PREV_CALLBACK_BASE_FRAMES.store(
        LAST_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    PREV_CALLBACK_FRAMES.store(
        LAST_CALLBACK_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    PREV_CALLBACK_ELAPSED_NANOS.store(
        LAST_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    LAST_CALLBACK_BASE_FRAMES.store(total_before, Ordering::Relaxed);
    LAST_CALLBACK_FRAMES.store(0, Ordering::Relaxed);
    LAST_CALLBACK_ELAPSED_NANOS.store(
        anchor_nanos.min(u64::MAX - 1).saturating_add(1),
        Ordering::Relaxed,
    );
    end_callback_clock_write();
}

#[inline(always)]
fn publish_callback_window_end(total_before: u64, frames: u64) {
    begin_callback_clock_write();
    LAST_CALLBACK_FRAMES.store(frames, Ordering::Relaxed);
    MUSIC_TOTAL_FRAMES.store(total_before.saturating_add(frames), Ordering::Relaxed);
    end_callback_clock_write();
}

fn load_callback_clock_snapshot_now() -> (Instant, u64, CallbackClockSource, CallbackClockWindow) {
    loop {
        let seq_start = CALLBACK_CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start & 1 != 0 {
            std::hint::spin_loop();
            continue;
        }
        let source = CallbackClockSource::load();
        let valid_at = Instant::now();
        let at_nanos = current_callback_clock_nanos(valid_at, source);
        let window = CallbackClockWindow {
            total_frames: MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed),
            last_nanos: LAST_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
            last_base_frames: LAST_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
            last_callback_frames: LAST_CALLBACK_FRAMES.load(Ordering::Relaxed),
            prev_nanos: PREV_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
            prev_base_frames: PREV_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
            prev_callback_frames: PREV_CALLBACK_FRAMES.load(Ordering::Relaxed),
        };
        let seq_end = CALLBACK_CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start == seq_end {
            let at_nanos = at_nanos.unwrap_or(window.last_nanos.saturating_sub(1));
            return (valid_at, at_nanos, source, window);
        }
    }
}

#[inline(always)]
fn stream_position_frames_from_window(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    window: CallbackClockWindow,
) -> f64 {
    if let Some(frames) = stream_position_frames_from_callback(
        sample_rate,
        start_frame,
        at_nanos,
        window.last_nanos,
        window.last_base_frames,
        window.last_callback_frames,
    ) {
        return frames;
    }
    if let Some(frames) = stream_position_frames_from_callback(
        sample_rate,
        start_frame,
        at_nanos,
        window.prev_nanos,
        window.prev_base_frames,
        window.prev_callback_frames,
    ) {
        return frames;
    }
    if let Some(frames) = stream_position_frames_from_anchor_pair(
        start_frame,
        at_nanos,
        window.prev_nanos,
        window.prev_base_frames,
        window.last_nanos,
        window.last_base_frames,
    ) {
        return frames;
    }
    if timing_diag_enabled() {
        debug!(
            "AUDIO_DIAG stream_pos_fallback sample_rate_hz={} at_nanos={} last_nanos={} last_base_frames={} last_callback_frames={} prev_nanos={} prev_base_frames={} prev_callback_frames={} total_frames={} start_frame={}",
            sample_rate,
            at_nanos,
            window.last_nanos,
            window.last_base_frames,
            window.last_callback_frames,
            window.prev_nanos,
            window.prev_base_frames,
            window.prev_callback_frames,
            window.total_frames,
            start_frame,
        );
    }
    window.total_frames.saturating_sub(start_frame) as f64
}

fn drain_played_music_map_segments() {
    let mut map = PLAYBACK_POS_MAP.lock().unwrap();
    while let Some(seg) = internal::music_seg_ring_pop(&PLAYED_MUSIC_MAP_SEGS) {
        map.insert(seg);
    }
}

fn lookup_music_position(stream_frames: f64, sample_rate: u32) -> Option<(f32, f32)> {
    drain_played_music_map_segments();
    let map = PLAYBACK_POS_MAP.lock().unwrap();
    map.search(stream_frames).map(|(music_sec, sec_per_frame)| {
        (
            music_sec as f32,
            (sec_per_frame * sample_rate as f64) as f32,
        )
    })
}

/// Plays a music track from a file path.
pub fn play_music(path: PathBuf, cut: Cut, looping: bool, rate: f32) {
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    reset_music_stream_clock();
    let _ = ENGINE
        .command_sender
        .send(AudioCommand::PlayMusic(path, cut, looping, rate));
}

/// Stops the currently playing music track.
pub fn stop_music() {
    reset_music_stream_clock();
    let _ = ENGINE.command_sender.send(AudioCommand::StopMusic);
}

/// Adjusts the playback rate for the current music stream, if any.
pub fn set_music_rate(rate: f32) {
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    let _ = ENGINE.command_sender.send(AudioCommand::SetMusicRate(rate));
}

/// Returns the elapsed real time (in seconds) of the currently playing
/// music stream, measured from the moment the first sample of that stream
/// reached the output callback. This is derived from the device's sample
/// clock and is independent of wall-clock time. The value is smoothed
/// between callbacks using the callback timestamp so it advances
/// continuously instead of in buffer-sized jumps.
pub fn get_music_stream_position_seconds() -> f32 {
    get_music_stream_clock_snapshot().stream_seconds
}

#[inline(always)]
fn music_stream_clock_snapshot_at_nanos(
    sample_rate: u32,
    start: u64,
    valid_at: Instant,
    at_nanos: u64,
    source: CallbackClockSource,
    window: CallbackClockWindow,
) -> MusicStreamClockSnapshot {
    let stream_frames = stream_position_frames_from_window(sample_rate, start, at_nanos, window);
    let stream_seconds = (stream_frames / sample_rate as f64) as f32;
    let (music_seconds, music_seconds_per_second, has_music_mapping) =
        match lookup_music_position(stream_frames, sample_rate) {
            Some((music_seconds, slope)) => (music_seconds, slope, true),
            None => (stream_seconds, 1.0, false),
        };
    MusicStreamClockSnapshot {
        stream_seconds,
        music_seconds,
        music_nanos: music_nanos_from_seconds(music_seconds as f64),
        music_seconds_per_second,
        has_music_mapping,
        valid_at,
        valid_at_host_nanos: match source {
            #[cfg(windows)]
            CallbackClockSource::Qpc => at_nanos,
            #[cfg(windows)]
            CallbackClockSource::Instant => 0,
            #[cfg(not(windows))]
            CallbackClockSource::Instant => at_nanos,
        },
    }
}

#[inline(always)]
fn music_stream_clock_snapshot_at_host_nanos(host_nanos: u64) -> Option<MusicStreamClockSnapshot> {
    if host_nanos == 0 || !MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire) {
        return None;
    }
    let sample_rate = ENGINE.device_sample_rate.max(1);
    let start = MUSIC_TRACK_START_FRAME.load(Ordering::Acquire);
    let (valid_at, _, source, window) = load_callback_clock_snapshot_now();
    #[cfg(windows)]
    if !matches!(source, CallbackClockSource::Qpc) {
        return None;
    }
    Some(music_stream_clock_snapshot_at_nanos(
        sample_rate,
        start,
        valid_at,
        host_nanos,
        source,
        window,
    ))
}

/// Returns the current stream position and the `Instant` it is valid for.
pub fn get_music_stream_clock_snapshot() -> MusicStreamClockSnapshot {
    let sample_rate = ENGINE.device_sample_rate.max(1);
    let has_started = MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire);
    if !has_started {
        return MusicStreamClockSnapshot {
            stream_seconds: 0.0,
            music_seconds: 0.0,
            music_nanos: 0,
            music_seconds_per_second: 1.0,
            has_music_mapping: false,
            valid_at: Instant::now(),
            valid_at_host_nanos: 0,
        };
    }
    let start = MUSIC_TRACK_START_FRAME.load(Ordering::Acquire);
    let (valid_at, at_nanos, source, window) = load_callback_clock_snapshot_now();
    music_stream_clock_snapshot_at_nanos(sample_rate, start, valid_at, at_nanos, source, window)
}

pub fn get_music_stream_position_nanos() -> i64 {
    get_music_stream_clock_snapshot().music_nanos
}

pub fn get_music_stream_position_nanos_at_host_nanos(host_nanos: u64) -> Option<i64> {
    music_stream_clock_snapshot_at_host_nanos(host_nanos).map(|snapshot| snapshot.music_nanos)
}

pub fn get_output_timing_snapshot() -> OutputTimingSnapshot {
    OutputTimingSnapshot {
        backend: OutputTelemetryBackend::load(),
        requested_output_mode: output_mode_from_bits(
            OUTPUT_TIMING_REQUESTED_MODE.load(Ordering::Relaxed),
        ),
        fallback_from_native: OUTPUT_TIMING_NATIVE_FALLBACK.load(Ordering::Relaxed),
        timing_clock: OutputTelemetryClock::load(),
        timing_quality: OutputTimingQuality::load(),
        sample_rate_hz: OUTPUT_TIMING_SAMPLE_RATE_HZ.load(Ordering::Relaxed),
        device_period_ns: OUTPUT_TIMING_DEVICE_PERIOD_NS.load(Ordering::Relaxed),
        stream_latency_ns: OUTPUT_TIMING_STREAM_LATENCY_NS.load(Ordering::Relaxed),
        buffer_frames: OUTPUT_TIMING_BUFFER_FRAMES.load(Ordering::Relaxed),
        padding_frames: OUTPUT_TIMING_PADDING_FRAMES.load(Ordering::Relaxed),
        queued_frames: OUTPUT_TIMING_QUEUED_FRAMES.load(Ordering::Relaxed),
        estimated_output_delay_ns: OUTPUT_TIMING_EST_DELAY_NS.load(Ordering::Relaxed),
        clock_fallback_count: OUTPUT_TIMING_CLOCK_FALLBACKS.load(Ordering::Relaxed),
        timing_sanity_failure_count: OUTPUT_TIMING_SANITY_FAILURES.load(Ordering::Relaxed),
        underrun_count: OUTPUT_TIMING_UNDERRUNS.load(Ordering::Relaxed),
    }
}

/* ============================ Engine internals ============================ */

#[inline(always)]
fn publish_output_backend_ready(ready: OutputBackendReady) {
    OUTPUT_TIMING_BACKEND.store(
        OutputTelemetryBackend::from_backend_name(ready.backend_name) as u8,
        Ordering::Relaxed,
    );
    OUTPUT_TIMING_REQUESTED_MODE.store(
        output_mode_bits(ready.requested_output_mode),
        Ordering::Relaxed,
    );
    OUTPUT_TIMING_NATIVE_FALLBACK.store(ready.fallback_from_native, Ordering::Relaxed);
    OUTPUT_TIMING_CLOCK.store(ready.timing_clock as u8, Ordering::Relaxed);
    OUTPUT_TIMING_QUALITY.store(ready.timing_quality as u8, Ordering::Relaxed);
    OUTPUT_TIMING_SAMPLE_RATE_HZ.store(ready.device_sample_rate, Ordering::Relaxed);
    OUTPUT_TIMING_DEVICE_PERIOD_NS.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_STREAM_LATENCY_NS.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_BUFFER_FRAMES.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_PADDING_FRAMES.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_QUEUED_FRAMES.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_EST_DELAY_NS.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_CLOCK_FALLBACKS.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_SANITY_FAILURES.store(0, Ordering::Relaxed);
    OUTPUT_TIMING_UNDERRUNS.store(0, Ordering::Relaxed);
}

#[inline(always)]
pub(crate) fn publish_output_timing(
    sample_rate_hz: u32,
    device_period_ns: u64,
    stream_latency_ns: u64,
    buffer_frames: u32,
    padding_frames: u32,
    queued_frames: u32,
    estimated_output_delay_ns: u64,
) {
    OUTPUT_TIMING_SAMPLE_RATE_HZ.store(sample_rate_hz, Ordering::Relaxed);
    OUTPUT_TIMING_DEVICE_PERIOD_NS.store(device_period_ns, Ordering::Relaxed);
    OUTPUT_TIMING_STREAM_LATENCY_NS.store(stream_latency_ns, Ordering::Relaxed);
    OUTPUT_TIMING_BUFFER_FRAMES.store(buffer_frames, Ordering::Relaxed);
    OUTPUT_TIMING_PADDING_FRAMES.store(padding_frames, Ordering::Relaxed);
    OUTPUT_TIMING_QUEUED_FRAMES.store(queued_frames, Ordering::Relaxed);
    OUTPUT_TIMING_EST_DELAY_NS.store(estimated_output_delay_ns, Ordering::Relaxed);
}

#[inline(always)]
pub(crate) fn note_output_underrun() {
    OUTPUT_TIMING_UNDERRUNS.fetch_add(1, Ordering::Relaxed);
    record_stutter_diag_event(
        StutterDiagAudioEventKind::Underrun,
        instant_nanos(Instant::now()),
        0,
        OutputTimingQuality::load(),
    );
}

#[inline(always)]
#[cfg(unix)]
pub(crate) fn publish_output_timing_quality(quality: OutputTimingQuality) {
    OUTPUT_TIMING_QUALITY.store(quality as u8, Ordering::Relaxed);
}

#[inline(always)]
#[cfg(unix)]
pub(crate) fn note_output_timing_sanity_failure(quality: OutputTimingQuality) {
    OUTPUT_TIMING_QUALITY.store(quality as u8, Ordering::Relaxed);
    OUTPUT_TIMING_SANITY_FAILURES.fetch_add(1, Ordering::Relaxed);
    if !matches!(quality, OutputTimingQuality::Fallback) {
        record_stutter_diag_event(
            StutterDiagAudioEventKind::TimingSanity,
            instant_nanos(Instant::now()),
            0,
            quality,
        );
    }
}

#[inline(always)]
#[cfg(unix)]
pub(crate) fn note_output_clock_fallback() {
    note_output_timing_sanity_failure(OutputTimingQuality::Fallback);
    OUTPUT_TIMING_CLOCK_FALLBACKS.fetch_add(1, Ordering::Relaxed);
    record_stutter_diag_event(
        StutterDiagAudioEventKind::ClockFallback,
        instant_nanos(Instant::now()),
        0,
        OutputTimingQuality::Fallback,
    );
}

fn commit_played_music_map(
    track_frame_start: i64,
    frames_popped: i64,
    queued_seg_ring: &internal::SpscRingMusicSeg,
    played_seg_ring: &internal::SpscRingMusicSeg,
    current_seg: &mut Option<MusicMapSeg>,
) {
    let mut stream_frame = track_frame_start;
    let mut remaining = frames_popped.max(0);
    while remaining > 0 {
        let mut seg = match current_seg.take() {
            Some(seg) => seg,
            None => match internal::music_seg_ring_pop(queued_seg_ring) {
                Some(seg) => seg,
                None => break,
            },
        };
        let take = remaining.min(seg.frames);
        let played = MusicMapSeg {
            stream_frame_start: stream_frame,
            frames: take,
            music_start_sec: seg.music_start_sec,
            music_sec_per_frame: seg.music_sec_per_frame,
        };
        let _ = internal::music_seg_ring_push(played_seg_ring, played);
        seg.frames -= take;
        seg.music_start_sec += seg.music_sec_per_frame * take as f64;
        stream_frame += take;
        remaining -= take;
        if seg.frames > 0 {
            *current_seg = Some(seg);
        }
    }
}

struct RenderState {
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    device_channels: usize,
    mix_i16: Vec<i16>,
    mix_f32: Vec<f32>,
    active_sfx: Vec<(Arc<Vec<i16>>, usize, SfxLane)>,
    queued_music_map: Arc<internal::SpscRingMusicSeg>,
    played_music_map: Arc<internal::SpscRingMusicSeg>,
    active_music_map: Option<MusicMapSeg>,
    music_map_generation: u64,
}

impl RenderState {
    fn new(
        music_ring: Arc<internal::SpscRingI16>,
        sfx_receiver: Receiver<QueuedSfx>,
        device_channels: usize,
    ) -> Self {
        Self {
            music_ring,
            sfx_receiver,
            device_channels,
            mix_i16: Vec::new(),
            mix_f32: Vec::new(),
            active_sfx: Vec::new(),
            queued_music_map: QUEUED_MUSIC_MAP_SEGS.clone(),
            played_music_map: PLAYED_MUSIC_MAP_SEGS.clone(),
            active_music_map: None,
            music_map_generation: MUSIC_MAP_GEN.load(Ordering::Acquire),
        }
    }

    #[inline(always)]
    fn begin_callback_nanos(&mut self, anchor_nanos: u64, source: CallbackClockSource) -> u64 {
        let map_generation = MUSIC_MAP_GEN.load(Ordering::Acquire);
        if map_generation != self.music_map_generation {
            self.active_music_map = None;
            self.music_map_generation = map_generation;
        }
        if !MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed) {
            self.active_music_map = None;
        }
        let total_before = MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed);
        publish_callback_window_start_nanos(total_before, anchor_nanos, source);
        total_before
    }

    #[cfg(windows)]
    #[inline(always)]
    fn begin_callback_qpc(&mut self, anchor_nanos: u64) -> u64 {
        self.begin_callback_nanos(anchor_nanos, CallbackClockSource::Qpc)
    }

    #[inline(always)]
    fn ensure_mix_buffers(&mut self, len: usize) {
        if self.mix_i16.len() != len {
            self.mix_i16.resize(len, 0);
        }
        if self.mix_f32.len() != len {
            self.mix_f32.resize(len, 0.0);
        }
    }

    #[inline(always)]
    fn mix_levels() -> (f32, f32, f32) {
        let config = crate::config::audio_mix_levels();
        let master_vol = f32::from(config.master_volume) * 0.01;
        let music_vol = f32::from(config.music_volume) * 0.01;
        let sfx_vol = f32::from(config.sfx_volume) * 0.01;
        let assist_tick_vol = f32::from(config.assist_tick_volume) * 0.01;
        (
            master_vol * music_vol,
            master_vol * sfx_vol,
            master_vol * assist_tick_vol,
        )
    }

    fn mix_f32_buffer(&mut self, total_before: u64, len: usize) -> usize {
        self.ensure_mix_buffers(len);
        let popped = internal::callback_fill_from_ring_i16(&self.music_ring, &mut self.mix_i16);
        if MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
            && !MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
            && popped > 0
        {
            MUSIC_TRACK_START_FRAME.store(total_before, Ordering::Release);
            MUSIC_TRACK_HAS_STARTED.store(true, Ordering::Release);
        }

        let (music_vol, sfx_vol, assist_tick_vol) = Self::mix_levels();
        for (dst, src) in self.mix_f32.iter_mut().zip(&self.mix_i16) {
            *dst = i16_to_f32(*src) * music_vol;
        }

        for new_sfx in self.sfx_receiver.try_iter() {
            self.active_sfx.push((new_sfx.data, 0, new_sfx.lane));
        }

        self.active_sfx.retain_mut(|(data, cursor, lane)| {
            let n = (data.len().saturating_sub(*cursor)).min(self.mix_f32.len());
            let lane_vol = match *lane {
                SfxLane::Effect => sfx_vol,
                SfxLane::AssistTick => assist_tick_vol,
            };
            for i in 0..n {
                let sfx_sample_f32 = i16_to_f32(data[*cursor + i]) * lane_vol;
                self.mix_f32[i] = (self.mix_f32[i] + sfx_sample_f32).clamp(-1.0, 1.0);
            }
            *cursor += n;
            *cursor < data.len()
        });

        popped
    }

    #[inline(always)]
    fn finish_callback(
        &mut self,
        total_before: u64,
        emitted_samples: usize,
        popped_samples: usize,
    ) {
        let frames = if self.device_channels == 0 {
            0
        } else {
            emitted_samples / self.device_channels
        };
        let popped_frames = if self.device_channels == 0 {
            0
        } else {
            popped_samples / self.device_channels
        };
        if MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
            && MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
            && popped_frames < frames
        {
            note_output_underrun();
        }
        let track_frames_before =
            total_before.saturating_sub(MUSIC_TRACK_START_FRAME.load(Ordering::Acquire));
        if popped_frames > 0 {
            commit_played_music_map(
                track_frames_before as i64,
                popped_frames as i64,
                &self.queued_music_map,
                &self.played_music_map,
                &mut self.active_music_map,
            );
        }
        if frames > 0 {
            publish_callback_window_end(total_before, frames as u64);
        }
    }

    #[cfg(windows)]
    fn render_i16_qpc(&mut self, out: &mut [i16], anchor_nanos: u64) {
        let total_before = self.begin_callback_qpc(anchor_nanos);
        let popped = self.mix_f32_buffer(total_before, out.len());
        for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
            *dst = f32_to_i16(*src);
        }
        self.finish_callback(total_before, out.len(), popped);
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn render_i16_host_nanos(&mut self, out: &mut [i16], anchor_nanos: u64) {
        let total_before = self.begin_callback_nanos(anchor_nanos, CallbackClockSource::Instant);
        let popped = self.mix_f32_buffer(total_before, out.len());
        for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
            *dst = f32_to_i16(*src);
        }
        self.finish_callback(total_before, out.len(), popped);
    }

    #[cfg(any(
        target_os = "macos",
        all(target_os = "linux", any(has_jack_audio, has_pipewire_audio))
    ))]
    fn render_f32_host_nanos(&mut self, out: &mut [f32], anchor_nanos: u64) {
        let total_before = self.begin_callback_nanos(anchor_nanos, CallbackClockSource::Instant);
        let popped = self.mix_f32_buffer(total_before, out.len());
        out.copy_from_slice(&self.mix_f32[..out.len()]);
        self.finish_callback(total_before, out.len(), popped);
    }

    #[cfg(windows)]
    fn render_f32_qpc(&mut self, out: &mut [f32], anchor_nanos: u64) {
        let total_before = self.begin_callback_qpc(anchor_nanos);
        let popped = self.mix_f32_buffer(total_before, out.len());
        out.copy_from_slice(&self.mix_f32[..out.len()]);
        self.finish_callback(total_before, out.len(), popped);
    }
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn linux_default_output_device(
    devices: &[backends::linux_alsa::AlsaOutputDevice],
) -> Option<&backends::linux_alsa::AlsaOutputDevice> {
    devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first())
}

#[cfg(target_os = "linux")]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, AudioThreadLaunch) {
    let alsa_devices = backends::linux_alsa::enumerate_output_devices();
    if alsa_devices.is_empty() {
        warn!(
            "No ALSA playback devices were enumerated at startup; Linux audio will rely on backend defaults."
        );
    }
    let device_probes: Vec<_> = alsa_devices
        .iter()
        .map(|device| OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: device.name.clone(),
                is_default: device.is_default,
                sample_rates_hz: device.sample_rates_hz.clone(),
            },
        })
        .collect();
    let output_mode = cfg.output_mode;
    let linux_backend = cfg.linux_backend;
    let default_device = linux_default_output_device(&alsa_devices);
    let requested_device = cfg
        .output_device_index
        .and_then(|idx| alsa_devices.get(idx as usize));
    let explicit_device_requested = requested_device.is_some();
    if let Some(requested_idx) = cfg.output_device_index {
        if let Some(device) = requested_device {
            info!(
                "Audio output device override selected: index {} '{}'.",
                requested_idx, device.name
            );
        } else {
            warn!(
                "Audio output device override index {} not found; using default device.",
                requested_idx
            );
        }
    }
    let selected_device = requested_device.or_else(|| {
        (matches!(output_mode, crate::config::AudioOutputMode::Exclusive)
            && matches!(
                linux_backend,
                crate::config::LinuxAudioBackend::Auto | crate::config::LinuxAudioBackend::Alsa
            ))
        .then_some(default_device)
        .flatten()
    });
    let (device_name, alsa_pcm_id) = if let Some(device) = selected_device {
        if !explicit_device_requested {
            info!(
                "Audio output device auto-selected for ALSA exclusive mode: '{}' ({})",
                device.name, device.pcm_id
            );
        }
        (device.name.clone(), Some(device.pcm_id.clone()))
    } else {
        ("Default Audio Device".to_string(), None)
    };
    let fallback_device = selected_device.or(default_device);
    let native_sample_rate_hz = cfg
        .sample_rate_hz
        .unwrap_or_else(|| fallback_device.map_or(48_000, |device| device.default_rate_hz));
    let native_channels = fallback_device.map_or(2, |device| device.channels);
    debug!(
        "Audio device: '{}' (native={} Hz, channels={}).",
        device_name, native_sample_rate_hz, native_channels
    );
    debug!(
        "Audio output stream config: {} Hz, {} ch, mode={} (Linux native path).",
        native_sample_rate_hz,
        native_channels,
        output_mode.as_str()
    );
    (
        device_probes,
        AudioThreadLaunch {
            explicit_device_requested,
            linux_backend,
            alsa: Some(AlsaBackendHint {
                pcm_id: alsa_pcm_id,
                device_name: device_name.clone(),
                sample_rate_hz: native_sample_rate_hz,
                channels: native_channels,
                output_mode,
            }),
            #[cfg(has_jack_audio)]
            jack: Some(JackBackendHint {
                requested_device_name: explicit_device_requested.then_some(device_name.clone()),
                requested_rate_hz: cfg.sample_rate_hz,
                output_mode,
            }),
            #[cfg(has_pipewire_audio)]
            pipewire: Some(PipeWireBackendHint {
                requested_device_name: explicit_device_requested.then_some(device_name.clone()),
                sample_rate_hz: native_sample_rate_hz,
                channels: native_channels,
                output_mode,
            }),
            #[cfg(has_pulse_audio)]
            pulse: Some(PulseBackendHint {
                requested_device_name: explicit_device_requested.then_some(device_name),
                sample_rate_hz: native_sample_rate_hz,
                channels: native_channels,
                output_mode,
            }),
        },
    )
}

#[cfg(target_os = "macos")]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, AudioThreadLaunch) {
    let devices = backends::macos_coreaudio::enumerate_output_devices();
    if devices.is_empty() {
        warn!(
            "No CoreAudio output devices were enumerated at startup; native audio will use the system default device."
        );
    }
    let device_probes: Vec<_> = devices
        .iter()
        .map(|device| OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: device.name.clone(),
                is_default: device.is_default,
                sample_rates_hz: device.sample_rates_hz.clone(),
            },
        })
        .collect();
    let output_mode = cfg.output_mode;
    let default_device = devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first());
    let requested_device = cfg
        .output_device_index
        .and_then(|idx| devices.get(idx as usize));
    if let Some(requested_idx) = cfg.output_device_index {
        if let Some(device) = requested_device {
            info!(
                "Audio output device override selected: index {} '{}'.",
                requested_idx, device.name
            );
        } else {
            warn!(
                "Audio output device override index {} not found; using default device.",
                requested_idx
            );
        }
    }
    let selected_device = requested_device.or(default_device);
    let device_name = selected_device
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "Default Audio Device".to_string());
    let device_uid = selected_device.map(|device| device.uid.clone());
    let requested_rate_hz = cfg.sample_rate_hz;
    let native_sample_rate_hz = requested_rate_hz
        .unwrap_or_else(|| selected_device.map_or(48_000, |device| device.default_rate_hz));
    let native_channels = selected_device.map_or(2, |device| device.channels);
    debug!(
        "Audio device: '{}' (native={} Hz, channels={}).",
        device_name, native_sample_rate_hz, native_channels
    );
    debug!(
        "Audio output stream config: {} Hz, {} ch, mode={} (CoreAudio native path).",
        native_sample_rate_hz,
        native_channels,
        output_mode.as_str()
    );
    (
        device_probes,
        AudioThreadLaunch {
            coreaudio: Some(CoreAudioBackendHint {
                device_uid,
                device_name,
                requested_rate_hz,
                channels: native_channels,
                output_mode,
            }),
        },
    )
}

#[cfg(windows)]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, AudioThreadLaunch) {
    let devices = match backends::windows_wasapi::enumerate_output_devices() {
        Ok(devices) => devices,
        Err(err) => {
            warn!("Failed to enumerate WASAPI output devices at startup: {err}");
            Vec::new()
        }
    };
    if devices.is_empty() {
        warn!(
            "No WASAPI output devices were enumerated at startup; native audio will use the system default device."
        );
    }
    let device_probes: Vec<_> = devices
        .iter()
        .map(|device| OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: device.name.clone(),
                is_default: device.is_default,
                sample_rates_hz: device.sample_rates_hz.clone(),
            },
        })
        .collect();
    let output_mode = cfg.output_mode;
    let requested_rate_hz = cfg.sample_rate_hz;
    let default_device = devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first());
    let requested_device = cfg
        .output_device_index
        .and_then(|idx| devices.get(idx as usize));
    if let Some(requested_idx) = cfg.output_device_index {
        if let Some(device) = requested_device {
            info!(
                "Audio output device override selected: index {} '{}'.",
                requested_idx, device.name
            );
        } else {
            warn!(
                "Audio output device override index {} not found; using default device.",
                requested_idx
            );
        }
    }
    let selected_device = requested_device.or(default_device);
    let device_name = selected_device
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "Default Audio Device".to_string());
    let device_id = selected_device.map(|device| device.id.clone());
    let native_sample_rate_hz = selected_device.map_or(48_000, |device| device.mix_rate_hz);
    let native_channels = selected_device.map_or(2, |device| device.channels);
    debug!(
        "Audio device: '{}' (native={} Hz, channels={}).",
        device_name, native_sample_rate_hz, native_channels
    );
    debug!(
        "Audio output stream config: {} Hz request, mode={} (WASAPI native path).",
        requested_rate_hz.unwrap_or(native_sample_rate_hz),
        output_mode.as_str()
    );
    (
        device_probes,
        AudioThreadLaunch {
            wasapi: Some(WasapiBackendHint {
                device_id,
                device_name,
                requested_rate_hz,
                output_mode,
            }),
        },
    )
}

#[cfg(target_os = "freebsd")]
fn build_audio_launch(cfg: &InitConfig) -> (Vec<OutputDeviceProbe>, AudioThreadLaunch) {
    let mut device_probes: Vec<_> = backends::freebsd_pcm::enumerate_output_devices()
        .into_iter()
        .map(|dev| OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: dev.name,
                is_default: dev.is_default,
                sample_rates_hz: Vec::new(),
            },
            freebsd_dsp_path: Some(dev.path),
        })
        .collect();
    let output_mode = cfg.output_mode;
    let mut device_name = device_probes
        .iter()
        .find(|probe| probe.info.is_default)
        .map(|probe| probe.info.name.clone())
        .unwrap_or_else(|| "FreeBSD PCM default".to_string());
    let mut dsp_path = device_probes
        .iter()
        .find(|probe| probe.info.is_default)
        .and_then(|probe| probe.freebsd_dsp_path.clone());
    if let Some(requested_idx) = cfg.output_device_index {
        if let Some(probe) = device_probes.get(requested_idx as usize) {
            device_name = probe.info.name.clone();
            dsp_path = probe.freebsd_dsp_path.clone();
            info!(
                "Audio output device override selected: index {} '{}'.",
                requested_idx, device_name
            );
        } else {
            warn!(
                "Audio output device override index {} not found; using default device.",
                requested_idx
            );
        }
    }
    if device_probes.is_empty() {
        warn!(
            "No FreeBSD PCM devices were enumerated at startup; native audio will still try /dev/dsp."
        );
        device_probes.push(OutputDeviceProbe {
            info: OutputDeviceInfo {
                name: "FreeBSD PCM (/dev/dsp)".to_string(),
                is_default: true,
                sample_rates_hz: Vec::new(),
            },
            freebsd_dsp_path: Some("/dev/dsp".to_string()),
        });
        if dsp_path.is_none() {
            dsp_path = Some("/dev/dsp".to_string());
            device_name = "FreeBSD PCM (/dev/dsp)".to_string();
        }
    }
    let sample_rate_hz = cfg.sample_rate_hz.unwrap_or(48_000).max(1);
    debug!(
        "FreeBSD PCM device '{}' selected at {} Hz, 2 ch, mode={}.",
        device_name,
        sample_rate_hz,
        output_mode.as_str()
    );
    (
        device_probes,
        AudioThreadLaunch {
            #[cfg(target_os = "linux")]
            explicit_device_requested: false,
            #[cfg(target_os = "linux")]
            linux_backend: cfg.linux_backend,
            #[cfg(target_os = "linux")]
            alsa: None,
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            jack: None,
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            pipewire: None,
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            pulse: None,
            #[cfg(target_os = "macos")]
            coreaudio: None,
            freebsd_pcm: Some(FreeBsdPcmBackendHint {
                dsp_path,
                device_name,
                sample_rate_hz,
                channels: 2,
                output_mode,
            }),
            #[cfg(windows)]
            wasapi: None,
        },
    )
}

fn init_engine_and_thread(cfg: InitConfig) -> AudioEngine {
    let (command_sender, command_receiver) = channel();
    let (ready_sender, ready_receiver) = channel();
    let (device_probes, launch) = build_audio_launch(&cfg);

    thread::spawn(move || {
        audio_manager_thread(command_receiver, ready_sender, launch);
    });

    let ready = match ready_receiver.recv() {
        Ok(Ok(ready)) => ready,
        Ok(Err(err)) => panic!("failed to initialize audio engine: {err}"),
        Err(_) => panic!("audio manager thread exited before reporting ready"),
    };

    info!(
        "Audio engine initialized ({} Hz, {} ch, backend={} req={} fallback={} clock={} quality={} device='{}').",
        ready.device_sample_rate,
        ready.device_channels,
        ready.backend_name,
        ready.requested_output_mode.as_str(),
        ready.fallback_from_native,
        ready.timing_clock,
        ready.timing_quality,
        ready.device_name
    );
    publish_output_backend_ready(ready.clone());
    AudioEngine {
        command_sender,
        sfx_cache: Mutex::new(HashMap::new()),
        device_sample_rate: ready.device_sample_rate,
        device_channels: ready.device_channels,
        startup_output_devices: device_probes.into_iter().map(|probe| probe.info).collect(),
    }
}

#[allow(dead_code)]
enum OutputBackend {
    #[cfg(target_os = "linux")]
    Alsa(backends::linux_alsa::AlsaOutputStream),
    #[cfg(target_os = "linux")]
    #[cfg(has_jack_audio)]
    Jack(backends::linux_jack::JackOutputStream),
    #[cfg(target_os = "linux")]
    #[cfg(has_pipewire_audio)]
    PipeWire(backends::linux_pipewire::PipeWireOutputStream),
    #[cfg(target_os = "linux")]
    #[cfg(has_pulse_audio)]
    Pulse(backends::linux_pulse::PulseOutputStream),
    #[cfg(target_os = "macos")]
    CoreAudio(backends::macos_coreaudio::CoreAudioOutputStream),
    #[cfg(target_os = "freebsd")]
    FreeBsdPcm(backends::freebsd_pcm::FreeBsdPcmOutputStream),
    #[cfg(windows)]
    Wasapi(backends::windows_wasapi::WasapiOutputStream),
}

#[cfg(target_os = "linux")]
fn start_linux_alsa_backend(
    alsa: AlsaBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    let access_mode = match alsa.output_mode {
        crate::config::AudioOutputMode::Exclusive => {
            backends::linux_alsa::AlsaAccessMode::Exclusive
        }
        crate::config::AudioOutputMode::Auto | crate::config::AudioOutputMode::Shared => {
            backends::linux_alsa::AlsaAccessMode::Shared
        }
    };
    let prep = backends::linux_alsa::prepare(
        alsa.pcm_id.clone(),
        alsa.device_name.clone(),
        alsa.sample_rate_hz,
        alsa.channels,
        access_mode,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = alsa.output_mode;
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
    let stream = backends::linux_alsa::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::Alsa(stream), ready, sfx_sender))
}

#[cfg(target_os = "linux")]
#[cfg(has_jack_audio)]
fn start_linux_jack_backend(
    jack: JackBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    if matches!(jack.output_mode, crate::config::AudioOutputMode::Exclusive) {
        return Err("JACK does not expose a separate exclusive output mode.".to_string());
    }
    let prep =
        backends::linux_jack::prepare(jack.requested_device_name.clone(), jack.requested_rate_hz)?;
    let mut ready = prep.ready();
    ready.requested_output_mode = jack.output_mode;
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
    let stream = backends::linux_jack::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::Jack(stream), ready, sfx_sender))
}

#[cfg(target_os = "linux")]
#[cfg(has_pipewire_audio)]
fn start_linux_pipewire_backend(
    pipewire: PipeWireBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    if matches!(
        pipewire.output_mode,
        crate::config::AudioOutputMode::Exclusive
    ) {
        return Err("PipeWire does not support a separate exclusive output mode.".to_string());
    }
    if let Some(name) = &pipewire.requested_device_name {
        warn!(
            "PipeWire backend ignores explicit Sound Device selection '{}'; using the default PipeWire sink.",
            name
        );
    }
    let prep = backends::linux_pipewire::prepare(
        pipewire.requested_device_name.clone(),
        pipewire.sample_rate_hz,
        pipewire.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = pipewire.output_mode;
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
    let stream = backends::linux_pipewire::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::PipeWire(stream), ready, sfx_sender))
}

#[cfg(target_os = "linux")]
#[cfg(has_pulse_audio)]
fn start_linux_pulse_backend(
    pulse: PulseBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    if matches!(pulse.output_mode, crate::config::AudioOutputMode::Exclusive) {
        return Err("PulseAudio does not support exclusive output.".to_string());
    }
    if let Some(name) = &pulse.requested_device_name {
        warn!(
            "PulseAudio backend ignores explicit Sound Device selection '{}'; using the default PulseAudio sink.",
            name
        );
    }
    let prep = backends::linux_pulse::prepare(
        pulse.requested_device_name.clone(),
        pulse.sample_rate_hz,
        pulse.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = pulse.output_mode;
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
    let stream = backends::linux_pulse::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::Pulse(stream), ready, sfx_sender))
}

#[cfg(target_os = "freebsd")]
fn start_freebsd_pcm_backend(
    pcm: FreeBsdPcmBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    if matches!(pcm.output_mode, crate::config::AudioOutputMode::Exclusive) {
        return Err("FreeBSD PCM exclusive output is not implemented yet.".to_string());
    }
    let prep = backends::freebsd_pcm::prepare(
        pcm.dsp_path.clone(),
        pcm.device_name.clone(),
        pcm.sample_rate_hz,
        pcm.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = pcm.output_mode;
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
    let stream = backends::freebsd_pcm::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::FreeBsdPcm(stream), ready, sfx_sender))
}

#[cfg(target_os = "macos")]
fn start_macos_coreaudio_backend(
    coreaudio: CoreAudioBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    if matches!(
        coreaudio.output_mode,
        crate::config::AudioOutputMode::Exclusive
    ) {
        return Err("CoreAudio exclusive output is not implemented yet.".to_string());
    }
    let prep = backends::macos_coreaudio::prepare(
        coreaudio.device_uid.clone(),
        coreaudio.device_name.clone(),
        coreaudio.requested_rate_hz,
        coreaudio.channels,
    )?;
    let mut ready = prep.ready();
    ready.requested_output_mode = coreaudio.output_mode;
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
    let stream = backends::macos_coreaudio::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::CoreAudio(stream), ready, sfx_sender))
}

fn start_output_backend(
    launch: AudioThreadLaunch,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    let AudioThreadLaunch {
        #[cfg(target_os = "linux")]
        explicit_device_requested,
        #[cfg(target_os = "linux")]
        linux_backend,
        #[cfg(target_os = "linux")]
        alsa,
        #[cfg(target_os = "linux")]
        #[cfg(has_jack_audio)]
        jack,
        #[cfg(target_os = "linux")]
        #[cfg(has_pipewire_audio)]
        pipewire,
        #[cfg(target_os = "linux")]
        #[cfg(has_pulse_audio)]
        pulse,
        #[cfg(target_os = "macos")]
        coreaudio,
        #[cfg(target_os = "freebsd")]
        freebsd_pcm,
        #[cfg(windows)]
        wasapi,
    } = launch;
    #[cfg(target_os = "linux")]
    let requested_output_mode = alsa
        .as_ref()
        .map(|hint| hint.output_mode)
        .or({
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            {
                pipewire.as_ref().map(|hint| hint.output_mode)
            }
            #[cfg(not(all(target_os = "linux", has_pipewire_audio)))]
            {
                None
            }
        })
        .or({
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            {
                jack.as_ref().map(|hint| hint.output_mode)
            }
            #[cfg(not(all(target_os = "linux", has_jack_audio)))]
            {
                None
            }
        })
        .or({
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            {
                pulse.as_ref().map(|hint| hint.output_mode)
            }
            #[cfg(not(all(target_os = "linux", has_pulse_audio)))]
            {
                None
            }
        })
        .unwrap_or(crate::config::AudioOutputMode::Auto);
    #[cfg(target_os = "linux")]
    match linux_backend {
        crate::config::LinuxAudioBackend::Alsa => {
            let Some(alsa) = alsa else {
                return Err("Linux ALSA backend hint unavailable.".to_string());
            };
            start_linux_alsa_backend(alsa, music_ring)
        }
        crate::config::LinuxAudioBackend::Jack => {
            #[cfg(has_jack_audio)]
            {
                let Some(jack) = jack else {
                    return Err("JACK backend hint unavailable.".to_string());
                };
                start_linux_jack_backend(jack, music_ring)
            }
            #[cfg(not(has_jack_audio))]
            {
                Err("JACK backend support was not built into this binary.".to_string())
            }
        }
        crate::config::LinuxAudioBackend::PipeWire => {
            #[cfg(has_pipewire_audio)]
            {
                let Some(pipewire) = pipewire else {
                    return Err("PipeWire backend hint unavailable.".to_string());
                };
                return start_linux_pipewire_backend(pipewire, music_ring);
            }
            #[cfg(not(has_pipewire_audio))]
            {
                Err("PipeWire backend support was not built into this binary.".to_string())
            }
        }
        crate::config::LinuxAudioBackend::PulseAudio => {
            #[cfg(has_pulse_audio)]
            {
                let Some(pulse) = pulse else {
                    return Err("PulseAudio backend hint unavailable.".to_string());
                };
                start_linux_pulse_backend(pulse, music_ring)
            }
            #[cfg(not(has_pulse_audio))]
            {
                return Err(
                    "PulseAudio backend support was not built into this binary.".to_string()
                );
            }
        }
        crate::config::LinuxAudioBackend::Auto => {
            if matches!(
                requested_output_mode,
                crate::config::AudioOutputMode::Exclusive
            ) {
                let Some(alsa) = alsa else {
                    return Err(
                        "Linux ALSA backend hint unavailable for exclusive output.".to_string()
                    );
                };
                return start_linux_alsa_backend(alsa, music_ring);
            }
            if explicit_device_requested {
                let Some(alsa) = alsa else {
                    return Err(
                        "Linux ALSA backend hint unavailable for the selected Sound Device."
                            .to_string(),
                    );
                };
                return start_linux_alsa_backend(alsa, music_ring).map_err(|err| {
                    format!(
                        "failed to start native ALSA output for the selected Sound Device: {err}"
                    )
                });
            }
            #[cfg(has_pipewire_audio)]
            if let Some(pipewire) = pipewire {
                match start_linux_pipewire_backend(pipewire, music_ring.clone()) {
                    Ok(output) => return Ok(output),
                    Err(err) => {
                        warn!(
                            "Failed to start native PipeWire output: {err}. Falling back to PulseAudio/ALSA."
                        );
                    }
                }
            }
            #[cfg(has_pulse_audio)]
            if backends::linux_pulse::is_available()
                && let Some(pulse) = pulse
            {
                match start_linux_pulse_backend(pulse, music_ring.clone()) {
                    Ok(output) => return Ok(output),
                    Err(err) => {
                        warn!(
                            "Failed to start native PulseAudio output: {err}. Falling back to ALSA/JACK."
                        );
                    }
                }
            }
            if let Some(alsa) = alsa {
                match start_linux_alsa_backend(alsa, music_ring.clone()) {
                    Ok(output) => return Ok(output),
                    Err(err) => {
                        #[cfg(has_jack_audio)]
                        if backends::linux_jack::is_available()
                            && let Some(jack) = jack
                        {
                            match start_linux_jack_backend(jack, music_ring) {
                                Ok(output) => return Ok(output),
                                Err(jack_err) => {
                                    return Err(format!(
                                        "failed to start native ALSA output: {err}; JACK fallback also failed: {jack_err}"
                                    ));
                                }
                            }
                        }
                        return Err(format!("failed to start native ALSA output: {err}"));
                    }
                }
            }
            Err("no native Linux audio backend hint is available.".to_string())
        }
    }
    #[cfg(target_os = "freebsd")]
    if let Some(pcm) = freebsd_pcm {
        return start_freebsd_pcm_backend(pcm.clone(), music_ring)
            .map_err(|err| format!("failed to start native FreeBSD PCM output: {err}"));
    }

    #[cfg(target_os = "macos")]
    if let Some(coreaudio) = coreaudio {
        return start_macos_coreaudio_backend(coreaudio.clone(), music_ring).map_err(|err| {
            format!(
                "failed to start native CoreAudio output for '{}': {err}",
                coreaudio.device_name
            )
        });
    }

    #[cfg(windows)]
    if let Some(wasapi) = wasapi {
        let access_mode = match wasapi.output_mode {
            crate::config::AudioOutputMode::Exclusive => {
                backends::windows_wasapi::WasapiAccessMode::Exclusive
            }
            crate::config::AudioOutputMode::Auto | crate::config::AudioOutputMode::Shared => {
                backends::windows_wasapi::WasapiAccessMode::Shared
            }
        };
        let prep = backends::windows_wasapi::prepare(
            wasapi.device_id.clone(),
            wasapi.device_name.clone(),
            wasapi.requested_rate_hz,
            access_mode,
        )
        .map_err(|err| {
            format!(
                "failed to prepare native WASAPI output for '{}': {err}",
                wasapi.device_name
            )
        })?;
        let mut ready = prep.ready();
        ready.requested_output_mode = wasapi.output_mode;
        let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
        let stream =
            backends::windows_wasapi::start(prep, music_ring, sfx_receiver).map_err(|err| {
                format!(
                    "failed to start native WASAPI output for '{}': {err}",
                    wasapi.device_name
                )
            })?;
        return Ok((OutputBackend::Wasapi(stream), ready, sfx_sender));
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("no native audio backend hint is available on this platform build.".to_string())
    }
}

/// Manager thread: builds the output backend, mixes SFX, and forwards music via ring.
fn audio_manager_thread(
    command_receiver: Receiver<AudioCommand>,
    ready_sender: Sender<Result<OutputBackendReady, String>>,
    launch: AudioThreadLaunch,
) {
    let mut music_stream: Option<MusicStream> = None;
    let music_ring = internal::ring_new(internal::RING_CAP_SAMPLES);
    let (mut _backend, _ready, sfx_sender) = match start_output_backend(launch, music_ring.clone())
    {
        Ok(output) => output,
        Err(err) => {
            let _ = ready_sender.send(Err(err));
            return;
        }
    };
    if ready_sender.send(Ok(_ready)).is_err() {
        return;
    }

    // Command loop: manage music decoder thread and pass SFX to the callback
    loop {
        match command_receiver.recv() {
            Ok(AudioCommand::PlaySfx(queued)) => {
                let _ = sfx_sender.send(queued);
            }
            Ok(AudioCommand::PlayMusic(path, cut, looping, rate)) => {
                if let Some(old) = music_stream.take() {
                    old.stop_signal
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = old.thread.join();
                }
                internal::ring_clear(&music_ring);
                MUSIC_TRACK_ACTIVE.store(true, Ordering::Relaxed);
                MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Relaxed);
                let rate_bits = Arc::new(AtomicU32::new(rate.to_bits()));
                music_stream = Some(resample::spawn_music_decoder_thread(
                    path,
                    cut,
                    looping,
                    rate_bits,
                    music_ring.clone(),
                ));
            }
            Ok(AudioCommand::StopMusic) => {
                if let Some(old) = music_stream.take() {
                    old.stop_signal
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = old.thread.join();
                }
                internal::ring_clear(&music_ring);
                MUSIC_TRACK_ACTIVE.store(false, Ordering::Relaxed);
                MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Relaxed);
            }
            Ok(AudioCommand::SetMusicRate(new_rate)) => {
                if let Some(ms) = &music_stream {
                    ms.rate_bits.store(new_rate.to_bits(), Ordering::Relaxed);
                }
                // Drop buffered old-rate samples so the change is heard immediately.
                internal::ring_clear(&music_ring);
                clear_music_pos_map();
            }
            Err(_) => break,
        }
    }

    if let Some(old) = music_stream.take() {
        old.stop_signal
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = old.thread.join();
    }
}

/* =========================== Internal primitives =========================== */

mod internal {
    use super::{Arc, MusicMapSeg};
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Pre-roll input frames and ring capacity
    pub const PREROLL_IN_FRAMES: u64 = 8;
    pub const RING_CAP_SAMPLES: usize = 1 << 16; // interleaved i16 samples (smaller = snappier)
    pub const MUSIC_SEG_RING_CAP: usize = 1 << 11;

    /* ----------------------------- SPSC ring ----------------------------- */

    pub struct SpscRingI16 {
        buf: UnsafeCell<Box<[i16]>>,
        mask: usize,
        head: AtomicUsize,
        tail: AtomicUsize,
    }
    // SAFETY: the ring is intentionally single-producer/single-consumer. Interior
    // mutability is synchronized by the `head`/`tail` atomics, and callers only
    // access the buffer through the ring API.
    unsafe impl Send for SpscRingI16 {}
    // SAFETY: shared references are safe because producer and consumer operate on
    // disjoint logical regions and publish ownership with atomic ordering.
    unsafe impl Sync for SpscRingI16 {}

    pub fn ring_new(cap_pow2: usize) -> Arc<SpscRingI16> {
        assert!(cap_pow2.is_power_of_two());
        Arc::new(SpscRingI16 {
            buf: UnsafeCell::new(vec![0i16; cap_pow2].into_boxed_slice()),
            mask: cap_pow2 - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }

    #[inline(always)]
    fn ring_cap(r: &SpscRingI16) -> usize {
        // SAFETY: the boxed slice is allocated once at construction time and never
        // moved out of `buf`; taking a shared view to read its length is safe.
        unsafe { (&*r.buf.get()).len() }
    }

    #[inline(always)]
    pub fn ring_free_samples(r: &SpscRingI16) -> usize {
        let cap = ring_cap(r);
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        cap.saturating_sub(h.wrapping_sub(t))
    }

    pub fn ring_push(r: &SpscRingI16, data: &[i16]) -> usize {
        let cap = ring_cap(r);
        let mask = r.mask;
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        let free = cap - h.wrapping_sub(t);
        let n = data.len().min(free);
        if n == 0 {
            return 0;
        }
        let idx = h & mask;
        // SAFETY: this is the single producer. The free-space check above ensures
        // the consumer cannot be reading the slots being written, and publication
        // happens only after the copies complete via the Release store to `head`.
        unsafe {
            let buf = &mut *r.buf.get();
            let first = (cap - idx).min(n);
            buf[idx..idx + first].copy_from_slice(&data[..first]);
            if n > first {
                buf[0..(n - first)].copy_from_slice(&data[first..n]);
            }
        }
        r.head.store(h.wrapping_add(n), Ordering::Release);
        n
    }

    pub fn ring_pop(r: &SpscRingI16, out: &mut [i16]) -> usize {
        let cap = ring_cap(r);
        let mask = r.mask;
        let h = r.head.load(Ordering::Acquire);
        let t = r.tail.load(Ordering::Relaxed);
        let avail = h.wrapping_sub(t);
        let n = out.len().min(avail);
        if n == 0 {
            return 0;
        }
        let idx = t & mask;
        // SAFETY: this is the single consumer. The Acquire load of `head`
        // guarantees the producer finished writing the visible region before we
        // copy from it, and these slots are not mutated again until `tail` advances.
        unsafe {
            let buf = &*r.buf.get();
            let first = (cap - idx).min(n);
            out[..first].copy_from_slice(&buf[idx..idx + first]);
            if n > first {
                out[first..n].copy_from_slice(&buf[0..(n - first)]);
            }
        }
        r.tail.store(t.wrapping_add(n), Ordering::Release);
        n
    }

    pub fn ring_clear(r: &SpscRingI16) {
        // This is called from the manager thread when the producer (decoder) is stopped.
        // It makes the buffer appear empty to the consumer (audio callback).
        let tail_pos = r.tail.load(Ordering::Relaxed);
        r.head.store(tail_pos, Ordering::Release);
    }

    /// Fill `dst` from the ring buffer, returning the number of interleaved
    /// samples actually popped from the ring. Any remaining slots are zeroed.
    pub fn callback_fill_from_ring_i16(ring: &SpscRingI16, dst: &mut [i16]) -> usize {
        let mut filled = 0;
        while filled < dst.len() {
            let got = ring_pop(ring, &mut dst[filled..]);
            if got == 0 {
                // underrun: zero the rest
                for d in &mut dst[filled..] {
                    *d = 0;
                }
                break;
            }
            filled += got;
        }
        filled
    }

    pub struct SpscRingMusicSeg {
        buf: UnsafeCell<Box<[MusicMapSeg]>>,
        mask: usize,
        head: AtomicUsize,
        tail: AtomicUsize,
    }
    // SAFETY: this ring follows the same SPSC discipline as `SpscRingI16`; the
    // only interior mutation is coordinated through the atomic indices.
    unsafe impl Send for SpscRingMusicSeg {}
    // SAFETY: shared references are safe because producer and consumer operate on
    // disjoint logical regions and publish ownership with atomic ordering.
    unsafe impl Sync for SpscRingMusicSeg {}

    pub fn music_seg_ring_new(cap_pow2: usize) -> Arc<SpscRingMusicSeg> {
        assert!(cap_pow2.is_power_of_two());
        Arc::new(SpscRingMusicSeg {
            buf: UnsafeCell::new(vec![MusicMapSeg::default(); cap_pow2].into_boxed_slice()),
            mask: cap_pow2 - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }

    #[inline(always)]
    fn music_seg_ring_cap(r: &SpscRingMusicSeg) -> usize {
        // SAFETY: the boxed slice is allocated once at construction time and never
        // moved out of `buf`; taking a shared view to read its length is safe.
        unsafe { (&*r.buf.get()).len() }
    }

    #[inline(always)]
    pub fn music_seg_ring_has_space(r: &SpscRingMusicSeg) -> bool {
        let cap = music_seg_ring_cap(r);
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        h.wrapping_sub(t) < cap
    }

    pub fn music_seg_ring_push(r: &SpscRingMusicSeg, seg: MusicMapSeg) -> bool {
        let cap = music_seg_ring_cap(r);
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        if h.wrapping_sub(t) >= cap {
            return false;
        }
        let idx = h & r.mask;
        // SAFETY: this is the single producer. The capacity check guarantees the
        // consumer is not reading this slot, and the Release store to `head`
        // publishes the initialized segment only after the write completes.
        unsafe {
            (&mut *r.buf.get())[idx] = seg;
        }
        r.head.store(h.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn music_seg_ring_pop(r: &SpscRingMusicSeg) -> Option<MusicMapSeg> {
        let h = r.head.load(Ordering::Acquire);
        let t = r.tail.load(Ordering::Relaxed);
        if h == t {
            return None;
        }
        let idx = t & r.mask;
        // SAFETY: this is the single consumer. The Acquire load of `head`
        // guarantees the producer has finished writing this slot before we copy
        // the `MusicMapSeg` value out of it.
        let seg = unsafe { (&*r.buf.get())[idx] };
        r.tail.store(t.wrapping_add(1), Ordering::Release);
        Some(seg)
    }

    pub fn music_seg_ring_clear(r: &SpscRingMusicSeg) {
        let tail_pos = r.tail.load(Ordering::Relaxed);
        r.head.store(tail_pos, Ordering::Release);
    }
}
