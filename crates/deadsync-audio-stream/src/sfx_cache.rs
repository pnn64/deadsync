use crate::{OutputFormat, load_and_resample_sfx};
use deadsync_audio::{QueuedSfx, SfxLane, sfx_stop_generation};
use log::{debug, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
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
}

impl SfxCache {
    pub fn new() -> Self {
        Self {
            sounds: Mutex::new(HashMap::new()),
            assist_tick: OnceLock::new(),
        }
    }

    pub fn play(
        &self,
        path: &str,
        lane: SfxLane,
        output: OutputFormat,
        sender: &SyncSender<QueuedSfx>,
        resolve_asset_path: impl FnOnce(&str) -> PathBuf,
    ) {
        if self.play_cached(path, lane, 0, sender) {
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
        send_sfx(sender, sound_data, lane, 0);
    }

    pub fn play_preloaded(&self, path: &str, lane: SfxLane, sender: &SyncSender<QueuedSfx>) {
        if !self.play_cached(path, lane, 0, sender) {
            warn!("Preloaded SFX cache miss for '{path}'; skipping synchronous decode");
        }
    }

    pub fn play_assist_tick(
        &self,
        path: &str,
        output: OutputFormat,
        sender: &SyncSender<QueuedSfx>,
        resolve_asset_path: impl FnOnce(&str) -> PathBuf,
    ) {
        if path == ASSIST_TICK_SFX_PATH
            && let Some(sound_data) = self.assist_tick.get().cloned()
        {
            send_sfx(sender, sound_data, SfxLane::AssistTick, 0);
            return;
        }
        self.play(
            path,
            SfxLane::AssistTick,
            output,
            sender,
            resolve_asset_path,
        );
    }

    pub fn play_preloaded_assist_tick(&self, path: &str, sender: &SyncSender<QueuedSfx>) {
        if path == ASSIST_TICK_SFX_PATH
            && let Some(sound_data) = self.assist_tick.get().cloned()
        {
            send_sfx(sender, sound_data, SfxLane::AssistTick, 0);
            return;
        }
        self.play_preloaded(path, SfxLane::AssistTick, sender);
    }

    pub fn play_scheduled_assist_tick(
        &self,
        path: &str,
        target_stream_frame: u64,
        sender: &SyncSender<QueuedSfx>,
    ) {
        if target_stream_frame == 0 {
            self.play_preloaded_assist_tick(path, sender);
            return;
        }
        if path == ASSIST_TICK_SFX_PATH
            && let Some(sound_data) = self.assist_tick.get().cloned()
        {
            send_sfx(sender, sound_data, SfxLane::AssistTick, target_stream_frame);
            return;
        }

        let cached = { self.sounds.lock().unwrap().get(path).cloned() };
        if let Some(sound_data) = cached {
            send_sfx(sender, sound_data, SfxLane::AssistTick, target_stream_frame);
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

    fn play_cached(
        &self,
        path: &str,
        lane: SfxLane,
        target_stream_frame: u64,
        sender: &SyncSender<QueuedSfx>,
    ) -> bool {
        let cached = { self.sounds.lock().unwrap().get(path).cloned() };
        if let Some(sound_data) = cached {
            send_sfx(sender, sound_data, lane, target_stream_frame);
            return true;
        }
        false
    }

    fn cache_assist_tick(&self, path: &str, data: Arc<[i16]>) {
        if path == ASSIST_TICK_SFX_PATH {
            let _ = self.assist_tick.set(data);
        }
    }
}

impl Default for SfxCache {
    fn default() -> Self {
        Self::new()
    }
}

fn send_sfx(
    sender: &SyncSender<QueuedSfx>,
    data: Arc<[i16]>,
    lane: SfxLane,
    target_stream_frame: u64,
) {
    let _ = sender.try_send(QueuedSfx {
        data,
        lane,
        stop_generation: sfx_stop_generation(lane),
        target_stream_frame,
    });
}
