mod backends;
pub mod folder;
pub mod replaygain;
mod resample;

#[cfg(target_os = "linux")]
use deadsync_audio::LinuxAudioBackend;
#[cfg(unix)]
use deadsync_audio::OutputTimingQuality;
pub(crate) use deadsync_audio::ring as internal;
use deadsync_audio::{
    ActiveSfx, AudioOutputMode, MAX_ACTIVE_SFX, MusicMapSeg, OutputBackendReady, PlaybackPosMap,
    QueuedSfx, SfxLane, StutterDiagAudioEvent, StutterDiagAudioEventKind, f32_to_i16,
    fallback_music_position, i16_to_f32, mix_active_sfx, mix_level_gains, music_clock_seed_enabled,
    music_nanos_from_seconds, normalized_music_rate, push_queued_sfx,
};
pub use deadsync_audio::{
    Cut, InitConfig, MusicStreamClockSnapshot, OutputDeviceInfo, OutputTimingSnapshot,
};
use deadsync_audio_decode as decode;
use deadsync_platform::dirs;
use deadsync_platform::host_time::{instant_nanos, now_nanos};
#[cfg(windows)]
use deadsync_platform::windows_rt::current_qpc_nanos;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

const SFX_QUEUE_CAP: usize = 128;
const ASSIST_TICK_SFX_PATH: &str = "assets/sounds/assist_tick.ogg";
/* ============================== Public API ============================== */

struct OutputDeviceProbe {
    info: OutputDeviceInfo,
    #[cfg(target_os = "freebsd")]
    freebsd_dsp_path: Option<String>,
}

// Commands to the audio engine
enum AudioCommand {
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
static ASSIST_TICK_SFX: OnceLock<Arc<[i16]>> = OnceLock::new();

struct AudioEngine {
    command_sender: Sender<AudioCommand>,
    sfx_sender: SyncSender<QueuedSfx>,
    sfx_cache: Mutex<HashMap<String, Arc<[i16]>>>,
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
    output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct AlsaBackendHint {
    pcm_id: Option<String>,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_jack_audio)]
#[derive(Clone)]
struct JackBackendHint {
    requested_device_name: Option<String>,
    requested_rate_hz: Option<u32>,
    output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_pipewire_audio)]
#[derive(Clone)]
struct PipeWireBackendHint {
    requested_device_name: Option<String>,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: AudioOutputMode,
}

#[cfg(target_os = "linux")]
#[cfg(has_pulse_audio)]
#[derive(Clone)]
struct PulseBackendHint {
    requested_device_name: Option<String>,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: AudioOutputMode,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct CoreAudioBackendHint {
    device_uid: Option<String>,
    device_name: String,
    requested_rate_hz: Option<u32>,
    channels: usize,
    output_mode: AudioOutputMode,
}

#[cfg(target_os = "freebsd")]
#[derive(Clone)]
struct FreeBsdPcmBackendHint {
    dsp_path: Option<String>,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    output_mode: AudioOutputMode,
}

#[derive(Clone)]
struct AudioThreadLaunch {
    #[cfg(target_os = "linux")]
    explicit_device_requested: bool,
    #[cfg(target_os = "linux")]
    linux_backend: LinuxAudioBackend,
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

struct AudioThreadReady {
    backend_ready: OutputBackendReady,
    sfx_sender: SyncSender<QueuedSfx>,
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
// Song-time fallback for the current/pending music stream. The precise
// packet map is published by the audio callback, but gameplay can query the
// clock before the first mapped packet has been consumed.
static MUSIC_CLOCK_SEEDED: AtomicBool = AtomicBool::new(false);
static MUSIC_CLOCK_CUT_START_BITS: AtomicU64 = AtomicU64::new(0.0f64.to_bits());
static MUSIC_CLOCK_RATE_BITS: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
// Per-play monotonic id used to associate asynchronous ReplayGain results
// with the track that requested them. `set_music_replaygain_if_matches` is
// a no-op if the id no longer matches the active track, preventing a stale
// gain value from being applied to a different song.
static MUSIC_TRACK_ID: AtomicU64 = AtomicU64::new(0);
// Target linear gain for the music stream. The mixer interpolates its
// own `current_gain` toward this target across ~80 ms (RAMP_FRAMES at the
// device sample rate) so cache-miss → cache-hit transitions don't produce
// an audible step. Default 1.0.
static MUSIC_TARGET_GAIN_BITS: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
// Generation counter incremented whenever a track boundary (play / stop)
// should snap the mixer's interpolated gain to its target instantly,
// rather than ramping across the boundary.
static MUSIC_GAIN_SNAP_GEN: AtomicU64 = AtomicU64::new(0);
static MUSIC_MAP_GEN: AtomicU64 = AtomicU64::new(1);
static SCREEN_SFX_STOP_GEN: AtomicU64 = AtomicU64::new(0);
// Generation counter for scheduled assist ticks. Bumped on every music stream
// reset (stop / seek / track change) so any in-flight scheduled tick whose
// target frame belongs to the previous timeline is dropped by the mixer rather
// than firing a stale clap.
static ASSIST_SFX_GEN: AtomicU64 = AtomicU64::new(0);

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
static AUDIO_TIMING_DIAG_LAST_SOURCE: AtomicU8 = AtomicU8::new(0);
static AUDIO_TIMING_DIAG_LAST_NANOS: AtomicU64 = AtomicU64::new(0);
static AUDIO_TIMING_DIAG_LAST_GAP_NS: AtomicU64 = AtomicU64::new(0);

const MAX_PACKET_START_SNAP_SEC: f64 = 0.25;

#[inline(always)]
fn seed_music_stream_clock(cut: Cut, rate: f32) {
    MUSIC_CLOCK_CUT_START_BITS.store(cut.start_sec.to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_RATE_BITS.store(normalized_music_rate(rate).to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_SEEDED.store(music_clock_seed_enabled(cut.start_sec), Ordering::Release);
}

#[inline(always)]
fn clear_music_stream_clock_seed() {
    MUSIC_CLOCK_CUT_START_BITS.store(0.0f64.to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_RATE_BITS.store(1.0f32.to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_SEEDED.store(false, Ordering::Release);
}

#[inline(always)]
fn seeded_music_position(stream_seconds: f32) -> Option<(f32, f32)> {
    if !MUSIC_CLOCK_SEEDED.load(Ordering::Acquire) {
        return None;
    }
    let cut_start_sec = f64::from_bits(MUSIC_CLOCK_CUT_START_BITS.load(Ordering::Relaxed));
    let rate = f32::from_bits(MUSIC_CLOCK_RATE_BITS.load(Ordering::Relaxed));
    Some(fallback_music_position(stream_seconds, cut_start_sec, rate))
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
    log::log_enabled!(log::Level::Debug)
}

#[inline(always)]
pub(crate) fn timing_diag_last_callback_gap_ns() -> u64 {
    AUDIO_TIMING_DIAG_LAST_GAP_NS.load(Ordering::Relaxed)
}

#[inline(always)]
fn stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

pub fn stutter_diag_trigger_seq() -> u64 {
    deadsync_audio::stutter_diag_trigger_seq()
}

pub fn collect_stutter_diag_events(
    now_host_nanos: u64,
    window_ns: u64,
    out: &mut Vec<StutterDiagAudioEvent>,
) {
    deadsync_audio::collect_stutter_diag_events(now_host_nanos, window_ns, out);
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
        if stutter_diag && gap_ns >= deadsync_audio::stutter_diag_callback_gap_threshold_ns() {
            deadsync_audio::record_stutter_diag_event(
                StutterDiagAudioEventKind::CallbackGap,
                now_nanos(),
                gap_ns,
                deadsync_audio::current_output_timing_quality(),
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
pub fn available_linux_backends() -> Vec<LinuxAudioBackend> {
    let mut backends = Vec::with_capacity(5);
    backends.push(LinuxAudioBackend::Auto);
    #[cfg(has_pipewire_audio)]
    backends.push(LinuxAudioBackend::PipeWire);
    #[cfg(has_pulse_audio)]
    if backends::linux_pulse::is_available() {
        backends.push(LinuxAudioBackend::PulseAudio);
    }
    backends.push(LinuxAudioBackend::Alsa);
    #[cfg(has_jack_audio)]
    if backends::linux_jack::is_available() {
        backends.push(LinuxAudioBackend::Jack);
    }
    backends
}

/// Plays a sound effect from the given path (cached after first load).
pub fn play_sfx(path: &str) {
    play_sfx_on_lane(path, SfxLane::Effect);
}

/// Plays a screen-owned sound effect that can be stopped on screen exit.
pub fn play_screen_sfx(path: &str) {
    play_sfx_on_lane(path, SfxLane::Screen);
}

/// Plays a sound effect only if it was already preloaded.
pub fn play_preloaded_sfx(path: &str) {
    play_preloaded_sfx_on_lane(path, SfxLane::Effect);
}

/// Stops active and queued screen-owned sound effects.
pub fn stop_screen_sfx() {
    SCREEN_SFX_STOP_GEN.fetch_add(1, Ordering::AcqRel);
}

/// Plays a gameplay assist tick that uses its own volume lane.
pub fn play_assist_tick(path: &str) {
    if path == ASSIST_TICK_SFX_PATH
        && let Some(sound_data) = ASSIST_TICK_SFX.get().cloned()
    {
        let _ = ENGINE.sfx_sender.try_send(QueuedSfx {
            data: sound_data,
            lane: SfxLane::AssistTick,
            stop_generation: sfx_stop_generation(SfxLane::AssistTick),
            target_stream_frame: 0,
        });
        return;
    }
    play_sfx_on_lane(path, SfxLane::AssistTick);
}

/// Plays a preloaded gameplay assist tick without decoding on miss.
pub fn play_preloaded_assist_tick(path: &str) {
    if path == ASSIST_TICK_SFX_PATH
        && let Some(sound_data) = ASSIST_TICK_SFX.get().cloned()
    {
        let _ = ENGINE.sfx_sender.try_send(QueuedSfx {
            data: sound_data,
            lane: SfxLane::AssistTick,
            stop_generation: sfx_stop_generation(SfxLane::AssistTick),
            target_stream_frame: 0,
        });
        return;
    }
    play_preloaded_sfx_on_lane(path, SfxLane::AssistTick);
}

/// Plays a preloaded gameplay assist tick scheduled to become audible at an
/// absolute stream frame (see [`assist_tick_stream_frame_for_music_seconds`]).
/// Because the target frame lies on the same `MUSIC_TOTAL_FRAMES` timeline the
/// mixer writes against, output latency is compensated implicitly. Falls back to
/// immediate playback when `target_stream_frame == 0`.
pub fn play_scheduled_assist_tick(path: &str, target_stream_frame: u64) {
    if target_stream_frame == 0 {
        play_preloaded_assist_tick(path);
        return;
    }
    if path == ASSIST_TICK_SFX_PATH
        && let Some(sound_data) = ASSIST_TICK_SFX.get().cloned()
    {
        let _ = ENGINE.sfx_sender.try_send(QueuedSfx {
            data: sound_data,
            lane: SfxLane::AssistTick,
            stop_generation: sfx_stop_generation(SfxLane::AssistTick),
            target_stream_frame,
        });
        return;
    }
    // Cache-miss fallback: schedule whatever is cached for this path.
    let cached = { ENGINE.sfx_cache.lock().unwrap().get(path).cloned() };
    if let Some(sound_data) = cached {
        let _ = ENGINE.sfx_sender.try_send(QueuedSfx {
            data: sound_data,
            lane: SfxLane::AssistTick,
            stop_generation: sfx_stop_generation(SfxLane::AssistTick),
            target_stream_frame,
        });
    } else {
        warn!("Scheduled assist tick cache miss for '{path}'; skipping");
    }
}

#[inline(always)]
fn sfx_stop_generation(lane: SfxLane) -> u64 {
    match lane {
        SfxLane::Screen => SCREEN_SFX_STOP_GEN.load(Ordering::Acquire),
        SfxLane::AssistTick => ASSIST_SFX_GEN.load(Ordering::Acquire),
        SfxLane::Effect => 0,
    }
}

#[inline(always)]
fn sfx_is_stale(lane: SfxLane, stop_generation: u64) -> bool {
    match lane {
        SfxLane::Screen => stop_generation != SCREEN_SFX_STOP_GEN.load(Ordering::Acquire),
        SfxLane::AssistTick => stop_generation != ASSIST_SFX_GEN.load(Ordering::Acquire),
        SfxLane::Effect => false,
    }
}

fn play_cached_sfx_on_lane(path: &str, lane: SfxLane) -> bool {
    #[cfg(test)]
    if !is_initialized() {
        return true;
    }

    let cached = { ENGINE.sfx_cache.lock().unwrap().get(path).cloned() };
    if let Some(sound_data) = cached {
        let _ = ENGINE.sfx_sender.try_send(QueuedSfx {
            data: sound_data,
            lane,
            stop_generation: sfx_stop_generation(lane),
            target_stream_frame: 0,
        });
        return true;
    }
    false
}

fn play_preloaded_sfx_on_lane(path: &str, lane: SfxLane) {
    if !play_cached_sfx_on_lane(path, lane) {
        warn!("Preloaded SFX cache miss for '{path}'; skipping synchronous decode");
    }
}

fn play_sfx_on_lane(path: &str, lane: SfxLane) {
    if play_cached_sfx_on_lane(path, lane) {
        return;
    }

    let resolved = dirs::app_dirs().resolve_asset_path(path);
    let resolved_str = resolved.to_string_lossy();
    let decoded = match resample::load_and_resample_sfx(&resolved_str) {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to load SFX '{path}': {e}");
            return;
        }
    };

    let sound_data = {
        let mut cache = ENGINE.sfx_cache.lock().unwrap();
        cache
            .entry(path.to_string())
            .or_insert_with(|| {
                debug!("Cached SFX: {path}");
                decoded
            })
            .clone()
    };
    cache_assist_tick(path, sound_data.clone());
    let _ = ENGINE.sfx_sender.try_send(QueuedSfx {
        data: sound_data,
        lane,
        stop_generation: sfx_stop_generation(lane),
        target_stream_frame: 0,
    });
}

/// Preloads a sound effect into cache without playing it.
pub fn preload_sfx(path: &str) {
    let cached = { ENGINE.sfx_cache.lock().unwrap().get(path).cloned() };
    if let Some(data) = cached {
        cache_assist_tick(path, data);
        return;
    }

    let resolved = dirs::app_dirs().resolve_asset_path(path);
    let resolved_str = resolved.to_string_lossy();
    let decoded = match resample::load_and_resample_sfx(&resolved_str) {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to preload SFX '{path}': {e}");
            return;
        }
    };

    let mut cache = ENGINE.sfx_cache.lock().unwrap();
    let data = cache
        .entry(path.to_string())
        .or_insert_with(|| {
            debug!("Cached SFX: {path}");
            decoded
        })
        .clone();
    cache_assist_tick(path, data);
}

fn cache_assist_tick(path: &str, data: Arc<[i16]>) {
    if path == ASSIST_TICK_SFX_PATH {
        let _ = ASSIST_TICK_SFX.set(data);
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
    // Invalidate any assist ticks scheduled against the previous timeline; their
    // absolute target frames no longer correspond to the music position.
    ASSIST_SFX_GEN.fetch_add(1, Ordering::AcqRel);
    clear_music_stream_clock_seed();
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

#[cfg(test)]
mod tests {
    use super::{CallbackClockWindow, stream_position_frames_from_window};

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

fn lookup_music_position(stream_frames: f64, sample_rate: u32) -> Option<(f32, f32)> {
    let mut map = PLAYBACK_POS_MAP.lock().unwrap();
    while let Some(seg) = internal::music_seg_ring_pop(&PLAYED_MUSIC_MAP_SEGS) {
        map.insert(seg);
    }
    map.search(stream_frames).map(|(music_sec, sec_per_frame)| {
        (
            music_sec as f32,
            (sec_per_frame * sample_rate as f64) as f32,
        )
    })
}

/// Maps a music position (seconds, pre-global-offset stream time) to the
/// absolute stream frame on the `MUSIC_TOTAL_FRAMES` timeline at which it will
/// be audible. Returns `None` when there is no usable mapping or the result is
/// implausible (caller should then fall back to immediate playback).
///
/// Runs on the gameplay thread; the lock is fine there since the audio callback
/// never takes it on its hot path.
pub fn assist_tick_stream_frame_for_music_seconds(music_seconds: f64) -> Option<u64> {
    if !music_seconds.is_finite() {
        return None;
    }
    let track_frame = {
        let mut map = PLAYBACK_POS_MAP.lock().unwrap();
        while let Some(seg) = internal::music_seg_ring_pop(&PLAYED_MUSIC_MAP_SEGS) {
            map.insert(seg);
        }
        map.invert(music_seconds)?
    };
    if !track_frame.is_finite() || track_frame < 0.0 {
        return None;
    }
    let start = MUSIC_TRACK_START_FRAME.load(Ordering::Acquire);
    Some(start.saturating_add(track_frame.round() as u64))
}

/// Plays a music track from a file path.
pub fn play_music(path: PathBuf, cut: Cut, looping: bool, rate: f32) {
    let rate = normalized_music_rate(rate);
    reset_music_stream_clock();
    seed_music_stream_clock(cut, rate);

    // Resolve a per-play track id and decide on the initial ReplayGain
    // value. If the user has the experimental ReplayGain setting on, and we
    // already have a cached/computed value, apply it immediately; otherwise
    // queue a background analysis job. If the setting is off, force unity
    // gain so a previous track's value doesn't leak forward.
    let track_id = MUSIC_TRACK_ID
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    let initial_gain: f32 = if crate::config::get().enable_replaygain {
        replaygain::get_or_queue_gain_linear(&path, track_id).unwrap_or(1.0)
    } else {
        1.0
    };
    MUSIC_TARGET_GAIN_BITS.store(initial_gain.to_bits(), Ordering::Relaxed);
    // Snap to the new target at the track boundary so the previous track's
    // gain doesn't audibly bleed into the start of this one.
    MUSIC_GAIN_SNAP_GEN.fetch_add(1, Ordering::Release);

    let _ = ENGINE
        .command_sender
        .send(AudioCommand::PlayMusic(path, cut, looping, rate));
}

pub fn snap_music_start_sec(path: &Path, start_sec: f64) -> f64 {
    let Ok(Some(snapped)) = decode::snap_start_forward_to_packet(path, start_sec) else {
        return start_sec;
    };
    if snapped < start_sec || snapped - start_sec > MAX_PACKET_START_SNAP_SEC {
        return start_sec;
    }
    if (snapped - start_sec).abs() > f64::EPSILON {
        debug!("Snapped music cut start from {start_sec:.6}s to packet boundary {snapped:.6}s.");
    }
    snapped
}

/// Applies a ReplayGain result from the background analyzer, but only if it
/// still corresponds to the currently active music track. Called by
/// [`replaygain`]; safe to call from any thread.
pub fn set_music_replaygain_if_matches(track_id: u64, gain_linear: f32) {
    let active_id = MUSIC_TRACK_ID.load(Ordering::Acquire);
    if active_id != track_id {
        return;
    }
    if !MUSIC_TRACK_ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let gain = if gain_linear.is_finite() && gain_linear > 0.0 {
        gain_linear
    } else {
        1.0
    };
    MUSIC_TARGET_GAIN_BITS.store(gain.to_bits(), Ordering::Relaxed);
}

/// Called from the config layer when the user toggles the ReplayGain
/// setting. When disabled, forces unity gain immediately so the change is
/// audible without requiring track restart.
pub fn on_replaygain_setting_changed(enabled: bool) {
    if !enabled {
        MUSIC_TARGET_GAIN_BITS.store(1.0f32.to_bits(), Ordering::Relaxed);
    }
}

/// Stops the currently playing music track.
pub fn stop_music() {
    reset_music_stream_clock();
    MUSIC_TARGET_GAIN_BITS.store(1.0f32.to_bits(), Ordering::Relaxed);
    MUSIC_GAIN_SNAP_GEN.fetch_add(1, Ordering::Release);
    let _ = ENGINE.command_sender.send(AudioCommand::StopMusic);
}

/// Adjusts the playback rate for the current music stream, if any.
pub fn set_music_rate(rate: f32) {
    let rate = normalized_music_rate(rate);
    MUSIC_CLOCK_RATE_BITS.store(rate.to_bits(), Ordering::Relaxed);
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

/// Current scheduled-assist-tick generation. Changes on every music stream reset
/// (stop / seek / track change). Gameplay reads this to detect that previously
/// scheduled ticks were invalidated and that its scheduling cursor must re-anchor.
pub fn assist_sfx_generation() -> u64 {
    ASSIST_SFX_GEN.load(Ordering::Acquire)
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
            None => match seeded_music_position(stream_seconds) {
                Some((music_seconds, slope)) => (music_seconds, slope, true),
                None => (stream_seconds, 1.0, false),
            },
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

/// Returns the current stream position and the `Instant` it is valid for.
pub fn get_music_stream_clock_snapshot() -> MusicStreamClockSnapshot {
    let sample_rate = ENGINE.device_sample_rate.max(1);
    let has_started = MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire);
    if !has_started {
        if let Some((music_seconds, slope)) = seeded_music_position(0.0) {
            return MusicStreamClockSnapshot {
                stream_seconds: 0.0,
                music_seconds,
                music_nanos: music_nanos_from_seconds(f64::from(music_seconds)),
                music_seconds_per_second: slope,
                has_music_mapping: true,
                valid_at: Instant::now(),
                valid_at_host_nanos: 0,
            };
        }
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

pub fn get_output_timing_snapshot() -> OutputTimingSnapshot {
    deadsync_audio::get_output_timing_snapshot()
}

/* ============================ Engine internals ============================ */

#[inline(always)]
fn publish_output_backend_ready(ready: OutputBackendReady) {
    deadsync_audio::publish_output_backend_ready(ready);
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
    deadsync_audio::publish_output_timing(
        sample_rate_hz,
        device_period_ns,
        stream_latency_ns,
        buffer_frames,
        padding_frames,
        queued_frames,
        estimated_output_delay_ns,
    );
}

#[inline(always)]
pub(crate) fn note_output_underrun() {
    deadsync_audio::note_output_underrun(now_nanos(), stutter_diag_enabled());
}

#[inline(always)]
#[cfg(unix)]
pub(crate) fn publish_output_timing_quality(quality: OutputTimingQuality) {
    deadsync_audio::publish_output_timing_quality(quality);
}

#[inline(always)]
#[cfg(unix)]
pub(crate) fn note_output_timing_sanity_failure(quality: OutputTimingQuality) {
    deadsync_audio::note_output_timing_sanity_failure(quality, now_nanos(), stutter_diag_enabled());
}

#[inline(always)]
#[cfg(unix)]
pub(crate) fn note_output_clock_fallback() {
    deadsync_audio::note_output_clock_fallback(now_nanos(), stutter_diag_enabled());
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
    active_sfx: Vec<ActiveSfx>,
    queued_music_map: Arc<internal::SpscRingMusicSeg>,
    played_music_map: Arc<internal::SpscRingMusicSeg>,
    active_music_map: Option<MusicMapSeg>,
    music_map_generation: u64,
    /// Current music gain as seen by the mixer. Ramps toward
    /// `MUSIC_TARGET_GAIN_BITS` over [`MUSIC_GAIN_RAMP_FRAMES`] frames so
    /// asynchronous ReplayGain results don't produce an audible step.
    music_gain_current: f32,
    /// Generation of `MUSIC_GAIN_SNAP_GEN` last observed; when it changes
    /// (e.g. on a track boundary), the mixer snaps `music_gain_current`
    /// straight to the target instead of ramping.
    music_gain_snap_seen: u64,
}

/// Number of device frames over which the music gain ramps when the
/// target changes. 4000 frames ≈ 83 ms at 48 kHz and ≈ 91 ms at 44.1 kHz —
/// fast enough to feel instantaneous, slow enough to eliminate the
/// click/step that an atomic gain swap would produce.
const MUSIC_GAIN_RAMP_FRAMES: f32 = 4000.0;

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
            active_sfx: Vec::with_capacity(MAX_ACTIVE_SFX),
            queued_music_map: QUEUED_MUSIC_MAP_SEGS.clone(),
            played_music_map: PLAYED_MUSIC_MAP_SEGS.clone(),
            active_music_map: None,
            music_map_generation: MUSIC_MAP_GEN.load(Ordering::Acquire),
            music_gain_current: f32::from_bits(MUSIC_TARGET_GAIN_BITS.load(Ordering::Relaxed)),
            music_gain_snap_seen: MUSIC_GAIN_SNAP_GEN.load(Ordering::Acquire),
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
        mix_level_gains(crate::config::audio_mix_levels())
    }

    fn mix_f32_buffer(&mut self, total_before: u64, len: usize) -> (usize, bool) {
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
        let target_gain = f32::from_bits(MUSIC_TARGET_GAIN_BITS.load(Ordering::Relaxed));
        let snap_gen = MUSIC_GAIN_SNAP_GEN.load(Ordering::Acquire);
        if snap_gen != self.music_gain_snap_seen {
            self.music_gain_current = target_gain;
            self.music_gain_snap_seen = snap_gen;
        }
        let device_channels = self.device_channels.max(1);
        let frames_in_buf = len / device_channels;
        let max_step = 1.0 / MUSIC_GAIN_RAMP_FRAMES;
        for f in 0..frames_in_buf {
            let diff = target_gain - self.music_gain_current;
            if diff.abs() <= max_step {
                self.music_gain_current = target_gain;
            } else {
                self.music_gain_current += diff.signum() * max_step;
            }
            let scale = music_vol * self.music_gain_current;
            let base = f * device_channels;
            for ch in 0..device_channels {
                let idx = base + ch;
                self.mix_f32[idx] = i16_to_f32(self.mix_i16[idx]) * scale;
            }
        }
        // Zero any tail that doesn't divide evenly into whole frames; the
        // mixer downstream expects exactly `len` valid f32 samples.
        for idx in frames_in_buf * device_channels..len {
            self.mix_f32[idx] = 0.0;
        }

        for new_sfx in self.sfx_receiver.try_iter() {
            push_queued_sfx(&mut self.active_sfx, new_sfx, sfx_is_stale);
        }

        let mixed_sfx = mix_active_sfx(
            &mut self.active_sfx,
            &mut self.mix_f32,
            total_before,
            device_channels,
            sfx_vol,
            assist_tick_vol,
            sfx_is_stale,
        );

        (popped, mixed_sfx)
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
        let (popped, _) = self.mix_f32_buffer(total_before, out.len());
        for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
            *dst = f32_to_i16(*src);
        }
        self.finish_callback(total_before, out.len(), popped);
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn render_i16_host_nanos(&mut self, out: &mut [i16], anchor_nanos: u64) {
        let total_before = self.begin_callback_nanos(anchor_nanos, CallbackClockSource::Instant);
        let (popped, _) = self.mix_f32_buffer(total_before, out.len());
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
        let (popped, mixed_sfx) = self.mix_f32_buffer(total_before, out.len());
        if mixed_sfx {
            for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
                *dst = src.clamp(-1.0, 1.0);
            }
        } else {
            out.copy_from_slice(&self.mix_f32[..out.len()]);
        }
        self.finish_callback(total_before, out.len(), popped);
    }

    #[cfg(windows)]
    fn render_f32_qpc(&mut self, out: &mut [f32], anchor_nanos: u64) {
        let total_before = self.begin_callback_qpc(anchor_nanos);
        let (popped, mixed_sfx) = self.mix_f32_buffer(total_before, out.len());
        if mixed_sfx {
            for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
                *dst = src.clamp(-1.0, 1.0);
            }
        } else {
            out.copy_from_slice(&self.mix_f32[..out.len()]);
        }
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
        (matches!(output_mode, AudioOutputMode::Exclusive)
            && matches!(
                linux_backend,
                LinuxAudioBackend::Auto | LinuxAudioBackend::Alsa
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

    let thread_ready = match ready_receiver.recv() {
        Ok(Ok(ready)) => ready,
        Ok(Err(err)) => panic!("failed to initialize audio engine: {err}"),
        Err(_) => panic!("audio manager thread exited before reporting ready"),
    };
    let AudioThreadReady {
        backend_ready: ready,
        sfx_sender,
    } = thread_ready;

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
        sfx_sender,
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
) -> Result<(OutputBackend, OutputBackendReady, SyncSender<QueuedSfx>), String> {
    let access_mode = match alsa.output_mode {
        AudioOutputMode::Exclusive => backends::linux_alsa::AlsaAccessMode::Exclusive,
        AudioOutputMode::Auto | AudioOutputMode::Shared => {
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
    let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
    let stream = backends::linux_alsa::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::Alsa(stream), ready, sfx_sender))
}

#[cfg(target_os = "linux")]
#[cfg(has_jack_audio)]
fn start_linux_jack_backend(
    jack: JackBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, SyncSender<QueuedSfx>), String> {
    if matches!(jack.output_mode, AudioOutputMode::Exclusive) {
        return Err("JACK does not expose a separate exclusive output mode.".to_string());
    }
    let prep =
        backends::linux_jack::prepare(jack.requested_device_name.clone(), jack.requested_rate_hz)?;
    let mut ready = prep.ready();
    ready.requested_output_mode = jack.output_mode;
    let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
    let stream = backends::linux_jack::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::Jack(stream), ready, sfx_sender))
}

#[cfg(target_os = "linux")]
#[cfg(has_pipewire_audio)]
fn start_linux_pipewire_backend(
    pipewire: PipeWireBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, SyncSender<QueuedSfx>), String> {
    if matches!(pipewire.output_mode, AudioOutputMode::Exclusive) {
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
    let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
    let stream = backends::linux_pipewire::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::PipeWire(stream), ready, sfx_sender))
}

#[cfg(target_os = "linux")]
#[cfg(has_pulse_audio)]
fn start_linux_pulse_backend(
    pulse: PulseBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, SyncSender<QueuedSfx>), String> {
    if matches!(pulse.output_mode, AudioOutputMode::Exclusive) {
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
    let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
    let stream = backends::linux_pulse::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::Pulse(stream), ready, sfx_sender))
}

#[cfg(target_os = "freebsd")]
fn start_freebsd_pcm_backend(
    pcm: FreeBsdPcmBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, SyncSender<QueuedSfx>), String> {
    if matches!(pcm.output_mode, AudioOutputMode::Exclusive) {
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
    let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
    let stream = backends::freebsd_pcm::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::FreeBsdPcm(stream), ready, sfx_sender))
}

#[cfg(target_os = "macos")]
fn start_macos_coreaudio_backend(
    coreaudio: CoreAudioBackendHint,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, SyncSender<QueuedSfx>), String> {
    if matches!(coreaudio.output_mode, AudioOutputMode::Exclusive) {
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
    let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
    let stream = backends::macos_coreaudio::start(prep, music_ring, sfx_receiver)?;
    Ok((OutputBackend::CoreAudio(stream), ready, sfx_sender))
}

fn start_output_backend(
    launch: AudioThreadLaunch,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, SyncSender<QueuedSfx>), String> {
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
        .unwrap_or(AudioOutputMode::Auto);
    #[cfg(target_os = "linux")]
    match linux_backend {
        LinuxAudioBackend::Alsa => {
            let Some(alsa) = alsa else {
                return Err("Linux ALSA backend hint unavailable.".to_string());
            };
            start_linux_alsa_backend(alsa, music_ring)
        }
        LinuxAudioBackend::Jack => {
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
        LinuxAudioBackend::PipeWire => {
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
        LinuxAudioBackend::PulseAudio => {
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
        LinuxAudioBackend::Auto => {
            if matches!(requested_output_mode, AudioOutputMode::Exclusive) {
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
            AudioOutputMode::Exclusive => backends::windows_wasapi::WasapiAccessMode::Exclusive,
            AudioOutputMode::Auto | AudioOutputMode::Shared => {
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
        let (sfx_sender, sfx_receiver) = sync_channel::<QueuedSfx>(SFX_QUEUE_CAP);
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

/// Manager thread: builds the output backend and manages music decoder lifecycle.
fn audio_manager_thread(
    command_receiver: Receiver<AudioCommand>,
    ready_sender: Sender<Result<AudioThreadReady, String>>,
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
    if ready_sender
        .send(Ok(AudioThreadReady {
            backend_ready: _ready,
            sfx_sender,
        }))
        .is_err()
    {
        return;
    }

    // Command loop: manage music decoder thread.
    loop {
        match command_receiver.recv() {
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
