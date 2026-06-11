#[cfg(target_os = "linux")]
use deadsync_audio::LinuxAudioBackend;
use deadsync_audio::{
    Cut, InitConfig, MusicStreamClockSnapshot, OutputBackendReady, OutputDeviceInfo,
    OutputTimingSnapshot, QueuedSfx, SfxLane, StutterDiagAudioEvent, normalized_music_rate,
};
use deadsync_audio_backend_native::launch::{
    NativeBackendLaunch, build_audio_launch, start_output_backend,
};
use deadsync_audio_replaygain as replaygain;
use deadsync_platform::dirs;
use log::info;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel};
use std::thread;

use crate::{
    MusicStreamRuntime, OutputFormat, SfxCache, StreamCommand, clear_music_pos_map,
    force_music_map_runtime, music_render_maps, music_stream_clock_snapshot, new_music_sample_ring,
};

static ENGINE_INIT_CFG: OnceLock<InitConfig> = OnceLock::new();
static ENGINE: std::sync::LazyLock<AudioEngine> =
    std::sync::LazyLock::new(|| init_engine_and_thread(engine_init_cfg()));
static REPLAYGAIN_ENABLED: AtomicBool = AtomicBool::new(false);

struct AudioEngine {
    command_sender: Sender<StreamCommand>,
    sfx_sender: SyncSender<QueuedSfx>,
    sfx_cache: SfxCache,
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
pub fn timing_diag_last_callback_gap_ns() -> u64 {
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

#[inline(always)]
fn engine_init_cfg() -> InitConfig {
    *ENGINE_INIT_CFG
        .get()
        .expect("deadsync_audio_stream::init must be called before audio use")
}

#[inline(always)]
pub fn is_initialized() -> bool {
    ENGINE_INIT_CFG.get().is_some()
}

pub fn init(cfg: InitConfig) -> Result<(), String> {
    if let Some(existing) = ENGINE_INIT_CFG.get() {
        if *existing != cfg {
            return Err("audio runtime already initialized with different config".to_string());
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
    force_music_map_runtime();
    Ok(())
}

pub fn startup_output_devices() -> Vec<OutputDeviceInfo> {
    ENGINE.startup_output_devices.clone()
}

#[cfg(target_os = "linux")]
pub fn available_linux_backends() -> Vec<LinuxAudioBackend> {
    deadsync_audio_backend_native::launch::available_linux_backends()
}

pub fn set_replaygain_enabled(enabled: bool) {
    REPLAYGAIN_ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        deadsync_audio::reset_music_target_gain();
    }
}

#[inline(always)]
pub fn replaygain_enabled() -> bool {
    REPLAYGAIN_ENABLED.load(Ordering::Relaxed)
}

pub fn play_sfx(path: &str) {
    play_sfx_on_lane(path, SfxLane::Effect);
}

pub fn play_screen_sfx(path: &str) {
    play_sfx_on_lane(path, SfxLane::Screen);
}

pub fn play_preloaded_sfx(path: &str) {
    play_preloaded_sfx_on_lane(path, SfxLane::Effect);
}

pub fn stop_screen_sfx() {
    deadsync_audio::bump_screen_sfx_generation();
}

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
    clear_music_pos_map();
}

pub fn play_music(path: PathBuf, cut: Cut, looping: bool, rate: f32) {
    let rate = normalized_music_rate(rate);
    reset_music_stream_clock();
    deadsync_audio::seed_music_stream_clock(cut.start_sec, rate);

    let track_id = deadsync_audio::next_music_track_id();
    let initial_gain = if replaygain_enabled() {
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

pub fn stop_music() {
    reset_music_stream_clock();
    deadsync_audio::reset_music_target_gain();
    deadsync_audio::snap_music_gain_generation();
    let _ = ENGINE.command_sender.send(StreamCommand::StopMusic);
}

pub fn set_music_rate(rate: f32) {
    let rate = normalized_music_rate(rate);
    deadsync_audio::set_music_clock_rate(rate);
    let _ = ENGINE
        .command_sender
        .send(StreamCommand::SetMusicRate(rate));
}

/// Returns the elapsed real time (in seconds) of the currently playing music
/// stream, measured from the moment the first sample of that stream reached
/// the output callback.
pub fn get_music_stream_position_seconds() -> f32 {
    get_music_stream_clock_snapshot().stream_seconds
}

pub fn assist_sfx_generation() -> u64 {
    deadsync_audio::assist_sfx_generation()
}

pub fn get_music_stream_clock_snapshot() -> MusicStreamClockSnapshot {
    music_stream_clock_snapshot(ENGINE.device_sample_rate)
}

pub fn get_output_timing_snapshot() -> OutputTimingSnapshot {
    deadsync_audio::get_output_timing_snapshot()
}

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
        Ok(Err(err)) => panic!("failed to initialize audio runtime: {err}"),
        Err(_) => panic!("audio manager thread exited before reporting ready"),
    };
    let AudioThreadReady {
        backend_ready: ready,
        sfx_sender,
    } = thread_ready;

    info!(
        "Audio runtime initialized ({} Hz, {} ch, backend={} req={} fallback={} clock={} quality={} device='{}').",
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
        sfx_cache: SfxCache::new(),
        device_sample_rate: ready.device_sample_rate,
        device_channels: ready.device_channels,
        startup_output_devices: device_probes.into_iter().map(|probe| probe.info).collect(),
    }
}

fn audio_manager_thread(
    command_receiver: Receiver<StreamCommand>,
    ready_sender: Sender<Result<AudioThreadReady, String>>,
    launch: NativeBackendLaunch,
) {
    let music_ring = new_music_sample_ring();
    let (mut _backend, ready, sfx_sender) =
        match start_output_backend(launch, music_ring.clone(), music_render_maps()) {
            Ok(output) => output,
            Err(err) => {
                let _ = ready_sender.send(Err(err));
                return;
            }
        };
    let stream_output = OutputFormat {
        sample_rate_hz: ready.device_sample_rate,
        channels: ready.device_channels,
    };
    if ready_sender
        .send(Ok(AudioThreadReady {
            backend_ready: ready,
            sfx_sender,
        }))
        .is_err()
    {
        return;
    }

    let mut music_runtime = MusicStreamRuntime::new(music_ring, stream_output);
    while let Ok(command) = command_receiver.recv() {
        music_runtime.handle(command);
    }
}
