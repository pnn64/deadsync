#[cfg(target_os = "linux")]
use deadsync_audio::LinuxAudioBackend;
pub use deadsync_audio::{
    Cut, InitConfig, MusicStreamClockSnapshot, OutputDeviceInfo, OutputTimingSnapshot,
};
use deadsync_audio::{
    OutputBackendReady, QueuedSfx, SfxLane, StutterDiagAudioEvent, normalized_music_rate,
};
use deadsync_audio_backend_native::launch::{
    NativeBackendLaunch, build_audio_launch, start_output_backend,
};
use deadsync_audio_replaygain as replaygain;
use deadsync_audio_stream as audio_stream;
use deadsync_audio_stream::{OutputFormat, StreamCommand};
use deadsync_platform::dirs;
use log::info;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel};
use std::thread;

/* ============================== Public API ============================== */

// Global engine (initialized once)
static ENGINE_INIT_CFG: OnceLock<InitConfig> = OnceLock::new();
static ENGINE: std::sync::LazyLock<AudioEngine> =
    std::sync::LazyLock::new(|| init_engine_and_thread(engine_init_cfg()));

struct AudioEngine {
    command_sender: Sender<StreamCommand>,
    sfx_sender: SyncSender<QueuedSfx>,
    sfx_cache: audio_stream::SfxCache,
    device_sample_rate: u32,
    device_channels: usize,
    startup_output_devices: Vec<OutputDeviceInfo>,
}

struct AudioThreadReady {
    backend_ready: OutputBackendReady,
    sfx_sender: SyncSender<QueuedSfx>,
}

#[inline(always)]
fn output_format() -> OutputFormat {
    OutputFormat {
        sample_rate_hz: ENGINE.device_sample_rate,
        channels: ENGINE.device_channels,
    }
}

#[inline(always)]
pub(crate) fn timing_diag_enabled() -> bool {
    audio_stream::timing_diag_enabled()
}

#[inline(always)]
pub(crate) fn timing_diag_last_callback_gap_ns() -> u64 {
    deadsync_audio::timing_diag_last_callback_gap_ns()
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
    let app_dirs = dirs::app_dirs();
    replaygain::init(replaygain::InitConfig {
        cache_file: app_dirs.replaygain_cache_file(),
        legacy_cache_dir: app_dirs.replaygain_cache_dir(),
        result_callback: set_music_replaygain_if_matches,
    })
    .map_err(str::to_string)?;
    std::sync::LazyLock::force(&ENGINE);
    audio_stream::force_music_map_runtime();
    Ok(())
}

pub fn startup_output_devices() -> Vec<OutputDeviceInfo> {
    ENGINE.startup_output_devices.clone()
}

#[cfg(target_os = "linux")]
pub fn available_linux_backends() -> Vec<LinuxAudioBackend> {
    deadsync_audio_backend_native::launch::available_linux_backends()
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
    #[cfg(test)]
    if !is_initialized() {
        return;
    }
    ENGINE.sfx_cache.play_assist_tick(
        path,
        output_format(),
        &ENGINE.sfx_sender,
        resolve_asset_path,
    );
}

/// Plays a preloaded gameplay assist tick without decoding on miss.
pub fn play_preloaded_assist_tick(path: &str) {
    #[cfg(test)]
    if !is_initialized() {
        return;
    }
    ENGINE
        .sfx_cache
        .play_preloaded_assist_tick(path, &ENGINE.sfx_sender);
}

/// Plays a preloaded gameplay assist tick scheduled to become audible at an
/// absolute stream frame (see [`assist_tick_stream_frame_for_music_seconds`]).
/// Because the target frame lies on the same audio stream timeline the mixer
/// writes against, output latency is compensated implicitly. Falls back to
/// immediate playback when `target_stream_frame == 0`.
pub fn play_scheduled_assist_tick(path: &str, target_stream_frame: u64) {
    #[cfg(test)]
    if !is_initialized() {
        return;
    }
    ENGINE
        .sfx_cache
        .play_scheduled_assist_tick(path, target_stream_frame, &ENGINE.sfx_sender);
}

fn play_preloaded_sfx_on_lane(path: &str, lane: SfxLane) {
    #[cfg(test)]
    if !is_initialized() {
        return;
    }
    ENGINE
        .sfx_cache
        .play_preloaded(path, lane, &ENGINE.sfx_sender);
}

fn play_sfx_on_lane(path: &str, lane: SfxLane) {
    #[cfg(test)]
    if !is_initialized() {
        return;
    }
    ENGINE.sfx_cache.play(
        path,
        lane,
        output_format(),
        &ENGINE.sfx_sender,
        resolve_asset_path,
    );
}

/// Preloads a sound effect into cache without playing it.
pub fn preload_sfx(path: &str) {
    ENGINE
        .sfx_cache
        .preload(path, output_format(), resolve_asset_path);
}

fn resolve_asset_path(path: &str) -> PathBuf {
    dirs::app_dirs().resolve_asset_path(path)
}

#[inline(always)]
fn reset_music_stream_clock() {
    // Reset immediately on the caller thread so async command handoff can't
    // leak the previous track's stream position into gameplay timing.
    deadsync_audio::reset_music_stream_clock_state();
    // Invalidate any assist ticks scheduled against the previous timeline; their
    // absolute target frames no longer correspond to the music position.
    deadsync_audio::bump_assist_sfx_generation();
    audio_stream::clear_music_pos_map();
}

/// Maps a music position (seconds, pre-global-offset stream time) to the
/// absolute audio stream frame timeline at which it will
/// be audible. Returns `None` when there is no usable mapping or the result is
/// implausible (caller should then fall back to immediate playback).
///
/// Runs on the gameplay thread; the lock is fine there since the audio callback
/// never takes it on its hot path.
pub fn assist_tick_stream_frame_for_music_seconds(music_seconds: f64) -> Option<u64> {
    audio_stream::assist_tick_stream_frame_for_music_seconds(music_seconds)
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

    let _ = ENGINE.command_sender.send(StreamCommand::PlayMusic {
        path,
        cut,
        looping,
        rate,
    });
}

pub fn snap_music_start_sec(path: &Path, start_sec: f64) -> f64 {
    audio_stream::snap_music_start_sec(path, start_sec)
}

/// Applies a ReplayGain result from the background analyzer, but only if it
/// still corresponds to the currently active music track. Called by
/// `deadsync_audio_replaygain`; safe to call from any thread.
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
    let _ = ENGINE.command_sender.send(StreamCommand::StopMusic);
}

/// Adjusts the playback rate for the current music stream, if any.
pub fn set_music_rate(rate: f32) {
    let rate = normalized_music_rate(rate);
    deadsync_audio::set_music_clock_rate(rate);
    let _ = ENGINE
        .command_sender
        .send(StreamCommand::SetMusicRate(rate));
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

/// Returns the current stream position and the `Instant` it is valid for.
pub fn get_music_stream_clock_snapshot() -> MusicStreamClockSnapshot {
    audio_stream::music_stream_clock_snapshot(ENGINE.device_sample_rate)
}

pub fn get_output_timing_snapshot() -> OutputTimingSnapshot {
    deadsync_audio::get_output_timing_snapshot()
}

/* ============================ Engine internals ============================ */

#[inline(always)]
fn publish_output_backend_ready(ready: OutputBackendReady) {
    deadsync_audio::publish_output_backend_ready(ready);
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
        sfx_cache: audio_stream::SfxCache::new(),
        device_sample_rate: ready.device_sample_rate,
        device_channels: ready.device_channels,
        startup_output_devices: device_probes.into_iter().map(|probe| probe.info).collect(),
    }
}

/// Manager thread: builds the output backend and forwards stream commands.
fn audio_manager_thread(
    command_receiver: Receiver<StreamCommand>,
    ready_sender: Sender<Result<AudioThreadReady, String>>,
    launch: NativeBackendLaunch,
) {
    let music_ring = audio_stream::new_music_sample_ring();
    let (mut _backend, _ready, sfx_sender) = match start_output_backend(
        launch,
        music_ring.clone(),
        audio_stream::music_render_maps(),
    ) {
        Ok(output) => output,
        Err(err) => {
            let _ = ready_sender.send(Err(err));
            return;
        }
    };
    let stream_output = OutputFormat {
        sample_rate_hz: _ready.device_sample_rate,
        channels: _ready.device_channels,
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

    let mut music_runtime = audio_stream::MusicStreamRuntime::new(music_ring, stream_output);
    while let Ok(command) = command_receiver.recv() {
        music_runtime.handle(command);
    }
}
