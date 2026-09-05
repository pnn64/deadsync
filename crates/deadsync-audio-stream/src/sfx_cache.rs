use crate::mix::ASSIST_TICK_BUS;
use crate::{OutputFormat, load_and_resample_sfx};
use deadlib_audio_core::{MixBus, MixControls, QueuedSfx, SfxSender};
use log::{debug, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Validated session-lifetime sound data resolved before a hot submission.
#[derive(Clone)]
pub struct SfxId(Arc<[i16]>);

/// Session-lifetime SFX cache owned by the application thread's audio control.
///
/// Thread model: the application thread is the sole cache and queue producer;
/// the audio callback never touches this cache. Capacity is intentionally
/// grow-only for the current session and warmed at screen/song transitions.
/// Only explicit preparation resolves paths and decodes samples. Playback
/// borrows retained `SfxId` values and submits them without locking, hashing,
/// allocation, or I/O.
pub struct SfxCache {
    sounds: HashMap<String, SfxId>,
    sender: SfxSender,
    controls: Arc<MixControls>,
}

impl SfxCache {
    pub fn new(controls: Arc<MixControls>, sender: SfxSender) -> Self {
        Self {
            sounds: HashMap::new(),
            sender,
            controls,
        }
    }

    /// Resolves, decodes and resamples a sound at startup or a screen/song transition.
    ///
    /// Returns `None` if loading fails. Repeated preparation reuses the samples.
    pub fn prepare(
        &mut self,
        path: &str,
        output: OutputFormat,
        resolve_asset_path: impl FnOnce(&str) -> PathBuf,
    ) -> Option<SfxId> {
        if let Some(sound) = self.sounds.get(path) {
            return Some(sound.clone());
        }

        let resolved = resolve_asset_path(path);
        let resolved_str = resolved.to_string_lossy();
        let decoded = match load_and_resample_sfx(&resolved_str, output) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to prepare SFX '{path}': {e}");
                return None;
            }
        };

        debug!("Cached SFX: {path}");
        let sound = SfxId(decoded);
        self.sounds.insert(path.to_owned(), sound.clone());
        Some(sound)
    }

    /// Enqueues prepared samples on `bus` without loading or looking up a path.
    pub fn play(&mut self, sound: &SfxId, bus: MixBus, target_stream_frame: u64) {
        let _ = self.sender.try_send(QueuedSfx {
            data: Arc::clone(&sound.0),
            bus,
            generation: self.controls.bus_generation(bus),
            target_stream_frame,
        });
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
        let prepared = cache.prepare("cue", output, |_| path.clone());
        std::fs::remove_file(&path).unwrap();
        let sound = prepared.expect("valid PCM fixture must prepare");
        assert!(
            receiver.try_iter().next().is_none(),
            "preparation must not play"
        );
        let same = cache
            .prepare("cue", output, |_| panic!("prepared cue resolved twice"))
            .expect("cached sound remains available without its source");

        let effect = MixBus::new(0);
        let screen = MixBus::new(1);
        cache.play(&sound, screen, 480);
        cache.play(&same, effect, 0);
        controls.stop_bus(screen);
        cache.play(&sound, screen, 960);
        let queued: Vec<_> = receiver.try_iter().collect();
        assert_eq!(queued.len(), 3);
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
        assert!(cache.prepare("missing", output, |_| path.clone()).is_none());
        assert!(receiver.try_iter().next().is_none());
    }
}
