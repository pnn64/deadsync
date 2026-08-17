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
/// Cold UI misses may decode synchronously, while gameplay retains `SfxId`
/// values and submits them without locking, hashing, allocation, or I/O.
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

    pub fn play(
        &mut self,
        path: &str,
        bus: MixBus,
        output: OutputFormat,
        resolve_asset_path: impl FnOnce(&str) -> PathBuf,
    ) {
        if let Some(sound) = self.sounds.get(path).cloned() {
            self.play_resolved(&sound, bus, 0);
            return;
        }

        let resolved = resolve_asset_path(path);
        let resolved_str = resolved.to_string_lossy();
        let decoded = match load_and_resample_sfx(&resolved_str, output) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to load SFX '{path}': {e}");
                return;
            }
        };

        let sound = self.sounds.entry(path.to_owned()).or_insert_with(|| {
            debug!("Cached SFX: {path}");
            SfxId(decoded)
        });
        let sound = sound.clone();
        self.play_resolved(&sound, bus, 0);
    }

    pub fn preload(
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
                warn!("Failed to preload SFX '{path}': {e}");
                return None;
            }
        };

        debug!("Cached SFX: {path}");
        let sound = SfxId(decoded);
        self.sounds.insert(path.to_owned(), sound.clone());
        Some(sound)
    }

    pub fn play_resolved(&mut self, sound: &SfxId, bus: MixBus, target_stream_frame: u64) {
        let _ = self.sender.try_send(QueuedSfx {
            data: Arc::clone(&sound.0),
            bus,
            generation: self.controls.bus_generation(bus),
            target_stream_frame,
        });
    }

    pub fn play_assist_tick(&mut self, sound: &SfxId, target_stream_frame: u64) {
        self.play_resolved(sound, ASSIST_TICK_BUS, target_stream_frame);
    }
}
