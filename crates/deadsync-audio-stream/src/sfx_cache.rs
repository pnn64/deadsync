use crate::mix::ASSIST_TICK_BUS;
use deadlib_audio::stream::{OutputFormat, load_and_resample_sfx};
use deadlib_audio_core::{MixBus, MixControls, QueuedSfx, SfxPan, SfxSender, SfxVoiceControl};
use log::{debug, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Validated session-lifetime sound data resolved before a hot submission.
#[derive(Clone)]
pub struct SfxId(Arc<[i16]>);

/// Session-lifetime SFX cache owned by the application thread's audio control.
///
/// Thread model: the application thread is the sole cache and queue producer;
/// the audio callback never touches this cache. Capacity is intentionally
/// grow-only for the current session and warmed at screen/song transitions.
/// Only explicit preparation decodes samples from caller-resolved paths. Playback
/// borrows retained `SfxId` values and submits them without locking, hashing,
/// allocation, or I/O.
pub struct SfxCache {
    sounds: HashMap<PathBuf, SfxId>,
    sender: SfxSender,
    controls: Arc<MixControls>,
    owned_voices: HashMap<u64, Arc<SfxVoiceControl>>,
}

impl SfxCache {
    pub fn new(controls: Arc<MixControls>, sender: SfxSender) -> Self {
        Self {
            sounds: HashMap::new(),
            sender,
            controls,
            owned_voices: HashMap::new(),
        }
    }

    /// Decodes and resamples a resolved sound at startup or a screen/song transition.
    ///
    /// Returns `None` if loading fails. Repeated preparation reuses the samples.
    pub fn prepare(&mut self, path: &Path, output: OutputFormat) -> Option<SfxId> {
        if let Some(sound) = self.sounds.get(path) {
            return Some(sound.clone());
        }

        let decoded = match load_and_resample_sfx(path, output, crate::DECODE_OPTIONS) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to prepare SFX '{}': {e}", path.display());
                return None;
            }
        };

        debug!("Cached SFX: {}", path.display());
        let sound = SfxId(decoded);
        self.sounds.insert(path.to_owned(), sound.clone());
        Some(sound)
    }

    /// Enqueues prepared samples on `bus` without loading or looking up a path.
    pub fn play(&mut self, sound: &SfxId, bus: MixBus, target_stream_frame: u64) {
        self.enqueue(sound, bus, target_stream_frame, None, SfxPan::Center);
    }

    /// Retriggers one logical sound instance, such as one player's mine sound.
    pub fn restart(&mut self, sound: &SfxId, bus: MixBus, slot: u8, pan: SfxPan) {
        self.enqueue(sound, bus, 0, Some(slot), pan);
    }

    fn enqueue(
        &mut self,
        sound: &SfxId,
        bus: MixBus,
        target_stream_frame: u64,
        restart_slot: Option<u8>,
        pan: SfxPan,
    ) {
        let _ = self.sender.try_send(QueuedSfx {
            data: Arc::clone(&sound.0),
            bus,
            generation: self.controls.bus_generation(bus),
            restart_slot,
            pan,
            target_stream_frame,
            voice: None,
        });
    }

    /// RageSound::Play starts a detached copy if the actor is already playing.
    /// Stop/Pause affect the actor's original voice, not other actors or copies.
    pub fn play_owned(&mut self, owner: u64, sound: &SfxId, bus: MixBus) {
        if self.owned_voices.get(&owner).is_some_and(|voice| voice.is_playing()) {
            self.play(sound, bus, 0);
            return;
        }
        self.owned_voices.retain(|_, voice| voice.is_playing());
        let voice = Arc::new(SfxVoiceControl::new());
        if self.sender.try_send(QueuedSfx {
            data: Arc::clone(&sound.0), bus,
            generation: self.controls.bus_generation(bus), target_stream_frame: 0,
            restart_slot: None,
            pan: SfxPan::Center,
            voice: Some(Arc::clone(&voice)),
        }).is_ok() {
            self.owned_voices.insert(owner, voice);
        }
    }

    pub fn stop_owned(&mut self, owner: u64) {
        if let Some(voice) = self.owned_voices.remove(&owner) { voice.stop(); }
    }

    pub fn pause_owned(&self, owner: u64, paused: bool) {
        if let Some(voice) = self.owned_voices.get(&owner) { voice.pause(paused); }
    }

    pub fn stop_owned_voices(&mut self) {
        for (_, voice) in self.owned_voices.drain() { voice.stop(); }
    }

    pub fn play_assist_tick(&mut self, sound: &SfxId, target_stream_frame: u64) {
        self.play(sound, ASSIST_TICK_BUS, target_stream_frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_audio_core::sfx_transport;

    #[test]
    fn actor_sound_stop_pause_and_overlapping_play_are_independent() {
        use deadlib_audio_core::{mix_active_sfx, push_queued_sfx, i16_to_f32};
        let controls = Arc::new(MixControls::new());
        let (sender, mut receiver) = sfx_transport(8);
        let mut cache = SfxCache::new(Arc::clone(&controls), sender);
        let sound = SfxId(Arc::from([1000_i16; 64]));
        let bus = MixBus::new(1);
        cache.play_owned(11, &sound, bus);
        cache.play_owned(22, &sound, bus);
        cache.play_owned(11, &sound, bus); // Native PlayCopy while original is playing.
        cache.stop_owned(11);
        cache.pause_owned(22, true);
        let mut active = Vec::new();
        for sound in receiver.try_iter() { push_queued_sfx(&mut active, sound, &controls); }
        let mut mix = [0.0; 8];
        mix_active_sfx(&mut active, &mut mix, 0, 1, &controls);
        assert_eq!(mix, [i16_to_f32(1000); 8], "only detached copy plays");
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].cursor, 0, "pause preserves actor cursor");
        cache.pause_owned(22, false);
        mix.fill(0.0);
        mix_active_sfx(&mut active, &mut mix, 8, 1, &controls);
        assert_eq!(mix, [i16_to_f32(1000) * 2.0; 8]);
        cache.stop_owned_voices();
        controls.stop_bus(bus);
        mix.fill(0.0);
        assert!(!mix_active_sfx(&mut active, &mut mix, 16, 1, &controls));
        assert!(active.is_empty(), "screen exit also stops detached copies");
    }

    #[test]
    fn prepared_sfx_survives_source_removal_and_bus_stop() {
        let path = std::env::temp_dir().join(format!("deadsync-sfx-{}.wav", std::process::id()));
        // A short mono PCM fixture requires both resampling and channel mapping.
        let samples = [1000_i16; 4096];
        let size = (samples.len() * 2) as u32;
        let hz = 24_000_u32;
        let mut bytes = Vec::with_capacity(44 + size as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        bytes.extend_from_slice(&hz.to_le_bytes());
        bytes.extend_from_slice(&(hz * 2).to_le_bytes());
        bytes.extend_from_slice(b"\x02\0\x10\0data");
        bytes.extend_from_slice(&size.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();

        let controls = Arc::new(MixControls::new());
        let (sender, mut receiver) = sfx_transport(4);
        let mut cache = SfxCache::new(Arc::clone(&controls), sender);
        let output = OutputFormat {
            sample_rate_hz: 48_000,
            channels: 2,
        };
        let prepared = cache.prepare(&path, output);
        std::fs::remove_file(&path).unwrap();
        let sound = prepared.expect("valid PCM fixture must prepare");
        assert!(
            receiver.try_iter().next().is_none(),
            "preparation must not play"
        );
        let same = cache
            .prepare(&path, output)
            .expect("cached sound remains available without its source");

        let effect = MixBus::new(0);
        let screen = MixBus::new(1);
        cache.play(&sound, screen, 480);
        cache.play(&same, effect, 0);
        controls.stop_bus(screen);
        cache.play(&sound, screen, 960);
        cache.restart(&sound, effect, 1, SfxPan::Left);
        let queued: Vec<_> = receiver.try_iter().collect();
        assert_eq!(queued.len(), 4);
        assert!(Arc::ptr_eq(&queued[0].data, &queued[1].data));
        assert!(Arc::ptr_eq(&queued[0].data, &queued[2].data));
        assert_eq!(queued[0].bus, screen);
        assert_eq!(queued[1].bus, effect);
        assert_eq!(queued[2].target_stream_frame, 960);
        assert_eq!(queued[0].target_stream_frame, 480);
        assert_eq!(queued[1].target_stream_frame, 0);
        assert!(!controls.is_current(screen, queued[0].generation));
        assert!(controls.is_current(effect, queued[1].generation));
        assert!(controls.is_current(screen, queued[2].generation));
        assert_eq!(queued[0].restart_slot, None);
        assert_eq!(queued[1].restart_slot, None);
        assert_eq!(queued[3].restart_slot, Some(1));
        assert_eq!(queued[3].pan, SfxPan::Left);
        assert!(queued[0].data.len() > samples.len() * 3);
        assert!(queued[0].data.iter().any(|sample| *sample > 900));
        assert!(
            queued[0]
                .data
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[0] == frame[1])
        );
        assert!(
            cache
                .prepare(&path.with_extension("missing"), output)
                .is_none()
        );
        assert!(receiver.try_iter().next().is_none());
    }
}
