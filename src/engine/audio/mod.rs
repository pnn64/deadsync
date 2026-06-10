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
    AudioOutputMode, AudioRenderCallbackResult, AudioRenderMaps, CallbackClockSource,
    CallbackClockWindow, MusicMapSeg, OutputBackendReady, PlaybackPosMap, QueuedSfx, SfxLane,
    StutterDiagAudioEvent, StutterDiagAudioEventKind, fallback_stream_position_frames,
    music_nanos_from_seconds, normalized_music_rate, sfx_stop_generation,
    stream_position_frames_from_window as audio_stream_position_frames_from_window,
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
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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

const MAX_PACKET_START_SNAP_SEC: f64 = 0.25;

#[inline(always)]
pub(crate) fn timing_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Debug)
}

#[inline(always)]
pub(crate) fn timing_diag_last_callback_gap_ns() -> u64 {
    deadsync_audio::timing_diag_last_callback_gap_ns()
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

static QUEUED_MUSIC_MAP_SEGS: std::sync::LazyLock<Arc<internal::SpscRingMusicSeg>> =
    std::sync::LazyLock::new(|| internal::music_seg_ring_new(internal::MUSIC_SEG_RING_CAP));
static PLAYED_MUSIC_MAP_SEGS: std::sync::LazyLock<Arc<internal::SpscRingMusicSeg>> =
    std::sync::LazyLock::new(|| internal::music_seg_ring_new(internal::MUSIC_SEG_RING_CAP));
static PLAYBACK_POS_MAP: std::sync::LazyLock<Mutex<PlaybackPosMap>> =
    std::sync::LazyLock::new(|| Mutex::new(PlaybackPosMap::default()));

#[inline(always)]
fn audio_render_maps() -> AudioRenderMaps {
    AudioRenderMaps::new(QUEUED_MUSIC_MAP_SEGS.clone(), PLAYED_MUSIC_MAP_SEGS.clone())
}

#[inline(always)]
pub(crate) fn report_audio_render_callback(result: AudioRenderCallbackResult) {
    if result.callback_gap_ns != 0
        && stutter_diag_enabled()
        && result.callback_gap_ns >= deadsync_audio::stutter_diag_callback_gap_threshold_ns()
    {
        deadsync_audio::record_stutter_diag_event(
            StutterDiagAudioEventKind::CallbackGap,
            now_nanos(),
            result.callback_gap_ns,
            deadsync_audio::current_output_timing_quality(),
        );
    }
    if result.output_underrun {
        note_output_underrun();
    }
}

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
    deadsync_audio::bump_screen_sfx_generation();
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
/// Because the target frame lies on the same audio stream timeline the mixer
/// writes against, output latency is compensated implicitly. Falls back to
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
    deadsync_audio::bump_music_map_generation();
}

#[inline(always)]
fn reset_music_stream_clock() {
    // Reset immediately on the caller thread so async command handoff can't
    // leak the previous track's stream position into gameplay timing.
    deadsync_audio::reset_music_stream_clock_state();
    // Invalidate any assist ticks scheduled against the previous timeline; their
    // absolute target frames no longer correspond to the music position.
    deadsync_audio::bump_assist_sfx_generation();
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

fn load_callback_clock_snapshot_now() -> (Instant, u64, CallbackClockSource, CallbackClockWindow) {
    deadsync_audio::load_callback_clock_snapshot_now(current_callback_clock_nanos)
}

#[inline(always)]
fn stream_position_frames_from_window(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    window: CallbackClockWindow,
) -> f64 {
    if let Some(frames) =
        audio_stream_position_frames_from_window(sample_rate, start_frame, at_nanos, window)
    {
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
    fallback_stream_position_frames(start_frame, window)
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
/// absolute audio stream frame timeline at which it will
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
    let start = deadsync_audio::music_track_start_frame();
    Some(start.saturating_add(track_frame.round() as u64))
}

/// Plays a music track from a file path.
pub fn play_music(path: PathBuf, cut: Cut, looping: bool, rate: f32) {
    let rate = normalized_music_rate(rate);
    reset_music_stream_clock();
    deadsync_audio::seed_music_stream_clock(cut.start_sec, rate);

    // Resolve a per-play track id and decide on the initial ReplayGain
    // value. If the user has the experimental ReplayGain setting on, and we
    // already have a cached/computed value, apply it immediately; otherwise
    // queue a background analysis job. If the setting is off, force unity
    // gain so a previous track's value doesn't leak forward.
    let track_id = deadsync_audio::next_music_track_id();
    let initial_gain: f32 = if crate::config::get().enable_replaygain {
        replaygain::get_or_queue_gain_linear(&path, track_id).unwrap_or(1.0)
    } else {
        1.0
    };
    deadsync_audio::set_music_target_gain(initial_gain);
    // Snap to the new target at the track boundary so the previous track's
    // gain doesn't audibly bleed into the start of this one.
    deadsync_audio::snap_music_gain_generation();

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
    let active_id = deadsync_audio::active_music_track_id();
    if active_id != track_id {
        return;
    }
    if !deadsync_audio::music_track_active() {
        return;
    }
    deadsync_audio::set_music_target_gain(gain_linear);
}

/// Called from the config layer when the user toggles the ReplayGain
/// setting. When disabled, forces unity gain immediately so the change is
/// audible without requiring track restart.
pub fn on_replaygain_setting_changed(enabled: bool) {
    if !enabled {
        deadsync_audio::reset_music_target_gain();
    }
}

/// Stops the currently playing music track.
pub fn stop_music() {
    reset_music_stream_clock();
    deadsync_audio::reset_music_target_gain();
    deadsync_audio::snap_music_gain_generation();
    let _ = ENGINE.command_sender.send(AudioCommand::StopMusic);
}

/// Adjusts the playback rate for the current music stream, if any.
pub fn set_music_rate(rate: f32) {
    let rate = normalized_music_rate(rate);
    deadsync_audio::set_music_clock_rate(rate);
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
    deadsync_audio::assist_sfx_generation()
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
            None => match deadsync_audio::seeded_music_position(stream_seconds) {
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
    let has_started = deadsync_audio::music_track_has_started();
    if !has_started {
        if let Some((music_seconds, slope)) = deadsync_audio::seeded_music_position(0.0) {
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
    let start = deadsync_audio::music_track_start_frame();
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
    let stream = backends::linux_alsa::start(prep, music_ring, sfx_receiver, audio_render_maps())?;
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
    let stream = backends::linux_jack::start(prep, music_ring, sfx_receiver, audio_render_maps())?;
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
    let stream =
        backends::linux_pipewire::start(prep, music_ring, sfx_receiver, audio_render_maps())?;
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
    let stream = backends::linux_pulse::start(prep, music_ring, sfx_receiver, audio_render_maps())?;
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
    let stream = backends::freebsd_pcm::start(prep, music_ring, sfx_receiver, audio_render_maps())?;
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
    let stream =
        backends::macos_coreaudio::start(prep, music_ring, sfx_receiver, audio_render_maps())?;
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
            backends::windows_wasapi::start(prep, music_ring, sfx_receiver, audio_render_maps())
                .map_err(|err| {
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
                deadsync_audio::activate_music_track();
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
                deadsync_audio::stop_music_track();
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
