use crate::mix::ASSIST_TICK_BUS;
use crate::{OutputFormat, load_and_resample_sfx};
use deadlib_audio::{MixBus, MixControls, QueuedSfx, SfxSender};
use log::{debug, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

const ASSIST_TICK_SFX_PATH: &str = "assets/sounds/assist_tick.ogg";

/// Session-lifetime SFX cache owned by the game thread facade.
///
/// Thread model: callers use it from game/screen/app threads behind a mutex;
/// the audio callback never touches this cache. Capacity is intentionally
/// grow-only for the currently loaded session and warmed at screen/song
/// transition points via `preload`; cache misses decode from disk synchronously
/// only for non-preloaded UI sounds. Gameplay-critical callers use
/// `play_preloaded*`, which skips miss insertion and logs instead.
pub struct SfxCache {
    sounds: Mutex<HashMap<String, Arc<[i16]>>>,
    assist_tick: OnceLock<Arc<[i16]>>,
    sender: Mutex<SfxSender>,
    controls: Arc<MixControls>,
}

impl SfxCache {
    pub fn new(controls: Arc<MixControls>, sender: SfxSender) -> Self {
        Self {
            sounds: Mutex::new(HashMap::new()),
            assist_tick: OnceLock::new(),
            sender: Mutex::new(sender),
            controls,
        }
    }

    pub fn play(
        &self,
        path: &str,
        bus: MixBus,
        output: OutputFormat,
        resolve_asset_path: impl FnOnce(&str) -> PathBuf,
    ) {
        if self.play_cached(path, bus, 0) {
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

        let sound_data = {
            let mut cache = self.sounds.lock().unwrap();
            cache
                .entry(path.to_string())
                .or_insert_with(|| {
                    debug!("Cached SFX: {path}");
                    decoded
                })
                .clone()
        };
        self.cache_assist_tick(path, sound_data.clone());
        self.send(sound_data, bus, 0);
    }

    pub fn play_preloaded(&self, path: &str, bus: MixBus) {
        if !self.play_cached(path, bus, 0) {
            warn!("Preloaded SFX cache miss for '{path}'; skipping synchronous decode");
        }
    }

    pub fn play_assist_tick(
        &self,
        path: &str,
        output: OutputFormat,
        resolve_asset_path: impl FnOnce(&str) -> PathBuf,
    ) {
        if path == ASSIST_TICK_SFX_PATH
            && let Some(sound_data) = self.assist_tick.get().cloned()
        {
            self.send(sound_data, ASSIST_TICK_BUS, 0);
            return;
        }
        self.play(path, ASSIST_TICK_BUS, output, resolve_asset_path);
    }

    pub fn play_preloaded_assist_tick(&self, path: &str) {
        if path == ASSIST_TICK_SFX_PATH
            && let Some(sound_data) = self.assist_tick.get().cloned()
        {
            self.send(sound_data, ASSIST_TICK_BUS, 0);
            return;
        }
        self.play_preloaded(path, ASSIST_TICK_BUS);
    }

    pub fn play_scheduled_assist_tick(&self, path: &str, target_stream_frame: u64) {
        if target_stream_frame == 0 {
            self.play_preloaded_assist_tick(path);
            return;
        }
        if path == ASSIST_TICK_SFX_PATH
            && let Some(sound_data) = self.assist_tick.get().cloned()
        {
            self.send(sound_data, ASSIST_TICK_BUS, target_stream_frame);
            return;
        }

        let cached = { self.sounds.lock().unwrap().get(path).cloned() };
        if let Some(sound_data) = cached {
            self.send(sound_data, ASSIST_TICK_BUS, target_stream_frame);
        } else {
            warn!("Scheduled assist tick cache miss for '{path}'; skipping");
        }
    }

    pub fn preload(
        &self,
        path: &str,
        output: OutputFormat,
        resolve_asset_path: impl FnOnce(&str) -> PathBuf,
    ) {
        let cached = { self.sounds.lock().unwrap().get(path).cloned() };
        if let Some(data) = cached {
            self.cache_assist_tick(path, data);
            return;
        }

        let resolved = resolve_asset_path(path);
        let resolved_str = resolved.to_string_lossy();
        let decoded = match load_and_resample_sfx(&resolved_str, output) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to preload SFX '{path}': {e}");
                return;
            }
        };

        let mut cache = self.sounds.lock().unwrap();
        let data = cache
            .entry(path.to_string())
            .or_insert_with(|| {
                debug!("Cached SFX: {path}");
                decoded
            })
            .clone();
        self.cache_assist_tick(path, data);
    }

    fn play_cached(&self, path: &str, bus: MixBus, target_stream_frame: u64) -> bool {
        let cached = { self.sounds.lock().unwrap().get(path).cloned() };
        if let Some(sound_data) = cached {
            self.send(sound_data, bus, target_stream_frame);
            return true;
        }
        false
    }

    fn cache_assist_tick(&self, path: &str, data: Arc<[i16]>) {
        if path == ASSIST_TICK_SFX_PATH {
            let _ = self.assist_tick.set(data);
        }
    }

    fn send(&self, data: Arc<[i16]>, bus: MixBus, target_stream_frame: u64) {
        let mut sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = sender.try_send(QueuedSfx {
            data,
            bus,
            generation: self.controls.bus_generation(bus),
            target_stream_frame,
        });
    }
}
