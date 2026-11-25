#[cfg(target_os = "linux")]
use deadlib_audio::LinuxAudioBackend;
use deadlib_audio::{InitConfig, prepare_output};
use deadlib_audio_core::{
    OutputDeviceInfo, OutputTimingSnapshot, SfxPan, StutterDiagAudioEvent, normalized_music_rate,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use crate::mix::{
    EFFECT_BUS, MINE_BUS, SCREEN_BUS, assist_tick_generation, init_controls,
    stop_assist_tick_bus, stop_mine_bus, stop_screen_bus,
};
use crate::music_map::clear_music_pos_map;
use crate::sfx_cache::SfxCache;
use crate::{MusicClock, SfxId};
use deadlib_audio::stream::{self, Cut, OutputFormat, StreamCommand};

static REPLAYGAIN_ENABLED: AtomicBool = AtomicBool::new(false);
static PRESERVE_PITCH_ENABLED: AtomicBool = AtomicBool::new(false);

/// Looks up a cached gain or queues analysis for the track starting playback.
pub type MusicGainLookup = fn(&Path, u64) -> Option<f32>;

/// Application-thread owner for audio commands and the sole SFX producer.
///
/// A successful initialization installs one engine. The no-audio value keeps
/// the same API but has no shared runtime state. `AudioControl` is deliberately
/// non-`Clone`: `App` is the only producer for the callback's bounded ring.
pub struct AudioControl {
    engine: Option<AudioEngine>,
    music_gain_lookup: Option<MusicGainLookup>,
}

struct AudioEngine {
    command_sender: Sender<StreamCommand>,
    controls: std::sync::Arc<deadlib_audio_core::MixControls>,
    sfx_cache: SfxCache,
    output: OutputFormat,
    startup_output_devices: Vec<OutputDeviceInfo>,
}

#[inline(always)]
#[must_use]
pub fn timing_diag_last_callback_gap_ns() -> u64 {
    deadlib_audio_core::timing_diag_last_callback_gap_ns()
}

#[must_use]
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

/// Initialize audio output and return its playback control and clock.
pub fn init(cfg: InitConfig) -> Result<(AudioControl, MusicClock), String> {
    let controls = init_controls();
    let output_plan = prepare_output(&cfg, controls.clone());
    let startup_output_devices = output_plan.devices().to_vec();
    let stream::StreamReady {
        command_sender,
        backend_ready: ready,
        sfx_sender,
        played_map,
    } = stream::start(output_plan)?;
    let music_clock = MusicClock::new(played_map, ready.device_sample_rate);
    Ok((
        AudioControl {
            music_gain_lookup: None,
            engine: Some(AudioEngine {
                command_sender,
                controls: controls.clone(),
                sfx_cache: SfxCache::new(controls, sfx_sender),
                output: OutputFormat {
                    sample_rate_hz: ready.device_sample_rate,
                    channels: ready.device_channels,
                },
                startup_output_devices,
            }),
        },
        music_clock,
    ))
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
    #[must_use]
    pub const fn without_audio() -> Self {
        Self {
            engine: None,
            music_gain_lookup: None,
        }
    }

    #[must_use]
    pub const fn with_music_gain_lookup(mut self, lookup: MusicGainLookup) -> Self {
        self.music_gain_lookup = Some(lookup);
        self
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

    /// Loads a sound for later playback; call during startup or screen/song preparation.
    ///
    /// Returns `None` when audio is unavailable or the sound cannot be loaded.
    pub fn prepare_sfx(&mut self, path: &Path) -> Option<SfxId> {
        let engine = self.engine.as_mut()?;
        engine.sfx_cache.prepare(path, engine.output)
    }

    /// Prepares an embedded WAV for retained playback on this output device.
    pub fn prepare_embedded_sfx(&self, name: &str, wav: &[u8]) -> Option<SfxId> {
        let engine = self.engine.as_ref()?;
        match SfxId::from_wav(wav, engine.output) {
            Ok(sound) => {
                log::debug!("Prepared embedded SFX: {name}");
                Some(sound)
            }
            Err(error) => {
                log::warn!("Failed to prepare embedded SFX '{name}': {error}");
                None
            }
        }
    }

    /// Enqueues a prepared effect without filesystem access or decoding.
    pub fn play_sfx(&mut self, sound: &SfxId) {
        if let Some(engine) = self.engine.as_mut() {
            engine.sfx_cache.play(sound, EFFECT_BUS, 0);
        }
    }

    /// Stop and replay the current mine sound for one player (ITGmania parity).
    pub fn play_mine_sfx(&mut self, sound: &SfxId, player: u8, two_players: bool) {
        if let Some(engine) = self.engine.as_mut() {
            let pan = if two_players {
                if player == 0 { SfxPan::Left } else { SfxPan::Right }
            } else {
                SfxPan::Center
            };
            engine.sfx_cache.restart(sound, MINE_BUS, player, pan);
        }
    }

    pub fn stop_mine_sfx(&self) {
        stop_mine_bus();
    }

    /// Enqueues a prepared effect that stops when the owning screen exits.
    pub fn play_screen_sfx(&mut self, sound: &SfxId) {
        if let Some(engine) = self.engine.as_mut() {
            engine.sfx_cache.play(sound, SCREEN_BUS, 0);
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

    pub fn play_actor_sfx(&mut self, owner: u64, sound: &SfxId) {
        if let Some(engine) = self.engine.as_mut() {
            engine.sfx_cache.play_owned(owner, sound, SCREEN_BUS);
        }
    }

    pub fn stop_actor_sfx(&mut self, owner: u64) {
        if let Some(engine) = self.engine.as_mut() { engine.sfx_cache.stop_owned(owner); }
    }

    pub fn pause_actor_sfx(&self, owner: u64, paused: bool) {
        if let Some(engine) = &self.engine { engine.sfx_cache.pause_owned(owner, paused); }
    }

    pub fn stop_screen_sfx(&mut self) {
        if let Some(engine) = self.engine.as_mut() { engine.sfx_cache.stop_owned_voices(); }
        stop_screen_bus();
    }

    pub fn play_music(&self, path: PathBuf, cut: Cut, looping: bool, rate: f32) {
        play_music(
            self.engine.as_ref(),
            path,
            cut,
            looping,
            rate,
            self.music_gain_lookup,
        );
    }

    pub fn stop_music(&self) {
        stop_music(self.engine.as_ref());
    }

    /// Freeze the current music cursor without stopping actor or menu sounds.
    pub fn pause_music(&self, paused: bool) {
        if let Some(engine) = &self.engine
            && engine.controls.set_stream_paused(paused)
        {
            // Scheduled assist sounds use device-frame targets, which no longer
            // correspond to their song times when music is paused or resumed.
            stop_assist_tick_bus();
        }
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

fn play_music(
    engine: Option<&AudioEngine>,
    path: PathBuf,
    cut: Cut,
    looping: bool,
    rate: f32,
    music_gain_lookup: Option<MusicGainLookup>,
) {
    let rate = normalized_music_rate(rate);
    let generation = reset_music_stream_clock();
    deadlib_audio_core::seed_music_stream_clock(cut.start_sec, rate);

    let track_id = deadlib_audio_core::next_music_track_id();
    let initial_gain = if replaygain_enabled() {
        music_gain_lookup
            .and_then(|lookup| lookup(&path, track_id))
            .unwrap_or(1.0)
    } else {
        1.0
    };
    deadlib_audio_core::set_music_target_gain(initial_gain);
    // Snap to the new target at the track boundary so the previous track's
    // gain doesn't audibly bleed into the start of this one.
    deadlib_audio_core::snap_music_gain_generation();

    if let Some(engine) = engine {
        engine.controls.set_stream_paused(false);
        let _ = engine.command_sender.send(StreamCommand::PlayMusic {
            decode_options: crate::DECODE_OPTIONS,
            path,
            cut,
            looping,
            rate,
            preserve_pitch: preserve_pitch_enabled(),
            generation,
        });
    }
}

/// Applies a `ReplayGain` result from the background analyzer, but only if it
/// still corresponds to the currently active music track. Safe to call from any thread.
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
    reset_music_stream_clock();
    deadlib_audio_core::reset_music_target_gain();
    deadlib_audio_core::snap_music_gain_generation();
    if let Some(engine) = engine {
        engine.controls.set_stream_paused(false);
        let _ = engine.command_sender.send(StreamCommand::StopMusic);
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

#[must_use]
pub fn assist_sfx_generation() -> u64 {
    assist_tick_generation()
}

#[must_use]
pub fn get_output_timing_snapshot() -> OutputTimingSnapshot {
    deadlib_audio_core::get_output_timing_snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_audio_core::{
        activate_music_track, active_music_track_id, music_target_gain, stop_music_track,
    };

    #[test]
    fn playback_uses_host_gain_for_the_current_track() {
        let _playback = crate::PLAYBACK_TEST_LOCK.lock().unwrap();
        fn lookup(path: &Path, track_id: u64) -> Option<f32> {
            assert_eq!(path, Path::new("cached.ogg"));
            assert_eq!(track_id, active_music_track_id());
            Some(0.75)
        }

        set_replaygain_enabled(true);
        let audio = AudioControl::without_audio();
        audio.play_music("cached.ogg".into(), Cut::default(), false, 1.0);
        assert_eq!(music_target_gain(), 1.0);
        let old_track = active_music_track_id();

        let audio = audio.with_music_gain_lookup(lookup);
        audio.play_music("cached.ogg".into(), Cut::default(), false, 1.0);
        let current_track = active_music_track_id();
        assert_eq!(music_target_gain(), 0.75);
        stop_music_track();
        set_music_replaygain_if_matches(current_track, 0.5);
        assert_eq!(music_target_gain(), 0.75);
        activate_music_track();
        set_music_replaygain_if_matches(old_track, 0.5);
        assert_eq!(music_target_gain(), 0.75);
        set_music_replaygain_if_matches(current_track, 0.5);
        assert_eq!(music_target_gain(), 0.5);

        set_replaygain_enabled(false);
        audio.play_music("cached.ogg".into(), Cut::default(), false, 1.0);
        assert_eq!(music_target_gain(), 1.0);
        audio.stop_music();
    }
}
