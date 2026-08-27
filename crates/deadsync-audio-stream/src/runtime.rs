#[cfg(target_os = "linux")]
use deadlib_audio::LinuxAudioBackend;
use deadlib_audio::{InitConfig, OutputPlan, prepare_output};
use deadlib_audio_core::{
    OutputBackendReady, OutputDeviceInfo, OutputTimingSnapshot, PlayedMapReader, SfxSender,
    StutterDiagAudioEvent, normalized_music_rate,
};
use deadlib_platform::dirs;
use deadsync_audio_replaygain as replaygain;
use log::info;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use crate::mix::{
    EFFECT_BUS, SCREEN_BUS, assist_tick_generation, init_controls, stop_assist_tick_bus,
    stop_screen_bus,
};
use crate::music_map::clear_music_pos_map;
use crate::sfx_cache::SfxCache;
use crate::{Cut, MusicClock, MusicStreamRuntime, OutputFormat, SfxId, StreamCommand};

static REPLAYGAIN_ENABLED: AtomicBool = AtomicBool::new(false);
static PRESERVE_PITCH_ENABLED: AtomicBool = AtomicBool::new(false);

/// Application-thread owner for audio commands and the sole SFX producer.
///
/// A successful initialization installs one engine. The no-audio value keeps
/// the same API but has no shared runtime state. `AudioControl` is deliberately
/// non-`Clone`: `App` is the only producer for the callback's bounded ring.
pub struct AudioControl {
    engine: Option<AudioEngine>,
}

struct AudioEngine {
    command_sender: Sender<StreamCommand>,
    sfx_cache: SfxCache,
    device_sample_rate: u32,
    device_channels: usize,
    startup_output_devices: Vec<OutputDeviceInfo>,
}

struct AudioThreadReady {
    backend_ready: OutputBackendReady,
    sfx_sender: SfxSender,
    played_map: PlayedMapReader,
}

const fn output_format(engine: &AudioEngine) -> OutputFormat {
    OutputFormat {
        sample_rate_hz: engine.device_sample_rate,
        channels: engine.device_channels,
    }
}

#[inline(always)]
pub fn timing_diag_last_callback_gap_ns() -> u64 {
    deadlib_audio_core::timing_diag_last_callback_gap_ns()
}

pub fn stutter_diag_trigger_seq() -> u64 {
    deadlib_audio_core::stutter_diag_trigger_seq()
}

pub fn collect_stutter_diag_events(
    now_host_nanos: u64,
    window_ns: u64,
    out: &mut Vec<StutterDiagAudioEvent>,
) {
    deadlib_audio_core::collect_stutter_diag_events(now_host_nanos, window_ns, out);
}

pub fn init(cfg: InitConfig) -> Result<(AudioControl, MusicClock), String> {
    let app_dirs = dirs::app_dirs();
    replaygain::init(replaygain::InitConfig {
        cache_file: app_dirs.replaygain_cache_file(),
        legacy_cache_dir: app_dirs.replaygain_cache_dir(),
        result_callback: set_music_replaygain_if_matches,
    })
    .map_err(str::to_string)?;
    Ok(init_engine_and_thread(&cfg))
}

#[cfg(target_os = "linux")]
pub fn available_linux_backends() -> Vec<LinuxAudioBackend> {
    deadlib_audio::available_linux_backends()
}

pub fn set_replaygain_enabled(enabled: bool) {
    REPLAYGAIN_ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        deadlib_audio_core::reset_music_target_gain();
    }
}

#[inline(always)]
pub fn replaygain_enabled() -> bool {
    REPLAYGAIN_ENABLED.load(Ordering::Relaxed)
}

/// Sets the startup `RateModPreservesPitch` preference before an audio control
/// exists. Live application changes go through [`AudioControl`] so the current
/// track receives the update as well.
pub fn set_preserve_pitch_enabled(enabled: bool) {
    PRESERVE_PITCH_ENABLED.store(enabled, Ordering::Relaxed);
}

#[inline(always)]
pub fn preserve_pitch_enabled() -> bool {
    PRESERVE_PITCH_ENABLED.load(Ordering::Relaxed)
}

impl AudioControl {
    pub const fn without_audio() -> Self {
        Self { engine: None }
    }

    #[inline(always)]
    pub const fn is_available(&self) -> bool {
        self.engine.is_some()
    }

    pub fn startup_output_devices(&self) -> &[OutputDeviceInfo] {
        self.engine
            .as_ref()
            .map_or(&[], |engine| engine.startup_output_devices.as_slice())
    }

    pub fn play_sfx(&mut self, path: &str) {
        self.play_sfx_on_bus(path, EFFECT_BUS);
    }

    pub fn play_screen_sfx(&mut self, path: &str) {
        self.play_sfx_on_bus(path, SCREEN_BUS);
    }

    fn play_sfx_on_bus(&mut self, path: &str, bus: deadlib_audio_core::MixBus) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        let output = output_format(engine);
        engine.sfx_cache.play(path, bus, output, resolve_asset_path);
    }

    pub fn preload_sfx(&mut self, path: &str) -> Option<SfxId> {
        let engine = self.engine.as_mut()?;
        let output = output_format(engine);
        engine.sfx_cache.preload(path, output, resolve_asset_path)
    }

    pub fn play_resolved_sfx(&mut self, sound: &SfxId) {
        if let Some(engine) = self.engine.as_mut() {
            engine.sfx_cache.play_resolved(sound, EFFECT_BUS, 0);
        }
    }

    pub fn play_resolved_assist_tick(&mut self, sound: &SfxId) {
        if let Some(engine) = self.engine.as_mut() {
            engine.sfx_cache.play_assist_tick(sound, 0);
        }
    }

    /// Schedules a resolved assist tick on the callback's absolute stream
    /// timeline. A target of zero preserves immediate-play fallback behavior.
    pub fn play_scheduled_assist_tick(&mut self, sound: &SfxId, target_stream_frame: u64) {
        if let Some(engine) = self.engine.as_mut() {
            engine
                .sfx_cache
                .play_assist_tick(sound, target_stream_frame);
        }
    }

    pub fn stop_screen_sfx(&self) {
        stop_screen_bus();
    }

    pub fn play_music(&self, path: PathBuf, cut: Cut, looping: bool, rate: f32) {
        play_music(self.engine.as_ref(), path, cut, looping, rate);
    }

    pub fn stop_music(&self) {
        stop_music(self.engine.as_ref());
    }

    pub fn set_music_rate(&self, rate: f32) {
        set_music_rate(self.engine.as_ref(), rate);
    }

    pub fn set_preserve_pitch_enabled(&self, enabled: bool) {
        set_preserve_pitch_enabled(enabled);
        if let Some(engine) = self.engine.as_ref() {
            let generation = clear_music_pos_map();
            let _ = engine.command_sender.send(StreamCommand::SetPreservePitch {
                enabled,
                generation,
            });
        }
    }

    pub fn set_replaygain_enabled(&self, enabled: bool) {
        set_replaygain_enabled(enabled);
    }
}

fn resolve_asset_path(path: &str) -> PathBuf {
    dirs::app_dirs().resolve_asset_path(path)
}

#[inline(always)]
fn reset_music_stream_clock() -> u64 {
    // Reset immediately on the caller thread so async command handoff can't
    // leak the previous track's stream position into gameplay timing.
    deadlib_audio_core::reset_music_stream_clock_state();
    // Invalidate any assist ticks scheduled against the previous timeline; their
    // absolute target frames no longer correspond to the music position.
    stop_assist_tick_bus();
    clear_music_pos_map()
}

fn play_music(engine: Option<&AudioEngine>, path: PathBuf, cut: Cut, looping: bool, rate: f32) {
    let rate = normalized_music_rate(rate);
    let generation = reset_music_stream_clock();
    deadlib_audio_core::seed_music_stream_clock(cut.start_sec, rate);

    let track_id = deadlib_audio_core::next_music_track_id();
    let initial_gain = if replaygain_enabled() {
        replaygain::get_or_queue_gain_linear(&path, track_id).unwrap_or(1.0)
    } else {
        1.0
    };
    deadlib_audio_core::set_music_target_gain(initial_gain);
    // Snap to the new target at the track boundary so the previous track's
    // gain doesn't audibly bleed into the start of this one.
    deadlib_audio_core::snap_music_gain_generation();

    if let Some(engine) = engine {
        let _ = engine.command_sender.send(StreamCommand::PlayMusic {
            path,
            cut,
            looping,
            rate,
            preserve_pitch: preserve_pitch_enabled(),
            generation,
        });
    }
}

/// Applies a ReplayGain result from the background analyzer, but only if it
/// still corresponds to the currently active music track. Called by
/// `deadsync_audio_replaygain`; safe to call from any thread.
pub fn set_music_replaygain_if_matches(track_id: u64, gain_linear: f32) {
    let active_id = deadlib_audio_core::active_music_track_id();
    if active_id != track_id {
        return;
    }
    if !deadlib_audio_core::music_track_active() {
        return;
    }
    deadlib_audio_core::set_music_target_gain(gain_linear);
}

fn stop_music(engine: Option<&AudioEngine>) {
    let generation = reset_music_stream_clock();
    deadlib_audio_core::reset_music_target_gain();
    deadlib_audio_core::snap_music_gain_generation();
    if let Some(engine) = engine {
        let _ = engine
            .command_sender
            .send(StreamCommand::StopMusic { generation });
    }
}

fn set_music_rate(engine: Option<&AudioEngine>, rate: f32) {
    let rate = normalized_music_rate(rate);
    deadlib_audio_core::set_music_clock_rate(rate);
    let generation = clear_music_pos_map();
    if let Some(engine) = engine {
        let _ = engine
            .command_sender
            .send(StreamCommand::SetMusicRate { rate, generation });
    }
}

pub fn assist_sfx_generation() -> u64 {
    assist_tick_generation()
}

pub fn get_output_timing_snapshot() -> OutputTimingSnapshot {
    deadlib_audio_core::get_output_timing_snapshot()
}

#[inline(always)]
fn publish_output_backend_ready(ready: OutputBackendReady) {
    deadlib_audio_core::publish_output_backend_ready(ready);
}

fn init_engine_and_thread(cfg: &InitConfig) -> (AudioControl, MusicClock) {
    let (command_sender, command_receiver) = channel();
    let (ready_sender, ready_receiver) = channel();
    let controls = init_controls();
    let output_plan = prepare_output(cfg, controls.clone());
    let startup_output_devices = output_plan.devices().to_vec();

    thread::spawn(move || {
        audio_manager_thread(command_receiver, ready_sender, output_plan);
    });

    let thread_ready = match ready_receiver.recv() {
        Ok(Ok(ready)) => ready,
        Ok(Err(err)) => panic!("failed to initialize audio runtime: {err}"),
        Err(_) => panic!("audio manager thread exited before reporting ready"),
    };
    let AudioThreadReady {
        backend_ready: ready,
        sfx_sender,
        played_map,
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
    let music_clock = MusicClock::new(played_map, ready.device_sample_rate);
    (
        AudioControl {
            engine: Some(AudioEngine {
                command_sender,
                sfx_cache: SfxCache::new(controls, sfx_sender),
                device_sample_rate: ready.device_sample_rate,
                device_channels: ready.device_channels,
                startup_output_devices,
            }),
        },
        music_clock,
    )
}

fn audio_manager_thread(
    command_receiver: Receiver<StreamCommand>,
    ready_sender: Sender<Result<AudioThreadReady, String>>,
    output_plan: OutputPlan,
) {
    let opened = match output_plan.open() {
        Ok(output) => output,
        Err(err) => {
            let _ = ready_sender.send(Err(err));
            return;
        }
    };
    let (_session, ready, sfx_sender, stream_handle) = opened.into_parts();
    let deadlib_audio_core::AudioStreamHandle { writer, played_map } = stream_handle;
    let stream_output = OutputFormat {
        sample_rate_hz: ready.device_sample_rate,
        channels: ready.device_channels,
    };
    if ready_sender
        .send(Ok(AudioThreadReady {
            backend_ready: ready,
            sfx_sender,
            played_map,
        }))
        .is_err()
    {
        drop(_session);
        drop(writer);
        return;
    }

    let mut music_runtime = MusicStreamRuntime::new(writer, stream_output);
    while let Ok(command) = command_receiver.recv() {
        music_runtime.handle(command);
    }
    // Stop and join the render callback while the recycle consumer still owns
    // the pool. Pooled blocks are then destroyed here on the manager thread.
    drop(_session);
    drop(music_runtime);
}
