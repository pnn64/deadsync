use crate::dynamic_media::banner_cache_options;
use crate::graphics::RenderAssets;
use deadlib_render::Backend;
use deadlib_render_core::SamplerDesc;
use deadsync_assets::media_cache;
use deadsync_assets::noteskin::Noteskin;
use deadsync_assets::song_lua::{SongLuaOverlayActor, SongLuaOverlayKind};
use deadsync_chart::{SongBackgroundChange, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::SongLuaRuntimeVisuals;
use hashbrown::HashSet as FastHashSet;
use log::warn;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[inline]
pub(super) fn insert_texture_key(seen: &mut FastHashSet<String>, key: &str) -> bool {
    let previous_len = seen.len();
    seen.get_or_insert_with(key, str::to_owned);
    seen.len() != previous_len
}

fn prewarm_model_texture_key(
    assets: &mut RenderAssets,
    backend: &mut Backend,
    seen: &mut FastHashSet<String>,
    seen_model_textures: &mut FastHashSet<String>,
    key: &str,
) {
    let key = deadsync_assets::textures::canonical_texture_key(key);
    if !seen_model_textures.insert(key.clone()) {
        return;
    }
    deadsync_assets::textures::ensure_texture_for_key_with_sampler(
        assets,
        backend,
        &key,
        deadsync_assets::textures::model_texture_sampler(&key),
    );
    seen.insert(key);
}

fn prewarm_noteskin_textures(
    assets: &mut RenderAssets,
    backend: &mut Backend,
    seen: &mut FastHashSet<String>,
    seen_model_textures: &mut FastHashSet<String>,
    noteskin: &Noteskin,
) {
    noteskin.for_each_slot(|slot| {
        for texture_slot in std::iter::once(slot).chain(slot.model_additive.as_deref()) {
            let key = texture_slot.texture_key();
            if slot.model.is_some() {
                for key in std::iter::once(key)
                    .chain(texture_slot.model_texture_keys.iter().map(AsRef::as_ref))
                {
                    prewarm_model_texture_key(assets, backend, seen, seen_model_textures, key);
                }
            } else if insert_texture_key(seen, key) {
                deadsync_assets::textures::ensure_native_texture_for_key(
                    assets,
                    backend,
                    key,
                );
            }
        }
    });
}

struct LiveImageBinding {
    key: Arc<str>,
    sampler: SamplerDesc,
    failed: bool,
}

/// Published live image bindings, owned by one Gameplay/Practice attempt.
/// Stable slots only compare retained keys/samplers. New bindings load before
/// composition; failed bindings wait for a source change or an explicit reset.
#[derive(Default)]
pub struct GameplayLiveTextures {
    bindings: Vec<Option<LiveImageBinding>>,
    texture_revision: u64,
}

impl GameplayLiveTextures {
    pub fn reset(&mut self) {
        self.bindings.clear();
        self.texture_revision = 0;
    }

    pub fn sync<'a>(
        &mut self,
        assets: &mut RenderAssets,
        backend: &mut Backend,
        overlays: impl IntoIterator<Item = &'a SongLuaOverlayActor>,
    ) {
        let revision = assets.texture_context().revision();
        self.visit_changed(overlays, revision, |path, key, sampler, reload| {
            if !reload && assets.has_uploaded_texture_key(key) {
                return true;
            }
            let rgba = match media_cache::load_banner_source_rgba(path, banner_cache_options()) {
                Ok(rgba) => rgba,
                Err(error) => {
                    warn!("Failed to load live song Lua image {path:?}: {error}. Skipping.");
                    return false;
                }
            };
            match assets.update_texture_for_key_with_sampler(backend, key, &rgba, sampler) {
                Ok(()) => true,
                Err(error) => {
                    warn!("Failed to upload live song Lua image {path:?}: {error}. Skipping.");
                    false
                }
            }
        });
        // A successful upload invalidates retained Sprite bindings, including a
        // previous miss. Remember the final revision after this resource batch.
        self.texture_revision = assets.texture_context().revision();
    }

    fn visit_changed<'a>(
        &mut self,
        overlays: impl IntoIterator<Item = &'a SongLuaOverlayActor>,
        revision: u64,
        mut load: impl FnMut(&Path, &str, SamplerDesc, bool) -> bool,
    ) {
        let registry_changed = revision != self.texture_revision;
        let mut count = 0;
        for overlay in overlays {
            if count == self.bindings.len() {
                self.bindings.push(None);
            }
            let previous = &mut self.bindings[count];
            count += 1;
            let resource = match &overlay.kind {
                SongLuaOverlayKind::Sprite {
                    texture_path,
                    texture_key,
                    ..
                }
                | SongLuaOverlayKind::ActorMultiVertex {
                    texture_path: Some(texture_path),
                    texture_key: Some(texture_key),
                    ..
                } if !deadlib_assets::dynamic::is_dynamic_video_path(texture_path) => {
                    Some((texture_path, texture_key))
                }
                _ => None,
            };
            let Some((path, key)) = resource else {
                *previous = None;
                continue;
            };
            let sampler = deadsync_assets::song_lua::overlay_sampler(overlay);
            let same_key = previous
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(&old.key, key) || old.key.as_ref() == key.as_ref());
            let same_binding =
                same_key && previous.as_ref().is_some_and(|old| old.sampler == sampler);
            if same_binding
                && (!registry_changed || previous.as_ref().is_some_and(|old| old.failed))
            {
                continue;
            }
            let reload = same_key && !same_binding;
            let failed = !load(path, key, sampler, reload);
            if same_binding {
                previous.as_mut().unwrap().failed = failed;
            } else {
                *previous = Some(LiveImageBinding {
                    key: Arc::clone(key),
                    sampler,
                    failed,
                });
            }
        }
        self.bindings.truncate(count);
        self.texture_revision = revision;
    }
}

pub fn prewarm_gameplay_assets<StateDelta>(
    assets: &mut RenderAssets,
    backend: &mut Backend,
    noteskin_sets: [&[Option<Arc<Noteskin>>; MAX_PLAYERS]; 4],
    song: &SongData,
    background_changes: &[SongBackgroundChange],
    song_lua_visuals: &SongLuaRuntimeVisuals<
        SongLuaOverlayActor,
        deadsync_song_lua::SongLuaCapturedActor,
        StateDelta,
    >,
) {
    let mut seen = FastHashSet::<String>::with_capacity(256);
    let mut seen_model_textures = FastHashSet::<String>::with_capacity(64);
    let mut seen_song_lua_fonts = FastHashSet::<&'static str>::default();
    for noteskin in noteskin_sets
        .into_iter()
        .flat_map(|set| set.iter().flatten())
    {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }

    for path in deadsync_assets::dynamic_media::gameplay_media_paths(song, background_changes) {
        let key = path.to_string_lossy();
        if insert_texture_key(&mut seen, key.as_ref()) {
            media_cache::ensure_banner_texture(assets, backend, path, banner_cache_options);
        }
    }

    let mut prewarm_song_lua_overlays = |overlays: &[SongLuaOverlayActor]| {
        for overlay in overlays {
            match &overlay.kind {
                SongLuaOverlayKind::BitmapText {
                    font_name,
                    font_path,
                    ..
                } => {
                    if seen_song_lua_fonts.insert(*font_name)
                        && assets.with_font(font_name, |_| ()).is_none()
                        && let Err(err) = deadsync_assets::fonts::load_font_from_ini_path(
                            assets, backend, font_name, font_path,
                        )
                    {
                        warn!(
                            "Failed to load song lua bitmap font '{}': {}",
                            font_path.display(),
                            err
                        );
                    }
                }
                SongLuaOverlayKind::Sprite {
                    texture_path,
                    texture_key,
                    ..
                }
                | SongLuaOverlayKind::ActorMultiVertex {
                    texture_path: Some(texture_path),
                    texture_key: Some(texture_key),
                    ..
                } => {
                    let changes = match &overlay.kind {
                        SongLuaOverlayKind::Sprite { textures, .. } => textures.as_ref(),
                        _ => &[],
                    };
                    for (texture_path, texture_key) in std::iter::once((texture_path, texture_key))
                        .chain(changes.iter().map(|texture| (&texture.path, &texture.key)))
                    {
                        let key = texture_key.as_ref();
                        let first_seen = insert_texture_key(&mut seen, key);
                        let sampler = deadsync_assets::song_lua::overlay_sampler(overlay);
                        if sampler != SamplerDesc::default() {
                            match media_cache::load_banner_source_rgba(texture_path, banner_cache_options()) {
                                Ok(rgba) => {
                                    if let Err(error) = assets.update_texture_for_key_with_sampler(
                                        backend, key, &rgba, sampler,
                                    ) {
                                        warn!(
                                            "Failed to create custom-sampled GPU texture for image {texture_path:?}: {error}. Skipping."
                                        );
                                    }
                                }
                                Err(error) => {
                                    warn!(
                                        "Failed to load song lua texture source {texture_path:?}: {error}. Skipping."
                                    );
                                }
                            }
                        } else if first_seen {
                            media_cache::ensure_banner_texture(assets, backend, texture_path, banner_cache_options);
                        }
                    }
                }
                SongLuaOverlayKind::Model { layers } => {
                    for layer in layers.iter() {
                        for key in std::iter::once(&layer.texture_key)
                            .chain(layer.additive.as_ref())
                            .chain(
                                layer
                                    .texture_frames
                                    .iter()
                                    .chain(layer.additive_frames.iter())
                                    .filter_map(|frame| frame.texture_key.as_ref()),
                            )
                        {
                            prewarm_model_texture_key(
                                assets,
                                backend,
                                &mut seen,
                                &mut seen_model_textures,
                                key.as_ref(),
                            );
                        }
                    }
                }
                SongLuaOverlayKind::NoteskinActor { slots, .. } => {
                    for slot in slots.iter() {
                        if slot.model.is_some() {
                            for texture in
                                std::iter::once(slot).chain(slot.model_additive.as_deref())
                            {
                                for key in std::iter::once(texture.texture_key())
                                    .chain(texture.model_texture_keys.iter().map(AsRef::as_ref))
                                {
                                    prewarm_model_texture_key(
                                        assets,
                                        backend,
                                        &mut seen,
                                        &mut seen_model_textures,
                                        key,
                                    );
                                }
                            }
                        } else if insert_texture_key(&mut seen, slot.texture_key()) {
                            deadsync_assets::textures::ensure_native_texture_for_key(
                                assets,
                                backend,
                                slot.texture_key(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    };
    prewarm_song_lua_overlays(&song_lua_visuals.overlays);
    for layer in &song_lua_visuals.background_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
    for player in &song_lua_visuals.player_actors {
        for texture in &player.judgment.textures {
            if insert_texture_key(&mut seen, &texture.key) {
                media_cache::ensure_banner_texture(assets, backend, &texture.path, banner_cache_options);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_actor(path: &str) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: PathBuf::from(path),
                texture_key: Arc::from(path),
                states: Arc::from([]),
                textures: Arc::from([]),
            },
            name: None,
            parent_index: None,
            initial_state: Default::default(),
            message_commands: Vec::new(),
        }
    }

    #[test]
    fn live_song_images_detect_delayed_load_replacement_and_sampler_changes() {
        let mut cache = GameplayLiveTextures::default();
        let mut actor = image_actor("first.png");
        actor.kind = SongLuaOverlayKind::Actor;
        cache.visit_changed([&actor], 1, |_, _, _, _| {
            panic!("unloaded Sprite has no image")
        });
        actor = image_actor("first.png");
        let mut loaded = Vec::new();
        cache.visit_changed([&actor], 1, |_, key, _, reload| {
            loaded.push((key.to_owned(), reload));
            true
        });
        // Publication may retain or reconstruct the Arc; unchanged keys do no work.
        for _ in 0..3 {
            actor = image_actor("first.png");
            cache.visit_changed([&actor], 1, |_, _, _, _| panic!("unchanged image reloaded"));
        }
        actor = image_actor("second.png");
        cache.visit_changed([&actor], 1, |_, key, _, reload| {
            loaded.push((key.to_owned(), reload));
            true
        });
        actor.initial_state.texture_wrapping = true;
        cache.visit_changed([&actor], 1, |_, key, _, reload| {
            loaded.push((key.to_owned(), reload));
            true
        });
        cache.visit_changed([&actor], 1, |_, _, _, _| {
            panic!("unchanged sampler reloaded")
        });
        assert_eq!(
            loaded,
            [
                ("first.png".into(), false),
                ("second.png".into(), false),
                ("second.png".into(), true)
            ]
        );
        cache.visit_changed([&actor], 2, |_, key, _, reload| {
            assert_eq!(key, "second.png");
            assert!(!reload);
            true
        });
        cache.visit_changed([&actor], 2, |_, _, _, _| panic!("stable registry reloaded"));
    }

    #[test]
    fn live_song_images_do_not_retry_failed_paths_until_binding_or_state_changes() {
        let mut cache = GameplayLiveTextures::default();
        let mut actor = image_actor("missing.png");
        cache.visit_changed([&actor], 1, |_, _, _, _| false);
        for revision in [1, 2, 3] {
            cache.visit_changed([&actor], revision, |_, _, _, _| {
                panic!("failed image retried")
            });
        }
        cache.reset();
        let mut retries = 0;
        cache.visit_changed([&actor], 3, |_, _, _, _| {
            retries += 1;
            false
        });
        actor = image_actor("replacement.png");
        cache.visit_changed([&actor], 3, |_, _, _, _| {
            retries += 1;
            true
        });
        assert_eq!(retries, 2);
    }

    #[test]
    fn texture_key_insert_reports_only_first_owner() {
        let mut seen = FastHashSet::new();
        assert!(insert_texture_key(&mut seen, "noteskin/receptor.png"));
        assert!(!insert_texture_key(&mut seen, "noteskin/receptor.png"));
        assert!(insert_texture_key(&mut seen, "noteskin/tap.png"));
        assert_eq!(seen.len(), 2);
        assert!(seen.contains("noteskin/receptor.png"));
        assert!(seen.contains("noteskin/tap.png"));
    }
}

const BOOM_SFX_PATH: &str = "assets/sounds/boom.ogg";

/// Song-lifetime resolved sounds retained by the application thread.
///
/// Fixed gameplay sounds take direct fields. Arbitrary `SongLua` paths use the
/// prewarmed map because their domain is chart-authored and not statically
/// bounded. Submission borrows an `SfxId`; no decode, allocation, lock, or
/// audio-cache lookup occurs on gameplay frames.
#[derive(Default)]
pub struct GameplaySfx {
    boom: Option<deadsync_audio_stream::SfxId>,
    assist_tick: Option<deadsync_audio_stream::SfxId>,
    beat_tick: Option<deadsync_audio_stream::SfxId>,
    measure_tick: Option<deadsync_audio_stream::SfxId>,
    song_lua: HashMap<String, deadsync_audio_stream::SfxId>,
}

impl GameplaySfx {
    pub fn resolve(&self, path: &str) -> Option<&deadsync_audio_stream::SfxId> {
        match path {
            BOOM_SFX_PATH => self.boom.as_ref(),
            deadsync_gameplay::ASSIST_TICK_SFX_PATH => self.assist_tick.as_ref(),
            deadsync_gameplay::BEAT_TICK_SFX_PATH => self.beat_tick.as_ref(),
            deadsync_gameplay::MEASURE_TICK_SFX_PATH => self.measure_tick.as_ref(),
            _ => self.song_lua.get(path),
        }
    }

    pub fn resolve_path(&self, path: &Path) -> Option<&deadsync_audio_stream::SfxId> {
        self.resolve(path.to_string_lossy().as_ref())
    }
}

pub fn prewarm_gameplay_sfx<CapturedActor, StateDelta>(
    audio: &mut deadsync_audio_stream::AudioControl,
    song_lua_visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor, CapturedActor, StateDelta>,
    song_lua_sound_paths: &[PathBuf],
) -> GameplaySfx {
    let boom = audio.prepare_sfx(&deadsync_assets::resolve_asset_path(BOOM_SFX_PATH));
    let assist_tick = audio.prepare_sfx(&deadsync_assets::resolve_asset_path(
        deadsync_gameplay::ASSIST_TICK_SFX_PATH,
    ));
    let beat_tick = audio.prepare_sfx(&deadsync_assets::resolve_asset_path(
        deadsync_gameplay::BEAT_TICK_SFX_PATH,
    ));
    let measure_tick = audio.prepare_sfx(&deadsync_assets::resolve_asset_path(
        deadsync_gameplay::MEASURE_TICK_SFX_PATH,
    ));

    let mut sound_paths = Vec::<PathBuf>::with_capacity(song_lua_sound_paths.len());
    let mut seen = HashSet::<String>::with_capacity(song_lua_sound_paths.len());
    let mut prewarm_sound_overlays = |overlays: &[SongLuaOverlayActor]| {
        deadsync_song_lua::push_song_lua_overlay_sound_paths(overlays, &mut seen, &mut sound_paths);
    };
    prewarm_sound_overlays(&song_lua_visuals.overlays);
    for layer in &song_lua_visuals.background_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    deadsync_song_lua::push_unique_song_lua_sound_paths(
        song_lua_sound_paths,
        &mut seen,
        &mut sound_paths,
    );
    let mut song_lua = HashMap::with_capacity(sound_paths.len());
    for sound_path in sound_paths {
        let key = sound_path.to_string_lossy();
        if let Some(sound) = audio.prepare_sfx(&deadsync_assets::resolve_asset_path(key.as_ref())) {
            // Native compiled resources use OS path components while authored
            // Lua resources use forward slashes. Prepare both lookup spellings
            // here so gameplay needs no normalization or allocation.
            let native_key = sound_path.components().collect::<PathBuf>()
                .to_string_lossy().into_owned();
            if native_key != key {
                song_lua.insert(native_key, sound.clone());
            }
            song_lua.insert(key.into_owned(), sound);
        }
    }
    GameplaySfx {
        boom,
        assist_tick,
        beat_tick,
        measure_tick,
        song_lua,
    }
}
